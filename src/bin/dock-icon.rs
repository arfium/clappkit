//! `dock-icon <src.png> <out.png>` — write the Dock/taskbar form of an icon.
//!
//! Packaging needs the SAME inset a running clapp applies (icon.rs: a full-bleed library
//! tile scaled to Apple's ~80% grid, a glyph left alone), so the icon baked into a macOS
//! `.app` bundle matches the one the process sets at runtime. Sharing the real function
//! keeps that a single, unit-tested implementation instead of a shell approximation —
//! `sips` cannot pad to a transparent canvas anyway.
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(src), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: dock-icon <src.png> <out.png>");
        return std::process::ExitCode::FAILURE;
    };
    match std::fs::read(src).map(|b| clappkit::dock_icon(&b)) {
        Ok(png) => match std::fs::write(out, png) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => { eprintln!("dock-icon: cannot write {out}: {e}"); std::process::ExitCode::FAILURE }
        },
        Err(e) => { eprintln!("dock-icon: cannot read {src}: {e}"); std::process::ExitCode::FAILURE }
    }
}
