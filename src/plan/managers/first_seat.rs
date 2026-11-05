//! First seat on the node becomes the node manager (spec §4.8, B10, S1). Every seat-creating or
//! -resurrecting plan path calls `assignment` with the first seat it creates or resurrects, and its `mutate`
//! calls `apply_assignment`; no path decides this itself. Paths: SeatCreate (hg-k4l.23), ApplicationApply and
//! TemplateEdit additions (hg-k4l.24), SeatResurrect, TeamspaceResurrect and undo restore (hg-k4l.25).

use super::{
    GRAPH_NODE_MANAGER, graph_blob, graph_relied_on, node_manager_effect, set_node_manager,
};
use crate::model::SeatId;
use crate::model::change::ReliedOn;
use crate::model::common::Lifecycle;
use crate::plan::types::PlanEffect;
use crate::ports::store::StoreError;
use crate::store::tree::TreeRead;
use crate::store::{Overlay, layout};
use crate::writer::MutationError;

/// The graph qualifies: graph.toml `node_manager_seat` is unset (None; a dangling id counts as set and gets the
/// §4.10 warning instead) and no seat is non-retired.
pub fn qualifies(tree: &dyn TreeRead) -> Result<bool, StoreError> {
    if layout::read_graph(tree)?.node_manager_seat.is_some() {
        return Ok(false);
    }
    Ok(layout::all_seats(tree)?
        .iter()
        .all(|(_, s)| s.lifecycle == Lifecycle::Retired))
}

/// Plan side. When `first` is Some and the graph qualifies: the `graph.node_manager_seat` effect (from None to
/// `first`, built with `node_manager_effect` and `graph_blob`) and its blob relied_on entry (`graph_relied_on`).
/// The caller must make that seat (and its teamspace) Active in the same plan (C3).
pub fn assignment(
    tree: &dyn TreeRead,
    first: Option<&SeatId>,
) -> Result<Option<(PlanEffect, ReliedOn)>, StoreError> {
    let Some(first) = first else { return Ok(None) };
    if !qualifies(tree)? {
        return Ok(None);
    }
    let effect = node_manager_effect(
        &layout::graph_id(tree)?,
        None,
        Some(first),
        &graph_blob(tree)?,
    );
    Ok(Some((effect, graph_relied_on(tree)?)))
}

/// Apply side: write the plan's `graph.node_manager_seat` effect, if it has one, with `set_node_manager` (the
/// confirmed plan equals the recompute, so the listed effect is authoritative). No-op when absent.
pub fn apply_assignment(
    tree: &mut Overlay<'_>,
    effects: &[PlanEffect],
) -> Result<(), MutationError> {
    let Some(e) = effects.iter().find(|e| e.kind == GRAPH_NODE_MANAGER) else {
        return Ok(());
    };
    let to: Option<SeatId> = e
        .detail
        .get("to")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|err| MutationError::Bug(format!("{} {}: malformed to: {err}", e.kind, e.object)))?
        .flatten();
    set_node_manager(tree, to)?;
    Ok(())
}

/// The plan assigns the role to `seat`: the path must create/resurrect it Active.
pub fn forces_active(effects: &[PlanEffect], seat: &SeatId) -> bool {
    effects.iter().any(|e| {
        e.kind == GRAPH_NODE_MANAGER
            && e.detail
                .get("to")
                .is_some_and(|to| to.as_str() == Some(seat.as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::store::Store;
    use crate::store::GitStore;
    use crate::store::init::init_instance;

    fn open(tmp: &tempfile::TempDir) -> GitStore {
        let root = tmp.path().join("inst");
        init_instance(&root).unwrap();
        GitStore::open(&root).unwrap()
    }

    #[test]
    fn qualifies_on_an_empty_graph_and_assignment_lists_the_effect() {
        let tmp = tempfile::tempdir().unwrap();
        let store = open(&tmp);
        let view = store.at(&store.head().unwrap());
        assert!(qualifies(&view).unwrap());
        let first = SeatId::new();
        let (effect, relied) = assignment(&view, Some(&first)).unwrap().unwrap();
        assert_eq!(effect.kind, GRAPH_NODE_MANAGER);
        assert_eq!(effect.detail["from"], serde_json::Value::Null);
        assert_eq!(effect.detail["to"], serde_json::json!(first));
        assert_eq!(relied, graph_relied_on(&view).unwrap());
        assert!(assignment(&view, None).unwrap().is_none());
    }

    #[test]
    fn does_not_qualify_when_the_field_is_set_even_dangling() {
        let tmp = tempfile::tempdir().unwrap();
        let store = open(&tmp);
        let head = store.head().unwrap();
        let mut ov = Overlay::new(&store, head);
        set_node_manager(&mut ov, Some(SeatId::new())).unwrap();
        assert!(!qualifies(&ov).unwrap());
        assert!(assignment(&ov, Some(&SeatId::new())).unwrap().is_none());
    }

    #[test]
    fn forces_active_only_for_the_named_seat() {
        let tmp = tempfile::tempdir().unwrap();
        let store = open(&tmp);
        let view = store.at(&store.head().unwrap());
        let (first, other) = (SeatId::new(), SeatId::new());
        let (effect, _) = assignment(&view, Some(&first)).unwrap().unwrap();
        assert!(forces_active(std::slice::from_ref(&effect), &first));
        assert!(!forces_active(std::slice::from_ref(&effect), &other));
        assert!(!forces_active(&[], &first));
    }

    #[test]
    fn apply_assignment_writes_the_listed_effect_and_is_a_noop_without_one() {
        let tmp = tempfile::tempdir().unwrap();
        let store = open(&tmp);
        let head = store.head().unwrap();
        let first = SeatId::new();
        let mut ov = Overlay::new(&store, head);
        apply_assignment(&mut ov, &[]).unwrap();
        assert_eq!(layout::read_graph(&ov).unwrap().node_manager_seat, None);
        let (effect, _) = assignment(&ov, Some(&first)).unwrap().unwrap();
        apply_assignment(&mut ov, &[effect]).unwrap();
        assert_eq!(
            layout::read_graph(&ov).unwrap().node_manager_seat,
            Some(first)
        );
    }
}
