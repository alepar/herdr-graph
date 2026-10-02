//! Native sessions, transcript coverage, processing requests (spec §8). Owned by hg-zmi.12.
//!
//! * `coverage` — order-independent byte-range arithmetic;
//! * `mutations` — the `bookkeeping.*` kinds for `tr_` / `rq_` records;
//! * `requests` — summarizer destination, file facts, `request list` rows;
//! * `delivery` — fallback / service-ACK adapters and the `DeliverRequest` effect family;
//! * `capture` — `session-report` and the `session.report` IPC command;
//! * `watcher` — daemon loop tracking transcript files (resume appends, replaced files);
//! * `liveness` — daemon-owned scan: redeliver, remind, retry (spec §8.2).
//!
//! This module is the `Transcripts` service that ties them to the writer, the reconciler and the threads port.
pub mod capture;
pub mod coverage;
pub mod delivery;
pub mod liveness;
pub mod mutations;
pub mod requests;
#[cfg(test)]
mod tests;
pub mod watcher;

pub use coverage::{align_end, gaps, merge_coverage};
pub use mutations::register_mutations;

use crate::daemon::registry::{CallerInfo, CommandCtx, CommandError, Registry, Shutdown};
use crate::journal::Journal;
use crate::model::change::ChangeRequest;
use crate::model::effective::resolve_in;
use crate::model::operation::OpState;
use crate::model::request::{ProcessingRequest, RequestStatus};
use crate::model::{AnyId, ByteRange, CloneId, NsId, OpId, RequestId, SeatId, TranscriptId};
use crate::observe::SessionEnded;
use crate::ports::clock::Clock;
use crate::ports::store::{Store, StoreError};
use crate::ports::threads::{OpKey, Severity, ThreadRef, ThreadsPort};
use crate::ports::writer::Writer;
use crate::reconcile::Reconciler;
use crate::store::layout;
use crate::store::tree::CommitView;
use crate::threads::PaneSeatMap;
use crate::threads::effects::Graph;
use delivery::{DeliveryCore, DeliveryExecutor, DeliverySource};
use mutations::{bookkeeping_request, is_merged};
use requests::{Destination, ListFilter, resolve_destination};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::sync::broadcast;

/// Timers and thresholds of the daemon loops (spec §8.2); tests shrink the waits.
#[derive(Debug, Clone)]
pub struct Tuning {
    pub watch_interval: Duration,
    pub liveness_interval: Duration,
    /// Delivered but not ACKed this long: one reminder `Notify{Warn}`.
    pub remind_after: chrono::Duration,
    /// ACKed but not completed this long: a new delivery marked `retry` under a fresh op key.
    pub retry_after: chrono::Duration,
    /// How long a bookkeeping write may take to reach a terminal state.
    pub commit_timeout: Duration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            watch_interval: Duration::from_secs(30),
            liveness_interval: Duration::from_secs(10 * 60),
            remind_after: chrono::Duration::minutes(30),
            retry_after: chrono::Duration::hours(6),
            commit_timeout: Duration::from_secs(30),
        }
    }
}

pub struct Transcripts {
    pub(crate) store: Arc<dyn Store>,
    pub(crate) writer: Arc<dyn Writer>,
    pub(crate) journal: Arc<Journal>,
    pub(crate) reconciler: Arc<Reconciler>,
    pub(crate) threads: Arc<dyn ThreadsPort>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) core: Arc<DeliveryCore>,
    session_ended: Mutex<Option<broadcast::Receiver<SessionEnded>>>,
    tuning: RwLock<Tuning>,
    /// Serializes spool ingestion (the loop and every live `session.report`).
    spool_lock: tokio::sync::Mutex<()>,
}

/// Seat or clone that reported a result.
fn caller_object(g: &Graph, caller: &CallerInfo) -> Option<AnyId> {
    if let Some(c) = caller.graph_clone.as_deref().and_then(|c| CloneId::parse(c).ok()).filter(|c| g.clones.contains_key(c)) {
        return Some(c.to_any());
    }
    if let Some(s) = caller.graph_seat.as_deref().and_then(|s| SeatId::parse(s).ok()).filter(|s| g.seats.contains_key(s)) {
        return Some(s.to_any());
    }
    caller.pane_id.as_deref().and_then(|p| clone_for_pane(g, p)).map(|c| c.to_any())
}

/// The clone bound to a Herdr pane (active clones first, then the newest).
pub(crate) fn clone_for_pane(g: &Graph, pane: &str) -> Option<CloneId> {
    g.clones
        .values()
        .filter(|c| c.runtime.bound.as_ref().and_then(|b| b.pane_id.as_ref()).is_some_and(|p| p.0 == pane))
        .max_by_key(|c| (c.lifecycle == crate::model::common::CloneLifecycle::Active, c.rev))
        .map(|c| c.id.clone())
}

impl Transcripts {
    pub fn new(
        store: Arc<dyn Store>,
        writer: Arc<dyn Writer>,
        reconciler: Arc<Reconciler>,
        threads: Arc<dyn ThreadsPort>,
        mapping: Arc<dyn PaneSeatMap>,
        clock: Arc<dyn Clock>,
        session_ended: Option<broadcast::Receiver<SessionEnded>>,
    ) -> Arc<Self> {
        let journal = reconciler.journal().clone();
        Arc::new(Self {
            store,
            writer,
            journal,
            reconciler,
            core: Arc::new(DeliveryCore::new(threads.clone(), mapping)),
            threads,
            clock,
            session_ended: Mutex::new(session_ended),
            tuning: RwLock::new(Tuning::default()),
            spool_lock: tokio::sync::Mutex::new(()),
        })
    }

    pub fn set_tuning(&self, t: Tuning) {
        *self.tuning.write().unwrap_or_else(|e| e.into_inner()) = t;
    }

    pub(crate) fn tuning(&self) -> Tuning {
        self.tuning.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(crate) fn view(&self) -> Result<CommitView<'_>, StoreError> {
        Ok(CommitView { store: &*self.store, at: self.store.head()? })
    }

    /// Admit a bookkeeping write and wait for it to commit. A rejection is the caller's error.
    pub(crate) async fn commit(&self, request: ChangeRequest) -> Result<OpId, CommandError> {
        let op = self.writer.admit(request).map_err(|e| CommandError::unavailable(e.to_string()))?;
        // Inside a CLI request this is the request deadline; internal callers keep `commit_timeout`.
        let deadline = crate::daemon::budget::wait_until(self.tuning().commit_timeout);
        loop {
            match self.writer.status(&op) {
                Ok(Some(OpState::Committed)) => return Ok(op),
                Ok(Some(state @ (OpState::Admitted | OpState::Applying))) => {
                    if tokio::time::Instant::now() >= deadline {
                        return Err(CommandError::still_running(&op, state));
                    }
                }
                Ok(Some(state)) => {
                    let why = self
                        .journal
                        .get(&op)
                        .ok()
                        .flatten()
                        .and_then(|r| r.rejection)
                        .map(|r| format!("{}: {}", r.reason, r.explanation))
                        .unwrap_or_else(|| format!("{state:?}"));
                    return Err(CommandError::rejected(why));
                }
                Ok(None) => return Err(CommandError::internal(format!("{op} vanished from the journal"))),
                Err(e) => return Err(CommandError::unavailable(e.to_string())),
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    // -----------------------------------------------------------------------------------------
    // request creation
    // -----------------------------------------------------------------------------------------

    /// A native session ended (or changed): create the request for the bytes nothing covers yet when the
    /// source seat's effective `summaries` is true (spec §8.2).
    pub async fn on_session_ended(&self, ev: SessionEnded) {
        if let Err(e) = self.session_ended_inner(&ev).await {
            eprintln!("herdr-graph: transcripts: request for {} failed: {}", ev.ns, e.message);
        }
    }

    async fn session_ended_inner(&self, ev: &SessionEnded) -> Result<(), CommandError> {
        let (summaries, path) = {
            let view = self.view().map_err(internal)?;
            let g = Graph::load(&view).map_err(internal)?;
            let Some(seat) = g.seats.get(&ev.seat) else { return Ok(()) };
            let summaries = resolve_in(&view, seat).map_err(internal)?.summaries;
            let from_record = g
                .clones
                .get(&ev.clone)
                .and_then(|c| c.sessions.iter().find(|s| s.id == ev.ns))
                .and_then(|s| s.transcript_path.clone());
            (summaries, ev.transcript_path.clone().or(from_record))
        };
        if !summaries {
            return Ok(());
        }
        self.request_for(&ev.clone, &ev.ns, path, None, false).await
    }

    /// Create the request for `[covered_end, current_size)` of the transcript at `path`. `tr` pins an existing
    /// transcript record (resume appends); `force_new` starts a fresh one (the file was replaced).
    pub(crate) async fn request_for(
        &self,
        clone: &CloneId,
        ns: &NsId,
        path: Option<PathBuf>,
        tr: Option<&TranscriptId>,
        force_new: bool,
    ) -> Result<(), CommandError> {
        let mut force_new = force_new;
        let mut size = None;
        if let Some(p) = &path
            && let Ok(state) = requests::stat_file(p)
        {
            force_new |= self.note_identity(p, &state);
            size = Some(state.aligned);
        }
        let args = json!({
            "clone": clone, "ns": ns, "path": path, "size": size, "summaries": true,
            "force_new": force_new, "tr": tr,
        });
        self.commit(bookkeeping_request("request_create", args)).await?;
        self.process_pending().await;
        Ok(())
    }

    /// Re-request an explicit byte range of a transcript (a gap): same dedup and merge rules.
    pub async fn create_request_for_range(&self, tr: &TranscriptId, range: ByteRange) -> Result<(), CommandError> {
        let (clone, ns, path) = {
            let view = self.view().map_err(internal)?;
            let rec = layout::list_transcripts(&view)
                .map_err(internal)?
                .into_iter()
                .map(|(_, t)| t)
                .find(|t| &t.id == tr)
                .ok_or_else(|| CommandError::bad_request(format!("unknown transcript {tr}")))?;
            (rec.clone, rec.native_session, rec.transcript_path)
        };
        let args = json!({
            "clone": clone, "ns": ns, "path": path, "size": range.end, "summaries": true, "range": range, "tr": tr,
        });
        self.commit(bookkeeping_request("request_create", args)).await?;
        self.process_pending().await;
        Ok(())
    }

    /// Identity of the transcript file (device + inode) as last seen; true when it changed (replaced file).
    pub(crate) fn note_identity(&self, path: &Path, state: &requests::FileState) -> bool {
        let key = format!("transcripts:ident:{}", path.display());
        let now = state.identity();
        let last = self.journal.meta_get(&key).ok().flatten();
        if last.as_deref() != Some(now.as_str()) {
            let _ = self.journal.meta_set(&key, &now);
        }
        last.is_some_and(|l| l != now)
    }

    // -----------------------------------------------------------------------------------------
    // routing of pending requests
    // -----------------------------------------------------------------------------------------

    /// For every pending request: an active summarizer without an occupant is relaunched through the
    /// reconciler (authority: the op that activated it); a request with no way to be delivered is flagged
    /// `undeliverable` and the teamspace channel is told once; a request that became deliverable loses the flag.
    pub async fn process_pending(&self) {
        if let Err(e) = self.process_pending_inner().await {
            eprintln!("herdr-graph: transcripts: routing pending requests failed: {}", e.message);
        }
    }

    async fn process_pending_inner(&self) -> Result<(), CommandError> {
        struct Work {
            rq: ProcessingRequest,
            dest: Destination,
            teamspace_thread: Option<ThreadRef>,
        }
        let work: Vec<Work> = {
            let view = self.view().map_err(internal)?;
            let g = Graph::load(&view).map_err(internal)?;
            let transcripts: Vec<_> = layout::list_transcripts(&view).map_err(internal)?.into_iter().map(|(_, t)| t).collect();
            let mut out = Vec::new();
            for (_, rq) in layout::list_requests(&view).map_err(internal)? {
                if rq.status != RequestStatus::Pending || is_merged(&rq) || rq.unresolved.is_some() {
                    continue;
                }
                let Some(tr) = transcripts.iter().find(|t| t.id == rq.transcript) else { continue };
                let dest = resolve_destination(&g, &view, &tr.seat).map_err(internal)?;
                let teamspace_thread = g
                    .seats
                    .get(&tr.seat)
                    .and_then(|s| g.teamspaces.get(&s.teamspace))
                    .and_then(|t| t.channel.thread_id.clone())
                    .map(ThreadRef);
                out.push(Work { rq, dest, teamspace_thread });
            }
            out
        };
        for w in work {
            match &w.dest {
                Destination::Ready { .. } | Destination::Relaunch { .. } => {
                    if w.rq.undeliverable.is_some() {
                        self.commit(bookkeeping_request(
                            "request_delivery",
                            json!({ "rq": w.rq.id, "clear_undeliverable": true }),
                        ))
                        .await?;
                    }
                    if let Destination::Relaunch { clone, authority, .. } = &w.dest
                        && let Err(e) = self.reconciler.request_relaunch(clone, authority)
                    {
                        eprintln!("herdr-graph: transcripts: cannot relaunch summarizer clone {clone}: {e:#}");
                    }
                }
                Destination::Undeliverable { reason, .. } => {
                    if w.rq.undeliverable.as_deref() != Some(*reason) {
                        self.commit(bookkeeping_request(
                            "request_delivery",
                            json!({ "rq": w.rq.id, "undeliverable": reason }),
                        ))
                        .await?;
                    }
                    self.notify_undeliverable_once(&w.rq.id, reason, w.teamspace_thread.as_ref()).await;
                }
            }
        }
        Ok(())
    }

    async fn notify_undeliverable_once(&self, rq: &RequestId, reason: &str, thread: Option<&ThreadRef>) {
        let key = format!("transcripts:undeliverable_notified:{rq}");
        if self.journal.meta_get(&key).ok().flatten().is_some() {
            return;
        }
        let Some(thread) = thread else { return };
        let body = format!("transcript request {rq} is undeliverable ({reason}); it stays pending until a summarizer is available.");
        let op_key = OpKey(format!("undeliverable:{rq}"));
        match self.threads.notify(thread, Severity::Warn, &body, &op_key).await {
            Ok(()) => {
                let _ = self.journal.meta_set(&key, "1");
            }
            Err(e) => eprintln!("herdr-graph: transcripts: cannot notify about {rq}: {e}"),
        }
    }

    // -----------------------------------------------------------------------------------------
    // registration
    // -----------------------------------------------------------------------------------------

    /// Delivery effect source and executor with the reconciler.
    pub fn register_with(&self, reconciler: &Reconciler) {
        reconciler.register_source(Arc::new(DeliverySource));
        reconciler.register_executor(Arc::new(DeliveryExecutor::new(self.core.clone())));
    }

    /// IPC commands `request.list|ack|complete` and `session.report`, and the `transcripts` status component.
    pub fn register_commands(self: &Arc<Self>, reg: &mut Registry) {
        let me = self.clone();
        reg.command("request.list", move |_cx: CommandCtx, args: serde_json::Value| {
            let me = me.clone();
            async move { me.cmd_list(args) }
        });
        let me = self.clone();
        reg.command("request.ack", move |_cx: CommandCtx, args: serde_json::Value| {
            let me = me.clone();
            async move { me.cmd_ack(args).await }
        });
        let me = self.clone();
        reg.command("request.complete", move |cx: CommandCtx, args: serde_json::Value| {
            let me = me.clone();
            async move { me.cmd_complete(cx.caller, args).await }
        });
        let me = self.clone();
        reg.command("session.report", move |cx: CommandCtx, args: serde_json::Value| {
            let me = me.clone();
            async move { me.session_report_live(cx.caller, args).await }
        });
        let me = self.clone();
        reg.status_provider("transcripts", Arc::new(move || me.status_json()));
    }

    /// Background loops: `SessionEnded` consumer, transcript watcher, liveness scan.
    pub fn register_loops(self: &Arc<Self>, reg: &mut Registry) {
        let me = self.clone();
        reg.background("transcripts.session_ended", move |sd| async move { me.run_session_ended(sd).await });
        let me = self.clone();
        reg.background("transcripts.watcher", move |sd| async move { me.run_watcher(sd).await });
        let me = self.clone();
        reg.background("transcripts.liveness", move |sd| async move { me.run_liveness(sd).await });
        let me = self.clone();
        reg.background("transcripts.spool", move |sd| async move { me.run_spool(sd).await });
    }

    async fn run_session_ended(self: Arc<Self>, mut sd: Shutdown) -> anyhow::Result<()> {
        let rx = self.session_ended.lock().unwrap_or_else(|e| e.into_inner()).take();
        let Some(mut rx) = rx else {
            sd.wait().await;
            return Ok(());
        };
        loop {
            tokio::select! {
                _ = sd.wait() => return Ok(()),
                ev = rx.recv() => match ev {
                    Ok(ev) => self.on_session_ended(ev).await,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("herdr-graph: transcripts: missed {n} session-ended events; the watcher and liveness scans catch up");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        sd.wait().await;
                        return Ok(());
                    }
                },
            }
        }
    }

    async fn run_watcher(self: Arc<Self>, mut sd: Shutdown) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                _ = sd.wait() => return Ok(()),
                _ = tokio::time::sleep(self.tuning().watch_interval) => {}
            }
            if let Err(e) = self.watch_once().await {
                eprintln!("herdr-graph: transcripts: watcher pass failed: {}", e.message);
            }
        }
    }

    async fn run_liveness(self: Arc<Self>, mut sd: Shutdown) -> anyhow::Result<()> {
        // The "on daemon start" pass: the writer loop runs by now, so recovered requests can commit.
        if let Err(e) = self.recover_session_requests().await {
            eprintln!("herdr-graph: transcripts: session-end recovery failed: {}", e.message);
        }
        loop {
            tokio::select! {
                _ = sd.wait() => return Ok(()),
                _ = tokio::time::sleep(self.tuning().liveness_interval) => {}
            }
            if let Err(e) = self.liveness_scan().await {
                eprintln!("herdr-graph: transcripts: liveness scan failed: {}", e.message);
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // commands
    // -----------------------------------------------------------------------------------------

    fn cmd_list(&self, args: serde_json::Value) -> Result<serde_json::Value, CommandError> {
        let filter: ListFilter = serde_json::from_value(args).unwrap_or_default();
        let view = self.view().map_err(internal)?;
        let rows = requests::list_view(&view, filter).map_err(internal)?;
        Ok(json!({ "requests": rows }))
    }

    fn request_arg(args: &serde_json::Value) -> Result<RequestId, CommandError> {
        let raw = args.get("request").and_then(|v| v.as_str()).ok_or_else(|| CommandError::bad_request("needs {request}"))?;
        RequestId::parse(raw).map_err(|e| CommandError::bad_request(e.to_string()))
    }

    async fn cmd_ack(&self, args: serde_json::Value) -> Result<serde_json::Value, CommandError> {
        let rq = Self::request_arg(&args)?;
        let at = self.clock.now();
        self.commit(bookkeeping_request("request_ack", json!({ "rq": rq, "at": at }))).await?;
        Ok(json!({ "request": rq, "status": "dispatched" }))
    }

    async fn cmd_complete(&self, caller: CallerInfo, args: serde_json::Value) -> Result<serde_json::Value, CommandError> {
        let rq = Self::request_arg(&args)?;
        let output = args
            .get("output")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CommandError::bad_request("needs {output}"))?
            .to_owned();
        let covered: ByteRange = serde_json::from_value(args.get("covered").cloned().unwrap_or_default())
            .map_err(|e| CommandError::bad_request(format!("covered: {e}")))?;
        if covered.start > covered.end {
            return Err(CommandError::bad_request("covered start must not exceed its end"));
        }
        let reported_by = {
            let view = self.view().map_err(internal)?;
            let g = Graph::load(&view).map_err(internal)?;
            match caller_object(&g, &caller) {
                Some(o) => o,
                // A human at a terminal: attribute to the source seat the request is about.
                None => {
                    let req = layout::list_requests(&view)
                        .map_err(internal)?
                        .into_iter()
                        .map(|(_, r)| r)
                        .find(|r| r.id == rq)
                        .ok_or_else(|| CommandError::rejected(format!("{rq} does not exist")))?;
                    let tr = layout::list_transcripts(&view)
                        .map_err(internal)?
                        .into_iter()
                        .map(|(_, t)| t)
                        .find(|t| t.id == req.transcript)
                        .ok_or_else(|| CommandError::rejected(format!("{} does not exist", req.transcript)))?;
                    tr.seat.to_any()
                }
            }
        };
        let at = self.clock.now();
        self.commit(bookkeeping_request(
            "request_complete",
            json!({ "rq": rq, "output_ref": output, "covered": covered, "reported_by": reported_by, "at": at }),
        ))
        .await?;
        Ok(json!({ "request": rq, "status": "completed" }))
    }

    fn status_json(&self) -> serde_json::Value {
        let Ok(view) = self.view() else { return json!({ "error": "no committed head" }) };
        let Ok(all) = layout::list_requests(&view) else { return json!({ "error": "cannot list requests" }) };
        let (mut pending, mut dispatched, mut unresolved, mut completed) = (0, 0, 0, 0);
        let mut undeliverable = Vec::new();
        for (_, r) in all {
            match r.status {
                RequestStatus::Pending | RequestStatus::Delivered => pending += 1,
                RequestStatus::Dispatched => dispatched += 1,
                RequestStatus::Completed => completed += 1,
                RequestStatus::Unresolved if !is_merged(&r) => unresolved += 1,
                RequestStatus::Unresolved => {}
            }
            if let Some(why) = &r.undeliverable {
                undeliverable.push(json!({ "request": r.id, "reason": why }));
            }
        }
        json!({
            "delivery": "fallback: ACK lives in graph, not threads (unless the threads-service-ack path is active)",
            "pending": pending, "dispatched": dispatched, "unresolved": unresolved, "completed": completed,
            "undeliverable": undeliverable,
        })
    }

}

pub(crate) fn internal(e: impl std::fmt::Display) -> CommandError {
    CommandError::internal(e.to_string())
}
