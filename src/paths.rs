//! Where a clapp keeps its state: one resolver, one hardening rule.
//!
//! Clatch sets `CLATCH_DATA_DIR` for a running instance; without it (the standalone
//! hatch, or the CLI role from a shell) we fall back to the per-user OS location.
//!
//! Three rules, each from a bug four hand-rolled resolvers hit:
//!
//! 1. **Join, never interpolate.** `format!("{home}/…")` produced `C:\Users\x/.clock`.
//! 2. **An empty `CLATCH_DATA_DIR` is not a data dir.** It made one app write its store
//!    into the cwd — under Clatch that is the install directory, wiped on update.
//! 3. **No `"."` fallback.** `unwrap_or(".")` means "put credentials in the install
//!    directory". With no home we say so and use a private temp dir.
//!
//! Unix gets `0700`. **Windows gets nothing equivalent** — the directory inherits its
//! parent's ACL, and an owner-only DACL needs a Win32 call this crate does not take.
//! Do not write a comment claiming otherwise.

use std::path::{Path, PathBuf};

/// The directory Clatch gives a running instance for its durable state
/// (docs/protocol.md § the launch environment). Set for the GUI role; usually absent for
/// the agent's CLI role, which is why [`crate::ipc::address`] must NOT be derived from it.
pub const DATA_DIR_ENV: &str = "CLATCH_DATA_DIR";

/// An env var's value, treating "set but empty" as "not set" — the case that silently
/// redirected one clapp's store into its install directory.
fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

/// The user's home directory: `HOME` (unix, and git-bash/MSYS on Windows), else
/// `USERPROFILE` (what Windows actually sets), else the classic `HOMEDRIVE`+`HOMEPATH`
/// pair. `None` when the process has no user context at all — a real condition in a
/// service account, and one the caller must handle rather than paper over with `"."`.
pub fn home() -> Option<PathBuf> {
    if let Some(h) = env_nonempty("HOME") {
        return Some(PathBuf::from(h));
    }
    if let Some(h) = env_nonempty("USERPROFILE") {
        return Some(PathBuf::from(h));
    }
    match (env_nonempty("HOMEDRIVE"), env_nonempty("HOMEPATH")) {
        (Some(d), Some(p)) => Some(PathBuf::from(format!("{d}{p}"))),
        _ => None,
    }
}

/// The per-user base directory an app's private state belongs in, as each OS expects it:
/// the home directory on unix (for the `~/.<cli>` dotdir every clapp already uses), and
/// `%LOCALAPPDATA%` on Windows — a dotdir in the profile root is a unix idiom Windows
/// tooling does not expect, and machine-local state (a device link, a token) must not
/// roam.
pub fn user_base() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env_nonempty("LOCALAPPDATA")
            .or_else(|| env_nonempty("APPDATA"))
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join("AppData").join("Local")))
    }
    #[cfg(not(windows))]
    {
        home()
    }
}

/// `<base>/.<cli>` in the unix style, `<base>\<cli>` in the Windows style. Split out from
/// the `cfg` so BOTH shapes are unit-testable on one host — the reason the old
/// hand-rolled copies were never exercised for Windows at all.
pub(crate) fn join_app_dir(base: &Path, cli: &str, windows_style: bool) -> PathBuf {
    if windows_style {
        base.join(cli)
    } else {
        base.join(format!(".{cli}"))
    }
}

/// The pure resolution rule, with the environment passed in: `CLATCH_DATA_DIR` (when
/// non-empty) wins; otherwise the per-user base decides. `None` means "nowhere to write",
/// which is an error, never a silent cwd.
pub(crate) fn resolve(
    cli: &str,
    data_dir_env: Option<&str>,
    base: Option<&Path>,
    windows_style: bool,
) -> Option<PathBuf> {
    match data_dir_env.map(str::trim).filter(|s| !s.is_empty()) {
        Some(d) => Some(PathBuf::from(d)),
        None => base.map(|b| join_app_dir(b, cli, windows_style)),
    }
}

/// Create `dir` (and its parents) and make it as private as the OS lets us: `0700` on
/// unix. Both steps are best-effort against an existing directory we do not own — a
/// failed `chmod` on someone else's directory is not a reason to refuse to run. See the
/// module docs for what Windows does and does not give you here.
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// This clapp's private data directory, created and hardened — or an error naming the
/// exact reason. Prefer this in code that can report to the human; [`data_dir`] is the
/// infallible form for the `fn path() -> PathBuf` call sites the apps already have.
pub fn try_data_dir(cli: &str) -> anyhow::Result<PathBuf> {
    let env = env_nonempty(DATA_DIR_ENV);
    let base = user_base();
    let dir = resolve(cli, env.as_deref(), base.as_deref(), cfg!(windows)).ok_or_else(|| {
        anyhow::anyhow!(
            "{cli}: cannot determine a writable data directory — neither {DATA_DIR_ENV} \
             nor a home directory ($HOME / %USERPROFILE%) is set"
        )
    })?;
    ensure_private_dir(&dir)?;
    Ok(dir)
}

/// This clapp's private data directory: `$CLATCH_DATA_DIR` when Clatch set a non-empty
/// one, else `<per-user base>/.<cli>` (`%LOCALAPPDATA%\<cli>` on Windows). Created if
/// missing and tightened to `0700` on unix.
///
/// Always returns a path, because a clapp that cannot find its store should degrade, not
/// crash. When resolution genuinely fails it says so on stderr and returns a private
/// directory under the system temp dir — **never** `"."`, which under Clatch is the
/// install directory (an update would eat the user's alarms, tokens or device link).
pub fn data_dir(cli: &str) -> PathBuf {
    match try_data_dir(cli) {
        Ok(d) => d,
        Err(e) => {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| eprintln!("clappkit: {e}; falling back to a temp directory"));
            let fallback = std::env::temp_dir().join(format!("clapp-{cli}"));
            let _ = ensure_private_dir(&fallback);
            fallback
        }
    }
}

/// A file inside [`data_dir`] — `data_dir(cli).join(name)`. The one-liner every app's
/// `fn path()` becomes.
pub fn data_file(cli: &str, name: &str) -> PathBuf {
    data_dir(cli).join(name)
}

/// A sub-directory inside [`data_dir`], created and hardened like its parent — for state
/// that is a tree rather than a file (WhatsApp's multi-device auth keys). Routing it
/// through here is what keeps it inside the directory Clatch backs up and purges.
pub fn data_subdir(cli: &str, name: &str) -> PathBuf {
    let dir = data_dir(cli).join(name);
    let _ = ensure_private_dir(&dir);
    dir
}

/// Move state written by an older version of this clapp to where it lives now, ONCE:
/// `true` if a move happened. A no-op when `current` already exists (the new location
/// wins) or `legacy` does not.
///
/// Relocating a store is otherwise a silent data loss: moving WhatsApp's auth keys under
/// `CLATCH_DATA_DIR` un-links the user's phone, and moving a token file logs every bot
/// out. Call this immediately before the first read of the new path.
pub fn adopt_legacy(legacy: &Path, current: &Path) -> bool {
    if current.exists() || !legacy.exists() {
        return false;
    }
    if let Some(parent) = current.parent() {
        let _ = ensure_private_dir(parent);
    }
    match std::fs::rename(legacy, current) {
        Ok(()) => {
            eprintln!(
                "clappkit: moved existing state from {} to {}",
                legacy.display(),
                current.display()
            );
            true
        }
        Err(e) => {
            // A cross-device rename is the realistic failure ($HOME and the Clatch home on
            // different volumes). Say so rather than starting empty in silence.
            eprintln!(
                "clappkit: could not move {} to {}: {e} — the old copy is still there",
                legacy.display(),
                current.display()
            );
            false
        }
    }
}

/// The install content root, the way the packaged layout defines it: the executable is
/// `<root>/bin/<cli>`, so the root is two levels up. Used to find things that ship BESIDE
/// the binary (a vendored Node runtime, a sidecar bundle) rather than state.
///
/// Symlinks are resolved (a dev checkout, brew, nvm) and then Windows' verbatim `\\?\`
/// prefix is removed by [`simplified`] — see there for why that matters.
pub fn install_root() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let exe = simplified(&std::fs::canonicalize(&exe).unwrap_or(exe));

    // Find the root by what is IN it, not by counting directories up. Counting was right
    // for exactly one layout (`<root>/bin/<cli>`) and broke silently the day macOS depots
    // started shipping a real `.app` bundle — there the executable is
    // `<root>/bin/<Name>.app/Contents/MacOS/<cli>`, five levels down, so "two up" landed
    // inside the bundle and every vendored thing (a Node runtime, a sidecar, an engine)
    // became invisible. `clatch.json` is the one file guaranteed to sit at the depot root,
    // so it is the honest marker.
    let mut dir = exe.parent();
    while let Some(d) = dir {
        if d.join("clatch.json").is_file() {
            return d.to_path_buf();
        }
        dir = d.parent();
    }

    // No manifest anywhere above us: a dev checkout running from `target/release`. Keep
    // the historical guess so nothing that worked before starts failing.
    exe.parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Strip Windows' extended-length `\\?\` prefix, which `fs::canonicalize` always returns
/// there. A verbatim path is not a normal path: it disables ALL normalisation — forward
/// slashes stop being separators, `.` and `..` stop being collapsed — and handing one to a
/// child process (Node's CJS loader, in particular) is a real risk for a path that is only
/// ever used to *find* a file. Everything else, and every unix path, passes through
/// unchanged.
pub fn simplified(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        // \\?\UNC\server\share → \\server\share; a naive 4-char strip would corrupt this.
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        // Only a drive path (`C:\…`) is safe to un-prefix; anything else keeps its prefix.
        let bytes = rest.as_bytes();
        if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
        {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_style_is_a_dotdir_windows_style_is_not() {
        let base = Path::new("/Users/berkin");
        assert_eq!(
            join_app_dir(base, "clock", false),
            PathBuf::from("/Users/berkin/.clock")
        );
        assert_eq!(
            join_app_dir(base, "clock", true),
            PathBuf::from("/Users/berkin").join("clock")
        );
    }

    #[test]
    fn join_never_produces_a_mixed_separator_path() {
        // The bug this module exists to kill: `format!("{home}/.clock")` on a Windows
        // home. `join` uses the host separator and never doubles or mixes one.
        let dir = join_app_dir(Path::new("C:\\Users\\berkin"), "whatsapp", true);
        let s = dir.to_string_lossy().into_owned();
        assert!(s.ends_with("whatsapp"), "{s}");
        assert!(!s.contains("//"), "{s}");
        #[cfg(windows)]
        assert!(!s.contains('/'), "{s}");
    }

    #[test]
    fn clatch_data_dir_wins_when_set() {
        let got = resolve("clock", Some("/var/clatch/appdata/com.arfium.clock"), Some(Path::new("/home/x")), false);
        assert_eq!(got, Some(PathBuf::from("/var/clatch/appdata/com.arfium.clock")));
    }

    #[test]
    fn an_empty_or_blank_data_dir_falls_back_to_the_home_dir() {
        // clock read CLATCH_DATA_DIR without this filter and wrote into the process cwd.
        for empty in ["", "   "] {
            let got = resolve("clock", Some(empty), Some(Path::new("/home/x")), false);
            assert_eq!(got, Some(PathBuf::from("/home/x/.clock")));
        }
    }

    #[test]
    fn no_data_dir_and_no_home_is_an_error_not_a_dot() {
        assert_eq!(resolve("clock", None, None, false), None);
        assert_eq!(resolve("clock", Some(""), None, true), None);
    }

    #[test]
    fn windows_resolution_uses_the_windows_layout() {
        let got = resolve("telegram", None, Some(Path::new("C:\\Users\\berkin\\AppData\\Local")), true);
        assert_eq!(
            got,
            Some(Path::new("C:\\Users\\berkin\\AppData\\Local").join("telegram"))
        );
    }

    #[test]
    fn ensure_private_dir_creates_recursively_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!("clappkit-paths-{}", std::process::id()));
        let deep = root.join("a").join("b");
        ensure_private_dir(&deep).unwrap();
        assert!(deep.is_dir());
        ensure_private_dir(&deep).unwrap(); // second call must not fail
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&deep).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "the data dir must be owner-only on unix");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_verbatim_prefix_is_stripped_only_where_it_is_safe() {
        // The drive form fs::canonicalize returns on Windows.
        assert_eq!(
            simplified(Path::new(r"\\?\C:\Users\berkin\bin\whatsapp.exe")),
            PathBuf::from(r"C:\Users\berkin\bin\whatsapp.exe")
        );
        // A UNC verbatim path must become \\server\share, never UNC\server\share.
        assert_eq!(
            simplified(Path::new(r"\\?\UNC\server\share\app")),
            PathBuf::from(r"\\server\share\app")
        );
        // Anything else — a device path, a plain path, any unix path — is untouched.
        assert_eq!(simplified(Path::new(r"\\?\Volume{123}\x")), PathBuf::from(r"\\?\Volume{123}\x"));
        assert_eq!(simplified(Path::new(r"C:\Users\x")), PathBuf::from(r"C:\Users\x"));
        assert_eq!(simplified(Path::new("/usr/local/bin/clock")), PathBuf::from("/usr/local/bin/clock"));
    }

    #[test]
    fn install_root_is_two_levels_above_the_executable() {
        // The packaged layout is <root>/bin/<cli>; the test binary is equally deep enough.
        let exe = std::env::current_exe().unwrap();
        let want = exe.parent().unwrap().parent().unwrap();
        assert_eq!(
            simplified(&install_root()),
            simplified(&std::fs::canonicalize(want).unwrap_or_else(|_| want.to_path_buf()))
        );
    }

    #[test]
    fn legacy_state_is_adopted_once_and_never_overwrites_the_new_copy() {
        let root = std::env::temp_dir().join(format!("clappkit-adopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let legacy = root.join("old").join("auth");
        let current = root.join("new").join("auth");
        ensure_private_dir(&legacy).unwrap();
        std::fs::write(legacy.join("creds.json"), b"keys").unwrap();

        assert!(adopt_legacy(&legacy, &current), "the first call moves it");
        assert!(current.join("creds.json").exists(), "the phone link must survive the move");
        assert!(!legacy.exists());

        // Second call: nothing left to move, and an existing new copy is never clobbered.
        assert!(!adopt_legacy(&legacy, &current));
        ensure_private_dir(&legacy).unwrap();
        std::fs::write(legacy.join("creds.json"), b"stale").unwrap();
        assert!(!adopt_legacy(&legacy, &current), "the new location wins");
        assert_eq!(std::fs::read(current.join("creds.json")).unwrap(), b"keys");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn data_file_lives_inside_data_dir() {
        // Own the environment for the duration: `data_dir` follows CLATCH_DATA_DIR for
        // every cli name, so without this the scratch dir below — and the cleanup that
        // deletes it — belong to whichever test happened to set the variable last.
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("clappkit-paths-{}", std::process::id()));
        std::env::set_var(DATA_DIR_ENV, &root);

        let f = data_file("clappkit-test", "state.json");
        assert_eq!(f.file_name().unwrap(), "state.json");
        assert_eq!(f.parent().unwrap(), data_dir("clappkit-test"));

        std::env::remove_var(DATA_DIR_ENV);
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod install_root_tests {
    use super::*;

    /// The regression that made a packaged macOS app unable to see its own vendored
    /// engine: `install_root` counted two directories up, which is only true for the
    /// pre-bundle layout. Both layouts must resolve to the depot root.
    #[test]
    fn the_root_is_found_from_both_depot_layouts() {
        let tmp = std::env::temp_dir().join(format!("clappkit-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let plain = tmp.join("bin");
        let bundled = tmp.join("bin/App.app/Contents/MacOS");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::write(tmp.join("clatch.json"), "{}").unwrap();

        // The walk-up is what `install_root` does once it has the executable path.
        let root_of = |exe: &Path| -> Option<PathBuf> {
            let mut dir = exe.parent();
            while let Some(d) = dir {
                if d.join("clatch.json").is_file() {
                    return Some(d.to_path_buf());
                }
                dir = d.parent();
            }
            None
        };

        assert_eq!(root_of(&plain.join("app")).as_deref(), Some(tmp.as_path()));
        assert_eq!(
            root_of(&bundled.join("app")).as_deref(),
            Some(tmp.as_path()),
            "a macOS .app bundle is five levels down, not two"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
