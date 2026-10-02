//! ChangeRequest (spec §3.1): every write enters the system as one of these.
use crate::model::common::BlobHash;
use crate::model::ids::{AnyId, CloneId, NsId, OpId, SeatId, TeamspaceId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    TeamspaceCreate,
    TeamspaceRename,
    TeamspaceRetire,
    TeamspaceResurrect,
    SeatCreate,
    SeatActivate,
    SeatDeactivate,
    SeatRename,
    SeatRetire,
    SeatResurrect,
    SeatOverride,
    CloneAdd,
    CloneRetire,
    CloneRebind,
    ParticipationJoin,
    ParticipationLeave,
    TemplateCreate,
    TemplateEdit,
    TemplateCopy,
    ApplicationApply,
    ApplicationRetire,
    Undo,
    ContentWrite,
    Observed,
    Bookkeeping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Organizational,
    Content,
    Observed,
    Bookkeeping,
}

impl RequestKind {
    pub fn category(self) -> Category {
        match self {
            RequestKind::ContentWrite => Category::Content,
            RequestKind::Observed => Category::Observed,
            RequestKind::Bookkeeping => Category::Bookkeeping,
            _ => Category::Organizational,
        }
    }
    /// Spec §3.2: only organizational kinds go through plan → confirm → apply.
    pub fn requires_confirmation(self) -> bool {
        self.category() == Category::Organizational
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Version {
    Rev(u64),
    Blob(BlobHash),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReliedOn {
    pub object: AnyId,
    pub version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Requester {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teamspace: Option<TeamspaceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<SeatId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone: Option<CloneId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_session: Option<NsId>,
    #[serde(default, skip_serializing_if = "crate::model::common::is_false")]
    pub human: bool,
}

/// Every write (spec §3.1). Stored as JSON in the journal; `args` is kind-specific.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeRequest {
    pub kind: RequestKind,
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default)]
    pub relied_on: Vec<ReliedOn>,
    pub requester: Requester,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<OpId>,
}
