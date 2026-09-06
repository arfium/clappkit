//! Transport: a local control socket, a unix domain socket on Linux/macOS, a named
//! pipe on Windows. The owner binds the address and accepts connections; the dialer
//! connects. The socket directory is created `0700`. Everything above this module
//! is platform-agnostic and works over any `AsyncRead + AsyncWrite` the listener or
//! connector yields.

use std::io;

#[cfg(unix)]
pub use unix::{connect, Listener};

#[cfg(windows)]
pub use windows::{connect, Listener};

#[cfg(unix)]
mod unix {
    use super::io;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use tokio::net::{UnixListener, UnixStream};

    /// A bound listener. The owner binds an address and accepts the peers that
    /// connect back.
    pub struct Listener {
        inner: UnixListener,
        addr: String,
    }

    impl Listener {
        /// Bind at `addr`, creating the parent dir `0700`. Fails with `AddrInUse`
        /// if the address is already taken; the caller decides whether that is a
        /// live peer or a stale file to reclaim. (Per-instance app sockets use a
        /// fresh random path, so they never collide.)
        pub fn bind(addr: &str) -> io::Result<Self> {
            if let Some(dir) = Path::new(addr).parent() {
                std::fs::create_dir_all(dir)?;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
                guard_dir(dir)?; // refuse a symlinked or group/world-writable dir we were pointed at
            }
            let inner = UnixListener::bind(addr)?;
            Ok(Self {
                inner,
                addr: addr.into(),
            })
        }

        pub fn addr(&self) -> &str {
            &self.addr
        }

        /// Accept the next connection.
        pub async fn accept(&self) -> io::Result<UnixStream> {
            let (stream, _) = self.inner.accept().await?;
            Ok(stream)
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.addr);
        }
    }

    /// Connect to a listener at `addr`. The dialer USED to check nothing: in the no-home
    /// fallback the socket lives under a world-writable `/tmp`, where an attacker could plant
    /// a directory and a socket at the predictable path and receive the control traffic — the
    /// instance token included — meant for the real app. So refuse a socket whose directory
    /// anyone else can write to, or that is reached through a symlink.
    pub async fn connect(addr: &str) -> io::Result<UnixStream> {
        if let Some(dir) = Path::new(addr).parent() {
            guard_dir(dir)?;
        }
        UnixStream::connect(addr).await
    }

    /// The socket's directory must be one only its owner can write to, reached directly and
    /// not through a symlink someone else controls. This does NOT prove who owns it — that
    /// needs a `geteuid` this crate does not link — so an attacker's OWN private directory at
    /// the predictable path is not caught here; what it removes is the group/world-writable
    /// plant, the realistic local hijack of a predictable socket path.
    fn guard_dir(dir: &Path) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(dir)?;
        if meta.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{}: control-socket directory is a symlink", dir.display()),
            ));
        }
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{}: control-socket directory is group- or world-writable", dir.display()),
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::io;
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    /// All server instances busy: the accept-window (B6). Retried. A missing
    /// name (`ERROR_FILE_NOT_FOUND`) is NOT retried: while a server lives, its
    /// name always has at least one instance (the connected one counts), so an
    /// absent name means no server is there, and stalling on that would tax every
    /// cold start with a five-second wait for something that does not exist. A
    /// boot race — dialling while the other end is still binding — belongs to the
    /// caller, which knows its own deadline.
    const ERROR_PIPE_BUSY: i32 = 231;

    /// A per-instance named-pipe listener. Compile-checked on Windows only; the
    /// platform-agnostic layers above are exercised on unix.
    pub struct Listener {
        addr: String,
        first: Mutex<Option<NamedPipeServer>>,
    }

    impl Listener {
        pub fn bind(addr: &str) -> io::Result<Self> {
            // Reserve the FIRST instance: `create` fails if the name already exists, so a
            // process that squatted our pipe name cannot quietly stand up a SECOND instance
            // and have the OS hand it dials meant for us. The control pipe carries the
            // instance token, so a shared name is a hijack, not a hiccup. This also matches
            // the unix bind, which fails when the address is taken and leaves reclaiming a
            // stale name to the caller (a lock's job, not the transport's). The cost —
            // weighed and accepted — is that a rebind while a dead server's clients still
            // hold instances fails until they drop; the caller retries, and failing toward
            // "not shared with a stranger" is the safe direction.
            let first = ServerOptions::new().first_pipe_instance(true).create(addr)?;
            Ok(Self {
                addr: addr.into(),
                first: Mutex::new(Some(first)),
            })
        }

        pub fn addr(&self) -> &str {
            &self.addr
        }

        pub async fn accept(&self) -> io::Result<NamedPipeServer> {
            // Use the reserved first instance, then make a fresh one per accept;
            // the guard is dropped before the await.
            let server = match self.first.lock().unwrap().take() {
                Some(s) => s,
                None => ServerOptions::new().create(&self.addr)?,
            };
            server.connect().await?;
            Ok(server)
        }
    }

    /// Dial the pipe, retrying the accept-window (B6): between the server's
    /// accepts every instance can momentarily be busy (`ERROR_PIPE_BUSY`), and a
    /// single `open` would then fail a perfectly valid dial. Bounded (~5s) so a
    /// name whose server died with clients still attached (busy corpse
    /// instances, nobody listening) errors instead of spinning forever. An
    /// absent name errors immediately: no server, the caller's business.
    ///
    /// Windows-only: this module is not compiled or run on the unix dev host, so it
    /// is verified on a Windows build, not by the macOS gate.
    pub async fn connect(addr: &str) -> io::Result<NamedPipeClient> {
        for _ in 0..250 {
            match ClientOptions::new().open(addr) {
                Ok(client) => return Ok(client),
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(e) => return Err(e),
            }
        }
        // Bound exhausted: one last try to surface the real, persistent error.
        ClientOptions::new().open(addr)
    }
}
