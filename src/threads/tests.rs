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
use crate::ports::clock::{Clock, ManualClock};
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
        rec.occupant = Some(Occupant {
            native_session: ns.id.clone(),
            harness: Harness::Shell,
            since: cx.now,
        });
        rec.sessions.push(ns);
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied {
            summary: "set occupant".into(),
            action: None,
        })
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
        Ok(Applied {
            summary: "clear occupant".into(),
            action: None,
        })
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
        Ok(Applied {
            summary: "set reload".into(),
            action: None,
        })
    }
}

/// Test-only stand-ins for the commits that call the ThreadsSeatAssignment helpers (undo/resurrection, purge).
struct SeatFixture;
impl Mutation for SeatFixture {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let clone: CloneId = cx.request.args["clone"].as_str().unwrap().parse().unwrap();
        let loc = cx.tree.locate(&clone.to_any())?.unwrap();
        let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        let mut deleted = false;
        match cx.request.args["op"].as_str().unwrap() {
            "restore" => {
                let epoch: OpId = cx.request.args["epoch"].as_str().unwrap().parse().unwrap();
                assignment::write_restore_intent(&mut rec, &epoch);
            }
            "tombstone" => {
                let reservation = cx.request.args["reservation"].as_str();
                match assignment::purge_tombstone(&rec, reservation) {
                    Some(t) => rec = t,
                    None => {
                        cx.tree.delete_file(&loc.record_path);
                        deleted = true;
                    }
                }
            }
            "raw_finalize" => {
                // The tombstone shape an earlier build wrote when it purged a seated clone.
                let seat = rec.threads_seat.clone().expect("raw_finalize needs a seat");
                rec.lifecycle = CloneLifecycle::Retired;
                rec.runtime = crate::model::common::Runtime::default();
                rec.occupant = None;
                rec.sessions = vec![];
                rec.invitations = vec![];
                rec.seat_fact = None;
                rec.seat_intent = Some(crate::model::clone::SeatIntent::Finalize { seat });
            }
            "raw_restore" => {
                rec.threads_seat = None;
                rec.seat_intent = Some(crate::model::clone::SeatIntent::Restore {
                    seat: "seat-gone".into(),
                    epoch: cx.request.args["epoch"].as_str().unwrap().parse().unwrap(),
                    threads_done: false,
                    launched: None,
                    last_refusal: None,
                });
            }
            "activate" => {
                rec.lifecycle = CloneLifecycle::Active;
                rec.retired = None;
            }
            "fact" => {
                rec.seat_fact = Some(crate::model::clone::SeatFact {
                    seat: cx.request.args["seat"].as_str().unwrap().into(),
                    state: serde_json::from_value(cx.request.args["state"].clone()).unwrap(),
                    read_at: cx.now,
                });
            }
            other => panic!("unknown fixture op {other}"),
        }
        if !deleted {
            cx.tree.put_record(loc.record_path, &mut rec)?;
        }
        Ok(Applied {
            summary: "seat fixture".into(),
            action: None,
        })
    }
}

/// Test-only stand-in for the observer's binding stamp: token `hg=<clone>`, availability, pane id.
struct SetBinding;
impl Mutation for SetBinding {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let clone: CloneId = cx.request.args["clone"].as_str().unwrap().parse().unwrap();
        let loc = cx.tree.locate(&clone.to_any())?.unwrap();
        let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        let a = &cx.request.args;
        let mut bound = rec.runtime.bound.clone().expect("clone has a binding");
        bound.token = a["token"]
            .as_bool()
            .unwrap()
            .then(|| crate::model::launch::graph_token(&clone.to_any()));
        bound.pane_id = a["pane"].as_str().map(|p| HerdrPaneId(p.into()));
        rec.runtime.availability = if a["present"].as_bool().unwrap() {
            crate::model::common::Availability::Present
        } else {
            crate::model::common::Availability::Absent
        };
        rec.runtime.bound = Some(bound);
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied {
            summary: "set binding".into(),
            action: None,
        })
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
    crate::observe::register_mutations(&mut reg);
    reg.register("bookkeeping.test_set_occupant", Arc::new(SetOccupant));
    reg.register("bookkeeping.test_clear_occupant", Arc::new(ClearOccupant));
    reg.register("bookkeeping.test_set_reload", Arc::new(SetReload));
    reg.register("bookkeeping.test_seat_fixture", Arc::new(SeatFixture));
    reg.register("bookkeeping.test_set_binding", Arc::new(SetBinding));
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let w = WriterCore::new(
        store.clone(),
        journal.clone(),
        Arc::new(reg),
        clock.clone(),
        WriterConfig::default(),
    );
    let deps = PlanDeps {
        kinds,
        plans,
        store: store.clone(),
        writer: w.clone(),
        clock: clock.clone(),
        instance: root.clone(),
    };
    let herdr = FakeHerdr::new();
    let cfg = ReconcilerConfig::new(root.clone());
    let rec = Reconciler::new(
        store.clone(),
        journal.clone(),
        w.clone(),
        herdr,
        clock.clone(),
        Arc::new(Quiet),
        cfg,
    );
    let threads = Arc::new(FakeThreads::new());
    let map = Arc::new(FakePaneSeatMap::new());
    register_with(&rec, threads.clone(), map.clone());
    Fx {
        _tmp: tmp,
        deps,
        w,
        store,
        journal,
        clock,
        rec,
        threads,
        map,
    }
}

fn plan_and_apply(fx: &Fx, change: &str) {
    let words = change.split_whitespace().map(str::to_owned).collect();
    let v = create_plan(&fx.deps, &CallerInfo::default(), words)
        .unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: crate::model::PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    let sp = fx.deps.plans.get(&id).unwrap().unwrap();
    let op = admit_apply(
        &fx.deps,
        &CallerInfo::default(),
        sp.plan.id.as_str(),
        Some(&sp.hash),
        "relay",
    )
    .unwrap_or_else(|e| panic!("admit {change}: {}", e.message));
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(
        row.state,
        OpState::Committed,
        "{change}: {:?}",
        row.rejection
    );
}

fn bookkeeping(fx: &Fx, sub: &str, mut args: serde_json::Value) {
    args["sub"] = json!(sub);
    let req = ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args,
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    };
    fx.w.admit(req).unwrap();
    fx.w.drain().unwrap();
}

fn observed(fx: &Fx, req: ChangeRequest) {
    let op = fx.w.admit(req).unwrap();
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
}

fn observed_rename(fx: &Fx, object: crate::model::AnyId, new: &str) {
    observed(
        fx,
        crate::observe::mutations::rename_request(
            &object,
            new,
            crate::ports::clock::Clock::now(&*fx.clock),
            None,
        ),
    );
}

fn notes_on(fx: &Fx, t: &ThreadRef) -> Vec<super::fake::FakeNotification> {
    fx.threads
        .notifications()
        .into_iter()
        .filter(|n| &n.thread == t)
        .collect()
}

fn view(fx: &Fx) -> crate::store::tree::CommitView<'_> {
    crate::store::tree::CommitView {
        store: &*fx.store,
        at: fx.store.head().unwrap(),
    }
}

fn seat(fx: &Fx, name: &str) -> SeatRecord {
    let mut found: Vec<_> = layout::all_seats(&view(fx))
        .unwrap()
        .into_iter()
        .filter(|(_, s)| s.name == name)
        .collect();
    assert_eq!(found.len(), 1, "seat {name}");
    found.remove(0).1
}

fn teamspace(fx: &Fx) -> crate::model::teamspace::TeamspaceRecord {
    layout::list_teamspaces(&view(fx)).unwrap().remove(0).1
}

fn clones_of(fx: &Fx, seat: &SeatId) -> Vec<CloneRecord> {
    let mut v: Vec<_> = layout::all_clones(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, c)| c)
        .filter(|c| &c.seat == seat)
        .collect();
    v.sort_by_key(|c| c.id.clone());
    v
}

fn clone_rec(fx: &Fx, id: &CloneId) -> CloneRecord {
    layout::all_clones(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, c)| c)
        .find(|c| &c.id == id)
        .unwrap()
}

fn pane_of(c: &CloneRecord) -> HerdrPaneId {
    c.runtime
        .bound
        .as_ref()
        .and_then(|b| b.pane_id.clone())
        .expect("clone has a pane binding")
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
    plan_and_apply(
        fx,
        "seat create foreman --teamspace alpha --active --harness shell",
    );
    steps(fx, 3).await;
    let s = seat(fx, "foreman");
    let mut cs = clones_of(fx, &s.id);
    assert_eq!(cs.len(), 1);
    cs.remove(0)
}

/// The clone's pane is registered as `threads_seat` with herdr-threads and the clone records it (the
/// resolve-seat want's outcome), then gets an occupant.
async fn occupy(fx: &Fx, clone: &CloneRecord, threads_seat: &str) {
    seat_the_clone(fx, clone, threads_seat);
    bookkeeping(fx, "test_set_occupant", json!({ "clone": clone.id }));
}

/// The clone records `threads_seat` (no occupant): invitations are keyed on it.
fn seat_the_clone(fx: &Fx, clone: &CloneRecord, threads_seat: &str) {
    fx.map
        .set(&pane_of(&clone_rec(fx, &clone.id)), threads_seat);
    assert_eq!(transition(fx, &clone.id, assign(threads_seat)), None);
}

fn thread_of_seat(fx: &Fx, name: &str) -> ThreadRef {
    ThreadRef(
        seat(fx, name)
            .channel
            .thread_id
            .expect("seat channel ensured"),
    )
}

fn thread_of_ts(fx: &Fx) -> ThreadRef {
    ThreadRef(
        teamspace(fx)
            .channel
            .thread_id
            .expect("teamspace channel ensured"),
    )
}

fn inv<'a>(c: &'a CloneRecord, thread: &ThreadRef) -> &'a crate::model::clone::Invitation {
    c.invitations
        .iter()
        .find(|i| i.thread == thread.0)
        .unwrap_or_else(|| panic!("no invitation to {}: {:?}", thread.0, c.invitations))
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
    fx.journal
        .effects_with_status(&all)
        .unwrap()
        .into_iter()
        .filter(|r| r.kind == kind)
        .collect()
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
    assert_eq!(
        seat_thread,
        ThreadRef(derived_thread(&seat(&fx, "foreman").id.to_any()))
    );
    assert_ne!(seat_thread, ts_thread);
    assert_eq!(
        fx.threads.call_count("ensure_thread"),
        2,
        "one per channel, however many steps ran"
    );
    assert_eq!(
        fx.threads.thread(&seat_thread).unwrap().topic,
        "alpha/foreman"
    );
    assert_eq!(fx.threads.thread(&ts_thread).unwrap().topic, "alpha");
    assert_eq!(rows(&fx, EffectKind::EnsureThread).len(), 2);
    assert!(
        rows(&fx, EffectKind::EnsureThread)
            .iter()
            .all(|r| r.status == EffectStatus::Done)
    );
}

#[tokio::test]
async fn dormant_seats_get_no_channel() {
    let fx = fx();
    plan_and_apply(&fx, "teamspace create alpha");
    plan_and_apply(&fx, "seat create idle --teamspace alpha --harness shell");
    steps(&fx, 2).await;
    assert!(seat(&fx, "idle").channel.thread_id.is_none());
    assert_eq!(
        fx.threads.call_count("ensure_thread"),
        0,
        "no active seat and no active teamspace yet"
    );
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
    assert_eq!(
        fx.threads.thread(&seat_thread).unwrap().topic,
        "beta/boss",
        "the seat topic carries the teamspace name"
    );
    steps(&fx, 2).await;
    assert_eq!(
        fx.threads.call_count("set_topic"),
        3,
        "settled topics are not set again"
    );
}

// ---------------------------------------------------------------------------------------------
// required invitations and membership
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn invite_required_for_occupied_clone_on_seat_and_teamspace_channels() {
    let fx = fx();
    let clone = base(&fx).await;
    steps(&fx, 2).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        0,
        "no threads seat, no invitation"
    );
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    for t in [&st, &ts] {
        let m = fx
            .threads
            .member(t, &seat_ref("seat-A"))
            .unwrap_or_else(|| panic!("seat-A not invited to {}", t.0));
        assert_eq!(m.constraint, InviteConstraint::Required);
        assert_eq!(m.state, InvitationState::Pending);
    }
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.invitations.len(), 2);
    for t in [&st, &ts] {
        let i = inv(&c, t);
        assert_eq!(
            (i.constraint, i.state),
            (InviteConstraint::Required, InvitationState::Pending)
        );
        let link = i.link.as_ref().unwrap();
        assert_eq!(link.seat, "seat-A");
        assert_eq!(
            link.occupant,
            c.occupant.as_ref().map(|o| o.native_session.to_string())
        );
        assert!(link.requirement.is_some() && link.revision == Some(1));
    }
    steps(&fx, 3).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        2,
        "an invitation is not repeated"
    );
}

#[tokio::test]
async fn invite_waits_for_the_pane_to_become_a_threads_seat() {
    let fx = fx();
    let clone = base(&fx).await;
    bookkeeping(&fx, "test_set_occupant", json!({ "clone": clone.id }));
    steps(&fx, 3).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        0,
        "the pane has no threads seat yet"
    );
    assert!(
        rows(&fx, EffectKind::Invite).is_empty(),
        "no threads seat recorded: no invitation is wanted"
    );
    fx.map.set(&pane_of(&clone_rec(&fx, &clone.id)), "seat-A");
    steps(&fx, 4).await;
    assert_eq!(
        clone_rec(&fx, &clone.id).threads_seat.as_deref(),
        Some("seat-A")
    );
    assert_eq!(fx.threads.call_count("invite"), 2);
}

#[tokio::test]
async fn invitation_is_issued_before_first_occupancy() {
    let fx = fx();
    let clone = base(&fx).await;
    assert!(clone_rec(&fx, &clone.id).occupant.is_none());
    seat_the_clone(&fx, &clone, "seat-A");
    steps(&fx, 3).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    for t in [&st, &ts] {
        let m = fx.threads.member(t, &seat_ref("seat-A")).expect("invited");
        assert_eq!(
            (m.constraint, m.state),
            (InviteConstraint::Required, InvitationState::Pending)
        );
    }
    assert!(
        clone_rec(&fx, &clone.id).occupant.is_none(),
        "no occupant was needed"
    );
    assert!(
        inv(&clone_rec(&fx, &clone.id), &st)
            .link
            .as_ref()
            .unwrap()
            .occupant
            .is_none()
    );
}

#[tokio::test]
async fn indefinitely_deferred_invite_backs_off_its_wake() {
    let fx = fx();
    let clone = base(&fx).await;
    bookkeeping(&fx, "test_set_occupant", json!({ "clone": clone.id })); // never resolved to a threads seat
    let start = fx.clock.now();
    let mut n = 0;
    while fx.clock.now() < start + chrono::Duration::minutes(10) {
        step(&fx).await;
        n += 1;
        let now = fx.clock.now();
        // What RuntimeLoop::run does: sleep until next_wake, at most one tick (60 s).
        let wake = fx
            .rec
            .next_wake()
            .unwrap_or(now + chrono::Duration::seconds(60))
            .min(now + chrono::Duration::seconds(60));
        fx.clock.set(wake);
    }
    assert!(
        n <= 16,
        "{n} steps in 10 min: open-ended deferrals must back off (fixed 2 s polling would be ~300)"
    );
    assert_eq!(fx.threads.call_count("invite"), 0);
    assert!(
        resolve_rows(&fx)
            .iter()
            .all(|r| r.status == EffectStatus::Pending && r.attempts == 0)
    );

    fx.map.set(&pane_of(&clone_rec(&fx, &clone.id)), "seat-A");
    steps(&fx, 4).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        2,
        "an event-driven step still delivers right away"
    );
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
    assert!(
        c.invitations
            .iter()
            .all(|i| i.state == InvitationState::Pending),
        "{:?}",
        c.invitations
    );
    for t in [thread_of_seat(&fx, "foreman"), thread_of_ts(&fx)] {
        assert_eq!(
            fx.threads.member(&t, &seat_ref("seat-A")).unwrap().state,
            InvitationState::Pending
        );
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
    assert_eq!(
        inv(&c, &st).state,
        InvitationState::Accepted,
        "recorded because threads reported it"
    );
    assert_eq!(
        inv(&c, &ts).state,
        InvitationState::Pending,
        "the other episode was not accepted"
    );
    fx.threads.accept(&ts, &seat_ref("seat-A"));
    later(&fx).await;
    later(&fx).await;
    let c = clone_rec(&fx, &clone.id);
    assert!(
        c.invitations
            .iter()
            .all(|i| i.state == InvitationState::Accepted)
    );
    assert!(crate::threads::pending_invitations(&view(&fx), &clone.id).is_empty());
}

#[tokio::test]
async fn pending_invitations_report_thread_and_constraint() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    let pending = crate::threads::pending_invitations(&view(&fx), &clone.id);
    assert_eq!(pending.len(), 2);
    let got = pending.iter().find(|p| p.thread == st.0).unwrap();
    assert_eq!(got.constraint, InviteConstraint::Required);
    let other = pending.iter().find(|p| p.thread == ts.0).unwrap();
    assert_eq!(other.constraint, InviteConstraint::Required);
    // Acceptance belongs to herdr-threads: the entry carries only thread and constraint.
    let v = serde_json::to_value(&pending[0]).unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["constraint", "thread"]);
    // An accepted invitation is no longer pending.
    fx.threads.accept(&st, &seat_ref("seat-A"));
    later(&fx).await;
    later(&fx).await;
    let pending = crate::threads::pending_invitations(&view(&fx), &clone.id);
    assert_eq!(
        pending.iter().map(|p| p.thread.clone()).collect::<Vec<_>>(),
        vec![ts.0.clone()]
    );
    assert!(crate::threads::pending_invitations(&view(&fx), &CloneId::new()).is_empty());
}

// ---------------------------------------------------------------------------------------------
// occupancy-keyed cleanup
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn occupancy_end_keeps_required_memberships() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    fx.threads.accept(&st, &seat_ref("seat-A"));
    later(&fx).await;
    // The agent exits with its pane still open.
    bookkeeping(&fx, "test_clear_occupant", json!({ "clone": clone.id }));
    steps(&fx, 3).await;
    later(&fx).await;
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    assert_eq!(fx.threads.call_count("invite"), 2, "nobody to re-invite");
    assert_eq!(
        fx.threads.member(&st, &seat_ref("seat-A")).unwrap().state,
        InvitationState::Accepted
    );
    assert_eq!(
        fx.threads.member(&ts, &seat_ref("seat-A")).unwrap().state,
        InvitationState::Pending
    );
    let c = clone_rec(&fx, &clone.id);
    assert!(
        c.invitations
            .iter()
            .all(|i| i.state != InvitationState::Released),
        "{:?}",
        c.invitations
    );
}

#[tokio::test]
async fn seat_succession_releases_the_old_seat_and_invites_the_new() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    fx.threads.accept(&st, &seat_ref("seat-A"));
    later(&fx).await;
    let epoch = OpId::new();
    write_restore(&fx, &clone.id, &epoch);
    assert_eq!(
        transition(
            &fx,
            &clone.id,
            SeatTransition::Succeed {
                seat: "seat-B".into(),
                from: "seat-A".into(),
                epoch,
            }
        ),
        None
    );
    steps(&fx, 4).await;
    for t in [&st, &ts] {
        let a = fx.threads.member(t, &seat_ref("seat-A")).unwrap();
        let b = fx.threads.member(t, &seat_ref("seat-B")).unwrap();
        assert_eq!(a.state, InvitationState::Released, "{}", t.0);
        assert_eq!(
            (b.constraint, b.state),
            (InviteConstraint::Required, InvitationState::Pending)
        );
    }
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(inv(&c, &st).link.as_ref().unwrap().seat, "seat-B");
    assert_eq!(inv(&c, &st).state, InvitationState::Pending);
    assert_eq!(fx.threads.call_count("release_requirement"), 2);
}

#[tokio::test]
async fn clear_or_relaunch_on_the_same_seat_issues_no_release_or_reinvite() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    fx.threads
        .accept(&thread_of_seat(&fx, "foreman"), &seat_ref("seat-A"));
    later(&fx).await;
    let before = clone_rec(&fx, &clone.id);
    // /clear or a relaunch: a new occupant on the same pane, so the same threads seat.
    bookkeeping(&fx, "test_set_occupant", json!({ "clone": clone.id }));
    steps(&fx, 3).await;
    later(&fx).await;
    let st = thread_of_seat(&fx, "foreman");
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    assert_eq!(fx.threads.call_count("invite"), 2);
    assert_eq!(
        fx.threads.member(&st, &seat_ref("seat-A")).unwrap().state,
        InvitationState::Accepted,
        "the accepted membership is untouched"
    );
    let c = clone_rec(&fx, &clone.id);
    assert_ne!(c.occupant, before.occupant, "the occupant did change");
    assert_eq!(inv(&c, &st).state, InvitationState::Accepted);
    assert_eq!(
        inv(&c, &st).link.as_ref().unwrap().occupant,
        inv(&before, &st).link.as_ref().unwrap().occupant,
        "the link keeps its attribution"
    );
}

#[tokio::test]
async fn upgrade_with_links_on_the_backfilled_seat_causes_no_reinvite() {
    let fx = fx();
    let clone = base(&fx).await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    // Pre-upgrade state: an occupied clone whose links name the pane's seat (carrying the old occupant)
    // and which has no threads_seat yet.
    bookkeeping(&fx, "test_set_occupant", json!({ "clone": clone.id }));
    let occupant = clone_rec(&fx, &clone.id)
        .occupant
        .map(|o| o.native_session.to_string());
    for (t, state) in [(&st, "accepted"), (&ts, "pending")] {
        bookkeeping(
            &fx,
            "invitation",
            json!({
                "clone": clone.id, "thread": t.0, "constraint": "required", "state": state,
                "link": { "seat": "seat-A", "occupant": occupant },
            }),
        );
    }
    fx.threads.accept(&st, &seat_ref("seat-A"));
    steps(&fx, 3).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        0,
        "no threads_seat, no wants"
    );
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    // The backfill (resolve-seat want) records the seat the links name.
    let pane = pane_of(&clone_rec(&fx, &clone.id));
    fx.map.set(&pane, "seat-A");
    set_binding(&fx, &clone.id, true, true, Some(&pane));
    steps(&fx, 4).await;
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.threads_seat.as_deref(), Some("seat-A"));
    assert_eq!(
        fx.threads.call_count("invite"),
        0,
        "links on the seat stay satisfied"
    );
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    assert_eq!(inv(&c, &st).state, InvitationState::Accepted);
}

/// Write one Required row of the clone the way the poll/release path would.
fn write_required(fx: &Fx, clone: &CloneId, t: &ThreadRef, state: &str, seat: &str) {
    bookkeeping(
        fx,
        "invitation",
        json!({
            "clone": clone, "thread": t.0, "constraint": "required", "state": state,
            "link": { "seat": seat },
        }),
    );
}

/// A pre-upgrade clone: its seat channel row is `released` (the old occupant-keyed path released it) and its
/// teamspace row `accepted`, both naming `seat-A`, and no `threads_seat` yet.
async fn pre_upgrade_clone(fx: &Fx) -> (CloneRecord, ThreadRef, ThreadRef) {
    let clone = base(fx).await;
    steps(fx, 2).await;
    let (st, ts) = (thread_of_seat(fx, "foreman"), thread_of_ts(fx));
    write_required(fx, &clone.id, &st, "released", "seat-A");
    write_required(fx, &clone.id, &ts, "accepted", "seat-A");
    steps(fx, 3).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        0,
        "no threads_seat, no wants"
    );
    (clone, st, ts)
}

#[tokio::test]
async fn released_required_row_on_the_backfilled_seat_is_reinvited_once() {
    let fx = fx();
    let (clone, st, ts) = pre_upgrade_clone(&fx).await;
    seat_the_clone(&fx, &clone, "seat-A");
    steps(&fx, 4).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        1,
        "exactly the released row"
    );
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(inv(&c, &st).state, InvitationState::Pending);
    assert_eq!(inv(&c, &ts).state, InvitationState::Accepted, "no churn");
    steps(&fx, 4).await;
    later(&fx).await;
    assert_eq!(fx.threads.call_count("invite"), 1, "once");
}

#[tokio::test]
async fn released_or_retired_required_row_on_the_current_seat_is_reinvited_once_per_report() {
    let fx = fx();
    let (clone, st, _) = pre_upgrade_clone(&fx).await;
    seat_the_clone(&fx, &clone, "seat-A");
    steps(&fx, 4).await;
    assert_eq!(fx.threads.call_count("invite"), 1);
    for (reported, total) in [("released", 2), ("retired", 3)] {
        write_required(&fx, &clone.id, &st, reported, "seat-A");
        steps(&fx, 4).await;
        assert_eq!(
            fx.threads.call_count("invite"),
            total,
            "one re-invite for the {reported} report"
        );
        assert_eq!(
            inv(&clone_rec(&fx, &clone.id), &st).state,
            InvitationState::Pending
        );
    }
    steps(&fx, 4).await;
    later(&fx).await;
    steps(&fx, 3).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        3,
        "per report, never per pass"
    );
}

#[tokio::test]
async fn ordinary_leave_then_rejoin_is_reinvited_once() {
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
    steps(&fx, 3).await;
    fx.threads.leave(&th, &seat_ref("seat-A"));
    later(&fx).await;
    later(&fx).await;
    assert_eq!(
        inv(&clone_rec(&fx, &clone.id), &th).state,
        InvitationState::Released
    );
    let n = fx.threads.call_count("invite");
    plan_and_apply(&fx, "participation join th_a --scope seat --seat foreman");
    steps(&fx, 3).await;
    later(&fx).await;
    steps(&fx, 3).await;
    assert_eq!(fx.threads.call_count("invite"), n + 1, "rejoin re-invites");
    let c = clone_rec(&fx, &clone.id);
    let row = inv(&c, &th);
    assert_eq!(row.state, InvitationState::Pending);
    assert_eq!(row.constraint, InviteConstraint::Ordinary);
    assert_eq!(row.link.as_ref().unwrap().seat, "seat-A");
    steps(&fx, 3).await;
    later(&fx).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        n + 1,
        "no churn once Pending"
    );
}

/// A seated clone with a migration-marked Released seat-channel link and a Pending teamspace link, then retired.
async fn retired_marked_clone(fx: &Fx) -> (CloneRecord, ThreadRef, ThreadRef) {
    let clone = base(fx).await;
    // A second clone keeps the seat active when the first is retired.
    plan_and_apply(fx, "clone add foreman --name second");
    steps(fx, 3).await;
    let (st, ts) = (thread_of_seat(fx, "foreman"), thread_of_ts(fx));
    write_required(fx, &clone.id, &st, "released", "seat-A");
    write_required(fx, &clone.id, &ts, "pending", "seat-A");
    seat_the_clone(fx, &clone, "seat-A");
    plan_and_apply(fx, &format!("clone retire {}", clone.id));
    assert_eq!(clone_rec(fx, &clone.id).lifecycle, CloneLifecycle::Retired);
    (clone, st, ts)
}

#[tokio::test]
async fn retired_clone_links_are_not_reinvited_across_passes() {
    let fx = fx();
    let (clone, st, ts) = retired_marked_clone(&fx).await;
    steps(&fx, 5).await;
    later(&fx).await;
    steps(&fx, 3).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        0,
        "a retired clone is not re-invited"
    );
    assert_eq!(
        fx.threads.call_count("release_requirement"),
        0,
        "its Pending links are kept, not released"
    );
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(inv(&c, &st).state, InvitationState::Released);
    assert_eq!(inv(&c, &ts).state, InvitationState::Pending);
}

#[tokio::test]
async fn undo_reactivated_clone_reinvites_its_released_row_once() {
    let fx = fx();
    let (clone, st, ts) = retired_marked_clone(&fx).await;
    steps(&fx, 4).await;
    assert_eq!(fx.threads.call_count("invite"), 0);
    bookkeeping(
        &fx,
        "test_seat_fixture",
        json!({ "clone": clone.id, "op": "activate" }),
    );
    steps(&fx, 4).await;
    later(&fx).await;
    steps(&fx, 3).await;
    assert_eq!(
        fx.threads.call_count("invite"),
        1,
        "once, on the recorded seat"
    );
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.threads_seat.as_deref(), Some("seat-A"));
    assert_eq!(inv(&c, &st).state, InvitationState::Pending);
    assert_eq!(inv(&c, &st).link.as_ref().unwrap().seat, "seat-A");
    assert_eq!(inv(&c, &ts).state, InvitationState::Pending);
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
}

#[tokio::test]
async fn threads_unreachable_issues_no_release_or_invite() {
    let fx = fx();
    let clone = base(&fx).await;
    seat_the_clone(&fx, &clone, "seat-A");
    fx.threads.disconnect();
    steps(&fx, 3).await;
    later(&fx).await;
    assert_eq!(fx.threads.call_count("invite"), 0);
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    let c = clone_rec(&fx, &clone.id);
    assert!(c.invitations.is_empty(), "{:?}", c.invitations);
    // Succession while unreachable: the release waits too.
    fx.threads.reconnect();
    later(&fx).await;
    steps(&fx, 3).await;
    assert_eq!(fx.threads.call_count("invite"), 2);
    let epoch = OpId::new();
    write_restore(&fx, &clone.id, &epoch);
    assert_eq!(
        transition(
            &fx,
            &clone.id,
            SeatTransition::Succeed {
                seat: "seat-B".into(),
                from: "seat-A".into(),
                epoch,
            }
        ),
        None
    );
    fx.threads.disconnect();
    steps(&fx, 3).await;
    later(&fx).await;
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    assert_eq!(fx.threads.call_count("invite"), 2);
    fx.threads.reconnect();
    later(&fx).await;
    steps(&fx, 4).await;
    assert_eq!(fx.threads.call_count("release_requirement"), 2);
    assert_eq!(fx.threads.call_count("invite"), 4);
}

#[tokio::test]
async fn graph_clone_retirement_keeps_memberships_until_purge() {
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
    plan_and_apply(&fx, &format!("clone retire {}", both[0].id));
    steps(&fx, 4).await;
    later(&fx).await;
    assert_eq!(
        clone_rec(&fx, &both[0].id).lifecycle,
        CloneLifecycle::Retired
    );
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    for t in [&st, &ts] {
        for s in ["seat-A", "seat-B"] {
            assert_eq!(
                fx.threads.member(t, &seat_ref(s)).unwrap().state,
                InvitationState::Pending,
                "{s}: a retired clone keeps its memberships until purge"
            );
        }
    }
}

#[tokio::test]
async fn pane_close_retires_the_clone_and_issues_no_release() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let st = thread_of_seat(&fx, "foreman");
    fx.threads.accept(&st, &seat_ref("seat-A"));
    later(&fx).await;
    retire_clone_observed(&fx, &clone.id);
    assert_eq!(
        clone_rec(&fx, &clone.id).lifecycle,
        CloneLifecycle::Retired,
        "the pane close retired the clone"
    );
    steps(&fx, 4).await;
    later(&fx).await;
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    assert_eq!(fx.threads.call_count("invite"), 2, "no re-invite either");
    assert_eq!(
        fx.threads.member(&st, &seat_ref("seat-A")).unwrap().state,
        InvitationState::Accepted
    );
    let c = clone_rec(&fx, &clone.id);
    assert!(
        c.invitations
            .iter()
            .all(|i| i.state != InvitationState::Released)
    );
}

#[tokio::test]
async fn tombstone_gets_no_wants() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    retire_clone_observed(&fx, &clone.id);
    bookkeeping(
        &fx,
        "test_seat_fixture",
        json!({ "clone": clone.id, "op": "raw_finalize" }),
    );
    let t = clone_rec(&fx, &clone.id);
    assert!(t.is_tombstone());
    assert_eq!(t.threads_seat.as_deref(), Some("seat-A"));
    let before = fx.threads.calls().len();
    steps(&fx, 4).await;
    later(&fx).await;
    assert_eq!(fx.threads.call_count("release_requirement"), 0);
    assert_eq!(fx.threads.call_count("invite"), 2);
    let new: Vec<_> = fx.threads.calls().split_off(before);
    assert!(
        new.iter()
            .all(|c| !c.starts_with("invite") && !c.starts_with("release")),
        "{new:?}"
    );
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
    plan_and_apply(
        &fx,
        &format!(
            "participation leave th_a --scope clone --clone {}",
            both[1].id
        ),
    );
    // No seat-wide participation yet: opting out of nothing changes nothing.
    plan_and_apply(&fx, "participation join th_a --scope seat --seat foreman");
    steps(&fx, 3).await;
    let a = fx
        .threads
        .member(&th, &seat_ref("seat-A"))
        .expect("clone A is invited to the participation thread");
    assert_eq!(
        (a.constraint, a.state),
        (InviteConstraint::Ordinary, InvitationState::Pending)
    );
    assert!(
        fx.threads.member(&th, &seat_ref("seat-B")).is_none(),
        "the opted-out clone is not invited"
    );
    let ca = clone_rec(&fx, &both[0].id);
    assert_eq!(inv(&ca, &th).constraint, InviteConstraint::Ordinary);
    // The clone rejoins: it is invited too.
    plan_and_apply(
        &fx,
        &format!(
            "participation join th_a --scope clone --clone {}",
            both[1].id
        ),
    );
    steps(&fx, 3).await;
    assert_eq!(
        fx.threads
            .member(&th, &seat_ref("seat-B"))
            .unwrap()
            .constraint,
        InviteConstraint::Ordinary
    );
    assert_eq!(
        fx.threads.call_count("invite"),
        2 * 2 + 2,
        "two required per clone plus one ordinary per clone"
    );
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
    let notes: Vec<_> = fx
        .threads
        .notifications()
        .into_iter()
        .filter(|n| n.thread.0 == seat_rec.channel.thread_id.clone().unwrap())
        .collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    let n = &notes[0];
    assert_eq!(n.severity, Severity::Info);
    let instruction = json!({ "op": leave_op.op, "seat": seat_rec.id, "rev": seat_rec.rev });
    assert!(
        n.body.contains(&instruction.to_string()),
        "instruction JSON {instruction} in {:?}",
        n.body
    );
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
    assert_eq!(
        inv(&c, &th).state,
        InvitationState::Released,
        "the agent left the thread itself"
    );
}

// ---------------------------------------------------------------------------------------------
// notifications
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn rename_notifies_seat_channels() {
    let fx = fx();
    base(&fx).await;
    plan_and_apply(
        &fx,
        "seat create second --teamspace alpha --active --harness shell",
    );
    steps(&fx, 3).await;
    plan_and_apply(&fx, "teamspace rename alpha beta");
    steps(&fx, 3).await;
    for name in ["foreman", "second"] {
        let t = thread_of_seat(&fx, name);
        let notes: Vec<_> = fx
            .threads
            .notifications()
            .into_iter()
            .filter(|n| n.thread == t)
            .collect();
        assert_eq!(notes.len(), 1, "{name}: {notes:?}");
        assert!(
            notes[0].body.contains("\"alpha\"") && notes[0].body.contains("\"beta\""),
            "{}",
            notes[0].body
        );
        assert!(
            notes[0].body.contains("folder moved"),
            "path change is reported: {}",
            notes[0].body
        );
    }
    steps(&fx, 2).await;
    assert_eq!(fx.threads.notifications().len(), 2, "once per seat");
}

#[tokio::test]
async fn seat_rename_notifies_its_channel_with_the_new_path() {
    let fx = fx();
    base(&fx).await;
    steps(&fx, 2).await;
    let t = thread_of_seat(&fx, "foreman");
    plan_and_apply(&fx, "seat rename foreman chief");
    steps(&fx, 3).await;
    let notes = notes_on(&fx, &t);
    assert_eq!(notes.len(), 1, "{notes:?}");
    let b = &notes[0].body;
    assert!(
        b.contains("foreman") && b.contains("chief") && b.contains("teamspaces/alpha/seats/chief"),
        "{b}"
    );
    steps(&fx, 2).await;
    assert_eq!(notes_on(&fx, &t).len(), 1);
}

#[tokio::test]
async fn observed_seat_rename_notifies_its_channel() {
    let fx = fx();
    base(&fx).await;
    steps(&fx, 2).await;
    let t = thread_of_seat(&fx, "foreman");
    observed_rename(&fx, seat(&fx, "foreman").id.to_any(), "chief");
    steps(&fx, 3).await;
    let notes = notes_on(&fx, &t);
    assert_eq!(notes.len(), 1, "{notes:?}");
    let b = &notes[0].body;
    assert!(
        b.contains("foreman")
            && b.contains("chief")
            && b.contains("in Herdr")
            && b.contains("teamspaces/alpha/seats/chief"),
        "{b}"
    );
    steps(&fx, 2).await;
    assert_eq!(notes_on(&fx, &t).len(), 1);
}

#[tokio::test]
async fn observed_teamspace_rename_notifies_every_seat_channel() {
    let fx = fx();
    base(&fx).await;
    plan_and_apply(
        &fx,
        "seat create second --teamspace alpha --active --harness shell",
    );
    steps(&fx, 3).await;
    observed_rename(&fx, teamspace(&fx).id.to_any(), "beta");
    steps(&fx, 3).await;
    for name in ["foreman", "second"] {
        let notes = notes_on(&fx, &thread_of_seat(&fx, name));
        assert_eq!(notes.len(), 1, "{name}: {notes:?}");
        let b = &notes[0].body;
        assert!(
            b.contains("\"alpha\"") && b.contains("\"beta\"") && b.contains("teamspaces/beta"),
            "{b}"
        );
    }
    assert_eq!(fx.threads.notifications().len(), 2);
}

#[tokio::test]
async fn observed_clone_rename_notifies_its_seat_channel() {
    let fx = fx();
    let clone = base(&fx).await;
    steps(&fx, 2).await;
    observed_rename(&fx, clone.id.to_any(), "twin");
    steps(&fx, 3).await;
    let notes = notes_on(&fx, &thread_of_seat(&fx, "foreman"));
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].body.contains("\"twin\"") && notes[0].body.contains("/clones/"),
        "{}",
        notes[0].body
    );
}

#[tokio::test]
async fn noop_observed_rename_notifies_nobody() {
    let fx = fx();
    base(&fx).await;
    steps(&fx, 2).await;
    observed_rename(&fx, seat(&fx, "foreman").id.to_any(), "foreman");
    steps(&fx, 3).await;
    assert!(
        fx.threads
            .notifications()
            .iter()
            .all(|n| !n.body.contains("was renamed")),
        "{:?}",
        fx.threads.notifications()
    );
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
    let notes: Vec<_> = fx
        .threads
        .notifications()
        .into_iter()
        .filter(|n| n.thread == t)
        .collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0].severity, Severity::Warn);
    assert!(notes[0].body.contains("Reload"), "{}", notes[0].body);
    steps(&fx, 2).await;
    assert_eq!(
        fx.threads.notifications().len(),
        1,
        "one warning per flag episode"
    );
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
    let entry = crate::writer::worktree::DirtyEntry {
        path: path.to_owned(),
        op: Some(op.clone()),
        at: t0(),
    };
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(".graph-local/worktree_dirty"))
        .unwrap();
    writeln!(f, "{}", serde_json::to_string(&entry).unwrap()).unwrap();
}

fn seat_folder(fx: &Fx, name: &str) -> String {
    let id = seat(fx, name).id;
    layout::all_seats(&view(fx))
        .unwrap()
        .into_iter()
        .find(|(_, s)| s.id == id)
        .unwrap()
        .0
        .folder
        .as_str()
        .to_owned()
}

fn dirty_notes(fx: &Fx, thread: &ThreadRef) -> Vec<super::fake::FakeNotification> {
    fx.threads
        .notifications()
        .into_iter()
        .filter(|n| &n.thread == thread && n.body.contains("left in place"))
        .collect()
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
    for (path, op) in [
        ("AGENTS.md", &op_a),
        ("notes/x.md", &op_a),
        ("AGENTS.md", &op_b),
    ] {
        let (path, op) = (format!("{folder}/{path}"), op.to_string());
        assert_eq!(
            notes
                .iter()
                .filter(|n| n.body.contains(&path) && n.body.contains(&op))
                .count(),
            1,
            "{path} {op}: {notes:?}"
        );
    }
    steps(&fx, 3).await;
    assert_eq!(
        dirty_notes(&fx, &t).len(),
        3,
        "level-triggered but once per (op, file)"
    );
}

#[tokio::test]
async fn worktree_dirty_outside_any_seat_is_not_notified() {
    let fx = fx();
    base(&fx).await;
    steps(&fx, 2).await;
    let ts_folder = layout::list_teamspaces(&view(&fx))
        .unwrap()
        .remove(0)
        .0
        .folder
        .as_str()
        .to_owned();
    append_dirty(&fx, &format!("{ts_folder}/teamspace.toml"), &OpId::new());
    steps(&fx, 3).await;
    let t = thread_of_seat(&fx, "foreman");
    assert!(dirty_notes(&fx, &t).is_empty());
    assert!(
        fx.threads
            .notifications()
            .iter()
            .all(|n| !n.body.contains("left in place")),
        "{:?}",
        fx.threads.notifications()
    );
}

#[tokio::test]
async fn worktree_dirty_for_retired_seat_is_not_notified() {
    let fx = fx();
    base(&fx).await;
    plan_and_apply(
        &fx,
        "seat create second --teamspace alpha --active --harness shell",
    );
    steps(&fx, 3).await;
    let folder = seat_folder(&fx, "second");
    plan_and_apply(&fx, "seat retire second");
    steps(&fx, 3).await;
    append_dirty(&fx, &format!("{folder}/AGENTS.md"), &OpId::new());
    steps(&fx, 3).await;
    assert!(
        fx.threads
            .notifications()
            .iter()
            .all(|n| !n.body.contains("left in place")),
        "{:?}",
        fx.threads.notifications()
    );
}

// ---------------------------------------------------------------------------------------------
// failure handling
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn service_busy_backs_off_no_takeover() {
    let fx = fx();
    plan_and_apply(&fx, "teamspace create alpha");
    plan_and_apply(
        &fx,
        "seat create foreman --teamspace alpha --active --harness shell",
    );
    fx.threads.set_busy(true);
    steps(&fx, 3).await;
    assert!(
        fx.threads.calls().is_empty(),
        "a busy service is never forced: {:?}",
        fx.threads.calls()
    );
    let ensures = rows(&fx, EffectKind::EnsureThread);
    assert_eq!(ensures.len(), 2);
    assert!(
        ensures.iter().all(|r| r.status == EffectStatus::Pending
            && r.last_error.as_deref() == Some("threads service busy")),
        "{ensures:?}"
    );
    let attempts: Vec<u32> = ensures.iter().map(|r| r.attempts).collect();
    steps(&fx, 2).await;
    assert_eq!(
        rows(&fx, EffectKind::EnsureThread)
            .iter()
            .map(|r| r.attempts)
            .collect::<Vec<_>>(),
        attempts,
        "backoff: no retry before it expires"
    );
    // Disconnects back off the same way.
    fx.threads.set_busy(false);
    fx.threads.disconnect();
    later(&fx).await;
    assert!(fx.threads.calls().is_empty());
    assert!(
        rows(&fx, EffectKind::EnsureThread)
            .iter()
            .all(|r| r.status == EffectStatus::Pending)
    );
    fx.threads.reconnect();
    later(&fx).await;
    later(&fx).await;
    assert_eq!(fx.threads.call_count("ensure_thread"), 2);
    assert!(
        rows(&fx, EffectKind::EnsureThread)
            .iter()
            .all(|r| r.status == EffectStatus::Done)
    );
}

#[tokio::test]
async fn seat_map_failure_backs_off() {
    let fx = fx();
    let clone = base(&fx).await;
    let pane = pane_of(&clone_rec(&fx, &clone.id));
    fx.map.set(&pane, "seat-A");
    fx.map.set_failing(true);
    steps(&fx, 3).await;
    assert_eq!(fx.threads.call_count("invite"), 0);
    let resolves = resolve_rows(&fx);
    assert!(
        !resolves.is_empty()
            && resolves
                .iter()
                .all(|r| r.status == EffectStatus::Pending && r.attempts >= 1),
        "{resolves:?}"
    );
    fx.map.set_failing(false);
    later(&fx).await;
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
    assert_eq!(
        reply.native_session,
        clone
            .occupant
            .as_ref()
            .map(|o| o.native_session.to_string())
    );
    assert!(
        reply.render().starts_with("alpha/foreman/"),
        "{}",
        reply.render()
    );
    assert!(
        reply
            .render()
            .ends_with(&reply.native_session.clone().unwrap()),
        "{}",
        reply.render()
    );
    // A threads seat resolves through the recorded invitation link.
    assert_eq!(
        who(&view(&fx), "seat-A").unwrap().clone.unwrap().id,
        clone.id.to_string()
    );
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
    let ctx = CommandCtx {
        request_id: "r".into(),
        caller: CallerInfo::default(),
    };
    let v = h
        .call(ctx.clone(), json!({ "target": pane_of(&clone).0 }))
        .await
        .unwrap();
    let reply: WhoReply = serde_json::from_value(v).unwrap();
    assert_eq!(reply.seat.unwrap().name, "foreman");
    let e = h.call(ctx, json!({})).await.unwrap_err();
    assert!(e.message.contains("target"), "{}", e.message);
}

#[tokio::test]
async fn invitation_mutation_ignores_stale_writes() {
    let fx = fx();
    let clone = base(&fx).await;
    let write = |state: &str, seat: &str, expect: serde_json::Value| {
        json!({
            "clone": clone.id, "thread": "th", "constraint": "required", "state": state,
            "link": { "seat": seat, "occupant": "ns_any" }, "expect_seat": expect,
        })
    };
    bookkeeping(&fx, "invitation", write("pending", "seat-new", json!(null)));
    // A late release about the previous seat must not overwrite the new seat's invitation.
    bookkeeping(
        &fx,
        "invitation",
        write("released", "seat-old", json!("seat-old")),
    );
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.invitations.len(), 1);
    assert_eq!(c.invitations[0].state, InvitationState::Pending);
    assert_eq!(c.invitations[0].link.as_ref().unwrap().seat, "seat-new");
    // A write about the current seat applies, whatever the occupant.
    bookkeeping(
        &fx,
        "invitation",
        write("accepted", "seat-new", json!("seat-new")),
    );
    assert_eq!(
        clone_rec(&fx, &clone.id).invitations[0].state,
        InvitationState::Accepted
    );
}

#[tokio::test]
async fn bookkeeping_channel_rejects_unknown_objects() {
    let fx = fx();
    base(&fx).await;
    bookkeeping(
        &fx,
        "channel",
        json!({ "object": SeatId::new(), "thread_id": "x" }),
    );
    let rejected = fx.journal.list(&[OpState::Rejected], 10).unwrap();
    assert_eq!(rejected.len(), 1);
    assert_eq!(
        rejected[0].rejection.as_ref().unwrap().reason,
        "object_missing"
    );
}

#[cfg(not(feature = "threads-service-ack"))]
#[tokio::test]
async fn delivery_capability_fallback_without_feature() {
    let dir = tempfile::tempdir().unwrap();
    let t = ServiceThreads::with_system_clock(
        dir.path().join("none.sock"),
        dir.path().join("state/intents"),
        herdr_threads::node::NodeUuid::from_bytes(*uuid::Uuid::new_v4().as_bytes()),
    )
    .unwrap();
    assert_eq!(
        t.delivery_capability().await.unwrap(),
        DeliveryCapability::NotifyFallback,
        "no I/O, no registration probe"
    );
    // send_request/receipt_state are real (ht-5nb); with no daemon behind the socket they fail to connect.
    let r = t
        .send_request(
            &ThreadRef("t".into()),
            &[seat_ref("s")],
            "b",
            &OpKey("k".into()),
        )
        .await;
    assert!(r.is_err());
    // The fake reports the same by default and can model a v2 service.
    let fake = FakeThreads::new();
    assert_eq!(
        fake.delivery_capability().await.unwrap(),
        DeliveryCapability::NotifyFallback
    );
    fake.set_capability(DeliveryCapability::ServiceAck);
    assert_eq!(
        fake.delivery_capability().await.unwrap(),
        DeliveryCapability::ServiceAck
    );
}

#[test]
fn threads_seat_reference_helpers() {
    // Long keys are hashed into valid, stable operation ids; short ones pass through.
    let long = OpKey(format!("invite:{}", "x".repeat(300)));
    let id = adapter::operation_id(&long);
    assert!(id.as_str().len() <= 128 && id.as_str().starts_with("h-"));
    assert_eq!(id, adapter::operation_id(&long));
    assert_ne!(
        id,
        adapter::operation_id(&OpKey(format!("invite:{}", "y".repeat(300))))
    );
    assert_eq!(
        adapter::operation_id(&OpKey("ensure:st_X".into())).as_str(),
        "ensure:st_X"
    );
    let _ = Lifecycle::Active;
}

#[tokio::test]
async fn reconnect_recreates_channels_without_replaying_old_effects_or_external_invites() {
    let fx = fx();
    let c = base(&fx).await;
    occupy(&fx, &c, "occupant-seat").await;
    steps(&fx, 4).await;
    let old_thread = thread_of_seat(&fx, "foreman");
    let old_effects = fx.journal.effects_for_object(&c.id.to_any()).unwrap();
    assert!(!old_effects.is_empty());
    let old_head = fx.store.head().unwrap();
    let old_tree = fx.store.at(&old_head);
    let cache = Arc::new(effects::GraphCache::default());
    let old_graph = cache.get(&old_tree, &old_head).unwrap();
    let executor = effects::ThreadsExecutor::new(
        fx.threads.clone(),
        fx.map.clone(),
        fx.journal.clone(),
        cache.clone(),
    );
    plan_and_apply(
        &fx,
        "participation join external-old --scope seat --seat foreman",
    );
    // Capture an admitted old-node channel write before the boundary; submit it after the reconnect.
    let mut stale = crate::transcripts::mutations::bookkeeping_request(
        "channel",
        json!({"object":seat(&fx,"foreman").id,"thread_id":"late-old-id"}),
    );
    namespace::stamp(&mut stale.args, "");
    let head = fx.store.head().unwrap();
    plan_and_apply(
        &fx,
        &format!(
            "threads reconnect --from legacy --to {} --head {}",
            "02".repeat(16),
            head.0
        ),
    );
    let stale_op = fx.w.admit(stale).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(
        fx.journal.get(&stale_op).unwrap().unwrap().state,
        OpState::Rejected
    );
    steps(&fx, 6).await;
    let fresh_thread = thread_of_seat(&fx, "foreman");
    assert_ne!(fresh_thread, old_thread);
    let clone = clone_rec(&fx, &c.id);
    assert!(!clone.invitations.is_empty());
    assert!(clone.invitations.iter().all(|i| i.thread != "external-old"));
    let current_epoch = namespace::epoch(&view(&fx)).unwrap();
    let new_effects = fx.journal.effects_for_object(&c.id.to_any()).unwrap();
    assert!(
        new_effects
            .iter()
            .any(|e| e.nonce_label == Some(format!("threads-epoch:{current_epoch}")))
    );
    for old in old_effects {
        assert!(new_effects.iter().any(|e| e.id == old.id));
    }
    // Warmed old Graph + old effect cannot become target-node work, and a new
    // effect cannot be executed against the old graph snapshot either.
    use crate::reconcile::executor::{EffectExecutor, ExecCx, ExecOutcome};
    let herdr = FakeHerdr::new();
    let snapshot = crate::ports::herdr::HerdrApi::snapshot(herdr.as_ref())
        .await
        .unwrap();
    let current_head = fx.store.head().unwrap();
    let current_tree = fx.store.at(&current_head);
    let cx = ExecCx {
        tree: &current_tree,
        head: &current_head,
        snapshot: &snapshot,
        herdr: herdr.as_ref(),
        writer: fx.w.as_ref(),
        now: fx.clock.now(),
    };
    let before_stale = fx.threads.calls().len();
    let old_effect = new_effects
        .iter()
        .find(|e| e.nonce_label.is_none())
        .unwrap();
    assert!(!executor.is_implied(&cx, old_effect));
    assert_eq!(
        executor.execute(&cx, old_effect).await,
        ExecOutcome::Obsolete
    );
    assert!(old_graph.epoch.is_empty());
    let old_cx = ExecCx {
        tree: &old_tree,
        head: &old_head,
        ..cx
    };
    let new_effect = new_effects
        .iter()
        .find(|e| e.nonce_label.is_some())
        .unwrap();
    assert_eq!(
        executor.execute(&old_cx, new_effect).await,
        ExecOutcome::Obsolete
    );
    assert_eq!(fx.threads.calls().len(), before_stale);
    let fresh_count = fx.threads.calls().len();
    steps(&fx, 3).await;
    assert_eq!(
        fx.threads.calls().len(),
        fresh_count,
        "settled new namespace must not resend"
    );
}

// ---------------------------------------------------------------------------------------------
// threads seat assignment (hg-66u.27)
// ---------------------------------------------------------------------------------------------

use super::assignment::{self, SeatTransition};
use crate::model::clone::SeatIntent;
use crate::model::common::CloneLifecycle;

/// Admit a transition and return the rejection reason, or None when it committed.
fn transition(fx: &Fx, clone: &CloneId, t: SeatTransition) -> Option<String> {
    let req = ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args: json!({ "sub": "threads_seat", "clone": clone, "transition": t }),
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    };
    let op = fx.w.admit(req).unwrap();
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    match row.state {
        OpState::Committed => None,
        _ => Some(row.rejection.expect("rejected op has a rejection").reason),
    }
}

fn assign(seat: &str) -> SeatTransition {
    SeatTransition::Assign { seat: seat.into() }
}

fn write_restore(fx: &Fx, clone: &CloneId, epoch: &OpId) {
    bookkeeping(
        fx,
        "test_seat_fixture",
        json!({ "clone": clone, "op": "restore", "epoch": epoch }),
    );
}

fn intent(fx: &Fx, clone: &CloneId) -> Option<SeatIntent> {
    clone_rec(fx, clone).seat_intent
}

/// A launch mark of `true` is the fixed time `marked_at()`; assert a live mark's time separately.
fn marked_at() -> Timestamp {
    chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap()
}

fn restore(seat: &str, epoch: &OpId, threads_done: bool, launched: bool) -> Option<SeatIntent> {
    Some(SeatIntent::Restore {
        seat: seat.into(),
        epoch: epoch.clone(),
        threads_done,
        launched: launched.then(marked_at),
        last_refusal: None,
    })
}

/// A second active clone, on a second seat.
async fn second(fx: &Fx) -> CloneRecord {
    plan_and_apply(
        fx,
        "seat create builder --teamspace alpha --active --harness shell",
    );
    steps(fx, 3).await;
    let s = seat(fx, "builder");
    clones_of(fx, &s.id).remove(0)
}

fn retire_clone_observed(fx: &Fx, clone: &CloneId) {
    observed(
        fx,
        crate::observe::mutations::cascade_request(
            crate::observe::classify::CascadeRule::Pane,
            &[clone.to_any()],
            crate::ports::clock::Clock::now(&*fx.clock),
        ),
    );
}

#[tokio::test]
async fn threads_seat_assign_is_write_once_and_idempotent() {
    let fx = fx();
    let c = base(&fx).await;
    assert_eq!(c.threads_seat, None);
    assert_eq!(transition(&fx, &c.id, assign("seat-a")), None);
    let rev = clone_rec(&fx, &c.id).rev;
    assert_eq!(
        clone_rec(&fx, &c.id).threads_seat.as_deref(),
        Some("seat-a")
    );
    assert_eq!(transition(&fx, &c.id, assign("seat-a")), None);
    assert_eq!(clone_rec(&fx, &c.id).rev, rev, "same seat writes nothing");
}

#[tokio::test]
async fn threads_seat_assign_rejects_a_different_seat() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    assert_eq!(
        transition(&fx, &c.id, assign("seat-b")).as_deref(),
        Some("threads_seat_conflict")
    );
    assert_eq!(
        clone_rec(&fx, &c.id).threads_seat.as_deref(),
        Some("seat-a")
    );
}

#[tokio::test]
async fn threads_seat_assign_rejects_a_seat_another_live_clone_holds() {
    let fx = fx();
    let a = base(&fx).await;
    let b = second(&fx).await;
    assert_eq!(transition(&fx, &a.id, assign("seat-a")), None);
    assert_eq!(
        transition(&fx, &b.id, assign("seat-a")).as_deref(),
        Some("threads_seat_held")
    );
    assert_eq!(clone_rec(&fx, &b.id).threads_seat, None);
    retire_clone_observed(&fx, &a.id);
    assert_eq!(clone_rec(&fx, &a.id).lifecycle, CloneLifecycle::Retired);
    assert_eq!(transition(&fx, &b.id, assign("seat-a")), None);
    assert_eq!(
        clone_rec(&fx, &b.id).threads_seat.as_deref(),
        Some("seat-a")
    );
}

#[tokio::test]
async fn threads_seat_succession_needs_a_matching_restore_intent() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-old"));
    let epoch = OpId::new();
    let succeed = |seat: &str, from: &str, epoch: &OpId| SeatTransition::Succeed {
        seat: seat.into(),
        from: from.into(),
        epoch: epoch.clone(),
    };
    assert_eq!(
        transition(&fx, &c.id, succeed("seat-new", "seat-old", &epoch)).as_deref(),
        Some("seat_intent_mismatch"),
        "no intent"
    );
    write_restore(&fx, &c.id, &epoch);
    assert_eq!(
        transition(&fx, &c.id, succeed("seat-new", "seat-old", &OpId::new())).as_deref(),
        Some("seat_intent_mismatch"),
        "wrong epoch"
    );
    assert_eq!(
        transition(&fx, &c.id, succeed("seat-new", "seat-other", &epoch)).as_deref(),
        Some("seat_intent_mismatch"),
        "wrong from"
    );
    assert_eq!(
        clone_rec(&fx, &c.id).threads_seat.as_deref(),
        Some("seat-old")
    );
    assert_eq!(
        transition(&fx, &c.id, succeed("seat-new", "seat-old", &epoch)),
        None
    );
    let rec = clone_rec(&fx, &c.id);
    assert_eq!(rec.threads_seat.as_deref(), Some("seat-new"));
    assert_eq!(rec.threads_seat_history, vec!["seat-old".to_string()]);
    assert_eq!(rec.seat_intent, restore("seat-new", &epoch, true, false));
    let rev = rec.rev;
    assert_eq!(
        transition(&fx, &c.id, succeed("seat-new", "seat-old", &epoch)),
        None
    );
    assert_eq!(
        clone_rec(&fx, &c.id).rev,
        rev,
        "replayed succession is idempotent"
    );
}

#[tokio::test]
async fn threads_seat_succession_to_a_seat_a_live_clone_holds_is_rejected() {
    let fx = fx();
    let a = base(&fx).await;
    let b = second(&fx).await;
    transition(&fx, &a.id, assign("seat-old"));
    transition(&fx, &b.id, assign("seat-taken"));
    let epoch = OpId::new();
    write_restore(&fx, &a.id, &epoch);
    assert_eq!(
        transition(
            &fx,
            &a.id,
            SeatTransition::Succeed {
                seat: "seat-taken".into(),
                from: "seat-old".into(),
                epoch
            }
        )
        .as_deref(),
        Some("threads_seat_held")
    );
}

#[tokio::test]
async fn seat_intent_acknowledge_sets_threads_done_and_a_stale_epoch_is_a_noop() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    let epoch = OpId::new();
    write_restore(&fx, &c.id, &epoch);
    assert_eq!(intent(&fx, &c.id), restore("seat-a", &epoch, false, false));
    let before = clone_rec(&fx, &c.id).rev;
    assert_eq!(
        transition(
            &fx,
            &c.id,
            SeatTransition::Acknowledge { epoch: OpId::new() }
        ),
        None
    );
    assert_eq!(intent(&fx, &c.id), restore("seat-a", &epoch, false, false));
    assert_eq!(clone_rec(&fx, &c.id).rev, before);
    assert_eq!(
        transition(
            &fx,
            &c.id,
            SeatTransition::Acknowledge {
                epoch: epoch.clone()
            }
        ),
        None
    );
    assert_eq!(intent(&fx, &c.id), restore("seat-a", &epoch, true, false));
    assert_eq!(
        transition(
            &fx,
            &c.id,
            SeatTransition::Acknowledge {
                epoch: epoch.clone()
            }
        ),
        None,
        "idempotent"
    );
    assert_eq!(intent(&fx, &c.id), restore("seat-a", &epoch, true, false));
}

#[tokio::test]
async fn restore_intent_helper_overwrites_an_older_epoch() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    let (e1, e2) = (OpId::new(), OpId::new());
    write_restore(&fx, &c.id, &e1);
    transition(
        &fx,
        &c.id,
        SeatTransition::Acknowledge { epoch: e1.clone() },
    );
    write_restore(&fx, &c.id, &e2);
    assert_eq!(intent(&fx, &c.id), restore("seat-a", &e2, false, false));
}

#[tokio::test]
async fn restore_intent_helper_writes_nothing_for_a_seatless_clone() {
    let fx = fx();
    let c = base(&fx).await;
    write_restore(&fx, &c.id, &OpId::new());
    assert_eq!(intent(&fx, &c.id), None);
}

#[tokio::test]
async fn mark_launched_is_once_per_epoch() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    let epoch = OpId::new();
    let mark = |e: &OpId| SeatTransition::MarkLaunched { epoch: e.clone() };
    assert_eq!(
        transition(&fx, &c.id, mark(&epoch)).as_deref(),
        Some("seat_intent_mismatch"),
        "no intent"
    );
    write_restore(&fx, &c.id, &epoch);
    assert_eq!(
        transition(&fx, &c.id, mark(&epoch)).as_deref(),
        Some("seat_intent_mismatch"),
        "before threads_done"
    );
    transition(
        &fx,
        &c.id,
        SeatTransition::Acknowledge {
            epoch: epoch.clone(),
        },
    );
    assert_eq!(
        transition(&fx, &c.id, mark(&OpId::new())).as_deref(),
        Some("seat_intent_mismatch"),
        "stale epoch"
    );
    assert_eq!(transition(&fx, &c.id, mark(&epoch)), None);
    let mut marked = intent(&fx, &c.id);
    assert!(
        matches!(&marked, Some(SeatIntent::Restore { launched: Some(t), .. }) if *t == fx.clock.now()),
        "MarkLaunched records the mark time: {marked:?}"
    );
    if let Some(SeatIntent::Restore { launched, .. }) = &mut marked {
        *launched = Some(marked_at());
    }
    assert_eq!(marked, restore("seat-a", &epoch, true, true));
    assert_eq!(
        transition(&fx, &c.id, mark(&epoch)).as_deref(),
        Some("already_launched")
    );
}

#[tokio::test]
async fn clear_on_reattach_clears_only_the_matching_epoch() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    let epoch = OpId::new();
    write_restore(&fx, &c.id, &epoch);
    let clear = |e: &OpId| SeatTransition::ClearOnReattach { epoch: e.clone() };
    assert_eq!(transition(&fx, &c.id, clear(&epoch)), None);
    assert_eq!(
        intent(&fx, &c.id),
        restore("seat-a", &epoch, false, false),
        "not acknowledged yet"
    );
    transition(
        &fx,
        &c.id,
        SeatTransition::Acknowledge {
            epoch: epoch.clone(),
        },
    );
    assert_eq!(transition(&fx, &c.id, clear(&OpId::new())), None);
    assert_eq!(intent(&fx, &c.id), restore("seat-a", &epoch, true, false));
    assert_eq!(transition(&fx, &c.id, clear(&epoch)), None);
    assert_eq!(intent(&fx, &c.id), None);
    assert_eq!(
        clone_rec(&fx, &c.id).threads_seat.as_deref(),
        Some("seat-a")
    );
}

#[tokio::test]
async fn pane_close_after_threads_done_retires_and_clears_the_restore_intent() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    let epoch = OpId::new();
    write_restore(&fx, &c.id, &epoch);
    transition(
        &fx,
        &c.id,
        SeatTransition::Acknowledge {
            epoch: epoch.clone(),
        },
    );
    assert_eq!(intent(&fx, &c.id), restore("seat-a", &epoch, true, false));
    retire_clone_observed(&fx, &c.id);
    let rec = clone_rec(&fx, &c.id);
    assert_eq!(rec.lifecycle, CloneLifecycle::Retired);
    assert_eq!(rec.seat_intent, None);
    assert_eq!(
        rec.threads_seat.as_deref(),
        Some("seat-a"),
        "the seat stays recorded"
    );
}

#[tokio::test]
async fn purge_tombstone_of_a_seated_clone_is_none() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    retire_clone_observed(&fx, &c.id);
    let rec = clone_rec(&fx, &c.id);
    assert_eq!(rec.threads_seat.as_deref(), Some("seat-a"));
    // Before D2 every seat is final: the record is deleted outright, with or without a reservation.
    assert!(assignment::purge_tombstone(&rec, None).is_none());
    assert!(assignment::purge_tombstone(&rec, Some("key-1")).is_none());
}

#[tokio::test]
async fn pre_existing_finalize_tombstone_is_deleted_on_load() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    retire_clone_observed(&fx, &c.id);
    bookkeeping(
        &fx,
        "test_seat_fixture",
        json!({ "clone": c.id, "op": "raw_finalize" }),
    );
    let t = clone_rec(&fx, &c.id);
    assert!(t.is_tombstone());
    assert_eq!(
        t.seat_intent,
        Some(SeatIntent::Finalize {
            seat: "seat-a".into()
        })
    );
    let before = fx.threads.calls().len();
    steps(&fx, 4).await;
    later(&fx).await;
    assert!(
        layout::all_clones(&view(&fx))
            .unwrap()
            .iter()
            .all(|(_, r)| r.id != c.id),
        "the pre-existing tombstone is deleted"
    );
    let new: Vec<_> = fx.threads.calls().split_off(before);
    assert!(
        new.iter().all(|c| !c.contains("seat-a")),
        "no threads call for the dropped tombstone: {new:?}"
    );
    // A second pass plans nothing for it.
    let head = fx.store.head().unwrap();
    steps(&fx, 2).await;
    assert_eq!(fx.store.head().unwrap(), head, "nothing left to do");
}

#[tokio::test]
async fn purge_tombstone_of_a_seatless_clone_with_a_reservation_is_release_key() {
    let fx = fx();
    let c = base(&fx).await;
    let t = assignment::purge_tombstone(&clone_rec(&fx, &c.id), Some("key-1")).unwrap();
    assert_eq!(
        t.seat_intent,
        Some(SeatIntent::ReleaseKey {
            key: "key-1".into()
        })
    );
    assert!(t.is_tombstone());
    assert_eq!(t.threads_seat, None);
    // A held seat wins over a reservation: no ReleaseKey, the record is deleted (pre-D2).
    transition(&fx, &c.id, assign("seat-a"));
    assert!(assignment::purge_tombstone(&clone_rec(&fx, &c.id), Some("key-1")).is_none());
}

#[tokio::test]
async fn purge_tombstone_of_a_seatless_unreserved_clone_is_none() {
    let fx = fx();
    let c = base(&fx).await;
    assert!(assignment::purge_tombstone(&clone_rec(&fx, &c.id), None).is_none());
}

#[tokio::test]
async fn tombstone_done_deletes_only_tombstones() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    assert_eq!(
        transition(&fx, &c.id, SeatTransition::TombstoneDone).as_deref(),
        Some("not_a_tombstone")
    );
    retire_clone_observed(&fx, &c.id);
    bookkeeping(
        &fx,
        "test_seat_fixture",
        json!({ "clone": c.id, "op": "raw_finalize" }),
    );
    assert!(clone_rec(&fx, &c.id).is_tombstone());
    assert_eq!(transition(&fx, &c.id, SeatTransition::TombstoneDone), None);
    assert!(
        layout::all_clones(&view(&fx))
            .unwrap()
            .iter()
            .all(|(_, r)| r.id != c.id),
        "tombstone record is gone"
    );
}

// ---------------------------------------------------------------------------------------------
// resolve-seat want
// ---------------------------------------------------------------------------------------------

fn resolve_rows(fx: &Fx) -> Vec<EffectRecord> {
    rows(fx, EffectKind::Custom(super::effects::RESOLVE_KIND.into()))
}

/// Stamp the clone's binding the way the observer does (token, Present).
fn set_binding(fx: &Fx, clone: &CloneId, token: bool, present: bool, pane: Option<&HerdrPaneId>) {
    bookkeeping(
        fx,
        "test_set_binding",
        json!({ "clone": clone, "token": token, "present": present, "pane": pane.map(|p| p.0.clone()) }),
    );
}

#[tokio::test]
async fn resolve_want_fires_only_on_verified_bindings() {
    let fx = fx();
    let c = base(&fx).await;
    // The launcher already stamped and observed the pane, so base() resolved once (threads had no seat yet).
    let pane = pane_of(&clone_rec(&fx, &c.id));
    let before = fx.map.resolve_ops().len();
    assert!(before >= 1);
    fx.map.set(&pane, "seat-A");
    for (token, present, has_pane) in [
        (false, true, true),
        (true, false, true),
        (true, true, false),
    ] {
        set_binding(&fx, &c.id, token, present, has_pane.then_some(&pane));
        steps(&fx, 2).await;
        assert_eq!(
            fx.map.resolve_ops().len(),
            before,
            "{token} {present} {has_pane}"
        );
        assert_eq!(clone_rec(&fx, &c.id).threads_seat, None);
    }
    set_binding(&fx, &c.id, true, true, Some(&pane));
    steps(&fx, 2).await;
    assert_eq!(
        clone_rec(&fx, &c.id).threads_seat.as_deref(),
        Some("seat-A")
    );
    let seated = fx.map.resolve_ops().len();
    assert!(seated > before);
    // Already seated: nothing further is resolved.
    steps(&fx, 2).await;
    assert_eq!(fx.map.resolve_ops().len(), seated);
}

#[tokio::test]
async fn resolve_refusal_defers_and_reuses_the_op_id() {
    let fx = fx();
    let c = base(&fx).await;
    let pane = pane_of(&clone_rec(&fx, &c.id));
    set_binding(&fx, &c.id, true, true, Some(&pane));
    fx.map.set_not_yet(&pane, "restore hold");
    steps(&fx, 2).await;
    later(&fx).await;
    later(&fx).await;
    let rs = resolve_rows(&fx);
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0].status, EffectStatus::Pending);
    assert_eq!(
        rs[0].attempts, 0,
        "a refusal defers without counting an attempt"
    );
    assert_eq!(clone_rec(&fx, &c.id).threads_seat, None);
    fx.map.clear_not_yet(&pane);
    fx.map.set(&pane, "seat-A");
    later(&fx).await;
    later(&fx).await;
    assert_eq!(
        clone_rec(&fx, &c.id).threads_seat.as_deref(),
        Some("seat-A")
    );
    let ops = fx.map.resolve_ops();
    assert!(ops.len() >= 3, "{ops:?}");
    assert!(ops.iter().all(|o| o == &ops[0]), "{ops:?}");
}

#[tokio::test]
async fn live_clones_are_backfilled() {
    let fx = fx();
    let c = base(&fx).await;
    occupy(&fx, &c, "seat-A").await;
    set_binding(&fx, &c.id, true, true, Some(&pane_of(&c)));
    steps(&fx, 4).await;
    let after = clone_rec(&fx, &c.id);
    assert_eq!(after.threads_seat.as_deref(), Some("seat-A"));
    assert!(after.occupant.is_some());
    assert_eq!(
        fx.threads.call_count("invite"),
        2,
        "invitations are unaffected"
    );
}

#[tokio::test]
async fn unreachable_threads_writes_no_seat_and_retry_reuses_the_op_id() {
    let fx = fx();
    let c = base(&fx).await;
    let pane = pane_of(&clone_rec(&fx, &c.id));
    set_binding(&fx, &c.id, true, true, Some(&pane));
    fx.map.set(&pane, "seat-A");
    fx.map.set_failing(true);
    steps(&fx, 2).await;
    later(&fx).await;
    assert_eq!(clone_rec(&fx, &c.id).threads_seat, None);
    let failed = fx.map.resolve_ops();
    assert!(!failed.is_empty());
    fx.map.set_failing(false);
    later(&fx).await;
    later(&fx).await;
    assert_eq!(
        clone_rec(&fx, &c.id).threads_seat.as_deref(),
        Some("seat-A")
    );
    let ops = fx.map.resolve_ops();
    assert!(ops.len() > failed.len());
    assert!(ops.iter().all(|o| o == &ops[0]), "{ops:?}");
}

#[tokio::test]
async fn no_resolve_for_a_clone_with_a_pending_restore_intent() {
    let fx = fx();
    let c = base(&fx).await;
    // A seatless clone carrying a Restore intent (a state the commits never write, §14.5) belongs to the
    // succession want: no resolution even with a verified binding.
    bookkeeping(
        &fx,
        "test_seat_fixture",
        json!({ "clone": c.id, "op": "raw_restore", "epoch": OpId::new() }),
    );
    let pane = pane_of(&clone_rec(&fx, &c.id));
    set_binding(&fx, &c.id, true, true, Some(&pane));
    fx.map.set(&pane, "seat-new");
    let rec = clone_rec(&fx, &c.id);
    assert!(matches!(rec.seat_intent, Some(SeatIntent::Restore { .. })));
    assert_eq!(rec.threads_seat, None);
    let before = fx.map.resolve_ops().len();
    steps(&fx, 3).await;
    assert_eq!(fx.map.resolve_ops().len(), before);
    assert_eq!(clone_rec(&fx, &c.id).threads_seat, None);
}

#[tokio::test]
async fn resolve_rejected_by_assignment_needs_revision() {
    let fx = fx();
    let a = base(&fx).await;
    let b = second(&fx).await;
    assert_eq!(transition(&fx, &a.id, assign("seat-A")), None);
    let pane = pane_of(&clone_rec(&fx, &b.id));
    set_binding(&fx, &b.id, true, true, Some(&pane));
    fx.map.set(&pane, "seat-A");
    steps(&fx, 3).await;
    assert_eq!(clone_rec(&fx, &b.id).threads_seat, None);
    let rs: Vec<_> = resolve_rows(&fx)
        .into_iter()
        .filter(|r| r.status == EffectStatus::NeedsRevision)
        .collect();
    assert!(!rs.is_empty());
    assert!(rs.iter().all(|r| r.object.as_str() == b.id.to_string()));
    let mine: Vec<_> = fx
        .map
        .resolve_ops()
        .into_iter()
        .filter(|o| o.0.contains(&b.id.to_string()))
        .collect();
    assert!(
        mine.iter().all(|o| o == &mine[0]),
        "never retried under another op id: {mine:?}"
    );
}

// --- ThreadsSeatView: wire -> SeatStatus (hg-66u.6) ---

/// A v3.0.0 `SeatInspect` result as the daemon serializes it, for one seat in the given state.
fn inspection_fixture(
    continuity: &str,
    target: Option<&str>,
    retired_at: Option<i64>,
) -> herdr_threads::protocol::results::SeatInspection {
    serde_json::from_value(json!({
        "summary": {
            "seat": "seat-a",
            "continuity": continuity,
            "target": target,
            "generation": 2,
            "created_at": 1_000,
            "retired_at": retired_at,
        },
        "mapping": { "state": continuity, "target": target, "detail_argv": null },
        "hold": null,
        "retirement": null,
        "history": {
            "items": [],
            "next_cursor": null,
            "next_argv": null,
            "high_water_ordinal": 0,
            "scope_revision": null,
            "has_more": false,
            "stop_reason": "complete",
            "consistency": "bounded_live",
        },
    }))
    .unwrap()
}

#[test]
fn seat_status_maps_v3_inspection_fixtures() {
    use super::mapping::seat_status;
    let pane = |p: &str| HerdrPaneId(p.into());
    assert_eq!(
        seat_status(&inspection_fixture("resolved", Some("w1:p2"), None)),
        SeatStatus::Resolved {
            pane: pane("w1:p2")
        }
    );
    assert_eq!(
        seat_status(&inspection_fixture("resolved", None, None)),
        SeatStatus::Unresolved,
        "resolved without a target names no pane"
    );
    assert_eq!(
        seat_status(&inspection_fixture("unresolved", None, None)),
        SeatStatus::Unresolved
    );
    assert_eq!(
        seat_status(&inspection_fixture("retired", None, Some(5_000))),
        SeatStatus::Retired {
            kind: RetirementKind::Final
        }
    );
    assert_eq!(
        seat_status(&inspection_fixture("retired", Some("w1:p2"), Some(5_000))),
        SeatStatus::Retired {
            kind: RetirementKind::Final
        },
        "a retired seat is never reported as bound to its old pane"
    );
    assert_eq!(
        seat_status(&inspection_fixture("remote", Some("w9:p9"), None)),
        SeatStatus::Unresolved,
        "a remote seat is never bound here"
    );
}

#[tokio::test]
async fn fake_seat_view_errors_are_errors() {
    use super::fake::{FakeSeatViewError, FakeThreadsSeatView};
    let view = FakeThreadsSeatView::new();
    let (a, b) = (seat_ref("a"), seat_ref("b"));
    view.set(
        &a,
        SeatStatus::Retired {
            kind: RetirementKind::Final,
        },
    );
    let read = view.inspect(&[a.clone(), b.clone()]).await.unwrap();
    assert_eq!(
        read[&a],
        SeatStatus::Retired {
            kind: RetirementKind::Final
        }
    );
    assert_eq!(
        read[&b],
        SeatStatus::Unresolved,
        "an unset seat reads Unresolved"
    );
    for (injected, check) in [
        (FakeSeatViewError::Disconnected, "disconnected"),
        (FakeSeatViewError::Busy, "busy"),
        (FakeSeatViewError::Rejected, "rejected"),
        (FakeSeatViewError::Unsupported, "unsupported"),
    ] {
        view.set_error(Some(injected));
        let err = view.inspect(std::slice::from_ref(&a)).await.unwrap_err();
        let kind = match err {
            ThreadsError::Disconnected(_) => "disconnected",
            ThreadsError::ServiceBusy => "busy",
            ThreadsError::Rejected(_) => "rejected",
            ThreadsError::Unsupported => "unsupported",
        };
        assert_eq!(kind, check);
    }
    view.set_error(None);
    assert!(view.inspect(&[a]).await.is_ok());
    assert_eq!(view.reads(), 6);
}

#[tokio::test]
async fn clear_on_reattach_clears_the_restore_and_returns_the_clone_to_the_ordinary_rule() {
    let fx = fx();
    let c = base(&fx).await;
    transition(&fx, &c.id, assign("seat-a"));
    let epoch = OpId::new();
    write_restore(&fx, &c.id, &epoch);
    let fact = |state: serde_json::Value| {
        bookkeeping(
            &fx,
            "test_seat_fixture",
            json!({ "clone": c.id, "op": "fact", "seat": "seat-a", "state": state }),
        );
    };
    let resolved = json!({ "state": "resolved", "pane": pane_of(&c).0 });
    // Reversibly retired: the succession want asks nothing of threads, so the Restore stays unacknowledged.
    fact(json!({ "state": "retired", "reversible": true }));
    bookkeeping(&fx, "test_set_occupant", json!({ "clone": c.id }));
    let clear_rows = || {
        rows(
            &fx,
            EffectKind::Custom(super::effects::CLEAR_RESTORE_KIND.into()),
        )
    };

    // Threads has not acknowledged the Restore yet: nothing to clear.
    steps(&fx, 3).await;
    assert!(clear_rows().is_empty());
    assert_eq!(intent(&fx, &c.id), restore("seat-a", &epoch, false, false));

    transition(
        &fx,
        &c.id,
        SeatTransition::Acknowledge {
            epoch: epoch.clone(),
        },
    );
    // Acknowledged but threads does not read the seat as Resolved: the agent has not reattached.
    fact(json!({ "state": "unresolved" }));
    steps(&fx, 3).await;
    assert!(clear_rows().is_empty());
    assert_eq!(intent(&fx, &c.id), restore("seat-a", &epoch, true, false));

    // Resolved under the Restore's seat and an agent in the pane: the intent clears.
    fact(resolved);
    steps(&fx, 3).await;
    assert_eq!(intent(&fx, &c.id), None);
    assert_eq!(clear_rows().len(), 1);
    assert_eq!(clear_rows()[0].status, EffectStatus::Done);
    assert_eq!(
        clone_rec(&fx, &c.id).threads_seat.as_deref(),
        Some("seat-a")
    );
}

// --- seat succession on undo and resurrection (hg-66u.29) ---

fn succeed_rows(fx: &Fx) -> Vec<EffectRecord> {
    rows(fx, EffectKind::Custom(super::effects::SUCCEED_KIND.into()))
}

fn ack_rows(fx: &Fx) -> Vec<EffectRecord> {
    rows(
        fx,
        EffectKind::Custom(super::effects::ACK_RESTORE_KIND.into()),
    )
}

fn seat_fact(fx: &Fx, clone: &CloneId, seat: &str, state: serde_json::Value) {
    bookkeeping(
        fx,
        "test_seat_fixture",
        json!({ "clone": clone, "op": "fact", "seat": seat, "state": state }),
    );
}

fn retired_final() -> serde_json::Value {
    json!({ "state": "retired", "reversible": false })
}

/// A seated clone with accepted memberships whose pane close retired the seat for good, then an undo:
/// a Restore intent for the old seat on a reopened pane that threads now maps to `seat-B`.
async fn undone_clone(fx: &Fx) -> (CloneRecord, OpId, ThreadRef, ThreadRef) {
    let clone = base(fx).await;
    occupy(fx, &clone, "seat-A").await;
    steps(fx, 2).await;
    let (st, ts) = (thread_of_seat(fx, "foreman"), thread_of_ts(fx));
    fx.threads.accept(&st, &seat_ref("seat-A"));
    fx.threads.accept(&ts, &seat_ref("seat-A"));
    later(fx).await;
    let epoch = OpId::new();
    write_restore(fx, &clone.id, &epoch);
    seat_fact(fx, &clone.id, "seat-A", retired_final());
    let pane = pane_of(&clone_rec(fx, &clone.id));
    fx.map.set(&pane, "seat-B");
    (clone, epoch, st, ts)
}

#[tokio::test]
async fn pane_close_then_undo_succeeds_onto_the_reopened_pane_seat() {
    let fx = fx();
    let (clone, epoch, st, ts) = undone_clone(&fx).await;
    steps(&fx, 5).await;
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.threads_seat.as_deref(), Some("seat-B"));
    assert_eq!(c.threads_seat_history, vec!["seat-A".to_owned()]);
    assert_eq!(
        c.seat_intent,
        restore("seat-B", &epoch, true, false),
        "succession acknowledges the Restore in the same write"
    );
    assert_eq!(c.lifecycle, CloneLifecycle::Active);
    assert_eq!(succeed_rows(&fx).len(), 1);
    assert_eq!(succeed_rows(&fx)[0].status, EffectStatus::Done);
    // Memberships are re-invited once on the successor and the old seat's requirements released.
    for t in [&st, &ts] {
        let a = fx.threads.member(t, &seat_ref("seat-A")).unwrap();
        let b = fx.threads.member(t, &seat_ref("seat-B")).unwrap();
        assert_eq!(a.state, InvitationState::Released, "{}", t.0);
        assert_eq!(
            (b.constraint, b.state),
            (InviteConstraint::Required, InvitationState::Pending),
            "{}",
            t.0
        );
        assert_eq!(inv(&c, t).link.as_ref().unwrap().seat, "seat-B");
    }
    let invites = fx.threads.call_count("invite");
    steps(&fx, 3).await;
    assert_eq!(fx.threads.call_count("invite"), invites, "invited once");
    assert_eq!(succeed_rows(&fx).len(), 1, "no second succession");
}

#[tokio::test]
async fn seat_reactivation_succeeds_the_seat_and_reinvites_memberships() {
    let fx = fx();
    let clone = base(&fx).await;
    occupy(&fx, &clone, "seat-A").await;
    steps(&fx, 2).await;
    let (st, ts) = (thread_of_seat(&fx, "foreman"), thread_of_ts(&fx));
    fx.threads.accept(&st, &seat_ref("seat-A"));
    fx.threads.accept(&ts, &seat_ref("seat-A"));
    later(&fx).await;
    plan_and_apply(&fx, "seat deactivate foreman");
    steps(&fx, 2).await; // the tab closes
    plan_and_apply(&fx, "seat activate foreman");
    let Some(SeatIntent::Restore { epoch, .. }) = clone_rec(&fx, &clone.id).seat_intent else {
        panic!("activation writes a Restore intent");
    };
    steps(&fx, 3).await; // the tab and pane are recreated and bound
    // Observer stand-in: the closed pane's seat reads final-retired; threads maps the reopened pane to seat-B.
    seat_fact(&fx, &clone.id, "seat-A", retired_final());
    fx.map.set(&pane_of(&clone_rec(&fx, &clone.id)), "seat-B");
    steps(&fx, 5).await;
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.threads_seat.as_deref(), Some("seat-B"));
    assert_eq!(c.threads_seat_history, vec!["seat-A".to_owned()]);
    assert_eq!(c.seat_intent, restore("seat-B", &epoch, true, false));
    assert_eq!(c.lifecycle, CloneLifecycle::Active);
    assert_eq!(succeed_rows(&fx).len(), 1);
    assert_eq!(succeed_rows(&fx)[0].status, EffectStatus::Done);
    for t in [&st, &ts] {
        let a = fx.threads.member(t, &seat_ref("seat-A")).unwrap();
        let b = fx.threads.member(t, &seat_ref("seat-B")).unwrap();
        assert_eq!(a.state, InvitationState::Released, "{}", t.0);
        assert_eq!(
            (b.constraint, b.state),
            (InviteConstraint::Required, InvitationState::Pending),
            "{}",
            t.0
        );
        assert_eq!(inv(&c, t).link.as_ref().unwrap().seat, "seat-B");
    }
    let invites = fx.threads.call_count("invite");
    steps(&fx, 3).await;
    assert_eq!(fx.threads.call_count("invite"), invites, "invited once");
    assert_eq!(succeed_rows(&fx).len(), 1, "no second succession");
}

#[tokio::test]
async fn stale_fact_from_before_the_restore_is_not_acted_on() {
    // A clone retired through the graph keeps the fact it had; undo writes the Restore intent over it. The
    // old Resolved fact must neither acknowledge the Restore nor start a launch into the final-retired seat.
    let fx = fx();
    let clone = base(&fx).await;
    seat_the_clone(&fx, &clone, "seat-A");
    seat_fact(
        &fx,
        &clone.id,
        "seat-A",
        json!({ "state": "resolved", "pane": "p-old" }),
    );
    let epoch = OpId::new();
    write_restore(&fx, &clone.id, &epoch);
    assert_eq!(clone_rec(&fx, &clone.id).seat_fact, None);
    steps(&fx, 1).await;
    assert!(ack_rows(&fx).is_empty() && succeed_rows(&fx).is_empty());
    assert_eq!(
        clone_rec(&fx, &clone.id).seat_intent,
        restore("seat-A", &epoch, false, false)
    );
}

#[tokio::test]
async fn resurrection_succeeds_the_seat() {
    // A resurrection commit writes the same Restore intent as an undo (`do_resurrect_seat`); the succession
    // want reads only the intent and the committed seat fact, so a resurrected clone follows the same path.
    let fx = fx();
    let clone = base(&fx).await;
    seat_the_clone(&fx, &clone, "seat-A");
    let epoch = OpId::new();
    write_restore(&fx, &clone.id, &epoch);
    seat_fact(&fx, &clone.id, "seat-A", retired_final());
    fx.map.set(&pane_of(&clone_rec(&fx, &clone.id)), "seat-R");
    steps(&fx, 4).await;
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.threads_seat.as_deref(), Some("seat-R"));
    assert_eq!(c.seat_intent, restore("seat-R", &epoch, true, false));
}

#[tokio::test]
async fn live_or_unresolved_seat_is_never_succeeded() {
    for (name, state) in [
        ("resolved", json!({ "state": "resolved", "pane": "p-live" })),
        ("unresolved", json!({ "state": "unresolved" })),
    ] {
        let fx = fx();
        let clone = base(&fx).await;
        seat_the_clone(&fx, &clone, "seat-A");
        let epoch = OpId::new();
        write_restore(&fx, &clone.id, &epoch);
        seat_fact(&fx, &clone.id, "seat-A", state);
        fx.map.set(&pane_of(&clone_rec(&fx, &clone.id)), "seat-B");
        let before = fx.map.resolve_ops().len();
        steps(&fx, 4).await;
        let c = clone_rec(&fx, &clone.id);
        assert_eq!(c.threads_seat.as_deref(), Some("seat-A"), "{name}");
        assert!(c.threads_seat_history.is_empty(), "{name}");
        assert_eq!(
            c.seat_intent,
            restore("seat-A", &epoch, true, false),
            "{name}: acknowledged with no threads command"
        );
        assert_eq!(fx.map.resolve_ops().len(), before, "{name}: no resolve");
        assert!(succeed_rows(&fx).is_empty(), "{name}");
        assert_eq!(ack_rows(&fx).len(), 1, "{name}");
    }
}

#[tokio::test]
async fn reversible_retirement_and_key_conflict_are_left_alone() {
    for (name, state) in [
        (
            "reversible",
            json!({ "state": "retired", "reversible": true }),
        ),
        (
            "key conflict",
            json!({ "state": "key_conflict", "pane": null }),
        ),
    ] {
        let fx = fx();
        let clone = base(&fx).await;
        seat_the_clone(&fx, &clone, "seat-A");
        let epoch = OpId::new();
        write_restore(&fx, &clone.id, &epoch);
        seat_fact(&fx, &clone.id, "seat-A", state);
        fx.map.set(&pane_of(&clone_rec(&fx, &clone.id)), "seat-B");
        steps(&fx, 4).await;
        let c = clone_rec(&fx, &clone.id);
        assert_eq!(c.threads_seat.as_deref(), Some("seat-A"), "{name}");
        assert_eq!(
            c.seat_intent,
            restore("seat-A", &epoch, false, false),
            "{name}"
        );
        assert!(
            succeed_rows(&fx).is_empty() && ack_rows(&fx).is_empty(),
            "{name}"
        );
    }
}

#[tokio::test]
async fn unreachable_threads_no_succession() {
    let fx = fx();
    let (clone, epoch, _, _) = undone_clone(&fx).await;
    let setup = fx.map.resolve_ops().len();
    fx.map.set_failing(true);
    steps(&fx, 2).await;
    later(&fx).await;
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.threads_seat.as_deref(), Some("seat-A"));
    assert!(c.threads_seat_history.is_empty());
    assert_eq!(c.seat_intent, restore("seat-A", &epoch, false, false));
    let failed = fx.map.resolve_ops();
    assert!(failed.len() > setup, "the resolution was attempted");
    // Threads returns: the retry replays the same operation key and succeeds.
    fx.map.set_failing(false);
    later(&fx).await;
    later(&fx).await;
    let c = clone_rec(&fx, &clone.id);
    assert_eq!(c.threads_seat.as_deref(), Some("seat-B"));
    let ops = fx.map.resolve_ops();
    assert!(ops.len() > failed.len());
    let ops = &ops[setup..];
    assert!(ops.iter().all(|o| o == &ops[0]), "{ops:?}");
    assert!(ops[0].0.contains(&format!("succeed:{}:{epoch}", clone.id)));
}

#[tokio::test]
async fn succession_refusal_defers_and_is_never_a_fresh_allocation() {
    let fx = fx();
    let (clone, epoch, _, _) = undone_clone(&fx).await;
    let setup = fx.map.resolve_ops().len();
    let pane = pane_of(&clone_rec(&fx, &clone.id));
    fx.map.set_not_yet(&pane, "restore hold");
    steps(&fx, 2).await;
    later(&fx).await;
    let rs = succeed_rows(&fx);
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0].status, EffectStatus::Pending);
    // The refusal is remembered on the intent for status; everything else is as written.
    let mut pending = clone_rec(&fx, &clone.id).seat_intent;
    if let Some(SeatIntent::Restore { last_refusal, .. }) = &mut pending {
        assert_eq!(last_refusal.take().as_deref(), Some("restore hold"));
    }
    assert_eq!(pending, restore("seat-A", &epoch, false, false));
    fx.map.clear_not_yet(&pane);
    later(&fx).await;
    later(&fx).await;
    assert_eq!(
        clone_rec(&fx, &clone.id).threads_seat.as_deref(),
        Some("seat-B")
    );
    let ops = fx.map.resolve_ops();
    let ops = &ops[setup..];
    assert!(ops.len() >= 3, "{ops:?}");
    assert!(ops.iter().all(|o| o == &ops[0]), "{ops:?}");
}

#[tokio::test]
async fn refused_succession_is_shown_with_its_last_refusal() {
    let fx = fx();
    let (clone, epoch, _, _) = undone_clone(&fx).await;
    let pane = pane_of(&clone_rec(&fx, &clone.id));
    fx.map.set_not_yet(&pane, "restore hold");
    steps(&fx, 2).await;
    later(&fx).await;
    let rs = succeed_rows(&fx);
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0].status, EffectStatus::Pending, "Deferred, not failed");
    let Some(SeatIntent::Restore {
        threads_done: false,
        last_refusal: Some(why),
        ..
    }) = clone_rec(&fx, &clone.id).seat_intent
    else {
        panic!("{:?}", clone_rec(&fx, &clone.id).seat_intent);
    };
    assert!(why.contains("restore hold"), "{why}");
    let st = crate::daemon::clone_status::clone_status_with_journal(&view(&fx), &fx.journal);
    let stuck = st["stuck_restores"].as_array().unwrap();
    assert_eq!(stuck.len(), 1, "{st}");
    assert_eq!(stuck[0]["clone"], clone.id.to_string());
    assert_eq!(stuck[0]["epoch"], epoch.to_string());
    assert_eq!(stuck[0]["reason"], "threads_pending");
    assert!(
        stuck[0]["last_refusal"]
            .as_str()
            .unwrap()
            .contains("restore hold")
    );
    fx.map.clear_not_yet(&pane);
    later(&fx).await;
    later(&fx).await;
    let done = clone_rec(&fx, &clone.id);
    assert_eq!(done.threads_seat.as_deref(), Some("seat-B"));
    assert!(
        matches!(
            done.seat_intent,
            Some(SeatIntent::Restore {
                last_refusal: None,
                ..
            })
        ),
        "{:?}",
        done.seat_intent
    );
    let st = crate::daemon::clone_status::clone_status_with_journal(&view(&fx), &fx.journal);
    assert!(
        st["stuck_restores"].as_array().unwrap().is_empty(),
        "{st}"
    );
}

#[tokio::test]
async fn no_resolve_for_a_pending_restore_except_succession() {
    let fx = fx();
    let (clone, epoch, _, _) = undone_clone(&fx).await;
    let setup = fx.map.resolve_ops().len();
    steps(&fx, 5).await;
    let ops = fx.map.resolve_ops();
    let ops = &ops[setup..];
    let key = format!("succeed:{}:{epoch}", clone.id);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().all(|o| o.0.contains(&key)),
        "only the succession resolves: {ops:?}"
    );
    assert!(
        resolve_rows(&fx)
            .iter()
            .all(|r| r.status != EffectStatus::Done || r.object.as_str() != clone.id.to_string()),
        "the ordinary resolve want never ran for the clone"
    );
}

#[tokio::test]
async fn seatless_closed_clone_takes_a_first_bind_when_undone() {
    // A clone closed before the upgrade has no threads_seat; the undo writes no Restore (§14.5), so the
    // ordinary resolve want gives it its first bind once the reopened pane is verified.
    let fx = fx();
    let c = base(&fx).await;
    let pane = pane_of(&clone_rec(&fx, &c.id));
    set_binding(&fx, &c.id, true, true, Some(&pane));
    write_restore(&fx, &c.id, &OpId::new());
    assert_eq!(intent(&fx, &c.id), None, "no Restore for a seatless clone");
    fx.map.set(&pane, "seat-first");
    steps(&fx, 3).await;
    let after = clone_rec(&fx, &c.id);
    assert_eq!(after.threads_seat.as_deref(), Some("seat-first"));
    assert!(after.threads_seat_history.is_empty());
    assert!(
        succeed_rows(&fx).is_empty(),
        "a first bind is not a succession"
    );
}
