//! Threads integration tests: FakeThreads + FakeHerdr + the reconciler + the real writer, journal and git store.
use super::effects::derived_thread;
use super::*;
use crate::daemon::registry::CallerInfo;
use crate::herdr::fake::FakeHerdr;
use crate::journal::Journal;
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::clone::{CloneRecord, InvitationState, InviteConstraint};
use crate::model::common::{Lifecycle, Occupant};
use crate::model::effect::{EffectKind, EffectRecord, EffectStatus};
use crate::model::harness::Harness;
use crate::model::native_session::NativeSession;
use crate::model::operation::OpState;
use crate::model::seat::SeatRecord;
use crate::model::{HerdrPaneId, NsId, OpId, Timestamp};
use crate::plan::commands::{PlanDeps, admit_apply, create_plan};
use crate::plan::core_kinds::register_core_kinds;
use crate::plan::kind::KindRegistry;
use crate::plan::store::PlanStore;
use crate::ports::clock::ManualClock;
use crate::ports::threads::*;
use crate::ports::writer::Writer;
use crate::reconcile::{Reconciler, ReconcilerConfig, RequesterNotifier, StepReport};
use crate::store::GitStore;
use crate::store::init::init_instance;
use crate::store::layout;
use crate::writer::{WriterConfig, WriterCore};
use chrono::TimeZone;
use serde_json::json;
use std::sync::Arc;

fn t0() -> Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

struct Quiet;
impl RequesterNotifier for Quiet {
    fn notify(&self, _op: &OpId, _severity: Severity, _text: &str) {}
}

/// Test-only occupancy writers: the real differ (hg-zmi.8) owns `observed.occupancy`.
struct SetOccupant;
impl Mutation for SetOccupant {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let clone: CloneId = cx.request.args["clone"].as_str().unwrap().parse().unwrap();
        let loc = cx.tree.locate(&clone.to_any())?.unwrap();
        let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        let ns = NativeSession {
            id: NsId::new(),
            harness: Harness::Shell,
            native_session_id: format!("native-{}", rec.sessions.len() + 1),
            transcript_path: None,
            transcript: None,
            cwd: "/".into(),
            started: cx.now,
            ended: None,
            end_reason: None,
        };
        rec.occupant = Some(Occupant { native_session: ns.id.clone(), harness: Harness::Shell, since: cx.now });
        rec.sessions.push(ns);
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "set occupant".into(), action: None })
    }
}

struct ClearOccupant;
impl Mutation for ClearOccupant {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let clone: CloneId = cx.request.args["clone"].as_str().unwrap().parse().unwrap();
        let loc = cx.tree.locate(&clone.to_any())?.unwrap();
        let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        rec.occupant = None;
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "clear occupant".into(), action: None })
    }
}

struct SetReload;
impl Mutation for SetReload {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let seat: SeatId = cx.request.args["seat"].as_str().unwrap().parse().unwrap();
        let loc = cx.tree.locate(&seat.to_any())?.unwrap();
        let mut rec: SeatRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        rec.reload_required = cx.request.args["on"].as_bool().unwrap();
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "set reload".into(), action: None })
    }
}

struct Fx {
    _tmp: tempfile::TempDir,
    deps: PlanDeps,
    w: Arc<WriterCore>,
    store: Arc<GitStore>,
    journal: Arc<Journal>,
    clock: Arc<ManualClock>,
    rec: Arc<Reconciler>,
    threads: Arc<FakeThreads>,
    map: Arc<FakePaneSeatMap>,
}

fn fx() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("inst");
    init_instance(&root).unwrap();
    let mut kinds = KindRegistry::default();
    register_core_kinds(&mut kinds);
    crate::plan::kinds_extra::register_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    let plans = Arc::new(PlanStore::new(root.join(".graph-local/plans")));
    let mut reg = MutationRegistry::default();
    kinds.register_mutations(&mut reg, plans.clone());
    crate::reconcile::register_mutations(&mut reg);
    register_mutations(&mut reg);
    reg.register("bookkeeping.test_set_occupant", Arc::new(SetOccupant));
    reg.register("bookkeeping.test_clear_occupant", Arc::new(ClearOccupant));
    reg.register("bookkeeping.test_set_reload", Arc::new(SetReload));
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let w = WriterCore::new(store.clone(), journal.clone(), Arc::new(reg), clock.clone(), WriterConfig::default());
    let deps = PlanDeps { kinds, plans, store: store.clone(), writer: w.clone(), clock: clock.clone(), instance: root.clone() };
    let herdr = FakeHerdr::new();
    let cfg = ReconcilerConfig::new(root.clone());
    let rec = Reconciler::new(store.clone(), journal.clone(), w.clone(), herdr, clock.clone(), Arc::new(Quiet), cfg);
    let threads = Arc::new(FakeThreads::new());
    let map = Arc::new(FakePaneSeatMap::new());
    register_with(&rec, threads.clone(), map.clone());
    Fx { _tmp: tmp, deps, w, store, journal, clock, rec, threads, map }
}

fn plan_and_apply(fx: &Fx, change: &str) {
    let words = change.split_whitespace().map(str::to_owned).collect();
    let v = create_plan(&fx.deps, &CallerInfo::default(), words).unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: crate::model::PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    let sp = fx.deps.plans.get(&id).unwrap().unwrap();
    let op = admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay")
        .unwrap_or_else(|e| panic!("admit {change}: {}", e.message));
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(row.state, OpState::Committed, "{change}: {:?}", row.rejection);
}

fn bookkeeping(fx: &Fx, sub: &str, mut args: serde_json::Value) {
    args["sub"] = json!(sub);
    let req = ChangeRequest { kind: RequestKind::Bookkeeping, args, relied_on: vec![], requester: Requester::default(), supersedes: None };
    fx.w.admit(req).unwrap();
    fx.w.drain().unwrap();
}

fn view(fx: &Fx) -> crate::store::tree::CommitView<'_> {
    crate::store::tree::CommitView { store: &*fx.store, at: fx.store.head().unwrap() }
}

fn seat(fx: &Fx, name: &str) -> SeatRecord {
    let mut found: Vec<_> = layout::all_seats(&view(fx)).unwrap().into_iter().filter(|(_, s)| s.name == name).collect();
    assert_eq!(found.len(), 1, "seat {name}");
    found.remove(0).1
}

fn teamspace(fx: &Fx) -> crate::model::teamspace::TeamspaceRecord {
    layout::list_teamspaces(&view(fx)).unwrap().remove(0).1
}

fn clones_of(fx: &Fx, seat: &SeatId) -> Vec<CloneRecord> {
    let mut v: Vec<_> = layout::all_clones(&view(fx)).unwrap().into_iter().map(|(_, c)| c).filter(|c| &c.seat == seat).collect();
    v.sort_by_key(|c| c.id.clone());
    v
}

fn clone_rec(fx: &Fx, id: &CloneId) -> CloneRecord {
    layout::all_clones(&view(fx)).unwrap().into_iter().map(|(_, c)| c).find(|c| &c.id == id).unwrap()
}

fn pane_of(c: &CloneRecord) -> HerdrPaneId {
    c.runtime.bound.as_ref().and_then(|b| b.pane_id.clone()).expect("clone has a pane binding")
}

async fn step(fx: &Fx) -> StepReport {
    let r = fx.rec.step_fresh().await;
    fx.w.drain().unwrap();
    r
}

async fn steps(fx: &Fx, n: usize) {
    for _ in 0..n {
        step(fx).await;
    }
}

/// Let the reconciler's backoff expire, then step.
async fn later(fx: &Fx) {
    fx.clock.advance(chrono::Duration::minutes(10));
    step(fx).await;
}

/// A teamspace `alpha` with the active shell seat `foreman`, its panes created and bound.
async fn base(fx: &Fx) -> CloneRecord {
    plan_and_apply(fx, "teamspace create alpha");
    plan_and_apply(fx, "seat create foreman --teamspace alpha --active --harness shell");
    steps(fx, 3).await;
    let s = seat(fx, "foreman");
    let mut cs = clones_of(fx, &s.id);
    assert_eq!(cs.len(), 1);
    cs.remove(0)
}

/// The clone gets an occupant and its pane is registered as `threads_seat` with herdr-threads.
async fn occupy(fx: &Fx, clone: &CloneRecord, threads_seat: &str) {
    bookkeeping(fx, "test_set_occupant", json!({ "clone": clone.id }));
    fx.map.set(&pane_of(&clone_rec(fx, &clone.id)), threads_seat);
}

fn thread_of_seat(fx: &Fx, name: &str) -> ThreadRef {
    ThreadRef(seat(fx, name).channel.thread_id.expect("seat channel ensured"))
}

fn thread_of_ts(fx: &Fx) -> ThreadRef {
    ThreadRef(teamspace(fx).channel.thread_id.expect("teamspace channel ensured"))
}

fn inv<'a>(c: &'a CloneRecord, thread: &ThreadRef) -> &'a crate::model::clone::Invitation {
    c.invitations.iter().find(|i| i.thread == thread.0).unwrap_or_else(|| panic!("no invitation to {}: {:?}", thread.0, c.invitations))
}

fn rows(fx: &Fx, kind: EffectKind) -> Vec<EffectRecord> {
    let all = [
        EffectStatus::Pending,
        EffectStatus::Done,
        EffectStatus::Obsolete,
        EffectStatus::Failed,
        EffectStatus::Unknown,
        EffectStatus::NeedsRevision,
        EffectStatus::BlockedNeedsHuman,
    ];
    fx.journal.effects_with_status(&all).unwrap().into_iter().filter(|r| r.kind == kind).collect()
}

fn seat_ref(s: &str) -> ThreadsSeatRef {
    ThreadsSeatRef(s.to_owned())
}

// ---------------------------------------------------------------------------------------------
// channels
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn ensure_thread_for_active_seat_and_teamspace_once() {
    let fx = fx();
    base(&fx).await;
    steps(&fx, 3).await;
    let seat_thread = thread_of_seat(&fx, "foreman");
    let ts_thread = thread_of_ts(&fx);
    assert_eq!(seat_thread, ThreadRef(derived_thread(&seat(&fx, "foreman").id.to_any())));
    assert_ne!(seat_thread, ts_thread);
    assert_eq!(fx.threads.call_count("ensure_thread"), 2, "one per channel, however many steps ran");
    assert_eq!(fx.threads.thread(&seat_thread).unwrap().topic, "alpha/foreman");
    assert_eq!(fx.threads.thread(&ts_thread).unwrap().topic, "alpha");
    assert_eq!(rows(&fx, EffectKind::EnsureThread).len(), 2);
    assert!(rows(&fx, EffectKind::EnsureThread).iter().all(|r| r.status == EffectStatus::Done));
}

#[tokio::test]
async fn dormant_seats_get_no_channel() {
    let fx = fx();
    plan_and_apply(&fx, "teamspace create alpha");
    plan_and_apply(&fx, "seat create idle --teamspace alpha --harness shell");
    steps(&fx, 2).await;
    assert!(seat(&fx, "idle").channel.thread_id.is_none());
    assert_eq!(fx.threads.call_count("ensure_thread"), 0, "no active seat and no active teamspace yet");
}

#[tokio::test]
async fn set_topic_on_rename() {
    let fx = fx();
    base(&fx).await;
    steps(&fx, 2).await;
    let seat_thread = thread_of_seat(&fx, "foreman");
    let ts_thread = thread_of_ts(&fx);
    plan_and_apply(&fx, "seat rename foreman boss");
    steps(&fx, 2).await;
    assert_eq!(fx.threads.thread(&seat_thread).unwrap().topic, "alpha/boss");
    assert_eq!(fx.threads.call_count("set_topic"), 1);
    plan_and_apply(&fx, "teamspace rename alpha beta");
    steps(&fx, 2).await;
    assert_eq!(fx.threads.thread(&ts_thread).unwrap().topic, "beta");
    assert_eq!(fx.threads.thread(&seat_thread).unwrap().topic, "beta/boss", "the seat topic carries the teamspace name");
    steps(&fx, 2).await;
    assert_eq!(fx.threads.call_count("set_topic"), 3, "settled topics are not set again");
}

// ---------------------------------------------------------------------------------------------
// required invitations and membership
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn invite_required_for_occupied_clone_on_seat_and_teamspace_channels() {
    let fx = fx();
    let clone = base(&fx).await;
    steps(&fx, 2).await;
    assert_eq!(fx.threads.call_count("invite"), 0, "no occupant, no invitation");
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    for t in [&st, &ts] {
        let m = fx.threads.member(t, &seat_ref("seat-A")).unwrap_or_else(|| panic!("seat-A not invited to {}", t.0));
        assert_eq!(m.constraint, InviteConstraint::Required);
        assert_eq!(m.state, InvitationState::Pending);
    }
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.invitations.len(), 2);
    for t in [&st, &ts] {
        let i = inv(&c, t);
        assert_eq!((i.constraint, i.state), (InviteConstraint::Required, InvitationState::Pending));
        let link = i.link.as_ref().unwrap();
        assert_eq!(link.seat, "seat-A");
        assert_eq!(link.occupant, c.occupant.as_ref().map(|o| o.native_session.to_string()));
        assert!(link.requirement.is_some() && link.revision == Some(1));
    }
    steps(&fx, 3).await;
    assert_eq!(fx.threads.call_count("invite"), 2, "an invitation is not repeated");
}

#[tokio::test]
async fn invite_waits_for_the_pane_to_become_a_threads_seat() {
    let fx = fx();
    let clone = base(&fx).await;
    bookkeeping(&fx, "test_set_occupant", json!({ "clone": clone.id }));
    steps(&fx, 3).await;
    assert_eq!(fx.threads.call_count("invite"), 0, "the pane has no threads seat yet");
    let invites = rows(&fx, EffectKind::Invite);
    assert_eq!(invites.len(), 2);
    assert!(invites.iter().all(|r| r.status == EffectStatus::Pending && r.attempts == 0), "deferred, not failed or counted");
    fx.map.set(&pane_of(&clone_rec(&fx, &clone.id)), "seat-A");
    steps(&fx, 2).await;
    assert_eq!(fx.threads.call_count("invite"), 2);
}

#[tokio::test]
async fn acceptance_never_fabricated() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    for _ in 0..8 {
        later(&fx).await;
    }
    let c = clone_rec(&fx, &clone.id);
    assert!(c.invitations.iter().all(|i| i.state == InvitationState::Pending), "{:?}", c.invitations);
    for t in [thread_of_seat(&fx, "foreman"), thread_of_ts(&fx)] {
        assert_eq!(fx.threads.member(&t, &seat_ref("seat-A")).unwrap().state, InvitationState::Pending);
    }
}

#[tokio::test]
async fn membership_records_accepted_after_native_accept() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    fx.threads.accept(&st, &seat_ref("seat-A"));
    later(&fx).await;
    later(&fx).await;
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(inv(&c, &st).state, InvitationState::Accepted, "recorded because threads reported it");
    assert_eq!(inv(&c, &ts).state, InvitationState::Pending, "the other episode was not accepted");
    fx.threads.accept(&ts, &seat_ref("seat-A"));
    later(&fx).await;
    later(&fx).await;
    let c = clone_rec(&fx, &clone.id);
    assert!(c.invitations.iter().all(|i| i.state == InvitationState::Accepted));
    assert!(crate::threads::pending_invitations(&view(&fx), &clone.id).is_empty());
}

#[tokio::test]
async fn pending_invitations_lists_accept_commands() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    let pending = crate::threads::pending_invitations(&view(&fx), &clone.id);
    assert_eq!(pending.len(), 2);
    let m = fx.threads.member(&st, &seat_ref("seat-A")).unwrap();
    let want = format!(
        "herdr-threads accept-required {} --invitation {} --requirement {} --revision {}",
        st.0,
        m.invitation,
        m.requirement.as_ref().unwrap(),
        m.revision
    );
    let got = pending.iter().find(|p| p.thread == st.0).unwrap();
    assert_eq!(got.constraint, InviteConstraint::Required);
    assert_eq!(got.accept_command, want);
    assert!(pending.iter().any(|p| p.thread == ts.0));
    // An accepted invitation is no longer pending.
    fx.threads.accept(&st, &seat_ref("seat-A"));
    later(&fx).await;
    later(&fx).await;
    let pending = crate::threads::pending_invitations(&view(&fx), &clone.id);
    assert_eq!(pending.iter().map(|p| p.thread.clone()).collect::<Vec<_>>(), vec![ts.0.clone()]);
    // Ordinary invitations are accepted with plain `accept`.
    use crate::model::clone::Invitation;
    let ordinary = Invitation { thread: "th-x".into(), constraint: InviteConstraint::Ordinary, state: InvitationState::Pending, link: None };
    assert_eq!(super::accept_command(&ordinary.thread, ordinary.constraint, None), "herdr-threads accept th-x");
    assert_eq!(
        super::accept_command("th-y", InviteConstraint::Required, None),
        "herdr-threads thread participants th-y",
        "episode ids unknown: read them from the thread"
    );
    assert!(crate::threads::pending_invitations(&view(&fx), &CloneId::new()).is_empty());
}

// ---------------------------------------------------------------------------------------------
// occupancy-keyed cleanup
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn occupancy_end_releases_requirement() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    fx.threads.accept(&st, &seat_ref("seat-A"));
    later(&fx).await;
    bookkeeping(&fx, "test_clear_occupant", json!({ "clone": clone.id }));
    steps(&fx, 2).await;
    for t in [&st, &ts] {
        assert_eq!(fx.threads.member(t, &seat_ref("seat-A")).unwrap().state, InvitationState::Released, "{}", t.0);
    }
    assert_eq!(fx.threads.call_count("release_requirement"), 2);
    let c = clone_rec(&fx, &clone.id);
    assert!(c.invitations.iter().all(|i| i.state == InvitationState::Released), "{:?}", c.invitations);
    steps(&fx, 3).await;
    assert_eq!(fx.threads.call_count("release_requirement"), 2, "released once");
    assert_eq!(fx.threads.call_count("invite"), 2, "nobody to re-invite");
}

#[tokio::test]
async fn new_occupant_reinvited() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    bookkeeping(&fx, "test_clear_occupant", json!({ "clone": clone.id }));
    steps(&fx, 2).await;
    // A different agent starts in the same pane and registers as another threads seat.
    occupy(&fx, &clone, "seat-B").await;
    steps(&fx, 2).await;
    for t in [&st, &ts] {
        let a = fx.threads.member(t, &seat_ref("seat-A")).unwrap();
        let b = fx.threads.member(t, &seat_ref("seat-B")).unwrap();
        assert_eq!(a.state, InvitationState::Released);
        assert_eq!((b.constraint, b.state), (InviteConstraint::Required, InvitationState::Pending));
    }
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(inv(&c, &st).link.as_ref().unwrap().seat, "seat-B");
    assert_eq!(inv(&c, &st).state, InvitationState::Pending);
}

#[tokio::test]
async fn occupant_replaced_in_one_commit_releases_before_reinviting_the_same_seat() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    fx.threads.accept(&thread_of_seat(&fx, "foreman"), &seat_ref("seat-A"));
    later(&fx).await;
    // The occupant changes without an empty moment; the pane (and so the threads seat) stays.
    bookkeeping(&fx, "test_set_occupant", json!({ "clone": clone.id }));
    steps(&fx, 3).await;
    let st = thread_of_seat(&fx, "foreman");
    let m = fx.threads.member(&st, &seat_ref("seat-A")).unwrap();
    assert_eq!(m.state, InvitationState::Pending, "a fresh episode for the new occupant, not the old release");
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(inv(&c, &st).state, InvitationState::Pending);
    assert_eq!(inv(&c, &st).link.as_ref().unwrap().occupant, c.occupant.as_ref().map(|o| o.native_session.to_string()));
}

#[tokio::test]
async fn clone_retirement_releases() {
    let fx = fx();
    let first = base(&fx).await;
    plan_and_apply(&fx, "clone add foreman --name second");
    steps(&fx, 3).await;
    let both = clones_of(&fx, &first.seat);
    assert_eq!(both.len(), 2);
    occupy(&fx, &both[0], "seat-A").await;
    occupy(&fx, &both[1], "seat-B").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    for t in [&st, &ts] {
        for s in ["seat-A", "seat-B"] {
            assert_eq!(fx.threads.member(t, &seat_ref(s)).unwrap().state, InvitationState::Pending);
        }
    }
    plan_and_apply(&fx, &format!("clone retire {}", both[0].id));
    steps(&fx, 3).await;
    for t in [&st, &ts] {
        assert_eq!(fx.threads.member(t, &seat_ref("seat-A")).unwrap().state, InvitationState::Released, "retired clone");
        assert_eq!(fx.threads.member(t, &seat_ref("seat-B")).unwrap().state, InvitationState::Pending, "other clone untouched");
    }
    let c = clone_rec(&fx, &both[0].id);
    assert!(c.invitations.iter().all(|i| i.state == InvitationState::Released));
}

// ---------------------------------------------------------------------------------------------
// participation
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn ordinary_invite_for_seat_wide_participation_respects_opt_outs() {
    let fx = fx();
    let first = base(&fx).await;
    plan_and_apply(&fx, "clone add foreman --name second");
    steps(&fx, 3).await;
    let both = clones_of(&fx, &first.seat);
    occupy(&fx, &both[0], "seat-A").await;
    occupy(&fx, &both[1], "seat-B").await;
    fx.threads.add_thread("th_a", "shared");
    let th = ThreadRef("th_a".into());
    plan_and_apply(&fx, &format!("participation leave th_a --scope clone --clone {}", both[1].id));
    // No seat-wide participation yet: opting out of nothing changes nothing.
    plan_and_apply(&fx, "participation join th_a --scope seat --seat foreman");
    steps(&fx, 3).await;
    let a = fx.threads.member(&th, &seat_ref("seat-A")).expect("clone A is invited to the participation thread");
    assert_eq!((a.constraint, a.state), (InviteConstraint::Ordinary, InvitationState::Pending));
    assert!(fx.threads.member(&th, &seat_ref("seat-B")).is_none(), "the opted-out clone is not invited");
    let ca = clone_rec(&fx, &both[0].id);
    assert_eq!(inv(&ca, &th).constraint, InviteConstraint::Ordinary);
    // The clone rejoins: it is invited too.
    plan_and_apply(&fx, &format!("participation join th_a --scope clone --clone {}", both[1].id));
    steps(&fx, 3).await;
    assert_eq!(fx.threads.member(&th, &seat_ref("seat-B")).unwrap().constraint, InviteConstraint::Ordinary);
    assert_eq!(fx.threads.call_count("invite"), 2 * 2 + 2, "two required per clone plus one ordinary per clone");
}

#[tokio::test]
async fn whole_seat_leave_notifies_with_delayed_instruction() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    fx.threads.add_thread("th_a", "shared");
    let th = ThreadRef("th_a".into());
    plan_and_apply(&fx, "participation join th_a --scope seat --seat foreman");
    steps(&fx, 3).await;
    fx.threads.accept(&th, &seat_ref("seat-A"));
    later(&fx).await;
    plan_and_apply(&fx, "participation leave th_a --scope seat --seat foreman");
    let leave_op = fx.journal.list(&[OpState::Committed], 1).unwrap().remove(0);
    steps(&fx, 3).await;
    let seat_rec = seat(&fx, "foreman");
    let notes: Vec<_> = fx.threads.notifications().into_iter().filter(|n| n.thread.0 == seat_rec.channel.thread_id.clone().unwrap()).collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    let n = &notes[0];
    assert_eq!(n.severity, Severity::Info);
    let instruction = json!({ "op": leave_op.op, "seat": seat_rec.id, "rev": seat_rec.rev });
    assert!(n.body.contains(&instruction.to_string()), "instruction JSON {instruction} in {:?}", n.body);
    assert!(
        n.body.contains(&format!("each clone: run `herdr-graph check-instruction {} {} {}`; if current, leave the thread yourself", leave_op.op, seat_rec.id, seat_rec.rev)),
        "{}",
        n.body
    );
    assert!(n.body.contains("th_a"));
    steps(&fx, 2).await;
    assert_eq!(fx.threads.notifications().len(), 1, "notified once");
    // Completion is tracked through Membership: nothing is recorded as left until threads says so.
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(inv(&c, &th).state, InvitationState::Accepted);
    fx.threads.leave(&th, &seat_ref("seat-A"));
    later(&fx).await;
    later(&fx).await;
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(inv(&c, &th).state, InvitationState::Released, "the agent left the thread itself");
}

// ---------------------------------------------------------------------------------------------
// notifications
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn rename_notifies_seat_channels() {
    let fx = fx();
    base(&fx).await;
    plan_and_apply(&fx, "seat create second --teamspace alpha --active --harness shell");
    steps(&fx, 3).await;
    plan_and_apply(&fx, "teamspace rename alpha beta");
    steps(&fx, 3).await;
    for name in ["foreman", "second"] {
        let t = thread_of_seat(&fx, name);
        let notes: Vec<_> = fx.threads.notifications().into_iter().filter(|n| n.thread == t).collect();
        assert_eq!(notes.len(), 1, "{name}: {notes:?}");
        assert!(notes[0].body.contains("\"alpha\"") && notes[0].body.contains("\"beta\""), "{}", notes[0].body);
        assert!(notes[0].body.contains("folder moved"), "path change is reported: {}", notes[0].body);
    }
    steps(&fx, 2).await;
    assert_eq!(fx.threads.notifications().len(), 2, "once per seat");
}

#[tokio::test]
async fn reload_required_notifies() {
    let fx = fx();
    base(&fx).await;
    steps(&fx, 2).await;
    let s = seat(&fx, "foreman");
    bookkeeping(&fx, "test_set_reload", json!({ "seat": s.id, "on": true }));
    steps(&fx, 3).await;
    let t = thread_of_seat(&fx, "foreman");
    let notes: Vec<_> = fx.threads.notifications().into_iter().filter(|n| n.thread == t).collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0].severity, Severity::Warn);
    assert!(notes[0].body.contains("Reload"), "{}", notes[0].body);
    steps(&fx, 2).await;
    assert_eq!(fx.threads.notifications().len(), 1, "one warning per flag episode");
    // Clearing and raising the flag again is a new episode.
    bookkeeping(&fx, "test_set_reload", json!({ "seat": s.id, "on": false }));
    steps(&fx, 2).await;
    bookkeeping(&fx, "test_set_reload", json!({ "seat": s.id, "on": true }));
    steps(&fx, 3).await;
    assert_eq!(fx.threads.notifications().len(), 2);
}

// ---------------------------------------------------------------------------------------------
// worktree_dirty (spec §3.5)
// ---------------------------------------------------------------------------------------------

/// Appends a dirty line the way `worktree::fast_forward` does.
fn append_dirty(fx: &Fx, path: &str, op: &OpId) {
    use std::io::Write;
    let root = fx.deps.instance.clone();
    std::fs::create_dir_all(root.join(".graph-local")).unwrap();
    let entry = crate::writer::worktree::DirtyEntry { path: path.to_owned(), op: Some(op.clone()), at: t0() };
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(root.join(".graph-local/worktree_dirty")).unwrap();
    writeln!(f, "{}", serde_json::to_string(&entry).unwrap()).unwrap();
}

fn seat_folder(fx: &Fx, name: &str) -> String {
    let id = seat(fx, name).id;
    layout::all_seats(&view(fx)).unwrap().into_iter().find(|(_, s)| s.id == id).unwrap().0.folder.as_str().to_owned()
}

fn dirty_notes(fx: &Fx, thread: &ThreadRef) -> Vec<super::fake::FakeNotification> {
    fx.threads.notifications().into_iter().filter(|n| &n.thread == thread && n.body.contains("left in place")).collect()
}

#[tokio::test]
async fn worktree_dirty_notifies_owning_seat_once_per_file_per_op() {
    let fx = fx();
    base(&fx).await;
    steps(&fx, 2).await;
    let folder = seat_folder(&fx, "foreman");
    let (op_a, op_b) = (OpId::new(), OpId::new());
    append_dirty(&fx, &format!("{folder}/AGENTS.md"), &op_a);
    append_dirty(&fx, &format!("{folder}/notes/x.md"), &op_a);
    append_dirty(&fx, &format!("{folder}/AGENTS.md"), &op_b);
    steps(&fx, 3).await;
    let t = thread_of_seat(&fx, "foreman");
    let notes = dirty_notes(&fx, &t);
    assert_eq!(notes.len(), 3, "{notes:?}");
    assert!(notes.iter().all(|n| n.severity == Severity::Warn));
    for (path, op) in [("AGENTS.md", &op_a), ("notes/x.md", &op_a), ("AGENTS.md", &op_b)] {
        let (path, op) = (format!("{folder}/{path}"), op.to_string());
        assert_eq!(notes.iter().filter(|n| n.body.contains(&path) && n.body.contains(&op)).count(), 1, "{path} {op}: {notes:?}");
    }
    steps(&fx, 3).await;
    assert_eq!(dirty_notes(&fx, &t).len(), 3, "level-triggered but once per (op, file)");
}

#[tokio::test]
async fn worktree_dirty_outside_any_seat_is_not_notified() {
    let fx = fx();
    base(&fx).await;
    steps(&fx, 2).await;
    let ts_folder = layout::list_teamspaces(&view(&fx)).unwrap().remove(0).0.folder.as_str().to_owned();
    append_dirty(&fx, &format!("{ts_folder}/teamspace.toml"), &OpId::new());
    steps(&fx, 3).await;
    let t = thread_of_seat(&fx, "foreman");
    assert!(dirty_notes(&fx, &t).is_empty());
    assert!(fx.threads.notifications().iter().all(|n| !n.body.contains("left in place")), "{:?}", fx.threads.notifications());
}

#[tokio::test]
async fn worktree_dirty_for_retired_seat_is_not_notified() {
    let fx = fx();
    base(&fx).await;
    plan_and_apply(&fx, "seat create second --teamspace alpha --active --harness shell");
    steps(&fx, 3).await;
    let folder = seat_folder(&fx, "second");
    plan_and_apply(&fx, "seat retire second");
    steps(&fx, 3).await;
    append_dirty(&fx, &format!("{folder}/AGENTS.md"), &OpId::new());
    steps(&fx, 3).await;
    assert!(fx.threads.notifications().iter().all(|n| !n.body.contains("left in place")), "{:?}", fx.threads.notifications());
}

// ---------------------------------------------------------------------------------------------
// failure handling
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn service_busy_backs_off_no_takeover() {
    let fx = fx();
    plan_and_apply(&fx, "teamspace create alpha");
    plan_and_apply(&fx, "seat create foreman --teamspace alpha --active --harness shell");
    fx.threads.set_busy(true);
    steps(&fx, 3).await;
    assert!(fx.threads.calls().is_empty(), "a busy service is never forced: {:?}", fx.threads.calls());
    let ensures = rows(&fx, EffectKind::EnsureThread);
    assert_eq!(ensures.len(), 2);
    assert!(ensures.iter().all(|r| r.status == EffectStatus::Pending && r.last_error.as_deref() == Some("threads service busy")), "{ensures:?}");
    let attempts: Vec<u32> = ensures.iter().map(|r| r.attempts).collect();
    steps(&fx, 2).await;
    assert_eq!(rows(&fx, EffectKind::EnsureThread).iter().map(|r| r.attempts).collect::<Vec<_>>(), attempts, "backoff: no retry before it expires");
    // Disconnects back off the same way.
    fx.threads.set_busy(false);
    fx.threads.disconnect();
    later(&fx).await;
    assert!(fx.threads.calls().is_empty());
    assert!(rows(&fx, EffectKind::EnsureThread).iter().all(|r| r.status == EffectStatus::Pending));
    fx.threads.reconnect();
    later(&fx).await;
    later(&fx).await;
    assert_eq!(fx.threads.call_count("ensure_thread"), 2);
    assert!(rows(&fx, EffectKind::EnsureThread).iter().all(|r| r.status == EffectStatus::Done));
}

#[tokio::test]
async fn seat_map_failure_backs_off() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    fx.map.set_failing(true);
    steps(&fx, 3).await;
    assert_eq!(fx.threads.call_count("invite"), 0);
    let invites = rows(&fx, EffectKind::Invite);
    assert!(invites.iter().all(|r| r.status == EffectStatus::Pending && r.attempts >= 1), "{invites:?}");
    fx.map.set_failing(false);
    later(&fx).await;
    later(&fx).await;
    assert_eq!(fx.threads.call_count("invite"), 2);
}

// ---------------------------------------------------------------------------------------------
// who, mutations, delivery capability
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn who_maps_pane_to_graph_identity() {
    let fx = fx();
    let clone = base(&fx).await;
    let clone = {
        occupy(&fx, &clone, "seat-A").await;
        clone_rec(&fx, &clone.id)
    };
    steps(&fx, 2).await;
    let pane = pane_of(&clone);
    let reply = who(&view(&fx), &pane.0).unwrap();
    assert_eq!(reply.teamspace.as_ref().unwrap().name, "alpha");
    assert_eq!(reply.seat.as_ref().unwrap().name, "foreman");
    assert_eq!(reply.clone.as_ref().unwrap().id, clone.id.to_string());
    assert_eq!(reply.native_session, clone.occupant.as_ref().map(|o| o.native_session.to_string()));
    assert!(reply.render().starts_with("alpha/foreman/"), "{}", reply.render());
    assert!(reply.render().ends_with(&reply.native_session.clone().unwrap()), "{}", reply.render());
    // A threads seat resolves through the recorded invitation link.
    assert_eq!(who(&view(&fx), "seat-A").unwrap().clone.unwrap().id, clone.id.to_string());
    // Graph ids resolve too; unknown targets are unbound.
    let seat_only = who(&view(&fx), seat(&fx, "foreman").id.as_str()).unwrap();
    assert_eq!(seat_only.render(), "alpha/foreman");
    let nobody = who(&view(&fx), "w9:p9").unwrap();
    assert!(!nobody.is_bound());
    assert_eq!(nobody.render(), "unbound");
}

#[tokio::test]
async fn who_command_is_registered_and_answers() {
    let fx = fx();
    let clone = base(&fx).await;
    let mut reg = Registry::default();
    register_commands(&mut reg, fx.store.clone());
    let h = reg.handler("who").expect("who registered");
    let ctx = CommandCtx { request_id: "r".into(), caller: CallerInfo::default() };
    let v = h.call(ctx.clone(), json!({ "target": pane_of(&clone).0 })).await.unwrap();
    let reply: WhoReply = serde_json::from_value(v).unwrap();
    assert_eq!(reply.seat.unwrap().name, "foreman");
    let e = h.call(ctx, json!({})).await.unwrap_err();
    assert!(e.message.contains("target"), "{}", e.message);
}

#[tokio::test]
async fn invitation_mutation_ignores_stale_writes() {
    let fx = fx();
    let clone = base(&fx).await;
    let write = |state: &str, link_occupant: &str, expect: serde_json::Value| {
        json!({
            "clone": clone.id, "thread": "th", "constraint": "required", "state": state,
            "link": { "seat": "seat-A", "occupant": link_occupant }, "expect_occupant": expect,
        })
    };
    bookkeeping(&fx, "invitation", write("pending", "ns_new", json!(null)));
    // A late release about the previous occupant must not overwrite the new occupant's invitation.
    bookkeeping(&fx, "invitation", write("released", "ns_old", json!("ns_old")));
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.invitations.len(), 1);
    assert_eq!(c.invitations[0].state, InvitationState::Pending);
    assert_eq!(c.invitations[0].link.as_ref().unwrap().occupant.as_deref(), Some("ns_new"));
    // A write about the current occupant applies.
    bookkeeping(&fx, "invitation", write("accepted", "ns_new", json!("ns_new")));
    assert_eq!(clone_rec(&fx, &clone.id).invitations[0].state, InvitationState::Accepted);
}

#[tokio::test]
async fn bookkeeping_channel_rejects_unknown_objects() {
    let fx = fx();
    base(&fx).await;
    bookkeeping(&fx, "channel", json!({ "object": SeatId::new(), "thread_id": "x" }));
    let rejected = fx.journal.list(&[OpState::Rejected], 10).unwrap();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].rejection.as_ref().unwrap().reason, "object_missing");
}

#[cfg(not(feature = "threads-service-ack"))]
#[tokio::test]
async fn delivery_capability_fallback_without_feature() {
    let dir = tempfile::tempdir().unwrap();
    let t = ServiceThreads::with_system_clock(dir.path().join("none.sock"), dir.path().join("state/intents"), uuid::Uuid::new_v4()).unwrap();
    assert_eq!(t.delivery_capability().await.unwrap(), DeliveryCapability::NotifyFallback, "no I/O, no registration probe");
    let r = t.send_request(&ThreadRef("t".into()), &[seat_ref("s")], "b", &OpKey("k".into())).await;
    assert!(matches!(r, Err(ThreadsError::Unsupported)));
    assert!(matches!(t.receipt_state(&[]).await, Err(ThreadsError::Unsupported)));
    // The fake reports the same by default and can model a v2 service.
    let fake = FakeThreads::new();
    assert_eq!(fake.delivery_capability().await.unwrap(), DeliveryCapability::NotifyFallback);
    fake.set_capability(DeliveryCapability::ServiceAck);
    assert_eq!(fake.delivery_capability().await.unwrap(), DeliveryCapability::ServiceAck);
}

#[test]
fn threads_seat_reference_helpers() {
    // Long keys are hashed into valid, stable operation ids; short ones pass through.
    let long = OpKey(format!("invite:{}", "x".repeat(300)));
    let id = adapter::operation_id(&long);
    assert!(id.as_str().len() <= 128 && id.as_str().starts_with("h-"));
    assert_eq!(id, adapter::operation_id(&long));
    assert_ne!(id, adapter::operation_id(&OpKey(format!("invite:{}", "y".repeat(300)))));
    assert_eq!(adapter::operation_id(&OpKey("ensure:st_X".into())).as_str(), "ensure:st_X");
    let _ = Lifecycle::Active;
}
