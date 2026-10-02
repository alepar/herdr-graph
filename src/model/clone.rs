//! Clone record (spec §2.4): a seat's live incarnation(s) and their session history.
use crate::model::common::{is_false, CloneLifecycle, NameChange, Occupant, Retirement, Runtime};
use crate::model::ids::{CloneId, SeatId};
use crate::model::native_session::NativeSession;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloneRecord {
    pub schema: u32,
    pub id: CloneId,
    pub rev: u64,
    pub seat: SeatId,
    pub name: String,
    #[serde(default)]
    pub name_history: Vec<NameChange>,
    pub lifecycle: CloneLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired: Option<Retirement>,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupant: Option<Occupant>,
    #[serde(default)]
    pub sessions: Vec<NativeSession>,
    #[serde(default)]
    pub opt_outs: Vec<String>,
    #[serde(default)]
    pub invitations: Vec<Invitation>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub reload_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitation {
    pub thread: String,
    pub constraint: InviteConstraint,
    pub state: InvitationState,
    /// Which threads seat was invited, for which occupant, and the ids threads reported (hg-zmi.11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ThreadsLink>,
}

/// Threads-side identity of an invitation: enough to release a requirement after the occupant is gone and to
/// print the exact `accept-required` command (spec §7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadsLink {
    /// The threads seat (mapped from the clone's pane) that was invited.
    pub seat: String,
    /// Native session record of the occupant the invitation was issued for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invitation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteConstraint {
    Required,
    Ordinary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationState {
    Pending,
    Accepted,
    Released,
    Retired,
}
