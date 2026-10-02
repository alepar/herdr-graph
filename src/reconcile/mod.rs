//! Desired-vs-live diff → effects, fencing, retries, session replacement (spec §4.4–4.5). Owned by hg-zmi.7.
//!
//! The reconciler is the second half of the shared observer/reconciler loop step (spec §4.3.6): the
//! observer commits what it saw, then `Reconciler::step` diffs the committed desired runtime against that
//! same snapshot, journals effects with deterministic ids, fences them against the committed head, executes
//! them in dependency order and writes bindings back as bookkeeping mutations.
pub mod backoff;
pub mod bookkeeping;
pub mod desired;
pub mod executor;
pub mod herdr_exec;
pub mod planner;
pub mod session;
#[cfg(test)]
mod tests;

pub use bookkeeping::register_mutations;
pub use desired::effect_op;
pub use executor::{DiffCx, EffectExecutor, EffectSource, ExecCx, ExecOutcome, PlannedEffect};

use crate::journal::{Journal, Notice};
use crate::model::common::CommitId;
use crate::model::effect::{Dispatch, EffectKind, EffectRecord, EffectStatus, PredictedEnd};
use crate::model::{CloneId, EffectId, OpId, Timestamp};
use crate::ports::clock::Clock;
use crate::ports::herdr::{HerdrApi, HerdrSnapshot};
use crate::ports::store::Store;
use crate::ports::threads::{OpKey, Severity, ThreadRef, ThreadsPort};
use crate::ports::writer::Writer;
use crate::store::tree::CommitView;
use herdr_exec::{DesiredCache, HerdrExecutor, HerdrSource};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ReconcilerConfig {
    /// Periodic re-diff interval (the loop that calls `step_fresh` owns the timer).
    pub tick: Duration,
    /// Session replacement: how long to wait for the occupant to go idle.
    pub idle_timeout: Duration,
    /// Session replacement: how long to wait for the shell after the exit sequence.
    pub exit_timeout: Duration,
    /// No relaunch `start_agent` this long after a Herdr incarnation change (r2).
    pub relaunch_grace: Duration,
    pub instance: PathBuf,
    /// Send the exit fallback if the agent is still running this long after the exit sequence (3 s).
    pub exit_followup: Duration,
    /// First re-check delay of an open-ended `Deferred` wait, and the polling period of a time-bound wait
    /// between its deadlines.
    pub deferred_recheck: Duration,
    /// Cap of the open-ended deferral backoff (`deferred_recheck` doubles up to this).
    pub deferred_max: Duration,
}

impl ReconcilerConfig {
    pub fn new(instance: PathBuf) -> Self {
        Self {
            tick: Duration::from_secs(60),
            idle_timeout: Duration::from_secs(600),
            exit_timeout: Duration::from_secs(30),
            relaunch_grace: Duration::from_secs(90),
            instance,
            exit_followup: Duration::from_secs(3),
            deferred_recheck: Duration::from_secs(2),
            deferred_max: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct StepReport {
    /// Effects newly journaled by this step.
    pub planned: Vec<EffectId>,
    /// Effects executed this step with the status they ended in.
    pub executed: Vec<(EffectId, EffectStatus)>,
    pub obsolete: Vec<EffectId>,
    /// Effects left waiting on dependencies, backoff or a grace period.
    pub deferred: Vec<EffectId>,
}

/// Tells the requester of an op that something needs their attention (`needs_revision`, blocked agents).
#[async_trait::async_trait]
pub trait RequesterNotifier: Send + Sync {
    fn notify(&self, op: &OpId, severity: Severity, text: &str);
    /// Durable delivery of one journaled notice. Ok(true) delivered, Ok(false) nowhere to deliver (logged),
    /// Err(why) retry later. Default: the synchronous `notify`, counted as delivered.
    async fn deliver(&self, op: &OpId, severity: Severity, text: &str, _key: &OpKey) -> Result<bool, String> {
        self.notify(op, severity, text);
        Ok(true)
    }
}

/// The idempotency key of an op's notice with this text.
fn notice_key(op: &OpId, text: &str) -> OpKey {
    let digest = Sha256::digest(text.as_bytes());
    OpKey(format!("{op}:notify:{:02x}{:02x}{:02x}{:02x}", digest[0], digest[1], digest[2], digest[3]))
}

/// Default notifier: the requester seat's channel thread through the threads port. No thread: log only.
pub struct ThreadsNotifier {
    pub threads: Arc<dyn ThreadsPort>,
    pub journal: Arc<Journal>,
    pub store: Arc<dyn Store>,
}

impl ThreadsNotifier {
    fn thread_for(&self, op: &OpId) -> Option<ThreadRef> {
        let seat = self.journal.get(op).ok().flatten()?.request.requester.seat?;
        let head = self.store.head().ok()?;
        let tree = CommitView { store: &*self.store, at: head };
        let loc = crate::store::layout::locate(&tree, &seat.to_any()).ok().flatten()?;
        let rec: crate::model::seat::SeatRecord = crate::store::record::read_toml(&tree, &loc.record_path).ok().flatten()?;
        rec.channel.thread_id.map(ThreadRef)
    }
}

#[async_trait::async_trait]
impl RequesterNotifier for ThreadsNotifier {
    fn notify(&self, _op: &OpId, _severity: Severity, text: &str) {
        eprintln!("herdr-graph: reconcile: {text}");
    }

    async fn deliver(&self, op: &OpId, severity: Severity, text: &str, key: &OpKey) -> Result<bool, String> {
        let Some(thread) = self.thread_for(op) else {
            eprintln!("herdr-graph: reconcile: {text}");
            return Ok(false);
        };
        self.threads.notify(&thread, severity, text, key).await.map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// Predicted end states of effects the observer has not yet consumed (spec §3.3, §4.4).
pub fn predictions(journal: &Journal) -> Vec<(EffectId, PredictedEnd)> {
    let rows = journal
        .effects_with_status(&[EffectStatus::Pending, EffectStatus::Done, EffectStatus::Unknown])
        .unwrap_or_default();
    rows.into_iter()
        .filter(|r| journal.meta_get(&format!("pred_used:{}", r.id)).ok().flatten().is_none())
        .flat_map(|r| r.predicted.into_iter().map(move |p| (r.id.clone(), p)))
        .collect()
}

/// The observer classified a change using `ef`'s prediction: do not offer it again.
pub fn consume_prediction(journal: &Journal, ef: &EffectId) {
    let _ = journal.meta_set(&format!("pred_used:{ef}"), "1");
}

const MAX_PASSES: usize = 32;

/// A revision bump must not duplicate a waiting effect, except for families whose rows are keyed by payload.
fn merges_into_open(kind: &EffectKind) -> bool {
    !matches!(
        kind,
        EffectKind::Invite | EffectKind::ReleaseRequirement | EffectKind::Notify | EffectKind::Custom(_)
    )
}

pub struct Reconciler {
    store: Arc<dyn Store>,
    journal: Arc<Journal>,
    writer: Arc<dyn Writer>,
    herdr: Arc<dyn HerdrApi>,
    clock: Arc<dyn Clock>,
    notifier: Arc<dyn RequesterNotifier>,
    cfg: ReconcilerConfig,
    cache: Arc<DesiredCache>,
    builtin: HerdrSource,
    sources: RwLock<Vec<Arc<dyn EffectSource>>>,
    executors: RwLock<Vec<Arc<dyn EffectExecutor>>>,
}

impl Reconciler {
    pub fn new(
        store: Arc<dyn Store>,
        journal: Arc<Journal>,
        writer: Arc<dyn Writer>,
        herdr: Arc<dyn HerdrApi>,
        clock: Arc<dyn Clock>,
        notifier: Arc<dyn RequesterNotifier>,
        cfg: ReconcilerConfig,
    ) -> Arc<Self> {
        let cache = Arc::new(DesiredCache::default());
        let builtin = HerdrSource { instance: cfg.instance.clone(), cache: cache.clone() };
        let exec = HerdrExecutor { journal: journal.clone(), cfg: cfg.clone(), cache: cache.clone() };
        Self::fold_legacy_meta(&journal);
        Arc::new(Self {
            store,
            journal,
            writer,
            herdr,
            clock,
            notifier,
            cfg,
            cache,
            builtin,
            sources: RwLock::new(Vec::new()),
            executors: RwLock::new(vec![Arc::new(exec)]),
        })
    }

    /// Rows journaled before the scheduling state moved onto them keep it in loose `deps:`/`retry_at:`/`wake_at:`/
    /// `defer_n:` meta keys: fold those into `row.sched` once, then drop the keys.
    fn fold_legacy_meta(journal: &Journal) {
        let rows = journal.effects_with_status(&[EffectStatus::Pending, EffectStatus::Unknown]).unwrap_or_else(|e| {
            eprintln!("herdr-graph: reconcile: cannot list effects to fold legacy scheduling meta: {e}");
            Vec::new()
        });
        let time = |key: &str| {
            let raw = journal.meta_get(key).ok().flatten()?;
            chrono::DateTime::parse_from_rfc3339(&raw).ok().map(|t| t.to_utc())
        };
        for mut row in rows {
            let keys = ["deps", "retry_at", "wake_at", "defer_n"].map(|k| format!("{k}:{}", row.id));
            let deps = journal.meta_get(&keys[0]).ok().flatten().and_then(|raw| serde_json::from_str::<Vec<EffectId>>(&raw).ok());
            let (retry_at, wake_at) = (time(&keys[1]), time(&keys[2]));
            let defer_n = journal.meta_get(&keys[3]).ok().flatten().and_then(|s| s.parse::<u32>().ok());
            if deps.is_some() || retry_at.is_some() || wake_at.is_some() || defer_n.is_some() {
                if let Some(d) = deps {
                    row.sched.deps = d;
                }
                row.sched.retry_at = retry_at.or(row.sched.retry_at);
                row.sched.wake_at = wake_at.or(row.sched.wake_at);
                row.sched.defer_n = defer_n.unwrap_or(row.sched.defer_n);
                if let Err(e) = journal.upsert_effect(&row) {
                    eprintln!("herdr-graph: reconcile: cannot fold scheduling meta of effect {}: {e}", row.id);
                    continue;
                }
            }
            for key in &keys {
                if let Err(e) = journal.meta_delete(key) {
                    eprintln!("herdr-graph: reconcile: cannot drop legacy meta {key}: {e}");
                }
            }
        }
    }

    /// The effect journal (components that key payloads to effect ids keep them in its `meta` table).
    pub fn journal(&self) -> &Arc<Journal> {
        &self.journal
    }

    /// The graph instance root this reconciler works on.
    pub fn instance(&self) -> &std::path::Path {
        &self.cfg.instance
    }

    /// Another component's effect family (threads, delivery): its diff runs with every step.
    pub fn register_source(&self, s: Arc<dyn EffectSource>) {
        self.sources.write().unwrap_or_else(|e| e.into_inner()).push(s);
    }

    pub fn register_executor(&self, e: Arc<dyn EffectExecutor>) {
        self.executors.write().unwrap_or_else(|x| x.into_inner()).push(e);
    }

    fn executor_for(&self, kind: &EffectKind) -> Option<Arc<dyn EffectExecutor>> {
        self.executors.read().unwrap_or_else(|e| e.into_inner()).iter().find(|e| e.handles(kind)).cloned()
    }

    /// Own snapshot at the current head (tests, and the loop on op-committed triggers).
    pub async fn step_fresh(&self) -> StepReport {
        let head = match self.store.head() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("herdr-graph: reconcile: no committed head: {e}");
                return StepReport::default();
            }
        };
        match self.herdr.snapshot().await {
            Ok(snap) => self.step(&head, &snap).await,
            Err(e) => {
                eprintln!("herdr-graph: reconcile: snapshot failed: {e}");
                StepReport::default()
            }
        }
    }

    /// The reconcile half of the loop step. `snap` is a snapshot whose observations are already committed;
    /// the caller guarantees that (spec §4.3.6) and never calls this on an uncommitted one.
    pub async fn step(&self, head: &CommitId, snap: &HerdrSnapshot) -> StepReport {
        let mut report = StepReport::default();
        let now = self.clock.now();
        self.note_incarnation(snap, now);

        let planned = {
            let tree = CommitView { store: &*self.store, at: head.clone() };
            let cx = DiffCx { tree: &tree, head, snapshot: snap, journal: &self.journal, now };
            let mut planned = self.builtin.effects(&cx);
            let sources: Vec<_> = self.sources.read().unwrap_or_else(|e| e.into_inner()).clone();
            for s in sources {
                planned.extend(s.effects(&cx));
            }
            planned
        };
        self.upsert(planned, &mut report);
        self.run_pending(snap, &mut report).await;
        self.deliver_notices().await;
        report
    }

    /// Deliver every due attention notice (spec §4.1, §4.4). A failed delivery stays journaled and is retried
    /// with backoff for as long as the effect keeps the attention status; a notice whose effect has left it is voided.
    pub async fn deliver_notices(&self) {
        let now = self.clock.now();
        let due = match self.journal.due_notices(now) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("herdr-graph: reconcile: cannot read notices: {e}");
                return;
            }
        };
        for n in due {
            let holds = matches!(
                self.journal.get_effect(&n.effect),
                Ok(Some(r)) if matches!(
                    r.status,
                    EffectStatus::NeedsRevision | EffectStatus::BlockedNeedsHuman | EffectStatus::Failed
                )
            );
            let done = |state: &str| {
                if let Err(e) = self.journal.notice_done(&n.key, state, now) {
                    eprintln!("herdr-graph: reconcile: cannot update notice {}: {e}", n.key);
                }
            };
            if !holds {
                done("void");
                continue;
            }
            match self.notifier.deliver(&n.op, n.severity, &n.text, &OpKey(n.key.clone())).await {
                Ok(true) => done("delivered"),
                Ok(false) => done("logged"),
                Err(why) => {
                    let wait = backoff::next(n.attempts.saturating_add(1));
                    let at = now + chrono::Duration::from_std(wait).unwrap_or_default();
                    eprintln!("herdr-graph: reconcile: notice {} not delivered (will retry): {why}", n.key);
                    if let Err(e) = self.journal.notice_retry(&n.key, &why, at, now) {
                        eprintln!("herdr-graph: reconcile: cannot update notice {}: {e}", n.key);
                    }
                }
            }
        }
    }

    /// A restart of Herdr changes the incarnation; relaunches wait out a grace period after it (r2).
    fn note_incarnation(&self, snap: &HerdrSnapshot, now: Timestamp) {
        let cur = serde_json::to_string(&snap.incarnation).unwrap_or_default();
        let last = self.journal.meta_get("incarnation:last").ok().flatten();
        if last.as_deref().is_some_and(|l| l != cur) {
            let _ = self.journal.meta_set("incarnation:changed_at", &now.to_rfc3339());
        }
        if last.as_deref() != Some(cur.as_str()) {
            let _ = self.journal.meta_set("incarnation:last", &cur);
        }
    }

    /// Journal newly planned effects as `pending`, skipping ids already known and merging into an open row
    /// of the same kind and object (a revision bump must not duplicate a waiting effect).
    fn upsert(&self, planned: Vec<PlannedEffect>, report: &mut StepReport) {
        let mut canonical: HashMap<EffectId, EffectId> = HashMap::new();
        let mut fresh: Vec<&PlannedEffect> = Vec::new();
        for p in &planned {
            let id = &p.record.id;
            if matches!(self.journal.get_effect(id), Ok(Some(_))) {
                canonical.insert(id.clone(), id.clone());
                continue;
            }
            // Payload-keyed families (threads invites, releases, notifies, custom effects) carry their identity
            // in the payload, so several open rows of one kind on one object are distinct work.
            let open = merges_into_open(&p.record.kind)
                .then(|| {
                    self.journal.effects_for_object(&p.record.object).unwrap_or_default().into_iter().find(|r| {
                        r.kind == p.record.kind && matches!(r.status, EffectStatus::Pending | EffectStatus::Unknown)
                    })
                })
                .flatten();
            match open {
                Some(r) => {
                    canonical.insert(id.clone(), r.id);
                }
                None => {
                    canonical.insert(id.clone(), id.clone());
                    fresh.push(p);
                }
            }
        }
        for p in fresh {
            let deps: Vec<&EffectId> = p.deps.iter().map(|d| canonical.get(d).unwrap_or(d)).collect();
            let mut record = p.record.clone();
            record.sched.deps = deps.into_iter().cloned().collect();
            if let Err(e) = self.journal.upsert_effect(&record) {
                eprintln!("herdr-graph: reconcile: cannot journal planned effect {}: {e}", record.id);
                continue;
            }
            report.planned.push(p.record.id.clone());
        }
    }

    fn mark(&self, row: &mut EffectRecord, status: EffectStatus, error: Option<String>, now: Timestamp) {
        row.status = status;
        row.last_error = error;
        row.updated_at = now;
        // The outcome is recorded: the write-ahead marker goes in the same write.
        row.sched.dispatched = None;
        let open = matches!(status, EffectStatus::Pending | EffectStatus::Unknown);
        // An effect that ended leaves no scheduling state behind.
        if !open {
            row.sched.retry_at = None;
            row.sched.wake_at = None;
            row.sched.defer_n = 0;
        }
        if let Err(e) = self.journal.upsert_effect(row) {
            eprintln!("herdr-graph: reconcile: cannot journal effect {}: {e}", row.id);
        }
        if !open {
            session::clear_state(&self.journal, &row.id);
        }
    }

    /// Record an attention status and its notice in one journal write (spec §4.1, §4.4).
    fn mark_attention(
        &self,
        row: &mut EffectRecord,
        status: EffectStatus,
        error: Option<String>,
        severity: Severity,
        text: String,
        now: Timestamp,
    ) {
        row.status = status;
        row.last_error = error;
        row.updated_at = now;
        // The outcome is recorded: the write-ahead marker and any scheduling state go in the same write.
        row.sched.dispatched = None;
        let open = matches!(status, EffectStatus::Pending | EffectStatus::Unknown);
        if !open {
            row.sched.retry_at = None;
            row.sched.wake_at = None;
            row.sched.defer_n = 0;
        }
        let notice = Notice {
            key: notice_key(&row.op, &text).0,
            effect: row.id.clone(),
            op: row.op.clone(),
            severity,
            text,
            state: "pending".into(),
            attempts: 0,
            last_error: None,
            next_at: None,
        };
        if let Err(e) = self.journal.upsert_effect_with_notice(row, &notice) {
            eprintln!("herdr-graph: reconcile: cannot journal effect {} with its notice: {e}", row.id);
        }
        if !open {
            session::clear_state(&self.journal, &row.id);
        }
    }

    /// The earliest time an open effect (`Pending`/`Unknown`) needs another look: its backoff expiry or its
    /// deferred recheck. The loop sleeps until then instead of waiting for the periodic tick. Times already
    /// past are ignored: the step that just ran has looked at those rows, and one that is still waiting is
    /// blocked on something else (a dependency), which a wake would not help.
    pub fn next_wake(&self) -> Option<Timestamp> {
        let now = self.clock.now();
        let rows = self.journal.effects_with_status(&[EffectStatus::Pending, EffectStatus::Unknown]).unwrap_or_default();
        let notice_at = self.journal.next_notice_at().ok().flatten();
        rows.iter()
            .flat_map(|r| [r.sched.retry_at, r.sched.wake_at])
            .chain([notice_at])
            .flatten()
            .filter(|t| *t > now)
            .min()
    }

    /// A deferral: the row stays open, wakes at `at`, and its dispatch marker goes in the same write.
    fn defer(&self, row: &mut EffectRecord, at: Timestamp, why: String) {
        row.sched.wake_at = Some(at);
        row.sched.dispatched = None;
        row.last_error = Some(why);
        if let Err(e) = self.journal.upsert_effect(row) {
            eprintln!("herdr-graph: reconcile: cannot journal effect {}: {e}", row.id);
        }
    }

    async fn run_pending(&self, snap: &HerdrSnapshot, report: &mut StepReport) {
        let mut cur = snap.clone();
        let mut ran: BTreeSet<EffectId> = BTreeSet::new();
        let mut waiting: BTreeSet<EffectId> = BTreeSet::new();
        for _ in 0..MAX_PASSES {
            let rows = self.journal.effects_with_status(&[EffectStatus::Pending, EffectStatus::Unknown]).unwrap_or_default();
            let mut progressed = false;
            for stale in rows {
                if ran.contains(&stale.id) {
                    continue;
                }
                // Re-read: a cascade earlier in this pass may have finished it.
                let Ok(Some(mut row)) = self.journal.get_effect(&stale.id) else { continue };
                if !matches!(row.status, EffectStatus::Pending | EffectStatus::Unknown) {
                    continue;
                }
                // Lost dispatch: the marker was journaled before a Herdr call and its outcome never was (a crash,
                // a SIGTERM, or a cancelled future such as the first-pass timeout). Resolve it as `Unknown`: the
                // executor looks for the effect's token/nonce in the snapshot before any retry.
                if row.sched.dispatched.is_some() {
                    row.status = EffectStatus::Unknown;
                    row.last_error = Some("dispatch outcome lost (crash, timeout or cancellation); inspecting the snapshot".into());
                    row.sched.dispatched = None;
                    if let Err(e) = self.journal.upsert_effect(&row) {
                        eprintln!("herdr-graph: reconcile: cannot journal effect {}: {e}", row.id);
                        continue;
                    }
                }
                let Some(exec) = self.executor_for(&row.kind) else { continue };
                let head = match self.store.head() {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("herdr-graph: reconcile: no committed head: {e}");
                        return;
                    }
                };
                let tree = CommitView { store: &*self.store, at: head.clone() };
                let now = self.clock.now();
                let cx = ExecCx { tree: &tree, head: &head, snapshot: &cur, herdr: &*self.herdr, writer: &*self.writer, now };

                // Fence: the object moved since planning and the effect is no longer implied.
                let rev_now = self.cache.get(&tree, &head, &self.cfg.instance).ok().and_then(|d| d.rev_of(&row.object));
                if let Some(rev) = rev_now
                    && rev != row.fencing_rev
                {
                    if !exec.is_implied(&cx, &row) {
                        self.mark(&mut row, EffectStatus::Obsolete, None, now);
                        report.obsolete.push(row.id.clone());
                        progressed = true;
                        continue;
                    }
                    row.fencing_rev = rev;
                    if let Err(e) = self.journal.upsert_effect(&row) {
                        eprintln!("herdr-graph: reconcile: cannot journal effect {}: {e}", row.id);
                    }
                }

                // Dependencies.
                let mut blocked = false;
                let mut dep_obsolete = false;
                for dep in row.sched.deps.clone() {
                    if let Ok(Some(d)) = self.journal.get_effect(&dep) {
                        match d.status {
                            EffectStatus::Done => {}
                            EffectStatus::Obsolete => dep_obsolete = true,
                            _ => blocked = true,
                        }
                    }
                }
                if dep_obsolete {
                    self.mark(&mut row, EffectStatus::Obsolete, Some("dependency obsolete".into()), now);
                    report.obsolete.push(row.id.clone());
                    progressed = true;
                    continue;
                }
                if blocked || row.sched.retry_at.is_some_and(|t| t > now) {
                    waiting.insert(row.id.clone());
                    continue;
                }

                // Write-ahead marker: a non-idempotent call is never made without it on the row.
                if row.kind.is_non_idempotent() {
                    row.sched.dispatched = Some(Dispatch { attempt: row.attempts + 1, at: now });
                    if let Err(e) = self.journal.upsert_effect(&row) {
                        eprintln!("herdr-graph: reconcile: cannot journal the dispatch of effect {}: {e}", row.id);
                        continue;
                    }
                }

                let outcome = exec.execute(&cx, &row).await;
                crate::failpoint!("reconcile.mid_effect");
                // The same point per effect kind: effect order within an op is by hash, so a crash test that
                // needs "right after the CreateTab" arms `reconcile.mid_effect.create_tab`, not the generic name.
                crate::failpoint!(&format!("reconcile.mid_effect.{}", row.kind.as_str()));
                ran.insert(row.id.clone());
                let now = self.clock.now();
                row.attempts += 1;
                if !matches!(outcome, ExecOutcome::Deferred(_) | ExecOutcome::DeferredUntil(..)) {
                    row.sched.defer_n = 0;
                    row.sched.wake_at = None;
                }
                let status = match outcome {
                    ExecOutcome::Done => {
                        row.sched.retry_at = None;
                        EffectStatus::Done
                    }
                    ExecOutcome::Transient(msg) => {
                        let wait = backoff::next(row.attempts);
                        let at = now + chrono::Duration::from_std(wait).unwrap_or_default();
                        row.sched.retry_at = Some(at);
                        self.mark(&mut row, EffectStatus::Pending, Some(msg), now);
                        report.executed.push((row.id.clone(), EffectStatus::Pending));
                        continue;
                    }
                    ExecOutcome::Unknown => EffectStatus::Unknown,
                    ExecOutcome::NeedsRevision(reason) => {
                        let text = format!("{} for {} needs revision: {reason}", row.kind.as_str(), row.object);
                        self.mark_attention(&mut row, EffectStatus::NeedsRevision, Some(reason), Severity::Warn, text, now);
                        report.executed.push((row.id.clone(), EffectStatus::NeedsRevision));
                        continue;
                    }
                    ExecOutcome::BlockedNeedsHuman => {
                        let text = format!("agent on {} is waiting for a human (trust or auth dialog)", row.object);
                        self.mark_attention(&mut row, EffectStatus::BlockedNeedsHuman, None, Severity::Warn, text, now);
                        report.executed.push((row.id.clone(), EffectStatus::BlockedNeedsHuman));
                        continue;
                    }
                    ExecOutcome::Failed(msg) => {
                        eprintln!("herdr-graph: reconcile: effect {} failed: {msg}", row.id);
                        let text = format!("{} for {} failed: {msg}", row.kind.as_str(), row.object);
                        self.mark_attention(&mut row, EffectStatus::Failed, Some(msg), Severity::Warn, text, now);
                        report.executed.push((row.id.clone(), EffectStatus::Failed));
                        continue;
                    }
                    ExecOutcome::Obsolete => {
                        self.mark(&mut row, EffectStatus::Obsolete, None, now);
                        report.obsolete.push(row.id.clone());
                        progressed = true;
                        continue;
                    }
                    ExecOutcome::Deferred(why) => {
                        row.attempts -= 1;
                        row.sched.defer_n = row.sched.defer_n.saturating_add(1);
                        let wait = backoff::deferred(self.cfg.deferred_recheck, self.cfg.deferred_max, row.sched.defer_n);
                        let at = now + chrono::Duration::from_std(wait).unwrap_or_default();
                        self.defer(&mut row, at, why);
                        waiting.insert(row.id.clone());
                        continue;
                    }
                    ExecOutcome::DeferredUntil(at, why) => {
                        row.attempts -= 1;
                        self.defer(&mut row, at, why);
                        waiting.insert(row.id.clone());
                        continue;
                    }
                };
                let error = (status == EffectStatus::Unknown).then(|| "outcome unknown; inspecting the snapshot".to_owned());
                self.mark(&mut row, status, error, now);
                report.executed.push((row.id.clone(), status));
                if status == EffectStatus::Done {
                    progressed = true;
                    // Dependents look their targets up in a snapshot that includes what this effect just did.
                    if let Ok(s) = self.herdr.snapshot().await {
                        cur = s;
                    }
                }
            }
            if !progressed {
                break;
            }
        }
        for id in waiting {
            if matches!(self.journal.get_effect(&id), Ok(Some(r)) if matches!(r.status, EffectStatus::Pending | EffectStatus::Unknown)) {
                report.deferred.push(id);
            }
        }
    }

    /// Hook for the summarizer (Task 13): enqueue a `RelaunchOccupant` for an active clone with no occupant.
    pub fn request_relaunch(&self, clone: &CloneId, authority: &OpId) -> anyhow::Result<EffectId> {
        let head = self.store.head()?;
        let tree = CommitView { store: &*self.store, at: head };
        let d = self.cache.get(&tree, &tree.at, &self.cfg.instance)?;
        let rec = d.clones.get(clone).ok_or_else(|| anyhow::anyhow!("unknown clone {clone}"))?;
        let Some(p) = d.pane(clone) else {
            anyhow::bail!("clone {clone} is not an active clone of an active seat");
        };
        if p.occupant.is_some() {
            anyhow::bail!("clone {clone} already has an occupant");
        }
        let object = clone.to_any();
        let kind = EffectKind::RelaunchOccupant;
        let id = EffectRecord::identity(authority, &object, &kind, rec.rev);
        if self.journal.get_effect(&id)?.is_none() {
            self.journal.upsert_effect(&EffectRecord {
                id: id.clone(),
                op: authority.clone(),
                object,
                kind,
                object_rev: rec.rev,
                fencing_rev: rec.rev,
                status: EffectStatus::Pending,
                predicted: vec![],
                nonce_label: None,
                attempts: 0,
                last_error: None,
                updated_at: self.clock.now(),
                sched: Default::default(),
            })?;
        }
        Ok(id)
    }
}
