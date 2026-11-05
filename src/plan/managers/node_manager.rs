//! `NodeManagerSet` kind (hg-k4l.6): `node-manager set <seat>` assigns the node-manager role, or moves it from
//! the current holder, rewriting every pointer that named the old one (spec §4.7, second bullet).
use super::{
    apply_pointer_effects, default_manager_effect, graph_blob, graph_relied_on,
    node_manager_effect, seat_manager_effect,
};
use crate::daemon::registry::CallerInfo;
use crate::model::change::{ReliedOn, RequestKind, Version};
use crate::model::common::Lifecycle;
use crate::model::manager::Pointers;
use crate::model::{AnyId, SeatId};
use crate::plan::core_kinds::{SeatActivate, mm, parse_args, read_seat};
use crate::plan::grammar::{self, Scope, positional};
use crate::plan::kind::{KindRegistry, OrgKind, PlanBody, PlanCx, PlanError};
use crate::plan::types::{Plan, PlanEffect, Reserved};
use crate::store::layout;
use crate::writer::{Applied, MutationCx, MutationError};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub(super) fn register(reg: &mut KindRegistry) {
    reg.register(Arc::new(NodeManagerSet));
}

#[derive(Deserialize)]
struct Args {
    seat: String,
}

struct NodeManagerSet;

fn activate_args(target: &SeatId) -> serde_json::Value {
    json!({ "seat": target.to_string() })
}

fn rev(object: impl Into<AnyId>, rev: u64) -> ReliedOn {
    ReliedOn {
        object: object.into(),
        version: Version::Rev(rev),
    }
}

impl OrgKind for NodeManagerSet {
    fn kind(&self) -> RequestKind {
        RequestKind::NodeManagerSet
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("node-manager", "set")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let seat = positional(words, 0)
            .ok_or_else(|| PlanError::Usage("node-manager set <seat>".into()))?;
        Ok(json!({ "seat": seat }))
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: Args = parse_args(args)?;
        let tree = cx.tree;
        let retired = |id: &SeatId| {
            PlanError::Invalid(format!(
                "seat {id} is retired; the node manager must be a live seat"
            ))
        };
        let target = match grammar::resolve_seat(tree, &a.seat, Scope::Live) {
            Ok(id) => id,
            Err(e) => {
                return Err(match grammar::resolve_seat(tree, &a.seat, Scope::Retired) {
                    Ok(id) => retired(&id),
                    Err(_) => e,
                });
            }
        };
        let t = read_seat(tree, &target)?;
        if t.rec.lifecycle == Lifecycle::Retired {
            return Err(retired(&target));
        }
        let pointers = Pointers::load(tree)?;
        let recorded = pointers.node_manager_seat.clone();
        if pointers.live_node_manager().as_ref() == Some(&target) {
            return Err(PlanError::Invalid(format!(
                "seat {} ({target}) is already the node manager; nothing to change",
                t.rec.name
            )));
        }

        // C3: the node manager must be reachable, so the seat and its teamspace end up Active.
        let mut effects = Vec::new();
        let mut relied_on = Vec::new();
        match t.rec.lifecycle {
            Lifecycle::Dormant => {
                let act = SeatActivate.plan(cx, &activate_args(&target), reserved)?;
                effects.extend(act.effects);
                relied_on.extend(act.relied_on);
            }
            _ if t.ts.rec.lifecycle != Lifecycle::Active => {
                return Err(PlanError::Invalid(format!(
                    "seat {} is active but its teamspace {} is not; activate the teamspace first",
                    t.rec.name, t.ts.rec.name
                )));
            }
            _ => {}
        }
        relied_on.push(rev(target.clone(), t.rec.rev));
        relied_on.push(graph_relied_on(tree)?);

        effects.push(node_manager_effect(
            &layout::graph_id(tree)?,
            recorded.as_ref(),
            Some(&target),
            &graph_blob(tree)?,
        ));

        let mut rewritten = |effect: PlanEffect, object: AnyId, record_rev: u64| {
            effects.push(effect);
            relied_on.push(rev(object, record_rev));
        };
        if let Some(old) = &recorded {
            for (_, s) in layout::all_seats(tree)? {
                if s.lifecycle == Lifecycle::Retired || s.id == target {
                    continue;
                }
                if s.manager.as_ref() == Some(old) {
                    rewritten(
                        seat_manager_effect(&s.id, Some(old), Some(&target)),
                        s.id.to_any(),
                        s.rev,
                    );
                } else if &s.id == old && s.manager.as_ref() != Some(&target) {
                    // The live old node manager reports to the new one.
                    rewritten(
                        seat_manager_effect(&s.id, s.manager.as_ref(), Some(&target)),
                        s.id.to_any(),
                        s.rev,
                    );
                }
            }
            for (_, ts) in layout::list_teamspaces(tree)? {
                if ts.lifecycle != Lifecycle::Retired && ts.default_manager.as_ref() == Some(old) {
                    rewritten(
                        default_manager_effect(&ts.id, Some(old), Some(&target)),
                        ts.id.to_any(),
                        ts.rev,
                    );
                }
            }
        }
        if let Some(m) = &t.rec.manager {
            // The node manager has no manager; the target's rev is already relied on.
            effects.push(seat_manager_effect(&target, Some(m), None));
        }

        let mut seen = Vec::new();
        relied_on.retain(|r| {
            let new = !seen.contains(&r.object);
            seen.push(r.object.clone());
            new
        });
        Ok(PlanBody {
            effects,
            relied_on,
            warnings: vec![],
            repair_required: None,
            summary: format!("set node manager {}", t.rec.name),
        })
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: Args = parse_args(args).map_err(mm)?;
        let target = grammar::resolve_seat(&cx.tree, &a.seat, Scope::Live).map_err(mm)?;
        if plan
            .effects
            .iter()
            .any(|e| e.kind == "seat.activate" && e.object == target.to_any())
        {
            SeatActivate.mutate(cx, &activate_args(&target), plan)?;
        }
        apply_pointer_effects(&mut cx.tree, &plan.effects)?;
        let name = read_seat(&cx.tree, &target).map_err(mm)?.rec.name;
        Ok(Applied {
            summary: format!("set node manager {name}"),
            action: None,
        })
    }
}
