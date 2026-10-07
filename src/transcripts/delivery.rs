//! Request delivery (spec §7.4, §8.2): the capability-chosen adapter, the `DeliverRequest` effect source and
//! executor, and the shared delivery routine the liveness scan reuses.
//!
//! `NotifyFallback` (default): `Notify{Warn}` on the summarizer seat channel naming the request; the summarizer
//! ACKs with `herdr-graph request ack`. `ServiceAck` (cargo feature `threads-service-ack`): an ACK-required
//! `send_request` whose receipts the liveness scan polls. Neither ever completes a request.
use super::mutations::{bookkeeping_request, is_merged};
use super::requests::{Destination, resolve_destination};
use crate::model::effect::{EffectKind, EffectRecord, EffectStatus};
use crate::model::request::{DeliveryAttempt, ProcessingRequest, RequestStatus};
use crate::model::{ByteRange, HerdrPaneId, RequestId, Timestamp, TranscriptId};
use crate::ports::store::StoreError;
use crate::ports::threads::{
    DeliveryCapability, OpKey, Severity, ThreadRef, ThreadsError, ThreadsPort,
};
use crate::ports::writer::Writer;
use crate::reconcile::{DiffCx, EffectExecutor, EffectSource, ExecCx, ExecOutcome, PlannedEffect};
use crate::store::layout;
use crate::store::tree::TreeRead;
use crate::threads::PaneSeatMap;
use crate::threads::effects::Graph;
use std::path::PathBuf;
use std::sync::Arc;

/// What delivery decided; the executor maps it to an effect outcome, the liveness scan just retries later.
#[derive(Debug, Clone, PartialEq)]
pub enum DeliverOutcome {
    Done,
    /// Retry later with backoff.
    Transient(String),
    /// Not deliverable yet (channel not created, panes not registered): wait without counting an attempt.
    Deferred(String),
    /// No longer implied by the committed state.
    Obsolete,
    Failed(String),
}

fn threads_outcome(e: ThreadsError) -> DeliverOutcome {
    match e {
        ThreadsError::ServiceBusy => DeliverOutcome::Transient("threads service busy".into()),
        ThreadsError::Disconnected(m) => DeliverOutcome::Transient(m),
        ThreadsError::Unsupported => {
            DeliverOutcome::Failed("operation unsupported by this threads service".into())
        }
        ThreadsError::Rejected(m) => DeliverOutcome::Failed(m),
    }
}

/// Everything a delivery needs, read from the committed tree before any await.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub rq: RequestId,
    /// Delivery attempts already recorded; the next attempt's number.
    pub n: usize,
    pub transcript: TranscriptId,
    pub path: PathBuf,
    pub range: ByteRange,
    pub source_seat: String,
    pub thread: Option<ThreadRef>,
    #[cfg_attr(not(feature = "threads-service-ack"), allow(dead_code))]
    pub panes: Vec<HerdrPaneId>,
}

impl Prepared {
    pub fn op_key(&self) -> OpKey {
        OpKey(format!("deliver:{}:{}", self.rq, self.n))
    }

    /// The fallback notification text.
    pub fn body(&self, retry: bool) -> String {
        let (rq, ByteRange { start, end }) = (&self.rq, self.range);
        format!(
            "{}transcript request {rq}: {} bytes {start}-{end} from seat {}. Run: herdr-graph request ack {rq} when \
             dispatched; herdr-graph request complete {rq} --output <path> --covered {start}-{end} when done.",
            if retry { "RETRY " } else { "" },
            self.path.display(),
            self.source_seat,
        )
    }
}

/// Read what delivering `rq` needs. `Err` is the outcome when it cannot be delivered now.
pub fn prepare(tree: &dyn TreeRead, rq: &ProcessingRequest) -> Result<Prepared, DeliverOutcome> {
    let read = |e: StoreError| DeliverOutcome::Transient(format!("cannot read the graph: {e}"));
    let g = Graph::load(tree).map_err(read)?;
    let tr = layout::list_transcripts(tree)
        .map_err(read)?
        .into_iter()
        .map(|(_, t)| t)
        .find(|t| t.id == rq.transcript)
        .ok_or(DeliverOutcome::Obsolete)?;
    let source_seat = tr
        .capture_attribution
        .as_ref()
        .map(|capture| capture.seat_name.clone())
        .unwrap_or_else(|| format!("{} (capture name unknown)", tr.seat));
    match resolve_destination(&g, tree, &tr.seat).map_err(read)? {
        Destination::Ready { thread, panes, .. } => Ok(Prepared {
            rq: rq.id.clone(),
            n: rq.delivery.attempts.len(),
            transcript: tr.id,
            path: tr.transcript_path,
            range: rq.range,
            source_seat,
            thread,
            panes,
        }),
        Destination::Relaunch { .. } => Err(DeliverOutcome::Deferred(
            "summarizer has no occupant yet".into(),
        )),
        Destination::Undeliverable { reason, .. } => Err(DeliverOutcome::Deferred(reason.into())),
    }
}

/// Sends requests through the threads port and records each attempt.
pub struct DeliveryCore {
    pub(crate) threads: Arc<dyn ThreadsPort>,
    #[cfg_attr(not(feature = "threads-service-ack"), allow(dead_code))]
    pub(crate) mapping: Arc<dyn PaneSeatMap>,
}

impl DeliveryCore {
    pub fn new(threads: Arc<dyn ThreadsPort>, mapping: Arc<dyn PaneSeatMap>) -> Self {
        Self { threads, mapping }
    }

    /// Deliver one attempt (fresh op key per attempt number) and record it as a bookkeeping write that only
    /// applies while the attempt count is still `p.n`, so a duplicate delivery of the same attempt is a no-op.
    pub async fn deliver(
        &self,
        p: &Prepared,
        writer: &dyn Writer,
        now: Timestamp,
        retry: bool,
    ) -> DeliverOutcome {
        let Some(thread) = p.thread.clone() else {
            return DeliverOutcome::Deferred("summarizer channel not created yet".into());
        };
        let capability = match self.threads.delivery_capability().await {
            Ok(c) => c,
            Err(e) => return threads_outcome(e),
        };
        let op_key = p.op_key();
        let message_id = match self.send(capability, &thread, p, &op_key, retry).await {
            Ok(m) => m,
            Err(o) => return o,
        };
        let attempt = DeliveryAttempt {
            op_key: op_key.0.clone(),
            message_id,
            at: now,
            retry,
        };
        let request = bookkeeping_request(
            "request_delivery",
            serde_json::json!({ "rq": p.rq, "expect_attempts": p.n, "attempt": attempt }),
        );
        match writer.admit(request) {
            Ok(_) => DeliverOutcome::Done,
            Err(e) => DeliverOutcome::Transient(format!("bookkeeping write: {e}")),
        }
    }

    async fn send(
        &self,
        capability: DeliveryCapability,
        thread: &ThreadRef,
        p: &Prepared,
        op_key: &OpKey,
        retry: bool,
    ) -> Result<Option<String>, DeliverOutcome> {
        match capability {
            #[cfg(feature = "threads-service-ack")]
            DeliveryCapability::ServiceAck => {
                let mut recipients = Vec::new();
                for pane in &p.panes {
                    match self.mapping.seat_for(pane).await {
                        Ok(Some(seat)) => recipients.push(seat),
                        Ok(None) => {}
                        Err(e) => return Err(threads_outcome(e)),
                    }
                }
                if recipients.is_empty() {
                    return Err(DeliverOutcome::Deferred(
                        "summarizer panes are not threads seats yet".into(),
                    ));
                }
                let id = self
                    .threads
                    .send_request(thread, &recipients, &p.body(retry), op_key)
                    .await
                    .map_err(threads_outcome)?;
                Ok(Some(id.0))
            }
            _ => {
                self.threads
                    .notify(thread, Severity::Warn, &p.body(retry), op_key)
                    .await
                    .map_err(threads_outcome)?;
                Ok(None)
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// reconciler effect family
// ---------------------------------------------------------------------------------------------

/// One `DeliverRequest` effect per request that is pending, undelivered and has a ready destination. The
/// effect identity folds in the attempt number, so a recorded attempt ends the want and a retry is a new one.
pub struct DeliverySource;

impl EffectSource for DeliverySource {
    fn effects(&self, cx: &DiffCx<'_>) -> Vec<PlannedEffect> {
        let requests = match layout::list_requests(cx.tree) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "herdr-graph: transcripts: cannot list requests at {}: {e}",
                    cx.head.0
                );
                return Vec::new();
            }
        };
        let pending: Vec<ProcessingRequest> = requests
            .into_iter()
            .map(|(_, r)| r)
            .filter(|r| {
                r.status == RequestStatus::Pending && !is_merged(r) && r.unresolved.is_none()
            })
            .collect();
        if pending.is_empty() {
            return Vec::new();
        }
        let Ok(g) = Graph::load(cx.tree) else {
            return Vec::new();
        };
        let Ok(transcripts) = layout::list_transcripts(cx.tree) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for rq in pending {
            let Some(tr) = transcripts
                .iter()
                .map(|(_, t)| t)
                .find(|t| t.id == rq.transcript)
            else {
                continue;
            };
            if !matches!(
                resolve_destination(&g, cx.tree, &tr.seat),
                Ok(Destination::Ready { .. })
            ) {
                continue;
            }
            let object = rq.id.to_any();
            let kind = EffectKind::DeliverRequest;
            let n = rq.delivery.attempts.len() as u64;
            out.push(PlannedEffect {
                record: EffectRecord {
                    id: EffectRecord::identity(&rq.created_by_op, &object, &kind, n),
                    op: rq.created_by_op.clone(),
                    object,
                    kind,
                    object_rev: n,
                    fencing_rev: n,
                    status: EffectStatus::Pending,
                    predicted: vec![],
                    nonce_label: None,
                    attempts: 0,
                    last_error: None,
                    updated_at: cx.now,
                    sched: Default::default(),
                },
                deps: vec![],
            });
        }
        out
    }
}

pub struct DeliveryExecutor {
    core: Arc<DeliveryCore>,
}

impl DeliveryExecutor {
    pub fn new(core: Arc<DeliveryCore>) -> Self {
        Self { core }
    }
}

#[async_trait::async_trait]
impl EffectExecutor for DeliveryExecutor {
    fn handles(&self, kind: &EffectKind) -> bool {
        *kind == EffectKind::DeliverRequest
    }

    async fn execute(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome {
        let prepared = {
            let Ok(rq_id) = RequestId::parse(e.object.as_str()) else {
                return ExecOutcome::Failed(format!("{} is not a request id", e.object));
            };
            let path = match layout::locate(cx.tree, &rq_id.to_any()) {
                Ok(Some(loc)) => loc.record_path,
                Ok(None) => return ExecOutcome::Obsolete,
                Err(err) => return ExecOutcome::Transient(format!("cannot locate {rq_id}: {err}")),
            };
            let rq = match crate::store::record::read_toml::<ProcessingRequest>(cx.tree, &path) {
                Ok(Some(r)) => r,
                Ok(None) => return ExecOutcome::Obsolete,
                Err(err) => return ExecOutcome::Transient(format!("cannot read {rq_id}: {err}")),
            };
            if rq.status != RequestStatus::Pending
                || rq.unresolved.is_some()
                || rq.delivery.attempts.len() as u64 != e.object_rev
            {
                return ExecOutcome::Obsolete;
            }
            match prepare(cx.tree, &rq) {
                Ok(p) => p,
                Err(DeliverOutcome::Deferred(why)) => return ExecOutcome::Deferred(why),
                Err(DeliverOutcome::Obsolete) => return ExecOutcome::Obsolete,
                Err(DeliverOutcome::Transient(m)) => return ExecOutcome::Transient(m),
                Err(other) => return ExecOutcome::Failed(format!("{other:?}")),
            }
        };
        match self.core.deliver(&prepared, cx.writer, cx.now, false).await {
            DeliverOutcome::Done => ExecOutcome::Done,
            DeliverOutcome::Transient(m) => ExecOutcome::Transient(m),
            DeliverOutcome::Deferred(m) => ExecOutcome::Deferred(m),
            DeliverOutcome::Obsolete => ExecOutcome::Obsolete,
            DeliverOutcome::Failed(m) => ExecOutcome::Failed(m),
        }
    }
}
