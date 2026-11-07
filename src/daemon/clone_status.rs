//! The `clones` status component: clone facts an operator may need to act on.
use crate::journal::Journal;
use crate::model::clone::{CloneRecord, LEGACY_LAUNCHED_AT, SeatFactState, SeatIntent};
use crate::model::common::CloneLifecycle;
use crate::reconcile::planner::{restore_mark_failed_key, restore_stuck_key, resume_blocked_key};
use crate::store::layout;
use crate::store::tree::TreeRead;
use serde_json::json;

/// `{"retired_unpurged": [{clone, name, seat, retired_at}], "key_conflicts": [{clone, name, pane, remedy}]}`:
/// retired clones still inside their undo window (purge tombstones are inert and never listed; `seat` is the
/// threads seat the clone holds, if any), and the key-conflict alerts (see `key_conflicts`).
pub fn clone_status(tree: &dyn TreeRead) -> serde_json::Value {
    let all: Vec<CloneRecord> = layout::all_clones(tree)
        .unwrap_or_default()
        .into_iter()
        .map(|(_, c)| c)
        .collect();
    let retired_unpurged: Vec<_> = all
        .iter()
        .filter(|c| c.lifecycle == CloneLifecycle::Retired && !c.is_tombstone())
        .map(|c| {
            json!({
                "clone": c.id.to_string(),
                "name": c.name,
                "seat": c.threads_seat,
                "retired_at": c.retired.as_ref().map(|r| r.at),
            })
        })
        .collect();
    json!({ "retired_unpurged": retired_unpurged, "key_conflicts": key_conflicts(&all) })
}

/// One row per non-tombstone clone whose committed seat fact is `KeyConflict` (spec §14.9): threads holds the
/// clone's continuity key against another pane, so the clone is Unknown and never launched. `pane` is the
/// conflicting pane (`unknown` when threads named none); the remedy is the operator's resolution.
fn key_conflicts(clones: &[CloneRecord]) -> Vec<serde_json::Value> {
    clones
        .iter()
        .filter(|c| !c.is_tombstone())
        .filter_map(|c| {
            let SeatFactState::KeyConflict { pane } = &c.seat_fact.as_ref()?.state else {
                return None;
            };
            let pane = pane.as_ref().map_or("unknown", |p| p.0.as_str());
            Some(json!({
                "clone": c.id.to_string(),
                "name": c.name,
                "pane": pane,
                "remedy": format!(
                    "close the stray pane {pane}, or retire/purge clone {}; threads clears the hold at the next check-in",
                    c.name
                ),
            }))
        })
        .collect()
}

/// `clone_status` plus the resume driver's two stuck lists (spec §9, §14.4, §14.7):
/// `"stuck_restores": [{clone, epoch, reason, remedy, last_refusal?}]` for every pending Restore that cannot
/// make progress (`launched_no_reattach`, `mark_failed`, `not_launchable`, `threads_pending`), and
/// `"resume_blocked": [{clone, pane, remedy}]` for a clone whose last pane no longer exists after a restart.
pub fn clone_status_with_journal(tree: &dyn TreeRead, journal: &Journal) -> serde_json::Value {
    let mut v = clone_status(tree);
    let clones: Vec<_> = layout::all_clones(tree)
        .unwrap_or_default()
        .into_iter()
        .map(|(_, c)| c)
        .filter(|c| c.lifecycle == CloneLifecycle::Active)
        .collect();
    let meta = |key: String| -> Option<serde_json::Value> {
        let raw = journal.meta_get(&key).ok().flatten()?;
        serde_json::from_str(&raw).ok()
    };
    let stuck_restores: Vec<_> = clones
        .iter()
        .filter_map(|c| {
            let Some(SeatIntent::Restore {
                epoch,
                threads_done,
                launched,
                last_refusal,
                ..
            }) = &c.seat_intent
            else {
                return None;
            };
            let mark_failed = meta(restore_mark_failed_key(&c.id))
                .filter(|m| m["epoch"] == epoch.to_string());
            let (reason, remedy, error) = if launched.is_some() {
                (
                    "launched_no_reattach",
                    "start the agent in the pane manually or retire the clone",
                    None,
                )
            } else if let Some(m) = mark_failed {
                (
                    "mark_failed",
                    "the launch mark could not be written; it is retried automatically; retire the clone to give up",
                    Some(m["error"].clone()),
                )
            } else if *threads_done && meta(restore_stuck_key(&c.id)).is_some() {
                (
                    "not_launchable",
                    "no resumable session; start an agent manually in the reopened pane or retire the clone",
                    None,
                )
            } else if !*threads_done && last_refusal.is_some() {
                (
                    "threads_pending",
                    "threads refused the restore step; it is retried; retire the clone to give up",
                    None,
                )
            } else {
                return None;
            };
            let mut entry = json!({
                "clone": c.id.to_string(),
                "epoch": epoch.to_string(),
                "reason": reason,
                "remedy": remedy,
            });
            if let Some(at) = launched.filter(|t| *t != LEGACY_LAUNCHED_AT) {
                entry["launched_at"] = json!(at.to_rfc3339());
            }
            if let Some(r) = last_refusal {
                entry["last_refusal"] = json!(r);
            }
            if let Some(e) = error {
                entry["error"] = e;
            }
            Some(entry)
        })
        .collect();
    let resume_blocked: Vec<_> = clones
        .iter()
        .filter_map(|c| {
            let raw = journal
                .meta_get(&resume_blocked_key(&c.id))
                .ok()
                .flatten()?;
            let m: serde_json::Value = serde_json::from_str(&raw).ok()?;
            Some(json!({
                "clone": c.id.to_string(),
                "pane": m["pane"],
                "remedy": "retire the clone; its last pane no longer exists",
            }))
        })
        .collect();
    v["stuck_restores"] = json!(stuck_restores);
    v["resume_blocked"] = json!(resume_blocked);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::clone::SeatFact;
    use crate::model::common::{CloneLifecycle, Timestamp};
    use crate::model::{CloneId, HerdrPaneId, SeatId};

    fn clone_with(name: &str, state: Option<SeatFactState>) -> CloneRecord {
        CloneRecord {
            schema: crate::model::SCHEMA_VERSION,
            id: CloneId::new(),
            rev: 1,
            seat: SeatId::new(),
            name: name.into(),
            name_history: vec![],
            lifecycle: CloneLifecycle::Active,
            retired: None,
            runtime: Default::default(),
            occupant: None,
            sessions: vec![],
            opt_outs: vec![],
            invitations: vec![],
            reload_required: false,
            threads_seat: Some("seat-1".into()),
            threads_seat_history: vec![],
            seat_intent: None,
            seat_fact: state.map(|state| SeatFact {
                seat: "seat-1".into(),
                state,
                read_at: Timestamp::UNIX_EPOCH,
            }),
            key_reservation: None,
        }
    }

    fn conflict(pane: Option<&str>) -> Option<SeatFactState> {
        Some(SeatFactState::KeyConflict {
            pane: pane.map(|p| HerdrPaneId(p.into())),
        })
    }

    #[test]
    fn status_shows_the_key_conflict_alert() {
        let clean = clone_with("clean", Some(SeatFactState::Unresolved));
        let unread = clone_with("unread", None);
        let hit = clone_with("hit", conflict(Some("w1:p9")));
        let rows = key_conflicts(&[clean, unread, hit.clone()]);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0]["clone"], hit.id.to_string());
        assert_eq!(rows[0]["name"], "hit");
        assert_eq!(rows[0]["pane"], "w1:p9");
    }

    #[test]
    fn status_names_the_key_conflict_pane_and_remedy() {
        let rows = key_conflicts(&[clone_with("hit", conflict(Some("w1:p9")))]);
        assert_eq!(
            rows[0]["remedy"],
            "close the stray pane w1:p9, or retire/purge clone hit; threads clears the hold at the next check-in"
        );
        let rows = key_conflicts(&[clone_with("hit", conflict(None))]);
        assert_eq!(rows[0]["pane"], "unknown");
        assert_eq!(
            rows[0]["remedy"],
            "close the stray pane unknown, or retire/purge clone hit; threads clears the hold at the next check-in"
        );
    }

    #[test]
    fn key_conflicts_is_empty_without_a_conflict() {
        let clones = [
            clone_with("a", Some(SeatFactState::Retired { reversible: false })),
            clone_with("b", None),
        ];
        assert!(key_conflicts(&clones).is_empty());
    }
}
