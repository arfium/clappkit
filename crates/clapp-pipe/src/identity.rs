//! Identity by injection (docs/protocol.md § Transport).
//!
//! Clatch is the parent: it mints a per-run identity and applies it to the
//! spawned app's environment before the app's code runs. **This side only reads
//! it back.** Minting and injecting are the launcher's half of the same act, and
//! an SDK able to mint would hand an app the one thing the token exists to
//! prove - that the process is the one Clatch spawned.

use crate::vocab::AppId;

/// Injected environment variable names (docs/protocol.md § Transport). No
/// protocol version is injected: the major an app targets is its manifest's
/// `protocol`, validated at install (docs/protocol.md § Versioning).
pub const ENV_APP_ID: &str = "CLATCH_APP_ID";
pub const ENV_INSTANCE_ID: &str = "CLATCH_INSTANCE_ID";
pub const ENV_CONTROL_ADDR: &str = "CLATCH_CONTROL_ADDR";
pub const ENV_TOKEN: &str = "CLATCH_INSTANCE_TOKEN";

/// The per-run identity Clatch assigned to this process: who it is, which run,
/// where to connect back, and the one-time secret proving the process is the one
/// Clatch spawned.
#[derive(Debug, Clone)]
pub struct Identity {
    pub app_id: AppId,
    pub instance_id: String,
    pub token: String,
    /// Socket path (unix) or pipe name (windows) to connect back to. Read from
    /// the environment, never composed here: the address is the launcher's to
    /// choose, and an app that built its own would be guessing.
    pub addr: String,
}

impl Identity {
    /// Read the injected identity. `None` means this process was not launched by
    /// Clatch, which is what the bootstrap and the dev hatch both branch on.
    pub fn from_env() -> Option<Self> {
        Some(Self {
            app_id: AppId::new(std::env::var(ENV_APP_ID).ok()?),
            instance_id: std::env::var(ENV_INSTANCE_ID).ok()?,
            token: std::env::var(ENV_TOKEN).ok()?,
            addr: std::env::var(ENV_CONTROL_ADDR).ok()?,
        })
    }
}
