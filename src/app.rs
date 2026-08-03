//! The Tauri glue — `tauri` feature only, so a headless clapp never compiles a webview.
//!
//! Only glue: the decisions live in [`crate::window`] and [`crate::asset`], which are
//! tauri-free and therefore testable without a window server.

use crate::window::{self, WindowPolicy, WindowVerb};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub use crate::window::{WindowPolicy as Policy, WindowVerb as Verb};

/// The webview event carrying a fresh snapshot. Every clapp's `bridge.ts` listens on it,
/// and the literal `"state"` was spelled out in eleven places on the Rust side alone.
pub const STATE_EVENT: &str = "state";

/// What a clapp's command handler answers: the response for the caller, plus the fresh
/// snapshot to broadcast.
///
/// Taking BOTH from one call is not a convenience — it forces them out of the same
/// critical section. The apps that returned only a response then re-locked their state to
/// take a snapshot left a window in which a scheduler tick could interleave, so the state
/// pushed to the webview was from a different moment than the response the caller got.
pub struct Reply {
    pub resp: Value,
    pub snapshot: Value,
}

impl Reply {
    pub fn new(resp: Value, snapshot: Value) -> Reply {
        Reply { resp, snapshot }
    }
}

/// The app's main window: the `"main"` label, falling back to whatever window exists — a
/// relabelled `tauri.conf.json` can never leave the app running with nothing on screen.
pub fn main_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    app.get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())
}

/// Give this bare-executable clapp its own mark the moment it comes up: the Dock on macOS
/// (AppKit), the main window — hence the taskbar — on Windows/Linux. `png` is the app's own
/// `include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/icon.png"))`; the bytes
/// stay per-app because they ARE the app's identity, and they are run through
/// [`crate::dock_icon`] first so a full-bleed shelf tile is inset to the native Dock grid
/// (docs/ICONS.md). **Call on the MAIN THREAD** — Tauri's `setup` closure is the main thread.
pub fn apply_icon(app: &AppHandle, png: &[u8]) {
    crate::set_dock_icon(png);
    #[cfg(not(target_os = "macos"))]
    if let Some(w) = main_window(app) {
        if let Ok(img) = tauri::image::Image::from_bytes(&crate::dock_icon(png)) {
            let _ = w.set_icon(img);
        }
    }
    #[cfg(target_os = "macos")]
    let _ = app;
}

/// Re-assert the Dock icon after promoting to `.regular` built a FRESH Dock tile — which
/// silently drops the icon set while `.accessory`, leaving a bare executable showing the
/// generic terminal tile. Once synchronously, once on the next main-loop turn in case the
/// tile is not ready yet. macOS only; a no-op elsewhere, and unnecessary for an app that
/// never demotes.
pub fn reassert_dock_icon(app: &AppHandle, png: &'static [u8]) {
    #[cfg(target_os = "macos")]
    {
        crate::set_dock_icon(png);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = app.run_on_main_thread(move || crate::set_dock_icon(png));
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, png);
    }
}

/// `show` / `focus` / launch: the window back on screen and (macOS) the app back in the
/// Dock. Pass `icon` for a backgroundable app, whose Dock tile is rebuilt by the promotion.
pub fn show_window(app: &AppHandle, icon: Option<&'static [u8]>) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(w) = main_window(app) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(png) = icon {
        reassert_dock_icon(app, png);
    }
}

/// `hide` / the close box on a backgroundable app: no window, no Dock tile, process still
/// alive and still doing its job.
pub fn hide_window(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let _ = w.hide();
    }
    enter_background(app);
}

/// No Dock tile, no menu bar (Swift's `enterBackground`). macOS only.
pub fn enter_background(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// The app-level verbs a clapp answers ITSELF, before its state sees anything —
/// `ping` · `focus`/`show` · `hide` · `quit`/`close`. `None` means "not a window verb —
/// pass it to your state".
///
/// `quit` answers first and exits after `policy.quit_grace_ms`, because the CLI is still
/// holding the socket and a process that dies before the response frame is written leaves
/// the agent with "the app closed the connection" instead of "bye".
pub fn window_cmd(app: &AppHandle, req: &Value, policy: WindowPolicy) -> Option<Value> {
    let verb = window::classify_req(req, &policy)?;
    match verb {
        WindowVerb::Ping => {}
        WindowVerb::Show => show_window(app, policy.icon),
        WindowVerb::Hide => hide_window(app),
        WindowVerb::Quit => {
            let handle = app.clone();
            let grace = policy.quit_grace_ms;
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(grace)).await;
                handle.exit(0);
            });
        }
    }
    Some(window::reply(verb))
}

/// Push a snapshot to the webview — `app.emit(STATE_EVENT, snap)`, in one place, so the
/// event name is a constant instead of a string literal in every app.
pub fn push_state(app: &AppHandle, snap: Value) {
    let _ = app.emit(STATE_EVENT, snap);
}

/// A local image as a `data:` URI — the webview cannot open `file://`, and the roster
/// hands out absolute paths. `None` when unreadable.
///
/// Declare the command in the app's own crate; `generate_handler!` cannot see it here:
/// `#[tauri::command] fn asset(path: String) -> Option<String> { clappkit::app::asset(&path) }`
pub fn asset(path: &str) -> Option<String> {
    crate::asset::data_uri(path)
}

/// Serve the app's private GUI↔CLI IPC on [`crate::ipc::address`]`(cli)` — the relay every
/// clapp wrote by hand.
///
/// Answers the window verbs here (per `policy`), hands everything else to `handler`
/// together with the CALLER'S AGENT ID (`req["agent"]`, absent for the human — one app
/// forgot to extract it and could not tell an agent's call from the human's), then pushes
/// the snapshot `handler` returned as a [`STATE_EVENT`]. Logs `"<cli>: ipc: <err>"` and
/// returns if the listener dies. Run once from Tauri's `setup`.
pub fn spawn_ipc<H, Fut>(app: AppHandle, cli: &'static str, policy: WindowPolicy, handler: H)
where
    H: Fn(Value, Option<String>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Reply> + Send + 'static,
{
    let handler = Arc::new(handler);
    tauri::async_runtime::spawn(async move {
        let serve_handler = move |req: Value| {
            let app = app.clone();
            let handler = handler.clone();
            async move {
                // Window verbs never reach the app's state: they are the app process itself.
                if let Some(resp) = window_cmd(&app, &req, policy) {
                    return resp;
                }
                let caller = req.get("agent").and_then(Value::as_str).map(String::from);
                let reply = handler(req, caller).await;
                push_state(&app, reply.snapshot);
                reply.resp
            }
        };
        if let Err(e) = crate::ipc::serve(&crate::ipc::address(cli), serve_handler).await {
            eprintln!("{cli}: ipc: {e}");
        }
    });
}
