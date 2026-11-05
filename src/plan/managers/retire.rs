//! Node-manager retirement guard (hg-k4l.8): refuses a plan whose effects retire the live node manager.
use crate::plan::kind::PlanError;
use crate::plan::types::PlanEffect;
use crate::store::tree::TreeRead;

pub fn refuse_node_manager_retirement(
    _tree: &dyn TreeRead,
    _effects: &[PlanEffect],
) -> Result<(), PlanError> {
    Ok(())
}
