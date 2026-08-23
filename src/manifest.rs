//! Reading this app's own `clatch.json` at runtime.
//!
//! Deliberately **not** a validator. The launcher validates a manifest at install and
//! refuses a package that breaks the format (`docs/format.md`); by the time this runs, the
//! file on disk has already passed that gate. What an app needs here is one thing — the
//! signal ids and types it declared, so the control loop can refuse to emit anything it did
//! not promise — and pulling in a whole install-time validator to read one array was the
//! reason clappkit ever depended on the launcher's registry crate.
//!
//! Unknown fields are ignored rather than rejected, which is the manifest's own additive
//! rule: a launcher ignores fields it does not know, and so does an app.

use crate::SignalDecl;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Connector {
    #[serde(default)]
    signals: Vec<SignalDecl>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    connector: Option<Connector>,
}

/// The `connector.signals[]` this manifest declares. A manifest with no `connector`, or a
/// connector with no `signals`, declares none — which is a valid app that never signals,
/// not an error.
pub fn declared_signals(json: &str) -> Result<Vec<SignalDecl>, serde_json::Error> {
    let m: Manifest = serde_json::from_str(json)?;
    Ok(m.connector.map(|c| c.signals).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SignalType;

    #[test]
    fn declarations_come_back_with_their_types() {
        let s = declared_signals(
            r#"{"id":"com.x.y","connector":{"cli":"y","signals":[
                 {"id":"a","type":"run"},{"id":"b.c","type":"context"}]}}"#,
        )
        .unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].id, "a");
        assert_eq!(s[0].signal_type, SignalType::Run);
        assert_eq!(s[1].signal_type, SignalType::Context);
    }

    /// An app that never signals is ordinary, so neither shape is an error.
    #[test]
    fn no_connector_and_no_signals_both_mean_none() {
        assert!(declared_signals(r#"{"id":"com.x.y"}"#).unwrap().is_empty());
        assert!(declared_signals(r#"{"connector":{"cli":"y"}}"#).unwrap().is_empty());
    }

    /// The manifest is additive-only: a field from a newer schema must not stop an older
    /// app from reading the part it does understand.
    #[test]
    fn a_field_we_do_not_know_is_ignored_not_refused() {
        let s = declared_signals(
            r#"{"somethingNew":42,"connector":{"signals":[{"id":"a","type":"buffered"}],
                 "alsoNew":{"deep":true}}}"#,
        )
        .unwrap();
        assert_eq!(s[0].signal_type, SignalType::Buffered);
    }

    /// A malformed type is a real error: silently dropping it would make the app believe it
    /// declared a signal the launcher will refuse at runtime.
    #[test]
    fn an_unknown_signal_type_is_an_error() {
        assert!(declared_signals(r#"{"connector":{"signals":[{"id":"a","type":"nope"}]}}"#).is_err());
    }
}
