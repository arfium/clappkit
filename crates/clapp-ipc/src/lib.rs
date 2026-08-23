//! `clapp-ipc` — the local control-channel substrate.
//!
//! One concept: a framed, ordered, bidirectional JSON-RPC 2.0 channel between two local
//! processes, over a unix domain socket (macOS/Linux) or a named pipe (Windows).
//!
//! Two vocabularies are built on top of it and this crate knows neither: the app control
//! pipe (`clapp-pipe`) and the app's own GUI↔CLI channel (`clappkit::ipc`), which the
//! launcher never sees.
//!
//! Layers: [`frame`] (length-prefixed JSON) carries the `rpc` envelope, over a `transport`
//! socket, driven by a [`Peer`] pump.

mod error;
pub mod frame;
mod peer;
mod rpc;
mod transport;

pub use error::{Error, Result};
pub use frame::FrameLimits;
pub use peer::{Inbox, Peer};
pub use rpc::{Frame, Id, Kind, Notification, Request, Response, RpcError};
pub use transport::{connect, Listener};
