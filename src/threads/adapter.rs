//! Real `ThreadsPort` over herdr-threads' `PersistentServiceClient` (spec §7.1). One registered service
//! session per graph instance; mutations go through the durable intent journal, reads are queries.
//! A busy service or a dropped connection is reported as such for the reconciler to back off on; the adapter
//! never retries registration on its own, so it can never take over another registration.
use super::fake::thread_for_ensure_key;
use crate::model::clone::{InvitationState, InviteConstraint};
use crate::ports::threads::*;
use herdr_threads::client::service::{PersistentServiceClient, ServiceCallError, ServiceIntentJournal};
use herdr_threads::protocol::ids::{OperationId, RequirementId, SeatId, ThreadId};
use herdr_threads::protocol::pagination::PageRequest;
use herdr_threads::protocol::results::{ApiError, ErrorCode};
use herdr_threads::protocol::service::{
    EnsureManagedThread, InvitationConstraint, NotificationSeverity, ReleaseRequirement, RequirementState,
    ServiceInvite, ServiceMembership, ServiceMembershipQuery, ServiceNotify, ServiceOperation, ServiceResult,
    ServiceSetTopic, VoluntaryMembershipState,
};
use herdr_threads::protocol::time::{CallBudget, Cancellation, Clock as ThreadsClock, MonoInstant};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// A budget that ends `d` from now on `clock`'s monotonic reading.
pub(crate) fn call_budget(clock: &dyn ThreadsClock, d: Duration) -> CallBudget {
    let ms = u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
    CallBudget { deadline: MonoInstant(clock.monotonic_now().0.saturating_add(ms)), cancellation: Cancellation::default() }
}

/// Map a threads API error onto the port's error classes: busy and connection trouble are retried with
/// backoff by the reconciler, everything else is a definitive answer.
pub(crate) fn api_error(e: ApiError) -> ThreadsError {
    match e.code {
        ErrorCode::ServiceBusy => ThreadsError::ServiceBusy,
        ErrorCode::HostUnavailable
        | ErrorCode::TransportDenied
        | ErrorCode::UnknownOutcome
        | ErrorCode::DeadlineExceeded
        | ErrorCode::Cancelled
        | ErrorCode::StaleServiceGeneration
        | ErrorCode::ServiceNotRegistered
        | ErrorCode::DaemonBootChanged => ThreadsError::Disconnected(format!("{:?}: {}", e.code, e.detail)),
        ErrorCode::Unsupported | ErrorCode::UnsupportedHarness => ThreadsError::Unsupported,
        code => ThreadsError::Rejected(format!("{code:?}: {}", e.detail)),
    }
}

fn rejected(what: &str, e: impl std::fmt::Display) -> ThreadsError {
    ThreadsError::Rejected(format!("{what}: {e}"))
}

/// Threads operation ids are opaque printable strings of at most 128 bytes; longer graph keys are hashed.
pub(crate) fn operation_id(key: &OpKey) -> OperationId {
    OperationId::parse(key.0.clone()).unwrap_or_else(|_| {
        let digest = Sha256::digest(key.0.as_bytes());
        let hex: String = digest.iter().take(20).map(|b| format!("{b:02x}")).collect();
        OperationId::new(format!("h-{hex}"))
    })
}

fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}

fn map_requirement(s: RequirementState) -> InvitationState {
    match s {
        RequirementState::Pending => InvitationState::Pending,
        RequirementState::Accepted => InvitationState::Accepted,
        RequirementState::Released => InvitationState::Released,
        RequirementState::Retired => InvitationState::Retired,
    }
}

/// A seat's standing in a thread. A requirement episode, when there is one, is the answer for a graph
/// invitation (it is independent of voluntary membership); otherwise the voluntary state is.
fn detail_of(m: &ServiceMembership) -> Option<MembershipDetail> {
    if let Some(r) = &m.requirement {
        return Some(MembershipDetail {
            state: map_requirement(r.state),
            invitation: Some(r.invitation.as_str().to_owned()),
            requirement: Some(r.requirement.as_str().to_owned()),
            revision: Some(r.revision),
        });
    }
    let state = match m.voluntary_state {
        VoluntaryMembershipState::Absent => return None,
        VoluntaryMembershipState::Invited => InvitationState::Pending,
        VoluntaryMembershipState::Joined => InvitationState::Accepted,
        VoluntaryMembershipState::Left => InvitationState::Released,
        VoluntaryMembershipState::Retired => InvitationState::Retired,
    };
    Some(MembershipDetail { state, invitation: None, requirement: None, revision: None })
}

/// Where a running herdr-threads daemon for one state directory and Herdr host endpoint listens, and the
/// instance UUID its frames must name (read from the daemon's published descriptor, never guessed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovered {
    pub socket: PathBuf,
    pub instance: uuid::Uuid,
}

/// Read the published endpoint of the daemon that serves `state_dir` for the Herdr at `host_endpoint`.
/// `NotFound` when no daemon has published one (never started, or stopped).
pub fn discover(state_dir: &Path, host_endpoint: &Path) -> std::io::Result<Discovered> {
    use herdr_threads::daemon::{ownership, paths};
    let ctx = paths::RuntimeContext::explicit(state_dir.to_path_buf(), host_endpoint.to_path_buf(), None)?;
    let paths = paths::InstancePaths::resolve(&ctx)?;
    let instance = ownership::read_existing_namespace(&paths)?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no herdr-threads daemon has run for this state"))?;
    let d = ownership::read_descriptor(&paths, instance)?;
    Ok(Discovered { socket: d.endpoint, instance: d.instance_uuid })
}

pub struct ServiceThreads {
    client: PersistentServiceClient,
    clock: Arc<dyn ThreadsClock>,
    timeout: Duration,
    #[cfg_attr(not(feature = "threads-service-ack"), allow(dead_code))]
    socket: PathBuf,
    #[cfg_attr(not(feature = "threads-service-ack"), allow(dead_code))]
    instance: uuid::Uuid,
    /// Mutations whose outcome is unknown: replayed (same envelope) after the next registration.
    pending: tokio::sync::Mutex<Vec<OperationId>>,
    #[cfg_attr(not(feature = "threads-service-ack"), allow(dead_code))]
    capability: tokio::sync::Mutex<Option<DeliveryCapability>>,
}

impl ServiceThreads {
    /// `socket`: the herdr-threads daemon socket (configuration; never the user's live default when testing).
    /// `intents_dir`: `.graph-local/threads-intents` (private to this instance's author).
    /// `instance`: the threads instance UUID of the daemon (see [`discover`]); intents saved under one instance
    /// are only replayed under the same one.
    pub fn new(
        socket: PathBuf,
        intents_dir: PathBuf,
        instance: uuid::Uuid,
        clock: Arc<dyn ThreadsClock>,
    ) -> std::io::Result<Self> {
        if let Some(parent) = intents_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let journal = ServiceIntentJournal::open(&intents_dir)?;
        let pending = journal.pending()?;
        let client = PersistentServiceClient::new(socket.clone(), clock.clone(), instance, None, journal);
        Ok(Self {
            client,
            clock,
            timeout: Duration::from_secs(10),
            socket,
            instance,
            pending: tokio::sync::Mutex::new(pending),
            capability: tokio::sync::Mutex::new(None),
        })
    }

    /// `new` with the system clock.
    pub fn with_system_clock(socket: PathBuf, intents_dir: PathBuf, instance: uuid::Uuid) -> std::io::Result<Self> {
        Self::new(socket, intents_dir, instance, Arc::new(herdr_threads::app::SystemClock::new()))
    }

    pub fn intents_dir_in(instance_root: &Path) -> PathBuf {
        instance_root.join(".graph-local").join("threads-intents")
    }

    fn budget(&self) -> CallBudget {
        call_budget(&*self.clock, self.timeout)
    }

    /// Register on first use (and after the session was dropped). A busy answer is returned as `ServiceBusy`;
    /// registration is attempted once per call and never forced.
    async fn ready(&self) -> Result<(), ThreadsError> {
        if self.client.registration().await.is_some() {
            return Ok(());
        }
        self.client.register(&self.budget()).await.map(|_| ()).map_err(api_error)
    }

    /// Replay mutations left unresolved by a dropped connection or an earlier crash, in order. The result of
    /// the one the caller is about to issue again (`want`), if it was among them, is returned.
    async fn replay_pending(&self, want: &OperationId) -> Result<Option<ServiceResult>, ThreadsError> {
        let keys: Vec<OperationId> = self.pending.lock().await.clone();
        let mut found = None;
        for key in keys {
            match self.client.replay(&key, &self.budget()).await {
                Ok(r) => {
                    if &key == want {
                        found = Some(r);
                    }
                }
                Err(e) if e.pending_operation().is_some() => return Err(self.call_error(e).await),
                // Already completed, or its intent is gone: nothing left to replay.
                Err(_) => {}
            }
            self.pending.lock().await.retain(|k| k != &key);
            let _ = self.client.journal().forget_completed(&key);
        }
        Ok(found)
    }

    async fn call_error(&self, e: ServiceCallError) -> ThreadsError {
        if let Some(key) = e.pending_operation() {
            let mut p = self.pending.lock().await;
            if !p.contains(key) {
                p.push(key.clone());
            }
        }
        match e {
            ServiceCallError::Api { error, .. } => api_error(error),
            ServiceCallError::Journal(io) => ThreadsError::Disconnected(format!("intent journal: {io}")),
            ServiceCallError::Completion { result, .. } => match result {
                Ok(_) => ThreadsError::Disconnected("completion record needs repair".into()),
                Err(e) => api_error(e),
            },
        }
    }

    /// One durable mutation: a completed or unresolved intent under the same key is resumed, never duplicated.
    async fn mutate(&self, op: ServiceOperation) -> Result<ServiceResult, ThreadsError> {
        self.ready().await?;
        let key = op.operation_key().cloned().ok_or_else(|| rejected("mutate", "operation has no key"))?;
        if let Some(r) = self.replay_pending(&key).await? {
            return Ok(r);
        }
        let journal = self.client.journal();
        let result = if let Ok(done) = journal.inspect_completed(&key) {
            done.response.result.map_err(api_error)
        } else if journal.inspect(&key).is_ok() {
            match self.client.replay(&key, &self.budget()).await {
                Ok(r) => Ok(r),
                Err(e) => return Err(self.call_error(e).await),
            }
        } else {
            match self.client.submit(op, &self.budget()).await {
                Ok((_, r)) => Ok(r),
                Err(e) => return Err(self.call_error(e).await),
            }
        };
        let _ = journal.forget_completed(&key);
        result
    }

    async fn query_membership(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
    ) -> Result<Option<ServiceMembership>, ThreadsError> {
        self.ready().await?;
        let thread_id = ThreadId::parse(thread.0.clone()).map_err(|e| rejected("thread id", e))?;
        let seat_id = SeatId::parse(seat.0.clone()).map_err(|e| rejected("seat id", e))?;
        let q = ServiceOperation::Membership(ServiceMembershipQuery {
            thread: thread_id,
            seat: Some(seat_id.clone()),
            page: PageRequest::default(),
        });
        match self.client.query(q, &self.budget()).await {
            Ok(ServiceResult::Membership(page)) => Ok(page.items.into_iter().find(|m| m.seat == seat_id)),
            Ok(_) => Err(ThreadsError::Rejected("unexpected result for a membership query".into())),
            Err(e) if e.code == ErrorCode::NotFound => Ok(None),
            Err(e) => Err(api_error(e)),
        }
    }
}

fn thread_id(thread: &ThreadRef) -> Result<ThreadId, ThreadsError> {
    ThreadId::parse(thread.0.clone()).map_err(|e| rejected("thread id", e))
}

fn seat_id(seat: &ThreadsSeatRef) -> Result<SeatId, ThreadsError> {
    SeatId::parse(seat.0.clone()).map_err(|e| rejected("seat id", e))
}

#[async_trait::async_trait]
impl ThreadsPort for ServiceThreads {
    async fn ensure_thread(
        &self,
        scope: ChannelScope,
        topic: &str,
        op_key: &OpKey,
    ) -> Result<ThreadRef, ThreadsError> {
        let thread = thread_for_ensure_key(op_key);
        let what = match scope {
            ChannelScope::Seat => "seat",
            ChannelScope::Teamspace => "teamspace",
        };
        let op = ServiceOperation::EnsureThread(EnsureManagedThread {
            thread: thread_id(&thread)?,
            topic: clip(topic, 1024),
            goal: clip(&format!("Herdr Graph {what} channel: {topic}"), 1024),
            operation: operation_id(op_key),
        });
        match self.mutate(op).await? {
            ServiceResult::ThreadEnsured(m) => Ok(ThreadRef(m.thread.as_str().to_owned())),
            other => Err(rejected("ensure_thread", format!("unexpected result {other:?}"))),
        }
    }

    async fn invite(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
        constraint: InviteConstraint,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError> {
        let op = ServiceOperation::Invite(ServiceInvite {
            thread: thread_id(thread)?,
            seat: seat_id(seat)?,
            constraint: match constraint {
                InviteConstraint::Required => InvitationConstraint::Required,
                InviteConstraint::Ordinary => InvitationConstraint::Ordinary,
            },
            deadline_millis: None,
            operation: operation_id(op_key),
        });
        match self.mutate(op).await? {
            // Already joined is a settled no-op, not an error.
            ServiceResult::Invitation(_) | ServiceResult::AlreadyJoined(_) => Ok(()),
            other => Err(rejected("invite", format!("unexpected result {other:?}"))),
        }
    }

    async fn membership(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
    ) -> Result<Option<InvitationState>, ThreadsError> {
        Ok(self.membership_detail(thread, seat).await?.map(|d| d.state))
    }

    async fn membership_detail(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
    ) -> Result<Option<MembershipDetail>, ThreadsError> {
        Ok(self.query_membership(thread, seat).await?.as_ref().and_then(detail_of))
    }

    async fn notify(
        &self,
        thread: &ThreadRef,
        severity: Severity,
        body: &str,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError> {
        let op = ServiceOperation::Notify(ServiceNotify {
            thread: thread_id(thread)?,
            severity: match severity {
                Severity::Info => NotificationSeverity::Info,
                Severity::Warn => NotificationSeverity::Warn,
            },
            event_json: serde_json::json!({ "kind": "graph_notice", "body": body }),
            operation: operation_id(op_key),
        });
        match self.mutate(op).await? {
            ServiceResult::Notification(_) => Ok(()),
            other => Err(rejected("notify", format!("unexpected result {other:?}"))),
        }
    }

    async fn set_topic(&self, thread: &ThreadRef, topic: &str, op_key: &OpKey) -> Result<(), ThreadsError> {
        let op = ServiceOperation::SetTopic(ServiceSetTopic {
            thread: thread_id(thread)?,
            topic: clip(topic, 1024),
            operation: operation_id(op_key),
        });
        match self.mutate(op).await? {
            ServiceResult::TopicChanged(_) => Ok(()),
            other => Err(rejected("set_topic", format!("unexpected result {other:?}"))),
        }
    }

    async fn release_requirement(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError> {
        // The requirement id comes from a membership query first; a missing or ended episode is already clean.
        let Some(m) = self.query_membership(thread, seat).await? else { return Ok(()) };
        let Some(req) = m.requirement else { return Ok(()) };
        if !matches!(req.state, RequirementState::Pending | RequirementState::Accepted) {
            return Ok(());
        }
        let requirement = req.requirement.as_str().to_owned();
        let key = if op_key.0.ends_with(&requirement) { op_key.clone() } else { OpKey(format!("{}:{requirement}", op_key.0)) };
        let op = ServiceOperation::ReleaseRequirement(ReleaseRequirement {
            thread: thread_id(thread)?,
            seat: seat_id(seat)?,
            requirement: RequirementId::new(requirement),
            operation: operation_id(&key),
        });
        match self.mutate(op).await? {
            ServiceResult::RequirementReleased(_) => Ok(()),
            other => Err(rejected("release_requirement", format!("unexpected result {other:?}"))),
        }
    }

    async fn send_request(
        &self,
        _thread: &ThreadRef,
        _recipients: &[ThreadsSeatRef],
        _body: &str,
        _op_key: &OpKey,
    ) -> Result<MessageRef, ThreadsError> {
        // The service ACK operations (Send/Receipts) are not in the service protocol this build links.
        Err(ThreadsError::Unsupported)
    }

    async fn receipt_state(&self, _messages: &[MessageRef]) -> Result<Vec<MessageReceipts>, ThreadsError> {
        Err(ThreadsError::Unsupported)
    }

    #[cfg(not(feature = "threads-service-ack"))]
    async fn delivery_capability(&self) -> Result<DeliveryCapability, ThreadsError> {
        Ok(DeliveryCapability::NotifyFallback)
    }

    #[cfg(feature = "threads-service-ack")]
    async fn delivery_capability(&self) -> Result<DeliveryCapability, ThreadsError> {
        let mut cached = self.capability.lock().await;
        if let Some(c) = *cached {
            return Ok(c);
        }
        let c = self.probe_service_ack().await?;
        *cached = Some(c);
        Ok(c)
    }
}

#[cfg(feature = "threads-service-ack")]
impl ServiceThreads {
    /// Raw `Register{service_session_v2}` on a throwaway connection: `Unsupported` (an older service) means
    /// the Notify fallback; the registered v1 session is untouched either way.
    async fn probe_service_ack(&self) -> Result<DeliveryCapability, ThreadsError> {
        use herdr_threads::daemon::transport::{read_frame, write_frame};
        use herdr_threads::protocol::service::{
            ServiceRegister, ServiceRequest, ServiceWireRequest, ServiceWireResponse,
        };
        use herdr_threads::protocol::wire::PROTOCOL_VERSION;
        let mut stream = tokio::time::timeout(Duration::from_secs(2), tokio::net::UnixStream::connect(&self.socket))
            .await
            .map_err(|_| ThreadsError::Disconnected("threads connect timed out".into()))?
            .map_err(|e| ThreadsError::Disconnected(format!("threads connect: {e}")))?;
        let request = ServiceWireRequest {
            version: PROTOCOL_VERSION,
            request_id: uuid::Uuid::new_v4().to_string(),
            expected_instance: self.instance.to_string(),
            service: ServiceRequest::Register(ServiceRegister { capability: "service_session_v2".into() }),
        };
        let body = serde_json::to_vec(&request).map_err(|e| rejected("probe", e))?;
        let exchange = async {
            write_frame(&mut stream, &body).await?;
            read_frame(&mut stream).await
        };
        let bytes = tokio::time::timeout(self.timeout, exchange)
            .await
            .map_err(|_| ThreadsError::Disconnected("capability probe timed out".into()))?
            .map_err(|e| ThreadsError::Disconnected(format!("capability probe: {e}")))?;
        let response: ServiceWireResponse = serde_json::from_slice(&bytes).map_err(|e| rejected("probe response", e))?;
        match response.result {
            Ok(ServiceResult::Registered(_)) => Ok(DeliveryCapability::ServiceAck),
            Ok(_) => Ok(DeliveryCapability::NotifyFallback),
            Err(e) if matches!(e.code, ErrorCode::Unsupported | ErrorCode::InvalidRequest) => {
                Ok(DeliveryCapability::NotifyFallback)
            }
            Err(e) => Err(api_error(e)),
        }
    }
}
