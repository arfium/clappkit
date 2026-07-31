//! The app-level verbs every clapp answers ITSELF, before its state sees anything:
//! `ping` · `focus`/`show` · `hide` · `quit`/`close` — the set the Swift `AppDelegate.handle`
//! owned. Four apps reimplemented this match and had already forked on the one constant
//! that matters (the quit grace), so the *decision* lives here, tauri-free and testable,
//! and [`crate::app`] (the `tauri` feature) is the ten lines that act on it.
//!
//! Why the split matters: whether a window verb is a window verb is pure logic that a
//! headless clapp, a test, and the GUI must all agree on. Whether `hide` demotes to
//! macOS `.accessory` is a policy each app declares once, in [`WindowPolicy`].

use serde_json::{json, Value};

/// What a clapp does with its window on the shared verbs. Declared once per app, next to
/// the manifest's `connector.commands`, so the two cannot drift.
#[derive(Clone, Copy, Debug)]
pub struct WindowPolicy {
    /// `hide` (and the close box) drop the app to the BACKGROUND instead of quitting —
    /// macOS `.accessory`, no Dock tile, the process keeps running. Clock sets this,
    /// because a clock that stops keeping time when you close its window is not a clock.
    /// The chess-like apps (chess/telegram/whatsapp) leave it false: closing quits, and
    /// `hide` is not one of their verbs.
    pub backgroundable: bool,
    /// Milliseconds to answer-then-exit on `quit`, so the CLI's response frame is written
    /// while it still holds the socket. This constant had drifted to 120 in three apps and
    /// 150 in the fourth with no explanation; 150 is the safe one, so 150 is the default.
    pub quit_grace_ms: u64,
    /// Icon bytes to re-assert after a `.regular` promotion rebuilds the Dock tile (which
    /// silently drops the icon set while `.accessory`). Only meaningful when
    /// `backgroundable`, since only a demoting app is ever promoted back.
    pub icon: Option<&'static [u8]>,
}

impl Default for WindowPolicy {
    fn default() -> Self {
        Self { backgroundable: false, quit_grace_ms: 150, icon: None }
    }
}

impl WindowPolicy {
    /// The clock-shaped policy: closing/hiding backgrounds the app instead of ending it,
    /// and `icon` is re-asserted when the window (and the Dock tile) comes back.
    pub fn backgroundable(icon: &'static [u8]) -> Self {
        Self { backgroundable: true, icon: Some(icon), ..Self::default() }
    }
}

/// One of the app-level verbs, already resolved from its aliases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowVerb {
    /// "are you there" — the liveness probe every CLI uses before it trusts a socket.
    Ping,
    /// `show` / `focus`: window back on screen, app back in the Dock.
    Show,
    /// `hide`: no window, no Dock tile, process still alive. Backgroundable apps only.
    Hide,
    /// `quit` / `close`: the one verb that ends the process.
    Quit,
}

/// Classify a command envelope's `cmd`. `None` means "not a window verb — pass it to your
/// state", which is what keeps a domain command named `set` or `send` from being swallowed
/// here.
///
/// `hide` is only a window verb for a [`WindowPolicy::backgroundable`] app: hiding an app
/// that cannot background leaves a window the human has no obvious way back to, so the
/// non-backgroundable apps let it fall through to their state's "unknown command" — the
/// behaviour their manifests already describe (none of them declares `hide`).
pub fn classify(cmd: &str, policy: &WindowPolicy) -> Option<WindowVerb> {
    match cmd {
        "ping" => Some(WindowVerb::Ping),
        "show" | "focus" => Some(WindowVerb::Show),
        "hide" if policy.backgroundable => Some(WindowVerb::Hide),
        "quit" | "close" => Some(WindowVerb::Quit),
        _ => None,
    }
}

/// [`classify`] straight off a command envelope (`{"cmd": …}`), the shape both the webview
/// and the CLI send.
pub fn classify_req(req: &Value, policy: &WindowPolicy) -> Option<WindowVerb> {
    classify(req.get("cmd").and_then(Value::as_str)?, policy)
}

/// The response a window verb answers with. `message` is what the CLIs print for a verb
/// that returns no state, so it is the whole of the user-visible output for these.
pub fn reply(verb: WindowVerb) -> Value {
    match verb {
        WindowVerb::Ping => json!({ "ok": true }),
        WindowVerb::Show => json!({ "ok": true, "message": "window shown" }),
        WindowVerb::Hide => json!({ "ok": true, "message": "running in the background" }),
        WindowVerb::Quit => json!({ "ok": true, "message": "bye" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_verbs_and_their_aliases() {
        let p = WindowPolicy::default();
        assert_eq!(classify("ping", &p), Some(WindowVerb::Ping));
        assert_eq!(classify("focus", &p), Some(WindowVerb::Show));
        assert_eq!(classify("show", &p), Some(WindowVerb::Show));
        assert_eq!(classify("quit", &p), Some(WindowVerb::Quit));
        assert_eq!(classify("close", &p), Some(WindowVerb::Quit));
    }

    #[test]
    fn hide_belongs_to_backgroundable_apps_only() {
        assert_eq!(classify("hide", &WindowPolicy::default()), None);
        assert_eq!(
            classify("hide", &WindowPolicy::backgroundable(b"png")),
            Some(WindowVerb::Hide)
        );
    }

    #[test]
    fn domain_commands_are_never_swallowed() {
        // Every verb the four manifests declare that is NOT a window verb must fall
        // through to the app's state, or the app loses the command entirely.
        let p = WindowPolicy::backgroundable(b"png");
        for cmd in [
            "board", "fen", "legal", "move", "say", "new", "resign", "takeback", // chess
            "now", "list", "alarm", "edit", "timer", "set", "cancel", "toggle", "pause",
            "resume", // clock
            "send", "bots", "chats", "status", // telegram / whatsapp
            "state", "setDefaultAgent", "addBot", "removeBot", "", "unknown",
        ] {
            assert_eq!(classify(cmd, &p), None, "{cmd} must reach the app's state");
        }
    }

    #[test]
    fn classify_req_reads_the_command_envelope() {
        let p = WindowPolicy::default();
        assert_eq!(classify_req(&json!({ "cmd": "ping" }), &p), Some(WindowVerb::Ping));
        assert_eq!(classify_req(&json!({ "cmd": "move", "uci": "e2e4" }), &p), None);
        assert_eq!(classify_req(&json!({}), &p), None);
        assert_eq!(classify_req(&json!({ "cmd": 7 }), &p), None);
    }

    #[test]
    fn every_reply_is_ok_and_the_no_state_verbs_carry_a_message() {
        for v in [WindowVerb::Ping, WindowVerb::Show, WindowVerb::Hide, WindowVerb::Quit] {
            assert_eq!(reply(v).get("ok").and_then(Value::as_bool), Some(true));
        }
        assert_eq!(reply(WindowVerb::Quit)["message"], "bye");
    }

    #[test]
    fn the_default_quit_grace_is_the_safe_one() {
        // Three apps shipped 120 ms, one 150 ms; the grace is what gets the CLI's reply
        // onto the wire before the process dies, so the longer value is the correct one.
        assert_eq!(WindowPolicy::default().quit_grace_ms, 150);
        assert!(!WindowPolicy::default().backgroundable);
        assert!(WindowPolicy::backgroundable(b"x").icon.is_some());
    }
}
