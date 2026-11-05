//! Reparenting on seat retirement (spec §4.11). Apply side (`reparent`, from do_retire_seat; hg-k4l.26) and plan
//! preview (`reparent_effects`; hg-k4l.27). Retirement routes never rewrite pointers themselves.
use super::{set_default_manager, set_seat_manager};
use crate::model::manager::Pointers;
use crate::model::{AnyId, SeatId};
use crate::ports::store::StoreError;
use crate::store::{Overlay, layout};
use std::collections::BTreeSet;

/// Compensation key of the rewrite list in every retirement action record.
pub const REPARENTED_KEY: &str = "reparented";
/// Compensation key of the observed-cascade fallback notes.
pub const REPARENT_FALLBACK_KEY: &str = "reparent_fallback";

/// One pointer a retirement rewrote: `field` is "manager" (holder = seat) or "default_manager" (holder = teamspace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    pub holder: AnyId,
    pub field: String,
    pub old: SeatId,
    pub new: Option<SeatId>,
}

impl Rewrite {
    /// `{holder, field, old, new}`; `new` is omitted when None (the pointer was cleared).
    pub fn to_toml(&self) -> toml::Value {
        let mut t = toml::Table::new();
        t.insert(
            "holder".into(),
            toml::Value::String(self.holder.to_string()),
        );
        t.insert("field".into(), toml::Value::String(self.field.clone()));
        t.insert("old".into(), toml::Value::String(self.old.to_string()));
        if let Some(new) = &self.new {
            t.insert("new".into(), toml::Value::String(new.to_string()));
        }
        toml::Value::Table(t)
    }

    pub fn from_toml(v: &toml::Value) -> Option<Rewrite> {
        let t = v.as_table()?;
        let new = match t.get("new") {
            Some(n) => Some(SeatId::parse(n.as_str()?).ok()?),
            None => None,
        };
        Some(Rewrite {
            holder: AnyId::parse(t.get("holder")?.as_str()?).ok()?,
            field: t.get("field")?.as_str()?.to_string(),
            old: SeatId::parse(t.get("old")?.as_str()?).ok()?,
            new,
        })
    }
}

/// Apply side, from do_retire_seat before the seat is archived: rewrite every live seat whose explicit manager is
/// `seat` and every non-retired teamspace whose default_manager is `seat` to `seat`'s nearest surviving manager.
/// Seats retired earlier in the same mutation are already retired in the overlay, so they are transparent; seats
/// retiring later in the same mutation are rewritten again when their turn comes, so final pointers match the preview.
/// `lenient`: an integrity error computing the target falls back to `live_node_manager` (or None) and returns a note;
/// not lenient, the error propagates (StoreError::Integrity → Fail::Data per hg-k4l.5).
pub fn reparent(
    tree: &mut Overlay<'_>,
    seat: &SeatId,
    lenient: bool,
) -> Result<(Vec<Rewrite>, Option<String>), StoreError> {
    let pointers = Pointers::load(&*tree)?;
    let retiring = BTreeSet::from([seat.clone()]);
    let (target, note) = match pointers.nearest_surviving_manager(seat, &retiring) {
        Ok(t) => (t, None),
        Err(e @ StoreError::Integrity { .. }) if lenient => {
            let t = pointers.live_node_manager();
            let note = format!(
                "reparenting reports of {seat}: {e}; fell back to {}",
                t.as_ref()
                    .map_or("no manager".to_string(), |t| format!("node manager {t}"))
            );
            (t, Some(note))
        }
        Err(e) => return Err(e),
    };
    let mut seats: Vec<SeatId> = pointers
        .seats
        .iter()
        .filter(|(_, p)| !p.retired && p.manager.as_ref() == Some(seat))
        .map(|(id, _)| id.clone())
        .collect();
    seats.sort();
    let mut teamspaces = Vec::new();
    for (_, ts) in layout::list_teamspaces(&*tree)? {
        if ts.lifecycle != crate::model::common::Lifecycle::Retired
            && ts.default_manager.as_ref() == Some(seat)
        {
            teamspaces.push(ts.id);
        }
    }
    teamspaces.sort();
    let mut out = Vec::new();
    for id in seats {
        // The node manager has no manager; never point a seat at itself.
        let new = target.clone().filter(|t| *t != id);
        set_seat_manager(tree, &id, new.clone())?;
        out.push(Rewrite {
            holder: id.to_any(),
            field: "manager".into(),
            old: seat.clone(),
            new,
        });
    }
    for id in teamspaces {
        set_default_manager(tree, &id, target.clone())?;
        out.push(Rewrite {
            holder: id.to_any(),
            field: "default_manager".into(),
            old: seat.clone(),
            new: target.clone(),
        });
    }
    Ok((out, note))
}

#[cfg(test)]
pub(crate) mod testing {
    use crate::model::change::{ChangeRequest, RequestKind, Requester};
    use crate::model::{SeatId, TeamspaceId};
    use crate::writer::{Applied, Mutation, MutationCx, MutationError};
    use serde_json::json;

    /// Registry key of `SetPointer`; fixtures add `reg.register(SET_POINTER_KEY, Arc::new(SetPointer))`.
    pub const SET_POINTER_KEY: &str = "bookkeeping.test_pointer";

    /// Test-only mutation: args `{"seat": "<st>"}` or `{"teamspace": "<ts>"}`, plus `"manager": "<st>"|null`; writes
    /// through `super::super::set_seat_manager` / `set_default_manager`. Writes any id, dangling ones included.
    pub struct SetPointer;
    impl Mutation for SetPointer {
        fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
            let bug = |what: &str, e: &dyn std::fmt::Display| {
                MutationError::Bug(format!("test_pointer {what}: {e}"))
            };
            let manager: Option<SeatId> = match cx.request.args["manager"].as_str() {
                Some(s) => Some(s.parse().map_err(|e| bug("manager", &e))?),
                None => None,
            };
            if let Some(s) = cx.request.args["seat"].as_str() {
                let seat: SeatId = s.parse().map_err(|e| bug("seat", &e))?;
                super::super::set_seat_manager(&mut cx.tree, &seat, manager)?;
            } else if let Some(s) = cx.request.args["teamspace"].as_str() {
                let ts: TeamspaceId = s.parse().map_err(|e| bug("teamspace", &e))?;
                super::super::set_default_manager(&mut cx.tree, &ts, manager)?;
            } else {
                return Err(MutationError::Bug("test_pointer: no target".into()));
            }
            Ok(Applied {
                summary: "set pointer".into(),
                action: None,
            })
        }
    }

    fn request(args: serde_json::Value) -> ChangeRequest {
        ChangeRequest {
            kind: RequestKind::Bookkeeping,
            args,
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
            confirmed: None,
        }
    }

    /// The rewrites an action record lists under `reparented` (empty when it lists none).
    pub fn recorded(act: &crate::model::action::ActionRecord) -> Vec<super::Rewrite> {
        act.compensation
            .get(super::REPARENTED_KEY)
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .map(|r| super::Rewrite::from_toml(r).expect("well-formed rewrite row"))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn seat_request(seat: &SeatId, manager: Option<&SeatId>) -> ChangeRequest {
        request(json!({ "sub": "test_pointer", "seat": seat, "manager": manager }))
    }

    pub fn teamspace_request(ts: &TeamspaceId, manager: Option<&SeatId>) -> ChangeRequest {
        request(json!({ "sub": "test_pointer", "teamspace": ts, "manager": manager }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_toml_round_trip() {
        let with = Rewrite {
            holder: SeatId::new().to_any(),
            field: "manager".into(),
            old: SeatId::new(),
            new: Some(SeatId::new()),
        };
        assert_eq!(Rewrite::from_toml(&with.to_toml()).as_ref(), Some(&with));
        let without = Rewrite {
            new: None,
            field: "default_manager".into(),
            ..with.clone()
        };
        let v = without.to_toml();
        assert!(v.as_table().unwrap().get("new").is_none());
        assert_eq!(Rewrite::from_toml(&v), Some(without));
        assert_eq!(Rewrite::from_toml(&toml::Value::Boolean(true)), None);
    }
}
