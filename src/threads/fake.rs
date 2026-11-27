//! In-memory `ThreadsPort` for tests (spec §11). Behaves like the real service where it matters to graph:
//! operation keys are idempotent, a Required invitation stays `pending` until `accept` simulates the native
//! agent, and nothing ever fabricates acceptance.
use crate::model::clone::{InvitationState, InviteConstraint};
use crate::model::{HerdrPaneId, Timestamp};
use crate::ports::clock::Clock;
use crate::ports::threads::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

/// One retirement the seat-lifecycle model tracks (threads 3.5.0 reversible close).
#[derive(Debug, Clone, PartialEq, Eq)]
struct FakeRetirement {
    keyed: bool,
    retired_at: Timestamp,
    /// The 24 h obligation settlement already ran.
    settled: bool,
    final_: bool,
}

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
    /// Successful D1/D2 answers by operation key, with the payload each answered. Threads v3.1.0 stores the
    /// first successful result per (actor scope, operation key) and replays it for the same payload
    /// (store/service_controls.rs); a refusal is not stored. The key/seat state is not recomputed on replay.
    stored: BTreeMap<String, (String, StoredAnswer)>,
    release_error: Option<FakeSeatViewError>,
    /// Seats a pane close or an operator retire left retired, until an unretire. Threads v3.1.0 reads every
    /// Pending or Accepted membership of a retired seat (reversible or final) as Retired, while the stored
    /// episode stays as it was (store/service_substrate.rs `current_requirement`, queries.rs participants).
    retired_seats: BTreeSet<String>,
    /// The opt-in 3.5.0/3.6.0 seat-lifecycle model (`enable_resurrect`): the seat view and the injectable clock.
    model: Option<(Arc<FakeThreadsSeatView>, Arc<dyn Clock>)>,
    retirements: BTreeMap<String, FakeRetirement>,
    /// Restart-held seats and the pane each is saved on.
    held: BTreeMap<String, HerdrPaneId>,
    /// Seats threads minted bare for a pane (the `replace` of a ResurrectSeat).
    bare: BTreeSet<String>,
    /// Seats keyed by another service.
    foreign_keyed: BTreeSet<String>,
    reconcile_pending: bool,
    /// Invitations `invite` minted, per (thread, seat); a reused pending invitation mints none.
    minted: BTreeMap<(String, String), usize>,
}

/// Apply the 24 h and 7 d windows of every reversible keyless retirement at the model clock's now.
fn settle(st: &mut State) {
    let Some((view, clock)) = st.model.clone() else {
        return;
    };
    let now = clock.now();
    let seats: Vec<String> = st.retirements.keys().cloned().collect();
    for seat in seats {
        let r = st.retirements[&seat].clone();
        if r.keyed || r.final_ {
            continue;
        }
        if !r.settled && now >= r.retired_at + KEYLESS_OBLIGATIONS_SETTLE_AFTER {
            // Threads settled the ordinary pending invitations as recipient_retired; accepted memberships and
            // required episodes stay.
            st.members.retain(|(_, s), m| {
                !(s == &seat
                    && m.constraint == InviteConstraint::Ordinary
                    && m.state == InvitationState::Pending)
            });
            st.retirements.get_mut(&seat).unwrap().settled = true;
        }
        if now >= r.retired_at + KEYLESS_REVERSIBLE_FOR {
            st.retirements.get_mut(&seat).unwrap().final_ = true;
            view.set(
                &ThreadsSeatRef(seat),
                SeatStatus::Retired {
                    kind: RetirementKind::Final,
                    keyed: false,
                    retired_at: Some(r.retired_at),
                },
            );
        }
    }
}

#[derive(Clone)]
enum StoredAnswer {
    Reserved(Option<String>),
    Done,
}

/// The stored answer for `op`, if any: `Err` when the key was used with another payload.
fn replay(st: &State, op: &OpKey, payload: &str) -> Option<Result<StoredAnswer, ThreadsError>> {
    st.stored.get(&op.0).map(|(p, a)| {
        if p == payload {
            Ok(a.clone())
        } else {
            Err(ThreadsError::Rejected(
                "OperationPayloadMismatch: operation key reused with different payload".into(),
            ))
        }
    })
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
                .map_err(|e| ThreadsError::Disconnected(format!("graph-local: {e}"))),
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
    /// With `enable_resurrect` every close is reversible (threads 3.5.0), keyed or not, and records the retirement
    /// time.
    pub fn close_pane(&self, view: &FakeThreadsSeatView, seat: &ThreadsSeatRef) -> RetirementKind {
        let mut st = self.lock();
        let keyed = st
            .keys
            .values()
            .any(|bound| bound.as_ref() == Some(&seat.0));
        let model_now = st.model.as_ref().map(|(_, c)| c.now());
        let kind = if keyed || model_now.is_some() {
            RetirementKind::Reversible
        } else {
            RetirementKind::Final
        };
        if let Some(now) = model_now {
            st.retirements.insert(
                seat.0.clone(),
                FakeRetirement {
                    keyed,
                    retired_at: now,
                    settled: false,
                    final_: false,
                },
            );
        }
        st.retired_seats.insert(seat.0.clone());
        drop(st);
        view.set(
            seat,
            SeatStatus::Retired {
                kind,
                keyed,
                retired_at: model_now,
            },
        );
        kind
    }

    /// Model an explicit operator final retire: the seat reads final and threads deletes its key, like
    /// `finalize_retirement` does at purge.
    pub fn operator_final_retire(&self, view: &FakeThreadsSeatView, seat: &ThreadsSeatRef) {
        let mut st = self.lock();
        st.keys.retain(|_, bound| bound.as_ref() != Some(&seat.0));
        st.retired_seats.insert(seat.0.clone());
        let now = st.model.as_ref().map(|(_, c)| c.now());
        if let Some(r) = st.retirements.get_mut(&seat.0) {
            r.final_ = true;
        }
        drop(st);
        view.set(
            seat,
            SeatStatus::Retired {
                kind: RetirementKind::Final,
                keyed: false,
                retired_at: now,
            },
        );
    }

    /// Turn on the threads 3.5.0/3.6.0 seat-lifecycle model: `view` is the seat view the fake keeps current and
    /// `clock` times the 24 h and 7 d windows. Without it `resurrect_seat` and `rebind_held_seat` answer
    /// `Unsupported`.
    pub fn enable_resurrect(&self, view: Arc<FakeThreadsSeatView>, clock: Arc<dyn Clock>) {
        self.lock().model = Some((view, clock));
    }

    /// Apply the 24 h and 7 d windows at the model clock's now (`resurrect_seat` runs this first).
    pub fn settle_windows(&self) {
        settle(&mut self.lock());
    }

    /// Hold `seat` for a Herdr restart: it reads `Unresolved` and `pane` is its saved pane.
    pub fn hold_for_restart(&self, seat: &ThreadsSeatRef, pane: &HerdrPaneId) {
        let mut st = self.lock();
        if let Some((view, _)) = &st.model {
            view.set(seat, SeatStatus::Unresolved);
        }
        st.held.insert(seat.0.clone(), pane.clone());
    }

    /// `seat` is a bare seat threads minted for `pane`: it reads `Resolved` there.
    pub fn set_bare_seat(&self, seat: &ThreadsSeatRef, pane: &HerdrPaneId) {
        let mut st = self.lock();
        if let Some((view, _)) = &st.model {
            view.set(seat, SeatStatus::Resolved { pane: pane.clone() });
        }
        st.bare.insert(seat.0.clone());
    }

    /// `seat` is keyed by another service: ResurrectSeat on it answers `Unauthorized`.
    pub fn set_foreign_key(&self, seat: &ThreadsSeatRef) {
        self.lock().foreign_keyed.insert(seat.0.clone());
    }

    /// While set, restoring a held seat answers `ServiceBusy` (reconciliation pending).
    pub fn set_reconcile_pending(&self, pending: bool) {
        self.lock().reconcile_pending = pending;
    }

    /// Shared core of `resurrect_seat` and `rebind_held_seat`; `name` is the call recorded.
    async fn restore(
        &self,
        name: &str,
        seat: &ThreadsSeatRef,
        pane: &HerdrPaneId,
        replace: Option<&ThreadsSeatRef>,
        op: &OpKey,
    ) -> SeatRestoreOutcome {
        use SeatRestoreRefusal as R;
        let _guard = match self.namespace_guard().await {
            Ok(g) => g,
            Err(e) => return SeatRestoreOutcome::from_error(&e),
        };
        let mut st = self.lock();
        if let Err(e) = Self::gate(&st) {
            return SeatRestoreOutcome::from_error(&e);
        }
        st.calls.push(name.into());
        st.lifecycle.push((name.into(), seat.0.clone(), op.clone()));
        let Some((view, clock)) = st.model.clone() else {
            return SeatRestoreOutcome::refused(R::Unsupported);
        };
        let payload = format!(
            "resurrect|{}|{}|{}",
            seat.0,
            pane.0,
            replace.map_or("-", |r| r.0.as_str())
        );
        if let Some(r) = replay(&st, op, &payload) {
            return match r {
                Ok(_) => SeatRestoreOutcome::Done,
                Err(e) => SeatRestoreOutcome::from_error(&e),
            };
        }
        settle(&mut st);
        if st.foreign_keyed.contains(&seat.0) {
            return SeatRestoreOutcome::refused(R::Unauthorized);
        }
        if replace.is_none()
            && view.status(seat) == Some(SeatStatus::Resolved { pane: pane.clone() })
        {
            return SeatRestoreOutcome::AlreadyResolved;
        }
        let done = |st: &mut State| {
            st.stored
                .insert(op.0.clone(), (payload.clone(), StoredAnswer::Done));
            SeatRestoreOutcome::Done
        };
        if let Some(saved) = st.held.get(&seat.0).cloned() {
            if replace.is_some() {
                return SeatRestoreOutcome::refused(R::InvalidRequest);
            }
            if &saved != pane {
                return SeatRestoreOutcome::refused(R::NotFound);
            }
            if st.reconcile_pending {
                return SeatRestoreOutcome::refused(R::ServiceBusy);
            }
            view.set(seat, SeatStatus::Resolved { pane: pane.clone() });
            st.held.remove(&seat.0);
            return done(&mut st);
        }
        if st.retirements.get(&seat.0).is_some_and(|r| !r.final_) {
            let owners: Vec<ThreadsSeatRef> = view.resolved_on(pane);
            if let Some(r) = replace
                && !(st.bare.contains(&r.0) && owners.contains(r))
            {
                return SeatRestoreOutcome::refused(R::InvalidRequest);
            }
            if owners.iter().any(|o| Some(o) != replace) {
                return SeatRestoreOutcome::refused(R::TargetAlreadyOwned);
            }
            st.retirements.remove(&seat.0);
            st.retired_seats.remove(&seat.0);
            view.set(seat, SeatStatus::Resolved { pane: pane.clone() });
            if let Some(r) = replace {
                let now = clock.now();
                view.set(
                    r,
                    SeatStatus::Retired {
                        kind: RetirementKind::Final,
                        keyed: false,
                        retired_at: Some(now),
                    },
                );
                st.bare.remove(&r.0);
                st.retired_seats.insert(r.0.clone());
            }
            return done(&mut st);
        }
        SeatRestoreOutcome::refused(R::NotFound)
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

    /// While set, `release_continuity_key` fails with this error.
    pub fn set_release_error(&self, error: Option<FakeSeatViewError>) {
        self.lock().release_error = error;
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

    /// How many invitations `invite` minted for `seat` on `thread` (an invite onto a pending one mints none).
    pub fn minted_invitations(&self, thread: &ThreadRef, seat: &ThreadsSeatRef) -> usize {
        self.lock()
            .minted
            .get(&(thread.0.clone(), seat.0.clone()))
            .copied()
            .unwrap_or(0)
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
        *st.minted.entry(key.clone()).or_default() += 1;
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
        let retired = st.retired_seats.contains(&seat.0);
        Ok(st
            .members
            .get(&(thread.0.clone(), seat.0.clone()))
            .map(|m| MembershipDetail {
                state: match m.state {
                    InvitationState::Pending | InvitationState::Accepted if retired => {
                        InvitationState::Retired
                    }
                    s => s,
                },
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
        let payload = format!("reserve|{key}|{:?}", seat.map(|s| &s.0));
        if let Some(r) = replay(&st, op, &payload) {
            return r.map(|a| match a {
                StoredAnswer::Reserved(b) => ReserveOutcome::Reserved {
                    seat: b.map(ThreadsSeatRef),
                },
                StoredAnswer::Done => ReserveOutcome::Reserved { seat: None },
            });
        }
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
        st.stored.insert(
            op.0.clone(),
            (payload, StoredAnswer::Reserved(binding.clone())),
        );
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
        let payload = format!("unretire|{}", seat.0);
        if let Some(r) = replay(&st, op, &payload) {
            return r.map(|_| ());
        }
        match st.unretire_error {
            Some(e) => Err(e.to_error()),
            None => {
                st.stored
                    .insert(op.0.clone(), (payload, StoredAnswer::Done));
                st.retired_seats.remove(&seat.0);
                // With the seat-lifecycle model on, threads no longer reads the seat retired: it is unbound
                // until its key checks in again.
                st.retirements.remove(&seat.0);
                if let Some((view, _)) = &st.model {
                    view.set(seat, SeatStatus::Unresolved);
                }
                Ok(())
            }
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
        let payload = format!("finalize|{}", seat.0);
        if let Some(r) = replay(&st, op, &payload) {
            return r.map(|_| ());
        }
        if let Some(e) = st.finalize_error {
            return Err(e.to_error());
        }
        // Threads releases the seat's key when its retirement becomes final.
        st.keys.retain(|_, bound| bound.as_ref() != Some(&seat.0));
        st.stored
            .insert(op.0.clone(), (payload, StoredAnswer::Done));
        Ok(())
    }

    async fn resurrect_seat(
        &self,
        seat: &ThreadsSeatRef,
        pane: &HerdrPaneId,
        replace: Option<&ThreadsSeatRef>,
        op: &OpKey,
    ) -> SeatRestoreOutcome {
        self.restore("resurrect_seat", seat, pane, replace, op)
            .await
    }

    async fn rebind_held_seat(
        &self,
        seat: &ThreadsSeatRef,
        pane: &HerdrPaneId,
        op: &OpKey,
    ) -> SeatRestoreOutcome {
        self.restore("rebind_held_seat", seat, pane, None, op).await
    }

    async fn release_continuity_key(&self, key: &str, op: &OpKey) -> Result<(), ThreadsError> {
        let _guard = self.namespace_guard().await?;
        let mut st = self.lock();
        Self::gate(&st)?;
        st.calls.push("release_continuity_key".into());
        st.lifecycle
            .push(("release_continuity_key".into(), key.to_owned(), op.clone()));
        let payload = format!("release|{key}");
        if let Some(r) = replay(&st, op, &payload) {
            return r.map(|_| ());
        }
        if let Some(e) = st.release_error {
            return Err(e.to_error());
        }
        match st.keys.get(key) {
            None => Err(ThreadsError::Rejected(
                "NotFound: continuity key is not reserved".into(),
            )),
            Some(Some(_)) => Err(ThreadsError::Rejected(
                "Conflict: continuity key is bound to a seat".into(),
            )),
            Some(None) => {
                st.keys.remove(key);
                st.stored
                    .insert(op.0.clone(), (payload, StoredAnswer::Done));
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
    /// The status last `set` for `seat` (`None`: never set).
    pub fn status(&self, seat: &ThreadsSeatRef) -> Option<SeatStatus> {
        self.lock().seats.get(seat).cloned()
    }
    /// Every seat currently set `Resolved` on `pane`.
    pub fn resolved_on(&self, pane: &HerdrPaneId) -> Vec<ThreadsSeatRef> {
        self.lock()
            .seats
            .iter()
            .filter(|(_, s)| matches!(s, SeatStatus::Resolved { pane: p } if p == pane))
            .map(|(k, _)| k.clone())
            .collect()
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
