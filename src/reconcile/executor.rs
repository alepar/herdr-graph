//! Effect source and executor registration API (spec §4.4). Other components (threads, delivery)
//! register their effect families here; the Herdr family is built in (`herdr_exec`).
use crate::journal::Journal;
use crate::model::Timestamp;
use crate::model::common::CommitId;
use crate::model::effect::{EffectKind, EffectRecord};
use crate::model::EffectId;
use crate::ports::herdr::{HerdrApi, HerdrSnapshot};
use crate::ports::writer::Writer;
use crate::store::tree::TreeRead;

/// Everything a source needs to diff committed desired state against the live snapshot.
pub struct DiffCx<'a> {
    pub tree: &'a dyn TreeRead,
    pub head: &'a CommitId,
    pub snapshot: &'a HerdrSnapshot,
    pub journal: &'a Journal,
    pub now: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedEffect {
    pub record: EffectRecord,
    /// Effects that must be `done` before this one runs.
    pub deps: Vec<EffectId>,
}

/// Produces the effects implied by one committed revision (level-triggered; ids are deterministic).
pub trait EffectSource: Send + Sync {
    fn effects(&self, cx: &DiffCx<'_>) -> Vec<PlannedEffect>;
}

pub struct ExecCx<'a> {
    /// `Sync` so that executor futures stay `Send`.
    pub tree: &'a (dyn TreeRead + Sync),
    pub head: &'a CommitId,
    pub snapshot: &'a HerdrSnapshot,
    pub herdr: &'a dyn HerdrApi,
    pub writer: &'a dyn Writer,
    pub now: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecOutcome {
    Done,
    /// Retry later with backoff.
    Transient(String),
    /// The call may or may not have happened: inspect the snapshot before any retry.
    Unknown,
    NeedsRevision(String),
    BlockedNeedsHuman,
    Failed(String),
    /// No longer implied by the committed state.
    Obsolete,
    /// Not yet allowed to run (grace period); stays pending without counting an attempt.
    Deferred(String),
}

#[async_trait::async_trait]
pub trait EffectExecutor: Send + Sync {
    fn handles(&self, kind: &EffectKind) -> bool;
    /// Fence (spec §4.4): is `e` still implied by the committed state `cx.tree` shows? Called before every
    /// execution, and for waiting rows, whenever the object's committed revision moved. Default: yes.
    fn is_implied(&self, _cx: &ExecCx<'_>, _e: &EffectRecord) -> bool {
        true
    }
    async fn execute(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome;
}
