//! Errors for the substrate. Every fallible API returns `Result<T, Error>`; IO errors keep
//! their path and their source, so nothing is flattened into a string on the way up.

use std::path::{Path, PathBuf};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("cancelled")]
    Cancelled,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("io {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    /// An IO error that keeps the path and the underlying `io::Error` — the source chain
    /// stays intact rather than being stringified.
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io { path: path.as_ref().to_path_buf(), source }
    }

    /// This error with `ctx` in front of it, keeping ONE kind prefix.
    /// `Invalid(format!("{ctx}: {e}"))` renders the kind twice — a Windows user once read
    /// "invalid: codex-acp: linking codex-acp: invalid: C:\…" off a red banner. An IO error
    /// travels unchanged: it already carries the path and the source, and prefixing would
    /// flatten that chain.
    pub fn context(self, ctx: impl std::fmt::Display) -> Self {
        match self {
            Self::Invalid(m) => Self::Invalid(format!("{ctx}: {m}")),
            Self::NotFound(m) => Self::NotFound(format!("{ctx}: {m}")),
            e => e,
        }
    }
}
