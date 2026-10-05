//! herdr-threads port (spec §7). Real adapter + FakeThreads implemented by hg-zmi.11.
//! Graph-local value types keep the port fake-friendly; the adapter maps them to herdr_threads::protocol.
use crate::model::Timestamp;
use crate::model::clone::{InvitationState, InviteConstraint};
use serde::{Deserialize, Serialize};

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
    #[error("threads rejected: {0}")]
    Rejected(String),
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
}
