//! Reconciler tests against FakeHerdr, a real writer/journal/git store and the plan engine.
use super::bookkeeping::{admit_binding, admit_runtime};
use super::*;
use crate::herdr::fake::{FakeCall, FakeHerdr, Fault};
use crate::model::clone::CloneRecord;
use crate::model::common::{Availability, Binding, Lifecycle, Occupant};
use crate::model::effect::{ContainerKind, EndState};
use crate::model::harness::{Harness, StartOutcome};
use crate::model::native_session::NativeSession;
use crate::model::seat::SeatRecord;
use crate::model::{AnyId, HerdrPaneId, NsId, SeatId};
use crate::plan::commands::{PlanDeps, admit_apply, create_plan};
use crate::plan::core_kinds::register_core_kinds;
use crate::plan::kind::KindRegistry;
use crate::plan::store::PlanStore;
use crate::plan::types::StoredPlan;
use crate::daemon::registry::CallerInfo;
use crate::model::operation::OpState;
use crate::ports::clock::ManualClock;
use crate::ports::herdr::{AgentInfo, AgentStatus, KeyInput, ProcessInfo};
use crate::store::init::init_instance;
use crate::store::layout;
use crate::store::GitStore;
use crate::writer::{Applied, Mutation, MutationCx, MutationError, WriterConfig, WriterCore};
use chrono::TimeZone;
use serde_json::json;
use std::sync::Mutex;

fn t0() -> Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

#[derive(Default)]
struct RecordingNotifier(Mutex<Vec<(OpId, Severity, String)>>);
impl RecordingNotifier {
    fn messages(&self) -> Vec<String> {
        self.0.lock().unwrap().iter().map(|m| m.2.clone()).collect()
    }
}
impl RequesterNotifier for RecordingNotifier {
    fn notify(&self, op: &OpId, severity: Severity, text: &str) {
        self.0.lock().unwrap().push((op.clone(), severity, text.to_owned()));
    }
}

/// Test-only mutations: the real `seat override` and occupant observation belong to other tasks.
struct TestSetModel;
impl Mutation for TestSetModel {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let seat: SeatId = cx.request.args["seat"].as_str().unwrap().parse().unwrap();
        let loc = cx.tree.locate(&seat.to_any())?.unwrap();
        let mut rec: SeatRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        rec.overrides.model = cx.request.args["model"].as_str().map(str::to_owned);
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "set model".into(), action: None })
    }
}

struct TestSetOccupant;
impl Mutation for TestSetOccupant {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let clone: CloneId = cx.request.args["clone"].as_str().unwrap().parse().unwrap();
        let native = cx.request.args["native"].as_str().unwrap().to_owned();
        let harness: Harness = serde_json::from_value(cx.request.args["harness"].clone()).unwrap();
        let loc = cx.tree.locate(&clone.to_any())?.unwrap();
        let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        let ns = NativeSession {
            id: NsId::new(),
            harness,
            native_session_id: native,
            transcript_path: None,
            transcript: None,
            cwd: "/".into(),
            started: cx.now,
            ended: None,
            end_reason: None,
        };
        rec.occupant = Some(Occupant { native_session: ns.id.clone(), harness, since: cx.now });
        rec.sessions.push(ns);
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "set occupant".into(), action: None })
    }
}

/// Test-only: gives a seat its channel thread (the notifier's delivery address).
struct TestSetThread;
impl Mutation for TestSetThread {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let seat: SeatId = cx.request.args["seat"].as_str().unwrap().parse().unwrap();
        let loc = cx.tree.locate(&seat.to_any())?.unwrap();
        let mut rec: SeatRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        rec.channel.thread_id = cx.request.args["thread"].as_str().map(str::to_owned);
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "set thread".into(), action: None })
    }
}

/// Test-only stand-in for the observer's occupancy end: clears the occupant and ends its session.
struct TestEndOccupant;
impl Mutation for TestEndOccupant {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let clone: CloneId = cx.request.args["clone"].as_str().unwrap().parse().unwrap();
        let native = cx.request.args["native"].as_str().unwrap();
        let loc = cx.tree.locate(&clone.to_any())?.unwrap();
        let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        rec.occupant = None;
        for ns in rec.sessions.iter_mut().filter(|n| n.native_session_id == native) {
            ns.ended = Some(cx.now);
        }
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "end occupant".into(), action: None })
    }
}

struct Fx {
    _tmp: tempfile::TempDir,
    deps: PlanDeps,
    w: Arc<WriterCore>,
    store: Arc<GitStore>,
    journal: Arc<Journal>,
    herdr: Arc<FakeHerdr>,
    clock: Arc<ManualClock>,
    notes: Arc<RecordingNotifier>,
    rec: Arc<Reconciler>,
}

fn fx() -> Fx {
    fx_with(|cfg| {
        cfg.idle_timeout = Duration::from_millis(100);
        cfg.exit_timeout = Duration::from_millis(100);
        cfg.exit_followup = Duration::from_millis(20);
    })
}

fn fx_with(tune: impl FnOnce(&mut ReconcilerConfig)) -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("inst");
    init_instance(&root).unwrap();
    let mut kinds = KindRegistry::default();
    register_core_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    let plans = Arc::new(PlanStore::new(root.join(".graph-local/plans")));
    let mut reg = crate::writer::MutationRegistry::default();
    kinds.register_mutations(&mut reg, plans.clone());
    register_mutations(&mut reg);
    reg.register("bookkeeping.test_set_model", Arc::new(TestSetModel));
    reg.register("bookkeeping.test_set_occupant", Arc::new(TestSetOccupant));
    reg.register("bookkeeping.test_end_occupant", Arc::new(TestEndOccupant));
    reg.register("bookkeeping.test_set_thread", Arc::new(TestSetThread));
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let w = WriterCore::new(store.clone(), journal.clone(), Arc::new(reg), clock.clone(), WriterConfig::default());
    let deps = PlanDeps { kinds, plans, store: store.clone(), writer: w.clone(), clock: clock.clone(), instance: root.clone() };
    let herdr = FakeHerdr::new();
    let notes = Arc::new(RecordingNotifier::default());
    let mut cfg = ReconcilerConfig::new(root);
    tune(&mut cfg);
    let rec = Reconciler::new(store.clone(), journal.clone(), w.clone(), herdr.clone(), clock.clone(), notes.clone(), cfg);
    Fx { _tmp: tmp, deps, w, store, journal, herdr, clock, notes, rec }
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn plan(fx: &Fx, change: &str) -> StoredPlan {
    let v = create_plan(&fx.deps, &CallerInfo::default(), words(change)).unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: crate::model::PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    fx.deps.plans.get(&id).unwrap().unwrap()
}

/// Plan and apply one change; it must commit.
fn commit(fx: &Fx, change: &str) {
    let sp = plan(fx, change);
    let op = admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay")
        .unwrap_or_else(|e| panic!("admit {change}: {}", e.message));
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(row.state, OpState::Committed, "{change}: {:?}", row.rejection);
}

fn view(fx: &Fx) -> crate::store::tree::CommitView<'_> {
    crate::store::tree::CommitView { store: &*fx.store, at: fx.store.head().unwrap() }
}

fn seat(fx: &Fx, name: &str) -> SeatRecord {
    let mut found: Vec<_> = layout::all_seats(&view(fx)).unwrap().into_iter().filter(|(_, s)| s.name == name).collect();
    assert_eq!(found.len(), 1, "seat {name}");
    found.remove(0).1
}

fn clones_of(fx: &Fx, seat: &SeatId) -> Vec<CloneRecord> {
    layout::all_clones(&view(fx)).unwrap().into_iter().map(|(_, c)| c).filter(|c| &c.seat == seat).collect()
}

fn only_clone(fx: &Fx, seat_name: &str) -> CloneRecord {
    let s = seat(fx, seat_name);
    let mut cs = clones_of(fx, &s.id);
    assert_eq!(cs.len(), 1);
    cs.remove(0)
}

/// One loop step: reconcile at the current head, then let the writer commit the write-backs.
async fn step(fx: &Fx) -> StepReport {
    let r = fx.rec.step_fresh().await;
    fx.w.drain().unwrap();
    r
}

/// A teamspace `alpha` and an active seat `foreman` of the given harness.
fn activate(fx: &Fx, harness: &str) {
    commit(fx, "teamspace create alpha");
    commit(fx, &format!("seat create foreman --teamspace alpha --active --harness {harness}"));
}

fn calls(fx: &Fx) -> Vec<FakeCall> {
    fx.herdr.calls()
}

fn count_calls(fx: &Fx, pred: impl Fn(&FakeCall) -> bool) -> usize {
    calls(fx).iter().filter(|c| pred(c)).count()
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

fn pane_of(fx: &Fx, c: &CloneRecord) -> HerdrPaneId {
    let fresh = clones_of(fx, &c.seat).into_iter().find(|x| x.id == c.id).unwrap();
    fresh.runtime.bound.expect("clone is bound").pane_id.expect("bound to a pane")
}

fn agent(status: AgentStatus) -> Option<AgentInfo> {
    Some(AgentInfo { kind: "claude".into(), status, session: None })
}

fn admit_bookkeeping(fx: &Fx, args: serde_json::Value) {
    use crate::ports::writer::Writer;
    fx.w
        .admit(crate::model::change::ChangeRequest {
            kind: crate::model::change::RequestKind::Bookkeeping,
            args,
            relied_on: vec![],
            requester: Default::default(),
            supersedes: None,
            confirmed: None,
        })
        .unwrap();
    fx.w.drain().unwrap();
}

fn set_occupant(fx: &Fx, c: &CloneRecord, native: &str) {
    admit_bookkeeping(fx, json!({"sub": "test_set_occupant", "clone": c.id, "native": native, "harness": "claude"}));
}

fn set_model(fx: &Fx, s: &SeatRecord, model: &str) {
    admit_bookkeeping(fx, json!({"sub": "test_set_model", "seat": s.id, "model": model}));
}

// -------------------------------------------------------------------------------------------------

#[tokio::test]
async fn activate_seat_creates_tab_pane_agent_in_order() {
    let fx = fx();
    activate(&fx, "claude");
    let report = step(&fx).await;
    assert!(report.executed.iter().all(|(_, s)| *s == EffectStatus::Done), "{report:?}");

    let c = only_clone(&fx, "foreman");
    let s = seat(&fx, "foreman");
    let log = calls(&fx);
    let shape: Vec<&str> = log
        .iter()
        .map(|c| match c {
            FakeCall::CreateWorkspace(_) => "create_workspace",
            FakeCall::ReportWorkspaceMetadata(..) => "stamp_workspace",
            FakeCall::RenameWorkspace(..) => "rename_workspace",
            FakeCall::CreateTab(_) => "create_tab",
            FakeCall::ReportPaneMetadata(..) => "stamp_pane",
            FakeCall::RenameTab(..) => "rename_tab",
            FakeCall::StartAgent(_) => "start_agent",
            other => panic!("unexpected call {other:?}"),
        })
        .collect();
    assert_eq!(
        shape,
        ["create_workspace", "stamp_workspace", "rename_workspace", "create_tab", "stamp_pane", "rename_tab", "start_agent"]
    );
    let FakeCall::CreateWorkspace(ws) = &log[0] else { unreachable!() };
    assert!(ws.label.starts_with("alpha ·"), "nonce label: {}", ws.label);
    let FakeCall::CreateTab(tab) = &log[3] else { unreachable!() };
    assert!(tab.label.starts_with("foreman ·"), "nonce label: {}", tab.label);
    let env: std::collections::BTreeMap<_, _> = tab.env.iter().cloned().collect();
    assert_eq!(env["HERDR_GRAPH"], "1");
    assert_eq!(env["HERDR_GRAPH_SEAT"], s.id.to_string());
    assert_eq!(env["HERDR_GRAPH_CLONE"], c.id.to_string());
    assert!(env.contains_key("HERDR_GRAPH_INSTANCE"));
    let FakeCall::ReportPaneMetadata(_, key, value) = &log[4] else { unreachable!() };
    assert_eq!((key.as_str(), value.as_str()), ("hg", format!("hg={}", c.id).as_str()));
    let FakeCall::RenameTab(_, plain) = &log[5] else { unreachable!() };
    assert_eq!(plain, "foreman");
    let FakeCall::StartAgent(start) = &log[6] else { unreachable!() };
    assert_eq!(start.kind, "claude");
    assert!(start.args.is_empty());

    // Binding written back as a bookkeeping mutation.
    let bound = clones_of(&fx, &s.id).remove(0).runtime;
    assert_eq!(bound.availability, Availability::Present);
    let b = bound.bound.expect("clone bound");
    assert_eq!(b.token.as_deref(), Some(format!("hg={}", c.id).as_str()));
    assert!(b.pane_id.is_some() && b.tab_id.is_some() && b.workspace_id.is_some());
    assert!(seat(&fx, "foreman").runtime.bound.is_some());

    // Level-triggered: a second step on the settled state does nothing.
    let before = calls(&fx).len();
    let again = step(&fx).await;
    assert_eq!(calls(&fx).len(), before, "no further herdr calls: {again:?}");
    assert!(again.planned.is_empty() && again.executed.is_empty(), "{again:?}");
}

#[tokio::test]
async fn shell_harness_starts_no_agent() {
    let fx = fx();
    activate(&fx, "shell");
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::CreateTab(_))), 1);
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 0);
    assert!(rows(&fx, EffectKind::StartAgent).is_empty());
    let c = only_clone(&fx, "foreman");
    assert!(fx.journal.meta_get(&planner::launched_key(&c.id)).unwrap().is_none());
}

#[tokio::test]
async fn retire_after_queued_create_is_obsolete() {
    let fx = fx();
    activate(&fx, "claude");
    fx.herdr.fail_next("tab.create", Fault::Unavailable);
    let first = step(&fx).await;
    let queued = rows(&fx, EffectKind::CreateTab);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].status, EffectStatus::Pending);
    assert!(first.executed.contains(&(queued[0].id.clone(), EffectStatus::Pending)), "{first:?}");

    commit(&fx, "seat retire foreman");
    let second = step(&fx).await;
    assert!(second.obsolete.contains(&queued[0].id), "{second:?}");
    assert_eq!(fx.journal.get_effect(&queued[0].id).unwrap().unwrap().status, EffectStatus::Obsolete);
    // The failed attempt is the only create_tab call, and no tab exists for the seat.
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::CreateTab(_))), 1);
    let snap = fx.herdr.snapshot().await.unwrap();
    assert!(snap.workspaces.iter().flat_map(|w| &w.tabs).all(|t| !t.label.starts_with("foreman")));
    // Its dependents were dropped with it.
    assert!(rows(&fx, EffectKind::StartAgent).iter().all(|r| r.status == EffectStatus::Obsolete));
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 0);
}

#[tokio::test]
async fn transient_failure_retried_with_backoff() {
    let fx = fx();
    activate(&fx, "claude");
    fx.herdr.fail_next("tab.create", Fault::Unavailable);
    step(&fx).await;
    let ef = rows(&fx, EffectKind::CreateTab).remove(0);
    assert_eq!(ef.status, EffectStatus::Pending);
    assert_eq!(ef.attempts, 1);
    assert!(ef.last_error.is_some());

    // Not before the backoff has elapsed.
    let early = step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::CreateTab(_))), 1);
    assert!(early.deferred.contains(&ef.id), "{early:?}");

    fx.clock.advance(chrono::Duration::seconds(5));
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::CreateTab(_))), 2);
    assert_eq!(fx.journal.get_effect(&ef.id).unwrap().unwrap().status, EffectStatus::Done);
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 1);
}

#[tokio::test]
async fn lost_response_create_adopted_by_nonce() {
    let fx = fx();
    activate(&fx, "claude");
    fx.herdr.fail_next("tab.create", Fault::LostResponse);
    step(&fx).await;
    let ef = rows(&fx, EffectKind::CreateTab).remove(0);
    assert_eq!(ef.status, EffectStatus::Unknown);

    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::CreateTab(_))), 1, "adopted, not duplicated");
    assert_eq!(fx.journal.get_effect(&ef.id).unwrap().unwrap().status, EffectStatus::Done);
    // Adoption stamped the token, dropped the nonce and bound the clone.
    let snap = fx.herdr.snapshot().await.unwrap();
    let tabs: Vec<_> = snap.workspaces.iter().flat_map(|w| &w.tabs).filter(|t| t.label.starts_with("foreman")).collect();
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].label, "foreman");
    assert!(tabs[0].panes[0].metadata.contains_key("hg"));
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 1);
    assert!(only_clone(&fx, "foreman").runtime.bound.is_some());
}

#[tokio::test]
async fn unknown_start_outcome_is_inspected_not_duplicated() {
    let fx = fx();
    activate(&fx, "claude");
    fx.herdr.fail_next("agent.start", Fault::LostResponse);
    step(&fx).await;
    let ef = rows(&fx, EffectKind::StartAgent).remove(0);
    assert_eq!(ef.status, EffectStatus::Unknown);
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 1);
    assert_eq!(fx.journal.get_effect(&ef.id).unwrap().unwrap().status, EffectStatus::Done);
    assert!(fx.journal.meta_get(&planner::launched_key(&only_clone(&fx, "foreman").id)).unwrap().is_some());
}

#[tokio::test]
async fn unknown_object_gets_no_create() {
    let fx = fx();
    activate(&fx, "claude");
    commit(&fx, "seat create other --teamspace alpha --active --harness shell");
    let c = only_clone(&fx, "foreman");
    let b = Binding { pane_id: Some(HerdrPaneId("p9".into())), ..Default::default() };
    admit_binding(&*fx.w, &c.id.to_any(), &b, Availability::Unknown).unwrap();
    fx.w.drain().unwrap();
    step(&fx).await;
    // Nothing is created for the unknown clone ...
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::SplitPane(_) | FakeCall::StartAgent(_))), 0);
    let tabs: Vec<_> = calls(&fx).into_iter().filter_map(|c| if let FakeCall::CreateTab(t) = c { Some(t.label) } else { None }).collect();
    assert_eq!(tabs.len(), 1, "{tabs:?}");
    assert!(tabs[0].starts_with("other ·"), "{tabs:?}");
    // ... while never-bound objects still are.
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::CreateWorkspace(_))), 1);
}

#[tokio::test]
async fn unknown_only_clone_creates_nothing_at_all() {
    let fx = fx();
    activate(&fx, "claude");
    let c = only_clone(&fx, "foreman");
    let b = Binding { pane_id: Some(HerdrPaneId("p9".into())), ..Default::default() };
    admit_binding(&*fx.w, &c.id.to_any(), &b, Availability::Unknown).unwrap();
    fx.w.drain().unwrap();
    let report = step(&fx).await;
    assert!(calls(&fx).is_empty(), "{:?}", calls(&fx));
    assert!(report.planned.is_empty(), "{report:?}");
}

#[tokio::test]
async fn moved_out_seat_gets_no_create_tab() {
    let fx = fx();
    activate(&fx, "claude");
    commit(&fx, "seat create other --teamspace alpha --active --harness shell");
    let s = seat(&fx, "foreman");
    admit_runtime(&*fx.w, &s.id.to_any(), Availability::Absent, Some(true)).unwrap();
    fx.w.drain().unwrap();
    assert!(seat(&fx, "foreman").moved_out);
    step(&fx).await;
    let tabs: Vec<_> = calls(&fx).into_iter().filter_map(|c| if let FakeCall::CreateTab(t) = c { Some(t.label) } else { None }).collect();
    assert_eq!(tabs.len(), 1, "{tabs:?}");
    assert!(tabs[0].starts_with("other ·"), "{tabs:?}");
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 0);
}

#[tokio::test]
async fn start_agent_needs_shell_foreground() {
    let fx = fx();
    activate(&fx, "claude");
    fx.herdr.fail_next("agent.start", Fault::Unavailable);
    step(&fx).await;
    let pane = pane_of(&fx, &only_clone(&fx, "foreman"));
    fx.herdr.set_process(&pane, ProcessInfo { foreground_pid: Some(7), foreground_argv: vec!["vim".into()], is_shell: false });
    fx.clock.advance(chrono::Duration::seconds(5));
    step(&fx).await;
    let ef = rows(&fx, EffectKind::StartAgent).remove(0);
    assert_eq!(ef.status, EffectStatus::NeedsRevision);
    assert_eq!(ef.last_error.as_deref(), Some("pane foreground is not the shell"));
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 1, "the refused start never reached herdr");
    let msgs = fx.notes.messages();
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].contains("not the shell"), "{msgs:?}");
}

#[tokio::test]
async fn blocked_needs_human_outcome() {
    let fx = fx();
    activate(&fx, "claude");
    fx.herdr.fail_next("agent.start", Fault::StartOutcome(StartOutcome::BlockedNeedsHuman));
    step(&fx).await;
    let ef = rows(&fx, EffectKind::StartAgent).remove(0);
    assert_eq!(ef.status, EffectStatus::BlockedNeedsHuman);
    assert_eq!(fx.notes.messages().len(), 1);
    // Not retried: the blocked agent is the occupant now.
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 1);
}

/// Launch a claude seat, then give its clone an occupant with native session `abc`.
async fn occupied(fx: &Fx) -> (SeatRecord, CloneRecord, HerdrPaneId) {
    activate(fx, "claude");
    step(fx).await;
    let c = only_clone(fx, "foreman");
    set_occupant(fx, &c, "abc");
    (seat(fx, "foreman"), only_clone(fx, "foreman"), pane_of(fx, &c))
}

#[tokio::test]
async fn model_change_on_occupied_clone_replaces_with_resume() {
    let fx = fx();
    let (s, c, pane) = occupied(&fx).await;
    set_model(&fx, &s, "opus-x");
    fx.herdr.clear_calls();
    let report = step(&fx).await;
    assert!(report.executed.iter().any(|(_, st)| *st == EffectStatus::Done), "{report:?}");

    let log = calls(&fx);
    let ctrl_c = FakeCall::SendKeys(pane.clone(), vec![KeyInput::Key("ctrl+c".into())]);
    assert_eq!(log.iter().filter(|c| **c == ctrl_c).count(), 2, "{log:?}");
    let starts: Vec<_> = log.iter().filter_map(|c| if let FakeCall::StartAgent(s) = c { Some(s) } else { None }).collect();
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0].pane, pane);
    assert_eq!(starts[0].args, ["--resume", "abc", "--model", "opus-x"]);
    // Exit keys come before the restart.
    let first_start = log.iter().position(|c| matches!(c, FakeCall::StartAgent(_))).unwrap();
    assert!(log[..first_start].iter().filter(|c| **c == ctrl_c).count() == 2);

    let launched: serde_json::Value =
        serde_json::from_str(&fx.journal.meta_get(&planner::launched_key(&c.id)).unwrap().unwrap()).unwrap();
    assert_eq!(launched["model"], "opus-x");

    // Level-triggered and idempotent: nothing more to do.
    fx.herdr.clear_calls();
    step(&fx).await;
    assert!(calls(&fx).is_empty(), "{:?}", calls(&fx));
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).len(), 1);
}

#[tokio::test]
async fn adopted_occupant_without_launch_record_not_replaced() {
    let fx = fx();
    let (s, c, _) = occupied(&fx).await;
    fx.journal.meta_delete(&planner::launched_key(&c.id)).unwrap();
    set_model(&fx, &s, "opus-x");
    fx.herdr.clear_calls();
    step(&fx).await;
    assert!(calls(&fx).is_empty(), "{:?}", calls(&fx));
    assert!(rows(&fx, EffectKind::ReplaceSession).is_empty());
}

fn ms(n: i64) -> chrono::Duration {
    chrono::Duration::milliseconds(n)
}

#[tokio::test]
async fn replacement_busy_occupant_needs_revision() {
    let fx = fx();
    let (s, _, pane) = occupied(&fx).await;
    fx.herdr.set_agent(&pane, agent(AgentStatus::Working));
    set_model(&fx, &s, "opus-x");
    fx.herdr.clear_calls();
    let report = step(&fx).await;
    let ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert_eq!(ef.status, EffectStatus::Pending, "the wait is not over yet");
    assert_eq!(report.deferred, vec![ef.id.clone()]);
    assert!(calls(&fx).is_empty(), "a working agent is never interrupted: {:?}", calls(&fx));
    assert!(fx.notes.messages().is_empty());

    fx.clock.advance(ms(100));
    step(&fx).await;
    let ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert_eq!(ef.status, EffectStatus::NeedsRevision);
    assert_eq!(ef.last_error.as_deref(), Some("occupant busy"));
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::SendKeys(..) | FakeCall::StartAgent(_))), 0, "{:?}", calls(&fx));
    assert!(fx.notes.messages().iter().any(|m| m.contains("occupant busy")));
}

#[tokio::test]
async fn replacement_exit_timeout_starts_no_agent() {
    let fx = fx();
    let (s, _, pane) = occupied(&fx).await;
    fx.herdr.set_process(&pane, ProcessInfo { foreground_pid: Some(7), foreground_argv: vec!["claude".into()], is_shell: false });
    set_model(&fx, &s, "opus-x");
    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).remove(0).status, EffectStatus::Pending);
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::SendKeys(..))), 2, "the two ctrl+c presses: {:?}", calls(&fx));

    fx.clock.advance(ms(20));
    step(&fx).await;
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).remove(0).status, EffectStatus::Pending);
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::SendKeys(..))), 3, "the /exit fallback: {:?}", calls(&fx));
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::SendKeys(..))), 3, "the fallback is sent once");

    fx.clock.advance(ms(80));
    step(&fx).await;
    let ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert_eq!(ef.status, EffectStatus::NeedsRevision);
    assert_eq!(ef.last_error.as_deref(), Some("occupant did not exit"));
    let log = calls(&fx);
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 0, "{log:?}");
    let sent: Vec<_> = log.iter().filter_map(|c| if let FakeCall::SendKeys(_, k) = c { Some(k.clone()) } else { None }).collect();
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert_eq!(sent[2], vec![KeyInput::Text("/exit".into()), KeyInput::Key("enter".into())]);
    assert!(fx.notes.messages().iter().any(|m| m.contains("occupant did not exit")));
}

#[tokio::test]
async fn replacement_wait_does_not_block_other_effects() {
    // Default timeouts: a working occupant would hold the old inline wait for 10 minutes.
    let fx = fx_with(|_| {});
    let (s, _, pane) = occupied(&fx).await;
    commit(&fx, "seat create bench --teamspace alpha --active --harness shell");
    step(&fx).await;
    fx.herdr.set_agent(&pane, agent(AgentStatus::Working));
    set_model(&fx, &s, "opus-x");
    commit(&fx, "seat rename bench zwei");
    fx.herdr.clear_calls();

    let report = tokio::time::timeout(Duration::from_secs(2), step(&fx)).await.expect("the step must not wait for the occupant");
    assert!(count_calls(&fx, |c| matches!(c, FakeCall::RenameTab(..))) >= 1, "{:?}", calls(&fx));
    let rename = rows(&fx, EffectKind::RenameTab).into_iter().last().expect("rename row");
    assert_eq!(rename.status, EffectStatus::Done);
    let repl = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert_eq!(repl.status, EffectStatus::Pending);
    assert!(report.deferred.contains(&repl.id), "{report:?}");
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::SendKeys(..) | FakeCall::StartAgent(_))), 0);
}

#[tokio::test]
async fn replacement_state_survives_restart() {
    let fx = fx();
    let (s, _, pane) = occupied(&fx).await;
    fx.herdr.set_agent(&pane, agent(AgentStatus::Working));
    set_model(&fx, &s, "opus-x");
    step(&fx).await;
    fx.clock.advance(ms(50));
    step(&fx).await;
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).remove(0).status, EffectStatus::Pending);

    // A restarted daemon: fresh reconciler over the same journal, store and clock.
    let mut cfg = ReconcilerConfig::new(fx.deps.instance.clone());
    cfg.idle_timeout = Duration::from_millis(100);
    let rec2 = Reconciler::new(fx.store.clone(), fx.journal.clone(), fx.w.clone(), fx.herdr.clone(), fx.clock.clone(), fx.notes.clone(), cfg);
    fx.clock.advance(ms(50));
    rec2.step_fresh().await;
    fx.w.drain().unwrap();
    let ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert_eq!(ef.status, EffectStatus::NeedsRevision, "the timer counts from the persisted start, not the restart");
    assert_eq!(ef.last_error.as_deref(), Some("occupant busy"));
}

fn non_shell() -> ProcessInfo {
    ProcessInfo { foreground_pid: Some(7), foreground_argv: vec!["claude".into()], is_shell: false }
}

fn shell() -> ProcessInfo {
    ProcessInfo { foreground_pid: None, foreground_argv: vec!["zsh".into()], is_shell: true }
}

fn send_keys_count(fx: &Fx) -> usize {
    count_calls(fx, |c| matches!(c, FakeCall::SendKeys(..)))
}

fn start_calls(fx: &Fx) -> Vec<crate::ports::herdr::StartAgent> {
    calls(fx).into_iter().filter_map(|c| if let FakeCall::StartAgent(s) = c { Some(s) } else { None }).collect()
}

#[tokio::test]
async fn replacement_late_step_sends_fallback_before_timing_out() {
    let fx = fx();
    let (s, _, pane) = occupied(&fx).await;
    fx.herdr.set_process(&pane, non_shell());
    set_model(&fx, &s, "opus-x");
    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(send_keys_count(&fx), 2);
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).remove(0).status, EffectStatus::Pending);

    // One late step, past both the follow-up and the timeout: the fallback still goes out first.
    fx.clock.advance(ms(150));
    step(&fx).await;
    assert_eq!(send_keys_count(&fx), 3);
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).remove(0).status, EffectStatus::Pending);
    assert!(fx.notes.messages().is_empty());

    fx.clock.advance(ms(20));
    step(&fx).await;
    let ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert_eq!(ef.status, EffectStatus::NeedsRevision);
    assert_eq!(ef.last_error.as_deref(), Some("occupant did not exit"));
    assert!(start_calls(&fx).is_empty());
}

#[tokio::test]
async fn replacement_resumes_after_occupant_cleared() {
    let fx = fx();
    let (s, c, pane) = occupied(&fx).await;
    fx.herdr.set_process(&pane, non_shell());
    set_model(&fx, &s, "opus-x");
    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(send_keys_count(&fx), 2);
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).remove(0).status, EffectStatus::Pending);

    // The agent exits and the observer records the occupancy end; only then does the shell show.
    fx.herdr.set_agent(&pane, None);
    admit_bookkeeping(&fx, json!({"sub": "test_end_occupant", "clone": c.id, "native": "abc"}));
    fx.herdr.set_process(&pane, shell());
    step(&fx).await;

    let ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert_eq!(ef.status, EffectStatus::Done, "{ef:?}");
    let starts = start_calls(&fx);
    assert_eq!(starts.len(), 1, "{starts:?}");
    assert_eq!(starts[0].args, ["--resume", "abc", "--model", "opus-x"]);
    let launched: serde_json::Value =
        serde_json::from_str(&fx.journal.meta_get(&planner::launched_key(&c.id)).unwrap().unwrap()).unwrap();
    assert_eq!(launched["model"], "opus-x");
    assert_eq!(fx.journal.meta_get(&format!("replace:{}", ef.id)).unwrap(), None);
}

#[tokio::test]
async fn replacement_unknown_start_is_not_exited_again() {
    let fx = fx();
    let (s, c, pane) = occupied(&fx).await;
    fx.herdr.set_process(&pane, non_shell());
    set_model(&fx, &s, "opus-x");
    step(&fx).await;
    fx.herdr.set_agent(&pane, None);
    admit_bookkeeping(&fx, json!({"sub": "test_end_occupant", "clone": c.id, "native": "abc"}));
    fx.herdr.set_process(&pane, shell());
    // The start goes through but its response is lost: the new agent is on the pane.
    fx.herdr.fail_next("agent.start", Fault::LostResponse);
    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).remove(0).status, EffectStatus::Unknown);
    assert_eq!(start_calls(&fx).len(), 1);

    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).remove(0).status, EffectStatus::Done);
    assert_eq!(send_keys_count(&fx), 0, "the new agent must not be sent the exit keys: {:?}", calls(&fx));
    assert!(start_calls(&fx).is_empty());
    let launched: serde_json::Value =
        serde_json::from_str(&fx.journal.meta_get(&planner::launched_key(&c.id)).unwrap().unwrap()).unwrap();
    assert_eq!(launched["model"], "opus-x");
}

#[tokio::test]
async fn terminal_effect_clears_its_meta() {
    let fx = fx();
    let (s, _, pane) = occupied(&fx).await;
    fx.herdr.set_agent(&pane, agent(AgentStatus::Working));
    set_model(&fx, &s, "opus-x");
    step(&fx).await;
    let ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert!(fx.journal.meta_get(&format!("replace:{}", ef.id)).unwrap().is_some(), "waiting state is kept");
    assert!(ef.sched.wake_at.is_some());
    let mut with_retry = ef.clone();
    with_retry.sched.retry_at = Some(fx.clock.now());
    fx.journal.upsert_effect(&with_retry).unwrap();

    fx.clock.advance(ms(100));
    step(&fx).await;
    let ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert_eq!(ef.status, EffectStatus::NeedsRevision);
    assert_eq!(fx.journal.meta_get(&format!("replace:{}", ef.id)).unwrap(), None, "replace state leaked");
    assert_eq!(ef.sched, Default::default(), "scheduling state leaked onto the ended row");
}

#[tokio::test]
async fn next_wake_reports_earliest_deferred_or_retry() {
    let fx = fx_with(|cfg| {
        cfg.idle_timeout = Duration::from_secs(600);
        cfg.deferred_recheck = Duration::from_secs(5);
    });
    assert_eq!(fx.rec.next_wake(), None);
    let (s, _, pane) = occupied(&fx).await;
    fx.herdr.set_agent(&pane, agent(AgentStatus::Working));
    set_model(&fx, &s, "opus-x");
    step(&fx).await;
    assert_eq!(fx.rec.next_wake(), Some(t0() + ms(5000)));

    // A transient failure of another effect backs off for less than the recheck.
    commit(&fx, "seat rename foreman zwei");
    fx.herdr.fail_next("tab.rename", Fault::Unavailable);
    step(&fx).await;
    let rename = rows(&fx, EffectKind::RenameTab).into_iter().find(|r| r.status == EffectStatus::Pending).expect("pending rename row");
    let retry = rename.sched.retry_at.expect("retry_at");
    assert!(retry < t0() + ms(5000), "backoff {retry} must be shorter than the recheck for this test");
    assert_eq!(fx.rec.next_wake(), Some(retry));
}

#[tokio::test]
async fn close_effects_predict_induced_closures() {
    let fx = fx();
    activate(&fx, "claude");
    step(&fx).await;
    // Herdr's own default tab goes, so the seat tab is the workspace's last.
    let snap = fx.herdr.snapshot().await.unwrap();
    let default_tab = snap.workspaces[0].tabs.iter().find(|t| t.label == "1").unwrap().id.clone();
    fx.herdr.user_close_tab(&default_tab);

    let c = only_clone(&fx, "foreman");
    let s = seat(&fx, "foreman");
    commit(&fx, &format!("clone retire {}", c.id));
    fx.herdr.clear_calls();
    step(&fx).await;

    let ef = rows(&fx, EffectKind::ClosePane).remove(0);
    assert_eq!(ef.status, EffectStatus::Done);
    let ts = s.teamspace.to_any();
    let p = |object: AnyId, container, induced| PredictedEnd { object, container, end: EndState::Closed, induced };
    assert_eq!(ef.predicted.len(), 3, "{:?}", ef.predicted);
    assert!(ef.predicted.contains(&p(c.id.to_any(), ContainerKind::Pane, false)));
    assert!(ef.predicted.contains(&p(s.id.to_any(), ContainerKind::Tab, true)));
    assert!(ef.predicted.contains(&p(ts, ContainerKind::Workspace, true)));
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::ClosePane(_))), 1);
    // The reconciler never creates anything for the retired clone again.
    fx.herdr.clear_calls();
    step(&fx).await;
    assert!(calls(&fx).is_empty(), "{:?}", calls(&fx));
}

#[tokio::test]
async fn deactivated_seat_closes_its_tab() {
    let fx = fx();
    activate(&fx, "claude");
    step(&fx).await;
    commit(&fx, "seat deactivate foreman");
    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::CloseTab(_))), 1);
    let ef = rows(&fx, EffectKind::CloseTab).remove(0);
    assert_eq!(ef.status, EffectStatus::Done);
    assert!(ef.predicted.iter().any(|p| p.container == ContainerKind::Tab && !p.induced));
    assert!(ef.predicted.iter().any(|p| p.container == ContainerKind::Pane && p.induced));
}

#[tokio::test]
async fn reactivated_seat_starts_agent_again() {
    let fx = fx();
    activate(&fx, "claude");
    step(&fx).await;
    commit(&fx, "seat deactivate foreman");
    step(&fx).await;
    commit(&fx, "seat activate foreman");
    fx.herdr.clear_calls();
    step(&fx).await;
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::CreateTab(_))), 1);
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 1, "{:?}", calls(&fx));
}

#[tokio::test]
async fn relaunch_hook_enqueues_start_for_unoccupied_active_clone() {
    let fx = fx();
    activate(&fx, "claude");
    step(&fx).await;
    let c = only_clone(&fx, "foreman");
    let pane = pane_of(&fx, &c);
    fx.herdr.set_agent(&pane, None);
    let authority = OpId::new();
    let ef = fx.rec.request_relaunch(&c.id, &authority).unwrap();
    let row = fx.journal.get_effect(&ef).unwrap().unwrap();
    assert_eq!((row.kind.clone(), row.op.clone(), row.status), (EffectKind::RelaunchOccupant, authority, EffectStatus::Pending));
    // Enqueueing twice is the same effect.
    assert_eq!(fx.rec.request_relaunch(&c.id, &row.op).unwrap(), ef);

    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(s) if s.pane == pane && s.kind == "claude")), 1);
    assert_eq!(fx.journal.get_effect(&ef).unwrap().unwrap().status, EffectStatus::Done);
}

#[tokio::test]
async fn relaunch_refused_for_dormant_seat() {
    let fx = fx();
    activate(&fx, "claude");
    step(&fx).await;
    let c = only_clone(&fx, "foreman");
    commit(&fx, "seat deactivate foreman");
    let err = fx.rec.request_relaunch(&c.id, &OpId::new()).unwrap_err();
    assert!(err.to_string().contains("not an active clone of an active seat"), "{err}");
    assert!(fx.rec.request_relaunch(&CloneId::new(), &OpId::new()).is_err());
}

#[tokio::test]
async fn relaunch_waits_grace_after_incarnation_change() {
    let fx = fx();
    activate(&fx, "claude");
    step(&fx).await;
    let c = only_clone(&fx, "foreman");
    // Herdr restarts: new incarnation, pane ids change, tokens survive.
    fx.herdr.restart(true, true);
    let snap = fx.herdr.snapshot().await.unwrap();
    let new_pane = snap
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .flat_map(|t| &t.panes)
        .find(|p| p.metadata.get("hg").is_some_and(|v| v == &format!("hg={}", c.id)))
        .unwrap()
        .id
        .clone();
    fx.herdr.set_agent(&new_pane, None);
    let ef = fx.rec.request_relaunch(&c.id, &OpId::new()).unwrap();
    fx.herdr.clear_calls();

    let early = step(&fx).await;
    assert!(early.deferred.contains(&ef), "{early:?}");
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 0);
    assert_eq!(fx.journal.get_effect(&ef).unwrap().unwrap().attempts, 0, "waiting is not an attempt");
    let changed = fx.journal.meta_get("incarnation:changed_at").unwrap().expect("incarnation:changed_at");
    let at = chrono::DateTime::parse_from_rfc3339(&changed).unwrap().to_utc();
    assert_eq!(fx.rec.next_wake(), Some(at + chrono::Duration::seconds(90)), "wakes at the end of the grace");

    fx.clock.advance(chrono::Duration::seconds(60));
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(_))), 0, "still inside the 90 s grace");

    fx.clock.advance(chrono::Duration::seconds(31));
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::StartAgent(s) if s.pane == new_pane)), 1);
    assert_eq!(fx.journal.get_effect(&ef).unwrap().unwrap().status, EffectStatus::Done);
}

#[tokio::test]
async fn effect_identity_is_deterministic() {
    let fx = fx();
    activate(&fx, "claude");
    // A second reconciler with its own journal and herdr, over the very same committed state.
    let tmp = tempfile::tempdir().unwrap();
    let other_journal = Arc::new(Journal::open(&tmp.path().join("other.sqlite3")).unwrap());
    let other_herdr = FakeHerdr::new();
    let other = Reconciler::new(
        fx.store.clone(),
        other_journal,
        fx.w.clone(),
        other_herdr,
        fx.clock.clone(),
        Arc::new(RecordingNotifier::default()),
        ReconcilerConfig::new(fx.deps.instance.clone()),
    );
    let a = fx.rec.step_fresh().await;
    let b = other.step_fresh().await;
    let mut ids_a = a.planned.clone();
    let mut ids_b = b.planned.clone();
    ids_a.sort();
    ids_b.sort();
    assert!(!ids_a.is_empty());
    assert_eq!(ids_a, ids_b);
}

#[tokio::test]
async fn bookkeeping_binding_mutation_updates_runtime() {
    let fx = fx();
    activate(&fx, "claude");
    let c = only_clone(&fx, "foreman");
    let binding = Binding {
        token: Some(format!("hg={}", c.id)),
        workspace_id: Some(crate::model::HerdrWorkspaceId("w7".into())),
        tab_id: Some(crate::model::HerdrTabId("t7".into())),
        pane_id: Some(HerdrPaneId("p7".into())),
        terminal_id: None,
        incarnation: Default::default(),
    };
    fx.clock.advance(chrono::Duration::seconds(3));
    admit_binding(&*fx.w, &c.id.to_any(), &binding, Availability::Present).unwrap();
    fx.w.drain().unwrap();
    let after = only_clone(&fx, "foreman");
    assert_eq!(after.runtime.bound.as_ref(), Some(&binding));
    assert_eq!(after.runtime.availability, Availability::Present);
    assert_eq!(after.runtime.observed_at, Some(t0() + chrono::Duration::seconds(3)));
    assert_eq!(after.rev, c.rev + 1);

    let s = seat(&fx, "foreman");
    admit_runtime(&*fx.w, &s.id.to_any(), Availability::Absent, Some(true)).unwrap();
    fx.w.drain().unwrap();
    let s2 = seat(&fx, "foreman");
    assert_eq!((s2.runtime.availability, s2.moved_out), (Availability::Absent, true));
    assert_eq!(s2.lifecycle, Lifecycle::Active, "bookkeeping never changes lifecycle");

    // Unknown objects are rejected by the writer, not invented.
    let ghost = CloneId::new();
    admit_runtime(&*fx.w, &ghost.to_any(), Availability::Absent, None).unwrap();
    fx.w.drain().unwrap();
    let rejected = fx.journal.list(&[OpState::Rejected], 10).unwrap();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].rejection.as_ref().unwrap().reason, "object_missing");
}

#[tokio::test]
async fn predictions_listed_until_consumed() {
    let fx = fx();
    activate(&fx, "shell");
    step(&fx).await;
    let preds = predictions(&fx.journal);
    assert!(preds.iter().any(|(_, p)| p.container == ContainerKind::Workspace && p.end == EndState::Present));
    assert!(preds.iter().any(|(_, p)| matches!(&p.end, EndState::Renamed { name } if name == "foreman")));
    let (ef, _) = preds[0].clone();
    consume_prediction(&fx.journal, &ef);
    assert!(predictions(&fx.journal).iter().all(|(id, _)| id != &ef));
}

struct CustomSource(AnyId);
impl EffectSource for CustomSource {
    fn effects(&self, cx: &DiffCx<'_>) -> Vec<PlannedEffect> {
        let kind = EffectKind::Custom("demo.ping".into());
        let op = OpId::from_ulid(ulid::Ulid::nil());
        let id = EffectRecord::identity(&op, &self.0, &kind, 1);
        vec![PlannedEffect {
            record: EffectRecord {
                id,
                op,
                object: self.0.clone(),
                kind,
                object_rev: 1,
                fencing_rev: 1,
                status: EffectStatus::Pending,
                predicted: vec![],
                nonce_label: None,
                attempts: 0,
                last_error: None,
                updated_at: cx.now,
                sched: Default::default(),
            },
            deps: vec![],
        }]
    }
}

struct CustomExec(Mutex<u32>);
#[async_trait::async_trait]
impl EffectExecutor for CustomExec {
    fn handles(&self, kind: &EffectKind) -> bool {
        matches!(kind, EffectKind::Custom(k) if k == "demo.ping")
    }
    async fn execute(&self, _cx: &ExecCx<'_>, _e: &EffectRecord) -> ExecOutcome {
        *self.0.lock().unwrap() += 1;
        ExecOutcome::Done
    }
}

#[tokio::test]
async fn custom_family_registers_source_and_executor() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    let exec = Arc::new(CustomExec(Mutex::new(0)));
    fx.rec.register_source(Arc::new(CustomSource(SeatId::new().to_any())));
    fx.rec.register_executor(exec.clone());
    let report = step(&fx).await;
    assert_eq!(*exec.0.lock().unwrap(), 1);
    assert!(report.executed.iter().any(|(_, s)| *s == EffectStatus::Done));
    step(&fx).await;
    assert_eq!(*exec.0.lock().unwrap(), 1, "a done effect is not re-run");
}

#[tokio::test]
async fn replacement_exit_followup_wakes_at_three_seconds() {
    let fx = fx_with(|cfg| {
        cfg.idle_timeout = Duration::from_secs(600);
        cfg.exit_timeout = Duration::from_secs(30);
        cfg.exit_followup = Duration::from_secs(3);
        cfg.deferred_recheck = Duration::from_secs(2);
    });
    let (s, _, pane) = occupied(&fx).await;
    fx.herdr.set_process(&pane, non_shell());
    set_model(&fx, &s, "opus-x");
    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(send_keys_count(&fx), 2);
    let t1 = fx.clock.now();
    assert_eq!(fx.rec.next_wake(), Some(t1 + chrono::Duration::seconds(2)));

    fx.clock.set(t1 + chrono::Duration::seconds(2));
    step(&fx).await;
    assert_eq!(send_keys_count(&fx), 2);
    assert_eq!(fx.rec.next_wake(), Some(t1 + chrono::Duration::seconds(3)));

    fx.clock.set(t1 + chrono::Duration::seconds(3));
    step(&fx).await;
    assert_eq!(send_keys_count(&fx), 3, "the /exit fallback goes out at the follow-up deadline");
    assert_eq!(fx.rec.next_wake(), Some(t1 + chrono::Duration::seconds(5)));
    assert_eq!(rows(&fx, EffectKind::ReplaceSession).remove(0).status, EffectStatus::Pending);
}

struct WaitSource(AnyId);
impl EffectSource for WaitSource {
    fn effects(&self, cx: &DiffCx<'_>) -> Vec<PlannedEffect> {
        let kind = EffectKind::Custom("demo.wait".into());
        let op = OpId::from_ulid(ulid::Ulid::nil());
        let id = EffectRecord::identity(&op, &self.0, &kind, 1);
        vec![PlannedEffect {
            record: EffectRecord {
                id,
                op,
                object: self.0.clone(),
                kind,
                object_rev: 1,
                fencing_rev: 1,
                status: EffectStatus::Pending,
                predicted: vec![],
                nonce_label: None,
                attempts: 0,
                last_error: None,
                updated_at: cx.now,
                sched: Default::default(),
            },
            deps: vec![],
        }]
    }
}

struct WaitExec {
    calls: Mutex<u32>,
    ready: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl EffectExecutor for WaitExec {
    fn handles(&self, kind: &EffectKind) -> bool {
        matches!(kind, EffectKind::Custom(k) if k == "demo.wait")
    }
    async fn execute(&self, _cx: &ExecCx<'_>, _e: &EffectRecord) -> ExecOutcome {
        *self.calls.lock().unwrap() += 1;
        if self.ready.load(std::sync::atomic::Ordering::SeqCst) { ExecOutcome::Done } else { ExecOutcome::Deferred("not yet".into()) }
    }
}

#[tokio::test]
async fn open_ended_deferral_backs_off_to_the_cap() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    let exec = Arc::new(WaitExec { calls: Mutex::new(0), ready: std::sync::atomic::AtomicBool::new(false) });
    fx.rec.register_source(Arc::new(WaitSource(SeatId::new().to_any())));
    fx.rec.register_executor(exec.clone());
    let mut gaps = vec![];
    for _ in 0..8 {
        step(&fx).await;
        let gap = (fx.rec.next_wake().unwrap() - fx.clock.now()).num_seconds();
        gaps.push(gap);
        fx.clock.advance(chrono::Duration::seconds(gap));
    }
    assert_eq!(gaps, [2, 4, 8, 16, 32, 60, 60, 60]);

    exec.ready.store(true, std::sync::atomic::Ordering::SeqCst);
    step(&fx).await;
    let row = rows(&fx, EffectKind::Custom("demo.wait".into())).remove(0);
    assert_eq!(row.status, EffectStatus::Done);
    assert_eq!(row.attempts, 1, "deferrals are not attempts");
    assert_eq!(fx.rec.next_wake(), None);
    assert_eq!(row.sched, Default::default());
}

/// A custom effect that is `Done` on its first run: it makes `run_pending` take another pass.
struct OnceSource(AnyId);
impl EffectSource for OnceSource {
    fn effects(&self, cx: &DiffCx<'_>) -> Vec<PlannedEffect> {
        let kind = EffectKind::Custom("demo.once".into());
        let op = OpId::from_ulid(ulid::Ulid::nil());
        let id = EffectRecord::identity(&op, &self.0, &kind, 1);
        vec![PlannedEffect {
            record: EffectRecord {
                id,
                op,
                object: self.0.clone(),
                kind,
                object_rev: 1,
                fencing_rev: 1,
                status: EffectStatus::Pending,
                predicted: vec![],
                nonce_label: None,
                attempts: 0,
                last_error: None,
                updated_at: cx.now,
                sched: Default::default(),
            },
            deps: vec![],
        }]
    }
}

struct OnceExec;
#[async_trait::async_trait]
impl EffectExecutor for OnceExec {
    fn handles(&self, kind: &EffectKind) -> bool {
        matches!(kind, EffectKind::Custom(k) if k == "demo.once")
    }
    async fn execute(&self, _cx: &ExecCx<'_>, _e: &EffectRecord) -> ExecOutcome {
        ExecOutcome::Done
    }
}

#[tokio::test]
async fn deferred_row_defers_once_per_step() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    let exec = Arc::new(WaitExec { calls: Mutex::new(0), ready: std::sync::atomic::AtomicBool::new(false) });
    fx.rec.register_source(Arc::new(WaitSource(SeatId::new().to_any())));
    fx.rec.register_executor(exec.clone());
    fx.rec.register_source(Arc::new(OnceSource(SeatId::new().to_any())));
    fx.rec.register_executor(Arc::new(OnceExec));
    let report = step(&fx).await;
    assert!(
        report.executed.iter().any(|(id, st)| *st == EffectStatus::Done && rows(&fx, EffectKind::Custom("demo.once".into()))[0].id == *id),
        "the second row must finish so that run_pending makes another pass: {report:?}"
    );
    assert_eq!(*exec.calls.lock().unwrap(), 1, "a deferred row is not executed again in the same step");
    let row = rows(&fx, EffectKind::Custom("demo.wait".into())).remove(0);
    assert_eq!(row.sched.defer_n, 1);
    assert!(report.deferred.contains(&row.id), "{report:?}");
}

#[tokio::test]
async fn legacy_meta_scheduling_is_folded_into_the_row() {
    let fx = fx();
    let dep = EffectId::new();
    let object = SeatId::new().to_any();
    let kind = EffectKind::Custom("demo.legacy".into());
    let op = OpId::from_ulid(ulid::Ulid::nil());
    let row = EffectRecord {
        id: EffectRecord::identity(&op, &object, &kind, 1),
        op,
        object,
        kind,
        object_rev: 1,
        fencing_rev: 1,
        status: EffectStatus::Pending,
        predicted: vec![],
        nonce_label: None,
        attempts: 0,
        last_error: None,
        updated_at: t0(),
        sched: Default::default(),
    };
    fx.journal.upsert_effect(&row).unwrap();
    let retry = t0() + ms(7000);
    let wake = t0() + ms(9000);
    fx.journal.meta_set(&format!("deps:{}", row.id), &serde_json::to_string(&vec![dep.clone()]).unwrap()).unwrap();
    fx.journal.meta_set(&format!("retry_at:{}", row.id), &retry.to_rfc3339()).unwrap();
    fx.journal.meta_set(&format!("wake_at:{}", row.id), &wake.to_rfc3339()).unwrap();
    fx.journal.meta_set(&format!("defer_n:{}", row.id), "3").unwrap();

    let cfg = ReconcilerConfig::new(fx.deps.instance.clone());
    let _rec = Reconciler::new(fx.store.clone(), fx.journal.clone(), fx.w.clone(), fx.herdr.clone(), fx.clock.clone(), fx.notes.clone(), cfg);

    let got = fx.journal.get_effect(&row.id).unwrap().unwrap();
    assert_eq!(got.sched.deps, vec![dep]);
    assert_eq!(got.sched.retry_at, Some(retry));
    assert_eq!(got.sched.wake_at, Some(wake));
    assert_eq!(got.sched.defer_n, 3);
    for key in ["deps", "retry_at", "wake_at", "defer_n"] {
        assert_eq!(fx.journal.meta_get(&format!("{key}:{}", row.id)).unwrap(), None, "{key} meta left behind");
    }
}

/// The agent a start left on `c`'s pane, if any.
async fn agent_on_pane(fx: &Fx, pane: &HerdrPaneId) -> bool {
    let snap = fx.herdr.snapshot().await.unwrap();
    snap.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).any(|p| p.id == *pane && p.agent.is_some())
}

#[tokio::test]
async fn dispatched_row_is_recovered_as_unknown() {
    let fx = fx();
    activate(&fx, "claude");
    fx.herdr.fail_next("agent.start", Fault::Unavailable);
    step(&fx).await;
    let c = only_clone(&fx, "foreman");
    let pane = pane_of(&fx, &c);
    let mut row = rows(&fx, EffectKind::StartAgent).remove(0);
    assert_eq!(row.status, EffectStatus::Pending);
    assert!(row.sched.retry_at.is_some(), "the failed start backs off: {row:?}");
    // The row as a crash right after the Herdr call would have left it.
    row.sched.dispatched = Some(Dispatch { attempt: 2, at: fx.clock.now() });
    row.sched.retry_at = None;
    fx.journal.upsert_effect(&row).unwrap();
    fx.herdr.set_agent(&pane, agent(AgentStatus::Idle));
    fx.herdr.clear_calls();

    step(&fx).await;
    let done = fx.journal.get_effect(&row.id).unwrap().unwrap();
    assert_eq!(done.status, EffectStatus::Done, "{done:?}");
    assert_eq!(done.sched.dispatched, None);
    assert!(start_calls(&fx).is_empty(), "the lost start was adopted, not repeated: {:?}", calls(&fx));
    assert!(fx.journal.meta_get(&planner::launched_key(&c.id)).unwrap().is_some(), "the launch is recorded");
    assert!(fx.notes.messages().is_empty(), "no needs-revision for an adopted start: {:?}", fx.notes.messages());
}

#[tokio::test]
async fn cancelled_start_agent_is_recovered_as_unknown() {
    let fx = fx();
    activate(&fx, "claude");
    fx.herdr.fail_next("agent.start", Fault::Hang);
    // The daemon's first-pass timeout cancels the step the same way.
    assert!(tokio::time::timeout(Duration::from_millis(300), fx.rec.step_fresh()).await.is_err());
    fx.w.drain().unwrap();
    let row = rows(&fx, EffectKind::StartAgent).remove(0);
    assert!(matches!(row.status, EffectStatus::Pending | EffectStatus::Unknown), "{row:?}");
    assert!(row.sched.dispatched.is_some(), "the cancelled call left its write-ahead marker: {row:?}");
    let pane = pane_of(&fx, &only_clone(&fx, "foreman"));
    assert!(agent_on_pane(&fx, &pane).await, "the hung call did reach Herdr");

    step(&fx).await;
    let done = fx.journal.get_effect(&row.id).unwrap().unwrap();
    assert_eq!(done.status, EffectStatus::Done, "{done:?}");
    assert_eq!(start_calls(&fx).len(), 1, "exactly one start in total: {:?}", calls(&fx));
}

#[tokio::test]
async fn cancelled_replacement_start_is_recovered() {
    let fx = fx();
    let (s, c, pane) = occupied(&fx).await;
    fx.herdr.set_process(&pane, non_shell());
    set_model(&fx, &s, "opus-x");
    step(&fx).await;
    let ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    assert_eq!(ef.status, EffectStatus::Pending);
    // The old occupant exits and the shell shows; the next step starts the new agent, but is cancelled in the call.
    fx.herdr.set_agent(&pane, None);
    admit_bookkeeping(&fx, json!({"sub": "test_end_occupant", "clone": c.id, "native": "abc"}));
    fx.herdr.set_process(&pane, shell());
    fx.herdr.clear_calls();
    fx.herdr.fail_next("agent.start", Fault::Hang);
    assert!(tokio::time::timeout(Duration::from_millis(300), fx.rec.step_fresh()).await.is_err());
    fx.w.drain().unwrap();
    assert_eq!(start_calls(&fx).len(), 1);
    let phase: super::session::ReplacePhase =
        serde_json::from_str(&fx.journal.meta_get(&format!("replace:{}", ef.id)).unwrap().expect("phase saved")).unwrap();
    assert!(matches!(phase, super::session::ReplacePhase::Starting { .. }), "{phase:?}");

    step(&fx).await;
    let done = fx.journal.get_effect(&ef.id).unwrap().unwrap();
    assert_eq!(done.status, EffectStatus::Done, "{done:?}");
    assert_eq!(start_calls(&fx).len(), 1, "no second start: {:?}", calls(&fx));
    assert_eq!(fx.journal.meta_get(&format!("replace:{}", ef.id)).unwrap(), None, "the phase is cleared");
}

#[tokio::test]
async fn replacement_without_starting_phase_does_not_adopt_old_agent() {
    let fx = fx();
    let (s, _, pane) = occupied(&fx).await;
    fx.herdr.set_agent(&pane, agent(AgentStatus::Working));
    set_model(&fx, &s, "opus-x");
    step(&fx).await;
    let mut ef = rows(&fx, EffectKind::ReplaceSession).remove(0);
    // An Unknown row with no phase saved: no start was ever dispatched, so the agent is still the old occupant.
    fx.journal.meta_delete(&format!("replace:{}", ef.id)).unwrap();
    ef.status = EffectStatus::Unknown;
    ef.sched = Default::default();
    fx.journal.upsert_effect(&ef).unwrap();
    fx.herdr.set_agent(&pane, agent(AgentStatus::Idle));
    fx.herdr.set_process(&pane, non_shell());
    fx.herdr.clear_calls();

    step(&fx).await;
    assert_eq!(send_keys_count(&fx), 2, "the exit sequence goes to the old agent: {:?}", calls(&fx));
    assert_ne!(fx.journal.get_effect(&ef.id).unwrap().unwrap().status, EffectStatus::Done);
    assert!(start_calls(&fx).is_empty());
}

#[tokio::test]
async fn multiple_clones_split_into_one_tab() {
    let fx = fx();
    activate(&fx, "shell");
    commit(&fx, "clone add foreman --name second");
    step(&fx).await;
    let s = seat(&fx, "foreman");
    let cs = clones_of(&fx, &s.id);
    assert_eq!(cs.len(), 2);
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::CreateTab(_))), 1);
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::SplitPane(_))), 1);
    let panes: Vec<_> = cs.iter().map(|c| pane_of(&fx, c)).collect();
    assert_ne!(panes[0], panes[1]);
    let snap = fx.herdr.snapshot().await.unwrap();
    let tab = snap.workspaces.iter().flat_map(|w| &w.tabs).find(|t| t.label == "foreman").unwrap();
    assert_eq!(tab.panes.len(), 2);
}

#[tokio::test]
async fn adopted_pane_of_retired_clone_is_not_closed() {
    use super::planner::{LiveRef, live_ref, set_live_ref};
    let fx = fx();
    activate(&fx, "shell");
    step(&fx).await;
    let a = only_clone(&fx, "foreman");
    let pa = pane_of(&fx, &a);
    commit(&fx, "clone add foreman --name second");
    let s = seat(&fx, "foreman");
    let b = clones_of(&fx, &s.id).into_iter().find(|c| c.id != a.id).unwrap();
    commit(&fx, &format!("clone retire {}", a.id));

    let snap = fx.herdr.snapshot().await.unwrap();
    let (ws, tab, pane) = snap
        .workspaces
        .iter()
        .flat_map(|w| w.tabs.iter().map(move |t| (w, t)))
        .flat_map(|(w, t)| t.panes.iter().map(move |p| (w, t, p)))
        .find(|(_, _, p)| p.id == pa)
        .unwrap();
    // The undo's adoption: B's committed binding names PA, which still carries hg=<A>, and the journal still
    // holds the live ref the original create of A left behind.
    set_live_ref(
        &fx.journal,
        &a.id.to_any(),
        &LiveRef { workspace: Some(ws.id.clone()), tab: Some(tab.id.clone()), pane: Some(pa.clone()), incarnation: snap.incarnation.clone() },
    );
    let binding = Binding {
        token: None,
        workspace_id: Some(ws.id.clone()),
        tab_id: Some(tab.id.clone()),
        pane_id: Some(pa.clone()),
        terminal_id: pane.terminal_id.clone(),
        incarnation: snap.incarnation.clone(),
    };
    admit_binding(&*fx.w, &b.id.to_any(), &binding, Availability::Present).unwrap();
    fx.w.drain().unwrap();

    fx.herdr.clear_calls();
    step(&fx).await;
    step(&fx).await;
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::ClosePane(_))), 0, "{:?}", calls(&fx));
    assert_eq!(count_calls(&fx, |c| matches!(c, FakeCall::SplitPane(_) | FakeCall::CreateTab(_))), 0, "{:?}", calls(&fx));
    let snap = fx.herdr.snapshot().await.unwrap();
    let live = snap.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).find(|p| p.id == pa).expect("PA survives");
    assert_eq!(live.metadata.get("hg"), Some(&format!("hg={}", b.id)));
    assert_eq!(live_ref(&fx.journal, &a.id.to_any()), None);
}

// ---- durable attention notices (hg-zmi.63) ----------------------------------------------------------

fn notices_in(fx: &Fx, state: &str) -> usize {
    fx.journal.notice_counts().unwrap().get(state).copied().unwrap_or(0) as usize
}

#[tokio::test]
async fn needs_revision_notice_survives_threads_down() {
    let fx = fx();
    let threads = Arc::new(crate::threads::fake::FakeThreads::new());
    threads.add_thread("t-foreman", "foreman");
    let rec = Reconciler::new(
        fx.store.clone(),
        fx.journal.clone(),
        fx.w.clone(),
        fx.herdr.clone(),
        fx.clock.clone(),
        Arc::new(ThreadsNotifier { threads: threads.clone(), journal: fx.journal.clone(), store: fx.store.clone() }),
        ReconcilerConfig::new(fx.deps.instance.clone()),
    );
    let step2 = || async {
        let r = rec.step_fresh().await;
        fx.w.drain().unwrap();
        r
    };
    activate(&fx, "claude");
    let foreman = seat(&fx, "foreman");
    admit_bookkeeping(&fx, json!({"sub": "test_set_thread", "seat": foreman.id, "thread": "t-foreman"}));
    fx.herdr.fail_next("agent.start", Fault::Unavailable);
    step2().await;
    let ef = rows(&fx, EffectKind::StartAgent).remove(0);
    let requester = crate::model::change::Requester { seat: Some(foreman.id.clone()), ..Default::default() };
    fx.journal.set_requester(&ef.op, &requester, fx.clock.now()).unwrap();

    threads.disconnect();
    let pane = pane_of(&fx, &only_clone(&fx, "foreman"));
    fx.herdr.set_process(&pane, non_shell());
    fx.clock.advance(chrono::Duration::seconds(5));
    step2().await;
    let ef = rows(&fx, EffectKind::StartAgent).remove(0);
    assert_eq!(ef.status, EffectStatus::NeedsRevision);
    let due = fx.journal.due_notices(fx.clock.now() + chrono::Duration::hours(1)).unwrap();
    assert_eq!(due.len(), 1, "{due:?}");
    assert_eq!(due[0].effect, ef.id);
    assert!(due[0].attempts >= 1 && due[0].last_error.is_some(), "{:?}", due[0]);
    assert!(threads.notifications().is_empty(), "nothing reached threads while it was down");
    assert!(rec.next_wake().is_some(), "the loop wakes for the notice retry");

    threads.reconnect();
    fx.clock.advance(chrono::Duration::minutes(10));
    step2().await;
    let sent = threads.notifications();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].thread, ThreadRef("t-foreman".into()));
    assert!(sent[0].body.contains("needs revision") && sent[0].body.contains("not the shell"), "{}", sent[0].body);
    assert_eq!(notices_in(&fx, "delivered"), 1);
    assert_eq!(notices_in(&fx, "pending"), 0);

    fx.clock.advance(chrono::Duration::minutes(10));
    step2().await;
    assert_eq!(threads.notifications().len(), 1, "delivered exactly once");
}

#[tokio::test]
async fn seatless_attention_notice_waits_for_affected_seat_channel() {
    let fx = fx();
    let threads = Arc::new(crate::threads::fake::FakeThreads::new());
    threads.add_thread("t-foreman", "foreman");
    let rec = Reconciler::new(
        fx.store.clone(), fx.journal.clone(), fx.w.clone(), fx.herdr.clone(), fx.clock.clone(),
        Arc::new(ThreadsNotifier { threads: threads.clone(), journal: fx.journal.clone(), store: fx.store.clone() }),
        ReconcilerConfig::new(fx.deps.instance.clone()),
    );
    activate(&fx, "claude");
    let foreman = seat(&fx, "foreman");
    fx.herdr.fail_next("agent.start", Fault::StartOutcome(StartOutcome::BlockedNeedsHuman));
    rec.step_fresh().await;
    fx.w.drain().unwrap();
    assert_eq!(notices_in(&fx, "pending"), 1, "no channel yet: do not drop the notice");
    assert_eq!(notices_in(&fx, "logged"), 0);
    assert!(threads.notifications().is_empty());
    assert!(rec.next_wake().is_some());

    admit_bookkeeping(&fx, json!({"sub": "test_set_thread", "seat": foreman.id, "thread": "t-foreman"}));
    fx.clock.advance(chrono::Duration::minutes(10));
    rec.deliver_notices().await;
    assert_eq!(notices_in(&fx, "pending"), 0);
    assert_eq!(notices_in(&fx, "delivered"), 1);
    let sent = threads.notifications();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].thread, ThreadRef("t-foreman".into()));
    rec.deliver_notices().await;
    assert_eq!(threads.notifications().len(), 1);
}

#[tokio::test]
async fn blocked_needs_human_is_noticed() {
    let fx = fx();
    activate(&fx, "claude");
    fx.herdr.fail_next("agent.start", Fault::StartOutcome(StartOutcome::BlockedNeedsHuman));
    step(&fx).await;
    let ef = rows(&fx, EffectKind::StartAgent).remove(0);
    assert_eq!(ef.status, EffectStatus::BlockedNeedsHuman);
    let key = notice_key(&ef.op, &format!("agent on {} is waiting for a human (trust or auth dialog)", ef.object));
    let n = fx.journal.get_notice(&key.0).unwrap().expect("a notice was journaled with the status");
    assert_eq!(n.effect, ef.id);
    assert_eq!(n.state, "delivered", "the sweep at the end of the step delivered it");
    assert_eq!(fx.notes.messages(), vec![n.text.clone()]);
    step(&fx).await;
    assert_eq!(fx.notes.messages().len(), 1, "not delivered again");
}

struct FailSource(AnyId);
impl EffectSource for FailSource {
    fn effects(&self, cx: &DiffCx<'_>) -> Vec<PlannedEffect> {
        let kind = EffectKind::Custom("demo.fail".into());
        let op = OpId::from_ulid(ulid::Ulid::nil());
        let id = EffectRecord::identity(&op, &self.0, &kind, 1);
        vec![PlannedEffect {
            record: EffectRecord {
                id,
                op,
                object: self.0.clone(),
                kind,
                object_rev: 1,
                fencing_rev: 1,
                status: EffectStatus::Pending,
                predicted: vec![],
                nonce_label: None,
                attempts: 0,
                last_error: None,
                updated_at: cx.now,
                sched: Default::default(),
            },
            deps: vec![],
        }]
    }
}

struct FailExec;
#[async_trait::async_trait]
impl EffectExecutor for FailExec {
    fn handles(&self, kind: &EffectKind) -> bool {
        matches!(kind, EffectKind::Custom(k) if k == "demo.fail")
    }
    async fn execute(&self, _cx: &ExecCx<'_>, _e: &EffectRecord) -> ExecOutcome {
        ExecOutcome::Failed("boom".into())
    }
}

#[tokio::test]
async fn failed_effect_is_noticed_and_counted() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    fx.rec.register_source(Arc::new(FailSource(SeatId::new().to_any())));
    fx.rec.register_executor(Arc::new(FailExec));
    step(&fx).await;
    let ef = rows(&fx, EffectKind::Custom("demo.fail".into())).remove(0);
    assert_eq!((ef.status, ef.last_error.as_deref()), (EffectStatus::Failed, Some("boom")));
    let msgs = fx.notes.messages();
    assert_eq!(msgs.len(), 1, "{msgs:?}");
    assert!(msgs[0].contains("failed: boom"), "{msgs:?}");
    assert_eq!(notices_in(&fx, "delivered"), 1);
    let status = crate::daemon::compose::reconciler_status(&fx.journal);
    assert_eq!(status["failed_effects"], 1);
    assert_eq!(status["attention"][0]["last_error"], "boom");
}

#[tokio::test]
async fn notice_is_voided_when_the_effect_leaves_the_attention_status() {
    let fx = fx();
    let op = OpId::new();
    let object = SeatId::new().to_any();
    let kind = EffectKind::CreateTab;
    let mut row = EffectRecord {
        id: EffectRecord::identity(&op, &object, &kind, 1),
        op: op.clone(),
        object,
        kind,
        object_rev: 1,
        fencing_rev: 1,
        status: EffectStatus::NeedsRevision,
        predicted: vec![],
        nonce_label: None,
        attempts: 1,
        last_error: Some("x".into()),
        updated_at: fx.clock.now(),
        sched: Default::default(),
    };
    let n = Notice {
        key: "k".into(),
        effect: row.id.clone(),
        op,
        severity: Severity::Warn,
        text: "t".into(),
        state: "pending".into(),
        attempts: 0,
        last_error: None,
        next_at: None,
    };
    fx.journal.upsert_effect_with_notice(&row, &n).unwrap();
    row.status = EffectStatus::Done;
    fx.journal.upsert_effect(&row).unwrap();
    fx.rec.deliver_notices().await;
    assert_eq!(fx.journal.get_notice("k").unwrap().unwrap().state, "void");
    assert!(fx.notes.messages().is_empty());
}
