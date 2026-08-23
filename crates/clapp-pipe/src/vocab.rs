//! The vocabulary the control pipe carries: who an app is, who its agents are, and what a
//! signal may be.
//!
//! These types are the wire's nouns, so they live beside the binding that speaks it rather
//! than in a launcher crate an app would otherwise have to depend on. `docs/protocol.md`
//! and `docs/format.md` in clappkit define them; this is that definition in Rust.

use clapp_ipc::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// The launcher binary to re-exec through, injected at spawn.
pub const ENV_BIN: &str = "CLATCH_BIN";

/// The Windows named-pipe namespace the per-run control pipe lives in.
pub const PIPE_PREFIX: &str = r"\\.\pipe\clatch-";

/// Ids become path segments (`apps/<id>`), so one that crosses a trust boundary must be
/// checked before it is used as a path. Construction stays infallible; validation is a gate
/// the caller applies.
fn check(what: &str, s: &str) -> Result<()> {
    let safe = !s.is_empty()
        && s.len() <= 128
        && s != "."
        && s != ".."
        && !s.contains("..")
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if safe {
        Ok(())
    } else {
        Err(Error::Invalid(format!("{what} {s:?} is not allowed")))
    }
}

/// An app's reverse-DNS id, e.g. `com.arfium.chess`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AppId(String);

impl AppId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// Reject anything unsafe as a path segment (traversal, separators, control).
    pub fn valid(&self) -> Result<()> {
        check("app id", &self.0)
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An agent's immutable identity — the ONLY key an app may target or store under. A name is
/// re-pointable; an id is not, which is why the wire carries ids and shows names.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentId(String);

impl AgentId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn valid(&self) -> Result<()> {
        check("agent id", &self.0)
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An agent's profile photo as served to an app: the metadata plus the same-machine path
/// the bytes are read from, the way app icons are read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Avatar {
    pub mime: String,
    /// Absolute path to the image file.
    pub path: String,
    pub width: u32,
    pub height: u32,
}

/// One agent bound to an app, as pushed to that app (`docs/protocol.md` § Connected
/// agents): the immutable `id`, the display `name` (unique but re-pointable — same id plus
/// a new name is the same agent re-labelled), its backend, current model, and avatar.
///
/// Deliberately minimal: an app never learns an agent's conversation, its other bindings,
/// or anything it was not granted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedAgent {
    pub id: AgentId,
    pub name: String,
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Avatar>,
}

/// A declared signal's TYPE, fixed in the manifest at declaration and never chosen per
/// emission (`docs/format.md` § `connector.signals[]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    /// Starts a turn on an idle agent; on a busy one it queues behind the in-flight turn.
    Run,
    /// Queued, ordered and lossless, injected at the agent's next turn boundary.
    Context,
    /// One replace-in-place slot that rides the user's next prompt.
    Buffered,
}

/// One declared signal: its stable **id** and its declared [`SignalType`]. The id is what an
/// `app.toAgent` frame carries and what the launcher matches against — the whole vocabulary,
/// never a per-emission counter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalDecl {
    pub id: String,
    #[serde(rename = "type")]
    pub signal_type: SignalType,
}

/// Windows: never flash a console for a child we spawn in the background. A no-op elsewhere.
pub fn hide_console(command: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
