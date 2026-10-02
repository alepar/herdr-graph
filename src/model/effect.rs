//! Effect record: the journal `effects` table row (spec §3.3, §4.4). Serialized as JSON by the
//! journal; also round-trips TOML.
use crate::model::common::Timestamp;
use crate::model::ids::{AnyId, EffectId, OpId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectRecord {
    pub id: EffectId,
    pub op: OpId,
    pub object: AnyId,
    pub kind: EffectKind,
    pub object_rev: u64,
    pub fencing_rev: u64,
    pub status: EffectStatus,
    /// Predicted end state, including induced container closures (spec §3.3, §4.4).
    #[serde(default)]
    pub predicted: Vec<PredictedEnd>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce_label: Option<String>,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub updated_at: Timestamp,
    /// Dispatch marker and scheduling state, journaled with the row (one write with the status).
    #[serde(default)]
    pub sched: EffectSched,
}

/// Per-effect execution and scheduling state, journaled on the row itself (one write with the status).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EffectSched {
    /// Set in the write just before a non-idempotent call; cleared by the write that records its outcome. Found
    /// set later means the outcome was lost: treat the row as `Unknown`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched: Option<Dispatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wake_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub defer_n: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<EffectId>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The write-ahead marker of one execution attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dispatch {
    pub attempt: u32,
    pub at: Timestamp,
}

impl EffectRecord {
    /// `ef_ = hash(op, object, kind, object_rev)`.
    pub fn identity(op: &OpId, object: &AnyId, kind: &EffectKind, object_rev: u64) -> EffectId {
        EffectId::derive(op, object, kind.as_str(), object_rev)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    CreateWorkspace,
    CreateTab,
    SplitPane,
    StampToken,
    StartAgent,
    RenameWorkspace,
    RenameTab,
    RenamePane,
    CloseWorkspace,
    ClosePane,
    CloseTab,
    ReplaceSession,
    EnsureThread,
    Invite,
    ReleaseRequirement,
    Notify,
    SetTopic,
    DeliverRequest,
    RelaunchOccupant,
    Custom(String),
}

impl EffectKind {
    /// Kinds whose Herdr call is not safe to repeat blindly: a lost outcome is resolved from the snapshot.
    pub fn is_non_idempotent(&self) -> bool {
        matches!(
            self,
            EffectKind::CreateWorkspace
                | EffectKind::CreateTab
                | EffectKind::SplitPane
                | EffectKind::StartAgent
                | EffectKind::RelaunchOccupant
                | EffectKind::ReplaceSession
        )
    }

    /// The snake_case name (or the custom string); the `kind` input of `EffectId::derive`.
    pub fn as_str(&self) -> &str {
        match self {
            EffectKind::CreateWorkspace => "create_workspace",
            EffectKind::CreateTab => "create_tab",
            EffectKind::SplitPane => "split_pane",
            EffectKind::StampToken => "stamp_token",
            EffectKind::StartAgent => "start_agent",
            EffectKind::RenameWorkspace => "rename_workspace",
            EffectKind::RenameTab => "rename_tab",
            EffectKind::RenamePane => "rename_pane",
            EffectKind::CloseWorkspace => "close_workspace",
            EffectKind::ClosePane => "close_pane",
            EffectKind::CloseTab => "close_tab",
            EffectKind::ReplaceSession => "replace_session",
            EffectKind::EnsureThread => "ensure_thread",
            EffectKind::Invite => "invite",
            EffectKind::ReleaseRequirement => "release_requirement",
            EffectKind::Notify => "notify",
            EffectKind::SetTopic => "set_topic",
            EffectKind::DeliverRequest => "deliver_request",
            EffectKind::RelaunchOccupant => "relaunch_occupant",
            EffectKind::Custom(s) => s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectStatus {
    Pending,
    Done,
    Obsolete,
    Failed,
    Unknown,
    NeedsRevision,
    BlockedNeedsHuman,
}

/// `induced = true` marks induced container closures predicted by plans (spec §3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictedEnd {
    pub object: AnyId,
    pub container: ContainerKind,
    pub end: EndState,
    pub induced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerKind {
    Workspace,
    Tab,
    Pane,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndState {
    Present,
    Closed,
    Renamed { name: String },
}
