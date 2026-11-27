//! herdr-threads port (spec §7). Real adapter + FakeThreads implemented by hg-zmi.11.
//! Graph-local value types keep the port fake-friendly; the adapter maps them to herdr_threads::protocol.
use crate::model::clone::{InvitationState, InviteConstraint};
use crate::model::{HerdrPaneId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

macro_rules! str_ref {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
    };
}
str_ref!(ThreadRef);
str_ref!(ThreadsSeatRef);
str_ref!(MessageRef);
// Stable idempotency key (threads `OperationId`), derived from graph object/op ids.
str_ref!(OpKey);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelScope {
    Seat,
    Teamspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryCapability {
    /// `service_session_v2` registration succeeded: Send/Receipts with per-recipient ACK (spec §7.4.1).
    ServiceAck,
    /// Notify{Warn} + `herdr-graph request ack` (spec §7.4.2).
    NotifyFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptState {
    Pending,
    Acknowledged {
        at: Timestamp,
    },
    Retired,
    /// The recipient has no ACK obligation; this is not proof of dispatch.
    NotRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientReceipt {
    pub seat: ThreadsSeatRef,
    pub state: ReceiptState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageReceipts {
    pub message: MessageRef,
    pub recipients: Vec<RecipientReceipt>,
}

/// What threads reports about one (thread, seat) membership: the state plus the ids a native agent needs for
/// `accept-required` (invitation, requirement episode and its revision).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipDetail {
    pub state: InvitationState,
    pub invitation: Option<String>,
    pub requirement: Option<String>,
    pub revision: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum ThreadsError {
    #[error("threads service busy")]
    ServiceBusy,
    #[error("threads service disconnected: {0}")]
    Disconnected(String),
    #[error("operation unsupported by this threads service")]
    Unsupported,
    /// A definitive refusal: an answer threads itself returned, or a request it would refuse as malformed (an id
    /// that does not parse). Never a graph-local failure (namespace guard or bind, host claim, intent journal,
    /// explicit config): those are `Disconnected` with a `graph-local: ` prefix, so they are retried.
    #[error("threads rejected: {0}")]
    Rejected(String),
}

/// How a retired seat can come back: `Reversible` can be un-retired (threads D2), `Final` cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetirementKind {
    Reversible,
    Final,
}

/// What threads says about one clone's seat. Owns the closed set of seat states the graph acts on; hides the wire
/// shapes; carries no graph decision (SeatObservation makes those). Every seat state is a value; only transport
/// problems are errors (ThreadsSeatView's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SeatStatus {
    Resolved {
        pane: HerdrPaneId,
    },
    Unresolved,
    /// `keyed` is whether the seat carries a continuity key (a finally retired seat whose key threads released
    /// reads `keyed: false`); `retired_at` is when threads retired it, if it says.
    Retired {
        kind: RetirementKind,
        keyed: bool,
        retired_at: Option<Timestamp>,
    },
    KeyConflict {
        pane: Option<HerdrPaneId>,
    },
}

/// Why threads refused a ResurrectSeat (or the U2 held-seat rebind). Transient reasons are retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatRestoreRefusal {
    // definitive
    NotFound,
    Unauthorized,
    TargetAlreadyOwned,
    InvalidRequest,
    Unsupported,
    InstanceMismatch,
    // transient
    ServiceBusy,
    StaleHostObservation,
    Unreachable,
    OutcomeUnknown,
}

impl SeatRestoreRefusal {
    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::ServiceBusy
                | Self::StaleHostObservation
                | Self::Unreachable
                | Self::OutcomeUnknown
        )
    }
}

/// Answer of `resurrect_seat` / `rebind_held_seat`. Transport failures are a retryable refusal, not an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SeatRestoreOutcome {
    Done,
    /// Threads `SeatAlreadyResolved`: the seat is already resolved on the target pane (a no-op; callers count it
    /// done).
    AlreadyResolved,
    Refused {
        reason: SeatRestoreRefusal,
        retryable: bool,
    },
}

impl SeatRestoreOutcome {
    /// The only constructor for `Refused`: `retryable` always equals `reason.retryable()`.
    pub fn refused(reason: SeatRestoreRefusal) -> Self {
        Self::Refused {
            reason,
            retryable: reason.retryable(),
        }
    }

    /// A transport error from a call that never got a typed answer. `Rejected` is definitive (`InvalidRequest`)
    /// because it only carries threads' own answers; graph-local failures arrive as `Disconnected` and map to the
    /// retryable `Unreachable`.
    pub fn from_error(e: &ThreadsError) -> Self {
        Self::refused(match e {
            ThreadsError::ServiceBusy => SeatRestoreRefusal::ServiceBusy,
            ThreadsError::Disconnected(_) => SeatRestoreRefusal::Unreachable,
            ThreadsError::Unsupported => SeatRestoreRefusal::Unsupported,
            ThreadsError::Rejected(_) => SeatRestoreRefusal::InvalidRequest,
        })
    }
}

/// Threads 3.5.0 keyless reversible retirement: obligations settle at 24 h, the retirement turns final at 7 d.
pub const KEYLESS_OBLIGATIONS_SETTLE_AFTER: chrono::Duration = chrono::Duration::seconds(24 * 3600);
pub const KEYLESS_REVERSIBLE_FOR: chrono::Duration = chrono::Duration::seconds(7 * 24 * 3600);

/// Read-only view of threads seat state, once per observation step. Never writes to threads, never commits graph
/// state, never fabricates a state when threads is unreachable.
#[async_trait::async_trait]
pub trait ThreadsSeatView: Send + Sync {
    /// One status per requested seat, or the transport error of the first failing read (no partial map).
    async fn inspect(
        &self,
        seats: &[ThreadsSeatRef],
    ) -> Result<BTreeMap<ThreadsSeatRef, SeatStatus>, ThreadsError>;
}

/// What threads answers to a continuity-key reservation. A key bound elsewhere, or a seat holding another key,
/// is a typed `Conflict`, not an error; a refusal (retired, named or hc seat) is `ThreadsError::Rejected`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReserveOutcome {
    /// The key is reserved for the graph; `seat` is the seat it is bound to, if any.
    Reserved {
        seat: Option<ThreadsSeatRef>,
    },
    Conflict,
}

#[async_trait::async_trait]
pub trait ThreadsPort: Send + Sync {
    /// Managed public channel; `op_key` derived from the owning object id (spec §7.1).
    async fn ensure_thread(
        &self,
        scope: ChannelScope,
        topic: &str,
        op_key: &OpKey,
    ) -> Result<ThreadRef, ThreadsError>;
    async fn invite(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
        constraint: InviteConstraint,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError>;
    /// Invitation/membership state; never fabricates acceptance. None = no invitation.
    async fn membership(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
    ) -> Result<Option<InvitationState>, ThreadsError>;
    /// `membership` plus the threads-side ids. Additive to the original trait: the default has no ids.
    async fn membership_detail(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
    ) -> Result<Option<MembershipDetail>, ThreadsError> {
        Ok(self
            .membership(thread, seat)
            .await?
            .map(|state| MembershipDetail {
                state,
                invitation: None,
                requirement: None,
                revision: None,
            }))
    }
    async fn notify(
        &self,
        thread: &ThreadRef,
        severity: Severity,
        body: &str,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError>;
    async fn set_topic(
        &self,
        thread: &ThreadRef,
        topic: &str,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError>;
    async fn release_requirement(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError>;
    /// ACK-required request (ServiceAck capability only; `Unsupported` otherwise).
    async fn send_request(
        &self,
        thread: &ThreadRef,
        recipients: &[ThreadsSeatRef],
        body: &str,
        op_key: &OpKey,
    ) -> Result<MessageRef, ThreadsError>;
    async fn receipt_state(
        &self,
        messages: &[MessageRef],
    ) -> Result<Vec<MessageReceipts>, ThreadsError>;
    /// Which delivery path this connection supports (spec §7.4).
    async fn delivery_capability(&self) -> Result<DeliveryCapability, ThreadsError>;
    /// Reserve the continuity key `key` for the graph, binding it to `seat` when one is named (spec §5). Idempotent
    /// under `op`. Additive to the original trait: the default is `Unsupported`.
    async fn reserve_continuity_key(
        &self,
        _key: &str,
        _seat: Option<&ThreadsSeatRef>,
        _op: &OpKey,
    ) -> Result<ReserveOutcome, ThreadsError> {
        Err(ThreadsError::Unsupported)
    }
    /// Undo the reversible retirement of `seat` (`service_session_v3`). Idempotent under `op`. Additive: the
    /// default is `Unsupported`.
    async fn unretire_seat(&self, _seat: &ThreadsSeatRef, _op: &OpKey) -> Result<(), ThreadsError> {
        Err(ThreadsError::Unsupported)
    }
    /// Make the retirement of `seat` final; threads releases the seat's continuity key (`service_session_v3`).
    /// Idempotent under `op`. Additive: the default is `Unsupported`.
    async fn finalize_retirement(
        &self,
        _seat: &ThreadsSeatRef,
        _op: &OpKey,
    ) -> Result<(), ThreadsError> {
        Err(ThreadsError::Unsupported)
    }
    /// Release the graph's reservation of `key`, which must be bound to no seat (`service_session_v3`).
    /// Idempotent under `op`. Additive: the default is `Unsupported`.
    async fn release_continuity_key(&self, _key: &str, _op: &OpKey) -> Result<(), ThreadsError> {
        Err(ThreadsError::Unsupported)
    }
    /// ResurrectSeat (`service_session_v4`): bring reversibly retired `seat` back onto `pane`, retiring the bare
    /// `replace` seat threads minted for that pane in the same decision. Idempotent under `op`. Additive: the
    /// default is `Unsupported`.
    async fn resurrect_seat(
        &self,
        _seat: &ThreadsSeatRef,
        _pane: &HerdrPaneId,
        _replace: Option<&ThreadsSeatRef>,
        _op: &OpKey,
    ) -> SeatRestoreOutcome {
        SeatRestoreOutcome::refused(SeatRestoreRefusal::Unsupported)
    }
    /// U2 (threads 3.6.0): ResurrectSeat with `replace: None` on a restart-held seat's own saved pane. Idempotent
    /// under `op`. Additive: the default is `Unsupported`.
    async fn rebind_held_seat(
        &self,
        _seat: &ThreadsSeatRef,
        _pane: &HerdrPaneId,
        _op: &OpKey,
    ) -> SeatRestoreOutcome {
        SeatRestoreOutcome::refused(SeatRestoreRefusal::Unsupported)
    }
}
