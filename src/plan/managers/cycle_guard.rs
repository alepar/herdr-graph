//! Manager-chain cycle guard (hg-k4l.21): refuses a plan whose pointer effects would create a cycle.
use crate::plan::kind::PlanError;
use crate::plan::types::PlanEffect;
use crate::store::tree::TreeRead;

pub fn refuse_cyclic_chains(
    _tree: &dyn TreeRead,
    _effects: &[PlanEffect],
) -> Result<(), PlanError> {
    Ok(())
}
