//! The shared foundation every clapp is built on — see `docs/architecture.md`.
//!
//! A clapp is one binary with two roles over one state: a window for the human and a CLI
//! for the agent. This crate is the plumbing both need:
//!
//!   * [`control`] — the Clatch control pipe: bootstrap, register, roster, signals
//!   * [`ipc`] — the app's own GUI↔CLI channel, which Clatch never sees
//!   * [`role`] — which of the two roles this process is
//!   * [`paths`] · [`store`] — where state lives, and writing it atomically and privately
//!   * [`media`] — the outbox/quarantine boundary for files
//!   * [`window`] · [`asset`] · [`snapshot`] — the small decisions every GUI clapp reran
//!
//! Two optional layers: `icon` (default) for the Dock/taskbar tile, and `tauri` (off) for
//! the Tauri glue a GUI clapp needs — a headless one never compiles a webview.
//!
//! Where an OS difference is unavoidable it is written once and named: [`ipc::address`],
//! [`paths::user_base`], [`store::atomic_write`].

pub use clapp_pipe::{SignalDecl, SignalType};

use anyhow::{bail, Result};

pub mod asset;
pub mod control;
#[cfg(feature = "icon")]
pub mod icon;
pub mod ipc;
pub mod manifest;
pub mod media;
pub mod paths;
pub mod role;
pub mod snapshot;
pub mod store;
pub mod window;

/// The Tauri half of a clapp (`tauri` feature): icon, window verbs, the `asset` command
/// and the GUI↔CLI relay.
#[cfg(feature = "tauri")]
pub mod app;

pub use control::{Agent, AgentRow, Avatar, Control, Emit, OnShutdown, ShutdownCause};
#[cfg(feature = "icon")]
pub use icon::{dock_icon, set_dock_icon};
pub use media::Kind;
pub use paths::{data_dir, data_file, data_subdir, ensure_private_dir};
pub use window::{WindowPolicy, WindowVerb};

/// Run only under Clatch — the Steam↔game dependency (reference/launch.md). Call it
/// FIRST in `main`. `Ok(true)` means it asked Clatch to relaunch your installed copy
/// properly and **this process must exit now**; `Ok(false)` means you are wired (or in
/// the `CLATCH_STANDALONE` dev hatch): continue. [`role::main_dispatch`] does this for you.
pub fn bootstrap(app_id: &str) -> Result<bool> {
    Ok(clapp_pipe::clatch_init(app_id)?)
}

/// This app's signal declarations, read from its `clatch.json`, or an error saying
/// exactly which failure it was — the file was not found (and where we looked), or it was
/// found and would not parse.
///
/// That distinction matters more than it looks: an empty declaration table is not a
/// harmless default. Every signal the app ever emits is then refused by the control loop,
/// so the window opens, the CLI answers, the roster tracks — and not one alarm, message
/// or move ever reaches an agent. Swallowing the cause made that outage unattributable.
pub fn try_declared_signals() -> Result<Vec<SignalDecl>> {
    let mut looked = Vec::new();
    for candidate in manifest_candidates() {
        match std::fs::read_to_string(&candidate) {
            Ok(json) => {
                return manifest::declared_signals(&json)
                    .map_err(|e| anyhow::anyhow!("{} failed to parse: {e}", candidate.display()));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => looked.push(candidate),
            Err(e) => bail!("cannot read {}: {e}", candidate.display()),
        }
    }
    let looked: Vec<String> = looked.iter().map(|p| p.display().to_string()).collect();
    bail!("clatch.json not found — looked in: {}", looked.join(", "))
}

/// Where a clapp's manifest can be: the working directory first (under Clatch that IS the
/// install dir), then beside the executable and one level up — the packaged layout, where
/// the binary is `<depot>/bin/<cli>` and the manifest is `<depot>/clatch.json`. Checking
/// those makes a symlinked install or a launcher change a non-event instead of a silent,
/// total signal outage.
fn manifest_candidates() -> Vec<std::path::PathBuf> {
    let mut out = vec![std::path::PathBuf::from("clatch.json")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("clatch.json"));
            if let Some(up) = dir.parent() {
                out.push(up.join("clatch.json"));
            }
        }
    }
    out
}

/// [`try_declared_signals`], degraded to the old lossy shape for call sites that just want
/// a list: the failure is printed (it is never nothing), and an empty table is returned.
/// The binding stamps each emitted signal's declared type from these — the same id→type
/// table a native app kept as constants.
pub fn declared_signals() -> Vec<SignalDecl> {
    match try_declared_signals() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("clappkit: {e}");
            Vec::new()
        }
    }
}

/// Connect the control pipe and register with Clatch, returning a reactive [`Control`]
/// handle: emit signals from any task while it serves Clatch's callbacks. `declared` is
/// usually [`declared_signals`]. In the `CLATCH_STANDALONE` dev hatch it returns a no-op
/// handle so the GUI and CLI still run.
pub async fn connect(declared: Vec<SignalDecl>) -> Result<Control> {
    Control::connect(declared).await
}

/// [`connect`] with this app's [`declared_signals`], or a fatal `<cli>: <err>` and
/// `exit(1)` — the four-line block every clapp opened `app::run` with. Call it on Tauri's
/// own async runtime (`tauri::async_runtime::block_on`) so the reactive loop shares that
/// runtime.
pub async fn connect_or_die(cli: &str) -> Control {
    match connect(declared_signals()).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{cli}: {e}");
            std::process::exit(1);
        }
    }
}

/// [`connect_or_die`] with a shutdown hook, for an app that owns something the process
/// exit would otherwise strand — a Node sidecar, a poller, an unflushed store. See
/// [`OnShutdown`].
pub async fn connect_or_die_with(cli: &str, on_shutdown: OnShutdown) -> Control {
    match Control::connect_with(declared_signals(), on_shutdown).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{cli}: {e}");
            std::process::exit(1);
        }
    }
}

/// One lock for every test in this crate that touches `CLATCH_DATA_DIR`.
///
/// The variable is process-global and [`paths::data_dir`] obeys it for EVERY cli name, so
/// two tests that set it concurrently do not merely interleave — one of them ends up
/// operating inside the other's scratch directory. That is not hypothetical: a paths test
/// deleted `data_dir("clappkit-test")` as cleanup, which under a media test's environment
/// resolved to the media quarantine and removed it mid-write. The suite failed about one
/// run in one, and only ever in the innocent test.
///
/// Take this before setting the variable, and hold it for the whole test.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    /// The two manifest tests move the process's working directory, which is global to the
    /// test binary — so they take turns.
    static CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn the_manifest_is_looked_for_in_the_cwd_then_beside_the_binary() {
        let c = manifest_candidates();
        assert_eq!(c[0], std::path::PathBuf::from("clatch.json"), "cwd first: Clatch runs us there");
        assert!(c.len() >= 2, "the packaged layout must also be searched");
        assert!(c.iter().all(|p| p.file_name().unwrap() == "clatch.json"));
    }

    #[test]
    fn a_missing_manifest_names_every_place_it_looked() {
        // Run from a directory with no clatch.json: the error must be actionable, and
        // `declared_signals` must still degrade to empty rather than panic.
        let _guard = CWD.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("clappkit-nomanifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let err = try_declared_signals().unwrap_err().to_string();
        std::env::set_current_dir(prev).unwrap();
        assert!(err.contains("clatch.json not found"), "{err}");
        assert!(err.contains("looked in"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_manifest_is_a_parse_error_not_an_empty_table() {
        let _guard = CWD.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("clappkit-badmanifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("clatch.json"), "{ nope").unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let err = try_declared_signals().unwrap_err().to_string();
        std::env::set_current_dir(prev).unwrap();
        assert!(err.contains("failed to parse"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
