//! Durable app state on disk: load-or-default, and a write a crash cannot truncate.
//!
//! Three clapps wrote the same tmp-then-rename dance by hand, down to the same trailing
//! comment, and drifted anyway (pretty vs compact, no temp-file cleanup). Worse, all
//! three created the file with `std::fs::write`, i.e. `0666 & !umask` — typically
//! world-readable — for a file that holds every @BotFather token in plaintext. The
//! directory was `0700`, so the secret's only protection was its parent; it did not
//! survive a copy, a backup, or a bug report.
//!
//! So this module is the one persistence primitive, and it is deliberately strict:
//!
//! * **atomic** — write a temp file in the same directory, then rename over the target.
//!   A rename is atomic against a concurrent reader on unix AND on Windows/NTFS, so a
//!   reader sees the old file or the new one, never a half-written one.
//! * **durable** — `fsync` the temp file before the rename and the directory after it.
//!   Without that, a power loss can make the rename visible while the data blocks are
//!   not, and the load path then reads a zero-length file as "fresh install" and starts
//!   empty. Silent total data loss is the worst failure a store can have.
//! * **private** — `0600` on the temp file, which the rename carries to the target. A
//!   credential at rest carries its own mode; see [`crate::paths`] on what Windows does
//!   and does not give here.
//! * **honest** — a file that exists but does not parse is reported, not silently
//!   treated as absent, and [`quarantine`] can move it aside so the next save does not
//!   overwrite the only remaining copy of the user's data.

use std::path::{Path, PathBuf};

/// Read and parse a JSON file, or `None` when it is missing, unreadable or unparseable.
/// A corrupt state file must never crash a clapp — it starts from its default instead —
/// but the difference between "no file yet" and "the file is damaged" is printed, because
/// only one of those is a first run.
pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None, // first run
        Err(e) => {
            eprintln!("clappkit: cannot read {}: {e}", path.display());
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!(
                "clappkit: {} is not valid JSON ({e}) — starting from defaults; \
                 move it aside to keep it",
                path.display()
            );
            None
        }
    }
}

/// Write `value` as pretty JSON, atomically and privately. `true` on success; a failure
/// (a full disk, a read-only install dir) is reported on stderr and returns `false` —
/// persistence is best-effort, never a panic and never a lost process.
pub fn save_json<T: serde::Serialize>(path: &Path, value: &T) -> bool {
    let text = match serde_json::to_string_pretty(value) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("clappkit: cannot serialize {}: {e}", path.display());
            return false;
        }
    };
    match atomic_write(path, text.as_bytes()) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("clappkit: cannot save {}: {e}", path.display());
            false
        }
    }
}

/// The primitive behind [`save_json`], for state that is not JSON. Creates the parent
/// directory, writes `<file>.tmp.<pid>` beside the target (same directory, so the rename
/// never crosses a filesystem), fsyncs it, renames over `path`, then fsyncs the
/// directory. The temp file is removed if any step after its creation fails, so a failed
/// save cannot leave litter beside the real file forever.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let tmp = tmp_path(path);

    let write = || -> std::io::Result<()> {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        // Owner-only from the instant it exists — the target inherits this through the
        // rename, so bots.json is never briefly world-readable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        use std::io::Write;
        f.write_all(bytes)?;
        f.sync_all()?; // the bytes are on the platter BEFORE the rename publishes them
        Ok(())
    };

    if let Err(e) = write() {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = rename_over(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    sync_dir(path);
    Ok(())
}

/// Move `path` aside as `<name>.corrupt-<epoch seconds>` and return where it went, so a
/// damaged store can be recovered by hand instead of being overwritten by the next save.
/// `None` if there is nothing there or the rename fails.
pub fn quarantine(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = path.file_name()?.to_string_lossy().into_owned();
    let aside = path.with_file_name(format!("{name}.corrupt-{stamp}"));
    match std::fs::rename(path, &aside) {
        Ok(()) => {
            eprintln!("clappkit: moved the damaged {} to {}", path.display(), aside.display());
            Some(aside)
        }
        Err(e) => {
            eprintln!("clappkit: cannot set {} aside: {e}", path.display());
            None
        }
    }
}

/// `<file>.tmp.<pid>` beside the target. The pid keeps two processes (a GUI and a CLI
/// that fell back to writing) from clobbering each other's staging file mid-write.
fn tmp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "state".into());
    path.with_file_name(format!("{name}.tmp.{}", std::process::id()))
}

/// Rename with a short Windows retry. `MoveFileEx(…, MOVEFILE_REPLACE_EXISTING)` — what
/// `fs::rename` compiles to there — fails with a sharing violation while another process
/// (an indexer, an AV scanner, a reader that opened without `FILE_SHARE_DELETE`) holds
/// the target open. That window is milliseconds, so a few retries turn a spurious "cannot
/// save" into a save. Unix has no such failure mode and takes the first branch every time.
fn rename_over(tmp: &Path, path: &Path) -> std::io::Result<()> {
    match std::fs::rename(tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            #[cfg(windows)]
            {
                for backoff_ms in [20u64, 50, 120] {
                    std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                    if std::fs::rename(tmp, path).is_ok() {
                        return Ok(());
                    }
                }
            }
            Err(e)
        }
    }
}

/// fsync the containing directory so the rename itself survives a power loss. Unix only:
/// on Windows a directory cannot be opened as a file, and `MoveFileEx` is journalled by
/// NTFS, so the metadata write is already ordered.
fn sync_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        if let Ok(f) = std::fs::File::open(dir) {
            let _ = f.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug, Default)]
    struct Persist {
        alarms: Vec<String>,
        next: u32,
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clappkit-store-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = scratch("roundtrip");
        let path = dir.join("clock.json");
        let p = Persist { alarms: vec!["a1".into(), "a2".into()], next: 3 };
        assert!(save_json(&path, &p));
        assert_eq!(load_json::<Persist>(&path), Some(p));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_none_not_an_error() {
        let dir = scratch("missing");
        assert_eq!(load_json::<Persist>(&dir.join("nope.json")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_file_loads_as_none_and_can_be_quarantined() {
        let dir = scratch("corrupt");
        let path = dir.join("bots.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load_json::<Persist>(&path), None);
        let aside = quarantine(&path).expect("quarantine moves it");
        assert!(!path.exists(), "the damaged file must not be left in place");
        assert!(aside.exists());
        assert_eq!(quarantine(&path), None, "nothing to quarantine twice");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_write_leaves_no_temp_file_behind() {
        let dir = scratch("notmp");
        let path = dir.join("state.json");
        assert!(save_json(&path, &Persist::default()));
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "stale temp files: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_save_replaces_the_first_completely() {
        // The truncate-vs-replace trap: a shorter payload must not leave a tail of the
        // longer one behind.
        let dir = scratch("replace");
        let path = dir.join("state.json");
        save_json(&path, &Persist { alarms: vec!["long".repeat(50)], next: 1 });
        save_json(&path, &Persist { alarms: vec![], next: 2 });
        assert_eq!(load_json::<Persist>(&path), Some(Persist { alarms: vec![], next: 2 }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_creates_missing_parents() {
        let dir = scratch("parents");
        let path = dir.join("deep").join("er").join("state.json");
        atomic_write(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_saved_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("mode");
        let path = dir.join("bots.json");
        assert!(save_json(&path, &Persist::default()));
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "a file holding bot tokens must not be world-readable");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_into_an_unwritable_place_reports_false_rather_than_panicking() {
        // A directory where a *file* must be: every step must fail softly.
        let dir = scratch("unwritable");
        let path = dir.join("state.json");
        std::fs::create_dir_all(&path).unwrap();
        assert!(!save_json(&path, &Persist::default()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
