//! `clapp-pipe` — the reference Rust binding for the **Clatch ↔ app control pipe**.
//!
//! The equivalent of a platform's `steam_api`: **the wire is the contract**, and this crate
//! is the recommended convenience that speaks it for you. `clappkit/docs/protocol.md`
//! defines that wire and is the source of truth; another language implements it directly,
//! because it is small and fully specified.
//!
//! It links only [`clapp_ipc`], so an app depends on it **without depending on the
//! launcher** — nothing here reaches into Clatch's own code.
//!
//! The channel is narrow, ordered and versioned: handshake, identity, signals, health,
//! shutdown. It is **not** the app's own GUI↔CLI pipe — that one is the developer's, and
//! Clatch never sees it.
//!
//! # The app author's surface
//!
//! - [`clatch_init`]: the launch bootstrap — run only under Clatch, relaunch through it
//!   otherwise.
//! - [`Client`]: connect with the injected identity and `register`, then `signal` /
//!   `notify` / `serve` (answer `ping`, exit on `shutdown`).
//! - [`Identity`]: the per-run identity Clatch injected — usually read for you by
//!   [`Client::from_env`].
//!
//! ```no_run
//! use clapp_pipe::{clatch_init, Client, SignalDecl, SignalType};
//!
//! # async fn app() -> clapp_ipc::Result<()> {
//! // Runs only under Clatch: hand off and exit if started any other way.
//! if clatch_init("com.you.todo")? {
//!     return Ok(());
//! }
//! // Connect back. The declared signals (id + type) mirror clatch.json, so the binding
//! // stamps each signal's type on the wire and the launcher can re-validate it.
//! let declared = vec![SignalDecl { id: "item.added".into(), signal_type: SignalType::Run }];
//! let mut client = Client::from_env(&declared).await?;
//! client.signal("item.added", serde_json::json!({ "id": 1 })).await?;
//! client.serve().await?; // answer ping, return on shutdown
//! # Ok(())
//! # }
//! ```
//!
//! Most apps never call this directly: `clappkit::control` wraps it, and
//! `arfium/template-clapp` is the worked example.
//!
//! The whole vocabulary — methods, params, error codes — is in [`wire`]; the nouns it
//! carries are in [`vocab`].

mod bootstrap;
mod client;
mod identity;
pub mod vocab;
pub mod wire;

pub use bootstrap::{clatch_init, ENV_BIN, ENV_STANDALONE};
pub use client::Client;
pub use identity::Identity;
pub use vocab::{AgentId, AppId, Avatar, ConnectedAgent, SignalDecl, SignalType};
