//! Node-manager retirement guard (hg-k4l.8): `make_plan` refuses a plan whose effects, induced ones included,
//! retire, deactivate, move or close the live node manager, or all of its live clones. The node manager is the
//! root of every escalation chain and must always be open, so the role is moved first (`node-manager set`).
//! The one exception is a release (`seat release`, `teamspace release`, undo of an adoption) that takes the
//! node manager together with every other live seat: the graph's last seat can go, and the release clears
//! graph.toml `node_manager_seat` back to unassigned (`release_clearing`).
//!
//! `clone.rebind` re-points a clone at a pane and does not move the seat; it is not a placement change and stays
//! allowed (it is how a stale binding is repaired). No kind moves a seat between teamspaces today; one that does
//! must emit an effect this table refuses.
use super::{
    GRAPH_NODE_MANAGER, NODE_MANAGER_SET_USAGE, graph_blob, graph_relied_on, node_manager_effect,
};
use crate::model::change::ReliedOn;
use crate::model::common::{CloneLifecycle, Lifecycle};
use crate::model::manager::live_node_manager;
use crate::model::{AnyId, CloneId, SeatId};
use crate::plan::kind::PlanError;
use crate::plan::types::PlanEffect;
use crate::ports::store::StoreError;
use crate::store::layout;
use crate::store::tree::TreeRead;
use std::collections::BTreeSet;

/// A release of `releasing` that takes the live node manager with it is allowed only when no other seat is
/// live: then it clears graph.toml `node_manager_seat` back to unassigned. Returns the clearing effect and its
/// graph.toml relied_on, or None when the release does not touch the node manager or other seats are live.
pub fn release_clearing(
    tree: &dyn TreeRead,
    releasing: &[SeatId],
) -> Result<Option<(PlanEffect, ReliedOn)>, StoreError> {
    let Some(nm) = live_node_manager(tree)? else {
        return Ok(None);
    };
    if !releasing.contains(&nm) {
        return Ok(None);
    }
    let others_live = layout::all_seats(tree)?
        .iter()
        .any(|(_, s)| s.lifecycle != Lifecycle::Retired && !releasing.contains(&s.id));
    if others_live {
        return Ok(None);
    }
    let effect = node_manager_effect(&layout::graph_id(tree)?, Some(&nm), None, &graph_blob(tree)?);
    Ok(Some((effect, graph_relied_on(tree)?)))
}

pub fn refuse_node_manager_retirement(
    tree: &dyn TreeRead,
    effects: &[PlanEffect],
) -> Result<(), PlanError> {
    let Some(nm) = live_node_manager(tree)? else {
        return Ok(());
    };
    let (loc, rec) = layout::all_seats(tree)?
        .into_iter()
        .find(|(_, s)| s.id == nm)
        .ok_or_else(|| PlanError::Invalid(format!("node manager {nm} has no seat record")))?;
    let live: BTreeSet<CloneId> = layout::list_clones(tree, &loc.folder)?
        .into_iter()
        .filter(|(_, c)| c.lifecycle != CloneLifecycle::Retired)
        .map(|(_, c)| c.id)
        .collect();
    let (seat, ts): (AnyId, AnyId) = (nm.to_any(), rec.teamspace.to_any());
    let refuse = |what: &str| {
        Err(PlanError::Invalid(format!(
            "node manager {} ({nm}) cannot be {what}; move the role first with {NODE_MANAGER_SET_USAGE}",
            rec.name
        )))
    };
    let refuse_release = || {
        Err(PlanError::Invalid(format!(
            "node manager {} ({nm}) cannot be released while other seats are live; move the role first with {NODE_MANAGER_SET_USAGE}",
            rec.name
        )))
    };
    // A release is clean only when it clears the role and takes every live seat with it.
    let released: BTreeSet<SeatId> = effects
        .iter()
        .filter(|e| e.kind == "seat.release")
        .filter_map(|e| SeatId::try_from(e.object.clone()).ok())
        .collect();
    let clears = effects.iter().any(|e| {
        e.kind == GRAPH_NODE_MANAGER
            && e.detail.get("from").and_then(|v| v.as_str()) == Some(nm.as_str())
            && e.detail.get("to").is_some_and(|v| v.is_null())
    });
    let clean_release = clears
        && layout::all_seats(tree)?
            .iter()
            .all(|(_, s)| s.lifecycle == Lifecycle::Retired || released.contains(&s.id));
    let covers_all_clones = |kind: &str| {
        let hit: BTreeSet<CloneId> = effects
            .iter()
            .filter(|e| e.kind == kind)
            .filter_map(|e| CloneId::try_from(e.object.clone()).ok())
            .collect();
        !live.is_empty() && live.is_subset(&hit)
    };
    for e in effects {
        match (e.kind.as_str(), &e.object) {
            ("seat.retire", o) if *o == seat => return refuse("retired"),
            ("seat.release", o) if *o == seat && !clean_release => return refuse_release(),
            ("seat.deactivate", o) if *o == seat => return refuse("deactivated"),
            ("runtime.close_tab", o) if *o == seat => return refuse("moved or closed"),
            ("teamspace.retire" | "runtime.close_workspace", o) if *o == ts => {
                return refuse("moved or closed");
            }
            ("teamspace.release", o) if *o == ts && !clean_release => return refuse_release(),
            _ => {}
        }
    }
    if covers_all_clones("clone.retire") {
        return refuse("retired");
    }
    if covers_all_clones("runtime.close_pane") {
        return refuse("deactivated");
    }
    Ok(())
}
