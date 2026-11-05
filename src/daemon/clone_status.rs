//! The `clones` status component: clone facts an operator may need to act on.
use crate::journal::Journal;
use crate::model::clone::SeatIntent;
use crate::model::common::CloneLifecycle;
use crate::reconcile::planner::resume_blocked_key;
use crate::store::layout;
use crate::store::tree::TreeRead;
use serde_json::json;

/// `{"retired_unpurged": [{clone, name, seat, retired_at}]}`: retired clones still inside their undo window
/// (purge tombstones are inert and never listed). `seat` is the threads seat the clone holds, if any.
pub fn clone_status(tree: &dyn TreeRead) -> serde_json::Value {
    let retired_unpurged: Vec<_> = layout::all_clones(tree)
        .unwrap_or_default()
        .into_iter()
        .map(|(_, c)| c)
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
    json!({ "retired_unpurged": retired_unpurged })
}

/// `clone_status` plus the resume driver's two stuck lists (spec §9, §14.4):
/// `"stuck_restores": [{clone, epoch, remedy}]` for a Restore whose one launch was already attempted, and
/// `"resume_blocked": [{clone, pane, remedy}]` for a clone whose last pane no longer exists after a restart.
pub fn clone_status_with_journal(tree: &dyn TreeRead, journal: &Journal) -> serde_json::Value {
    let mut v = clone_status(tree);
    let clones: Vec<_> = layout::all_clones(tree)
        .unwrap_or_default()
        .into_iter()
        .map(|(_, c)| c)
        .filter(|c| c.lifecycle == CloneLifecycle::Active)
        .collect();
    let stuck_restores: Vec<_> = clones
        .iter()
        .filter_map(|c| match &c.seat_intent {
            Some(SeatIntent::Restore {
                epoch,
                launched: true,
                ..
            }) => Some(json!({
                "clone": c.id.to_string(),
                "epoch": epoch.to_string(),
                "remedy": "start the agent in the pane manually or retire the clone",
            })),
            _ => None,
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
