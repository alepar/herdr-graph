//! Manager-chain cycle guard (hg-k4l.21): refuses a plan whose pointer effects would leave a manager chain cyclic.
//!
//! Maps the plan's effects to `PointerEdit`s and the seats whose effective manager may change, applies the
//! edits to `Pointers` in effect order and asks `cycle_from` about those seats:
//!
//! | effect                          | edit                         | subjects                              |
//! |---------------------------------|------------------------------|---------------------------------------|
//! | `seat.create`                   | `Live` in detail.teamspace   | the seat                              |
//! | `seat.resurrect`                | un-retire (keeps `manager`)  | the seat                              |
//! | `seat.manager`                  | `SeatManager`                | the seat                              |
//! | `teamspace.default_manager`     | `DefaultManager`             | every live seat of that teamspace, and the new default manager (a cycle the edit creates passes through it, retired seats included) |
//! | `graph.node_manager_seat`       | `NodeManager`                | every live seat                       |
//! | `seat.retire`                   | `Retire`                     | none                                  |
//!
//! `teamspace.resurrect` maps to nothing: its seats arrive as `seat.resurrect` effects. No kind moves a seat
//! between teamspaces today; a future move kind must emit an effect this table maps.
//! Plans with no pointer-relevant effect return before the tree is read.
use super::{GRAPH_NODE_MANAGER, SEAT_MANAGER, TEAMSPACE_DEFAULT_MANAGER};
use crate::model::manager::{PointerEdit, Pointers};
use crate::model::{SeatId, TeamspaceId};
use crate::plan::kind::PlanError;
use crate::plan::types::PlanEffect;
use crate::store::tree::TreeRead;
use std::collections::BTreeSet;

const SEAT_CREATE: &str = "seat.create";
const SEAT_RESURRECT: &str = "seat.resurrect";
const SEAT_RETIRE: &str = "seat.retire";

fn is_pointer_relevant(e: &PlanEffect) -> bool {
    matches!(
        e.kind.as_str(),
        SEAT_CREATE
            | SEAT_RESURRECT
            | SEAT_RETIRE
            | SEAT_MANAGER
            | TEAMSPACE_DEFAULT_MANAGER
            | GRAPH_NODE_MANAGER
    )
}

fn malformed(e: &PlanEffect, what: &str) -> PlanError {
    PlanError::Invalid(format!("{} {}: malformed {what}", e.kind, e.object))
}

/// `detail[key]` as a typed value; absent or null is `None`.
fn detail<T: serde::de::DeserializeOwned>(
    e: &PlanEffect,
    key: &str,
) -> Result<Option<T>, PlanError> {
    match e.detail.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|_| malformed(e, key)),
    }
}

fn seat_of(e: &PlanEffect) -> Result<SeatId, PlanError> {
    SeatId::try_from(e.object.clone()).map_err(|_| malformed(e, "object"))
}

pub fn refuse_cyclic_chains(tree: &dyn TreeRead, effects: &[PlanEffect]) -> Result<(), PlanError> {
    if !effects.iter().any(is_pointer_relevant) {
        return Ok(());
    }
    let mut p = Pointers::load(tree)?;
    let mut subjects: BTreeSet<SeatId> = BTreeSet::new();
    let mut all_live = false;
    // New default managers: walked after the live seats, so existing walk-order messages stay the same.
    let mut new_defaults: BTreeSet<SeatId> = BTreeSet::new();
    let mut teamspaces: BTreeSet<TeamspaceId> = BTreeSet::new();
    for e in effects {
        match e.kind.as_str() {
            SEAT_CREATE => {
                let seat = seat_of(e)?;
                let name: String = detail(e, "name")?.ok_or_else(|| malformed(e, "name"))?;
                let teamspace: TeamspaceId =
                    detail(e, "teamspace")?.ok_or_else(|| malformed(e, "teamspace"))?;
                p.apply(&PointerEdit::Live {
                    seat: seat.clone(),
                    name,
                    teamspace,
                });
                subjects.insert(seat);
            }
            SEAT_RESURRECT => {
                // Back to live with its recorded teamspace, name and explicit manager.
                let seat = seat_of(e)?;
                if let Some(s) = p.seats.get_mut(&seat) {
                    s.retired = false;
                }
                subjects.insert(seat);
            }
            SEAT_MANAGER => {
                let seat = seat_of(e)?;
                p.apply(&PointerEdit::SeatManager {
                    seat: seat.clone(),
                    manager: detail(e, "to")?,
                });
                subjects.insert(seat);
            }
            TEAMSPACE_DEFAULT_MANAGER => {
                let teamspace =
                    TeamspaceId::try_from(e.object.clone()).map_err(|_| malformed(e, "object"))?;
                let to: Option<SeatId> = detail(e, "to")?;
                p.apply(&PointerEdit::DefaultManager {
                    teamspace: teamspace.clone(),
                    manager: to.clone(),
                });
                teamspaces.insert(teamspace);
                new_defaults.extend(to);
            }
            GRAPH_NODE_MANAGER => {
                p.apply(&PointerEdit::NodeManager(detail(e, "to")?));
                all_live = true;
            }
            SEAT_RETIRE => p.apply(&PointerEdit::Retire(seat_of(e)?)),
            _ => {}
        }
    }
    // Seats whose effective manager moved through a shared pointer: judged after every edit has landed.
    subjects.extend(
        p.seats
            .iter()
            .filter(|(_, s)| !s.retired && (all_live || teamspaces.contains(&s.teamspace)))
            .map(|(id, _)| id.clone()),
    );
    let subjects: Vec<SeatId> = subjects.into_iter().collect();
    let new_defaults: Vec<SeatId> = new_defaults.into_iter().collect();
    match p
        .cycle_from(&subjects)
        .or_else(|| p.cycle_from(&new_defaults))
    {
        None => Ok(()),
        Some(cycle) => {
            let names: Vec<String> = cycle
                .iter()
                .map(|id| format!("{} ({id})", p.name(id).unwrap_or("?")))
                .collect();
            Err(PlanError::Invalid(format!(
                "this change would make a manager cycle: {}",
                names.join(" -> ")
            )))
        }
    }
}
