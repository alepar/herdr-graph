//! Single serialized writer: recheck → tree build → CAS commit → worktree FF (spec §3.5–3.6).
//! Implements crate::ports::writer::Writer. Owned by hg-zmi.3.
pub mod commit;
pub mod mutation;
pub mod recovery;
pub mod worktree;
#[cfg(test)]
mod tests;

use crate::failpoint;
use crate::journal::{Journal, JournalError, OpRow};
use crate::model::change::{ReliedOn, RequestKind, Version};
use crate::model::operation::{OpState, OperationRecord};
use crate::model::{ActionId, CommitId, OpId, PlanId, Timestamp};
use crate::ports::clock::Clock;
use crate::ports::store::{RepoPath, Store, StoreError};
use crate::ports::writer::{Writer, WriterError};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::{GitStore, Overlay};
use git2::Oid;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

pub use mutation::*;

#[derive(Debug, Clone)]
pub struct WriterConfig {
    /// An op is poison (state `failed`) once it has been applied this many times without finishing.
    pub max_attempts: u32,
    /// Consecutive infrastructure failures before `run` halts the writer.
    pub infra_retries: u32,
    pub infra_backoff: Duration,
    pub lock_retries: u32,
    pub lock_backoff: Duration,
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            infra_retries: 5,
            infra_backoff: Duration::from_millis(200),
            lock_retries: 5,
            lock_backoff: Duration::from_millis(200),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpEvent {
    pub op: OpId,
    pub kind: RequestKind,
    pub state: OpState,
    pub commit: Option<CommitId>,
    pub action: Option<ActionId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    Idle,
    Committed(OpId, CommitId),
    Rejected(OpId),
    Failed(OpId),
    Skipped(OpId),
}

pub const WRITER_HALTED: &str = "writer_halted";

/// Why a step stopped before the ref moved.
enum Fail {
    /// Retryable: the op goes back to `admitted` without counting an attempt.
    Infra(String),
    /// Stop the writer; the op stays `applying` and recovery requeues it.
    Halt(String),
}

impl From<StoreError> for Fail {
    fn from(e: StoreError) -> Self {
        Fail::Infra(e.to_string())
    }
}
impl From<JournalError> for Fail {
    fn from(e: JournalError) -> Self {
        Fail::Infra(e.to_string())
    }
}
impl From<git2::Error> for Fail {
    fn from(e: git2::Error) -> Self {
        Fail::Infra(e.to_string())
    }
}

/// What the pre-CAS phase produced.
enum Phase {
    Done(StepOutcome),
    Committed { commit: Oid, applied: Applied, old_tree: Oid, new_tree: Oid },
}

fn journal_err(e: JournalError) -> WriterError {
    WriterError::Journal(e.to_string())
}

pub struct WriterCore {
    store: Arc<GitStore>,
    journal: Arc<Journal>,
    registry: Arc<MutationRegistry>,
    clock: Arc<dyn Clock>,
    cfg: WriterConfig,
    events: tokio::sync::broadcast::Sender<OpEvent>,
    wake: Arc<tokio::sync::Notify>,
    infra_failures: AtomicU32,
}

impl WriterCore {
    pub fn new(
        store: Arc<GitStore>,
        journal: Arc<Journal>,
        registry: Arc<MutationRegistry>,
        clock: Arc<dyn Clock>,
        cfg: WriterConfig,
    ) -> Arc<Self> {
        let (events, _) = tokio::sync::broadcast::channel(256);
        Arc::new(Self {
            store,
            journal,
            registry,
            clock,
            cfg,
            events,
            wake: Arc::new(tokio::sync::Notify::new()),
            infra_failures: AtomicU32::new(0),
        })
    }

    pub fn journal(&self) -> &Arc<Journal> {
        &self.journal
    }

    pub fn store(&self) -> &Arc<GitStore> {
        &self.store
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<OpEvent> {
        self.events.subscribe()
    }

    fn emit(&self, row: &OpRow, state: OpState, commit: Option<CommitId>, action: Option<ActionId>) {
        // No subscribers is fine.
        let _ = self.events.send(OpEvent { op: row.op.clone(), kind: row.request.kind, state, commit, action });
    }

    /// Process exactly one admitted op (sync; git2 + rusqlite). Err(Halted) when writer_halted is set.
    pub fn step(&self) -> Result<StepOutcome, WriterError> {
        if let Some(reason) = self.journal.meta_get(WRITER_HALTED).map_err(journal_err)? {
            return Err(WriterError::Halted(reason));
        }
        let Some(row) = self.journal.next_admitted().map_err(journal_err)? else {
            return Ok(StepOutcome::Idle);
        };
        let now = self.clock.now();
        let Some(attempts) = self.journal.begin_applying(&row.op, now).map_err(journal_err)? else {
            return Ok(StepOutcome::Skipped(row.op));
        };
        if attempts > self.cfg.max_attempts {
            let reason = format!("poison: exceeded {} attempts", self.cfg.max_attempts);
            self.journal.finish_failed(&row.op, &reason, now).map_err(journal_err)?;
            self.emit(&row, OpState::Failed, None, None);
            return Ok(StepOutcome::Failed(row.op));
        }

        match self.apply_phase(&row, now) {
            Ok(Phase::Done(outcome)) => {
                self.infra_failures.store(0, Ordering::SeqCst);
                Ok(outcome)
            }
            Ok(Phase::Committed { commit, applied, old_tree, new_tree }) => {
                self.infra_failures.store(0, Ordering::SeqCst);
                self.finish_commit(&row, now, commit, applied, old_tree, new_tree)
            }
            Err(Fail::Halt(reason)) => {
                self.journal.meta_set(WRITER_HALTED, &reason).map_err(journal_err)?;
                Err(WriterError::Halted(reason))
            }
            Err(Fail::Infra(msg)) => {
                // Best effort: if even this fails, recovery requeues the stuck `applying` op.
                let _ = self.journal.requeue(&row.op, false, now);
                self.infra_failures.fetch_add(1, Ordering::SeqCst);
                Err(WriterError::Journal(msg))
            }
        }
    }

    /// Steps 3-9: recheck, mutation, tree build, commit object, CAS of refs/heads/main.
    fn apply_phase(&self, row: &OpRow, now: Timestamp) -> Result<Phase, Fail> {
        let op = &row.op;
        let req = &row.request;
        let head = self.store.head()?;

        if let Some(reject) = self.recheck_relied_on(&head, req)? {
            self.journal.finish_rejected(op, &reject.into(), now)?;
            self.emit(row, OpState::Rejected, None, None);
            return Ok(Phase::Done(StepOutcome::Rejected(op.clone())));
        }

        let key = mutation_key(req);
        let Some(mutation) = self.registry.get(&key) else {
            self.journal.finish_failed(op, &format!("no mutation registered for {key}"), now)?;
            self.emit(row, OpState::Failed, None, None);
            return Ok(Phase::Done(StepOutcome::Failed(op.clone())));
        };

        let store: &dyn Store = &*self.store;
        let mut cx = MutationCx { tree: Overlay::new(store, head.clone()), op: op.clone(), now, request: req };
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| mutation.apply(&mut cx)));
        let applied = match result {
            Err(panic) => {
                let what = panic
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_owned())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "non-string panic".to_owned());
                return self.fail_op(row, now, &format!("mutation {key} panicked: {what}"));
            }
            Ok(Err(MutationError::Bug(why))) => return self.fail_op(row, now, &format!("mutation {key}: {why}")),
            Ok(Err(MutationError::Reject(r))) => {
                self.journal.finish_rejected(op, &r.into(), now)?;
                self.emit(row, OpState::Rejected, None, None);
                return Ok(Phase::Done(StepOutcome::Rejected(op.clone())));
            }
            Ok(Err(MutationError::Store(e))) => return Err(e.into()),
            Ok(Ok(applied)) => applied,
        };

        self.write_operation_record(&mut cx, row, &applied)?;
        let new_tree = self.store.build_tree(&head, cx.tree.edits())?;
        failpoint!("writer.after_tree_build");

        let kind = mutation::kind_name(req.kind);
        let summary = applied.summary.replace(['\n', '\r'], " ");
        let mut message = format!("{kind}: {summary}\n\nGraph-Op: {op}\n");
        if let Some(act) = &applied.action {
            message.push_str(&format!("Graph-Action: {act}\n"));
        }
        let head_oid = Oid::from_str(&head.0).map_err(|e| Fail::Infra(e.to_string()))?;
        let cfg = &self.cfg;
        let (new_commit, old_tree) = self.store.with_repo(|repo| {
            let old_tree = repo.find_commit(head_oid)?.tree_id();
            let c = commit::create_commit(repo, head_oid, new_tree, &message)?;
            Ok((c, old_tree))
        })?;
        let cas = self.store.with_repo(|repo| Ok(commit::cas_main(repo, head_oid, new_commit, &message, cfg)))?;
        match cas {
            Ok(()) => Ok(Phase::Committed { commit: new_commit, applied, old_tree, new_tree }),
            Err(commit::CasError::LockContention) => Err(Fail::Halt("git lock contention on refs/heads/main".into())),
            Err(e) => Err(Fail::Infra(e.to_string())),
        }
    }

    fn fail_op(&self, row: &OpRow, now: Timestamp, reason: &str) -> Result<Phase, Fail> {
        self.journal.finish_failed(&row.op, reason, now)?;
        self.emit(row, OpState::Failed, None, None);
        Ok(Phase::Done(StepOutcome::Failed(row.op.clone())))
    }

    /// Steps 11-12, after the ref moved: journal, supersede, working-tree view. The commit exists, so errors
    /// here must not requeue; recovery completes the journal from the trailer.
    fn finish_commit(
        &self,
        row: &OpRow,
        now: Timestamp,
        commit: Oid,
        applied: Applied,
        old_tree: Oid,
        new_tree: Oid,
    ) -> Result<StepOutcome, WriterError> {
        let op = &row.op;
        let commit_id = CommitId(commit.to_string());
        failpoint!("writer.after_cas_before_journal");
        self.journal.finish_committed(op, &commit_id, applied.action.as_ref(), now).map_err(journal_err)?;
        if let Some(old) = &row.request.supersedes
            && let Err(e) = self.journal.supersede(old, op, now)
        {
            eprintln!("herdr-graph: could not mark {old} superseded by {op}: {e}");
        }
        failpoint!("writer.after_journal_before_ff");
        let root = self.store.root();
        let from = worktree::view_rev(root)
            .and_then(|v| Oid::from_str(&v.0).ok())
            .and_then(|oid| self.store.with_repo(|r| Ok(r.find_commit(oid)?.tree_id())).ok())
            .unwrap_or(old_tree);
        let ff = self
            .store
            .with_repo(|repo| Ok(worktree::fast_forward(repo, root, Some(from), new_tree, &commit_id, Some(op), now)));
        match ff {
            Ok(Ok(report)) if !report.dirty.is_empty() => {
                eprintln!("herdr-graph: worktree_dirty: {} file(s) left untouched after {op}", report.dirty.len());
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) | Err(e) => eprintln!("herdr-graph: working-tree fast-forward failed after {op}: {e}"),
        }
        self.emit(row, OpState::Committed, Some(commit_id.clone()), applied.action);
        Ok(StepOutcome::Committed(op.clone(), commit_id))
    }

    /// Step 3: every `relied_on` must still hold at the committed head. Returns the rejection if not.
    fn recheck_relied_on(&self, head: &CommitId, req: &crate::model::change::ChangeRequest) -> Result<Option<Reject>, StoreError> {
        let mut stale = Vec::new();
        let mut current = Vec::new();
        for r in &req.relied_on {
            let now_version = self.current_version(head, r, req)?;
            if now_version.as_ref() != Some(&r.version) {
                let shown = match &now_version {
                    Some(Version::Rev(v)) => format!("rev {v}"),
                    Some(Version::Blob(h)) => format!("blob {}", h.0),
                    None => "missing".to_owned(),
                };
                let expected = match &r.version {
                    Version::Rev(v) => format!("rev {v}"),
                    Version::Blob(h) => format!("blob {}", h.0),
                };
                stale.push(format!("{} expected {expected} but committed is {shown}", r.object));
                if let Some(v) = now_version {
                    current.push(ReliedOn { object: r.object.clone(), version: v });
                }
            }
        }
        if stale.is_empty() {
            return Ok(None);
        }
        Ok(Some(Reject {
            reason: "precondition_failed".into(),
            explanation: stale.join("; "),
            current_revs: current,
        }))
    }

    fn current_version(
        &self,
        head: &CommitId,
        r: &ReliedOn,
        req: &crate::model::change::ChangeRequest,
    ) -> Result<Option<Version>, StoreError> {
        let Some(loc) = self.store.locate(head, &r.object)? else { return Ok(None) };
        match &r.version {
            Version::Rev(_) => {
                let table: Option<toml::Table> = read_toml(&self.store.at(head), &loc.record_path)?;
                Ok(table.and_then(|t| t.get("rev").and_then(|v| v.as_integer())).map(|v| Version::Rev(v as u64)))
            }
            Version::Blob(_) => {
                let Some(rel) = req.args.get("path").and_then(|v| v.as_str()) else { return Ok(None) };
                let path = loc.folder.join(rel)?;
                Ok(self.store.blob_hash(head, &path)?.map(Version::Blob))
            }
        }
    }

    /// Step 6: `operations/<yyyy-mm>/<op>.toml` rides in the same commit as the change.
    fn write_operation_record(&self, cx: &mut MutationCx<'_>, row: &OpRow, applied: &Applied) -> Result<(), StoreError> {
        let req = &row.request;
        let mut rec = OperationRecord {
            schema: crate::model::SCHEMA_VERSION,
            id: row.op.clone(),
            rev: 0,
            kind: req.kind,
            summary: applied.summary.clone(),
            requester: req.requester.clone(),
            state: OpState::Committed,
            confirmation: req.args.get("_confirmation").and_then(|v| serde_json::from_value(v.clone()).ok()),
            plan: req.args.get("_plan").and_then(|v| v.as_str()).and_then(|s| s.parse::<PlanId>().ok()),
            commit: None, // a commit cannot contain its own oid
            action: applied.action.clone(),
            supersedes: req.supersedes.clone(),
            superseded_by: None,
            admitted_at: row.admitted_at,
            finished_at: Some(cx.now),
            rejection: None,
        };
        let path: RepoPath = layout::operation_record(cx.now, &row.op);
        cx.tree.put_record(path, &mut rec)
    }

    /// step() until Idle.
    pub fn drain(&self) -> Result<Vec<StepOutcome>, WriterError> {
        let mut out = Vec::new();
        loop {
            match self.step()? {
                StepOutcome::Idle => return Ok(out),
                o => out.push(o),
            }
        }
    }

    /// Spec §3.6; the caller holds the daemon flock (hg-zmi.4).
    pub fn recover(&self) -> Result<recovery::RecoveryReport, WriterError> {
        let now = self.clock.now();
        let mut report = recovery::RecoveryReport::default();
        let to_store = |e: StoreError| WriterError::Journal(e.to_string());
        let (head_oid, removed) = self
            .store
            .with_repo(|repo| Ok((repo.refname_to_id(commit::MAIN_REF)?, recovery::remove_stale_git_locks(repo))))
            .map_err(to_store)?;
        report.removed_locks = removed;

        let checkpoint = self.journal.checkpoint().map_err(journal_err)?.and_then(|c| Oid::from_str(&c.0).ok());
        let trailers = self
            .store
            .with_repo(|repo| recovery::trailers_since(repo, head_oid, checkpoint))
            .map_err(to_store)?;
        for (op, commit, action) in trailers {
            let Some(row) = self.journal.get(&op).map_err(journal_err)? else { continue };
            if !matches!(row.state, OpState::Applying | OpState::Admitted) {
                continue;
            }
            self.journal.finish_committed(&op, &commit, action.as_ref(), now).map_err(journal_err)?;
            if let Some(old) = &row.request.supersedes {
                let _ = self.journal.supersede(old, &op, now);
            }
            report.marked_committed.push(op);
        }
        for row in self.journal.list(&[OpState::Applying], usize::MAX >> 1).map_err(journal_err)? {
            // A crash mid-apply counts as an attempt, so a crash-looping op becomes poison.
            self.journal.requeue(&row.op, true, now).map_err(journal_err)?;
            report.requeued.push(row.op);
        }
        let head = CommitId(head_oid.to_string());
        self.journal.set_checkpoint(&head).map_err(journal_err)?;
        report.checkpoint = Some(head.clone());

        let root = self.store.root();
        let from = worktree::view_rev(root).and_then(|v| Oid::from_str(&v.0).ok()).and_then(|oid| {
            self.store.with_repo(|r| Ok(r.find_commit(oid)?.tree_id())).ok()
        });
        report.ff = self
            .store
            .with_repo(|repo| {
                let new_tree = repo.find_commit(head_oid)?.tree_id();
                Ok(worktree::fast_forward(repo, root, from, new_tree, &head, None, now))
            })
            .map_err(to_store)?
            .map_err(to_store)?;
        Ok(report)
    }

    /// Poll the journal until the op is terminal (committed|rejected|failed|cancelled|superseded) or timeout.
    pub fn wait_terminal(&self, op: &OpId, timeout: Duration) -> Result<Option<OpState>, WriterError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let Some(row) = self.journal.get(op).map_err(journal_err)? else { return Ok(None) };
            if !matches!(row.state, OpState::Admitted | OpState::Applying) {
                return Ok(Some(row.state));
            }
            if std::time::Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The single writer task: loop { spawn_blocking(drain); wait for wake | 1s tick | shutdown }.
    /// Infra error → sleep cfg.infra_backoff * 2^n; after cfg.infra_retries consecutive → meta writer_halted=<reason>,
    /// stop processing.
    pub async fn run(self: Arc<Self>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        loop {
            if *shutdown.borrow() {
                return;
            }
            let me = self.clone();
            let result = tokio::task::spawn_blocking(move || me.drain()).await;
            match result {
                Ok(Ok(_)) => {}
                Ok(Err(WriterError::Halted(_))) => {}
                Ok(Err(e)) => {
                    let n = self.infra_failures.load(Ordering::SeqCst);
                    if n >= self.cfg.infra_retries {
                        let reason = format!("infrastructure failures: {e}");
                        let _ = self.journal.meta_set(WRITER_HALTED, &reason);
                    } else {
                        let backoff = self.cfg.infra_backoff * 2u32.saturating_pow(n.saturating_sub(1));
                        tokio::select! {
                            _ = tokio::time::sleep(backoff) => {}
                            _ = shutdown.changed() => {}
                        }
                        continue;
                    }
                }
                Err(join) => eprintln!("herdr-graph: writer task failed: {join}"),
            }
            tokio::select! {
                _ = self.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                _ = shutdown.changed() => {}
            }
        }
    }
}

impl Writer for WriterCore {
    /// Admission (spec §3.4): the kind must have a registered mutation and every relied_on object must exist at head.
    fn admit(&self, req: crate::model::change::ChangeRequest) -> Result<OpId, WriterError> {
        let key = mutation_key(&req);
        if self.registry.get(&key).is_none() {
            return Err(WriterError::Invalid(format!("no mutation registered for {key}")));
        }
        let head = self.store.head().map_err(|e| WriterError::Journal(e.to_string()))?;
        for r in &req.relied_on {
            match self.store.locate(&head, &r.object) {
                Ok(Some(_)) => {}
                Ok(None) => return Err(WriterError::Invalid(format!("relied-on object {} does not exist", r.object))),
                Err(e) => return Err(WriterError::Journal(e.to_string())),
            }
        }
        self.warn_if_queued_conflict(&req);
        let op = self.journal.admit(&req, self.clock.now()).map_err(journal_err)?;
        failpoint!("writer.after_admit");
        self.wake.notify_one();
        Ok(op)
    }

    fn status(&self, op: &OpId) -> Result<Option<OpState>, WriterError> {
        Ok(self.journal.get(op).map_err(journal_err)?.map(|r| r.state))
    }
}

impl WriterCore {
    /// Best-effort admission-time compatibility check against the queued projection: the op is still admitted
    /// (the writer's recheck is authoritative), the conflict is only logged.
    fn warn_if_queued_conflict(&self, req: &crate::model::change::ChangeRequest) {
        let Ok(queued) = self.journal.list(&[OpState::Admitted, OpState::Applying], 1000) else { return };
        for r in &req.relied_on {
            let clash = queued.iter().any(|q| q.request.relied_on.iter().any(|o| same_target(o, r)));
            if clash {
                eprintln!(
                    "herdr-graph: warning: queued op already relies on {} at the same version; one of them will be rejected",
                    r.object
                );
            }
        }
    }
}

fn same_target(a: &ReliedOn, b: &ReliedOn) -> bool {
    a.object == b.object && a.version == b.version
}
