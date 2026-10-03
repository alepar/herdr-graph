//! In-memory `ThreadsPort` for tests (spec §11). Behaves like the real service where it matters to graph:
//! operation keys are idempotent, a Required invitation stays `pending` until `accept` simulates the native
//! agent, and nothing ever fabricates acceptance.
use crate::model::clone::{InvitationState, InviteConstraint};
use crate::ports::threads::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeThread {
    pub scope: ChannelScope,
    pub topic: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeMember {
    pub constraint: InviteConstraint,
    pub state: InvitationState,
    pub invitation: String,
    pub requirement: Option<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeNotification {
    pub thread: ThreadRef,
    pub severity: Severity,
    pub body: String,
    pub op_key: OpKey,
}

#[derive(Default)]
struct State {
    threads: BTreeMap<String, FakeThread>,
    members: BTreeMap<(String, String), FakeMember>,
    notifications: Vec<FakeNotification>,
    seen_ops: BTreeSet<String>,
    /// Successful and attempted mutating calls by name, for assertions.
    calls: Vec<String>,
    busy: bool,
    disconnected: bool,
    capability: Option<DeliveryCapability>,
    next_id: u64,
}

/// The thread id the fake (and the real adapter) derive from an `ensure:<object>` operation key.
pub fn thread_for_ensure_key(op_key: &OpKey) -> ThreadRef {
    let object = op_key.0.strip_prefix("ensure:").unwrap_or(&op_key.0);
    ThreadRef(format!("hg-{}", object.to_lowercase()))
}

#[derive(Default)]
pub struct FakeThreads {
    state: Mutex<State>,
}

impl FakeThreads {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// While busy every call fails with `ServiceBusy` (another registration owns the service).
    pub fn set_busy(&self, busy: bool) {
        self.lock().busy = busy;
    }

    /// While disconnected every call fails with `Disconnected`.
    pub fn disconnect(&self) {
        self.lock().disconnected = true;
    }

    pub fn reconnect(&self) {
        self.lock().disconnected = false;
    }

    pub fn set_capability(&self, c: DeliveryCapability) {
        self.lock().capability = Some(c);
    }

    /// The native agent explicitly accepts: a pending Required episode becomes `accepted`, an ordinary
    /// invitation becomes joined. The only way acceptance ever appears.
    pub fn accept(&self, thread: &ThreadRef, seat: &ThreadsSeatRef) {
        if let Some(m) = self
            .lock()
            .members
            .get_mut(&(thread.0.clone(), seat.0.clone()))
            && m.state == InvitationState::Pending
        {
            m.state = InvitationState::Accepted;
        }
    }

    /// The native agent leaves an ordinary thread voluntarily.
    pub fn leave(&self, thread: &ThreadRef, seat: &ThreadsSeatRef) {
        if let Some(m) = self
            .lock()
            .members
            .get_mut(&(thread.0.clone(), seat.0.clone()))
            && m.constraint == InviteConstraint::Ordinary
        {
            m.state = InvitationState::Released;
        }
    }

    /// Create a thread the graph did not manage (a participation thread).
    pub fn add_thread(&self, thread: &str, topic: &str) {
        self.lock().threads.insert(
            thread.to_owned(),
            FakeThread {
                scope: ChannelScope::Teamspace,
                topic: topic.to_owned(),
            },
        );
    }

    pub fn thread(&self, thread: &ThreadRef) -> Option<FakeThread> {
        self.lock().threads.get(&thread.0).cloned()
    }

    pub fn threads(&self) -> Vec<ThreadRef> {
        self.lock().threads.keys().cloned().map(ThreadRef).collect()
    }

    pub fn member(&self, thread: &ThreadRef, seat: &ThreadsSeatRef) -> Option<FakeMember> {
        self.lock()
            .members
            .get(&(thread.0.clone(), seat.0.clone()))
            .cloned()
    }

    pub fn notifications(&self) -> Vec<FakeNotification> {
        self.lock().notifications.clone()
    }

    /// Names of the mutating calls that reached the service (`ensure_thread`, `invite`, ...), in order.
    pub fn calls(&self) -> Vec<String> {
        self.lock().calls.clone()
    }

    pub fn call_count(&self, name: &str) -> usize {
        self.lock()
            .calls
            .iter()
            .filter(|c| c.as_str() == name)
            .count()
    }

    fn gate(st: &State) -> Result<(), ThreadsError> {
        if st.busy {
            return Err(ThreadsError::ServiceBusy);
        }
        if st.disconnected {
            return Err(ThreadsError::Disconnected(
                "fake threads disconnected".into(),
            ));
        }
        Ok(())
    }
}

fn require_thread<'a>(st: &'a State, thread: &ThreadRef) -> Result<&'a FakeThread, ThreadsError> {
    st.threads
        .get(&thread.0)
        .ok_or_else(|| ThreadsError::Rejected(format!("unknown thread {}", thread.0)))
}

#[async_trait::async_trait]
impl ThreadsPort for FakeThreads {
    async fn ensure_thread(
        &self,
        scope: ChannelScope,
        topic: &str,
        op_key: &OpKey,
    ) -> Result<ThreadRef, ThreadsError> {
        let mut st = self.lock();
        Self::gate(&st)?;
        let thread = thread_for_ensure_key(op_key);
        st.calls.push("ensure_thread".into());
        st.threads
            .entry(thread.0.clone())
            .or_insert_with(|| FakeThread {
                scope,
                topic: topic.to_owned(),
            });
        st.seen_ops.insert(op_key.0.clone());
        Ok(thread)
    }

    async fn invite(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
        constraint: InviteConstraint,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError> {
        let mut st = self.lock();
        Self::gate(&st)?;
        require_thread(&st, thread)?;
        st.calls.push("invite".into());
        if !st.seen_ops.insert(op_key.0.clone()) {
            return Ok(());
        }
        st.next_id += 1;
        let n = st.next_id;
        let key = (thread.0.clone(), seat.0.clone());
        if let Some(existing) = st.members.get_mut(&key) {
            // Already joined: a settled no-op. A Required episode on a joined member is still pending
            // confirmation (spec §7.1), a fresh episode when the previous one ended.
            if constraint == InviteConstraint::Required
                && !(existing.constraint == InviteConstraint::Required
                    && matches!(
                        existing.state,
                        InvitationState::Pending | InvitationState::Accepted
                    ))
            {
                existing.constraint = constraint;
                existing.state = InvitationState::Pending;
                existing.requirement = Some(format!("requirement-{n}"));
                existing.revision += 1;
            } else if constraint == InviteConstraint::Ordinary
                && matches!(
                    existing.state,
                    InvitationState::Released | InvitationState::Retired
                )
            {
                existing.state = InvitationState::Pending;
            }
            return Ok(());
        }
        st.members.insert(
            key,
            FakeMember {
                constraint,
                state: InvitationState::Pending,
                invitation: format!("inv-{n}"),
                requirement: (constraint == InviteConstraint::Required)
                    .then(|| format!("requirement-{n}")),
                revision: 1,
            },
        );
        Ok(())
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
        let st = self.lock();
        Self::gate(&st)?;
        Ok(st
            .members
            .get(&(thread.0.clone(), seat.0.clone()))
            .map(|m| MembershipDetail {
                state: m.state,
                invitation: Some(m.invitation.clone()),
                requirement: m.requirement.clone(),
                revision: m.requirement.is_some().then_some(m.revision),
            }))
    }

    async fn notify(
        &self,
        thread: &ThreadRef,
        severity: Severity,
        body: &str,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError> {
        let mut st = self.lock();
        Self::gate(&st)?;
        require_thread(&st, thread)?;
        st.calls.push("notify".into());
        if st.seen_ops.insert(op_key.0.clone()) {
            st.notifications.push(FakeNotification {
                thread: thread.clone(),
                severity,
                body: body.to_owned(),
                op_key: op_key.clone(),
            });
        }
        Ok(())
    }

    async fn set_topic(
        &self,
        thread: &ThreadRef,
        topic: &str,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError> {
        let mut st = self.lock();
        Self::gate(&st)?;
        require_thread(&st, thread)?;
        st.calls.push("set_topic".into());
        st.seen_ops.insert(op_key.0.clone());
        if let Some(t) = st.threads.get_mut(&thread.0) {
            t.topic = topic.to_owned();
        }
        Ok(())
    }

    async fn release_requirement(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
        op_key: &OpKey,
    ) -> Result<(), ThreadsError> {
        let mut st = self.lock();
        Self::gate(&st)?;
        st.calls.push("release_requirement".into());
        st.seen_ops.insert(op_key.0.clone());
        if let Some(m) = st.members.get_mut(&(thread.0.clone(), seat.0.clone()))
            && m.constraint == InviteConstraint::Required
            && matches!(
                m.state,
                InvitationState::Pending | InvitationState::Accepted
            )
        {
            m.state = InvitationState::Released;
        }
        Ok(())
    }

    async fn send_request(
        &self,
        _thread: &ThreadRef,
        _recipients: &[ThreadsSeatRef],
        _body: &str,
        _op_key: &OpKey,
    ) -> Result<MessageRef, ThreadsError> {
        Err(ThreadsError::Unsupported)
    }

    async fn receipt_state(
        &self,
        _messages: &[MessageRef],
    ) -> Result<Vec<MessageReceipts>, ThreadsError> {
        Err(ThreadsError::Unsupported)
    }

    async fn delivery_capability(&self) -> Result<DeliveryCapability, ThreadsError> {
        let st = self.lock();
        Self::gate(&st)?;
        Ok(st.capability.unwrap_or(DeliveryCapability::NotifyFallback))
    }
}
