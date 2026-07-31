//! One binary, two roles, chosen by argv (reference/app-developer.md § a note on
//! shape). `app` (or no args, the launcher default) is the GUI process Clatch starts
//! via the manifest `launch`; anything else is the agent's CLI client. Keeping this
//! dispatch identical across clapps is part of the DRY foundation.

/// What this invocation is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// The GUI/app process — Clatch's `launch` runs `<bin> app`.
    App,
    /// The agent's CLI client: the verb and its arguments (argv without the program).
    Cli(Vec<String>),
}

/// Classify `std::env::args()`: `app` or no arguments → the GUI; anything else → the
/// CLI client. `<bin> render …` (a dev preview) is treated as CLI and left for the app
/// to interpret.
pub fn role() -> Role {
    classify(std::env::args().skip(1).collect())
}

/// [`role`]'s rule, with argv passed in — so it is testable, and so a test can pin the
/// one decision that routes every invocation this binary will ever receive.
pub fn classify(args: Vec<String>) -> Role {
    match args.first().map(String::as_str) {
        Some("app") | None => Role::App,
        _ => Role::Cli(args),
    }
}

/// The whole of a clapp's `main()`: dispatch on argv, run the agent's CLI on a fresh
/// multi-thread runtime, or [`crate::bootstrap`] (run-only-under-Clatch) and enter the GUI.
///
/// Four `main.rs` files were 19 identical lines differing only in the app id and the
/// string in one `eprintln!`. `cli_name` is that string — it prefixes the fatal error so
/// it reads like every other message the agent sees: `<cli>: <what went wrong>`.
///
/// The GUI half is deliberately NOT run on a runtime: Tauri builds its own
/// (`tauri::async_runtime`), and `gui` must own the main thread — on macOS the window
/// server accepts nothing else.
///
/// A note for the Windows port: do **not** put `windows_subsystem = "windows"` on a clapp
/// binary. The attribute applies to the whole image, but this image is two roles, and a
/// GUI-subsystem process gets no console and is not waited on by the `.cmd` shim Clatch
/// links onto the agent's PATH — so every CLI call would return instantly, empty, with
/// exit code 0. Clatch already spawns the launch command with `CREATE_NO_WINDOW`, so a
/// console-subsystem clapp shows no console anyway.
pub fn main_dispatch<F, Fut, T, G>(app_id: &str, cli_name: &str, cli: F, gui: G)
where
    F: FnOnce(Vec<String>) -> Fut,
    Fut: std::future::Future<Output = T>,
    G: FnOnce(),
{
    match role() {
        Role::Cli(args) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap_or_else(|e| die(cli_name, &format!("cannot start a runtime: {e}")));
            rt.block_on(cli(args));
        }
        // `Ok(true)`: bootstrap asked Clatch to relaunch our installed copy properly, so
        // THIS process must simply end (reference/launch.md, the Steam↔game dependency).
        Role::App => match crate::bootstrap(app_id) {
            Ok(true) => {}
            Ok(false) => gui(),
            Err(e) => die(cli_name, &e.to_string()),
        },
    }
}

fn die(cli_name: &str, msg: &str) -> ! {
    eprintln!("{cli_name}: {msg}");
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_launcher_default_and_the_explicit_app_verb_are_the_gui() {
        assert_eq!(classify(vec![]), Role::App);
        assert_eq!(classify(args(&["app"])), Role::App);
        // Clock's `app --background` still routes on argv[1], so the flag rides along.
        assert_eq!(classify(args(&["app", "--background"])), Role::App);
    }

    #[test]
    fn everything_else_is_the_agents_cli_with_argv_intact() {
        assert_eq!(classify(args(&["list"])), Role::Cli(args(&["list"])));
        assert_eq!(
            classify(args(&["alarm", "07:30", "--repeat", "weekdays"])),
            Role::Cli(args(&["alarm", "07:30", "--repeat", "weekdays"]))
        );
        // A dev preview verb is CLI, for the app to interpret.
        assert_eq!(classify(args(&["render", "x.png"])), Role::Cli(args(&["render", "x.png"])));
    }

    /// A clapp's `cli::run` is declared `async fn run(args: Vec<String>) -> !` — it never
    /// returns, it exits. `Future<Output = !>` does NOT satisfy `Future<Output = ()>`, so
    /// `main_dispatch` is generic over the output type; this instantiation is the proof.
    /// Never called — compiling it is the whole test.
    #[allow(dead_code)]
    fn _accepts_a_cli_that_never_returns() {
        async fn cli(_args: Vec<String>) -> ! {
            std::process::exit(0)
        }
        fn gui() {}
        if false {
            main_dispatch("com.arfium.test", "test", cli, gui);
        }
    }

    /// …and equally a plain `async fn run(args) -> ()`.
    #[allow(dead_code)]
    fn _accepts_a_cli_that_returns_unit() {
        async fn cli(_args: Vec<String>) {}
        if false {
            main_dispatch("com.arfium.test", "test", cli, || {});
        }
    }
}
