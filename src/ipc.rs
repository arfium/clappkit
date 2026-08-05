//! The app's own GUI↔CLI channel: a private, local request/response IPC that Clatch
//! never sees (reference/app-developer.md § the boundary — "you own GUI, CLI, state,
//! and their sync"). It reuses `clatch-ipc`'s transport, so it is a **unix-domain
//! socket** on macOS/Linux and a **named pipe** on Windows with no app-level `cfg` —
//! the one place the platform difference lives is [`address`].
//!
//! The wire is one length-prefixed JSON request → one JSON response per connection
//! (`clatch-ipc`'s `frame`, 1 MiB fail-fast bound). The GUI process runs [`serve`]; the
//! agent's CLI role runs [`request`].

use anyhow::Result;
use clatch_ipc::{connect, frame, FrameLimits, Listener};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

const LIMITS: FrameLimits = FrameLimits::control(); // 1 MiB, fail-fast — ample for app IPC

/// How long a connected client has to send its request before we drop it. A CLI writes
/// its frame immediately; anything that connects and then says nothing is a stuck or
/// hostile peer, and without a deadline it would hold a task and a file descriptor for
/// the life of the app.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Consecutive accept failures before [`serve`] gives up and returns the error. One bad
/// accept is transient; hundreds in a row mean the listener is broken and the app's whole
/// CLI surface is down — which the caller must be able to print, rather than the loop
/// spinning silently forever.
const MAX_ACCEPT_FAILURES: u32 = 32;

/// The per-app IPC address, keyed by the CLI shorthand so two clapps never collide:
/// `\\.\pipe\clapp-<cli>-<user>` on Windows, `~/.<cli>/<cli>.sock` on unix.
///
/// **Not [`crate::paths::data_dir`]**, and it must stay that way: the CLI role is a
/// separate process Clatch never launched, so it has no `CLATCH_DATA_DIR`. Both roles must
/// derive the same address from the home alone, or every CLI call misses the running GUI.
///
/// The two arms differ in privacy. Unix gets a socket in a `0700` directory — genuinely
/// owner-only. A Windows pipe name is machine-wide with a default DACL; the `-<user>`
/// suffix stops collisions between logged-in users, it does not make the channel private.
/// Closing that needs security attributes in `clatch-ipc`'s transport, where it belongs.
pub fn address(cli: &str) -> String {
    #[cfg(windows)]
    {
        pipe_name(cli, &user_scope())
    }
    #[cfg(unix)]
    {
        let home = crate::paths::home().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
        socket_path(&home, cli).to_string_lossy().into_owned()
    }
}

/// `<home>/.<cli>/<cli>.sock`, built with `join` so no separator is ever hand-written.
#[cfg_attr(windows, allow(dead_code))]
fn socket_path(home: &std::path::Path, cli: &str) -> std::path::PathBuf {
    home.join(format!(".{cli}")).join(format!("{cli}.sock"))
}

/// `\\.\pipe\clapp-<cli>-<scope>`. Split out from the `cfg` so the naming rule is
/// testable on any host.
#[cfg_attr(unix, allow(dead_code))]
fn pipe_name(cli: &str, scope: &str) -> String {
    format!(r"\\.\pipe\clapp-{cli}-{scope}")
}

/// A short, stable, non-identifying tag for the current Windows user, so the pipe name is
/// per-user instead of machine-global. Both roles inherit the same environment from the
/// same login session, so both derive the same tag. Hashed rather than embedded verbatim
/// because a pipe name is world-readable and a username is not ours to publish.
#[cfg_attr(unix, allow(dead_code))]
fn user_scope() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    if user.is_empty() && domain.is_empty() {
        return "shared".into();
    }
    // FNV-1a, 64-bit: no dependency, and a name collision is merely a shared pipe, so
    // cryptographic strength buys nothing here.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in format!("{domain}\\{user}").bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

/// Serve the app side: accept connections and answer each one's JSON request with the
/// value the **async** `handler` resolves to. Loops until the process exits, or until the
/// listener has failed [`MAX_ACCEPT_FAILURES`] times in a row — which it returns, so the
/// caller can print it. Run this on the GUI process; the agent's CLI role calls
/// [`request`]. `handler` is shared across connections (`Arc`), so it captures the app's
/// state (e.g. a `tokio::sync::Mutex<…>`) and may `.await` — mutate state, call an API, or
/// emit a signal via the held [`crate::Control`] — before returning the response.
pub async fn serve<H, Fut>(addr: &str, handler: H) -> Result<()>
where
    H: Fn(Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Value> + Send + 'static,
{
    let listener = bind_reclaim(addr)?;
    let handler = Arc::new(handler);
    // Accept failures are transient on unix (EMFILE, ECONNABORTED) but STRUCTURAL on
    // Windows, where `accept` creates a new pipe-server instance before it awaits — so a
    // persistent create failure returns synchronously, and a bare `continue` would spin a
    // core forever with nothing awaited and nothing logged.
    let mut failures: u32 = 0;
    loop {
        let mut stream = match listener.accept().await {
            Ok(s) => {
                failures = 0;
                s
            }
            Err(e) => {
                failures += 1;
                if failures == 1 {
                    eprintln!("clappkit: ipc accept failed on {addr}: {e}");
                }
                if failures >= MAX_ACCEPT_FAILURES {
                    return Err(anyhow::anyhow!(
                        "ipc listener at {addr} failed {failures} times in a row: {e}"
                    ));
                }
                // Backoff 50ms → 800ms. Also the loop's only await on the error path, so
                // a broken listener can never monopolise a runtime worker.
                let ms = 50u64 << (failures - 1).min(4);
                tokio::time::sleep(Duration::from_millis(ms)).await;
                continue;
            }
        };
        let handler = handler.clone();
        tokio::spawn(async move {
            let read = tokio::time::timeout(REQUEST_TIMEOUT, frame::read::<_, Value>(&mut stream, LIMITS));
            if let Ok(Ok(Some(req))) = read.await {
                let resp = handler(req).await;
                let _ = frame::write(&mut stream, &resp, LIMITS.max_frame).await;
            }
        });
    }
}

/// Bind the app IPC as the single instance: reclaim what a crashed previous instance left
/// behind, refuse to clobber a LIVE one.
///
/// Both halves are platform-specific and neither is free. On unix a stale socket FILE
/// survives a crash and `bind` fails with `AddrInUse`, so we probe it and remove it if
/// nothing answers. On Windows the transport deliberately does NOT reserve the name
/// (`clatch-ipc`'s `Listener::bind` skips `first_pipe_instance` so Clatch's daemon can
/// rebind over a corpse), which means `create` SUCCEEDS on a name another server already
/// owns — the second instance would silently add an instance to the same pipe and the OS
/// would hand each incoming CLI connection to whichever server happened to be pending.
/// Two `clock app` processes, one pipe, commands landing in the wrong store at random. So
/// on Windows we must probe BEFORE binding; `AddrInUse` never arrives there.
fn bind_reclaim(addr: &str) -> Result<Listener> {
    #[cfg(windows)]
    {
        if probe_live(addr) == Some(true) {
            return Err(already_running(addr));
        }
    }
    match Listener::bind(addr) {
        Ok(l) => Ok(l),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            #[cfg(unix)]
            {
                // If nothing answers on the socket, it is a crash leftover: reclaim it.
                if std::os::unix::net::UnixStream::connect(addr).is_err() {
                    let _ = std::fs::remove_file(addr);
                    return Ok(Listener::bind(addr)?);
                }
            }
            Err(already_running(addr))
        }
        Err(e) => Err(e.into()),
    }
}

fn already_running(addr: &str) -> anyhow::Error {
    anyhow::anyhow!("another instance is already running (IPC address {addr} in use)")
}

/// Is a live server holding this pipe name? `Some(true)` yes, `Some(false)` the name is
/// free, `None` we cannot tell (let `bind` decide). A single non-retrying dial — NOT
/// `clatch_ipc::connect`, which retries `ERROR_PIPE_BUSY` for five seconds and would turn
/// every launch into a five-second stall. `ERROR_PIPE_BUSY` is itself proof of life: the
/// name exists and every instance is occupied.
#[cfg(windows)]
fn probe_live(addr: &str) -> Option<bool> {
    use tokio::net::windows::named_pipe::ClientOptions;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_PIPE_BUSY: i32 = 231;
    match ClientOptions::new().open(addr) {
        Ok(_client) => Some(true), // dropped immediately; the server sees a bare connect
        Err(e) => match e.raw_os_error() {
            Some(ERROR_PIPE_BUSY) => Some(true),
            Some(ERROR_FILE_NOT_FOUND) => Some(false),
            _ => None,
        },
    }
}

/// CLI side: connect to the running app, send one JSON request, return its JSON reply.
/// Failures are mapped to agent-actionable messages (reference/app-developer.md §
/// sandbox-aware errors) so the model escalates or relaunches correctly.
pub async fn request(cli: &str, req: &Value) -> Result<Value> {
    let addr = address(cli);
    let mut stream = match connect(&addr).await {
        Ok(s) => s,
        Err(e) => return Err(sandbox_aware(cli, &e)),
    };
    frame::write(&mut stream, req, LIMITS.max_frame).await?;
    frame::read::<_, Value>(&mut stream, LIMITS)
        .await?
        .ok_or_else(|| anyhow::anyhow!("{cli}: the app closed the connection"))
}

/// Map a connect error to something the agent can act on.
fn sandbox_aware(cli: &str, e: &std::io::Error) -> anyhow::Error {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound | ConnectionRefused => {
            anyhow::anyhow!("{cli}: app is not running — start it with `clatch run <id>`")
        }
        PermissionDenied => anyhow::anyhow!(
            "{cli}: connection blocked by the sandbox; retry with escalated permissions"
        ),
        _ => anyhow::anyhow!("{cli}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_unix_socket_lives_in_the_apps_dotdir() {
        assert_eq!(
            socket_path(Path::new("/Users/ada"), "clock"),
            Path::new("/Users/ada/.clock/clock.sock")
        );
    }

    #[test]
    fn the_socket_path_is_joined_never_interpolated() {
        // A Windows home in the unix arm (git-bash sets HOME) must not yield the mixed
        // `C:\Users\x/.clock/clock.sock` the old `format!` produced.
        let p = socket_path(Path::new("C:\\Users\\berkin"), "clock");
        assert_eq!(p.file_name().unwrap(), "clock.sock");
        assert_eq!(p.parent().unwrap().file_name().unwrap(), ".clock");
    }

    #[test]
    fn the_pipe_name_is_per_app_and_per_user() {
        assert_eq!(pipe_name("clock", "abc"), r"\\.\pipe\clapp-clock-abc");
        assert_ne!(pipe_name("clock", "abc"), pipe_name("chess", "abc"));
        assert_ne!(pipe_name("clock", "abc"), pipe_name("clock", "def"));
    }

    #[test]
    fn the_user_scope_is_stable_short_and_not_the_username() {
        let a = user_scope();
        assert_eq!(a, user_scope(), "both roles must derive the same name");
        assert_eq!(a.len(), if a == "shared" { 6 } else { 12 });
        if let Ok(u) = std::env::var("USERNAME") {
            if !u.is_empty() {
                assert!(!a.contains(&u), "the pipe name must not publish the username");
            }
        }
    }

    #[tokio::test]
    async fn a_second_bind_on_a_live_address_is_refused() {
        let dir = std::env::temp_dir().join(format!("clappkit-ipc-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let addr = dir.join("t.sock").to_string_lossy().into_owned();
        let first = bind_reclaim(&addr).expect("first bind owns the address");
        let refusal = match bind_reclaim(&addr) {
            Ok(_) => panic!("a live instance must not be clobbered"),
            Err(e) => e.to_string(),
        };
        assert!(refusal.contains("already running"), "{refusal}");
        drop(first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_stale_socket_from_a_crashed_instance_is_reclaimed() {
        let dir = std::env::temp_dir().join(format!("clappkit-ipc-stale-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.sock");
        let addr = path.to_string_lossy().into_owned();
        // A crash leaves the socket FILE behind with nothing listening on it. std's
        // UnixListener does not unlink on drop, so this reproduces that state exactly.
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists(), "the leftover socket file is the precondition");
        let reclaimed = bind_reclaim(&addr);
        assert!(reclaimed.is_ok(), "a dead instance's socket must be reclaimable");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn serve_answers_a_request_and_survives_a_client_that_says_nothing() {
        let dir = std::env::temp_dir().join(format!("clappkit-ipc-serve-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let addr = dir.join("s.sock").to_string_lossy().into_owned();
        let a = addr.clone();
        tokio::spawn(async move {
            let _ = serve(&a, |req: Value| async move {
                serde_json::json!({ "ok": true, "echo": req.get("cmd").cloned() })
            })
            .await;
        });
        // Give the listener a moment to bind.
        tokio::time::sleep(Duration::from_millis(80)).await;

        // A client that connects and drops without writing must not wedge the server.
        drop(connect(&addr).await.expect("connect"));

        let mut s = connect(&addr).await.expect("connect");
        frame::write(&mut s, &serde_json::json!({ "cmd": "ping" }), LIMITS.max_frame)
            .await
            .unwrap();
        let resp = frame::read::<_, Value>(&mut s, LIMITS).await.unwrap().unwrap();
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["echo"], "ping");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
