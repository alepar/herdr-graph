//! The shared runtime loop step (spec §4.3.6): complete snapshot → rebind pass or diff → observed
//! mutations (admitted, awaited) → baseline advance → session-ended hook → `Reconciler::step`.
use super::baseline::{self, Baseline};
use super::classify::{Classified, ClassifyCx, EndedSession, classify};
use super::diff::{self, DiffCx, needs_process};
use super::matcher::{Matches, PaneLoc, find_tab, find_ws, match_snapshot, pane_locs};
use super::mutations::{
    availability_request, cascade_request, move_request, occupancy_request, rename_request,
};
use crate::config::InstancePaths;
use crate::journal::Journal;
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::common::{Availability, Binding, CloneLifecycle, Lifecycle, Runtime};
use crate::model::effect::EffectStatus;
use crate::model::harness::Harness;
use crate::model::native_session::SessionEndReason;
use crate::model::operation::OpState;
use crate::model::{AnyId, CloneId, Incarnation, NsId, OpId, SeatId, Timestamp};
use crate::ports::clock::Clock;
use crate::ports::herdr::{HerdrApi, HerdrSnapshot, ProcessInfo, TabInfo, WorkspaceInfo};
use crate::ports::store::Store;
use crate::ports::writer::Writer;
use crate::reconcile::desired::DesiredRuntime;
use crate::reconcile::planner::TOKEN_KEY;
use crate::reconcile::{Reconciler, StepReport, consume_prediction, predictions};
use crate::store::tree::CommitView;
use crate::writer::OpEvent;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::sync::{broadcast, watch};

/// Broadcast after the observed mutation that ended a native session commits (consumed by transcripts).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionEnded {
    pub clone: CloneId,
    pub seat: SeatId,
    pub ns: NsId,
    pub harness: Harness,
    pub transcript_path: Option<PathBuf>,
    pub reason: SessionEndReason,
    pub op: OpId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    /// First complete snapshot after start, reconnect or incarnation change: identity matching only.
    Rebind,
    /// Diff between two complete snapshots of one incarnation.
    Diff,
    /// The snapshot failed; nothing was diffed.
    Disconnected,
}

#[derive(Debug, Clone, Default)]
pub struct StepSummary {
    pub mode: Option<StepMode>,
    /// Ops admitted by this step's observation half (observed and binding bookkeeping).
    pub observed: Vec<OpId>,
    pub committed: usize,
    /// Ops that ended rejected, failed, or did not finish in time.
    pub rejected: usize,
    pub baseline_advanced: bool,
    /// `None` when the observations did not all commit (the reconciler never acts on those) or on disconnect.
    pub reconcile: Option<StepReport>,
}

#[derive(Debug, Clone)]
pub struct LoopTuning {
    /// How long one observed op may take to reach a terminal state.
    pub commit_timeout: Duration,
    /// How long a step waits for ops already queued in the writer before it reads committed state.
    pub settle_timeout: Duration,
    /// Pause after a trigger so a burst of events becomes one step.
    pub debounce: Duration,
}

impl Default for LoopTuning {
    fn default() -> Self {
        Self {
            commit_timeout: Duration::from_secs(30),
            settle_timeout: Duration::from_secs(2),
            debounce: Duration::from_millis(100),
        }
    }
}

struct LoopState {
    /// The previous snapshot succeeded (so a failure now is a disconnect, not a still-down connection).
    healthy: bool,
    needs_rebind: bool,
    baseline: Option<Baseline>,
}

struct PlannedOp {
    request: ChangeRequest,
    ends: Vec<EndedSession>,
}

pub struct RuntimeLoop {
    herdr: Arc<dyn HerdrApi>,
    store: Arc<dyn Store>,
    writer: Arc<dyn Writer>,
    journal: Arc<Journal>,
    reconciler: Arc<Reconciler>,
    clock: Arc<dyn Clock>,
    paths: InstancePaths,
    tick: Duration,
    session_ended: broadcast::Sender<SessionEnded>,
    op_events: Mutex<Option<broadcast::Receiver<OpEvent>>>,
    state: Mutex<LoopState>,
    step_lock: tokio::sync::Mutex<()>,
    tuning: RwLock<LoopTuning>,
    claude_root: RwLock<PathBuf>,
}

fn bookkeeping(args: serde_json::Value) -> ChangeRequest {
    ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args,
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    }
}

fn binding_request(object: &AnyId, b: &Binding) -> ChangeRequest {
    bookkeeping(
        json!({ "sub": "binding", "object": object, "binding": b, "availability": Availability::Present }),
    )
}

fn clone_binding(loc: &PaneLoc<'_>, inc: &Incarnation) -> Binding {
    Binding {
        token: loc.pane.metadata.get(TOKEN_KEY).cloned(),
        workspace_id: Some(loc.ws.id.clone()),
        tab_id: Some(loc.tab.id.clone()),
        pane_id: Some(loc.pane.id.clone()),
        terminal_id: loc.pane.terminal_id.clone(),
        incarnation: inc.clone(),
    }
}

fn seat_binding(ws: &WorkspaceInfo, tab: &TabInfo, inc: &Incarnation) -> Binding {
    Binding {
        token: None,
        workspace_id: Some(ws.id.clone()),
        tab_id: Some(tab.id.clone()),
        pane_id: None,
        terminal_id: None,
        incarnation: inc.clone(),
    }
}

fn ts_binding(ws: &WorkspaceInfo, inc: &Incarnation) -> Binding {
    Binding {
        token: ws.metadata.get(TOKEN_KEY).cloned(),
        workspace_id: Some(ws.id.clone()),
        tab_id: None,
        pane_id: None,
        terminal_id: None,
        incarnation: inc.clone(),
    }
}

fn runtime_of<'a>(d: &'a DesiredRuntime, object: &AnyId) -> Option<&'a Runtime> {
    if let Ok(c) = CloneId::parse(object.as_str()) {
        return d.clones.get(&c).map(|r| &r.runtime);
    }
    if let Ok(s) = SeatId::parse(object.as_str()) {
        return d.seats.get(&s).map(|r| &r.runtime);
    }
    crate::model::TeamspaceId::parse(object.as_str())
        .ok()
        .and_then(|t| d.teamspaces.get(&t))
        .map(|r| &r.runtime)
}

/// Turn a classification into the requests of one step, in commit order: cascades, renames, occupancy,
/// moves, explained closures, unknowns (rebind only), and finally binding write-backs for matched objects.
fn build_ops(
    d: &DesiredRuntime,
    snap: &HerdrSnapshot,
    m: &Matches,
    c: &Classified,
    rebind: bool,
    now: Timestamp,
) -> Vec<PlannedOp> {
    let mut ops: Vec<PlannedOp> = Vec::new();
    let retired_now = c.retired_now();
    let plain = |request: ChangeRequest| PlannedOp {
        request,
        ends: vec![],
    };

    for cas in &c.cascades {
        ops.push(PlannedOp {
            request: cascade_request(cas.rule, &cas.retire, now),
            ends: cas.ends.clone(),
        });
    }
    for (object, new) in &c.renames {
        ops.push(plain(rename_request(object, new, now, None)));
    }
    for oc in &c.occupancy {
        ops.push(PlannedOp {
            request: occupancy_request(
                &oc.clone,
                oc.end.as_ref().map(|e| e.reason),
                oc.start.as_ref(),
                now,
            ),
            ends: oc.end.iter().cloned().collect(),
        });
    }

    let locs = pane_locs(snap);
    let mut attached: BTreeSet<SeatId> = BTreeSet::new();
    let mut moved: BTreeSet<CloneId> = BTreeSet::new();
    for mv in &c.moves {
        moved.insert(mv.clone.clone());
        let binding = if mv.known {
            m.clone_pane
                .get(&mv.clone)
                .and_then(|p| locs.get(p))
                .map(|loc| clone_binding(loc, &snap.incarnation))
        } else {
            None
        };
        let seat = d
            .clones
            .get(&mv.clone)
            .map(|r| r.seat.clone())
            .filter(|s| c.moved_out.contains(s) && !attached.contains(s));
        if let Some(s) = &seat {
            attached.insert(s.clone());
        }
        ops.push(plain(move_request(
            Some(&mv.clone),
            binding.as_ref(),
            seat.as_ref(),
        )));
    }
    for seat in c.moved_out.iter().filter(|s| !attached.contains(*s)) {
        ops.push(plain(move_request(None, None, Some(seat))));
    }

    for object in &c.gone {
        let needs = runtime_of(d, object)
            .is_some_and(|rt| rt.availability != Availability::Absent || rt.bound.is_some());
        if !retired_now.contains(object) && needs {
            ops.push(plain(availability_request(object, Availability::Absent)));
        }
    }
    for e in &c.gone_ends {
        if !retired_now.contains(&e.clone.to_any()) {
            ops.push(PlannedOp {
                request: occupancy_request(&e.clone, Some(e.reason), None, now),
                ends: vec![e.clone()],
            });
        }
    }
    if rebind {
        for object in &m.unmatched {
            let known =
                runtime_of(d, object).is_some_and(|rt| rt.availability == Availability::Unknown);
            if !retired_now.contains(object) && !known {
                ops.push(plain(availability_request(object, Availability::Unknown)));
            }
        }
    }

    // Bindings of matched objects. A clone living in a tab graph does not know stays as the move left it.
    let inc = &snap.incarnation;
    for (cid, pid) in &m.clone_pane {
        let (Some(rec), Some(loc)) = (d.clones.get(cid), locs.get(pid)) else {
            continue;
        };
        if rec.lifecycle == CloneLifecycle::Retired
            || moved.contains(cid)
            || retired_now.contains(&cid.to_any())
        {
            continue;
        }
        if !m.tabs.contains_key(&loc.tab.id) {
            continue;
        }
        let b = clone_binding(loc, inc);
        if rec.runtime.bound.as_ref() != Some(&b)
            || rec.runtime.availability != Availability::Present
        {
            ops.push(plain(binding_request(&cid.to_any(), &b)));
        }
    }
    for (tid, sid) in &m.tabs {
        let (Some(rec), Some((ws, tab))) = (d.seats.get(sid), find_tab(snap, tid)) else {
            continue;
        };
        if rec.lifecycle == Lifecycle::Retired || retired_now.contains(&sid.to_any()) {
            continue;
        }
        let b = seat_binding(ws, tab, inc);
        if rec.runtime.bound.as_ref() != Some(&b)
            || rec.runtime.availability != Availability::Present
        {
            ops.push(plain(binding_request(&sid.to_any(), &b)));
        }
    }
    for (wid, tsid) in &m.workspaces {
        let (Some(rec), Some(ws)) = (d.teamspaces.get(tsid), find_ws(snap, wid)) else {
            continue;
        };
        if rec.lifecycle == Lifecycle::Retired || retired_now.contains(&tsid.to_any()) {
            continue;
        }
        let b = ts_binding(ws, inc);
        if rec.runtime.bound.as_ref() != Some(&b)
            || rec.runtime.availability != Availability::Present
        {
            ops.push(plain(binding_request(&tsid.to_any(), &b)));
        }
    }
    ops
}

impl RuntimeLoop {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        herdr: Arc<dyn HerdrApi>,
        store: Arc<dyn Store>,
        writer: Arc<dyn Writer>,
        journal: Arc<Journal>,
        reconciler: Arc<Reconciler>,
        clock: Arc<dyn Clock>,
        paths: InstancePaths,
        tick: Duration,
        op_events: Option<broadcast::Receiver<OpEvent>>,
        claude_root: PathBuf,
    ) -> Arc<Self> {
        let (session_ended, _) = broadcast::channel(256);
        let baseline = baseline::load(&paths.baseline);
        Arc::new(Self {
            herdr,
            store,
            writer,
            journal,
            reconciler,
            clock,
            paths,
            tick,
            session_ended,
            op_events: Mutex::new(op_events),
            // Always a rebind pass first: the daemon may have missed anything while it was down.
            state: Mutex::new(LoopState {
                healthy: false,
                needs_rebind: true,
                baseline,
            }),
            step_lock: tokio::sync::Mutex::new(()),
            tuning: RwLock::new(LoopTuning::default()),
            claude_root: RwLock::new(claude_root),
        })
    }

    pub fn subscribe_session_ended(&self) -> broadcast::Receiver<SessionEnded> {
        self.session_ended.subscribe()
    }

    pub fn set_tuning(&self, t: LoopTuning) {
        *self.tuning.write().unwrap_or_else(|e| e.into_inner()) = t;
    }

    pub fn set_claude_root(&self, root: PathBuf) {
        *self.claude_root.write().unwrap_or_else(|e| e.into_inner()) = root;
    }

    fn tuning(&self) -> LoopTuning {
        self.tuning
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, LoopState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The next snapshot is a rebind pass (a subscription ended or was re-established).
    pub fn force_rebind(&self) {
        self.state().needs_rebind = true;
    }

    /// The persisted baseline as this loop currently holds it.
    pub fn baseline(&self) -> Option<Baseline> {
        self.state().baseline.clone()
    }

    async fn wait_writer_idle(&self, limit: Duration) {
        let deadline = tokio::time::Instant::now() + limit;
        while tokio::time::Instant::now() < deadline {
            let busy = self
                .journal
                .list(&[OpState::Admitted, OpState::Applying], 1)
                .map(|r| !r.is_empty())
                .unwrap_or(false);
            if !busy {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn wait_terminal(&self, op: &OpId, limit: Duration) -> Option<OpState> {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            match self.writer.status(op) {
                Ok(Some(s)) if !matches!(s, OpState::Admitted | OpState::Applying) => {
                    return Some(s);
                }
                Ok(None) | Err(_) => return None,
                Ok(Some(_)) => {}
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Admit `ops` in order and wait for each to reach a terminal state. Emits `SessionEnded` for every
    /// committed op that ended sessions. Returns whether all committed.
    async fn commit_ops(
        &self,
        ops: Vec<PlannedOp>,
        limit: Duration,
        summary: &mut StepSummary,
    ) -> bool {
        let mut admitted: Vec<(OpId, Vec<EndedSession>)> = Vec::new();
        let mut all_ok = true;
        for op in ops {
            match self.writer.admit(op.request) {
                Ok(id) => {
                    summary.observed.push(id.clone());
                    admitted.push((id, op.ends));
                }
                Err(e) => {
                    eprintln!("herdr-graph: observe: cannot admit observed mutation: {e}");
                    summary.rejected += 1;
                    all_ok = false;
                }
            }
        }
        for (id, ends) in admitted {
            match self.wait_terminal(&id, limit).await {
                Some(OpState::Committed) => {
                    summary.committed += 1;
                    for e in ends {
                        // No subscribers is fine.
                        let _ = self.session_ended.send(SessionEnded {
                            clone: e.clone,
                            seat: e.seat,
                            ns: e.ns,
                            harness: e.harness,
                            transcript_path: e.transcript_path,
                            reason: e.reason,
                            op: id.clone(),
                        });
                    }
                }
                other => {
                    eprintln!(
                        "herdr-graph: observe: observed op {id} ended {other:?}; the next diff re-derives it"
                    );
                    summary.rejected += 1;
                    all_ok = false;
                }
            }
        }
        all_ok
    }

    /// One shared step (spec §4.3.6).
    pub async fn step_once(&self) -> anyhow::Result<StepSummary> {
        let _one_at_a_time = self.step_lock.lock().await;
        let tuning = self.tuning();
        self.wait_writer_idle(tuning.settle_timeout).await;
        let head = self.store.head()?;
        let snap = match self.herdr.snapshot().await {
            Ok(s) => s,
            Err(e) => return self.on_disconnect(e, &tuning).await,
        };
        let now = self.clock.now();
        let (rebind, baseline) = {
            let mut st = self.state();
            st.healthy = true;
            let b = st.baseline.clone();
            let rebind =
                st.needs_rebind || b.as_ref().is_none_or(|b| b.incarnation != snap.incarnation);
            (rebind, b)
        };
        let desired = {
            let tree = CommitView {
                store: &*self.store,
                at: head.clone(),
            };
            DesiredRuntime::load(&tree, &self.paths.root)?
        };
        let matches = match_snapshot(&snap, &desired);
        let mut procs: BTreeMap<crate::model::HerdrPaneId, ProcessInfo> = BTreeMap::new();
        for pane in needs_process(&desired, &snap, &matches) {
            if let Ok(info) = self.herdr.process_info(&pane).await {
                procs.insert(pane, info);
            }
        }
        let claude_root = self
            .claude_root
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let cx = DiffCx {
            desired: &desired,
            snap: &snap,
            matches: &matches,
            claude_root: &claude_root,
            procs: &procs,
        };
        let base_matches;
        let (elements, baseline_matches) = match (&baseline, rebind) {
            (Some(b), false) => {
                base_matches = match_snapshot(&b.snapshot, &desired);
                (
                    diff::diff(&cx, &b.snapshot, &base_matches),
                    Some(&base_matches),
                )
            }
            _ => (diff::rebind(&cx), None),
        };
        let preds = predictions(&self.journal);
        let classified = classify(
            &ClassifyCx {
                desired: &desired,
                journal: &self.journal,
                preds: &preds,
                matches: &matches,
                baseline: baseline_matches,
            },
            elements,
        );
        let ops = build_ops(&desired, &snap, &matches, &classified, rebind, now);

        let mut summary = StepSummary {
            mode: Some(if rebind {
                StepMode::Rebind
            } else {
                StepMode::Diff
            }),
            ..Default::default()
        };
        if !self
            .commit_ops(ops, tuning.commit_timeout, &mut summary)
            .await
        {
            // Baseline stays; the next step re-derives what did not commit (spec §4.3.7).
            return Ok(summary);
        }

        // The observations are committed: predictions were used once, and every effect completed before this
        // snapshot has had its diff, so its prediction is spent (or discarded because the container remains).
        for ef in &classified.consumed {
            consume_prediction(&self.journal, ef);
        }
        let spent: BTreeSet<_> = predictions(&self.journal)
            .into_iter()
            .map(|(ef, _)| ef)
            .collect();
        for ef in spent {
            if matches!(self.journal.get_effect(&ef), Ok(Some(r)) if r.status == EffectStatus::Done)
            {
                consume_prediction(&self.journal, &ef);
            }
        }
        let new_baseline = Baseline::new(snap.clone(), now);
        if let Err(e) = baseline::save(&self.paths.baseline, &new_baseline) {
            eprintln!("herdr-graph: observe: cannot persist baseline: {e}");
        }
        {
            let mut st = self.state();
            st.baseline = Some(new_baseline);
            st.needs_rebind = false;
        }
        summary.baseline_advanced = true;

        let head = self.store.head()?;
        summary.reconcile = Some(self.reconciler.step(&head, &snap).await);
        Ok(summary)
    }

    /// The snapshot failed. After a healthy connection every bound object becomes `unknown` (never retired);
    /// either way the next successful snapshot is a rebind pass.
    async fn on_disconnect(
        &self,
        err: crate::ports::herdr::HerdrError,
        tuning: &LoopTuning,
    ) -> anyhow::Result<StepSummary> {
        eprintln!("herdr-graph: observe: snapshot failed: {err}");
        let was_healthy = {
            let mut st = self.state();
            let was = st.healthy;
            st.healthy = false;
            st.needs_rebind = true;
            was
        };
        let mut summary = StepSummary {
            mode: Some(StepMode::Disconnected),
            ..Default::default()
        };
        if !was_healthy {
            return Ok(summary);
        }
        let head = self.store.head()?;
        let tree = CommitView {
            store: &*self.store,
            at: head,
        };
        let d = DesiredRuntime::load(&tree, &self.paths.root)?;
        let mut ops: Vec<PlannedOp> = Vec::new();
        let mut mark = |object: AnyId, rt: &Runtime| {
            if rt.bound.is_some() && rt.availability != Availability::Unknown {
                ops.push(PlannedOp {
                    request: availability_request(&object, Availability::Unknown),
                    ends: vec![],
                });
            }
        };
        for t in d
            .teamspaces
            .values()
            .filter(|t| t.lifecycle != Lifecycle::Retired)
        {
            mark(t.id.to_any(), &t.runtime);
        }
        for s in d
            .seats
            .values()
            .filter(|s| s.lifecycle != Lifecycle::Retired)
        {
            mark(s.id.to_any(), &s.runtime);
        }
        for c in d
            .clones
            .values()
            .filter(|c| c.lifecycle != CloneLifecycle::Retired)
        {
            mark(c.id.to_any(), &c.runtime);
        }
        self.commit_ops(ops, tuning.commit_timeout, &mut summary)
            .await;
        Ok(summary)
    }

    /// Trigger loop: Herdr events, the tick, op-committed notifications and reconnects all just cause a new
    /// complete snapshot (spec §4.3.1). Returns when `shutdown` turns true.
    pub async fn run(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        let mut op_rx = self
            .op_events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let mut events: Option<crate::ports::herdr::HerdrEventStream> = None;
        let mut ticker = tokio::time::interval(self.tick.max(Duration::from_millis(1)));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;
        loop {
            if *shutdown.borrow() {
                return;
            }
            if events.is_none() {
                match self.herdr.subscribe().await {
                    Ok(stream) => {
                        events = Some(stream);
                        self.force_rebind();
                    }
                    Err(e) => {
                        eprintln!("herdr-graph: observe: cannot subscribe to herdr events: {e}")
                    }
                }
            }
            if let Err(e) = self.step_once().await {
                eprintln!("herdr-graph: observe: step failed: {e:#}");
            }
            let resubscribe = events.is_none();
            // Deferred or backed-off effects need another look at their own time, not only on the tick.
            let wake = self.reconciler.next_wake().map(|t| {
                (t - self.clock.now())
                    .to_std()
                    .unwrap_or_default()
                    .clamp(Duration::from_millis(20), self.tick)
            });
            tokio::select! {
                _ = shutdown.changed() => {}
                _ = async { match wake { Some(d) => tokio::time::sleep(d).await, None => std::future::pending::<()>().await } } => {}
                _ = ticker.tick() => {}
                _ = async { if resubscribe { tokio::time::sleep(Duration::from_secs(5)).await } else { std::future::pending::<()>().await } } => {}
                ev = async {
                    match events.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if ev.is_none() {
                        events = None;
                        self.force_rebind();
                    }
                }
                ev = async {
                    match op_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if matches!(ev, Err(broadcast::error::RecvError::Closed)) {
                        op_rx = None;
                    }
                }
            }
            // One step per burst: let the rest of the burst arrive, then drop what queued meanwhile.
            tokio::time::sleep(self.tuning().debounce).await;
            if let Some(rx) = events.as_mut() {
                while rx.try_recv().is_ok() {}
            }
            if let Some(rx) = op_rx.as_mut() {
                while rx.try_recv().is_ok() {}
            }
        }
    }
}
