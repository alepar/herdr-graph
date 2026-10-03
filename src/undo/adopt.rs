//! Caller-pane adoption (spec §6): when an undo restores runtime presence, the pane the caller is typing
//! in becomes one of the restored clones instead of a second pane being opened next to it.
use crate::model::CloneId;
use crate::model::common::{Availability, Binding, CloneLifecycle, HerdrPaneId, Occupant};
use crate::ports::herdr::HerdrSnapshot;
use crate::ports::store::StoreError;
use crate::store::layout;
use crate::store::tree::TreeRead;

/// Request-envelope key (outside the confirmed plan's args) carrying the caller's pane binding.
pub const ADOPT_BINDING_KEY: &str = "adopt_binding";

/// The restored clone that takes over the caller's pane, and the clone that held the pane until now.
#[derive(Debug, Clone, PartialEq)]
pub struct Adoption {
    pub clone: CloneId,
    pub pane: String,
    /// The active clone the pane is bound to today with its occupant (if any): it is displaced and retired.
    pub displaced: Option<(CloneId, Option<Occupant>)>,
    /// The displaced clone's binding, shown in the preview and handed to the adopted clone.
    pub displaced_binding: Option<Binding>,
}

/// `restored_clones`: the clones the undo brings back with a runtime, in preview order. None when there is
/// nothing to adopt the pane.
pub fn adoption(
    tree: &dyn TreeRead,
    caller_pane: &str,
    restored_clones: &[CloneId],
) -> Result<Option<Adoption>, StoreError> {
    let Some(clone) = restored_clones.first().cloned() else {
        return Ok(None);
    };
    let mut displaced = None;
    let mut displaced_binding = None;
    for (_, c) in layout::all_clones(tree)? {
        if c.lifecycle != CloneLifecycle::Active || restored_clones.contains(&c.id) {
            continue;
        }
        if c.runtime
            .bound
            .as_ref()
            .and_then(|b| b.pane_id.as_ref())
            .is_some_and(|p| p.0 == caller_pane)
        {
            displaced = Some((c.id.clone(), c.occupant.clone()));
            displaced_binding = c.runtime.bound.clone();
            break;
        }
    }
    Ok(Some(Adoption {
        clone,
        pane: caller_pane.to_owned(),
        displaced,
        displaced_binding,
    }))
}

impl Adoption {
    /// The binding the adopted clone starts with: the displaced clone's (token cleared: the reconciler stamps
    /// the new clone's token on its next step), or just the pane when nothing held it.
    pub fn binding(&self) -> Binding {
        match &self.displaced_binding {
            Some(b) => Binding {
                token: None,
                ..b.clone()
            },
            None => Binding {
                pane_id: Some(HerdrPaneId(self.pane.clone())),
                ..Binding::default()
            },
        }
    }

    /// What the adopted clone's runtime is committed as. `observed` is the caller's pane as Herdr showed it when
    /// the undo was applied: the clone is `present` and bound to it, so nothing creates a second pane. Without
    /// it (the pane was not listed) the clone stays `unknown` with the pane-only binding, which blocks creates
    /// until the observer has looked.
    pub fn committed_runtime(&self, observed: Option<&Binding>) -> (Availability, Binding) {
        match observed {
            Some(b) if b.pane_id.as_ref().is_some_and(|p| p.0 == self.pane) => (
                Availability::Present,
                Binding {
                    token: None,
                    ..b.clone()
                },
            ),
            _ => (Availability::Unknown, self.binding()),
        }
    }

    /// Why the adoption cannot go ahead: the pane's current clone (`described`) is occupied by a running session.
    pub fn conflict(&self, described: &str) -> Option<String> {
        let (clone, occ) = self.displaced.as_ref()?;
        occ.as_ref().map(|o| {
            format!(
                "your pane is bound to clone {described} ({clone}) which has an active {:?} session; end that \
                 session or rebind the clone before undoing",
                o.harness
            )
        })
    }
}

/// The caller's pane as `snap` shows it: the binding the adopted clone commits with (no token: the reconciler
/// stamps the clone's token on its next step). None when Herdr does not list the pane.
pub fn caller_binding(snap: &HerdrSnapshot, pane: &str) -> Option<Binding> {
    snap.workspaces.iter().find_map(|w| {
        w.tabs.iter().find_map(|t| {
            t.panes.iter().find(|p| p.id.0 == pane).map(|p| Binding {
                token: None,
                workspace_id: Some(w.id.clone()),
                tab_id: Some(t.id.clone()),
                pane_id: Some(p.id.clone()),
                terminal_id: p.terminal_id.clone(),
                incarnation: snap.incarnation.clone(),
            })
        })
    })
}
