//! The writer `Mutation` registered for every organizational kind: confirmation check, recompute,
//! exact-effects staleness, then the kind's own mutation (spec §3.3).
use super::hash::{plan_hash, same_effects};
use super::kind::{KindRegistry, PlanCx, PlanError, make_plan};
use super::store::PlanStore;
use super::types::{PlanEffect, StoredPlan};
use crate::model::PlanId;
use crate::model::operation::Confirmation;
use crate::writer::{Applied, Mutation, MutationCx, MutationError, Reject};
use std::path::PathBuf;
use std::sync::Arc;

pub struct OrgMutation {
    pub kinds: Arc<KindRegistry>,
    pub plans: Arc<PlanStore>,
    pub instance: PathBuf,
}

fn reject(reason: &str, explanation: String) -> MutationError {
    MutationError::Reject(Reject { reason: reason.into(), explanation, current_revs: vec![] })
}

/// `-`/`+` lines for the effects only one side has.
fn effect_diff(confirmed: &[PlanEffect], recomputed: &[PlanEffect]) -> String {
    let mut parts = Vec::new();
    for e in confirmed.iter().filter(|e| !recomputed.contains(e)) {
        parts.push(format!("- {}", e.describe()));
    }
    for e in recomputed.iter().filter(|e| !confirmed.contains(e)) {
        parts.push(format!("+ {}", e.describe()));
    }
    parts.join("; ")
}

/// The request args without the `_plan` / `_confirmation` envelope keys.
fn kind_args(args: &serde_json::Value) -> serde_json::Value {
    match args {
        serde_json::Value::Object(m) => {
            serde_json::Value::Object(m.iter().filter(|(k, _)| !k.starts_with('_')).map(|(k, v)| (k.clone(), v.clone())).collect())
        }
        other => other.clone(),
    }
}

impl Mutation for OrgMutation {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let req = cx.request;
        let kind = self
            .kinds
            .get(req.kind)
            .ok_or_else(|| MutationError::Bug(format!("no organizational kind registered for {:?}", req.kind)))?;

        // 1. the confirmed plan
        let plan_id = req
            .args
            .get("_plan")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<PlanId>().ok())
            .ok_or_else(|| reject("unknown_plan", "request carries no valid plan id".into()))?;
        let stored = self
            .plans
            .get(&plan_id)
            .map_err(|e| MutationError::Store(e.into()))?
            .ok_or_else(|| reject("unknown_plan", format!("plan {plan_id} is not in the plan store")))?;

        // 2. the confirmation names exactly this plan
        let confirmation: Option<Confirmation> =
            req.args.get("_confirmation").and_then(|v| serde_json::from_value(v.clone()).ok());
        match confirmation {
            Some(c) if c.plan_hash == stored.hash => {}
            _ => {
                return Err(reject(
                    "confirmation_mismatch",
                    format!("confirmation does not match plan {plan_id} (hash {})", stored.hash),
                ));
            }
        }
        let args = kind_args(&req.args);
        if args != stored.plan.request.args || req.kind != stored.plan.request.kind {
            return Err(reject("confirmation_mismatch", format!("request does not match plan {plan_id}")));
        }

        // 3. recompute against the committed revision this apply runs on
        let at = cx.tree.base().clone();
        let pcx = PlanCx { tree: &cx.tree, at, caller: &stored.caller, now: cx.now, instance: &self.instance };
        let fresh = match make_plan(&*kind, &pcx, args.clone(), stored.plan.reserved.clone()) {
            Ok(p) => p,
            Err(PlanError::Store(e)) => return Err(MutationError::Store(e)),
            // The request no longer makes sense against the committed state (e.g. a name it used no longer
            // resolves): the confirmed effects cannot run, and no replacement plan can be computed.
            Err(e) => {
                return Err(reject(
                    "stale_plan",
                    format!("plan {plan_id} is stale: {e}; no replacement plan could be computed, plan again"),
                ));
            }
        };

        if let Some(why) = &fresh.repair_required {
            return Err(reject("repair_required", format!("plan {plan_id} cannot apply: {why}")));
        }

        // 4. only the confirmed effect set may run
        if !same_effects(&stored.plan.effects, &fresh.effects) {
            let new_hash = plan_hash(&fresh);
            let replacement = StoredPlan {
                hash: new_hash.clone(),
                plan: fresh,
                created_at: cx.now,
                caller: stored.caller.clone(),
                requester: stored.requester.clone(),
                supersedes: stored.supersedes.clone(),
            };
            let explanation = format!(
                "plan {plan_id} is stale: effects changed ({}); new plan {} (hash {new_hash}) needs confirmation",
                effect_diff(&stored.plan.effects, &replacement.plan.effects),
                replacement.plan.id
            );
            self.plans.put(&replacement).map_err(|e| MutationError::Store(e.into()))?;
            return Err(reject("stale_plan", explanation));
        }

        // 5. perform the confirmed change
        kind.mutate(cx, &args, &stored.plan)
    }
}
