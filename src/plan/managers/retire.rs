//! Node-manager retirement guard (hg-k4l.8): `make_plan` refuses a plan whose effects, induced ones included,
//! retire, deactivate, move or close the live node manager, or all of its live clones. The node manager is the
//! root of every escalation chain and must always be open, so the role is moved first (`node-manager set`).
//!
//! `clone.rebind` re-points a clone at a pane and does not move the seat; it is not a placement change and stays
//! allowed (it is how a stale binding is repaired). No kind moves a seat between teamspaces today; one that does
//! must emit an effect this table refuses.
use super::NODE_MANAGER_SET_USAGE;
use crate::model::common::CloneLifecycle;
use crate::model::manager::live_node_manager;
use crate::model::{AnyId, CloneId};
use crate::plan::kind::PlanError;
use crate::plan::types::PlanEffect;
use crate::store::layout;
use crate::store::tree::TreeRead;
use std::collections::BTreeSet;

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
            ("seat.deactivate", o) if *o == seat => return refuse("deactivated"),
            ("runtime.close_tab", o) if *o == seat => return refuse("moved or closed"),
            ("teamspace.retire" | "runtime.close_workspace", o) if *o == ts => {
                return refuse("moved or closed");
            }
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
