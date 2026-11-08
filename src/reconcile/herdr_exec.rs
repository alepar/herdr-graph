//! Built-in Herdr effect family: the planner as an `EffectSource` and the executor that performs
//! creates, stamps, renames, closes, agent starts and session replacement (spec §4.2, §4.4, §4.5).
use super::ReconcilerConfig;
use super::bookkeeping::{admit_binding, admit_runtime};
use super::desired::{DesiredPane, DesiredRuntime};
use super::executor::{DiffCx, EffectExecutor, EffectSource, ExecCx, ExecOutcome, PlannedEffect};
use super::planner::{
    LiveIndex, LiveRef, Surplus, TOKEN_KEY, delete_live_ref, incarnation_changed_at,
    launch_env_key, launched_before, launched_key, live_ref, plan_effects, restore_mark_failed_key,
    restore_mark_failures, resumed_key, set_live_ref, token_of,
};
use super::session::{self, Relaunch, advance_replacement, start_outcome, transient_or_failed};
use crate::journal::Journal;
use crate::model::clone::SeatIntent;
use crate::model::common::{Availability, Binding, CommitId, HerdrPaneId};
use crate::model::effect::{EffectKind, EffectRecord, EffectStatus};
use crate::model::harness::{Harness, profile};
use crate::model::launch::{agent_env, graph_token, nonce_label};
use crate::model::{AnyId, CloneId, SeatId, TeamspaceId};
use crate::ports::herdr::{
    CreateTab, CreateWorkspace, HerdrError, SplitDirection, SplitPane, StartAgent,
};
use crate::ports::store::StoreError;
use crate::store::tree::TreeRead;
use crate::threads::assignment::{SeatTransition, admit_threads_seat};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// Desired runtime of the most recently read revision, shared by the source and the executor.
#[derive(Default)]
pub struct DesiredCache(Mutex<Option<(CommitId, Arc<DesiredRuntime>)>>);

impl DesiredCache {
    pub fn get(
        &self,
        tree: &dyn TreeRead,
        head: &CommitId,
        instance: &std::path::Path,
    ) -> Result<Arc<DesiredRuntime>, StoreError> {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, d)) = slot.as_ref()
            && at == head
        {
            return Ok(d.clone());
        }
        let d = Arc::new(DesiredRuntime::load(tree, instance)?);
        *slot = Some((head.clone(), d.clone()));
        Ok(d)
    }
}

/// Built-in source: diff the committed desired runtime against the snapshot.
pub struct HerdrSource {
    pub instance: std::path::PathBuf,
    pub cache: Arc<DesiredCache>,
}

impl EffectSource for HerdrSource {
    fn effects(&self, cx: &DiffCx<'_>) -> Vec<PlannedEffect> {
        match self.cache.get(cx.tree, cx.head, &self.instance) {
            Ok(d) => plan_effects(&d, cx.snapshot, cx.journal, cx.now),
            Err(e) => {
                eprintln!(
                    "herdr-graph: reconcile: cannot read desired state at {}: {e}",
                    cx.head.0
                );
                Vec::new()
            }
        }
    }
}

pub struct HerdrExecutor {
    pub journal: Arc<Journal>,
    pub cfg: ReconcilerConfig,
    pub cache: Arc<DesiredCache>,
}

fn create_outcome(e: HerdrError) -> ExecOutcome {
    match e {
        // The create may have happened: resolved by nonce lookup before any retry.
        HerdrError::Timeout => ExecOutcome::Unknown,
        other => transient_or_failed(other),
    }
}

fn idempotent(r: Result<(), HerdrError>) -> ExecOutcome {
    match r {
        Ok(()) => ExecOutcome::Done,
        Err(e) => transient_or_failed(e),
    }
}

/// Closing something already gone is success.
fn close_outcome(r: Result<(), HerdrError>) -> ExecOutcome {
    match r {
        Err(HerdrError::Rejected { message, .. }) if message.contains("not found") => {
            ExecOutcome::Done
        }
        other => idempotent(other),
    }
}

fn clone_of(e: &EffectRecord) -> Option<CloneId> {
    CloneId::parse(e.object.as_str()).ok()
}
fn seat_of(e: &EffectRecord) -> Option<SeatId> {
    SeatId::parse(e.object.as_str()).ok()
}
fn ts_of(e: &EffectRecord) -> Option<TeamspaceId> {
    TeamspaceId::parse(e.object.as_str()).ok()
}

impl HerdrExecutor {
    fn desired(&self, cx: &ExecCx<'_>) -> Result<Arc<DesiredRuntime>, StoreError> {
        self.cache.get(cx.tree, cx.head, &self.cfg.instance)
    }

    fn launched(&self, clone: &CloneId) -> Option<serde_json::Value> {
        let raw = self.journal.meta_get(&launched_key(clone)).ok().flatten()?;
        serde_json::from_str(&raw).ok()
    }

    fn record_launch(&self, p: &DesiredPane) {
        let _ = self
            .journal
            .meta_set(&launched_key(&p.clone), &p.launch_shape().to_string());
    }

    /// The variable names the start exported into `pane`'s shell, so a check-in that lacks them can be named
    /// (the shell may have rejected the export, spec §5).
    fn record_launch_env(&self, clone: &CloneId, pane: &HerdrPaneId, env: &[(String, String)]) {
        let vars: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        let _ = self.journal.meta_set(
            &launch_env_key(clone),
            &serde_json::json!({ "pane": pane.0, "vars": vars }).to_string(),
        );
    }

    /// A new pane for `clone` is a bare shell: forget the previous pane's launch so `start_needed` fires again.
    fn clear_launch(&self, clone: &CloneId) {
        let _ = self.journal.meta_delete(&launched_key(clone));
    }

    fn open_pane_id(&self, idx: &LiveIndex<'_>, clone: &CloneId) -> Option<HerdrPaneId> {
        idx.pane(clone).map(|lp| lp.pane.id.clone())
    }

    /// Spec §4.9 Reopening: the node manager's tab is back, so the seat is no longer moved out. Left set, the
    /// matcher would never link the reopened tab (it skips moved-out seats), and a later rebind would mark the
    /// seat Unknown and stop every further reopen.
    fn reclaim_home(&self, cx: &ExecCx<'_>, d: &DesiredRuntime, tab: &super::desired::DesiredTab) {
        if !tab.moved_out || d.node_manager.as_ref() != Some(&tab.seat) {
            return;
        }
        let object = tab.seat.to_any();
        if let Err(e) = admit_runtime(cx.writer, &object, Availability::Present, Some(false)) {
            eprintln!("herdr-graph: reconcile: moved_out write-back for {object} failed: {e}");
        }
    }

    fn write_binding(&self, cx: &ExecCx<'_>, object: &AnyId, binding: Binding) {
        if let Err(e) = admit_binding(cx.writer, object, &binding, Availability::Present) {
            eprintln!("herdr-graph: reconcile: binding write-back for {object} failed: {e}");
        }
    }

    /// Is `e` still implied by the committed state? (Fence, spec §4.4.)
    fn implied(&self, d: &DesiredRuntime, e: &EffectRecord) -> bool {
        match &e.kind {
            EffectKind::CreateWorkspace => ts_of(e)
                .and_then(|t| d.workspace(&t))
                .is_some_and(|w| !w.is_unknown() || d.restoring_teamspace(&w.ts)),
            EffectKind::RenameWorkspace => ts_of(e).and_then(|t| d.workspace(&t)).is_some(),
            EffectKind::CreateTab => seat_of(e)
                .and_then(|s| d.tab(&s))
                // Spec §4.9 Reopening: `moved_out` never gates the node manager's tab recreation.
                .is_some_and(|t| {
                    (!t.moved_out || d.node_manager.as_ref() == Some(&t.seat))
                        && (!t.is_unknown() || d.restoring_seat(&t.seat))
                }),
            EffectKind::RenameTab => seat_of(e).and_then(|s| d.tab(&s)).is_some(),
            EffectKind::StampToken => match e.object.kind() {
                crate::model::IdKind::Teamspace => ts_of(e).and_then(|t| d.workspace(&t)).is_some(),
                _ => clone_of(e).and_then(|c| d.pane(&c)).is_some(),
            },
            EffectKind::SplitPane => clone_of(e)
                .and_then(|c| d.pane(&c))
                .is_some_and(|p| !p.is_unknown()),
            EffectKind::RenamePane => clone_of(e).and_then(|c| d.pane(&c)).is_some(),
            EffectKind::StartAgent => clone_of(e)
                .and_then(|c| d.pane(&c))
                .is_some_and(|p| p.occupant.is_none()),
            EffectKind::RelaunchOccupant => clone_of(e)
                .and_then(|c| d.pane(&c))
                .is_some_and(|p| p.occupant.is_none()),
            EffectKind::ReplaceSession => clone_of(e)
                .and_then(|c| d.pane(&c).map(|p| (c, p)))
                .is_some_and(|(c, p)| {
                    // A replacement that has begun is finished even though the observer already recorded the old
                    // occupant as gone; one whose start outcome was lost (`Unknown`) is resolved from the snapshot.
                    (p.occupant.is_some()
                        || e.status == EffectStatus::Unknown
                        || session::has_state(&self.journal, &e.id))
                        && self.launched(&c).is_some_and(|l| l != p.launch_shape())
                }),
            EffectKind::ClosePane => clone_of(e).is_some_and(|c| d.pane(&c).is_none()),
            EffectKind::CloseTab => seat_of(e).is_some_and(|s| d.tab(&s).is_none()),
            EffectKind::CloseWorkspace => ts_of(e).is_some_and(|t| d.workspace(&t).is_none()),
            _ => true,
        }
    }

    async fn create_workspace(
        &self,
        cx: &ExecCx<'_>,
        d: &DesiredRuntime,
        e: &EffectRecord,
    ) -> ExecOutcome {
        let Some(ts) = ts_of(e) else {
            return ExecOutcome::Failed("create_workspace needs a teamspace".into());
        };
        let Some(w) = d.workspace(&ts) else {
            return ExecOutcome::Obsolete;
        };
        let idx = LiveIndex::new(cx.snapshot, d, &self.journal);
        let inc = cx.snapshot.incarnation.clone();
        if let Some(l) = idx.workspace(&ts) {
            set_live_ref(
                &self.journal,
                &e.object,
                &LiveRef {
                    workspace: Some(l.ws.id.clone()),
                    tab: None,
                    pane: None,
                    incarnation: inc,
                },
            );
            return ExecOutcome::Done;
        }
        let label = e
            .nonce_label
            .clone()
            .unwrap_or_else(|| nonce_label(&w.name, &e.id));
        // Lost-response recovery: a workspace carrying our nonce label is the one we created.
        if let Some(found) = cx.snapshot.workspaces.iter().find(|x| x.label == label) {
            set_live_ref(
                &self.journal,
                &e.object,
                &LiveRef {
                    workspace: Some(found.id.clone()),
                    tab: None,
                    pane: None,
                    incarnation: inc,
                },
            );
            return ExecOutcome::Done;
        }
        match cx
            .herdr
            .create_workspace(CreateWorkspace {
                label,
                cwd: w.cwd.clone(),
                env: w.env.clone(),
            })
            .await
        {
            Ok(c) => {
                crate::failpoint!(&format!("reconcile.after_call.{}", e.kind.as_str()));
                set_live_ref(
                    &self.journal,
                    &e.object,
                    &LiveRef {
                        workspace: c.workspace,
                        tab: None,
                        pane: None,
                        incarnation: inc,
                    },
                );
                ExecOutcome::Done
            }
            Err(err) => create_outcome(err),
        }
    }

    async fn create_tab(
        &self,
        cx: &ExecCx<'_>,
        d: &DesiredRuntime,
        e: &EffectRecord,
    ) -> ExecOutcome {
        let Some(seat) = seat_of(e) else {
            return ExecOutcome::Failed("create_tab needs a seat".into());
        };
        let Some(tab) = d.tab(&seat) else {
            return ExecOutcome::Obsolete;
        };
        let idx = LiveIndex::new(cx.snapshot, d, &self.journal);
        let inc = cx.snapshot.incarnation.clone();
        let Some(ws) = idx.workspace(&tab.ts) else {
            return ExecOutcome::Transient("workspace is not present yet".into());
        };
        let missing = idx.missing_clones(&seat);
        let first_clone = missing.first().map(|p| p.clone.clone());
        let adopt = |ws_id: &crate::model::HerdrWorkspaceId, t: &crate::ports::herdr::TabInfo| {
            let seat_ref = LiveRef {
                workspace: Some(ws_id.clone()),
                tab: Some(t.id.clone()),
                pane: None,
                incarnation: inc.clone(),
            };
            set_live_ref(&self.journal, &e.object, &seat_ref);
            if let (Some(c), Some(p)) = (&first_clone, t.panes.first()) {
                let r = LiveRef {
                    pane: Some(p.id.clone()),
                    ..seat_ref.clone()
                };
                set_live_ref(&self.journal, &c.to_any(), &r);
            }
            self.write_binding(
                cx,
                &e.object,
                Binding {
                    token: None,
                    workspace_id: Some(ws_id.clone()),
                    tab_id: Some(t.id.clone()),
                    pane_id: None,
                    terminal_id: None,
                    incarnation: inc.clone(),
                },
            );
        };
        if let Some((w, t)) = idx.tab_for_seat(&seat) {
            adopt(&w.id, t);
            self.reclaim_home(cx, d, tab);
            return ExecOutcome::Done;
        }
        let label = e
            .nonce_label
            .clone()
            .unwrap_or_else(|| nonce_label(&tab.name, &e.id));
        // Lost-response recovery: tabs have no tokens, so the nonce label is the only handle.
        if let Some(t) = ws.ws.tabs.iter().find(|t| t.label == label) {
            adopt(&ws.ws.id, t);
            self.reclaim_home(cx, d, tab);
            return ExecOutcome::Done;
        }
        let Some(first) = missing.first() else {
            return ExecOutcome::Obsolete;
        };
        self.clear_launch(&first.clone);
        match cx
            .herdr
            .create_tab(CreateTab {
                workspace: ws.ws.id.clone(),
                label,
                cwd: first.cwd.clone(),
                env: first.env.clone(),
            })
            .await
        {
            Ok(c) => {
                crate::failpoint!(&format!("reconcile.after_call.{}", e.kind.as_str()));
                let seat_ref = LiveRef {
                    workspace: c.workspace.clone(),
                    tab: c.tab.clone(),
                    pane: None,
                    incarnation: inc.clone(),
                };
                set_live_ref(&self.journal, &e.object, &seat_ref);
                set_live_ref(
                    &self.journal,
                    &first.clone.to_any(),
                    &LiveRef {
                        pane: c.pane.clone(),
                        ..seat_ref
                    },
                );
                self.write_binding(
                    cx,
                    &e.object,
                    Binding {
                        token: None,
                        workspace_id: c.workspace,
                        tab_id: c.tab,
                        pane_id: None,
                        terminal_id: None,
                        incarnation: inc,
                    },
                );
                self.reclaim_home(cx, d, tab);
                ExecOutcome::Done
            }
            Err(err) => create_outcome(err),
        }
    }

    async fn split_pane(
        &self,
        cx: &ExecCx<'_>,
        d: &DesiredRuntime,
        e: &EffectRecord,
    ) -> ExecOutcome {
        let Some(clone) = clone_of(e) else {
            return ExecOutcome::Failed("split_pane needs a clone".into());
        };
        let Some(p) = d.pane(&clone) else {
            return ExecOutcome::Obsolete;
        };
        let idx = LiveIndex::new(cx.snapshot, d, &self.journal);
        let inc = cx.snapshot.incarnation.clone();
        if idx.pane(&clone).is_some() {
            return ExecOutcome::Done;
        }
        let Some((ws, tab)) = idx.tab_for_seat(&p.seat) else {
            return ExecOutcome::Transient("seat tab is not present yet".into());
        };
        let mk_ref = |pane: HerdrPaneId| LiveRef {
            workspace: Some(ws.id.clone()),
            tab: Some(tab.id.clone()),
            pane: Some(pane),
            incarnation: inc.clone(),
        };
        if e.status == EffectStatus::Unknown {
            // A split whose response was lost left an unstamped pane that no sibling clone is bound to.
            let taken: BTreeSet<HerdrPaneId> = d
                .panes_of(&p.seat)
                .filter(|q| q.clone != clone)
                .filter_map(|q| self.open_pane_id(&idx, &q.clone))
                .collect();
            if let Some(found) = tab
                .panes
                .iter()
                .find(|x| token_of(&x.metadata).is_none() && !taken.contains(&x.id))
            {
                set_live_ref(&self.journal, &e.object, &mk_ref(found.id.clone()));
                return ExecOutcome::Done;
            }
        }
        self.clear_launch(&clone);
        let target = tab
            .panes
            .iter()
            .find(|x| token_of(&x.metadata).is_some())
            .or(tab.panes.first());
        let Some(target) = target else {
            return ExecOutcome::Transient("seat tab has no pane to split".into());
        };
        match cx
            .herdr
            .split_pane(SplitPane {
                target: target.id.clone(),
                direction: SplitDirection::Right,
                cwd: p.cwd.clone(),
                env: p.env.clone(),
            })
            .await
        {
            Ok(c) => {
                crate::failpoint!(&format!("reconcile.after_call.{}", e.kind.as_str()));
                if let Some(pane) = c.pane {
                    set_live_ref(&self.journal, &e.object, &mk_ref(pane));
                }
                ExecOutcome::Done
            }
            Err(err) => create_outcome(err),
        }
    }

    async fn stamp(&self, cx: &ExecCx<'_>, d: &DesiredRuntime, e: &EffectRecord) -> ExecOutcome {
        let idx = LiveIndex::new(cx.snapshot, d, &self.journal);
        let inc = cx.snapshot.incarnation.clone();
        let token = graph_token(&e.object);
        if let Some(ts) = ts_of(e) {
            let Some(l) = idx.workspace(&ts) else {
                return ExecOutcome::Transient("workspace is not present".into());
            };
            let id = l.ws.id.clone();
            if let Err(err) = cx
                .herdr
                .report_workspace_metadata(&id, TOKEN_KEY, &token)
                .await
            {
                return transient_or_failed(err);
            }
            self.write_binding(
                cx,
                &e.object,
                Binding {
                    token: Some(token),
                    workspace_id: Some(id),
                    tab_id: None,
                    pane_id: None,
                    terminal_id: None,
                    incarnation: inc,
                },
            );
            return ExecOutcome::Done;
        }
        let Some(clone) = clone_of(e) else {
            return ExecOutcome::Failed("stamp_token needs a teamspace or clone".into());
        };
        let Some(lp) = idx.pane(&clone) else {
            return ExecOutcome::Transient("pane is not present".into());
        };
        let (ws, tab, pane) = (lp.ws.id.clone(), lp.tab.id.clone(), lp.pane.clone());
        if let Err(err) = cx
            .herdr
            .report_pane_metadata(&pane.id, TOKEN_KEY, &token)
            .await
        {
            return transient_or_failed(err);
        }
        // Any other clone's live ref that still names this pane is stale now (the pane was adopted).
        for c in d.clones.keys().filter(|c| **c != clone) {
            if live_ref(&self.journal, &c.to_any())
                .is_some_and(|l| l.pane.as_ref() == Some(&pane.id))
            {
                delete_live_ref(&self.journal, &c.to_any());
            }
        }
        self.write_binding(
            cx,
            &e.object,
            Binding {
                token: Some(token),
                workspace_id: Some(ws),
                tab_id: Some(tab),
                pane_id: Some(pane.id),
                terminal_id: pane.terminal_id,
                incarnation: inc,
            },
        );
        ExecOutcome::Done
    }

    async fn start(
        &self,
        cx: &ExecCx<'_>,
        d: &DesiredRuntime,
        e: &EffectRecord,
        relaunch: bool,
    ) -> ExecOutcome {
        let Some(clone) = clone_of(e) else {
            return ExecOutcome::Failed("start needs a clone".into());
        };
        let Some(p) = d.pane(&clone) else {
            return ExecOutcome::Obsolete;
        };
        let prof = profile(p.harness);
        let Some(kind) = prof.agent_kind else {
            return ExecOutcome::Done;
        };
        let idx = LiveIndex::new(cx.snapshot, d, &self.journal);
        // The resume driver launches into the clone's last bound pane, which a restart may have left unstamped.
        let Some(lp) = idx.pane(&clone).or_else(|| idx.bound_pane(&clone)) else {
            return ExecOutcome::Transient("pane is not present".into());
        };
        if relaunch && let Some(wait) = self.grace_remaining(cx) {
            return ExecOutcome::DeferredUntil(
                cx.now + wait,
                format!(
                    "{}s of relaunch grace left after the incarnation change",
                    wait.num_seconds().max(1)
                ),
            );
        }
        // §4.1 readiness: no agent on the pane and the foreground is the shell.
        if lp.pane.agent.is_some() {
            if e.status == EffectStatus::Unknown {
                // The earlier start went through; only its response was lost.
                self.record_launch(p);
                self.record_launch_env(
                    &clone,
                    &lp.pane.id,
                    &agent_env(&clone, p.key_reservation.is_some()),
                );
                return ExecOutcome::Done;
            }
            return ExecOutcome::NeedsRevision("agent already running on the pane".into());
        }
        match cx.herdr.process_info(&lp.pane.id).await {
            Ok(pi) if pi.is_shell => {}
            Ok(_) => return ExecOutcome::NeedsRevision("pane foreground is not the shell".into()),
            Err(err) => return transient_or_failed(err),
        }
        // A pending Restore launches once per epoch: the mark is admitted before the dispatch, and a clone whose
        // epoch is already marked (or acknowledged by no one) is never started here.
        // A restart relaunch of a Restore launched before the incarnation change is an ordinary restart relaunch.
        let restart_owned = relaunch
            && incarnation_changed_at(&self.journal)
                .is_some_and(|at| launched_before(&p.seat_intent, at));
        if !restart_owned
            && let Some(SeatIntent::Restore {
                epoch,
                threads_done,
                launched,
                ..
            }) = &p.seat_intent
        {
            // Stale: not acknowledged, already launched, or the intent moved to another epoch than this effect's.
            if !*threads_done || launched.is_some() || *epoch != e.op {
                return ExecOutcome::Obsolete;
            }
            let failed_key = restore_mark_failed_key(&clone);
            if let Err(err) = admit_threads_seat(
                cx.writer,
                &clone,
                &SeatTransition::MarkLaunched {
                    epoch: epoch.clone(),
                },
            ) {
                // The intent is unchanged: nothing launches, and the planner retries with a new identity.
                let failures = restore_mark_failures(&self.journal, &clone, epoch) + 1;
                let _ = self.journal.meta_set(
                    &failed_key,
                    &serde_json::json!({
                        "epoch": epoch.to_string(),
                        "error": err.to_string(),
                        "failures": failures,
                    })
                    .to_string(),
                );
                return ExecOutcome::Failed(format!("restore launch mark: {err}"));
            }
            let _ = self.journal.meta_delete(&failed_key);
        }
        let args = prof.argv(p.model.as_deref(), p.resume.as_deref(), &p.args);
        let env = agent_env(&clone, p.key_reservation.is_some());
        let started = cx
            .herdr
            .start_agent(StartAgent {
                pane: lp.pane.id.clone(),
                kind: kind.to_owned(),
                args,
                env: env.clone(),
            })
            .await;
        crate::failpoint!(&format!("reconcile.after_call.{}", e.kind.as_str()));
        let out = start_outcome(started);
        if matches!(out, ExecOutcome::Done | ExecOutcome::BlockedNeedsHuman) {
            self.record_launch(p);
            self.record_launch_env(&clone, &lp.pane.id, &env);
            if relaunch && let Some(at) = incarnation_changed_at(&self.journal) {
                // One restart relaunch per incarnation change.
                let _ = self
                    .journal
                    .meta_set(&resumed_key(&clone), &at.to_rfc3339());
            }
        }
        out
    }

    /// Close every late copy of `e`'s object that carries `e`'s nonce label and is still as Herdr created it
    /// with a shell in the foreground. A copy someone has started using is left alone and reported.
    async fn close_duplicate(
        &self,
        cx: &ExecCx<'_>,
        idx: &LiveIndex<'_>,
        e: &EffectRecord,
    ) -> ExecOutcome {
        let Some(label) = e.nonce_label.as_deref() else {
            return ExecOutcome::Failed("close_duplicate needs the copy's nonce label".into());
        };
        let copies = idx.duplicates();
        for dup in copies
            .iter()
            .filter(|d| d.object == e.object && d.copy.label() == label)
        {
            let what = match dup.copy {
                Surplus::Workspace(_) => "workspace",
                Surplus::Tab(_) => "tab",
            };
            let in_use = || {
                ExecOutcome::NeedsRevision(format!(
                    "duplicate {what} {label:?} from a lost create is in use; close it if it is not wanted"
                ))
            };
            let Some(pane) = dup.copy.pristine_pane() else {
                return in_use();
            };
            match cx.herdr.process_info(&pane.id).await {
                Ok(pi) if pi.is_shell => {}
                Ok(_) => return in_use(),
                Err(err) => return close_outcome(Err(err)),
            }
            let closed = match dup.copy {
                Surplus::Workspace(w) => cx.herdr.close_workspace(&w.id).await,
                Surplus::Tab(t) => cx.herdr.close_tab(&t.id).await,
            };
            match close_outcome(closed) {
                ExecOutcome::Done => {}
                other => return other,
            }
        }
        ExecOutcome::Done
    }

    /// Time left of the 90 s grace after the last incarnation change (r2), if any.
    fn grace_remaining(&self, cx: &ExecCx<'_>) -> Option<chrono::Duration> {
        let raw = self
            .journal
            .meta_get("incarnation:changed_at")
            .ok()
            .flatten()?;
        let at = chrono::DateTime::parse_from_rfc3339(&raw).ok()?.to_utc();
        let until = at + chrono::Duration::from_std(self.cfg.relaunch_grace).ok()?;
        (cx.now < until).then(|| until - cx.now)
    }

    async fn replace(&self, cx: &ExecCx<'_>, d: &DesiredRuntime, e: &EffectRecord) -> ExecOutcome {
        let Some(clone) = clone_of(e) else {
            return ExecOutcome::Failed("replace_session needs a clone".into());
        };
        let Some(p) = d.pane(&clone) else {
            return ExecOutcome::Obsolete;
        };
        let idx = LiveIndex::new(cx.snapshot, d, &self.journal);
        let Some(lp) = idx.pane(&clone) else {
            return ExecOutcome::Transient("pane is not present".into());
        };
        let Some(from) = self.launched(&clone).and_then(|l| {
            serde_json::from_value::<Harness>(l.get("harness").cloned().unwrap_or_default()).ok()
        }) else {
            return ExecOutcome::Obsolete;
        };
        let to_prof = profile(p.harness);
        // Once the observer has recorded the occupancy end, `p.resume` is the session that just ended.
        let resume = (from == p.harness && to_prof.supports_resume())
            .then(|| p.occupant_native_id.clone().or_else(|| p.resume.clone()))
            .flatten();
        let env = agent_env(&clone, p.key_reservation.is_some());
        let to = to_prof.agent_kind.map(|kind| Relaunch {
            kind,
            args: to_prof.argv(p.model.as_deref(), resume.as_deref(), &p.args),
            env: env.clone(),
        });
        let out = advance_replacement(
            cx.herdr,
            &self.cfg,
            &self.journal,
            &e.id,
            &lp.pane.id,
            from,
            to,
            cx.now,
        )
        .await;
        if matches!(out, ExecOutcome::Done | ExecOutcome::BlockedNeedsHuman) {
            self.record_launch(p);
            if to_prof.agent_kind.is_some() {
                self.record_launch_env(&clone, &lp.pane.id, &env);
            }
        }
        out
    }
}

#[async_trait::async_trait]
impl EffectExecutor for HerdrExecutor {
    fn handles(&self, kind: &EffectKind) -> bool {
        matches!(
            kind,
            EffectKind::CreateWorkspace
                | EffectKind::CreateTab
                | EffectKind::SplitPane
                | EffectKind::StampToken
                | EffectKind::StartAgent
                | EffectKind::RenameWorkspace
                | EffectKind::RenameTab
                | EffectKind::RenamePane
                | EffectKind::CloseWorkspace
                | EffectKind::ClosePane
                | EffectKind::CloseTab
                | EffectKind::CloseDuplicate
                | EffectKind::ReplaceSession
                | EffectKind::RelaunchOccupant
        )
    }

    fn is_implied(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> bool {
        match self.desired(cx) {
            Ok(d) => self.implied(&d, e),
            Err(_) => true,
        }
    }

    async fn execute(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome {
        let d = match self.desired(cx) {
            Ok(d) => d,
            Err(err) => {
                return ExecOutcome::Transient(format!("cannot read committed state: {err}"));
            }
        };
        if !self.implied(&d, e) {
            return ExecOutcome::Obsolete;
        }
        let idx = LiveIndex::new(cx.snapshot, &d, &self.journal);
        match &e.kind {
            EffectKind::CreateWorkspace => self.create_workspace(cx, &d, e).await,
            EffectKind::CreateTab => self.create_tab(cx, &d, e).await,
            EffectKind::SplitPane => self.split_pane(cx, &d, e).await,
            EffectKind::StampToken => self.stamp(cx, &d, e).await,
            EffectKind::StartAgent => self.start(cx, &d, e, false).await,
            EffectKind::RelaunchOccupant => self.start(cx, &d, e, true).await,
            EffectKind::ReplaceSession => self.replace(cx, &d, e).await,
            EffectKind::RenameWorkspace => {
                let Some(ts) = ts_of(e) else {
                    return ExecOutcome::Failed("needs a teamspace".into());
                };
                let (Some(w), Some(l)) = (d.workspace(&ts), idx.workspace(&ts)) else {
                    return ExecOutcome::Transient("workspace is not present".into());
                };
                if l.ws.label == w.name {
                    return ExecOutcome::Done;
                }
                idempotent(cx.herdr.rename_workspace(&l.ws.id, &w.name).await)
            }
            EffectKind::RenameTab => {
                let Some(seat) = seat_of(e) else {
                    return ExecOutcome::Failed("needs a seat".into());
                };
                let (Some(t), Some((_, live))) = (d.tab(&seat), idx.tab_for_seat(&seat)) else {
                    return ExecOutcome::Transient("tab is not present".into());
                };
                if live.label == t.name {
                    return ExecOutcome::Done;
                }
                idempotent(cx.herdr.rename_tab(&live.id, &t.name).await)
            }
            EffectKind::RenamePane => {
                let Some(clone) = clone_of(e) else {
                    return ExecOutcome::Failed("needs a clone".into());
                };
                let (Some(p), Some(lp)) = (d.pane(&clone), idx.pane(&clone)) else {
                    return ExecOutcome::Transient("pane is not present".into());
                };
                if lp.pane.label.as_deref() == Some(p.name.as_str()) {
                    return ExecOutcome::Done;
                }
                idempotent(cx.herdr.rename_pane(&lp.pane.id, &p.name).await)
            }
            EffectKind::ClosePane => {
                let Some(clone) = clone_of(e) else {
                    return ExecOutcome::Done;
                };
                let Some(lp) = idx.pane(&clone) else {
                    return ExecOutcome::Done;
                };
                if idx.pane_claimed_by(&lp.pane.id, &clone).is_some() {
                    // The pane belongs to another clone's committed binding now (adopted by an undo).
                    delete_live_ref(&self.journal, &clone.to_any());
                    return ExecOutcome::Obsolete;
                }
                close_outcome(cx.herdr.close_pane(&lp.pane.id).await)
            }
            EffectKind::CloseTab => {
                let Some((_, t)) = seat_of(e).and_then(|s| idx.tab_for_seat(&s)) else {
                    return ExecOutcome::Done;
                };
                close_outcome(cx.herdr.close_tab(&t.id).await)
            }
            EffectKind::CloseWorkspace => {
                let Some(l) = ts_of(e).and_then(|t| idx.workspace(&t)) else {
                    return ExecOutcome::Done;
                };
                close_outcome(cx.herdr.close_workspace(&l.ws.id).await)
            }
            EffectKind::CloseDuplicate => self.close_duplicate(cx, &idx, e).await,
            other => {
                ExecOutcome::Failed(format!("herdr executor does not handle {}", other.as_str()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OpId;
    use crate::reconcile::desired::DesiredTab;

    fn create_tab_row(seat: &SeatId) -> EffectRecord {
        let op = OpId::new();
        let object = seat.to_any();
        EffectRecord {
            id: EffectRecord::identity(&op, &object, &EffectKind::CreateTab, 1),
            op,
            object,
            kind: EffectKind::CreateTab,
            object_rev: 1,
            fencing_rev: 1,
            status: EffectStatus::Pending,
            predicted: vec![],
            nonce_label: None,
            attempts: 0,
            last_error: None,
            updated_at: chrono::Utc::now(),
            sched: Default::default(),
        }
    }

    fn desired_with_tab(seat: &SeatId, node_manager: bool, moved_out: bool) -> DesiredRuntime {
        DesiredRuntime {
            tabs: vec![DesiredTab {
                seat: seat.clone(),
                ts: TeamspaceId::new(),
                name: "seat".into(),
                cwd: "/".into(),
                env: vec![],
                moved_out,
                bound: None,
                availability: Availability::Present,
                rev: 1,
            }],
            node_manager: node_manager.then(|| seat.clone()),
            ..Default::default()
        }
    }

    /// Spec §4.9 Reopening: `moved_out` never gates the node manager's tab recreation; every other seat keeps
    /// the gate.
    #[test]
    fn moved_out_node_manager_create_tab_is_implied() {
        let dir = tempfile::tempdir().unwrap();
        let exec = HerdrExecutor {
            journal: Arc::new(Journal::open(&dir.path().join("journal.sqlite3")).unwrap()),
            cfg: ReconcilerConfig::new(dir.path().to_path_buf()),
            cache: Arc::new(DesiredCache::default()),
        };
        let seat = SeatId::new();
        let row = create_tab_row(&seat);
        assert!(
            exec.implied(&desired_with_tab(&seat, true, true), &row),
            "a moved-out node manager's CreateTab is not cancelled"
        );
        assert!(
            !exec.implied(&desired_with_tab(&seat, false, true), &row),
            "a moved-out plain seat's CreateTab is cancelled"
        );
        assert!(
            exec.implied(&desired_with_tab(&seat, false, false), &row),
            "a seat that is not moved out keeps its CreateTab"
        );
    }

    fn create_workspace_row(ts: &TeamspaceId) -> EffectRecord {
        EffectRecord {
            object: ts.to_any(),
            kind: EffectKind::CreateWorkspace,
            ..create_tab_row(&SeatId::new())
        }
    }

    /// Spec section 8 step 1 over the Unknown rule (section 4.3.2): a seat whose clone carries a pane-less Restore
    /// reopens its Unknown tab and workspace; without the Restore the old rule holds.
    #[test]
    fn create_tab_and_workspace_of_a_restoring_seat_are_implied_while_unknown() {
        use crate::model::clone::SeatIntent;
        use crate::model::harness::Harness;
        use crate::reconcile::desired::{DesiredPane, DesiredWorkspace};
        let dir = tempfile::tempdir().unwrap();
        let exec = HerdrExecutor {
            journal: Arc::new(Journal::open(&dir.path().join("journal.sqlite3")).unwrap()),
            cfg: ReconcilerConfig::new(dir.path().to_path_buf()),
            cache: Arc::new(DesiredCache::default()),
        };
        let seat = SeatId::new();
        let ts = TeamspaceId::new();
        let bound = Some(Binding::default());
        let mut d = desired_with_tab(&seat, false, false);
        d.tabs[0].ts = ts.clone();
        d.tabs[0].bound = bound.clone();
        d.tabs[0].availability = Availability::Unknown;
        d.workspaces.push(DesiredWorkspace {
            ts: ts.clone(),
            name: "ts".into(),
            cwd: "/".into(),
            env: vec![],
            bound,
            availability: Availability::Unknown,
            rev: 1,
        });
        d.panes.push(DesiredPane {
            clone: CloneId::new(),
            seat: seat.clone(),
            ts: ts.clone(),
            name: "c".into(),
            cwd: "/".into(),
            env: vec![],
            harness: Harness::Shell,
            model: None,
            args: vec![],
            resume: None,
            occupant: None,
            occupant_native_id: None,
            bound: None,
            availability: Availability::Present,
            rev: 1,
            threads_seat: Some("seat-1".into()),
            seat_fact: None,
            seat_intent: Some(SeatIntent::Restore {
                seat: "seat-1".into(),
                epoch: OpId::new(),
                threads_done: false,
                launched: None,
                last_refusal: None,
            }),
            key_reservation: None,
            had_agent: false,
        });
        let tab_row = create_tab_row(&seat);
        let ws_row = create_workspace_row(&ts);
        assert!(exec.implied(&d, &tab_row), "a restoring seat's CreateTab");
        assert!(
            exec.implied(&d, &ws_row),
            "a restoring seat's CreateWorkspace"
        );
        d.panes[0].seat_intent = None;
        assert!(
            !exec.implied(&d, &tab_row),
            "no Restore: Unknown gets no tab"
        );
        assert!(
            !exec.implied(&d, &ws_row),
            "no Restore: Unknown gets no workspace"
        );
    }
}
