//! Reparenting on seat retirement (spec §4.11). Apply side (`reparent`, from do_retire_seat; hg-k4l.26) and plan
//! preview (`reparent_effects`; hg-k4l.27). Retirement routes never rewrite pointers themselves.
use super::{default_manager_effect, seat_manager_effect, set_default_manager, set_seat_manager};
use crate::model::manager::Pointers;
use crate::model::{AnyId, SeatId, TeamspaceId};
use crate::plan::types::PlanEffect;
use crate::ports::store::StoreError;
use crate::store::tree::TreeRead;
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

/// Live records pointing at one retiring seat, each sorted by id.
struct Holders {
    seats: Vec<SeatId>,
    teamspaces: Vec<TeamspaceId>,
}

/// The live records whose pointer is `old`: seats not retired and not in `retiring` whose `manager` is `old`, and
/// teamspaces not Retired whose `default_manager` is `old`. `retiring` holders are skipped (a retiring holder's
/// pointer is archived with it).
fn holders(
    pointers: &Pointers,
    tree: &dyn TreeRead,
    old: &SeatId,
    retiring: &BTreeSet<SeatId>,
) -> Result<Holders, StoreError> {
    let mut seats: Vec<SeatId> = pointers
        .seats
        .iter()
        .filter(|(id, p)| !p.retired && !retiring.contains(*id) && p.manager.as_ref() == Some(old))
        .map(|(id, _)| id.clone())
        .collect();
    seats.sort();
    let mut teamspaces = Vec::new();
    for (_, ts) in layout::list_teamspaces(tree)? {
        if ts.lifecycle != crate::model::common::Lifecycle::Retired
            && ts.default_manager.as_ref() == Some(old)
        {
            teamspaces.push(ts.id);
        }
    }
    teamspaces.sort();
    Ok(Holders { seats, teamspaces })
}

/// A retiring seat's live holders, the target they move to, and the lenient-fallback note if one was taken.
type Target = (Holders, Option<SeatId>, Option<String>);

/// The one reparent target computation, shared by `reparent` (apply) and `reparent_effects` (preview). The chain
/// walk happens only when `old` has at least one live holder (spec §4.11): `None` means no holder, nothing to
/// rewrite, and the chain was not walked, so a broken chain of `old` itself cannot fail or be noted.
/// `lenient`: an integrity error from the walk falls back to `live_node_manager` (or None) and returns a note;
/// not lenient, the error propagates (StoreError::Integrity → Fail::Data per hg-k4l.5).
fn lazy_target(
    pointers: &Pointers,
    tree: &dyn TreeRead,
    old: &SeatId,
    retiring: &BTreeSet<SeatId>,
    lenient: bool,
) -> Result<Option<Target>, StoreError> {
    let h = holders(pointers, tree, old, retiring)?;
    if h.seats.is_empty() && h.teamspaces.is_empty() {
        return Ok(None);
    }
    let (target, note) = match pointers.nearest_surviving_manager(old, retiring) {
        Ok(t) => (t, None),
        Err(e @ StoreError::Integrity { .. }) if lenient => {
            let t = pointers.live_node_manager();
            let note = format!(
                "reparenting reports of {old}: {e}; fell back to {}",
                t.as_ref()
                    .map_or("no manager".to_string(), |t| format!("node manager {t}"))
            );
            (t, Some(note))
        }
        Err(e) => return Err(e),
    };
    Ok(Some((h, target, note)))
}

/// Apply side, from do_retire_seat before the seat is archived: rewrite every live seat whose explicit manager is
/// `seat` and every non-retired teamspace whose default_manager is `seat` to `seat`'s nearest surviving manager.
/// `retiring` is every seat the mutation retires (routes that retire several seats fill it; `seat` is added): those
/// seats are skipped as targets and as holders (a retiring holder's pointer is archived with it), so every live
/// holder gets one row whose `old` is its original pointer and final pointers match the preview. With no live
/// holder nothing is computed (see `lazy_target`).
/// `lenient`: see `lazy_target`.
pub fn reparent(
    tree: &mut Overlay<'_>,
    seat: &SeatId,
    retiring: &BTreeSet<SeatId>,
    lenient: bool,
) -> Result<(Vec<Rewrite>, Option<String>), StoreError> {
    let pointers = Pointers::load(&*tree)?;
    let mut retiring = retiring.clone();
    retiring.insert(seat.clone());
    let Some((h, target, note)) = lazy_target(&pointers, &*tree, seat, &retiring, lenient)? else {
        return Ok((Vec::new(), None));
    };
    let mut out = Vec::new();
    for id in h.seats {
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
    for id in h.teamspaces {
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

/// Plan preview for seats retiring together: for each live seat (not itself in `retiring`) whose explicit manager is
/// in `retiring`, and each non-retired teamspace whose default_manager is in `retiring`, one seat.manager /
/// teamspace.default_manager effect (`seat_manager_effect` / `default_manager_effect`) from the retiring seat to its
/// nearest surviving manager, skipping every seat in `retiring`. Seat effects come first (by holder id), then
/// teamspace effects (by teamspace id). A retiring holder gets no effect: its pointer is archived with it, so
/// preview and apply agree on final pointers of live holders. Targets come from `lazy_target`, so a retiring seat
/// nothing points at is never walked.
pub fn reparent_effects(
    tree: &dyn TreeRead,
    retiring: &[SeatId],
) -> Result<Vec<PlanEffect>, StoreError> {
    let retiring: BTreeSet<SeatId> = retiring.iter().cloned().collect();
    let pointers = Pointers::load(tree)?;
    let mut seats: Vec<(SeatId, SeatId, Option<SeatId>)> = Vec::new();
    let mut teamspaces: Vec<(TeamspaceId, SeatId, Option<SeatId>)> = Vec::new();
    for old in &retiring {
        let Some((h, target, _)) = lazy_target(&pointers, tree, old, &retiring, false)? else {
            continue;
        };
        for id in h.seats {
            // Mirrors `reparent`: never point a seat at itself.
            let new = target.clone().filter(|t| *t != id);
            seats.push((id, old.clone(), new));
        }
        for id in h.teamspaces {
            teamspaces.push((id, old.clone(), target.clone()));
        }
    }
    seats.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    teamspaces.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let mut out = Vec::new();
    for (id, old, new) in seats {
        out.push(seat_manager_effect(&id, Some(&old), new.as_ref()));
    }
    for (id, old, new) in teamspaces {
        out.push(default_manager_effect(&id, Some(&old), new.as_ref()));
    }
    Ok(out)
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
