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
    /// Reserved continuity keys and the seat each is bound to.
    keys: BTreeMap<String, Option<String>>,
    /// Keys whose reservation answers `Conflict`.
    reserve_conflicts: BTreeSet<String>,
    reserve_error: Option<FakeSeatViewError>,
    /// Every `reserve_continuity_key` call that reached the service: (key, seat).
    reserves: Vec<(String, Option<String>)>,
    /// The operation key of each of those calls.
    reserve_ops: Vec<OpKey>,
    unretire_error: Option<FakeSeatViewError>,
    finalize_error: Option<FakeSeatViewError>,
    /// (call name, subject, operation key) of every unretire/finalize/release that reached the service.
    lifecycle: Vec<(String, String, OpKey)>,
}

/// The thread id the fake (and the real adapter) derive from an `ensure:<object>` operation key.
pub fn thread_for_ensure_key(op_key: &OpKey) -> ThreadRef {
    let object = op_key.0.strip_prefix("ensure:").unwrap_or(&op_key.0);
    ThreadRef(format!("hg-{}", object.to_lowercase()))
}

#[derive(Default)]
pub struct FakeThreads {
    state: Mutex<State>,
    namespace_root: Mutex<Option<std::path::PathBuf>>,
}

impl FakeThreads {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply the same external-call guard used by production wrappers in contract tests.
    pub fn enforce_namespace(&self, root: std::path::PathBuf) {
        *self.namespace_root.lock().unwrap() = Some(root);
    }
    async fn namespace_guard(&self) -> Result<Option<std::fs::File>, ThreadsError> {
        let root = self.namespace_root.lock().unwrap().clone();
        match root {
            Some(root) => crate::threads::namespace::guard(&root)
                .await
                .map(Some)
                .map_err(|e| ThreadsError::Rejected(e.to_string())),
            None => Ok(None),
        }
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

    /// The seat the reserved continuity `key` is bound to (`None`: unbound or never reserved).
    pub fn key_binding(&self, key: &str) -> Option<String> {
        self.lock().keys.get(key).cloned().flatten()
    }

    /// Whether `key` is reserved (bound or not).
    pub fn key_reserved(&self, key: &str) -> bool {
        self.lock().keys.contains_key(key)
    }

    /// Pre-bind `key` to `seat`, as if the key had been reserved and linked earlier.
    pub fn set_key_binding(&self, key: &str, seat: &str) {
        self.lock()
            .keys
            .insert(key.to_owned(), Some(seat.to_owned()));
    }

    /// Model closing `seat`'s pane (spec §14.12): the retirement is reversible only while a continuity key is
    /// bound to the seat, final otherwise. Returns the kind `view` now reports for the seat.
    pub fn close_pane(&self, view: &FakeThreadsSeatView, seat: &ThreadsSeatRef) -> RetirementKind {
        let keyed = self
            .lock()
            .keys
            .values()
            .any(|bound| bound.as_ref() == Some(&seat.0));
        let kind = if keyed {
            RetirementKind::Reversible
        } else {
            RetirementKind::Final
        };
        view.set(seat, SeatStatus::Retired { kind });
        kind
    }

    /// Model an explicit operator final retire: the seat reads final and threads deletes its key, like
    /// `finalize_retirement` does at purge.
    pub fn operator_final_retire(&self, view: &FakeThreadsSeatView, seat: &ThreadsSeatRef) {
        self.lock()
            .keys
            .retain(|_, bound| bound.as_ref() != Some(&seat.0));
        view.set(
            seat,
            SeatStatus::Retired {
                kind: RetirementKind::Final,
            },
        );
    }

    /// Reserving `key` answers `Conflict` while set.
    pub fn set_reserve_conflict(&self, key: &str) {
        self.lock().reserve_conflicts.insert(key.to_owned());
    }

    /// While set, `reserve_continuity_key` fails with this error.
    pub fn set_reserve_error(&self, error: Option<FakeSeatViewError>) {
        self.lock().reserve_error = error;
    }

    /// The operation key of every `reserve_continuity_key` call, in order.
    pub fn reserve_ops(&self) -> Vec<OpKey> {
        self.lock().reserve_ops.clone()
    }

    /// Every `reserve_continuity_key` (key, seat) that reached the service, in order, failed ones included.
    pub fn reserve_calls(&self) -> Vec<(String, Option<String>)> {
        self.lock().reserves.clone()
    }

    /// While set, `unretire_seat` fails with this error.
    pub fn set_unretire_error(&self, error: Option<FakeSeatViewError>) {
        self.lock().unretire_error = error;
    }

    /// While set, `finalize_retirement` fails with this error.
    pub fn set_finalize_error(&self, error: Option<FakeSeatViewError>) {
        self.lock().finalize_error = error;
    }

    /// The subjects (seat or key) of the lifecycle calls named `name` that reached the service, in order.
    pub fn lifecycle_calls(&self, name: &str) -> Vec<String> {
        self.lock()
            .lifecycle
            .iter()
            .filter(|(n, _, _)| n == name)
            .map(|(_, subject, _)| subject.clone())
            .collect()
    }

    /// The operation keys of the lifecycle calls named `name`, in order.
    pub fn lifecycle_ops(&self, name: &str) -> Vec<OpKey> {
        self.lock()
            .lifecycle
            .iter()
            .filter(|(n, _, _)| n == name)
            .map(|(_, _, op)| op.clone())
            .collect()
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
        let _guard = self.namespace_guard().await?;
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
        let _guard = self.namespace_guard().await?;
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
        let _guard = self.namespace_guard().await?;
        Ok(self.membership_detail(thread, seat).await?.map(|d| d.state))
    }

    async fn membership_detail(
        &self,
        thread: &ThreadRef,
        seat: &ThreadsSeatRef,
    ) -> Result<Option<MembershipDetail>, ThreadsError> {
        let _guard = self.namespace_guard().await?;
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
        let _guard = self.namespace_guard().await?;
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
        let _guard = self.namespace_guard().await?;
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
        let _guard = self.namespace_guard().await?;
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
        let _guard = self.namespace_guard().await?;
        Err(ThreadsError::Unsupported)
    }

    async fn receipt_state(
        &self,
        _messages: &[MessageRef],
    ) -> Result<Vec<MessageReceipts>, ThreadsError> {
        let _guard = self.namespace_guard().await?;
        Err(ThreadsError::Unsupported)
    }

    async fn reserve_continuity_key(
        &self,
        key: &str,
        seat: Option<&ThreadsSeatRef>,
        op: &OpKey,
    ) -> Result<ReserveOutcome, ThreadsError> {
        let _guard = self.namespace_guard().await?;
        let mut st = self.lock();
        Self::gate(&st)?;
        st.calls.push("reserve_continuity_key".into());
        st.reserves
            .push((key.to_owned(), seat.map(|s| s.0.clone())));
        st.reserve_ops.push(op.clone());
        if let Some(e) = st.reserve_error {
            return Err(e.to_error());
        }
        let wanted = seat.map(|s| s.0.clone());
        let bound = st.keys.get(key).cloned().flatten();
        let other_key_on_seat = wanted.as_ref().is_some_and(|w| {
            st.keys
                .iter()
                .any(|(k, b)| k != key && b.as_ref() == Some(w))
        });
        let bound_elsewhere = matches!((&bound, &wanted), (Some(b), Some(w)) if b != w);
        if st.reserve_conflicts.contains(key) || other_key_on_seat || bound_elsewhere {
            return Ok(ReserveOutcome::Conflict);
        }
        let binding = wanted.or(bound);
        st.keys.insert(key.to_owned(), binding.clone());
        Ok(ReserveOutcome::Reserved {
            seat: binding.map(ThreadsSeatRef),
        })
    }

    async fn unretire_seat(&self, seat: &ThreadsSeatRef, op: &OpKey) -> Result<(), ThreadsError> {
        let _guard = self.namespace_guard().await?;
        let mut st = self.lock();
        Self::gate(&st)?;
        st.calls.push("unretire_seat".into());
        st.lifecycle
            .push(("unretire_seat".into(), seat.0.clone(), op.clone()));
        match st.unretire_error {
            Some(e) => Err(e.to_error()),
            None => Ok(()),
        }
    }

    async fn finalize_retirement(
        &self,
        seat: &ThreadsSeatRef,
        op: &OpKey,
    ) -> Result<(), ThreadsError> {
        let _guard = self.namespace_guard().await?;
        let mut st = self.lock();
        Self::gate(&st)?;
        st.calls.push("finalize_retirement".into());
        st.lifecycle
            .push(("finalize_retirement".into(), seat.0.clone(), op.clone()));
        if let Some(e) = st.finalize_error {
            return Err(e.to_error());
        }
        // Threads releases the seat's key when its retirement becomes final.
        st.keys.retain(|_, bound| bound.as_ref() != Some(&seat.0));
        Ok(())
    }

    async fn release_continuity_key(&self, key: &str, op: &OpKey) -> Result<(), ThreadsError> {
        let _guard = self.namespace_guard().await?;
        let mut st = self.lock();
        Self::gate(&st)?;
        st.calls.push("release_continuity_key".into());
        st.lifecycle
            .push(("release_continuity_key".into(), key.to_owned(), op.clone()));
        match st.keys.get(key) {
            None => Err(ThreadsError::Rejected(
                "NotFound: continuity key is not reserved".into(),
            )),
            Some(Some(_)) => Err(ThreadsError::Rejected(
                "Conflict: continuity key is bound to a seat".into(),
            )),
            Some(None) => {
                st.keys.remove(key);
                Ok(())
            }
        }
    }

    async fn delivery_capability(&self) -> Result<DeliveryCapability, ThreadsError> {
        let _guard = self.namespace_guard().await?;
        let st = self.lock();
        Self::gate(&st)?;
        Ok(st.capability.unwrap_or(DeliveryCapability::NotifyFallback))
    }
}

/// Transport failure a `FakeThreadsSeatView` (or `FakeThreads`' key reservation) can be told to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeSeatViewError {
    Disconnected,
    Busy,
    Rejected,
    Unsupported,
}

impl FakeSeatViewError {
    fn to_error(self) -> ThreadsError {
        match self {
            Self::Disconnected => ThreadsError::Disconnected("fake seat view unavailable".into()),
            Self::Busy => ThreadsError::ServiceBusy,
            Self::Rejected => ThreadsError::Rejected("fake seat view rejected".into()),
            Self::Unsupported => ThreadsError::Unsupported,
        }
    }
}

#[derive(Default)]
struct SeatViewState {
    seats: BTreeMap<ThreadsSeatRef, SeatStatus>,
    error: Option<FakeSeatViewError>,
    refused: BTreeMap<ThreadsSeatRef, String>,
    reads: usize,
}

/// In-memory `ThreadsSeatView`. A seat never `set` reads `Unresolved`, so an unconfigured fake never makes
/// threads speak; an injected error fails the whole read like the real view, while a refused seat is only
/// omitted from the map (a per-seat refusal).
#[derive(Default)]
pub struct FakeThreadsSeatView {
    state: Mutex<SeatViewState>,
}

impl FakeThreadsSeatView {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(&self, seat: &ThreadsSeatRef, status: SeatStatus) {
        self.lock().seats.insert(seat.clone(), status);
    }
    pub fn set_error(&self, error: Option<FakeSeatViewError>) {
        self.lock().error = error;
    }
    /// `inspect` omits `seat` from its map, as the real view does for a per-seat refusal.
    pub fn refuse(&self, seat: &ThreadsSeatRef, message: &str) {
        self.lock().refused.insert(seat.clone(), message.to_owned());
    }
    pub fn clear_refusal(&self, seat: &ThreadsSeatRef) {
        self.lock().refused.remove(seat);
    }
    /// Number of `inspect` calls so far, failed ones included.
    pub fn reads(&self) -> usize {
        self.lock().reads
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, SeatViewState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait::async_trait]
impl ThreadsSeatView for FakeThreadsSeatView {
    async fn inspect(
        &self,
        seats: &[ThreadsSeatRef],
    ) -> Result<BTreeMap<ThreadsSeatRef, SeatStatus>, ThreadsError> {
        let mut st = self.lock();
        st.reads += 1;
        if let Some(e) = st.error {
            return Err(e.to_error());
        }
        Ok(seats
            .iter()
            .filter(|s| !st.refused.contains_key(s))
            .map(|s| {
                (
                    s.clone(),
                    st.seats.get(s).cloned().unwrap_or(SeatStatus::Unresolved),
                )
            })
            .collect())
    }
}
