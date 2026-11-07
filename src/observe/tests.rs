//! Observer tests: FakeHerdr + real writer, journal, git store, plan engine and reconciler, driven through
//! `RuntimeLoop::step_once`. The "user" is `FakeHerdr::user_*`.
use super::mutations::{availability_request, cascade_request, cascade_request_with};
use super::step::{LoopTuning, StepMode};
use super::*;
use crate::config::InstancePaths;
use crate::daemon::registry::CallerInfo;
use crate::herdr::fake::{FakeCall, FakeHerdr, Fault};
use crate::journal::Journal;
use crate::model::action::{ActionKind, ActionRecord};
use crate::model::clone::CloneRecord;
use crate::model::common::{Availability, CloneLifecycle, Lifecycle, NameSource, RetireMechanism};
use crate::model::effect::{
    ContainerKind, EffectKind, EffectRecord, EffectStatus, EndState, PredictedEnd,
};
use crate::model::harness::{Harness, claude_project_slug};
use crate::model::native_session::SessionEndReason;
use crate::model::operation::OpState;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{
    AnyId, CloneId, EffectId, HerdrPaneId, HerdrTabId, HerdrWorkspaceId, OpId, SeatId, Timestamp,
};
use crate::plan::commands::{PlanDeps, admit_apply, create_plan};
use crate::plan::core_kinds::register_core_kinds;
use crate::plan::kind::KindRegistry;
use crate::plan::store::PlanStore;
use crate::plan::types::StoredPlan;
use crate::ports::clock::{Clock, ManualClock};
use crate::ports::herdr::{
    AgentInfo, AgentSession, AgentStatus, CreateTab, HerdrApi, HerdrSnapshot, PaneInfo, ProcessInfo,
};
use crate::ports::writer::{Writer, WriterError};
use crate::reconcile::{Reconciler, ReconcilerConfig, RequesterNotifier, predictions};
use crate::store::GitStore;
use crate::store::init::init_instance;
use crate::store::layout;
use crate::store::tree::CommitView;
use crate::writer::{WriterConfig, WriterCore};
use chrono::TimeZone;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn t0() -> Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

struct QuietNotifier;
impl RequesterNotifier for QuietNotifier {
    fn notify(&self, _: &OpId, _: crate::ports::threads::Severity, _: &str) {}
}

/// Admission drains the writer synchronously, so every observed op is terminal when `admit` returns and the
/// tests are deterministic. `hold` leaves ops queued (admitted but never applied).
struct DrainingWriter {
    core: Arc<WriterCore>,
    hold: AtomicBool,
}
impl Writer for DrainingWriter {
    fn admit(&self, request: crate::model::change::ChangeRequest) -> Result<OpId, WriterError> {
        let op = self.core.admit(request)?;
        if !self.hold.load(Ordering::SeqCst) {
            self.core.drain()?;
        }
        Ok(op)
    }
    fn status(&self, op: &OpId) -> Result<Option<OpState>, WriterError> {
        self.core.status(op)
    }
}

/// Test-only: point `graph.toml node_manager_seat` at a seat (the real `node-manager set` is another task's).
struct TestNodeManager;
impl crate::writer::Mutation for TestNodeManager {
    fn apply(
        &self,
        cx: &mut crate::writer::MutationCx<'_>,
    ) -> Result<crate::writer::Applied, crate::writer::MutationError> {
        let seat: SeatId = cx.request.args["seat"].as_str().unwrap().parse().unwrap();
        let mut g = layout::read_graph(&cx.tree)?;
        g.node_manager_seat = Some(seat);
        cx.tree.put_file(
            layout::graph_toml(),
            toml::to_string(&g).unwrap().into_bytes(),
        );
        Ok(crate::writer::Applied {
            summary: "set node manager".into(),
            action: None,
        })
    }
}

struct Fx {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    deps: PlanDeps,
    w: Arc<WriterCore>,
    dw: Arc<DrainingWriter>,
    store: Arc<GitStore>,
    journal: Arc<Journal>,
    herdr: Arc<FakeHerdr>,
    clock: Arc<ManualClock>,
    rec: Arc<Reconciler>,
    lp: Arc<RuntimeLoop>,
}

fn new_loop(
    herdr: &Arc<FakeHerdr>,
    store: &Arc<GitStore>,
    dw: &Arc<DrainingWriter>,
    journal: &Arc<Journal>,
    rec: &Arc<Reconciler>,
    clock: &Arc<ManualClock>,
    root: &Path,
) -> Arc<RuntimeLoop> {
    let lp = RuntimeLoop::new(
        herdr.clone(),
        store.clone(),
        dw.clone(),
        journal.clone(),
        rec.clone(),
        clock.clone(),
        InstancePaths::new(root),
        Duration::from_secs(60),
        None,
        root.join("claude"),
    );
    lp.set_tuning(LoopTuning {
        commit_timeout: Duration::from_secs(5),
        settle_timeout: Duration::from_millis(50),
        debounce: Duration::from_millis(5),
    });
    lp
}

fn fx() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("inst");
    init_instance(&root).unwrap();
    crate::plan::managers::testing::opt_out(&root);
    let mut kinds = KindRegistry::default();
    register_core_kinds(&mut kinds);
    crate::plan::kinds_extra::register_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    let plans = Arc::new(PlanStore::new(root.join(".graph-local/plans")));
    let mut reg = crate::writer::MutationRegistry::default();
    kinds.register_mutations(&mut reg, plans.clone());
    crate::reconcile::register_mutations(&mut reg);
    register_mutations(&mut reg);
    reg.register(
        "bookkeeping.test_node_manager_obs",
        Arc::new(TestNodeManager),
    );
    reg.register(
        crate::plan::managers::reparent::testing::SET_POINTER_KEY,
        Arc::new(crate::plan::managers::reparent::testing::SetPointer),
    );
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
    let dw = Arc::new(DrainingWriter {
        core: w.clone(),
        hold: AtomicBool::new(false),
    });
    let deps = PlanDeps {
        kinds,
        plans,
        store: store.clone(),
        writer: w.clone(),
        clock: clock.clone(),
        instance: root.clone(),
    };
    let herdr = FakeHerdr::new();
    let mut cfg = ReconcilerConfig::new(root.clone());
    cfg.idle_timeout = Duration::from_millis(100);
    cfg.exit_timeout = Duration::from_millis(100);
    cfg.exit_followup = Duration::from_millis(20);
    cfg.deferred_recheck = Duration::from_millis(50);
    let rec = Reconciler::new(
        store.clone(),
        journal.clone(),
        dw.clone(),
        herdr.clone(),
        clock.clone(),
        Arc::new(QuietNotifier),
        cfg,
    );
    let lp = new_loop(&herdr, &store, &dw, &journal, &rec, &clock, &root);
    Fx {
        _tmp: tmp,
        root,
        deps,
        w,
        dw,
        store,
        journal,
        herdr,
        clock,
        rec,
        lp,
    }
}

impl Fx {
    /// A second loop over the same instance and Herdr (a daemon restart): fresh in-memory state, baseline from disk.
    fn restarted_loop(&self) -> Arc<RuntimeLoop> {
        new_loop(
            &self.herdr,
            &self.store,
            &self.dw,
            &self.journal,
            &self.rec,
            &self.clock,
            &self.root,
        )
    }
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn plan(fx: &Fx, change: &str) -> StoredPlan {
    let v = create_plan(&fx.deps, &CallerInfo::default(), words(change))
        .unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: crate::model::PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    fx.deps.plans.get(&id).unwrap().unwrap()
}

fn commit(fx: &Fx, change: &str) {
    let sp = plan(fx, change);
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

fn view(fx: &Fx) -> CommitView<'_> {
    CommitView {
        store: &*fx.store,
        at: crate::ports::store::Store::head(&*fx.store).unwrap(),
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

fn seat_by_id(fx: &Fx, id: &SeatId) -> (crate::ports::store::ObjectLocation, SeatRecord) {
    layout::all_seats(&view(fx))
        .unwrap()
        .into_iter()
        .find(|(_, s)| &s.id == id)
        .expect("seat exists")
}

fn ts_rec(fx: &Fx, name: &str) -> TeamspaceRecord {
    layout::list_teamspaces(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, t)| t)
        .find(|t| t.name == name)
        .expect("teamspace")
}

fn clones_of(fx: &Fx, seat: &SeatId) -> Vec<CloneRecord> {
    layout::all_clones(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, c)| c)
        .filter(|c| &c.seat == seat)
        .collect()
}

fn only_clone(fx: &Fx, seat_name: &str) -> CloneRecord {
    let s = seat(fx, seat_name);
    let mut cs = clones_of(fx, &s.id);
    assert_eq!(cs.len(), 1, "clones of {seat_name}");
    cs.remove(0)
}

fn clone_by_id(fx: &Fx, id: &CloneId) -> CloneRecord {
    layout::all_clones(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, c)| c)
        .find(|c| &c.id == id)
        .expect("clone")
}

fn actions(fx: &Fx, kind: ActionKind) -> Vec<ActionRecord> {
    layout::list_actions(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, a)| a)
        .filter(|a| a.kind == kind)
        .collect()
}

fn calls(fx: &Fx) -> Vec<FakeCall> {
    fx.herdr.calls()
}

fn creates(fx: &Fx) -> usize {
    calls(fx)
        .iter()
        .filter(|c| {
            matches!(
                c,
                FakeCall::CreateWorkspace(_) | FakeCall::CreateTab(_) | FakeCall::SplitPane(_)
            )
        })
        .count()
}

async fn step(fx: &Fx) -> super::step::StepSummary {
    fx.lp.step_once().await.expect("step")
}

/// Step until the loop has nothing left to observe or reconcile.
async fn settle(fx: &Fx) {
    for _ in 0..8 {
        let s = step(fx).await;
        let quiet = s.observed.is_empty()
            && s.reconcile
                .as_ref()
                .is_some_and(|r| r.planned.is_empty() && r.executed.is_empty());
        if quiet {
            return;
        }
    }
    panic!("loop did not settle");
}

async fn snapshot(fx: &Fx) -> HerdrSnapshot {
    fx.herdr.snapshot().await.unwrap()
}

fn pane_of(fx: &Fx, c: &CloneRecord) -> HerdrPaneId {
    clone_by_id(fx, &c.id)
        .runtime
        .bound
        .expect("clone is bound")
        .pane_id
        .expect("bound to a pane")
}

fn tab_of_seat(fx: &Fx, name: &str) -> HerdrTabId {
    seat(fx, name)
        .runtime
        .bound
        .expect("seat is bound")
        .tab_id
        .expect("bound to a tab")
}

async fn workspace_id(fx: &Fx) -> HerdrWorkspaceId {
    snapshot(fx).await.workspaces[0].id.clone()
}

/// Herdr's own default tab goes, so the seat tab is the workspace's last.
async fn drop_default_tab(fx: &Fx) {
    let snap = snapshot(fx).await;
    let t = snap.workspaces[0]
        .tabs
        .iter()
        .find(|t| t.label == "1")
        .unwrap()
        .id
        .clone();
    fx.herdr.user_close_tab(&t);
}

fn activate(fx: &Fx, harness: &str) {
    commit(fx, "teamspace create alpha");
    commit(
        fx,
        &format!("seat create foreman --teamspace alpha --active --harness {harness}"),
    );
}

fn make_node_manager(fx: &Fx, seat: &SeatId) {
    fx.w.admit(crate::model::change::ChangeRequest {
        kind: crate::model::change::RequestKind::Bookkeeping,
        args: serde_json::json!({"sub": "test_node_manager_obs", "seat": seat}),
        relied_on: vec![],
        requester: Default::default(),
        supersedes: None,
        confirmed: None,
    })
    .unwrap();
    fx.w.drain().unwrap();
}

fn claude_agent(session: Option<&str>) -> Option<AgentInfo> {
    Some(AgentInfo {
        kind: "claude".into(),
        status: AgentStatus::Idle,
        session: session.map(|s| AgentSession::Id(s.into())),
    })
}

// =================================================================================================
// closure cascades
// =================================================================================================

#[tokio::test]
async fn pane_close_retires_clone_only() {
    let fx = fx();
    activate(&fx, "claude");
    commit(&fx, "clone add foreman --name second");
    settle(&fx).await;
    let s = seat(&fx, "foreman");
    let mut cs = clones_of(&fx, &s.id);
    assert_eq!(cs.len(), 2);
    let victim = cs.remove(0);
    let keeper = cs.remove(0);
    fx.herdr.user_close_pane(&pane_of(&fx, &victim));

    let sum = step(&fx).await;
    assert_eq!(sum.mode, Some(StepMode::Diff));
    let v = clone_by_id(&fx, &victim.id);
    assert_eq!(v.lifecycle, CloneLifecycle::Retired);
    let r = v.retired.expect("retirement recorded");
    assert_eq!(r.mechanism, RetireMechanism::ObservedPaneClose);
    assert!(r.action.is_some());
    assert_eq!(
        clone_by_id(&fx, &keeper.id).lifecycle,
        CloneLifecycle::Active
    );
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Active);
    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].retired, vec![victim.id.to_any()]);
    assert!(acts[0].already_retired.is_empty());
    assert_eq!(Some(acts[0].id.clone()), r.action);

    // Level-triggered: nothing more happens and the pane is not recreated.
    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
}

#[tokio::test]
async fn last_pane_close_closing_tab_retires_seat_once() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let s = seat(&fx, "foreman");
    fx.herdr.user_close_pane(&pane_of(&fx, &c));

    step(&fx).await;
    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1, "one cascade, not one per object");
    let retired: std::collections::BTreeSet<_> = acts[0].retired.iter().cloned().collect();
    assert_eq!(
        retired,
        [s.id.to_any(), c.id.to_any()].into_iter().collect()
    );
    let (loc, s2) = seat_by_id(&fx, &s.id);
    assert_eq!(s2.lifecycle, Lifecycle::Retired);
    assert_eq!(
        s2.retired.unwrap().mechanism,
        RetireMechanism::ObservedTabClose
    );
    assert!(
        loc.folder.as_str().contains("/archive/seats/"),
        "archived: {}",
        loc.folder.as_str()
    );
    let c2 = clone_by_id(&fx, &c.id);
    assert_eq!(
        c2.retired.unwrap().mechanism,
        RetireMechanism::ObservedTabClose
    );
    assert_eq!(
        ts_rec(&fx, "alpha").lifecycle,
        Lifecycle::Active,
        "the workspace survives its other tab"
    );

    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);
    assert_eq!(creates(&fx), 0);
}

#[tokio::test]
async fn workspace_close_retires_dormant_seats_too() {
    let fx = fx();
    activate(&fx, "shell");
    commit(
        &fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    commit(&fx, "seat deactivate bench");
    settle(&fx).await;
    assert_eq!(seat(&fx, "bench").lifecycle, Lifecycle::Dormant);
    let ws = workspace_id(&fx).await;
    fx.herdr.user_close_workspace(&ws);

    step(&fx).await;
    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1);
    let (foreman, bench) = (seat(&fx, "foreman"), seat(&fx, "bench"));
    let expect: std::collections::BTreeSet<AnyId> = [
        ts_rec(&fx, "alpha").id.to_any(),
        foreman.id.to_any(),
        bench.id.to_any(),
        only_clone(&fx, "foreman").id.to_any(),
        only_clone(&fx, "bench").id.to_any(),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        acts[0]
            .retired
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        expect
    );
    assert!(acts[0].already_retired.is_empty());
    let ts = ts_rec(&fx, "alpha");
    assert_eq!(ts.lifecycle, Lifecycle::Retired);
    assert_eq!(
        ts.retired.unwrap().mechanism,
        RetireMechanism::ObservedWorkspaceClose
    );
    assert_eq!(
        seat_by_id(&fx, &bench.id).1.lifecycle,
        Lifecycle::Retired,
        "the dormant seat goes with the workspace"
    );
    assert_eq!(seat_by_id(&fx, &foreman.id).1.lifecycle, Lifecycle::Retired);
}

/// The seat has a live tab and its clone a live pane in the fake Herdr.
async fn assert_reopened(fx: &Fx, seat_name: &str) {
    let s = seat(fx, seat_name);
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert_eq!(s.runtime.availability, Availability::Present, "{seat_name}");
    let c = only_clone(fx, seat_name);
    assert_eq!(c.lifecycle, CloneLifecycle::Active);
    let pane = pane_of(fx, &c);
    let snap = snapshot(fx).await;
    assert!(
        snap.workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .flat_map(|t| &t.panes)
            .any(|p| p.id == pane),
        "the clone's pane exists in Herdr again"
    );
}

#[tokio::test]
async fn node_manager_tab_close_is_not_retired_and_reopens() {
    let fx = fx();
    activate(&fx, "shell");
    settle(&fx).await;
    let foreman = seat(&fx, "foreman");
    make_node_manager(&fx, &foreman.id);
    let old_pane = pane_of(&fx, &only_clone(&fx, "foreman"));
    fx.herdr.user_close_tab(&tab_of_seat(&fx, "foreman"));
    fx.herdr.clear_calls();

    step(&fx).await;
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Active);
    settle(&fx).await;
    assert!(creates(&fx) > 0, "{:?}", calls(&fx));
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    assert_reopened(&fx, "foreman").await;
    assert_ne!(pane_of(&fx, &only_clone(&fx, "foreman")), old_pane);
}

#[tokio::test]
async fn node_manager_pane_close_is_not_retired() {
    let fx = fx();
    activate(&fx, "shell");
    commit(&fx, "clone add foreman --name second");
    settle(&fx).await;
    let foreman = seat(&fx, "foreman");
    make_node_manager(&fx, &foreman.id);
    let victim = clones_of(&fx, &foreman.id).remove(0);
    fx.herdr.user_close_pane(&pane_of(&fx, &victim));
    fx.herdr.clear_calls();

    step(&fx).await;
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    assert_eq!(
        clone_by_id(&fx, &victim.id).lifecycle,
        CloneLifecycle::Active
    );
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Active);
    settle(&fx).await;
    assert!(creates(&fx) > 0, "the pane is recreated: {:?}", calls(&fx));
    assert!(
        clones_of(&fx, &foreman.id)
            .iter()
            .all(|c| c.lifecycle == CloneLifecycle::Active && c.runtime.bound.is_some())
    );
}

#[tokio::test]
async fn node_manager_tab_close_still_cascades_other_seats() {
    let fx = fx();
    activate(&fx, "shell");
    commit(
        &fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    make_node_manager(&fx, &seat(&fx, "foreman").id);
    fx.herdr.user_close_tab(&tab_of_seat(&fx, "bench"));
    step(&fx).await;
    assert_eq!(seat(&fx, "bench").lifecycle, Lifecycle::Retired);
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Active);
}

#[tokio::test]
async fn workspace_close_keeps_node_manager_teamspace_active() {
    let fx = fx();
    activate(&fx, "shell");
    commit(
        &fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    let (foreman, bench) = (seat(&fx, "foreman"), seat(&fx, "bench"));
    make_node_manager(&fx, &foreman.id);
    let bench_clone = only_clone(&fx, "bench");
    let ws = workspace_id(&fx).await;
    fx.herdr.user_close_workspace(&ws);
    fx.herdr.clear_calls();

    step(&fx).await;
    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1);
    let retired: std::collections::BTreeSet<AnyId> = acts[0].retired.iter().cloned().collect();
    assert_eq!(
        retired,
        [bench.id.to_any(), bench_clone.id.to_any()]
            .into_iter()
            .collect(),
        "only the other seat and its clone"
    );
    let ts = ts_rec(&fx, "alpha");
    assert_eq!(ts.lifecycle, Lifecycle::Active);
    assert!(ts.retired.is_none());
    assert_eq!(seat_by_id(&fx, &bench.id).1.lifecycle, Lifecycle::Retired);
    assert_eq!(seat_by_id(&fx, &foreman.id).1.lifecycle, Lifecycle::Active);

    settle(&fx).await;
    let log = calls(&fx);
    let first_ws = log
        .iter()
        .position(|c| matches!(c, FakeCall::CreateWorkspace(_)))
        .expect("workspace recreated");
    let first_tab = log
        .iter()
        .position(|c| matches!(c, FakeCall::CreateTab(_)))
        .expect("tab recreated");
    assert!(first_ws < first_tab, "{log:?}");
    assert_reopened(&fx, "foreman").await;
    assert_eq!(ts_rec(&fx, "alpha").lifecycle, Lifecycle::Active);
}

#[tokio::test]
async fn node_manager_move_is_tolerated_and_warned() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(
        &fx,
        "seat create foreman --teamspace alpha --active --harness shell",
    );
    commit(
        &fx,
        "seat create other --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    let foreman = seat(&fx, "foreman");
    make_node_manager(&fx, &foreman.id);
    let fc = only_clone(&fx, "foreman");
    let oc = only_clone(&fx, "other");
    let before = snapshot(&fx).await;
    let other_tab = tab_of_seat(&fx, "other");
    fx.herdr.user_move_pane(&pane_of(&fx, &fc), &other_tab);
    let after = snapshot(&fx).await;

    // Classify by hand: classification reports the move; the observer step turns it into the warning.
    let desired = crate::reconcile::desired::DesiredRuntime::load(&view(&fx), &fx.root).unwrap();
    assert_eq!(desired.node_manager, Some(foreman.id.clone()));
    let (mb, ma) = (
        super::matcher::match_snapshot(&before, &desired),
        super::matcher::match_snapshot(&after, &desired),
    );
    let moved = |c: &CloneRecord| super::diff::DiffElement::Moved {
        clone: c.id.clone(),
        from_tab: HerdrTabId("unused".into()),
        to_tab: other_tab.clone(),
    };
    let run = |els| {
        super::classify::classify(
            &super::classify::ClassifyCx {
                desired: &desired,
                journal: &fx.journal,
                preds: &[],
                matches: &ma,
                baseline: Some(&mb),
            },
            els,
        )
    };
    let out = run(vec![moved(&fc)]);
    assert!(
        out.warnings.is_empty(),
        "classify decides nothing about the node manager"
    );
    assert_eq!(out.moves.len(), 1, "the move is recorded as before");
    let warn = super::step::node_manager_move_warning(&desired, &out).expect("warned");
    assert!(warn.contains("foreman"), "{warn}");
    assert!(warn.contains(foreman.id.as_str()));
    let other_move = run(vec![moved(&oc)]);
    assert!(super::step::node_manager_move_warning(&desired, &other_move).is_none());

    // End to end: the move is recorded, not reverted, and not retired.
    fx.herdr.clear_calls();
    step(&fx).await;
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    assert!(seat(&fx, "foreman").moved_out);
    assert!(clone_by_id(&fx, &fc.id).reload_required);
    settle(&fx).await;
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Active);
}

/// Foreman (node manager) and `other` active in alpha, settled.
async fn nm_and_other() -> (Fx, SeatRecord) {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(
        &fx,
        "seat create foreman --teamspace alpha --active --harness shell",
    );
    commit(
        &fx,
        "seat create other --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    let foreman = seat(&fx, "foreman");
    make_node_manager(&fx, &foreman.id);
    (fx, foreman)
}

/// Spec §4.9 B11: the node manager's only pane moved out of a plan into a fresh tab (`pane move --new-tab`, or
/// `--new-workspace` when `new_workspace`). The seat follows: the new tab becomes its tab, nothing reopens at the
/// old home, the seat keeps its name (the tab is relabelled to it), and the clone is told to reload.
async fn nm_follows_move_to_fresh_tab(new_workspace: bool) {
    let (fx, foreman) = nm_and_other().await;
    let fc = only_clone(&fx, "foreman");
    let pane = pane_of(&fx, &fc);
    let home_ws = ts_rec(&fx, "alpha")
        .runtime
        .bound
        .unwrap()
        .workspace_id
        .unwrap();
    let target = (!new_workspace).then_some(&home_ws);
    fx.herdr.clear_calls();
    let new_tab = fx.herdr.user_move_pane_new_tab(&pane, target, "7");
    settle(&fx).await;

    let all = calls(&fx);
    let s = seat(&fx, "foreman");
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert!(!s.moved_out, "the seat follows its pane: {all:?}");
    assert_eq!(s.name, foreman.name, "a move is not a rename: {all:?}");
    assert_eq!(tab_of_seat(&fx, "foreman"), new_tab, "{all:?}");
    assert_eq!(s.runtime.availability, Availability::Present);
    assert_eq!(creates(&fx), 0, "nothing reopens at the old home: {all:?}");
    let c = clone_by_id(&fx, &fc.id);
    assert_eq!(pane_of(&fx, &c), pane, "the clone stays bound to its pane");
    assert!(c.reload_required);
    let snap = snapshot(&fx).await;
    let live = snap
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .find(|t| t.id == new_tab)
        .expect("the followed tab is live");
    assert_eq!(
        live.label, foreman.name,
        "the followed tab carries the seat's name"
    );
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    assert_eq!(seat(&fx, "other").lifecycle, Lifecycle::Active);

    // Closing the followed tab is an ordinary node-manager closure: it reopens.
    fx.herdr.user_close_tab(&new_tab);
    fx.herdr.clear_calls();
    settle(&fx).await;
    assert_reopened(&fx, "foreman").await;
}

#[tokio::test]
async fn nm_follows_pane_moved_to_new_tab() {
    nm_follows_move_to_fresh_tab(false).await;
}

#[tokio::test]
async fn nm_follows_pane_moved_to_new_workspace() {
    nm_follows_move_to_fresh_tab(true).await;
}

/// A plain seat moved the same way keeps the moved-out rule: B11 adoption is for the node manager only.
#[tokio::test]
async fn plain_seat_pane_moved_to_new_tab_is_moved_out() {
    let (fx, _) = nm_and_other().await;
    let oc = only_clone(&fx, "other");
    let home_ws = ts_rec(&fx, "alpha")
        .runtime
        .bound
        .unwrap()
        .workspace_id
        .unwrap();
    fx.herdr
        .user_move_pane_new_tab(&pane_of(&fx, &oc), Some(&home_ws), "7");
    settle(&fx).await;
    let s = seat(&fx, "other");
    assert!(s.moved_out);
    assert_eq!(s.lifecycle, Lifecycle::Active);
}

/// Foreman (node manager) and `other` active in alpha, settled; foreman's only pane is moved into `other`'s tab.
/// Returns the fixture, foreman's clone and `other`'s tab.
async fn node_manager_moved_into_other() -> (Fx, CloneRecord, HerdrTabId) {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(
        &fx,
        "seat create foreman --teamspace alpha --active --harness shell",
    );
    commit(
        &fx,
        "seat create other --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    make_node_manager(&fx, &seat(&fx, "foreman").id);
    let fc = only_clone(&fx, "foreman");
    let other_tab = tab_of_seat(&fx, "other");
    fx.herdr.user_move_pane(&pane_of(&fx, &fc), &other_tab);
    step(&fx).await;
    assert!(seat(&fx, "foreman").moved_out);
    fx.herdr.clear_calls();
    settle(&fx).await;
    assert_eq!(
        creates(&fx),
        0,
        "the move stays tolerated: {:?}",
        calls(&fx)
    );
    (fx, fc, other_tab)
}

#[tokio::test]
async fn node_manager_closed_after_move_is_reopened() {
    let (fx, fc, _) = node_manager_moved_into_other().await;
    fx.herdr.user_close_pane(&pane_of(&fx, &fc));
    fx.herdr.clear_calls();
    step(&fx).await;
    let s = seat(&fx, "foreman");
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert!(
        !s.moved_out,
        "the closed node manager no longer lives elsewhere"
    );
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    settle(&fx).await;
    assert!(creates(&fx) > 0, "{:?}", calls(&fx));
    assert!(
        calls(&fx)
            .iter()
            .any(|c| matches!(c, FakeCall::CreateTab(_))),
        "{:?}",
        calls(&fx)
    );
    assert_reopened(&fx, "foreman").await;
}

#[tokio::test]
async fn node_manager_moved_into_closed_tab_is_reopened() {
    let (fx, _, other_tab) = node_manager_moved_into_other().await;
    fx.herdr.user_close_tab(&other_tab);
    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(seat(&fx, "other").lifecycle, Lifecycle::Retired);
    let s = seat(&fx, "foreman");
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert!(!s.moved_out);
    settle(&fx).await;
    assert!(
        calls(&fx)
            .iter()
            .any(|c| matches!(c, FakeCall::CreateTab(_))),
        "{:?}",
        calls(&fx)
    );
    assert_reopened(&fx, "foreman").await;
}

#[tokio::test]
async fn node_manager_moved_to_other_workspace_survives_home_workspace_close() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(
        &fx,
        "seat create foreman --teamspace alpha --active --harness shell",
    );
    commit(
        &fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    commit(&fx, "teamspace create beta");
    commit(
        &fx,
        "seat create bob --teamspace beta --active --harness shell",
    );
    settle(&fx).await;
    make_node_manager(&fx, &seat(&fx, "foreman").id);
    let alpha_ws = ts_rec(&fx, "alpha")
        .runtime
        .bound
        .unwrap()
        .workspace_id
        .unwrap();
    let beta_ws = ts_rec(&fx, "beta")
        .runtime
        .bound
        .unwrap()
        .workspace_id
        .unwrap();
    assert_ne!(alpha_ws, beta_ws);
    let fc = only_clone(&fx, "foreman");
    let bob_clone = only_clone(&fx, "bob");
    let (bob_tab, bob_pane) = (tab_of_seat(&fx, "bob"), pane_of(&fx, &bob_clone));
    fx.herdr.user_move_pane(&pane_of(&fx, &fc), &bob_tab);
    settle(&fx).await;
    assert!(seat(&fx, "foreman").moved_out);
    let moved_pane = pane_of(&fx, &fc);

    fx.herdr.user_close_workspace(&alpha_ws);
    fx.herdr.clear_calls();
    settle(&fx).await;

    let all = calls(&fx);
    assert_eq!(seat(&fx, "bob").lifecycle, Lifecycle::Active, "{all:?}");
    assert_eq!(ts_rec(&fx, "beta").lifecycle, Lifecycle::Active, "{all:?}");
    let bob_after = clone_by_id(&fx, &bob_clone.id);
    assert_eq!(bob_after.lifecycle, CloneLifecycle::Active, "{all:?}");
    assert_eq!(pane_of(&fx, &bob_after), bob_pane, "{all:?}");
    for c in &all {
        let touches_beta = match c {
            FakeCall::RenameWorkspace(w, _)
            | FakeCall::ReportWorkspaceMetadata(w, ..)
            | FakeCall::CloseWorkspace(w) => *w == beta_ws,
            FakeCall::CloseTab(t) => *t == bob_tab,
            FakeCall::CreateTab(ct) => ct.workspace == beta_ws,
            _ => false,
        };
        assert!(!touches_beta, "{c:?} touches beta: {all:?}");
    }
    let foreman = seat(&fx, "foreman");
    assert_eq!(foreman.lifecycle, Lifecycle::Active, "{all:?}");
    assert!(foreman.moved_out, "{all:?}");
    let fc_after = clone_by_id(&fx, &fc.id);
    assert_eq!(fc_after.lifecycle, CloneLifecycle::Active, "{all:?}");
    assert_eq!(pane_of(&fx, &fc_after), moved_pane, "{all:?}");
    assert_eq!(seat(&fx, "bench").lifecycle, Lifecycle::Retired, "{all:?}");
    assert_eq!(ts_rec(&fx, "alpha").lifecycle, Lifecycle::Active, "{all:?}");
}

#[tokio::test]
async fn cascade_rechecks_node_manager_at_apply() {
    let fx = fx();
    activate(&fx, "shell");
    commit(
        &fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    let (foreman, bench) = (seat(&fx, "foreman"), seat(&fx, "bench"));
    let (fc, bc) = (only_clone(&fx, "foreman"), only_clone(&fx, "bench"));
    let ts = ts_rec(&fx, "alpha");
    // Classified while foreman was an ordinary seat ...
    let tab_req = cascade_request(
        super::classify::CascadeRule::Tab,
        &[foreman.id.to_any(), fc.id.to_any()],
        fx.clock.now(),
    );
    let ws_req = cascade_request(
        super::classify::CascadeRule::Workspace,
        &[
            ts.id.to_any(),
            foreman.id.to_any(),
            fc.id.to_any(),
            bench.id.to_any(),
            bc.id.to_any(),
        ],
        fx.clock.now(),
    );
    // ... then `node-manager set` lands before the cascade applies.
    make_node_manager(&fx, &foreman.id);
    fx.w.admit(tab_req).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Active);
    assert_eq!(clone_by_id(&fx, &fc.id).lifecycle, CloneLifecycle::Active);
    for a in actions(&fx, ActionKind::ClosureCascade) {
        assert!(!a.retired.contains(&foreman.id.to_any()), "{:?}", a.retired);
        assert!(!a.retired.contains(&fc.id.to_any()), "{:?}", a.retired);
    }

    fx.w.admit(ws_req).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Active);
    assert_eq!(clone_by_id(&fx, &fc.id).lifecycle, CloneLifecycle::Active);
    assert_eq!(ts_rec(&fx, "alpha").lifecycle, Lifecycle::Active);
    assert_eq!(seat_by_id(&fx, &bench.id).1.lifecycle, Lifecycle::Retired);
    assert_eq!(clone_by_id(&fx, &bc.id).lifecycle, CloneLifecycle::Retired);
}

/// Classify a tab closure of `seat_name` by hand at the current head, as the observer does before its awaited
/// snapshot returns; the result is applied later by `apply_classified`.
async fn classify_tab_closure(fx: &Fx, seat_name: &str) -> super::classify::Classified {
    let desired = crate::reconcile::desired::DesiredRuntime::load(&view(fx), &fx.root).unwrap();
    let tab = tab_of_seat(fx, seat_name);
    let before = snapshot(fx).await;
    fx.herdr.user_close_tab(&tab);
    let after = snapshot(fx).await;
    let (mb, ma) = (
        super::matcher::match_snapshot(&before, &desired),
        super::matcher::match_snapshot(&after, &desired),
    );
    super::classify::classify(
        &super::classify::ClassifyCx {
            desired: &desired,
            journal: &fx.journal,
            preds: &[],
            matches: &ma,
            baseline: Some(&mb),
        },
        vec![super::diff::DiffElement::Disappeared {
            object: seat(fx, seat_name).id.to_any(),
            container: ContainerKind::Tab,
        }],
    )
}

fn apply_classified(fx: &Fx, out: &super::classify::Classified) {
    for cas in &out.cascades {
        fx.w.admit(cascade_request_with(
            cas.rule,
            &cas.retire,
            &cas.survived,
            fx.clock.now(),
        ))
        .unwrap();
    }
    for obj in &out.gone {
        fx.w.admit(availability_request(obj, Availability::Absent))
            .unwrap();
    }
    fx.w.drain().unwrap();
}

#[tokio::test]
async fn node_manager_closure_after_concurrent_move_retires_old_seat() {
    // Direction 1: foreman is node manager at classification; `node-manager set bench` lands before apply.
    let fx = fx();
    activate(&fx, "shell");
    commit(
        &fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    let (foreman, bench) = (seat(&fx, "foreman"), seat(&fx, "bench"));
    let fc = only_clone(&fx, "foreman");
    make_node_manager(&fx, &foreman.id);
    let out = classify_tab_closure(&fx, "foreman").await;
    make_node_manager(&fx, &bench.id);
    apply_classified(&fx, &out);
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Retired);
    assert_eq!(clone_by_id(&fx, &fc.id).lifecycle, CloneLifecycle::Retired);
    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1);
    assert!(acts[0].retired.contains(&foreman.id.to_any()));
    assert_eq!(seat(&fx, "bench").lifecycle, Lifecycle::Active);
}

#[tokio::test]
async fn node_manager_closure_classified_before_set_is_recorded_absent() {
    // Direction 2: foreman is ordinary at classification, then becomes node manager before apply.
    let fx = fx();
    activate(&fx, "shell");
    settle(&fx).await;
    let foreman = seat(&fx, "foreman");
    let fc = only_clone(&fx, "foreman");
    let out = classify_tab_closure(&fx, "foreman").await;
    make_node_manager(&fx, &foreman.id);
    apply_classified(&fx, &out);
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    let s = seat(&fx, "foreman");
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert_eq!(s.runtime.availability, Availability::Absent);
    assert!(s.runtime.bound.is_none());
    let c = clone_by_id(&fx, &fc.id);
    assert_eq!(c.lifecycle, CloneLifecycle::Active);
    assert_eq!(c.runtime.availability, Availability::Absent);
    assert!(c.occupant.is_none(), "the session ended with the pane");
    settle(&fx).await;
    assert_reopened(&fx, "foreman").await;
}

#[tokio::test]
async fn graph_issued_close_not_double_retired() {
    let fx = fx();
    activate(&fx, "claude");
    commit(&fx, "clone add foreman --name second");
    settle(&fx).await;
    let s = seat(&fx, "foreman");
    let victim = clones_of(&fx, &s.id).remove(0);
    commit(&fx, &format!("clone retire {}", victim.id));
    step(&fx).await; // the reconciler closes the pane
    assert!(
        calls(&fx)
            .iter()
            .any(|c| matches!(c, FakeCall::ClosePane(_)))
    );
    step(&fx).await; // the observer sees the disappearance
    step(&fx).await;

    assert_eq!(
        actions(&fx, ActionKind::ClosureCascade).len(),
        0,
        "explained by intent: no second retirement"
    );
    assert_eq!(actions(&fx, ActionKind::Retire).len(), 1);
    let v = clone_by_id(&fx, &victim.id);
    assert_eq!(v.lifecycle, CloneLifecycle::Retired);
    assert!(!matches!(
        v.retired.as_ref().unwrap().mechanism,
        RetireMechanism::ObservedPaneClose
            | RetireMechanism::ObservedTabClose
            | RetireMechanism::ObservedWorkspaceClose
    ));
    assert_eq!(
        (v.runtime.availability, v.runtime.bound),
        (Availability::Absent, None),
        "bookkeeping only"
    );
}

#[tokio::test]
async fn retire_last_clone_induced_tab_close_explained() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    drop_default_tab(&fx).await;
    step(&fx).await;
    let c = only_clone(&fx, "foreman");
    commit(&fx, &format!("clone retire {}", c.id));
    step(&fx).await; // closes the pane; Herdr takes the tab and the workspace with it
    assert!(snapshot(&fx).await.workspaces.is_empty());
    step(&fx).await;
    step(&fx).await;

    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 0);
    let ts = ts_rec(&fx, "alpha");
    assert_eq!(
        ts.lifecycle,
        Lifecycle::Active,
        "the induced workspace close is predicted, not a user retirement"
    );
    assert_eq!(ts.runtime.availability, Availability::Absent);
    assert_eq!(
        seat(&fx, "foreman").lifecycle,
        Lifecycle::Retired,
        "the plan's induced seat retirement"
    );
    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
}

#[tokio::test]
async fn deactivate_produces_no_retirement() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    commit(&fx, "seat deactivate foreman");
    step(&fx).await;
    step(&fx).await;
    step(&fx).await;

    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    let s = seat(&fx, "foreman");
    assert_eq!(s.lifecycle, Lifecycle::Dormant);
    assert_eq!(
        (s.runtime.availability, s.runtime.bound.is_none()),
        (Availability::Absent, true)
    );
    let c = only_clone(&fx, "foreman");
    assert_eq!(c.lifecycle, CloneLifecycle::Active, "clones stay active");
    assert_eq!(
        (c.runtime.availability, c.runtime.bound.is_none()),
        (Availability::Absent, true)
    );

    // Reactivation recreates the tab: the dormant interlude left nothing behind.
    commit(&fx, "seat activate foreman");
    fx.herdr.clear_calls();
    settle(&fx).await;
    assert_eq!(
        calls(&fx)
            .iter()
            .filter(|c| matches!(c, FakeCall::CreateTab(_)))
            .count(),
        1
    );
    assert_eq!(
        seat(&fx, "foreman").runtime.availability,
        Availability::Present
    );
}

// =================================================================================================
// disconnects, restarts, rebind
// =================================================================================================

#[tokio::test]
async fn disconnect_marks_unknown_never_retires() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    fx.herdr.disconnect_subscribers();
    fx.herdr.fail_next("session.snapshot", Fault::Unavailable);
    let sum = step(&fx).await;
    assert_eq!(sum.mode, Some(StepMode::Disconnected));
    assert!(sum.reconcile.is_none());

    let (s, c, ts) = (
        seat(&fx, "foreman"),
        only_clone(&fx, "foreman"),
        ts_rec(&fx, "alpha"),
    );
    for rt in [&s.runtime, &c.runtime, &ts.runtime] {
        assert_eq!(rt.availability, Availability::Unknown);
        assert!(
            rt.bound.is_some(),
            "the binding is kept so the rebind can match it"
        );
    }
    assert_eq!(
        (s.lifecycle, c.lifecycle, ts.lifecycle),
        (Lifecycle::Active, CloneLifecycle::Active, Lifecycle::Active)
    );
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());

    // Still down: no more writes, no retirement.
    fx.herdr.fail_next("session.snapshot", Fault::Timeout);
    let again = step(&fx).await;
    assert!(again.observed.is_empty());

    // Back up: a rebind pass restores presence; nothing was closed, so nothing is retired or created.
    fx.herdr.clear_calls();
    let up = step(&fx).await;
    assert_eq!(up.mode, Some(StepMode::Rebind));
    assert_eq!(
        seat(&fx, "foreman").runtime.availability,
        Availability::Present
    );
    assert_eq!(
        only_clone(&fx, "foreman").runtime.availability,
        Availability::Present
    );
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    assert_eq!(creates(&fx), 0);
}

#[tokio::test]
async fn herdr_restart_no_mass_retirement() {
    let fx = fx();
    activate(&fx, "claude");
    commit(&fx, "clone add foreman --name second");
    settle(&fx).await;
    let before = only_clone_pair(&fx);
    fx.herdr.clear_calls();
    fx.herdr.restart(true, true);

    let sum = step(&fx).await;
    assert_eq!(sum.mode, Some(StepMode::Rebind));
    settle(&fx).await;
    assert!(
        actions(&fx, ActionKind::ClosureCascade).is_empty(),
        "a restart is not a closure"
    );
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
    let snap = snapshot(&fx).await;
    for c in &before {
        let now = clone_by_id(&fx, &c.id);
        assert_eq!(now.lifecycle, CloneLifecycle::Active);
        let b = now.runtime.bound.expect("rebound");
        assert_eq!(
            b.incarnation, snap.incarnation,
            "binding follows the new incarnation"
        );
        assert_ne!(
            b.pane_id,
            c.runtime.bound.as_ref().unwrap().pane_id,
            "new pane id recorded"
        );
        assert_eq!(now.runtime.availability, Availability::Present);
    }
    assert_eq!(
        seat(&fx, "foreman").runtime.bound.unwrap().incarnation,
        snap.incarnation
    );
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Active);
}

fn only_clone_pair(fx: &Fx) -> Vec<CloneRecord> {
    clones_of(fx, &seat(fx, "foreman").id)
}

#[tokio::test]
async fn restart_without_tokens_matches_terminal_id_with_cwd() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    fx.herdr.clear_calls();
    fx.herdr.restart(false, true);

    let sum = step(&fx).await;
    assert_eq!(sum.mode, Some(StepMode::Rebind));
    settle(&fx).await;
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
    let now = clone_by_id(&fx, &c.id);
    let snap = snapshot(&fx).await;
    assert_eq!(now.runtime.availability, Availability::Present);
    assert_eq!(
        now.runtime.bound.as_ref().unwrap().incarnation,
        snap.incarnation
    );
    // The reconciler re-stamps what the restart dropped.
    let stamped = snap.workspaces[0]
        .tabs
        .iter()
        .flat_map(|t| &t.panes)
        .any(|p| {
            p.metadata
                .get("hg")
                .is_some_and(|v| *v == format!("hg={}", c.id))
        });
    assert!(stamped, "pane token restored");
    assert!(
        snap.workspaces[0].metadata.contains_key("hg"),
        "workspace token restored"
    );
}

/// D2 (hg-zmi.50): after a Herdr restart dropped tokens and terminal ids, `clone rebind` binds the clone to the
/// pane and the reconciler re-stamps the token with a fresh effect identity (spec 4.2).
#[tokio::test]
async fn rebind_after_restart_restamps_token() {
    let fx = fx();
    activate(&fx, "shell");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let token = crate::model::launch::graph_token(&c.id.to_any());
    fx.herdr.restart(false, false);
    step(&fx).await;
    step(&fx).await;
    assert_eq!(
        clone_by_id(&fx, &c.id).runtime.availability,
        Availability::Unknown
    );
    let snap = snapshot(&fx).await;
    let new_pane = snap
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .flat_map(|t| &t.panes)
        .next()
        .expect("pane survives restart")
        .id
        .clone();
    assert!(
        snap.workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .flat_map(|t| &t.panes)
            .all(|p| !p.metadata.contains_key("hg") || p.id != new_pane)
    );
    fx.herdr.clear_calls();

    commit(&fx, &format!("clone rebind {} --pane {}", c.id, new_pane.0));
    step(&fx).await;
    step(&fx).await;

    assert!(
        calls(&fx).contains(&FakeCall::ReportPaneMetadata(
            new_pane.clone(),
            "hg".into(),
            token.clone()
        )),
        "{:?}",
        calls(&fx)
    );
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
    let snap = snapshot(&fx).await;
    let pane = snap
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .flat_map(|t| &t.panes)
        .find(|p| p.id == new_pane)
        .unwrap();
    assert_eq!(pane.metadata.get("hg"), Some(&token));
    let now = clone_by_id(&fx, &c.id);
    assert_eq!(
        now.runtime.bound.as_ref().unwrap().incarnation,
        snap.incarnation
    );
}

/// hg-zmi.76: `clone rebind` committed after the disconnect marked the clone unknown but before the first
/// snapshot of the restarted server is observed: that first (rebind) pass must keep the explicit adoption,
/// not mark the clone unknown again because the pane has no token yet and its terminal id is new.
#[tokio::test]
async fn rebind_before_the_first_post_restart_pass_survives_it() {
    let fx = fx();
    activate(&fx, "shell");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let token = crate::model::launch::graph_token(&c.id.to_any());
    fx.herdr.restart(false, false);
    let snap = snapshot(&fx).await;
    let new_pane = snap
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .flat_map(|t| &t.panes)
        .next()
        .expect("pane survives restart")
        .id
        .clone();
    fx.herdr.clear_calls();

    commit(&fx, &format!("clone rebind {} --pane {}", c.id, new_pane.0));
    let sum = step(&fx).await;
    assert_eq!(sum.mode, Some(StepMode::Rebind));
    step(&fx).await;

    let now = clone_by_id(&fx, &c.id);
    assert_eq!(
        now.runtime.availability,
        Availability::Present,
        "the explicit rebind is not overwritten by the first pass"
    );
    assert_eq!(
        now.runtime.bound.as_ref().and_then(|b| b.pane_id.clone()),
        Some(new_pane.clone())
    );
    assert!(
        calls(&fx).contains(&FakeCall::ReportPaneMetadata(new_pane, "hg".into(), token)),
        "{:?}",
        calls(&fx)
    );
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
}

#[tokio::test]
async fn restart_without_tokens_or_terminal_ids_is_unknown_not_recreated() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    fx.herdr.clear_calls();
    fx.herdr.restart(false, false);

    step(&fx).await;
    step(&fx).await;
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
    assert_eq!(
        creates(&fx),
        0,
        "no create effects for unknown objects: {:?}",
        calls(&fx)
    );
    let now = clone_by_id(&fx, &c.id);
    assert_eq!(now.lifecycle, CloneLifecycle::Active);
    assert_eq!(now.runtime.availability, Availability::Unknown);
}

#[tokio::test]
async fn close_while_daemon_down_is_unknown_not_recreated() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let before = baseline_bytes(&fx);
    assert!(!before.is_empty(), "baseline persisted");
    // The user closes the pane while no daemon runs.
    fx.herdr.user_close_pane(&pane_of(&fx, &c));
    let lp2 = fx.restarted_loop();
    fx.herdr.clear_calls();

    let sum = lp2.step_once().await.unwrap();
    assert_eq!(
        sum.mode,
        Some(StepMode::Rebind),
        "a fresh daemon never diffs against an old baseline"
    );
    lp2.step_once().await.unwrap();
    assert!(
        actions(&fx, ActionKind::ClosureCascade).is_empty(),
        "never silent retirement"
    );
    assert_eq!(clone_by_id(&fx, &c.id).lifecycle, CloneLifecycle::Active);
    assert_eq!(
        clone_by_id(&fx, &c.id).runtime.availability,
        Availability::Unknown
    );
    assert_eq!(
        seat(&fx, "foreman").runtime.availability,
        Availability::Unknown
    );
    assert_eq!(creates(&fx), 0, "never recreation: {:?}", calls(&fx));
}

fn baseline_bytes(fx: &Fx) -> Vec<u8> {
    std::fs::read(InstancePaths::new(&fx.root).baseline).unwrap_or_default()
}

#[tokio::test]
async fn rename_while_down_recorded_on_rebind() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let tab = tab_of_seat(&fx, "foreman");
    fx.herdr.user_rename_tab(&tab, "boss");
    fx.clock.advance(chrono::Duration::seconds(30));
    let lp2 = fx.restarted_loop();
    fx.herdr.clear_calls();

    let sum = lp2.step_once().await.unwrap();
    assert_eq!(sum.mode, Some(StepMode::Rebind));
    let s = seat(&fx, "boss");
    let ch = s.name_history.last().unwrap();
    assert_eq!(
        (ch.old.as_str(), ch.new.as_str(), ch.source),
        ("foreman", "boss", NameSource::Observed)
    );
    assert_eq!(ch.observed_at, t0() + chrono::Duration::seconds(30));
    assert_eq!(ch.event_at, None);
    assert!(
        !calls(&fx)
            .iter()
            .any(|c| matches!(c, FakeCall::RenameTab(..))),
        "recorded, not reverted: {:?}",
        calls(&fx)
    );
}

#[tokio::test]
async fn herdr_resumed_agent_adopted_as_occupancy_start() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    assert!(
        c.occupant.is_none(),
        "an agent without a session id is not recorded yet"
    );
    fx.herdr.restart(true, true);
    let snap = snapshot(&fx).await;
    let pane = snap
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .flat_map(|t| &t.panes)
        .find(|p| p.metadata.contains_key("hg"))
        .unwrap()
        .id
        .clone();
    fx.herdr
        .set_agent(&pane, claude_agent(Some("sess-resumed")));
    fx.clock.advance(chrono::Duration::seconds(5));

    let sum = step(&fx).await;
    assert_eq!(sum.mode, Some(StepMode::Rebind));
    let now = only_clone(&fx, "foreman");
    let occ = now.occupant.expect("adopted as the occupant");
    assert_eq!(occ.harness, Harness::Claude);
    assert_eq!(occ.since, t0() + chrono::Duration::seconds(5));
    let ns = now
        .sessions
        .iter()
        .find(|s| s.id == occ.native_session)
        .unwrap();
    assert_eq!(ns.native_session_id, "sess-resumed");
    assert!(ns.ended.is_none());
}

// =================================================================================================
// renames
// =================================================================================================

#[tokio::test]
async fn rename_tracked_with_observed_at_only() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let tab = tab_of_seat(&fx, "foreman");
    fx.herdr.user_rename_tab(&tab, "boss");
    fx.clock.advance(chrono::Duration::seconds(7));
    fx.herdr.clear_calls();

    step(&fx).await;
    let s = seat(&fx, "boss");
    assert_eq!(s.name, "boss");
    let ch = s.name_history.last().unwrap();
    assert_eq!(
        (ch.old.as_str(), ch.new.as_str(), ch.source),
        ("foreman", "boss", NameSource::Observed)
    );
    assert_eq!(ch.observed_at, t0() + chrono::Duration::seconds(7));
    assert_eq!(ch.event_at, None, "event time is never invented");
    let (loc, _) = seat_by_id(&fx, &s.id);
    assert!(
        loc.folder.as_str().ends_with("/seats/boss"),
        "folder follows the name: {}",
        loc.folder.as_str()
    );
    step(&fx).await;
    assert!(
        !calls(&fx)
            .iter()
            .any(|c| matches!(c, FakeCall::RenameTab(..))),
        "no rename effect is generated back"
    );
    assert_eq!(seat(&fx, "boss").name_history.len(), 1);
}

#[tokio::test]
async fn dropped_rename_recovered_from_diff() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    // No subscriber exists, so the rename event is never delivered; the next tick's snapshot diff finds it.
    fx.herdr.disconnect_subscribers();
    let ws = workspace_id(&fx).await;
    fx.herdr.user_rename_workspace(&ws, "gamma");
    fx.clock.advance(chrono::Duration::seconds(60));

    step(&fx).await;
    let ts = ts_rec(&fx, "gamma");
    let ch = ts.name_history.last().unwrap();
    assert_eq!((ch.old.as_str(), ch.new.as_str()), ("alpha", "gamma"));
    assert_eq!(ch.observed_at, t0() + chrono::Duration::seconds(60));
    assert_eq!(ch.event_at, None);
}

// =================================================================================================
// moves
// =================================================================================================

#[tokio::test]
async fn move_sets_reload_required() {
    let fx = fx();
    activate(&fx, "claude");
    commit(
        &fx,
        "seat create other --teamspace alpha --active --harness claude",
    );
    commit(&fx, "clone add foreman --name second");
    settle(&fx).await;
    let s = seat(&fx, "foreman");
    let mover = clones_of(&fx, &s.id).remove(0);
    let other_tab = tab_of_seat(&fx, "other");
    fx.herdr.user_move_pane(&pane_of(&fx, &mover), &other_tab);

    step(&fx).await;
    let m = clone_by_id(&fx, &mover.id);
    assert!(m.reload_required);
    assert_eq!(m.lifecycle, CloneLifecycle::Active);
    let b = m.runtime.bound.expect("rebound into the known tab");
    assert_eq!(b.tab_id, Some(other_tab));
    assert_eq!(m.runtime.availability, Availability::Present);
    assert!(
        !seat(&fx, "foreman").moved_out,
        "the seat's tab still holds its other clone"
    );
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());
}

#[tokio::test]
async fn move_emptying_tab_sets_moved_out_and_no_tab_recreated() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let ws = workspace_id(&fx).await;
    let scratch = fx
        .herdr
        .create_tab(CreateTab {
            workspace: ws,
            label: "scratch".into(),
            cwd: "/".into(),
            env: vec![],
        })
        .await
        .unwrap()
        .tab
        .unwrap();
    fx.herdr.user_move_pane(&pane_of(&fx, &c), &scratch);
    fx.herdr.clear_calls();

    step(&fx).await;
    let s = seat(&fx, "foreman");
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert!(s.moved_out);
    assert_eq!(s.runtime.availability, Availability::Absent);
    assert!(
        actions(&fx, ActionKind::ClosureCascade).is_empty(),
        "a move is not a closure"
    );
    let m = clone_by_id(&fx, &c.id);
    assert!(m.reload_required);
    assert_eq!(m.lifecycle, CloneLifecycle::Active);

    step(&fx).await;
    step(&fx).await;
    assert_eq!(
        creates(&fx),
        0,
        "no tab for a seat whose clones live elsewhere: {:?}",
        calls(&fx)
    );
    assert!(
        seat(&fx, "foreman").moved_out,
        "stays moved out until a rebind plan"
    );
}

/// Two active shell seats `a` and `b`, settled, then `a`'s only pane is moved into `b`'s tab.
async fn moved_a_into_b() -> (Fx, SeatId, SeatId, HerdrTabId) {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(
        &fx,
        "seat create a --teamspace alpha --active --harness shell",
    );
    commit(
        &fx,
        "seat create b --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    let a_id = seat(&fx, "a").id;
    let b_id = seat(&fx, "b").id;
    let b_tab = tab_of_seat(&fx, "b");
    let a_pane = pane_of(&fx, &only_clone(&fx, "a"));
    fx.herdr.user_move_pane(&a_pane, &b_tab);
    fx.herdr.clear_calls();
    for _ in 0..3 {
        step(&fx).await;
    }
    (fx, a_id, b_id, b_tab)
}

#[tokio::test]
async fn moved_out_seat_does_not_rename_destination_tab() {
    let (fx, a_id, b_id, b_tab) = moved_a_into_b().await;
    assert!(
        seat_by_id(&fx, &a_id).1.moved_out,
        "a's only pane left its tab"
    );
    let renames_of_b: Vec<_> = calls(&fx)
        .into_iter()
        .filter(|c| matches!(c, FakeCall::RenameTab(t, _) if *t == b_tab))
        .collect();
    assert!(
        renames_of_b.is_empty(),
        "b's tab must never be renamed: {renames_of_b:?}"
    );
    let b = seat_by_id(&fx, &b_id).1;
    assert_eq!(b.name, "b");
    assert!(
        b.name_history.is_empty(),
        "no rename observed against b: {:?}",
        b.name_history
    );
    let snap = snapshot(&fx).await;
    let live_b = snap
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .find(|t| t.id == b_tab)
        .expect("b's tab");
    assert_eq!(live_b.label, "b");
    let named_a = layout::all_seats(&view(&fx))
        .unwrap()
        .into_iter()
        .filter(|(_, s)| s.name == "a")
        .count();
    assert_eq!(named_a, 1, "exactly one seat is named a");
}

#[tokio::test]
async fn moved_out_seat_gets_no_tab_found_by_token() {
    let (fx, a_id, _b_id, _b_tab) = moved_a_into_b().await;
    let a = seat_by_id(&fx, &a_id).1;
    assert!(a.moved_out);
    let snap = snapshot(&fx).await;
    let desired = crate::reconcile::desired::DesiredRuntime::load(&view(&fx), &fx.root).unwrap();
    let idx = crate::reconcile::planner::LiveIndex::new(&snap, &desired, &fx.journal);
    assert!(
        idx.tab_for_seat(&a.id).is_none(),
        "token fallback must be skipped for a moved_out seat"
    );
}

#[tokio::test]
async fn move_into_unknown_tab_unbinds() {
    let fx = fx();
    activate(&fx, "claude");
    commit(&fx, "clone add foreman --name second");
    settle(&fx).await;
    let s = seat(&fx, "foreman");
    let mut cs = clones_of(&fx, &s.id);
    let mover = cs.remove(0);
    let stay = cs.remove(0);
    let ws = workspace_id(&fx).await;
    let scratch = fx
        .herdr
        .create_tab(CreateTab {
            workspace: ws,
            label: "scratch".into(),
            cwd: "/".into(),
            env: vec![],
        })
        .await
        .unwrap()
        .tab
        .unwrap();
    fx.herdr.user_move_pane(&pane_of(&fx, &mover), &scratch);
    fx.herdr.clear_calls();

    step(&fx).await;
    let m = clone_by_id(&fx, &mover.id);
    assert!(m.reload_required);
    assert!(m.runtime.bound.is_none(), "unbound");
    assert_eq!(m.runtime.availability, Availability::Unknown);
    assert_eq!(m.lifecycle, CloneLifecycle::Active);
    let st = clone_by_id(&fx, &stay.id);
    assert!(!st.reload_required);
    assert_eq!(st.runtime.availability, Availability::Present);
    let seat_now = seat(&fx, "foreman");
    assert!(!seat_now.moved_out);
    assert_eq!(seat_now.runtime.availability, Availability::Present);

    // It stays unbound on later steps and nothing is created for it.
    step(&fx).await;
    assert!(clone_by_id(&fx, &mover.id).runtime.bound.is_none());
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
}

// =================================================================================================
// event loss, predictions, rejection, baseline
// =================================================================================================

#[tokio::test]
async fn event_loss_recovered_by_snapshot() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    // Events are lost (no subscriber); the periodic snapshot still finds the closure.
    fx.herdr.disconnect_subscribers();
    fx.herdr.user_close_pane(&pane_of(&fx, &c));
    let sum = step(&fx).await;
    assert_eq!(sum.mode, Some(StepMode::Diff));
    assert_eq!(clone_by_id(&fx, &c.id).lifecycle, CloneLifecycle::Retired);
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);
}

#[tokio::test]
async fn run_loop_reacts_to_events_and_ticks() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    fx.clock.advance(chrono::Duration::seconds(1));
    let started = fx.clock.now();
    let lp = RuntimeLoop::new(
        fx.herdr.clone(),
        fx.store.clone(),
        fx.dw.clone(),
        fx.journal.clone(),
        fx.rec.clone(),
        fx.clock.clone(),
        InstancePaths::new(&fx.root),
        Duration::from_millis(40),
        Some(fx.w.subscribe()),
        fx.root.join("claude"),
    );
    lp.set_tuning(LoopTuning {
        commit_timeout: Duration::from_secs(5),
        settle_timeout: Duration::from_millis(50),
        debounce: Duration::from_millis(5),
    });
    let (tx, rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(lp.clone().run(rx));
    // First iteration: subscribe, rebind pass, a fresh baseline stamped with the advanced clock.
    for _ in 0..200 {
        if lp.baseline().is_some_and(|b| b.taken_at == started) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        lp.baseline().map(|b| b.taken_at),
        Some(started),
        "the loop took its first snapshot"
    );
    fx.herdr.user_close_pane(&pane_of(&fx, &c));
    let mut retired = false;
    for _ in 0..300 {
        if clone_by_id(&fx, &c.id).lifecycle == CloneLifecycle::Retired {
            retired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tx.send(true).unwrap();
    task.await.unwrap();
    assert!(retired, "the running loop retired the closed clone");
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);
}

fn done_effect(object: &AnyId, container: ContainerKind, also: Vec<PredictedEnd>) -> EffectRecord {
    let op = OpId::new();
    let kind = EffectKind::ClosePane;
    EffectRecord {
        id: EffectId::derive(&op, object, kind.as_str(), 1),
        op,
        object: object.clone(),
        kind,
        object_rev: 1,
        fencing_rev: 1,
        status: EffectStatus::Done,
        predicted: [PredictedEnd {
            object: object.clone(),
            container,
            end: EndState::Closed,
            induced: false,
        }]
        .into_iter()
        .chain(also)
        .collect(),
        nonce_label: None,
        attempts: 1,
        last_error: None,
        updated_at: t0(),
        sched: Default::default(),
    }
}

#[tokio::test]
async fn prediction_single_use_and_discarded_if_container_remains() {
    let fx = fx();
    activate(&fx, "claude");
    commit(&fx, "clone add foreman --name second");
    settle(&fx).await;
    let s = seat(&fx, "foreman");
    let mut cs = clones_of(&fx, &s.id);
    let (a, b) = (cs.remove(0), cs.remove(0));

    // `a`: the effect is done but Herdr never closed the pane. The next complete snapshot still shows it, so
    // the prediction is discarded.
    let ea = done_effect(&a.id.to_any(), ContainerKind::Pane, vec![]);
    fx.journal.upsert_effect(&ea).unwrap();
    assert!(predictions(&fx.journal).iter().any(|(e, _)| *e == ea.id));
    step(&fx).await;
    assert!(
        !predictions(&fx.journal).iter().any(|(e, _)| *e == ea.id),
        "discarded: the container remained"
    );
    // So a later, real user closure of that pane is not absorbed by the stale prediction.
    fx.herdr.user_close_pane(&pane_of(&fx, &a));
    step(&fx).await;
    assert_eq!(clone_by_id(&fx, &a.id).lifecycle, CloneLifecycle::Retired);
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);

    // `b`: the effect closed the pane (as predicted): explained, no retirement, and the prediction is spent.
    // Closing the seat's last pane takes the tab with it, which the effect predicted as induced.
    let tab = PredictedEnd {
        object: s.id.to_any(),
        container: ContainerKind::Tab,
        end: EndState::Closed,
        induced: true,
    };
    let eb = done_effect(&b.id.to_any(), ContainerKind::Pane, vec![tab]);
    fx.journal.upsert_effect(&eb).unwrap();
    fx.herdr.user_close_pane(&pane_of(&fx, &b));
    step(&fx).await;
    assert_eq!(
        clone_by_id(&fx, &b.id).lifecycle,
        CloneLifecycle::Active,
        "explained by the predicted end state"
    );
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);
    assert!(
        !predictions(&fx.journal).iter().any(|(e, _)| *e == eb.id),
        "single use"
    );
}

#[tokio::test]
async fn observed_rejection_is_rederived() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let s = seat(&fx, "foreman");
    let c = only_clone(&fx, "foreman");
    fx.herdr.user_close_pane(&pane_of(&fx, &c));
    fx.lp.set_tuning(LoopTuning {
        settle_timeout: Duration::ZERO,
        ..LoopTuning::default()
    });

    // A confirmed `seat retire` is queued ahead of the observer's cascade: both target the same seat.
    let sp = plan(&fx, "seat retire foreman");
    admit_apply(
        &fx.deps,
        &CallerInfo::default(),
        sp.plan.id.as_str(),
        Some(&sp.hash),
        "relay",
    )
    .unwrap();
    step(&fx).await;

    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1, "the cascade still ran");
    assert!(
        acts[0].retired.is_empty(),
        "nothing left to retire: {:?}",
        acts[0].retired
    );
    let already: std::collections::BTreeSet<_> = acts[0].already_retired.iter().cloned().collect();
    assert_eq!(
        already,
        [s.id.to_any(), c.id.to_any()].into_iter().collect()
    );
    let (_, s2) = seat_by_id(&fx, &s.id);
    assert_eq!(s2.lifecycle, Lifecycle::Retired);
    assert!(
        !matches!(
            s2.retired.unwrap().mechanism,
            RetireMechanism::ObservedTabClose
        ),
        "the confirmed retirement keeps its provenance"
    );
}

#[tokio::test]
async fn baseline_advances_only_after_commit() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let pane = pane_of(&fx, &c);
    let before = baseline_bytes(&fx);
    fx.herdr.user_close_pane(&pane);
    fx.lp.set_tuning(LoopTuning {
        commit_timeout: Duration::from_millis(100),
        settle_timeout: Duration::ZERO,
        ..LoopTuning::default()
    });

    // The writer holds the op: it is admitted but never commits.
    fx.dw.hold.store(true, Ordering::SeqCst);
    let sum = step(&fx).await;
    assert!(!sum.baseline_advanced);
    assert!(
        sum.reconcile.is_none(),
        "the reconciler never acts on uncommitted observations"
    );
    assert_eq!(baseline_bytes(&fx), before, "baseline unchanged on disk");
    assert!(
        fx.lp
            .baseline()
            .unwrap()
            .snapshot
            .workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .flat_map(|t| &t.panes)
            .any(|p| p.id == pane)
    );
    assert_eq!(clone_by_id(&fx, &c.id).lifecycle, CloneLifecycle::Active);

    // The writer catches up; the next step re-derives against the same baseline and advances it.
    fx.dw.hold.store(false, Ordering::SeqCst);
    fx.w.drain().unwrap();
    let sum = step(&fx).await;
    assert!(sum.baseline_advanced);
    assert_ne!(baseline_bytes(&fx), before);
    let now = fx.lp.baseline().unwrap();
    assert!(
        !now.snapshot
            .workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .flat_map(|t| &t.panes)
            .any(|p| p.id == pane)
    );
    assert_eq!(clone_by_id(&fx, &c.id).lifecycle, CloneLifecycle::Retired);
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);
}

// =================================================================================================
// session-ended hook
// =================================================================================================

#[tokio::test]
async fn session_ended_hook_emitted_after_commit() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let pane = pane_of(&fx, &c);
    // A Claude transcript exists for the session under the recorded cwd.
    let claude_root = fx.root.join("claude-root");
    let cwd = fx.root.join(".graph-local/cwd").join(c.seat.as_str());
    let transcript = claude_root
        .join("projects")
        .join(claude_project_slug(&cwd))
        .join("sess-1.jsonl");
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    std::fs::write(&transcript, "{}\n").unwrap();
    fx.lp.set_claude_root(claude_root);
    let mut rx = fx.lp.subscribe_session_ended();

    fx.herdr.set_agent(&pane, claude_agent(Some("sess-1")));
    step(&fx).await;
    let started = only_clone(&fx, "foreman");
    let occ = started.occupant.clone().expect("occupancy started");
    let ns = started
        .sessions
        .iter()
        .find(|s| s.id == occ.native_session)
        .unwrap();
    assert_eq!(ns.native_session_id, "sess-1");
    assert_eq!(ns.transcript_path.as_deref(), Some(transcript.as_path()));
    assert!(rx.try_recv().is_err(), "nothing ended yet");

    fx.clock.advance(chrono::Duration::seconds(9));
    fx.herdr.set_agent(&pane, None);
    step(&fx).await;
    let ev = rx.try_recv().expect("session-ended hook fired");
    assert_eq!(
        (ev.clone.clone(), ev.seat.clone(), ev.ns.clone()),
        (c.id.clone(), c.seat.clone(), occ.native_session.clone())
    );
    assert_eq!(
        (ev.harness, ev.reason),
        (Harness::Claude, SessionEndReason::AgentExited)
    );
    assert_eq!(ev.transcript_path.as_deref(), Some(transcript.as_path()));
    // Emitted after the committing op finished, and the record shows the end.
    assert_eq!(
        fx.journal.get(&ev.op).unwrap().unwrap().state,
        OpState::Committed
    );
    let ended = only_clone(&fx, "foreman");
    assert!(ended.occupant.is_none());
    let ns = ended.sessions.iter().find(|s| s.id == ev.ns).unwrap();
    assert_eq!(ns.ended, Some(t0() + chrono::Duration::seconds(9)));
    assert_eq!(ns.end_reason, Some(SessionEndReason::AgentExited));
    assert!(rx.try_recv().is_err(), "once");
}

#[tokio::test]
async fn closing_a_pane_with_an_occupant_ends_the_session() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let pane = pane_of(&fx, &c);
    fx.herdr.set_agent(&pane, claude_agent(Some("sess-2")));
    step(&fx).await;
    let mut rx = fx.lp.subscribe_session_ended();
    fx.herdr.user_close_pane(&pane);
    step(&fx).await;
    let ev = rx.try_recv().expect("cascade ended the session");
    assert_eq!(ev.reason, SessionEndReason::PaneClosed);
    let r = clone_by_id(&fx, &c.id);
    assert!(r.occupant.is_none());
    assert_eq!(r.sessions[0].end_reason, Some(SessionEndReason::PaneClosed));
}

#[tokio::test]
async fn observation_proceeds_while_replacement_waits() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    commit(
        &fx,
        "seat create other --teamspace alpha --active --harness claude",
    );
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let pane = pane_of(&fx, &c);
    fx.herdr.set_agent(
        &pane,
        Some(AgentInfo {
            kind: "claude".into(),
            status: AgentStatus::Working,
            session: Some(AgentSession::Id("sess-1".into())),
        }),
    );
    step(&fx).await;
    assert!(
        clone_by_id(&fx, &c.id).occupant.is_some(),
        "the occupant is observed"
    );

    // The model change plans a replacement that has to wait for the working occupant.
    fx.herdr.clear_calls();
    commit(&fx, "seat override foreman --model fancy");
    tokio::time::timeout(Duration::from_secs(2), step(&fx))
        .await
        .expect("the step must not wait for the occupant");
    let waiting: Vec<_> = fx
        .journal
        .effects_with_status(&[EffectStatus::Pending])
        .unwrap()
        .into_iter()
        .filter(|r| r.kind == EffectKind::ReplaceSession)
        .collect();
    assert_eq!(waiting.len(), 1, "the replacement is waiting");

    // A user rename on the other seat is observed and committed by the next step.
    let tab = tab_of_seat(&fx, "other");
    fx.herdr.user_rename_tab(&tab, "renamed");
    tokio::time::timeout(Duration::from_secs(2), step(&fx))
        .await
        .expect("the step must not wait for the occupant");
    assert_eq!(seat(&fx, "renamed").name, "renamed");
    assert_eq!(
        calls(&fx)
            .iter()
            .filter(|c| matches!(c, FakeCall::SendKeys(..) | FakeCall::StartAgent(_)))
            .count(),
        0,
        "the working occupant was never touched"
    );
}

fn replace_rows(fx: &Fx) -> Vec<EffectRecord> {
    use EffectStatus::*;
    fx.journal
        .effects_with_status(&[
            Pending,
            Done,
            Obsolete,
            Failed,
            Unknown,
            NeedsRevision,
            BlockedNeedsHuman,
        ])
        .unwrap()
        .into_iter()
        .filter(|r| r.kind == EffectKind::ReplaceSession)
        .collect()
}

fn start_calls(fx: &Fx, pane: &HerdrPaneId) -> Vec<Vec<String>> {
    calls(fx)
        .into_iter()
        .filter_map(|c| match c {
            FakeCall::StartAgent(s) if &s.pane == pane => Some(s.args),
            _ => None,
        })
        .collect()
}

fn shell_process() -> ProcessInfo {
    ProcessInfo {
        foreground_pid: None,
        foreground_argv: vec!["zsh".into()],
        is_shell: true,
    }
}

fn claude_process() -> ProcessInfo {
    ProcessInfo {
        foreground_pid: Some(7),
        foreground_argv: vec!["claude".into()],
        is_shell: false,
    }
}

/// A claude seat whose clone holds an observed idle occupant `sess-1`, then a model override. Returns the
/// clone and its pane, with the override committed but not yet reconciled.
async fn observed_occupant(fx: &Fx) -> (CloneRecord, HerdrPaneId) {
    activate(fx, "claude");
    settle(fx).await;
    let c = only_clone(fx, "foreman");
    let pane = pane_of(fx, &c);
    fx.herdr.set_agent(&pane, claude_agent(Some("sess-1")));
    fx.herdr.set_process(&pane, claude_process());
    step(fx).await;
    assert!(
        clone_by_id(fx, &c.id).occupant.is_some(),
        "the occupant is observed"
    );
    commit(fx, "seat override foreman --model fancy");
    fx.herdr.clear_calls();
    (c, pane)
}

#[tokio::test]
async fn replacement_survives_observed_occupancy_end_and_resumes() {
    let fx = fx();
    let (c, pane) = observed_occupant(&fx).await;
    step(&fx).await;
    let sent = calls(&fx)
        .iter()
        .filter(|x| matches!(x, FakeCall::SendKeys(..)))
        .count();
    assert!(sent >= 2, "the exit keys were sent: {:?}", calls(&fx));
    assert!(start_calls(&fx, &pane).is_empty());
    assert_eq!(replace_rows(&fx).remove(0).status, EffectStatus::Pending);

    // The old agent exits; the next step's observer commits the occupancy end before the reconciler runs.
    fx.herdr.set_agent(&pane, None);
    fx.herdr.set_process(&pane, shell_process());
    step(&fx).await;
    let after = clone_by_id(&fx, &c.id);
    assert!(after.occupant.is_none());
    let ns = after
        .sessions
        .iter()
        .find(|n| n.native_session_id == "sess-1")
        .unwrap();
    assert!(ns.ended.is_some(), "the observer ended the session");
    assert_eq!(
        start_calls(&fx, &pane),
        vec![vec!["--resume", "sess-1", "--model", "fancy"]]
    );
    let ef = replace_rows(&fx).remove(0);
    assert_eq!(ef.status, EffectStatus::Done, "{ef:?}");
    let launched: serde_json::Value = serde_json::from_str(
        &fx.journal
            .meta_get(&crate::reconcile::planner::launched_key(&c.id))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(launched["model"], "fancy");
    assert_eq!(
        fx.journal.meta_get(&format!("replace:{}", ef.id)).unwrap(),
        None
    );

    step(&fx).await;
    assert_eq!(
        start_calls(&fx, &pane).len(),
        1,
        "the replacement is not started twice"
    );
}

#[tokio::test]
async fn run_loop_wakes_for_deferred_effect() {
    let fx = fx();
    let (_c, pane) = observed_occupant(&fx).await;

    let (tx, rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(fx.lp.clone().run(rx));
    let mut keys = false;
    for _ in 0..300 {
        if calls(&fx)
            .iter()
            .any(|x| matches!(x, FakeCall::SendKeys(..)))
        {
            keys = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(keys, "the loop sent the exit keys");
    // `set_process` emits no Herdr event: only the deferred wake can trigger the next step (the tick is 60 s).
    fx.herdr.set_process(&pane, shell_process());
    let mut started = false;
    for _ in 0..300 {
        if !start_calls(&fx, &pane).is_empty() {
            started = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tx.send(true).unwrap();
    task.await.unwrap();
    assert!(
        started,
        "the loop woke for the deferred replacement: {:?}",
        calls(&fx)
    );
}

// =================================================================================================
// session capture (per harness)
// =================================================================================================

fn pane_with(session: Option<AgentSession>, kind: &str) -> PaneInfo {
    PaneInfo {
        id: HerdrPaneId("p1".into()),
        terminal_id: None,
        label: None,
        cwd: None,
        metadata: BTreeMap::new(),
        agent: Some(AgentInfo {
            kind: kind.into(),
            status: AgentStatus::Idle,
            session,
        }),
    }
}

#[test]
fn capture_claude_agent_session_id_and_transcript_path() {
    let root = tempfile::tempdir().unwrap();
    let cwd = Path::new("/work/my proj");
    let expected = root
        .path()
        .join("projects")
        .join(claude_project_slug(cwd))
        .join("abc-123.jsonl");
    std::fs::create_dir_all(expected.parent().unwrap()).unwrap();
    std::fs::write(&expected, "").unwrap();
    let pane = pane_with(Some(AgentSession::Id("abc-123".into())), "claude");
    let cap = capture_session(Harness::Claude, &pane, None, root.path(), cwd).unwrap();
    assert_eq!(cap.native_session_id, "abc-123");
    assert_eq!(cap.transcript_path.as_deref(), Some(expected.as_path()));
    assert_eq!((cap.harness, cap.cwd.as_path()), (Harness::Claude, cwd));
}

#[test]
fn capture_claude_path_kind() {
    let root = tempfile::tempdir().unwrap();
    let p = PathBuf::from("/somewhere/else/sess-9.jsonl");
    let pane = pane_with(Some(AgentSession::Path(p.clone())), "claude");
    let cap = capture_session(Harness::Claude, &pane, None, root.path(), Path::new("/w")).unwrap();
    assert_eq!(cap.native_session_id, "sess-9", "id is the file stem");
    assert_eq!(
        cap.transcript_path,
        Some(p),
        "the path kind is the transcript path"
    );
}

#[test]
fn capture_claude_glob_fallback() {
    let root = tempfile::tempdir().unwrap();
    let found = root.path().join("projects/-some-other-cwd/zzz.jsonl");
    std::fs::create_dir_all(found.parent().unwrap()).unwrap();
    std::fs::write(&found, "").unwrap();
    let pane = pane_with(Some(AgentSession::Id("zzz".into())), "claude");
    let cap = capture_session(
        Harness::Claude,
        &pane,
        None,
        root.path(),
        Path::new("/not/the/slug"),
    )
    .unwrap();
    assert_eq!(
        cap.transcript_path,
        Some(found),
        "glob projects/*/<id>.jsonl"
    );
    let missing = pane_with(Some(AgentSession::Id("nope".into())), "claude");
    let cap = capture_session(
        Harness::Claude,
        &missing,
        None,
        root.path(),
        Path::new("/w"),
    )
    .unwrap();
    assert_eq!(
        (cap.native_session_id.as_str(), cap.transcript_path),
        ("nope", None)
    );
}

#[test]
fn capture_codex_agent_session() {
    let root = tempfile::tempdir().unwrap();
    let pane = pane_with(Some(AgentSession::Id("cdx-1".into())), "codex");
    let argv = ProcessInfo {
        foreground_pid: Some(1),
        foreground_argv: vec!["codex".into(), "resume".into(), "other".into()],
        is_shell: false,
    };
    let cap = capture_session(
        Harness::Codex,
        &pane,
        Some(&argv),
        root.path(),
        Path::new("/w"),
    )
    .unwrap();
    assert_eq!(
        cap.native_session_id, "cdx-1",
        "Herdr's agent_session outranks argv"
    );
    assert_eq!(
        cap.transcript_path, None,
        "codex transcripts stay unresolved"
    );
}

#[test]
fn capture_codex_argv_resume_id() {
    let root = tempfile::tempdir().unwrap();
    let pane = pane_with(None, "codex");
    let argv = ProcessInfo {
        foreground_pid: Some(1),
        foreground_argv: ["codex", "--no-daemon", "resume", "r-77", "-m", "x"]
            .map(str::to_owned)
            .to_vec(),
        is_shell: false,
    };
    let cap = capture_session(
        Harness::Codex,
        &pane,
        Some(&argv),
        root.path(),
        Path::new("/w"),
    )
    .unwrap();
    assert_eq!(cap.native_session_id, "r-77");
    let none = ProcessInfo {
        foreground_pid: Some(1),
        foreground_argv: vec!["codex".into()],
        is_shell: false,
    };
    assert!(
        capture_session(
            Harness::Codex,
            &pane,
            Some(&none),
            root.path(),
            Path::new("/w")
        )
        .is_none()
    );
    assert!(capture_session(Harness::Codex, &pane, None, root.path(), Path::new("/w")).is_none());
}

#[test]
fn capture_shell_none() {
    let root = tempfile::tempdir().unwrap();
    let pane = pane_with(Some(AgentSession::Id("whatever".into())), "claude");
    let argv = ProcessInfo {
        foreground_pid: Some(1),
        foreground_argv: vec!["resume".into(), "x".into()],
        is_shell: true,
    };
    assert!(
        capture_session(
            Harness::Shell,
            &pane,
            Some(&argv),
            root.path(),
            Path::new("/w")
        )
        .is_none()
    );
}

#[test]
fn baseline_roundtrip_and_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/baseline.json");
    assert!(baseline::load(&path).is_none());
    let snap = HerdrSnapshot {
        incarnation: crate::model::Incarnation {
            generation: 3,
            server_pid: Some(9),
            server_started: None,
        },
        workspaces: vec![],
    };
    let b = baseline::Baseline::new(snap, t0());
    baseline::save(&path, &b).unwrap();
    assert_eq!(baseline::load(&path), Some(b));
    std::fs::write(&path, "{not json").unwrap();
    assert!(
        baseline::load(&path).is_none(),
        "a corrupt baseline means a rebind pass"
    );
}

// =================================================================================================
// reparenting on observed closure (hg-k4l.26)
// =================================================================================================

/// foreman (node manager), bench (active, its own tab) and a dormant `report`; `report.manager` = bench and
/// `bench.manager` = the node manager, or a seat id that does not exist when `dangling`.
async fn manager_world(fx: &Fx, dangling: bool) {
    use crate::plan::managers::reparent::testing::seat_request;
    activate(fx, "shell");
    commit(
        fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    commit(fx, "seat create report --teamspace alpha");
    settle(fx).await;
    let foreman = seat(fx, "foreman").id;
    make_node_manager(fx, &foreman);
    let bench_manager = if dangling { SeatId::new() } else { foreman };
    for req in [
        seat_request(&seat(fx, "bench").id, Some(&bench_manager)),
        seat_request(&seat(fx, "report").id, Some(&seat(fx, "bench").id)),
    ] {
        fx.w.admit(req).unwrap();
        fx.w.drain().unwrap();
    }
}

#[tokio::test]
async fn reparent_on_observed_cascade() {
    use crate::plan::managers::reparent::{Rewrite, testing::recorded};
    let fx = fx();
    manager_world(&fx, false).await;
    let foreman = seat(&fx, "foreman").id;
    let bench = seat(&fx, "bench").id;
    fx.herdr.user_close_tab(&tab_of_seat(&fx, "bench"));
    step(&fx).await;

    assert_eq!(seat(&fx, "report").manager, Some(foreman.clone()));
    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1);
    assert_eq!(
        recorded(&acts[0]),
        vec![Rewrite {
            holder: seat(&fx, "report").id.to_any(),
            field: "manager".into(),
            old: bench,
            new: Some(foreman),
        }]
    );
    assert!(
        !acts[0]
            .compensation
            .contains_key(crate::plan::managers::reparent::REPARENT_FALLBACK_KEY),
        "no fallback when the target resolves"
    );
}

#[tokio::test]
async fn reparent_two_hop_on_observed_workspace_close() {
    use crate::plan::managers::reparent::{
        Rewrite,
        testing::{recorded, seat_request},
    };
    let fx = fx();
    activate(&fx, "shell");
    for n in ["a", "b"] {
        commit(
            &fx,
            &format!("seat create {n} --teamspace alpha --active --harness shell"),
        );
    }
    commit(&fx, "teamspace create beta");
    commit(&fx, "seat create nm --teamspace beta");
    commit(&fx, "seat create x --teamspace beta");
    settle(&fx).await;
    let (nm, x) = (seat(&fx, "nm").id, seat(&fx, "x").id);
    let (a, b) = (seat(&fx, "a").id, seat(&fx, "b").id);
    make_node_manager(&fx, &nm);
    for req in [
        seat_request(&b, Some(&nm)),
        seat_request(&a, Some(&b)),
        seat_request(&x, Some(&a)),
    ] {
        fx.w.admit(req).unwrap();
        fx.w.drain().unwrap();
    }
    let ws = workspace_id(&fx).await;
    fx.herdr.user_close_workspace(&ws);
    step(&fx).await;

    assert_eq!(seat(&fx, "a").lifecycle, Lifecycle::Retired);
    assert_eq!(seat(&fx, "b").lifecycle, Lifecycle::Retired);
    assert_eq!(seat(&fx, "x").manager, Some(nm.clone()));
    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1);
    assert_eq!(
        recorded(&acts[0]),
        vec![Rewrite {
            holder: x.to_any(),
            field: "manager".into(),
            old: a,
            new: Some(nm),
        }]
    );
}

#[tokio::test]
async fn reparent_fallback_on_integrity() {
    use crate::plan::managers::reparent::{REPARENT_FALLBACK_KEY, Rewrite, testing::recorded};
    let fx = fx();
    manager_world(&fx, true).await;
    let (foreman, bench) = (seat(&fx, "foreman").id, seat(&fx, "bench").id);
    fx.herdr.user_close_tab(&tab_of_seat(&fx, "bench"));
    step(&fx).await;

    assert_eq!(
        seat_by_id(&fx, &bench).1.lifecycle,
        Lifecycle::Retired,
        "the cascade still retires the seat"
    );
    assert_eq!(seat(&fx, "report").manager, Some(foreman.clone()));
    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1);
    assert_eq!(
        recorded(&acts[0]),
        vec![Rewrite {
            holder: seat(&fx, "report").id.to_any(),
            field: "manager".into(),
            old: bench,
            new: Some(foreman),
        }]
    );
    let notes = acts[0].compensation[REPARENT_FALLBACK_KEY]
        .as_array()
        .expect("fallback notes recorded");
    assert_eq!(notes.len(), 1);
    assert!(
        notes[0].as_str().unwrap().contains("node manager"),
        "{notes:?}"
    );
}

#[tokio::test]
async fn observed_workspace_close_keeps_own_default_rewrite() {
    use crate::plan::managers::reparent::{
        Rewrite,
        testing::{recorded, teamspace_request},
    };
    let fx = fx();
    activate(&fx, "shell");
    commit(
        &fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    commit(&fx, "teamspace create beta");
    commit(&fx, "seat create nm --teamspace beta");
    settle(&fx).await;
    let (nm, bench) = (seat(&fx, "nm").id, seat(&fx, "bench").id);
    make_node_manager(&fx, &nm);
    let alpha = ts_rec(&fx, "alpha").id;
    fx.w.admit(teamspace_request(&alpha, Some(&bench))).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(ts_rec(&fx, "alpha").default_manager, Some(bench.clone()));

    let ws = workspace_id(&fx).await;
    fx.herdr.user_close_workspace(&ws);
    step(&fx).await;

    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1);
    assert_eq!(
        recorded(&acts[0]),
        vec![Rewrite {
            holder: alpha.to_any(),
            field: "default_manager".into(),
            old: bench,
            new: Some(nm.clone()),
        }]
    );
    let stored = ts_rec(&fx, "alpha");
    assert_eq!(stored.lifecycle, Lifecycle::Retired);
    assert_eq!(
        stored.default_manager,
        Some(nm),
        "the archived record keeps the reparented default"
    );
}

#[tokio::test]
async fn node_manager_workspace_close_keeps_own_default_rewrite() {
    use crate::plan::managers::reparent::testing::{recorded, teamspace_request};
    let fx = fx();
    activate(&fx, "shell");
    commit(
        &fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    let (foreman, bench) = (seat(&fx, "foreman").id, seat(&fx, "bench").id);
    make_node_manager(&fx, &foreman);
    let alpha = ts_rec(&fx, "alpha").id;
    fx.w.admit(teamspace_request(&alpha, Some(&bench))).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(ts_rec(&fx, "alpha").default_manager, Some(bench.clone()));

    let ws = workspace_id(&fx).await;
    fx.herdr.user_close_workspace(&ws);
    step(&fx).await;

    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1);
    let row = recorded(&acts[0])
        .into_iter()
        .find(|r| r.holder == alpha.to_any() && r.field == "default_manager")
        .expect("the teamspace's default_manager is reparented");
    assert_eq!(row.old, bench);
    let stored = ts_rec(&fx, "alpha");
    assert_eq!(
        stored.lifecycle,
        Lifecycle::Active,
        "the node manager's teamspace stays Active"
    );
    assert_eq!(
        stored.default_manager, row.new,
        "the stored pointer equals the recorded rewrite"
    );
    assert!(stored.default_manager.is_some());
}

// =================================================================================================
// per-clone survival decides the node manager's closure (spec §4.9, CascadeMutation)
// =================================================================================================

struct Two {
    fx: Fx,
    alpha_ws: HerdrWorkspaceId,
    beta_ws: HerdrWorkspaceId,
    bob_tab: HerdrTabId,
}

/// alpha: foreman (harness `fh`, node manager, `extra` further clones) and bench (harness `bh`); beta: bob.
/// Settled.
async fn two_teamspaces(fh: &str, bh: &str, extra: usize) -> Two {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(
        &fx,
        &format!("seat create foreman --teamspace alpha --active --harness {fh}"),
    );
    for i in 0..extra {
        commit(&fx, &format!("clone add foreman --name extra{i}"));
    }
    commit(
        &fx,
        &format!("seat create bench --teamspace alpha --active --harness {bh}"),
    );
    commit(&fx, "teamspace create beta");
    commit(
        &fx,
        "seat create bob --teamspace beta --active --harness shell",
    );
    settle(&fx).await;
    make_node_manager(&fx, &seat(&fx, "foreman").id);
    let ws_of = |name: &str| {
        ts_rec(&fx, name)
            .runtime
            .bound
            .unwrap()
            .workspace_id
            .unwrap()
    };
    let (alpha_ws, beta_ws) = (ws_of("alpha"), ws_of("beta"));
    assert_ne!(alpha_ws, beta_ws);
    let bob_tab = tab_of_seat(&fx, "bob");
    Two {
        fx,
        alpha_ws,
        beta_ws,
        bob_tab,
    }
}

fn touches_beta(c: &FakeCall, t: &Two) -> bool {
    match c {
        FakeCall::RenameWorkspace(w, _)
        | FakeCall::ReportWorkspaceMetadata(w, ..)
        | FakeCall::CloseWorkspace(w) => *w == t.beta_ws,
        FakeCall::CloseTab(tab) => *tab == t.bob_tab,
        FakeCall::CreateTab(ct) => ct.workspace == t.beta_ws,
        _ => false,
    }
}

#[tokio::test]
async fn nm_two_clones_one_moved_home_workspace_closed() {
    let t = two_teamspaces("shell", "shell", 1).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    let (mut moved, mut stays) = {
        let mut cs = clones_of(fx, &foreman.id);
        assert_eq!(cs.len(), 2);
        let b = cs.remove(1);
        (cs.remove(0), b)
    };
    let bob_clone = only_clone(fx, "bob");
    let bob_pane = pane_of(fx, &bob_clone);
    fx.herdr.user_move_pane(&pane_of(fx, &moved), &t.bob_tab);
    settle(fx).await;
    assert!(
        !seat(fx, "foreman").moved_out,
        "a clone is still home, so the seat is not moved out"
    );
    moved = clone_by_id(fx, &moved.id);
    let moved_pane = pane_of(fx, &moved);
    stays = clone_by_id(fx, &stays.id);
    let stays_pane = pane_of(fx, &stays);

    fx.herdr.user_close_workspace(&t.alpha_ws);
    fx.herdr.clear_calls();
    settle(fx).await;

    let all = calls(fx);
    assert_eq!(seat(fx, "bob").lifecycle, Lifecycle::Active, "{all:?}");
    assert_eq!(ts_rec(fx, "beta").lifecycle, Lifecycle::Active, "{all:?}");
    assert_eq!(pane_of(fx, &clone_by_id(fx, &bob_clone.id)), bob_pane);
    for c in &all {
        assert!(!touches_beta(c, &t), "{c:?} touches beta: {all:?}");
    }
    let m = clone_by_id(fx, &moved.id);
    assert_eq!(m.lifecycle, CloneLifecycle::Active, "{all:?}");
    assert_eq!(pane_of(fx, &m), moved_pane, "the moved clone stays bound");
    let s = clone_by_id(fx, &stays.id);
    assert_eq!(s.lifecycle, CloneLifecycle::Active, "{all:?}");
    assert_ne!(pane_of(fx, &s), stays_pane, "the home clone has a new pane");
    assert_eq!(seat(fx, "foreman").lifecycle, Lifecycle::Active);
    assert_eq!(ts_rec(fx, "alpha").lifecycle, Lifecycle::Active);
    assert_eq!(seat(fx, "bench").lifecycle, Lifecycle::Retired);
    assert!(
        all.iter()
            .any(|c| matches!(c, FakeCall::CreateWorkspace(_))),
        "alpha gets a fresh home workspace: {all:?}"
    );
}

/// The move and the home workspace's disappearance land in one diff. The survived node-manager clone still gets
/// its MoveFact: reload_required is set, it stays bound where it went, and the operator warning names the move.
#[tokio::test]
async fn nm_clone_moved_in_same_diff_as_home_workspace_closes_records_move() {
    let t = two_teamspaces("shell", "shell", 1).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    let cs = clones_of(fx, &foreman.id);
    assert_eq!(cs.len(), 2);
    let moved = cs[0].clone();
    let before = snapshot(fx).await;
    fx.herdr.user_move_pane(&pane_of(fx, &moved), &t.bob_tab);
    let moved_pane = pane_of(fx, &moved);
    fx.herdr.user_close_workspace(&t.alpha_ws);
    let after = snapshot(fx).await;

    let desired = crate::reconcile::desired::DesiredRuntime::load(&view(fx), &fx.root).unwrap();
    let (mb, ma) = (
        super::matcher::match_snapshot(&before, &desired),
        super::matcher::match_snapshot(&after, &desired),
    );
    let home_tab = before
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .find(|tab| tab.panes.iter().any(|p| p.id == moved_pane))
        .expect("home tab")
        .id
        .clone();
    let els = vec![
        super::diff::DiffElement::Moved {
            clone: moved.id.clone(),
            from_tab: home_tab,
            to_tab: t.bob_tab.clone(),
        },
        super::diff::DiffElement::Disappeared {
            object: ts_rec(fx, "alpha").id.to_any(),
            container: ContainerKind::Workspace,
        },
    ];
    let out = super::classify::classify(
        &super::classify::ClassifyCx {
            desired: &desired,
            journal: &fx.journal,
            preds: &[],
            matches: &ma,
            baseline: Some(&mb),
        },
        els,
    );
    assert!(
        out.moves.iter().any(|m| m.clone == moved.id),
        "the survived clone's move is recorded: {:?}",
        out.moves
    );
    let warn = super::step::node_manager_move_warning(&desired, &out).expect("warned");
    assert!(warn.contains(foreman.id.as_str()), "{warn}");

    fx.herdr.clear_calls();
    settle(fx).await;
    let m = clone_by_id(fx, &moved.id);
    assert_eq!(m.lifecycle, CloneLifecycle::Active);
    assert!(m.reload_required, "the moved agent is told to reload");
    assert_eq!(pane_of(fx, &m), moved_pane, "the moved clone stays bound");
    assert_eq!(seat(fx, "foreman").lifecycle, Lifecycle::Active);
    assert_eq!(seat(fx, "bench").lifecycle, Lifecycle::Retired);
}

/// The same one-diff shape for a plain seat: its clone's pane survives in another teamspace's tab, but the
/// workspace cascade retires the seat and every clone; the recorded move does not revive the retired clone.
#[tokio::test]
async fn plain_clone_moved_in_same_diff_as_workspace_closes_stays_retired() {
    let t = two_teamspaces("shell", "shell", 0).await;
    let fx = &t.fx;
    let bench = only_clone(fx, "bench");
    fx.herdr.user_move_pane(&pane_of(fx, &bench), &t.bob_tab);
    fx.herdr.user_close_workspace(&t.alpha_ws);
    settle(fx).await;
    assert_eq!(seat(fx, "bench").lifecycle, Lifecycle::Retired);
    let b = clone_by_id(fx, &bench.id);
    assert_eq!(b.lifecycle, CloneLifecycle::Retired);
    assert!(!b.reload_required);
    assert_ne!(b.runtime.availability, Availability::Present);
}

/// Spec §4.9 required test (1), placement half: the missing node-manager clone reopens in a fresh home
/// workspace and tab, never as a split of the survivor's pane in the foreign teamspace's tab.
#[tokio::test]
async fn nm_two_clones_one_moved_reopens_in_fresh_home_tab() {
    let t = two_teamspaces("shell", "shell", 1).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    let cs = clones_of(fx, &foreman.id);
    assert_eq!(cs.len(), 2);
    let (moved, stays) = (cs[0].clone(), cs[1].clone());
    fx.herdr.user_move_pane(&pane_of(fx, &moved), &t.bob_tab);
    settle(fx).await;
    let bob_tab_panes = |snap: &HerdrSnapshot| {
        snap.workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .find(|tab| tab.id == t.bob_tab)
            .expect("bob's tab")
            .panes
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>()
    };
    let bob_tab_before = bob_tab_panes(&snapshot(fx).await);
    assert!(bob_tab_before.contains(&pane_of(fx, &moved)));

    fx.herdr.user_close_workspace(&t.alpha_ws);
    fx.herdr.clear_calls();
    settle(fx).await;

    let all = calls(fx);
    let snap = snapshot(fx).await;
    assert_eq!(bob_tab_panes(&snap), bob_tab_before, "{all:?}");
    for c in &all {
        if let FakeCall::SplitPane(sp) = c {
            assert!(
                !bob_tab_before.contains(&sp.target),
                "split into bob's tab: {all:?}"
            );
        }
    }
    let new_pane = pane_of(fx, &clone_by_id(fx, &stays.id));
    let home = snap
        .workspaces
        .iter()
        .find(|w| {
            w.tabs
                .iter()
                .any(|tab| tab.panes.iter().any(|p| p.id == new_pane))
        })
        .expect("the reopened clone lives somewhere");
    assert_ne!(home.id, t.beta_ws, "{all:?}");
    assert_eq!(
        crate::reconcile::planner::token_of(&home.metadata),
        Some(foreman.teamspace.to_any()),
        "reopened in a workspace stamped for the node manager's teamspace"
    );
    assert!(
        all.iter()
            .any(|c| matches!(c, FakeCall::CreateTab(ct) if ct.workspace == home.id)),
        "a CreateTab targeted the home workspace: {all:?}"
    );
}

#[tokio::test]
async fn plain_seat_moved_home_workspace_closed_emits_session_ended() {
    let t = two_teamspaces("shell", "claude", 0).await;
    let fx = &t.fx;
    let bench_clone = only_clone(fx, "bench");
    let bench_pane = pane_of(fx, &bench_clone);
    fx.herdr
        .set_agent(&bench_pane, claude_agent(Some("sess-b")));
    step(fx).await;
    assert!(clone_by_id(fx, &bench_clone.id).occupant.is_some());
    fx.herdr.user_move_pane(&bench_pane, &t.bob_tab);
    settle(fx).await;
    assert!(seat(fx, "bench").moved_out);
    let mut rx = fx.lp.subscribe_session_ended();

    fx.herdr.user_close_workspace(&t.alpha_ws);
    step(fx).await;

    assert_eq!(seat(fx, "bench").lifecycle, Lifecycle::Retired);
    let c = clone_by_id(fx, &bench_clone.id);
    assert_eq!(c.lifecycle, CloneLifecycle::Retired);
    assert!(c.occupant.is_none());
    let ev = rx.try_recv().expect("the retired clone's session ended");
    assert_eq!(ev.clone, bench_clone.id);
    assert_eq!(ev.reason, SessionEndReason::PaneClosed);
    assert!(rx.try_recv().is_err(), "exactly one");
}

#[tokio::test]
async fn nm_survived_clone_gets_no_session_ended() {
    let t = two_teamspaces("claude", "shell", 1).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    let cs = clones_of(fx, &foreman.id);
    let (moved, closed) = (cs[0].clone(), cs[1].clone());
    fx.herdr
        .set_agent(&pane_of(fx, &moved), claude_agent(Some("sess-m")));
    fx.herdr
        .set_agent(&pane_of(fx, &closed), claude_agent(Some("sess-c")));
    step(fx).await;
    fx.herdr.user_move_pane(&pane_of(fx, &moved), &t.bob_tab);
    settle(fx).await;
    assert!(clone_by_id(fx, &moved.id).occupant.is_some());
    let mut rx = fx.lp.subscribe_session_ended();

    fx.herdr.user_close_workspace(&t.alpha_ws);
    step(fx).await;

    let ev = rx.try_recv().expect("the closed clone's session ended");
    assert_eq!(ev.clone, closed.id);
    assert!(rx.try_recv().is_err(), "the survived clone's did not end");
    let m = clone_by_id(fx, &moved.id);
    assert!(m.occupant.is_some(), "still running elsewhere");
    assert!(m.runtime.bound.is_some());
    assert!(clone_by_id(fx, &closed.id).occupant.is_none());
}

/// A foreign tab closes under a single-clone node manager that was moved into it: it reopens in its own
/// teamspace and the foreign teamspace's other seats are untouched.
async fn nm_single_clone_in_foreign_tab(close_whole_workspace: bool) {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(
        &fx,
        "seat create foreman --teamspace alpha --active --harness shell",
    );
    commit(&fx, "teamspace create beta");
    commit(
        &fx,
        "seat create bob --teamspace beta --active --harness shell",
    );
    commit(
        &fx,
        "seat create bob2 --teamspace beta --active --harness shell",
    );
    settle(&fx).await;
    let foreman = seat(&fx, "foreman");
    make_node_manager(&fx, &foreman.id);
    let (bob_tab, bob2_tab) = (tab_of_seat(&fx, "bob"), tab_of_seat(&fx, "bob2"));
    let beta_ws = ts_rec(&fx, "beta")
        .runtime
        .bound
        .unwrap()
        .workspace_id
        .unwrap();
    let bob2_pane = pane_of(&fx, &only_clone(&fx, "bob2"));
    fx.herdr
        .user_move_pane(&pane_of(&fx, &only_clone(&fx, "foreman")), &bob_tab);
    settle(&fx).await;
    assert!(seat(&fx, "foreman").moved_out);

    if close_whole_workspace {
        fx.herdr.user_close_workspace(&beta_ws);
    } else {
        fx.herdr.user_close_tab(&bob_tab);
    }
    fx.herdr.clear_calls();
    settle(&fx).await;

    let s = seat(&fx, "foreman");
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert!(!s.moved_out);
    assert_reopened(&fx, "foreman").await;
    let pane = pane_of(&fx, &only_clone(&fx, "foreman"));
    let snap = snapshot(&fx).await;
    let home = snap
        .workspaces
        .iter()
        .find(|w| w.tabs.iter().any(|t| t.panes.iter().any(|p| p.id == pane)))
        .expect("the pane lives somewhere");
    assert_eq!(
        crate::reconcile::planner::token_of(&home.metadata),
        Some(foreman.teamspace.to_any()),
        "reopened in its own teamspace's workspace"
    );
    if !close_whole_workspace {
        assert_eq!(seat(&fx, "bob2").lifecycle, Lifecycle::Active);
        assert_eq!(ts_rec(&fx, "beta").lifecycle, Lifecycle::Active);
        assert_eq!(pane_of(&fx, &only_clone(&fx, "bob2")), bob2_pane);
        assert_eq!(tab_of_seat(&fx, "bob2"), bob2_tab);
        for c in calls(&fx) {
            assert!(
                !matches!(&c, FakeCall::CloseTab(t) if *t == bob2_tab),
                "{c:?}"
            );
        }
    }
}

#[tokio::test]
async fn nm_single_clone_moved_to_foreign_tab_then_foreign_tab_closed() {
    nm_single_clone_in_foreign_tab(false).await;
}

#[tokio::test]
async fn nm_single_clone_moved_to_foreign_tab_then_foreign_workspace_closed() {
    nm_single_clone_in_foreign_tab(true).await;
}

#[tokio::test]
async fn nm_moved_out_preset_no_survivor_home_workspace_closed() {
    let t = two_teamspaces("shell", "shell", 0).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    fx.w.admit(super::mutations::move_request(
        None,
        None,
        Some(&foreman.id),
    ))
    .unwrap();
    fx.w.drain().unwrap();
    assert!(seat(fx, "foreman").moved_out);

    fx.herdr.user_close_workspace(&t.alpha_ws);
    fx.herdr.clear_calls();
    settle(fx).await;

    let s = seat(fx, "foreman");
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert!(!s.moved_out, "no clone lives elsewhere");
    assert_reopened(fx, "foreman").await;
    assert_eq!(seat(fx, "bench").lifecycle, Lifecycle::Retired);
}

#[tokio::test]
async fn nm_reopen_keeps_recorded_tab_identity() {
    let fx = fx();
    activate(&fx, "shell");
    settle(&fx).await;
    let foreman = seat(&fx, "foreman");
    make_node_manager(&fx, &foreman.id);
    let clone = only_clone(&fx, "foreman");
    fx.herdr.user_close_tab(&tab_of_seat(&fx, "foreman"));
    settle(&fx).await;

    assert_reopened(&fx, "foreman").await;
    let after = seat(&fx, "foreman");
    assert_eq!(after.teamspace, foreman.teamspace, "no placement change");
    assert_eq!(after.name, foreman.name);
    let tab = tab_of_seat(&fx, "foreman");
    let snap = snapshot(&fx).await;
    let live = snap
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .find(|t| t.id == tab)
        .expect("tab is live");
    assert_eq!(live.label, foreman.name, "the recorded tab name");
    let pane = live
        .panes
        .iter()
        .find(|p| crate::reconcile::planner::token_of(&p.metadata) == Some(clone.id.to_any()));
    assert!(pane.is_some(), "the pane carries the clone's graph token");
    for a in actions(&fx, ActionKind::ClosureCascade) {
        assert!(!a.retired.contains(&foreman.id.to_any()));
    }
    assert!(actions(&fx, ActionKind::Retire).is_empty());
}

/// Spec §4.9 required test (4): the home tab closes while one clone lives in another teamspace's tab. The tab
/// path sets `moved_out` without a cascade; the planner still reopens the home clone in the home tab.
#[tokio::test]
async fn nm_home_tab_closed_with_clone_moved_out_reopens_home_clone() {
    let t = two_teamspaces("shell", "shell", 1).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    let cs = clones_of(fx, &foreman.id);
    assert_eq!(cs.len(), 2);
    let (moved, home) = (cs[0].clone(), cs[1].clone());
    let bob_pane = pane_of(fx, &only_clone(fx, "bob"));
    fx.herdr.user_move_pane(&pane_of(fx, &moved), &t.bob_tab);
    settle(fx).await;
    assert!(!seat(fx, "foreman").moved_out);
    let moved_pane = pane_of(fx, &clone_by_id(fx, &moved.id));
    let home_tab = tab_of_seat(fx, "foreman");
    assert_ne!(home_tab, t.bob_tab);
    let bob_tab_panes = |snap: &HerdrSnapshot| {
        snap.workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .find(|tab| tab.id == t.bob_tab)
            .expect("bob's tab")
            .panes
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>()
    };
    let bob_tab_before = bob_tab_panes(&snapshot(fx).await);

    fx.herdr.user_close_tab(&home_tab);
    fx.herdr.clear_calls();
    step(fx).await;
    let s = seat(fx, "foreman");
    // The tab path marks the seat moved out; the reconcile pass of the same step may already have reopened the
    // home tab, which clears it again.
    assert!(
        s.moved_out
            || calls(fx)
                .iter()
                .any(|c| matches!(c, FakeCall::CreateTab(_))),
        "the tab path marks the seat moved out"
    );
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert!(
        actions(fx, ActionKind::ClosureCascade)
            .iter()
            .all(|a| !a.retired.contains(&foreman.id.to_any())),
        "no cascade retires the node manager"
    );
    let h = clone_by_id(fx, &home.id);
    assert_eq!(h.lifecycle, CloneLifecycle::Active);
    assert!(h.occupant.is_none());
    settle(fx).await;

    let all = calls(fx);
    let snap = snapshot(fx).await;
    let h_pane = pane_of(fx, &clone_by_id(fx, &home.id));
    let alpha = snap
        .workspaces
        .iter()
        .find(|w| w.id == t.alpha_ws)
        .expect("alpha's workspace");
    assert!(
        alpha
            .tabs
            .iter()
            .any(|tab| tab.panes.iter().any(|p| p.id == h_pane)),
        "the home clone is live in a tab of alpha's workspace: {all:?}"
    );
    assert_eq!(bob_tab_panes(&snap), bob_tab_before, "{all:?}");
    assert!(
        all.iter().any(|c| matches!(c, FakeCall::CreateTab(_))),
        "{all:?}"
    );
    let m = clone_by_id(fx, &moved.id);
    assert_eq!(m.lifecycle, CloneLifecycle::Active);
    assert_eq!(pane_of(fx, &m), moved_pane, "the moved clone stays bound");
    assert_eq!(
        pane_of(fx, &clone_by_id(fx, &only_clone(fx, "bob").id)),
        bob_pane
    );
    for c in &all {
        assert!(!touches_beta(c, &t), "{c:?} touches beta: {all:?}");
    }
    assert!(
        !seat(fx, "foreman").moved_out,
        "the reopened home tab clears moved_out"
    );
}

/// Required test (4) carried on: once the home tab is reopened the seat is no longer moved out, so a disconnect
/// and rebind re-match the reopened tab, and closing it again reopens it again.
#[tokio::test]
async fn nm_reopened_home_tab_survives_rebind_and_reopens_again() {
    let t = two_teamspaces("shell", "shell", 1).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    let cs = clones_of(fx, &foreman.id);
    let (moved, home) = (cs[0].clone(), cs[1].clone());
    fx.herdr.user_move_pane(&pane_of(fx, &moved), &t.bob_tab);
    settle(fx).await;
    fx.herdr.user_close_tab(&tab_of_seat(fx, "foreman"));
    settle(fx).await;
    assert!(
        !seat(fx, "foreman").moved_out,
        "the reopened home tab is the seat's tab"
    );

    fx.herdr.disconnect_subscribers();
    fx.herdr.fail_next("session.snapshot", Fault::Unavailable);
    step(fx).await;
    settle(fx).await;
    let s = seat(fx, "foreman");
    assert_eq!(
        s.runtime.availability,
        Availability::Present,
        "rebind re-matches the reopened tab"
    );
    let reopened = tab_of_seat(fx, "foreman");
    let first_pane = pane_of(fx, &clone_by_id(fx, &home.id));

    fx.herdr.user_close_tab(&reopened);
    fx.herdr.clear_calls();
    settle(fx).await;
    let all = calls(fx);
    assert!(
        all.iter().any(|c| matches!(c, FakeCall::CreateTab(_))),
        "{all:?}"
    );
    let h = clone_by_id(fx, &home.id);
    assert_eq!(h.lifecycle, CloneLifecycle::Active);
    assert_ne!(
        pane_of(fx, &h),
        first_pane,
        "the home clone is reopened again"
    );
    assert_eq!(
        pane_of(fx, &clone_by_id(fx, &moved.id)),
        pane_of(fx, &moved)
    );
    assert_eq!(seat(fx, "foreman").lifecycle, Lifecycle::Active);
}

/// Required test (4), same-teamspace variant: the moved clone sits in another alpha seat's tab. That tab is not
/// the node manager's, so the home clone reopens through CreateTab rather than as a split in bench's tab.
#[tokio::test]
async fn nm_home_tab_closed_with_clone_in_same_teamspace_tab_reopens_home_clone() {
    let t = two_teamspaces("shell", "shell", 1).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    let cs = clones_of(fx, &foreman.id);
    assert_eq!(cs.len(), 2);
    let (moved, home) = (cs[0].clone(), cs[1].clone());
    let bench_tab = tab_of_seat(fx, "bench");
    fx.herdr.user_move_pane(&pane_of(fx, &moved), &bench_tab);
    settle(fx).await;
    let home_tab = tab_of_seat(fx, "foreman");
    assert_ne!(home_tab, bench_tab);
    let tab_panes = |snap: &HerdrSnapshot, id: &HerdrTabId| {
        snap.workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .find(|tab| tab.id == *id)
            .expect("live tab")
            .panes
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>()
    };
    let bench_before = tab_panes(&snapshot(fx).await, &bench_tab);

    fx.herdr.user_close_tab(&home_tab);
    fx.herdr.clear_calls();
    settle(fx).await;

    let all = calls(fx);
    let s = seat(fx, "foreman");
    assert_eq!(s.lifecycle, Lifecycle::Active);
    let h = clone_by_id(fx, &home.id);
    assert_eq!(h.lifecycle, CloneLifecycle::Active);
    let snap = snapshot(fx).await;
    assert_eq!(tab_panes(&snap, &bench_tab), bench_before, "{all:?}");
    assert!(
        all.iter().any(|c| matches!(c, FakeCall::CreateTab(_))),
        "{all:?}"
    );
    let h_pane = pane_of(fx, &h);
    let h_tab = snap
        .workspaces
        .iter()
        .find(|w| w.id == t.alpha_ws)
        .expect("alpha's workspace")
        .tabs
        .iter()
        .find(|tab| tab.panes.iter().any(|p| p.id == h_pane))
        .map(|tab| tab.id.clone())
        .expect("home clone live in alpha's workspace");
    assert_ne!(h_tab, bench_tab);
    assert_eq!(seat(fx, "bench").lifecycle, Lifecycle::Active);
}

/// Spec §4.9 required test (5): `moved_out` is already set when the home workspace closes. The node manager's
/// non-survived clones reopen in a fresh workspace and tab; the moved clone and the other teamspace stay.
#[tokio::test]
async fn nm_moved_out_preset_home_workspace_closed_reopens_non_survived() {
    let t = two_teamspaces("shell", "shell", 1).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    let cs = clones_of(fx, &foreman.id);
    assert_eq!(cs.len(), 2);
    let (moved, home) = (cs[0].clone(), cs[1].clone());
    let bob_pane = pane_of(fx, &only_clone(fx, "bob"));
    fx.herdr.user_move_pane(&pane_of(fx, &moved), &t.bob_tab);
    settle(fx).await;
    let moved_pane = pane_of(fx, &clone_by_id(fx, &moved.id));
    let home_pane = pane_of(fx, &clone_by_id(fx, &home.id));
    fx.w.admit(super::mutations::move_request(
        None,
        None,
        Some(&foreman.id),
    ))
    .unwrap();
    fx.w.drain().unwrap();
    assert!(seat(fx, "foreman").moved_out, "moved_out is pre-set");

    fx.herdr.user_close_workspace(&t.alpha_ws);
    fx.herdr.clear_calls();
    settle(fx).await;

    let all = calls(fx);
    assert_eq!(seat(fx, "bench").lifecycle, Lifecycle::Retired, "{all:?}");
    assert_eq!(seat(fx, "foreman").lifecycle, Lifecycle::Active, "{all:?}");
    let m = clone_by_id(fx, &moved.id);
    assert_eq!(m.lifecycle, CloneLifecycle::Active, "{all:?}");
    assert_eq!(pane_of(fx, &m), moved_pane, "the moved clone stays bound");
    let h = clone_by_id(fx, &home.id);
    assert_eq!(h.lifecycle, CloneLifecycle::Active, "{all:?}");
    let h_pane = pane_of(fx, &h);
    assert_ne!(h_pane, home_pane, "the home clone has a new pane");
    let snap = snapshot(fx).await;
    let fresh = snap
        .workspaces
        .iter()
        .find(|w| {
            w.tabs
                .iter()
                .any(|tab| tab.panes.iter().any(|p| p.id == h_pane))
        })
        .expect("the reopened clone lives somewhere");
    assert_ne!(fresh.id, t.alpha_ws);
    assert_ne!(fresh.id, t.beta_ws, "{all:?}");
    let pos = |f: &dyn Fn(&FakeCall) -> bool| all.iter().position(f);
    let ws_at = pos(&|c| matches!(c, FakeCall::CreateWorkspace(_))).expect("CreateWorkspace");
    let tab_at = pos(&|c| matches!(c, FakeCall::CreateTab(ct) if ct.workspace == fresh.id))
        .expect("CreateTab in the fresh workspace");
    assert!(ws_at < tab_at, "{all:?}");
    assert_eq!(
        pane_of(fx, &clone_by_id(fx, &only_clone(fx, "bob").id)),
        bob_pane
    );
    assert_eq!(seat(fx, "bob").lifecycle, Lifecycle::Active);
    for c in &all {
        assert!(!touches_beta(c, &t), "{c:?} touches beta: {all:?}");
    }
}

/// Classification only reports facts: the same closure classifies identically with and without a node manager,
/// and a clone whose pane left the closed workspace alive is listed in `retire` and `ends` and named in `survived`.
#[tokio::test]
async fn classify_reports_survival_without_node_manager_input() {
    let t = two_teamspaces("claude", "shell", 1).await;
    let fx = &t.fx;
    let foreman = seat(fx, "foreman");
    let cs = clones_of(fx, &foreman.id);
    let (moved, closed) = (cs[0].clone(), cs[1].clone());
    fx.herdr
        .set_agent(&pane_of(fx, &moved), claude_agent(Some("sess-m")));
    fx.herdr
        .set_agent(&pane_of(fx, &closed), claude_agent(Some("sess-c")));
    step(fx).await;
    fx.herdr.user_move_pane(&pane_of(fx, &moved), &t.bob_tab);
    settle(fx).await;

    let with_nm = crate::reconcile::desired::DesiredRuntime::load(&view(fx), &fx.root).unwrap();
    assert_eq!(with_nm.node_manager, Some(foreman.id.clone()));
    let mut without_nm = with_nm.clone();
    without_nm.node_manager = None;
    let before = snapshot(fx).await;
    fx.herdr.user_close_workspace(&t.alpha_ws);
    let after = snapshot(fx).await;
    let run = |d: &crate::reconcile::desired::DesiredRuntime| {
        let (mb, ma) = (
            super::matcher::match_snapshot(&before, d),
            super::matcher::match_snapshot(&after, d),
        );
        super::classify::classify(
            &super::classify::ClassifyCx {
                desired: d,
                journal: &fx.journal,
                preds: &[],
                matches: &ma,
                baseline: Some(&mb),
            },
            vec![super::diff::DiffElement::Disappeared {
                object: ts_rec(fx, "alpha").id.to_any(),
                container: ContainerKind::Workspace,
            }],
        )
    };
    let (a, b) = (run(&with_nm), run(&without_nm));
    assert_eq!(a.cascades, b.cascades);
    assert_eq!(a.warnings, b.warnings);
    assert_eq!(a.cascades.len(), 1);
    let cas = &a.cascades[0];
    assert_eq!(cas.survived, vec![moved.id.clone()]);
    assert!(cas.retire.contains(&moved.id.to_any()));
    assert!(cas.retire.contains(&closed.id.to_any()));
    let ended: Vec<_> = cas.ends.iter().map(|e| e.clone.clone()).collect();
    assert!(
        ended.contains(&moved.id) && ended.contains(&closed.id),
        "{ended:?}"
    );
}

/// The observation alone (before the reconciler reopens anything): the home workspace holding the node manager
/// and a sibling closes. The sibling is retired; the node manager's teamspace stays Active, runtime absent.
#[tokio::test]
async fn workspace_close_marks_node_manager_teamspace_absent() {
    let fx = fx();
    activate(&fx, "shell");
    commit(
        &fx,
        "seat create bench --teamspace alpha --active --harness shell",
    );
    settle(&fx).await;
    make_node_manager(&fx, &seat(&fx, "foreman").id);
    let desired = crate::reconcile::desired::DesiredRuntime::load(&view(&fx), &fx.root).unwrap();
    let ws = workspace_id(&fx).await;
    let before = snapshot(&fx).await;
    fx.herdr.user_close_workspace(&ws);
    let after = snapshot(&fx).await;
    let (mb, ma) = (
        super::matcher::match_snapshot(&before, &desired),
        super::matcher::match_snapshot(&after, &desired),
    );
    let out = super::classify::classify(
        &super::classify::ClassifyCx {
            desired: &desired,
            journal: &fx.journal,
            preds: &[],
            matches: &ma,
            baseline: Some(&mb),
        },
        vec![super::diff::DiffElement::Disappeared {
            object: ts_rec(&fx, "alpha").id.to_any(),
            container: ContainerKind::Workspace,
        }],
    );
    apply_classified(&fx, &out);

    let ts = ts_rec(&fx, "alpha");
    assert_eq!(ts.lifecycle, Lifecycle::Active);
    assert_eq!(ts.runtime.availability, Availability::Absent);
    assert!(ts.runtime.bound.is_none());
    assert_eq!(seat(&fx, "bench").lifecycle, Lifecycle::Retired);
    let f = seat(&fx, "foreman");
    assert_eq!(f.lifecycle, Lifecycle::Active);
    assert_eq!(f.runtime.availability, Availability::Absent);
    settle(&fx).await;
    assert_reopened(&fx, "foreman").await;
}

// =================================================================================================
// redesign-code-1 integration sweep (hg-k4l.67)
// =================================================================================================

/// Every ended native session in the store, as `(clone, ns, end reason)` strings.
fn ended_sessions(fx: &Fx) -> std::collections::BTreeSet<(String, String, String)> {
    layout::all_clones(&view(fx))
        .unwrap()
        .into_iter()
        .flat_map(|(_, c)| {
            c.sessions
                .iter()
                .filter(|s| s.ended.is_some())
                .map(|s| {
                    (
                        format!("{:?}", c.id),
                        format!("{:?}", s.id),
                        format!("{:?}", s.end_reason),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The `SessionEnded` events received since the last drain, in the shape of [`ended_sessions`].
fn drain_ended(
    rx: &mut tokio::sync::broadcast::Receiver<super::step::SessionEnded>,
) -> std::collections::BTreeSet<(String, String, String)> {
    let mut out = std::collections::BTreeSet::new();
    while let Ok(ev) = rx.try_recv() {
        out.insert((
            format!("{:?}", ev.clone),
            format!("{:?}", ev.ns),
            format!("{:?}", Some(ev.reason)),
        ));
    }
    out
}

/// Spec §4.9 / §4.11 scenarios composed in one world: observe -> `CascadeMutation` -> `commit_ops` -> reconcile
/// against `FakeHerdr`. After each scenario the store's newly ended sessions equal the emitted `SessionEnded`
/// events, and the foreign teamspace `other` is untouched.
#[tokio::test]
async fn redesign_sweep_scenarios_compose() {
    use crate::plan::managers::reparent::testing::{recorded, teamspace_request};
    let fx = fx();
    let fx = &fx;
    commit(fx, "teamspace create hq");
    commit(
        fx,
        "seat create nm --teamspace hq --active --harness claude",
    );
    commit(fx, "clone add nm --name nm2");
    commit(
        fx,
        "seat create sib --teamspace hq --active --harness claude",
    );
    commit(fx, "teamspace create other");
    commit(
        fx,
        "seat create o --teamspace other --active --harness claude",
    );
    commit(fx, "teamspace create away");
    commit(
        fx,
        "seat create p --teamspace away --active --harness claude",
    );
    settle(fx).await;
    let (nm, sib) = (seat(fx, "nm").id, seat(fx, "sib").id);
    make_node_manager(fx, &nm);
    let hq = ts_rec(fx, "hq").id;
    fx.w.admit(teamspace_request(&hq, Some(&sib))).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(ts_rec(fx, "hq").default_manager, Some(sib.clone()));

    // Every clone gets an occupant, so each end is a native session end.
    for s in ["nm", "sib", "o", "p"] {
        for (n, c) in clones_of(fx, &seat(fx, s).id).iter().enumerate() {
            fx.herdr.set_agent(
                &pane_of(fx, c),
                claude_agent(Some(&format!("sess-{s}-{n}"))),
            );
        }
    }
    step(fx).await;
    for s in ["nm", "sib", "o", "p"] {
        for c in clones_of(fx, &seat(fx, s).id) {
            assert!(c.occupant.is_some(), "{s} clone has an occupant");
            assert_eq!(c.sessions.len(), 1, "{s} clone captured its session");
        }
    }
    let mut rx = fx.lp.subscribe_session_ended();
    let ws_of = |name: &str| {
        ts_rec(fx, name)
            .runtime
            .bound
            .unwrap()
            .workspace_id
            .unwrap()
    };
    let (hq_ws, other_ws, away_ws) = (ws_of("hq"), ws_of("other"), ws_of("away"));
    let o_tab = tab_of_seat(fx, "o");
    let o_pane = pane_of(fx, &only_clone(fx, "o"));
    let mut ended_before = ended_sessions(fx);
    assert!(ended_before.is_empty());
    fx.herdr.clear_calls();

    // `other` is untouched: no workspace/tab-level call and no split targets it, `o` stays bound and Active.
    let check_other = |when: &str| {
        let all = calls(fx);
        for c in &all {
            let hit = match c {
                FakeCall::RenameWorkspace(w, _)
                | FakeCall::ReportWorkspaceMetadata(w, ..)
                | FakeCall::CloseWorkspace(w) => *w == other_ws,
                FakeCall::CloseTab(t) | FakeCall::RenameTab(t, _) => *t == o_tab,
                FakeCall::CreateTab(ct) => ct.workspace == other_ws,
                FakeCall::SplitPane(sp) => sp.target == o_pane,
                FakeCall::ClosePane(p) => *p == o_pane,
                _ => false,
            };
            assert!(!hit, "{when}: {c:?} touches `other`: {all:?}");
        }
        assert_eq!(seat(fx, "o").lifecycle, Lifecycle::Active, "{when}");
        assert_eq!(ts_rec(fx, "other").lifecycle, Lifecycle::Active, "{when}");
        assert_eq!(
            ts_rec(fx, "other").runtime.bound.unwrap().workspace_id,
            Some(other_ws.clone()),
            "{when}"
        );
        assert_eq!(tab_of_seat(fx, "o"), o_tab, "{when}");
    };
    let check_ends =
        |when: &str,
         before: &mut std::collections::BTreeSet<(String, String, String)>,
         rx: &mut tokio::sync::broadcast::Receiver<super::step::SessionEnded>| {
            let now = ended_sessions(fx);
            let new: std::collections::BTreeSet<_> = now.difference(before).cloned().collect();
            assert!(before.is_subset(&now), "{when}: a session end was undone");
            assert_eq!(new, drain_ended(rx), "{when}: store ends equal events");
            *before = now;
            new
        };

    // 1. Move: nm's second clone and p's clone into `other`'s tab.
    let nm_clones = clones_of(fx, &nm);
    assert_eq!(nm_clones.len(), 2);
    let (nm_home, nm_moved) = (nm_clones[0].clone(), nm_clones[1].clone());
    let (nm_home_pane, nm_moved_pane) = (pane_of(fx, &nm_home), pane_of(fx, &nm_moved));
    let p_clone = only_clone(fx, "p");
    let p_pane = pane_of(fx, &p_clone);
    fx.herdr.user_move_pane(&nm_moved_pane, &o_tab);
    fx.herdr.user_move_pane(&p_pane, &o_tab);
    settle(fx).await;
    assert!(check_ends("move", &mut ended_before, &mut rx).is_empty());
    check_other("move");

    // 2. Two-clone node manager moved, then the home workspace closes; the teamspace default rewrite survives.
    fx.herdr.user_close_workspace(&hq_ws);
    settle(fx).await;
    let all = calls(fx);
    assert_eq!(seat(fx, "nm").lifecycle, Lifecycle::Active, "{all:?}");
    assert_eq!(ts_rec(fx, "hq").lifecycle, Lifecycle::Active, "{all:?}");
    assert_eq!(seat(fx, "sib").lifecycle, Lifecycle::Retired, "{all:?}");
    let m = clone_by_id(fx, &nm_moved.id);
    assert_eq!(m.lifecycle, CloneLifecycle::Active);
    assert_eq!(
        pane_of(fx, &m),
        nm_moved_pane,
        "the moved clone stays bound"
    );
    let h = clone_by_id(fx, &nm_home.id);
    assert_eq!(h.lifecycle, CloneLifecycle::Active, "{all:?}");
    let h_pane = pane_of(fx, &h);
    assert_ne!(h_pane, nm_home_pane, "the home clone has a new pane");
    let snap = snapshot(fx).await;
    let fresh = snap
        .workspaces
        .iter()
        .find(|w| {
            w.tabs
                .iter()
                .any(|t| t.panes.iter().any(|p| p.id == h_pane))
        })
        .expect("the reopened clone lives somewhere");
    assert_ne!(fresh.id, hq_ws, "a new workspace id");
    assert_ne!(fresh.id, other_ws, "{all:?}");
    let home_tab = tab_of_seat(fx, "nm");
    assert_ne!(
        home_tab, o_tab,
        "the node manager's seat tab is not other's"
    );
    assert!(
        fresh.tabs.iter().any(|t| t.id == home_tab),
        "the seat tab is in the fresh workspace"
    );
    assert_eq!(
        ts_rec(fx, "hq").runtime.bound.unwrap().workspace_id,
        Some(fresh.id.clone())
    );
    let acts = actions(fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1, "one cascade so far");
    let row = recorded(&acts[0])
        .into_iter()
        .find(|r| r.holder == hq.to_any() && r.field == "default_manager")
        .expect("hq's default_manager is reparented");
    assert_eq!(row.old, sib);
    assert_eq!(row.new, Some(nm.clone()));
    assert_eq!(
        ts_rec(fx, "hq").default_manager,
        row.new,
        "the stored pointer equals the recorded rewrite"
    );
    let ends = check_ends("node manager workspace", &mut ended_before, &mut rx);
    let ended_clones: std::collections::BTreeSet<_> = ends.iter().map(|e| e.0.clone()).collect();
    let sib_clone = only_clone(fx, "sib");
    assert_eq!(
        ended_clones,
        [format!("{:?}", nm_home.id), format!("{:?}", sib_clone.id)]
            .into_iter()
            .collect(),
        "nm's survived clone gets no end"
    );
    check_other("node manager workspace");

    // 3. Plain seat moved, then its home workspace closes: retired, SessionEnded for its surviving clone.
    fx.herdr.user_close_workspace(&away_ws);
    settle(fx).await;
    assert_eq!(seat(fx, "p").lifecycle, Lifecycle::Retired);
    let pc = clone_by_id(fx, &p_clone.id);
    assert_eq!(pc.lifecycle, CloneLifecycle::Retired);
    assert!(pc.occupant.is_none());
    let ends = check_ends("plain seat", &mut ended_before, &mut rx);
    assert_eq!(ends.len(), 1);
    assert_eq!(ends.iter().next().unwrap().0, format!("{:?}", p_clone.id));
    assert_eq!(seat(fx, "nm").lifecycle, Lifecycle::Active);
    check_other("plain seat");

    // 4. The reopened node-manager home tab closes while its other clone is still in `other`'s tab.
    let nm_home_tab = tab_of_seat(fx, "nm");
    let reopened_pane = pane_of(fx, &clone_by_id(fx, &nm_home.id));
    fx.herdr.user_close_tab(&nm_home_tab);
    settle(fx).await;
    let s = seat(fx, "nm");
    assert_eq!(s.lifecycle, Lifecycle::Active);
    assert_eq!(s.runtime.availability, Availability::Present);
    let h = clone_by_id(fx, &nm_home.id);
    assert_eq!(h.lifecycle, CloneLifecycle::Active);
    let h_pane = pane_of(fx, &h);
    assert_ne!(h_pane, reopened_pane, "the home clone was reopened");
    let snap = snapshot(fx).await;
    let home_ws = snap
        .workspaces
        .iter()
        .find(|w| {
            w.tabs
                .iter()
                .any(|t| t.panes.iter().any(|p| p.id == h_pane))
        })
        .expect("the reopened clone lives somewhere");
    assert_eq!(home_ws.id, fresh.id, "reopened in the hq home workspace");
    assert_eq!(pane_of(fx, &clone_by_id(fx, &nm_moved.id)), nm_moved_pane);
    check_ends("node manager tab", &mut ended_before, &mut rx);
    check_other("node manager tab");
}
