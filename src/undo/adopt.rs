//! Caller-pane adoption (spec §6): when an undo restores runtime presence, the pane the caller is typing
//! in becomes one of the restored clones instead of a second pane being opened next to it.
use crate::model::common::{Availability, Binding, CloneLifecycle, HerdrPaneId, Occupant};
use crate::model::{ActionId, CloneId};
use crate::ports::store::{Store, StoreError};
use crate::ports::writer::{Writer, WriterError};
use crate::store::layout;
use crate::store::tree::{CommitView, TreeRead};

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
    let Some(clone) = restored_clones.first().cloned() else { return Ok(None) };
    let mut displaced = None;
    let mut displaced_binding = None;
    for (_, c) in layout::all_clones(tree)? {
        if c.lifecycle != CloneLifecycle::Active || restored_clones.contains(&c.id) {
            continue;
        }
        if c.runtime.bound.as_ref().and_then(|b| b.pane_id.as_ref()).is_some_and(|p| p.0 == caller_pane) {
            displaced = Some((c.id.clone(), c.occupant.clone()));
            displaced_binding = c.runtime.bound.clone();
            break;
        }
    }
    Ok(Some(Adoption { clone, pane: caller_pane.to_owned(), displaced, displaced_binding }))
}

impl Adoption {
    /// The binding the adopted clone starts with: the displaced clone's (token cleared: the reconciler stamps
    /// the new clone's token on its next step), or just the pane when nothing held it.
    pub fn binding(&self) -> Binding {
        match &self.displaced_binding {
            Some(b) => Binding { token: None, ..b.clone() },
            None => Binding { pane_id: Some(HerdrPaneId(self.pane.clone())), ..Binding::default() },
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

/// Admit the binding write for the adopted pane after the undo op committed: reads the adoption the undo
/// recorded in its action's compensation. Returns whether a binding write was admitted (false when the
/// action adopted nothing).
pub fn admit_adopted_binding(store: &dyn Store, writer: &dyn Writer, action: &ActionId) -> Result<bool, WriterError> {
    let head = store.head().map_err(|e| WriterError::Invalid(e.to_string()))?;
    let view = CommitView { store, at: head };
    let rec = super::candidates::read_action(&view, action).map_err(|e| WriterError::Invalid(e.to_string()))?;
    let Some(adopt) = rec.and_then(|r| r.compensation.get("adopt").and_then(|v| v.as_table().cloned())) else {
        return Ok(false);
    };
    let Some(clone) = adopt.get("clone").and_then(|v| v.as_str()).and_then(|s| s.parse::<CloneId>().ok()) else {
        return Ok(false);
    };
    let Some(binding) = adopt.get("binding").cloned().and_then(|v| v.try_into::<Binding>().ok()) else {
        return Ok(false);
    };
    crate::reconcile::bookkeeping::admit_binding(writer, &clone.to_any(), &binding, Availability::Present)?;
    Ok(true)
}
