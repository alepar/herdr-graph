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

use crate::journal::Journal;
use crate::model::common::CommitId;
use crate::model::effect::{EffectKind, EffectRecord, EffectStatus, PredictedEnd};
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
    /// How soon a `Deferred` effect is looked at again when no Herdr event arrives.
    pub deferred_recheck: Duration,
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
pub trait RequesterNotifier: Send + Sync {
    fn notify(&self, op: &OpId, severity: Severity, text: &str);
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

impl RequesterNotifier for ThreadsNotifier {
    fn notify(&self, op: &OpId, severity: Severity, text: &str) {
        let (Some(thread), Ok(rt)) = (self.thread_for(op), tokio::runtime::Handle::try_current()) else {
            eprintln!("herdr-graph: reconcile: {text}");
            return;
        };
        let digest = Sha256::digest(text.as_bytes());
        let key = OpKey(format!("{op}:notify:{:02x}{:02x}{:02x}{:02x}", digest[0], digest[1], digest[2], digest[3]));
        let (threads, text) = (self.threads.clone(), text.to_owned());
        rt.spawn(async move {
            let _ = threads.notify(&thread, severity, &text, &key).await;
        });
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
        report
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
            if self.journal.upsert_effect(&p.record).is_err() {
                continue;
            }
            let _ = self.journal.meta_set(&format!("deps:{}", p.record.id), &serde_json::to_string(&deps).unwrap_or_default());
            report.planned.push(p.record.id.clone());
        }
    }

    fn deps_of(&self, id: &EffectId) -> Vec<EffectId> {
        self.journal
            .meta_get(&format!("deps:{id}"))
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn retry_at(&self, id: &EffectId) -> Option<Timestamp> {
        let raw = self.journal.meta_get(&format!("retry_at:{id}")).ok().flatten()?;
        chrono::DateTime::parse_from_rfc3339(&raw).ok().map(|t| t.to_utc())
    }

    fn mark(&self, row: &mut EffectRecord, status: EffectStatus, error: Option<String>, now: Timestamp) {
        row.status = status;
        row.last_error = error;
        row.updated_at = now;
        if let Err(e) = self.journal.upsert_effect(row) {
            eprintln!("herdr-graph: reconcile: cannot journal effect {}: {e}", row.id);
        }
        // An effect that ended leaves no per-effect meta behind.
        if !matches!(status, EffectStatus::Pending | EffectStatus::Unknown) {
            let _ = self.journal.meta_delete(&format!("retry_at:{}", row.id));
            let _ = self.journal.meta_delete(&format!("wake_at:{}", row.id));
            session::clear_state(&self.journal, &row.id);
        }
    }

    fn meta_time(&self, key: String) -> Option<Timestamp> {
        let raw = self.journal.meta_get(&key).ok().flatten()?;
        chrono::DateTime::parse_from_rfc3339(&raw).ok().map(|t| t.to_utc())
    }

    /// The earliest time an open effect (`Pending`/`Unknown`) needs another look: its backoff expiry or its
    /// deferred recheck. The loop sleeps until then instead of waiting for the periodic tick. Times already
    /// past are ignored: the step that just ran has looked at those rows, and one that is still waiting is
    /// blocked on something else (a dependency), which a wake would not help.
    pub fn next_wake(&self) -> Option<Timestamp> {
        let now = self.clock.now();
        let rows = self.journal.effects_with_status(&[EffectStatus::Pending, EffectStatus::Unknown]).unwrap_or_default();
        rows.iter()
            .flat_map(|r| [self.meta_time(format!("retry_at:{}", r.id)), self.meta_time(format!("wake_at:{}", r.id))])
            .flatten()
            .filter(|t| *t > now)
            .min()
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
                    let _ = self.journal.upsert_effect(&row);
                }

                // Dependencies.
                let mut blocked = false;
                let mut dep_obsolete = false;
                for dep in self.deps_of(&row.id) {
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
                if blocked || self.retry_at(&row.id).is_some_and(|t| t > now) {
                    waiting.insert(row.id.clone());
                    continue;
                }

                let outcome = exec.execute(&cx, &row).await;
                crate::failpoint!("reconcile.mid_effect");
                // The same point per effect kind: effect order within an op is by hash, so a crash test that
                // needs "right after the CreateTab" arms `reconcile.mid_effect.create_tab`, not the generic name.
                crate::failpoint!(&format!("reconcile.mid_effect.{}", row.kind.as_str()));
                ran.insert(row.id.clone());
                let now = self.clock.now();
                row.attempts += 1;
                let status = match outcome {
                    ExecOutcome::Done => {
                        let _ = self.journal.meta_delete(&format!("retry_at:{}", row.id));
                        EffectStatus::Done
                    }
                    ExecOutcome::Transient(msg) => {
                        let wait = backoff::next(row.attempts);
                        let at = now + chrono::Duration::from_std(wait).unwrap_or_default();
                        let _ = self.journal.meta_set(&format!("retry_at:{}", row.id), &at.to_rfc3339());
                        self.mark(&mut row, EffectStatus::Pending, Some(msg), now);
                        report.executed.push((row.id.clone(), EffectStatus::Pending));
                        continue;
                    }
                    ExecOutcome::Unknown => EffectStatus::Unknown,
                    ExecOutcome::NeedsRevision(reason) => {
                        self.notifier.notify(
                            &row.op,
                            Severity::Warn,
                            &format!("{} for {} needs revision: {reason}", row.kind.as_str(), row.object),
                        );
                        self.mark(&mut row, EffectStatus::NeedsRevision, Some(reason), now);
                        report.executed.push((row.id.clone(), EffectStatus::NeedsRevision));
                        continue;
                    }
                    ExecOutcome::BlockedNeedsHuman => {
                        self.notifier.notify(
                            &row.op,
                            Severity::Warn,
                            &format!("agent on {} is waiting for a human (trust or auth dialog)", row.object),
                        );
                        EffectStatus::BlockedNeedsHuman
                    }
                    ExecOutcome::Failed(msg) => {
                        self.mark(&mut row, EffectStatus::Failed, Some(msg), now);
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
                        row.last_error = Some(why);
                        let _ = self.journal.upsert_effect(&row);
                        let at = now + chrono::Duration::from_std(self.cfg.deferred_recheck).unwrap_or_default();
                        let _ = self.journal.meta_set(&format!("wake_at:{}", row.id), &at.to_rfc3339());
                        ran.remove(&row.id);
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
            })?;
        }
        Ok(id)
    }
}
