//! Observer tests: FakeHerdr + real writer, journal, git store, plan engine and reconciler, driven through
//! `RuntimeLoop::step_once`. The "user" is `FakeHerdr::user_*`.
use super::step::{LoopTuning, StepMode};
use super::*;
use crate::config::InstancePaths;
use crate::daemon::registry::CallerInfo;
use crate::herdr::fake::{FakeCall, FakeHerdr, Fault};
use crate::journal::Journal;
use crate::model::action::{ActionKind, ActionRecord};
use crate::model::clone::CloneRecord;
use crate::model::common::{Availability, CloneLifecycle, Lifecycle, NameSource, RetireMechanism};
use crate::model::effect::{ContainerKind, EffectKind, EffectRecord, EffectStatus, EndState, PredictedEnd};
use crate::model::harness::{Harness, claude_project_slug};
use crate::model::native_session::SessionEndReason;
use crate::model::operation::OpState;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{AnyId, CloneId, EffectId, HerdrPaneId, HerdrTabId, HerdrWorkspaceId, OpId, SeatId, Timestamp};
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
    );
    lp.set_tuning(LoopTuning { commit_timeout: Duration::from_secs(5), settle_timeout: Duration::from_millis(50), debounce: Duration::from_millis(5) });
    lp
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
    let mut reg = crate::writer::MutationRegistry::default();
    kinds.register_mutations(&mut reg, plans.clone());
    crate::reconcile::register_mutations(&mut reg);
    register_mutations(&mut reg);
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let w = WriterCore::new(store.clone(), journal.clone(), Arc::new(reg), clock.clone(), WriterConfig::default());
    let dw = Arc::new(DrainingWriter { core: w.clone(), hold: AtomicBool::new(false) });
    let deps = PlanDeps { kinds, plans, store: store.clone(), writer: w.clone(), clock: clock.clone(), instance: root.clone() };
    let herdr = FakeHerdr::new();
    let mut cfg = ReconcilerConfig::new(root.clone());
    cfg.idle_timeout = Duration::from_millis(100);
    cfg.exit_timeout = Duration::from_millis(100);
    cfg.exit_followup = Duration::from_millis(20);
    let rec = Reconciler::new(store.clone(), journal.clone(), dw.clone(), herdr.clone(), clock.clone(), Arc::new(QuietNotifier), cfg);
    let lp = new_loop(&herdr, &store, &dw, &journal, &rec, &clock, &root);
    Fx { _tmp: tmp, root, deps, w, dw, store, journal, herdr, clock, rec, lp }
}

impl Fx {
    /// A second loop over the same instance and Herdr (a daemon restart): fresh in-memory state, baseline from disk.
    fn restarted_loop(&self) -> Arc<RuntimeLoop> {
        new_loop(&self.herdr, &self.store, &self.dw, &self.journal, &self.rec, &self.clock, &self.root)
    }
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn plan(fx: &Fx, change: &str) -> StoredPlan {
    let v = create_plan(&fx.deps, &CallerInfo::default(), words(change)).unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: crate::model::PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    fx.deps.plans.get(&id).unwrap().unwrap()
}

fn commit(fx: &Fx, change: &str) {
    let sp = plan(fx, change);
    let op = admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay")
        .unwrap_or_else(|e| panic!("admit {change}: {}", e.message));
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(row.state, OpState::Committed, "{change}: {:?}", row.rejection);
}

fn view(fx: &Fx) -> CommitView<'_> {
    CommitView { store: &*fx.store, at: crate::ports::store::Store::head(&*fx.store).unwrap() }
}

fn seat(fx: &Fx, name: &str) -> SeatRecord {
    let mut found: Vec<_> = layout::all_seats(&view(fx)).unwrap().into_iter().filter(|(_, s)| s.name == name).collect();
    assert_eq!(found.len(), 1, "seat {name}");
    found.remove(0).1
}

fn seat_by_id(fx: &Fx, id: &SeatId) -> (crate::ports::store::ObjectLocation, SeatRecord) {
    layout::all_seats(&view(fx)).unwrap().into_iter().find(|(_, s)| &s.id == id).expect("seat exists")
}

fn ts_rec(fx: &Fx, name: &str) -> TeamspaceRecord {
    layout::list_teamspaces(&view(fx)).unwrap().into_iter().map(|(_, t)| t).find(|t| t.name == name).expect("teamspace")
}

fn clones_of(fx: &Fx, seat: &SeatId) -> Vec<CloneRecord> {
    layout::all_clones(&view(fx)).unwrap().into_iter().map(|(_, c)| c).filter(|c| &c.seat == seat).collect()
}

fn only_clone(fx: &Fx, seat_name: &str) -> CloneRecord {
    let s = seat(fx, seat_name);
    let mut cs = clones_of(fx, &s.id);
    assert_eq!(cs.len(), 1, "clones of {seat_name}");
    cs.remove(0)
}

fn clone_by_id(fx: &Fx, id: &CloneId) -> CloneRecord {
    layout::all_clones(&view(fx)).unwrap().into_iter().map(|(_, c)| c).find(|c| &c.id == id).expect("clone")
}

fn actions(fx: &Fx, kind: ActionKind) -> Vec<ActionRecord> {
    layout::list_actions(&view(fx)).unwrap().into_iter().map(|(_, a)| a).filter(|a| a.kind == kind).collect()
}

fn calls(fx: &Fx) -> Vec<FakeCall> {
    fx.herdr.calls()
}

fn creates(fx: &Fx) -> usize {
    calls(fx)
        .iter()
        .filter(|c| matches!(c, FakeCall::CreateWorkspace(_) | FakeCall::CreateTab(_) | FakeCall::SplitPane(_)))
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
            && s.reconcile.as_ref().is_some_and(|r| r.planned.is_empty() && r.executed.is_empty());
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
    clone_by_id(fx, &c.id).runtime.bound.expect("clone is bound").pane_id.expect("bound to a pane")
}

fn tab_of_seat(fx: &Fx, name: &str) -> HerdrTabId {
    seat(fx, name).runtime.bound.expect("seat is bound").tab_id.expect("bound to a tab")
}

async fn workspace_id(fx: &Fx) -> HerdrWorkspaceId {
    snapshot(fx).await.workspaces[0].id.clone()
}

/// Herdr's own default tab goes, so the seat tab is the workspace's last.
async fn drop_default_tab(fx: &Fx) {
    let snap = snapshot(fx).await;
    let t = snap.workspaces[0].tabs.iter().find(|t| t.label == "1").unwrap().id.clone();
    fx.herdr.user_close_tab(&t);
}

fn activate(fx: &Fx, harness: &str) {
    commit(fx, "teamspace create alpha");
    commit(fx, &format!("seat create foreman --teamspace alpha --active --harness {harness}"));
}

fn claude_agent(session: Option<&str>) -> Option<AgentInfo> {
    Some(AgentInfo { kind: "claude".into(), status: AgentStatus::Idle, session: session.map(|s| AgentSession::Id(s.into())) })
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
    assert_eq!(clone_by_id(&fx, &keeper.id).lifecycle, CloneLifecycle::Active);
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
    assert_eq!(retired, [s.id.to_any(), c.id.to_any()].into_iter().collect());
    let (loc, s2) = seat_by_id(&fx, &s.id);
    assert_eq!(s2.lifecycle, Lifecycle::Retired);
    assert_eq!(s2.retired.unwrap().mechanism, RetireMechanism::ObservedTabClose);
    assert!(loc.folder.as_str().contains("/archive/seats/"), "archived: {}", loc.folder.as_str());
    let c2 = clone_by_id(&fx, &c.id);
    assert_eq!(c2.retired.unwrap().mechanism, RetireMechanism::ObservedTabClose);
    assert_eq!(ts_rec(&fx, "alpha").lifecycle, Lifecycle::Active, "the workspace survives its other tab");

    fx.herdr.clear_calls();
    step(&fx).await;
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);
    assert_eq!(creates(&fx), 0);
}

#[tokio::test]
async fn workspace_close_retires_dormant_seats_too() {
    let fx = fx();
    activate(&fx, "shell");
    commit(&fx, "seat create bench --teamspace alpha --active --harness shell");
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
    assert_eq!(acts[0].retired.iter().cloned().collect::<std::collections::BTreeSet<_>>(), expect);
    assert!(acts[0].already_retired.is_empty());
    let ts = ts_rec(&fx, "alpha");
    assert_eq!(ts.lifecycle, Lifecycle::Retired);
    assert_eq!(ts.retired.unwrap().mechanism, RetireMechanism::ObservedWorkspaceClose);
    assert_eq!(seat_by_id(&fx, &bench.id).1.lifecycle, Lifecycle::Retired, "the dormant seat goes with the workspace");
    assert_eq!(seat_by_id(&fx, &foreman.id).1.lifecycle, Lifecycle::Retired);
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
    assert!(calls(&fx).iter().any(|c| matches!(c, FakeCall::ClosePane(_))));
    step(&fx).await; // the observer sees the disappearance
    step(&fx).await;

    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 0, "explained by intent: no second retirement");
    assert_eq!(actions(&fx, ActionKind::Retire).len(), 1);
    let v = clone_by_id(&fx, &victim.id);
    assert_eq!(v.lifecycle, CloneLifecycle::Retired);
    assert!(!matches!(
        v.retired.as_ref().unwrap().mechanism,
        RetireMechanism::ObservedPaneClose | RetireMechanism::ObservedTabClose | RetireMechanism::ObservedWorkspaceClose
    ));
    assert_eq!((v.runtime.availability, v.runtime.bound), (Availability::Absent, None), "bookkeeping only");
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
    assert_eq!(ts.lifecycle, Lifecycle::Active, "the induced workspace close is predicted, not a user retirement");
    assert_eq!(ts.runtime.availability, Availability::Absent);
    assert_eq!(seat(&fx, "foreman").lifecycle, Lifecycle::Retired, "the plan's induced seat retirement");
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
    assert_eq!((s.runtime.availability, s.runtime.bound.is_none()), (Availability::Absent, true));
    let c = only_clone(&fx, "foreman");
    assert_eq!(c.lifecycle, CloneLifecycle::Active, "clones stay active");
    assert_eq!((c.runtime.availability, c.runtime.bound.is_none()), (Availability::Absent, true));

    // Reactivation recreates the tab: the dormant interlude left nothing behind.
    commit(&fx, "seat activate foreman");
    fx.herdr.clear_calls();
    settle(&fx).await;
    assert_eq!(calls(&fx).iter().filter(|c| matches!(c, FakeCall::CreateTab(_))).count(), 1);
    assert_eq!(seat(&fx, "foreman").runtime.availability, Availability::Present);
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

    let (s, c, ts) = (seat(&fx, "foreman"), only_clone(&fx, "foreman"), ts_rec(&fx, "alpha"));
    for rt in [&s.runtime, &c.runtime, &ts.runtime] {
        assert_eq!(rt.availability, Availability::Unknown);
        assert!(rt.bound.is_some(), "the binding is kept so the rebind can match it");
    }
    assert_eq!((s.lifecycle, c.lifecycle, ts.lifecycle), (Lifecycle::Active, CloneLifecycle::Active, Lifecycle::Active));
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty());

    // Still down: no more writes, no retirement.
    fx.herdr.fail_next("session.snapshot", Fault::Timeout);
    let again = step(&fx).await;
    assert!(again.observed.is_empty());

    // Back up: a rebind pass restores presence; nothing was closed, so nothing is retired or created.
    fx.herdr.clear_calls();
    let up = step(&fx).await;
    assert_eq!(up.mode, Some(StepMode::Rebind));
    assert_eq!(seat(&fx, "foreman").runtime.availability, Availability::Present);
    assert_eq!(only_clone(&fx, "foreman").runtime.availability, Availability::Present);
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
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty(), "a restart is not a closure");
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
    let snap = snapshot(&fx).await;
    for c in &before {
        let now = clone_by_id(&fx, &c.id);
        assert_eq!(now.lifecycle, CloneLifecycle::Active);
        let b = now.runtime.bound.expect("rebound");
        assert_eq!(b.incarnation, snap.incarnation, "binding follows the new incarnation");
        assert_ne!(b.pane_id, c.runtime.bound.as_ref().unwrap().pane_id, "new pane id recorded");
        assert_eq!(now.runtime.availability, Availability::Present);
    }
    assert_eq!(seat(&fx, "foreman").runtime.bound.unwrap().incarnation, snap.incarnation);
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
    assert_eq!(now.runtime.bound.as_ref().unwrap().incarnation, snap.incarnation);
    // The reconciler re-stamps what the restart dropped.
    let stamped = snap.workspaces[0].tabs.iter().flat_map(|t| &t.panes).any(|p| p.metadata.get("hg").is_some_and(|v| *v == format!("hg={}", c.id)));
    assert!(stamped, "pane token restored");
    assert!(snap.workspaces[0].metadata.contains_key("hg"), "workspace token restored");
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
    assert_eq!(clone_by_id(&fx, &c.id).runtime.availability, Availability::Unknown);
    let snap = snapshot(&fx).await;
    let new_pane = snap.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).next().expect("pane survives restart").id.clone();
    assert!(snap.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).all(|p| !p.metadata.contains_key("hg") || p.id != new_pane));
    fx.herdr.clear_calls();

    commit(&fx, &format!("clone rebind {} --pane {}", c.id, new_pane.0));
    step(&fx).await;
    step(&fx).await;

    assert!(
        calls(&fx).contains(&FakeCall::ReportPaneMetadata(new_pane.clone(), "hg".into(), token.clone())),
        "{:?}",
        calls(&fx)
    );
    assert_eq!(creates(&fx), 0, "{:?}", calls(&fx));
    let snap = snapshot(&fx).await;
    let pane = snap.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).find(|p| p.id == new_pane).unwrap();
    assert_eq!(pane.metadata.get("hg"), Some(&token));
    let now = clone_by_id(&fx, &c.id);
    assert_eq!(now.runtime.bound.as_ref().unwrap().incarnation, snap.incarnation);
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
    assert_eq!(creates(&fx), 0, "no create effects for unknown objects: {:?}", calls(&fx));
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
    assert_eq!(sum.mode, Some(StepMode::Rebind), "a fresh daemon never diffs against an old baseline");
    lp2.step_once().await.unwrap();
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty(), "never silent retirement");
    assert_eq!(clone_by_id(&fx, &c.id).lifecycle, CloneLifecycle::Active);
    assert_eq!(clone_by_id(&fx, &c.id).runtime.availability, Availability::Unknown);
    assert_eq!(seat(&fx, "foreman").runtime.availability, Availability::Unknown);
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
    assert_eq!((ch.old.as_str(), ch.new.as_str(), ch.source), ("foreman", "boss", NameSource::Observed));
    assert_eq!(ch.observed_at, t0() + chrono::Duration::seconds(30));
    assert_eq!(ch.event_at, None);
    assert!(!calls(&fx).iter().any(|c| matches!(c, FakeCall::RenameTab(..))), "recorded, not reverted: {:?}", calls(&fx));
}

#[tokio::test]
async fn herdr_resumed_agent_adopted_as_occupancy_start() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    assert!(c.occupant.is_none(), "an agent without a session id is not recorded yet");
    fx.herdr.restart(true, true);
    let snap = snapshot(&fx).await;
    let pane = snap.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).find(|p| p.metadata.contains_key("hg")).unwrap().id.clone();
    fx.herdr.set_agent(&pane, claude_agent(Some("sess-resumed")));
    fx.clock.advance(chrono::Duration::seconds(5));

    let sum = step(&fx).await;
    assert_eq!(sum.mode, Some(StepMode::Rebind));
    let now = only_clone(&fx, "foreman");
    let occ = now.occupant.expect("adopted as the occupant");
    assert_eq!(occ.harness, Harness::Claude);
    assert_eq!(occ.since, t0() + chrono::Duration::seconds(5));
    let ns = now.sessions.iter().find(|s| s.id == occ.native_session).unwrap();
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
    assert_eq!((ch.old.as_str(), ch.new.as_str(), ch.source), ("foreman", "boss", NameSource::Observed));
    assert_eq!(ch.observed_at, t0() + chrono::Duration::seconds(7));
    assert_eq!(ch.event_at, None, "event time is never invented");
    let (loc, _) = seat_by_id(&fx, &s.id);
    assert!(loc.folder.as_str().ends_with("/seats/boss"), "folder follows the name: {}", loc.folder.as_str());
    step(&fx).await;
    assert!(!calls(&fx).iter().any(|c| matches!(c, FakeCall::RenameTab(..))), "no rename effect is generated back");
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
    commit(&fx, "seat create other --teamspace alpha --active --harness claude");
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
    assert!(!seat(&fx, "foreman").moved_out, "the seat's tab still holds its other clone");
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
        .create_tab(CreateTab { workspace: ws, label: "scratch".into(), cwd: "/".into(), env: vec![] })
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
    assert!(actions(&fx, ActionKind::ClosureCascade).is_empty(), "a move is not a closure");
    let m = clone_by_id(&fx, &c.id);
    assert!(m.reload_required);
    assert_eq!(m.lifecycle, CloneLifecycle::Active);

    step(&fx).await;
    step(&fx).await;
    assert_eq!(creates(&fx), 0, "no tab for a seat whose clones live elsewhere: {:?}", calls(&fx));
    assert!(seat(&fx, "foreman").moved_out, "stays moved out until a rebind plan");
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
        .create_tab(CreateTab { workspace: ws, label: "scratch".into(), cwd: "/".into(), env: vec![] })
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
    );
    lp.set_tuning(LoopTuning { commit_timeout: Duration::from_secs(5), settle_timeout: Duration::from_millis(50), debounce: Duration::from_millis(5) });
    let (tx, rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(lp.clone().run(rx));
    // First iteration: subscribe, rebind pass, a fresh baseline stamped with the advanced clock.
    for _ in 0..200 {
        if lp.baseline().is_some_and(|b| b.taken_at == started) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(lp.baseline().map(|b| b.taken_at), Some(started), "the loop took its first snapshot");
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
        predicted: [PredictedEnd { object: object.clone(), container, end: EndState::Closed, induced: false }]
            .into_iter()
            .chain(also)
            .collect(),
        nonce_label: None,
        attempts: 1,
        last_error: None,
        updated_at: t0(),
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
    assert!(!predictions(&fx.journal).iter().any(|(e, _)| *e == ea.id), "discarded: the container remained");
    // So a later, real user closure of that pane is not absorbed by the stale prediction.
    fx.herdr.user_close_pane(&pane_of(&fx, &a));
    step(&fx).await;
    assert_eq!(clone_by_id(&fx, &a.id).lifecycle, CloneLifecycle::Retired);
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);

    // `b`: the effect closed the pane (as predicted): explained, no retirement, and the prediction is spent.
    // Closing the seat's last pane takes the tab with it, which the effect predicted as induced.
    let tab = PredictedEnd { object: s.id.to_any(), container: ContainerKind::Tab, end: EndState::Closed, induced: true };
    let eb = done_effect(&b.id.to_any(), ContainerKind::Pane, vec![tab]);
    fx.journal.upsert_effect(&eb).unwrap();
    fx.herdr.user_close_pane(&pane_of(&fx, &b));
    step(&fx).await;
    assert_eq!(clone_by_id(&fx, &b.id).lifecycle, CloneLifecycle::Active, "explained by the predicted end state");
    assert_eq!(actions(&fx, ActionKind::ClosureCascade).len(), 1);
    assert!(!predictions(&fx.journal).iter().any(|(e, _)| *e == eb.id), "single use");
}

#[tokio::test]
async fn observed_rejection_is_rederived() {
    let fx = fx();
    activate(&fx, "claude");
    settle(&fx).await;
    let s = seat(&fx, "foreman");
    let c = only_clone(&fx, "foreman");
    fx.herdr.user_close_pane(&pane_of(&fx, &c));
    fx.lp.set_tuning(LoopTuning { settle_timeout: Duration::ZERO, ..LoopTuning::default() });

    // A confirmed `seat retire` is queued ahead of the observer's cascade: both target the same seat.
    let sp = plan(&fx, "seat retire foreman");
    admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay").unwrap();
    step(&fx).await;

    let acts = actions(&fx, ActionKind::ClosureCascade);
    assert_eq!(acts.len(), 1, "the cascade still ran");
    assert!(acts[0].retired.is_empty(), "nothing left to retire: {:?}", acts[0].retired);
    let already: std::collections::BTreeSet<_> = acts[0].already_retired.iter().cloned().collect();
    assert_eq!(already, [s.id.to_any(), c.id.to_any()].into_iter().collect());
    let (_, s2) = seat_by_id(&fx, &s.id);
    assert_eq!(s2.lifecycle, Lifecycle::Retired);
    assert!(
        !matches!(s2.retired.unwrap().mechanism, RetireMechanism::ObservedTabClose),
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
    fx.lp.set_tuning(LoopTuning { commit_timeout: Duration::from_millis(100), settle_timeout: Duration::ZERO, ..LoopTuning::default() });

    // The writer holds the op: it is admitted but never commits.
    fx.dw.hold.store(true, Ordering::SeqCst);
    let sum = step(&fx).await;
    assert!(!sum.baseline_advanced);
    assert!(sum.reconcile.is_none(), "the reconciler never acts on uncommitted observations");
    assert_eq!(baseline_bytes(&fx), before, "baseline unchanged on disk");
    assert!(fx.lp.baseline().unwrap().snapshot.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).any(|p| p.id == pane));
    assert_eq!(clone_by_id(&fx, &c.id).lifecycle, CloneLifecycle::Active);

    // The writer catches up; the next step re-derives against the same baseline and advances it.
    fx.dw.hold.store(false, Ordering::SeqCst);
    fx.w.drain().unwrap();
    let sum = step(&fx).await;
    assert!(sum.baseline_advanced);
    assert_ne!(baseline_bytes(&fx), before);
    let now = fx.lp.baseline().unwrap();
    assert!(!now.snapshot.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).any(|p| p.id == pane));
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
    let transcript = claude_root.join("projects").join(claude_project_slug(&cwd)).join("sess-1.jsonl");
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    std::fs::write(&transcript, "{}\n").unwrap();
    fx.lp.set_claude_root(claude_root);
    let mut rx = fx.lp.subscribe_session_ended();

    fx.herdr.set_agent(&pane, claude_agent(Some("sess-1")));
    step(&fx).await;
    let started = only_clone(&fx, "foreman");
    let occ = started.occupant.clone().expect("occupancy started");
    let ns = started.sessions.iter().find(|s| s.id == occ.native_session).unwrap();
    assert_eq!(ns.native_session_id, "sess-1");
    assert_eq!(ns.transcript_path.as_deref(), Some(transcript.as_path()));
    assert!(rx.try_recv().is_err(), "nothing ended yet");

    fx.clock.advance(chrono::Duration::seconds(9));
    fx.herdr.set_agent(&pane, None);
    step(&fx).await;
    let ev = rx.try_recv().expect("session-ended hook fired");
    assert_eq!((ev.clone.clone(), ev.seat.clone(), ev.ns.clone()), (c.id.clone(), c.seat.clone(), occ.native_session.clone()));
    assert_eq!((ev.harness, ev.reason), (Harness::Claude, SessionEndReason::AgentExited));
    assert_eq!(ev.transcript_path.as_deref(), Some(transcript.as_path()));
    // Emitted after the committing op finished, and the record shows the end.
    assert_eq!(fx.journal.get(&ev.op).unwrap().unwrap().state, OpState::Committed);
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
    commit(&fx, "seat create other --teamspace alpha --active --harness claude");
    settle(&fx).await;
    let c = only_clone(&fx, "foreman");
    let pane = pane_of(&fx, &c);
    fx.herdr.set_agent(&pane, Some(AgentInfo { kind: "claude".into(), status: AgentStatus::Working, session: Some(AgentSession::Id("sess-1".into())) }));
    step(&fx).await;
    assert!(clone_by_id(&fx, &c.id).occupant.is_some(), "the occupant is observed");

    // The model change plans a replacement that has to wait for the working occupant.
    fx.herdr.clear_calls();
    commit(&fx, "seat override foreman --model fancy");
    tokio::time::timeout(Duration::from_secs(2), step(&fx)).await.expect("the step must not wait for the occupant");
    let waiting: Vec<_> = fx.journal.effects_with_status(&[EffectStatus::Pending]).unwrap().into_iter().filter(|r| r.kind == EffectKind::ReplaceSession).collect();
    assert_eq!(waiting.len(), 1, "the replacement is waiting");

    // A user rename on the other seat is observed and committed by the next step.
    let tab = tab_of_seat(&fx, "other");
    fx.herdr.user_rename_tab(&tab, "renamed");
    tokio::time::timeout(Duration::from_secs(2), step(&fx)).await.expect("the step must not wait for the occupant");
    assert_eq!(seat(&fx, "renamed").name, "renamed");
    assert_eq!(calls(&fx).iter().filter(|c| matches!(c, FakeCall::SendKeys(..) | FakeCall::StartAgent(_))).count(), 0, "the working occupant was never touched");
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
        agent: Some(AgentInfo { kind: kind.into(), status: AgentStatus::Idle, session }),
    }
}

#[test]
fn capture_claude_agent_session_id_and_transcript_path() {
    let root = tempfile::tempdir().unwrap();
    let cwd = Path::new("/work/my proj");
    let expected = root.path().join("projects").join(claude_project_slug(cwd)).join("abc-123.jsonl");
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
    assert_eq!(cap.transcript_path, Some(p), "the path kind is the transcript path");
}

#[test]
fn capture_claude_glob_fallback() {
    let root = tempfile::tempdir().unwrap();
    let found = root.path().join("projects/-some-other-cwd/zzz.jsonl");
    std::fs::create_dir_all(found.parent().unwrap()).unwrap();
    std::fs::write(&found, "").unwrap();
    let pane = pane_with(Some(AgentSession::Id("zzz".into())), "claude");
    let cap = capture_session(Harness::Claude, &pane, None, root.path(), Path::new("/not/the/slug")).unwrap();
    assert_eq!(cap.transcript_path, Some(found), "glob projects/*/<id>.jsonl");
    let missing = pane_with(Some(AgentSession::Id("nope".into())), "claude");
    let cap = capture_session(Harness::Claude, &missing, None, root.path(), Path::new("/w")).unwrap();
    assert_eq!((cap.native_session_id.as_str(), cap.transcript_path), ("nope", None));
}

#[test]
fn capture_codex_agent_session() {
    let root = tempfile::tempdir().unwrap();
    let pane = pane_with(Some(AgentSession::Id("cdx-1".into())), "codex");
    let argv = ProcessInfo { foreground_pid: Some(1), foreground_argv: vec!["codex".into(), "resume".into(), "other".into()], is_shell: false };
    let cap = capture_session(Harness::Codex, &pane, Some(&argv), root.path(), Path::new("/w")).unwrap();
    assert_eq!(cap.native_session_id, "cdx-1", "Herdr's agent_session outranks argv");
    assert_eq!(cap.transcript_path, None, "codex transcripts stay unresolved");
}

#[test]
fn capture_codex_argv_resume_id() {
    let root = tempfile::tempdir().unwrap();
    let pane = pane_with(None, "codex");
    let argv = ProcessInfo {
        foreground_pid: Some(1),
        foreground_argv: ["codex", "--no-daemon", "resume", "r-77", "-m", "x"].map(str::to_owned).to_vec(),
        is_shell: false,
    };
    let cap = capture_session(Harness::Codex, &pane, Some(&argv), root.path(), Path::new("/w")).unwrap();
    assert_eq!(cap.native_session_id, "r-77");
    let none = ProcessInfo { foreground_pid: Some(1), foreground_argv: vec!["codex".into()], is_shell: false };
    assert!(capture_session(Harness::Codex, &pane, Some(&none), root.path(), Path::new("/w")).is_none());
    assert!(capture_session(Harness::Codex, &pane, None, root.path(), Path::new("/w")).is_none());
}

#[test]
fn capture_shell_none() {
    let root = tempfile::tempdir().unwrap();
    let pane = pane_with(Some(AgentSession::Id("whatever".into())), "claude");
    let argv = ProcessInfo { foreground_pid: Some(1), foreground_argv: vec!["resume".into(), "x".into()], is_shell: true };
    assert!(capture_session(Harness::Shell, &pane, Some(&argv), root.path(), Path::new("/w")).is_none());
}

#[test]
fn baseline_roundtrip_and_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/baseline.json");
    assert!(baseline::load(&path).is_none());
    let snap = HerdrSnapshot { incarnation: crate::model::Incarnation { generation: 3, server_pid: Some(9), server_started: None }, workspaces: vec![] };
    let b = baseline::Baseline::new(snap, t0());
    baseline::save(&path, &b).unwrap();
    assert_eq!(baseline::load(&path), Some(b));
    std::fs::write(&path, "{not json").unwrap();
    assert!(baseline::load(&path).is_none(), "a corrupt baseline means a rebind pass");
}
