//! The `clones` status component: clone facts an operator may need to act on.
use crate::journal::Journal;
use crate::model::clone::{CloneRecord, LEGACY_LAUNCHED_AT, SeatFactState, SeatIntent};
use crate::model::common::{CloneLifecycle, Timestamp};
use crate::observe::seat_observation::SEAT_LOST_AFTER;
use crate::reconcile::planner::{
    incarnation_changed_at, rebind_refused_in, restore_mark_failed_key, restore_stuck_key,
    resume_blocked_key,
};
use crate::store::layout;
use crate::store::tree::TreeRead;
use crate::threads::effects::refused_meta;
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

/// One row per active seated clone whose seat is still held after a Herdr restart: the fact graph holds for the
/// seat is a fresh `Unresolved` read (read in the current incarnation) and one grace period has passed since the
/// incarnation began. `SEAT_LOST_AFTER` is that grace (documented as equal to the 90 s relaunch grace). It does
/// not depend on what the rebind did: refused (reason recorded), errored or never attempted. The row clears when
/// the fact reads anything else.
fn restart_held(
    clones: &[CloneRecord],
    journal: &Journal,
    now: Timestamp,
) -> Vec<serde_json::Value> {
    let Some(changed_at) = incarnation_changed_at(journal) else {
        return Vec::new();
    };
    if now < changed_at + SEAT_LOST_AFTER {
        return Vec::new();
    }
    clones
        .iter()
        .filter_map(|c| {
            let seat = c.threads_seat.as_ref()?;
            let fact = c.seat_fact.as_ref()?;
            if fact.seat != *seat
                || fact.state != SeatFactState::Unresolved
                || fact.read_at < changed_at
            {
                return None;
            }
            let pane = c
                .runtime
                .bound
                .as_ref()
                .and_then(|b| b.pane_id.as_ref())
                .map_or("unknown", |p| p.0.as_str());
            let reason = match rebind_refused_in(journal, &c.id, changed_at) {
                Some(r) => format!("rebind refused: {r}"),
                None => "rebind did not complete".to_owned(),
            };
            Some(json!({
                "clone": c.id.to_string(),
                "name": c.name,
                "seat": seat,
                "pane": pane,
                "reason": reason,
                "since": changed_at.to_rfc3339(),
                "remedy": format!("herdr-threads seat rebind {seat} --pane {pane} --operator"),
            }))
        })
        .collect()
}

/// `clone_status` plus the resume driver's two stuck lists (spec §9, §14.4, §14.7):
/// `"stuck_restores": [{clone, epoch, reason, remedy, last_refusal?}]` for every pending Restore that cannot
/// make progress (`launched_no_reattach`, `mark_failed`, `not_launchable`, `threads_pending`), and
/// `"resume_blocked": [{clone, pane, remedy}]` for a clone whose last pane no longer exists after a restart.
pub fn clone_status_with_journal(tree: &dyn TreeRead, journal: &Journal) -> serde_json::Value {
    clone_status_at(tree, journal, chrono::Utc::now())
}

/// `clone_status_with_journal` at an explicit `now` (the held rows are time-dependent).
/// Adds `"restart_held": [{clone, name, seat, pane, reason, since, remedy}]` (see `restart_held`). Also `"resurrect_held": [{clone, name, seat, pane, reason, since,
/// remedy}]`: a pending Restore whose resurrect threads refused definitively (journal meta of the Restore's
/// epoch) and whose seat still reads `Retired{Reversible}`, once `SEAT_LOST_AFTER` has passed since the first
/// refusal. It clears when the fact leaves that state, the Restore is acknowledged or the epoch moves on.
pub fn clone_status_at(
    tree: &dyn TreeRead,
    journal: &Journal,
    now: Timestamp,
) -> serde_json::Value {
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
                    "start the agent in the pane manually (fix the pane shell if it rejects the continuity-key export) or retire the clone",
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
    v["restart_held"] = json!(restart_held(&clones, journal, now));
    let resurrect_held: Vec<_> = clones
        .iter()
        .filter_map(|c| {
            let Some(SeatIntent::Restore {
                seat,
                epoch,
                threads_done: false,
                ..
            }) = &c.seat_intent
            else {
                return None;
            };
            let reversible = c.seat_fact.as_ref().is_some_and(|f| {
                &f.seat == seat
                    && matches!(f.state, SeatFactState::Retired { reversible: true, .. })
            });
            let m = refused_meta(journal, &c.id, epoch).filter(|_| reversible)?;
            let first_at = chrono::DateTime::parse_from_rfc3339(m["first_at"].as_str()?).ok()?;
            if now < first_at.with_timezone(&chrono::Utc) + SEAT_LOST_AFTER {
                return None;
            }
            let pane = m["pane"].as_str().unwrap_or("unknown");
            Some(json!({
                "clone": c.id.to_string(),
                "name": c.name,
                "seat": seat,
                "pane": pane,
                "reason": m["reason"],
                "since": m["first_at"],
                "remedy": format!(
                    "threads refused graph's resurrect; an operator rebind brings the seat back, and graph then acknowledges it: herdr-threads seat rebind {seat} --pane {pane} --operator"
                ),
            }))
        })
        .collect();
    v["resurrect_held"] = json!(resurrect_held);
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
            keyless: false,
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
            clone_with(
                "a",
                Some(SeatFactState::Retired {
                    reversible: false,
                    keyed: true,
                    retired_at: None,
                }),
            ),
            clone_with("b", None),
        ];
        assert!(key_conflicts(&clones).is_empty());
    }
}
