//! Operation record `operations/<yyyy-mm>/<op-id>.toml` (spec §3.4).
use crate::model::change::{ReliedOn, RequestKind, Requester};
use crate::model::common::{CommitId, Timestamp};
use crate::model::ids::{ActionId, OpId, PlanId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub schema: u32,
    pub id: OpId,
    pub rev: u64,
    pub kind: RequestKind,
    pub summary: String,
    pub requester: Requester,
    pub state: OpState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<Confirmation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ActionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<OpId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<OpId>,
    pub admitted_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection: Option<Rejection>,
}

/// Details live in `commit`, `rejection`, `superseded_by`; `Failed` reason in `rejection.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpState {
    Admitted,
    Applying,
    Committed,
    Rejected,
    Cancelled,
    Superseded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirmation {
    pub mode: ConfirmMode,
    pub plan_hash: String,
    pub at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmMode {
    Tty,
    Relay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejection {
    pub reason: String,
    pub explanation: String,
    #[serde(default)]
    pub current_revs: Vec<ReliedOn>,
}
