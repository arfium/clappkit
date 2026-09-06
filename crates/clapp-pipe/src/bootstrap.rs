//! The launch bootstrap (docs/format.md): [`clatch_init`], the
//! `SteamAPI_RestartAppIfNecessary` equivalent. An app calls it first thing in
//! `main`; it guarantees the app runs only under Clatch by relaunching itself
//! through the launcher when it was started any other way. It is env checks plus
//! one `clatch run`, so any language can implement it; this is the Rust convenience.

use crate::identity::{ENV_APP_ID, ENV_TOKEN};
use clapp_ipc::{Error, Result};
use std::process::Command;

/// `CLATCH_STANDALONE=1` opts out of the relaunch (the dev hatch): [`clatch_init`]
/// returns `false` so the app keeps running without a launcher.
pub const ENV_STANDALONE: &str = "CLATCH_STANDALONE";
/// `CLATCH_BIN` overrides the `clatch` binary used for the relaunch (tests, custom
/// installs), honored only as an absolute path to an existing file (see [`clatch_bin`]);
/// otherwise `clatch` is resolved on `PATH`. The name itself is the shared
/// [`crate::vocab::ENV_BIN`]; this re-export keeps the pipe crate's API.
pub const ENV_BIN: &str = crate::vocab::ENV_BIN;

/// Ensure this process runs under Clatch (docs/format.md), returning whether it
/// handed off. **`Ok(true)` means the caller must exit now**, Clatch is relaunching
/// the installed copy.
///
/// - **Wired** (`CLATCH_INSTANCE_TOKEN` present): Clatch spawned us. Returns
///   `Ok(false)`; connect and register next ([`crate::Client`]). A `CLATCH_APP_ID`
///   that disagrees with `app_id` is a hard error (wrong binary or manifest id).
/// - **Standalone** (`CLATCH_STANDALONE=1`): the dev hatch. Returns `Ok(false)`.
/// - **Otherwise**: asks Clatch to launch the *installed* copy (`clatch run
///   <app_id>`, which starts the daemon if needed), then returns `Ok(true)`. An
///   error means Clatch could not take over (not installed, `clatch` not found); the
///   app must not run bare.
pub fn clatch_init(app_id: &str) -> Result<bool> {
    if std::env::var_os(ENV_TOKEN).is_some() {
        // Clatch injects identity before our code runs, so a relaunched copy always
        // lands here: this is the loop guard.
        if let Some(wired) = std::env::var_os(ENV_APP_ID) {
            if wired != *app_id {
                return Err(Error::Invalid(format!(
                    "launched as {:?} but this binary is {app_id}",
                    wired.to_string_lossy()
                )));
            }
        }
        return Ok(false);
    }
    if std::env::var_os(ENV_STANDALONE).is_some() {
        if standalone_allowed() {
            return Ok(false);
        }
        // A build compiled without the `standalone` feature refuses the hatch: otherwise
        // CLATCH_STANDALONE=1 silently turns off the "only under Clatch" guarantee on
        // shipped bytes. Fall through to the relaunch (fail-closed) rather than run bare.
        eprintln!("clappkit: {ENV_STANDALONE} is ignored in this build; handing off to Clatch");
    }
    relaunch(app_id).map(|()| true)
}

/// Whether the standalone dev hatch is compiled in. The `standalone` feature is ON by
/// default, so `cargo run`, the tests and the packaged round-trip keep the hatch. A release
/// depot that must refuse bare execution builds with the feature disabled: then
/// CLATCH_STANDALONE cannot make it run without a launcher.
fn standalone_allowed() -> bool {
    cfg!(feature = "standalone")
}

/// The launcher binary for the relaunch. `CLATCH_BIN` may override it for tests and custom
/// installs, but ONLY as an absolute path to an existing file: a relative name is resolved
/// against `PATH` and, on Windows, against the app's own directory first, and this exec is
/// the very first thing a bare-launched clapp does, so a planted `clatch` there would run.
/// An override that is not an absolute existing file is ignored (with a warning) in favour
/// of `clatch` on `PATH`.
fn clatch_bin() -> std::ffi::OsString {
    if let Some(v) = std::env::var_os(ENV_BIN) {
        let p = std::path::Path::new(&v);
        if p.is_absolute() && p.is_file() {
            return v;
        }
        eprintln!(
            "clappkit: ignoring {ENV_BIN}={:?} (not an absolute path to an existing file)",
            v.to_string_lossy()
        );
    }
    "clatch".into()
}

/// Hand off to the launcher: `clatch run <app_id>` (the `clatch://run/<app_id>`
/// path) starts the daemon if needed and launches the installed copy. We wait for
/// the launch to be acknowledged, so a failure surfaces to the caller.
fn relaunch(app_id: &str) -> Result<()> {
    let bin = clatch_bin();
    let mut cmd = Command::new(&bin);
    cmd.arg("run").arg(app_id);
    // No console flash when an app relaunches itself managed (Windows): the same
    // rule the GUI and daemon spawns already follow.
    crate::vocab::hide_console(&mut cmd);
    let status = cmd.status().map_err(|e| Error::io(&bin, e))?;
    if !status.success() {
        return Err(Error::Invalid(format!(
            "clatch could not launch {app_id} (is it installed?)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // clatch_init itself is not unit-tested: its no-token path EXECs `clatch`. These cover
    // the pure decisions around it. They mutate the process-global environment, so they
    // hold this lock to take turns.
    static ENV: Mutex<()> = Mutex::new(());

    #[test]
    fn clatch_bin_ignores_an_unsafe_override_and_honours_an_absolute_file() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let clatch = std::ffi::OsString::from("clatch");

        std::env::remove_var(ENV_BIN);
        assert_eq!(clatch_bin(), clatch, "no override falls back to PATH `clatch`");

        // A relative name would resolve against PATH and the app's own dir: ignored.
        std::env::set_var(ENV_BIN, "clatch");
        assert_eq!(clatch_bin(), clatch, "a relative override is not honoured");

        // An absolute path that does not exist: ignored.
        std::env::set_var(ENV_BIN, "/nonexistent/clatch-xyz-000");
        assert_eq!(clatch_bin(), clatch, "a missing absolute override is not honoured");

        // An absolute path to a real file (the test binary itself): honoured.
        let real = std::env::current_exe().unwrap();
        std::env::set_var(ENV_BIN, &real);
        assert_eq!(clatch_bin(), real.clone().into_os_string(), "an absolute file wins");

        std::env::remove_var(ENV_BIN);
    }

    #[test]
    fn standalone_is_gated_on_the_feature() {
        assert_eq!(standalone_allowed(), cfg!(feature = "standalone"));
    }

    #[test]
    fn from_env_scrubs_only_the_instance_token() {
        use crate::identity::{Identity, ENV_APP_ID, ENV_CONTROL_ADDR, ENV_INSTANCE_ID, ENV_TOKEN};
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());

        std::env::set_var(ENV_APP_ID, "com.x.y");
        std::env::set_var(ENV_INSTANCE_ID, "run-1");
        std::env::set_var(ENV_CONTROL_ADDR, "/tmp/x.sock");
        std::env::set_var(ENV_TOKEN, "s3cret");

        let id = Identity::from_env().expect("all four vars present");
        assert_eq!(id.token, "s3cret", "the token is kept in the struct for the handshake");
        assert!(std::env::var_os(ENV_TOKEN).is_none(), "and scrubbed from the environment");
        // Only the secret is scrubbed; the rest remain for whatever reads them next.
        assert!(std::env::var_os(ENV_APP_ID).is_some());
        assert!(std::env::var_os(ENV_CONTROL_ADDR).is_some());

        for v in [ENV_APP_ID, ENV_INSTANCE_ID, ENV_CONTROL_ADDR] {
            std::env::remove_var(v);
        }
    }
}
