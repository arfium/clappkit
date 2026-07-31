//! The snapshot revision — the one field that decides a race the GUI cannot otherwise win.
//!
//! Two writers feed a clapp's window: the return value of the human's own `run_cmd`
//! invoke, and the `state` event the core pushes on its own (a scheduler tick, an inbound
//! message, an agent's move). They race, and an invoke response that resolves late is
//! OLDER than an event already applied — so without an ordering the window can be dragged
//! backwards to a stale board, roster or alarm list.
//!
//! A monotonic counter stamped into every snapshot settles it: the front end keeps the
//! highest `rev` it has seen and drops anything below it (`useSnapshot` in clappkit/web).
//! Only chess had this; the other three had the same race with no way to detect it.
//!
//! Stamp it in ONE place — the app's own `snapshot()` — so the response and the pushed
//! event carry the same number when they describe the same moment.

use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

static REV: AtomicU64 = AtomicU64::new(0);

/// The next snapshot revision. Process-wide and monotonic: a clapp has one state, so one
/// counter orders every view of it. Starts at 1, so a snapshot that somehow lost its stamp
/// (deserialised as 0) always loses to a real one.
pub fn next_rev() -> u64 {
    REV.fetch_add(1, Ordering::Relaxed) + 1
}

/// Stamp a fresh `rev` into a snapshot object and return it. A non-object (an error
/// response, a bare string) is left untouched — there is nothing to order.
pub fn stamp_rev(snapshot: &mut Value) -> u64 {
    let rev = next_rev();
    if let Some(obj) = snapshot.as_object_mut() {
        obj.insert("rev".into(), Value::from(rev));
    }
    rev
}

/// [`stamp_rev`] for the common `json!({ … })` tail position:
/// `clappkit::snapshot::with_rev(json!({ "ok": true, … }))`.
pub fn with_rev(mut snapshot: Value) -> Value {
    stamp_rev(&mut snapshot);
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn revisions_only_ever_go_up() {
        let a = next_rev();
        let b = next_rev();
        assert!(b > a, "{b} must follow {a}");
        assert!(a > 0, "0 is reserved for an unstamped snapshot");
    }

    #[test]
    fn an_object_is_stamped_and_the_number_matches_what_was_returned() {
        let mut snap = json!({ "ok": true, "alarms": [] });
        let rev = stamp_rev(&mut snap);
        assert_eq!(snap["rev"], rev);
        assert_eq!(snap["ok"], true, "stamping must not disturb the payload");
    }

    #[test]
    fn a_later_snapshot_always_outranks_an_earlier_one() {
        // The exact comparison the front end makes to drop a stale invoke response.
        let first = with_rev(json!({ "ok": true }));
        let second = with_rev(json!({ "ok": true }));
        assert!(second["rev"].as_u64() > first["rev"].as_u64());
    }

    #[test]
    fn a_non_object_is_left_alone() {
        let mut v = json!("bye");
        stamp_rev(&mut v);
        assert_eq!(v, json!("bye"));
        assert_eq!(with_rev(json!([1, 2])), json!([1, 2]));
    }

    #[test]
    fn an_existing_rev_is_replaced_not_duplicated() {
        let mut v = json!({ "rev": 7, "ok": true });
        let rev = stamp_rev(&mut v);
        assert_eq!(v["rev"], rev);
        assert_ne!(v["rev"], 7);
    }
}
