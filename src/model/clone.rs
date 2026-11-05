//! Clone record (spec §2.4): a seat's live incarnation(s) and their session history.
use crate::model::HerdrPaneId;
use crate::model::common::{
    CloneLifecycle, NameChange, Occupant, Retirement, Runtime, Timestamp, is_false,
};
use crate::model::ids::{CloneId, OpId, SeatId};
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
    /// The herdr-threads seat this clone is bound to (spec §2). Written only by ThreadsSeatAssignment
    /// (`bookkeeping.threads_seat`): write-once, except seat succession against a matching Restore intent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threads_seat: Option<String>,
    /// Seats this clone held before a succession, oldest first (history only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads_seat_history: Vec<String>,
    /// Graph-initiated threads lifecycle intent (spec §8, §14). Written only by ThreadsSeatAssignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat_intent: Option<SeatIntent>,
    /// What threads last said about `threads_seat` (spec §14.9). Written only by SeatObservation
    /// (`bookkeeping.seat_fact`); the key-conflict alert level is `state == KeyConflict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat_fact: Option<SeatFact>,
}

/// One successful threads read of a clone's seat, with its read time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatFact {
    pub seat: String,
    #[serde(flatten)]
    pub state: SeatFactState,
    pub read_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SeatFactState {
    Resolved { pane: HerdrPaneId },
    Unresolved,
    KeyConflict { pane: Option<HerdrPaneId> },
    Retired { reversible: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SeatIntent {
    /// Written by an undo/resurrection commit; `epoch` is that commit's operation id.
    Restore {
        seat: String,
        epoch: OpId,
        #[serde(default)]
        threads_done: bool,
        #[serde(default)]
        launched: bool,
        /// The last threads refusal of this epoch's threads step, for status (§14.7).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_refusal: Option<String>,
    },
    /// Purge tombstone of a clone that holds a seat (§14.2).
    Finalize { seat: String },
    /// Purge tombstone of a seatless clone with a recorded key reservation (§14.3).
    ReleaseKey { key: String },
}

impl CloneRecord {
    /// A purged clone kept only to drive a finalize/release want (§13 "Tombstones are inert").
    pub fn is_tombstone(&self) -> bool {
        matches!(
            self.seat_intent,
            Some(SeatIntent::Finalize { .. } | SeatIntent::ReleaseKey { .. })
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitation {
    pub thread: String,
    pub constraint: InviteConstraint,
    pub state: InvitationState,
    /// Which threads seat was invited (attributed to which occupant), and the ids threads reported (hg-zmi.11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ThreadsLink>,
}

/// Threads-side identity of an invitation: enough to release a requirement after the occupant is gone and to
/// print the exact `accept-required` command (spec §7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadsLink {
    /// The threads seat (mapped from the clone's pane) that was invited.
    pub seat: String,
    /// Native session record of the occupant at the time of the invitation: attribution only. Nothing
    /// branches on it; the invitation belongs to the clone's `threads_seat` (`seat` above).
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
