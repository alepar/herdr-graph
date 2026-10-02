//! Daemon-owned liveness scan (spec §8.2), every 10 minutes and driven by the injected `Clock`:
//!
//! * pending and undelivered: deliver;
//! * delivered but not ACKed after 30 min: one reminder `Notify{Warn}` naming the request and its original
//!   message id; the exactly-once `Send` is never repeated under the same key;
//! * ACKed (dispatched) but not completed after 6 h: a new delivery marked `retry` under a fresh op key that
//!   records its own message id;
//! * `Unresolved` requests stay visible and are never touched.
//!
//! The summarizer role's own `/loop` scan is a convenience; this scan is the recovery mechanism.
use super::delivery::{DeliverOutcome, Prepared, prepare};
use super::mutations::{bookkeeping_request, is_merged};
use super::{Transcripts, internal};
use crate::daemon::registry::CommandError;
use crate::model::Timestamp;
use crate::model::request::{ProcessingRequest, RequestStatus};
use crate::ports::threads::{MessageRef, OpKey, ReceiptState, Severity};
use crate::store::layout;
use serde_json::json;

enum Action {
    Deliver { p: Prepared, retry: bool },
    PollReceipt { rq: ProcessingRequest },
    Remind { p: Prepared, rq: ProcessingRequest },
}

fn last_attempt(rq: &ProcessingRequest) -> Option<Timestamp> {
    rq.delivery.attempts.last().map(|a| a.at)
}

impl Transcripts {
    /// One scan: route pending requests, then act on every open request by its age.
    pub async fn liveness_scan(&self) -> Result<(), CommandError> {
        self.process_pending().await;
        let now = self.clock.now();
        let tuning = self.tuning();
        let actions: Vec<Action> = {
            let view = self.view().map_err(internal)?;
            let mut out = Vec::new();
            for (_, rq) in layout::list_requests(&view).map_err(internal)? {
                if is_merged(&rq) {
                    continue;
                }
                match rq.status {
                    RequestStatus::Pending if rq.unresolved.is_none() => {
                        if let Ok(p) = prepare(&view, &rq) {
                            out.push(Action::Deliver { p, retry: false });
                        }
                    }
                    RequestStatus::Delivered => {
                        if rq.delivery.message_id.is_some() {
                            out.push(Action::PollReceipt { rq: rq.clone() });
                        }
                        let Some(last) = last_attempt(&rq) else { continue };
                        let reminded = rq.delivery.reminded_at.is_some_and(|r| r >= last);
                        if !reminded
                            && now - last >= tuning.remind_after
                            && let Ok(p) = prepare(&view, &rq)
                        {
                            out.push(Action::Remind { p, rq });
                        }
                    }
                    RequestStatus::Dispatched => {
                        let base = [rq.delivery.dispatched_at, last_attempt(&rq)].into_iter().flatten().max();
                        if base.is_some_and(|b| now - b >= tuning.retry_after)
                            && let Ok(p) = prepare(&view, &rq)
                        {
                            out.push(Action::Deliver { p, retry: true });
                        }
                    }
                    _ => {}
                }
            }
            out
        };
        for action in actions {
            match action {
                Action::Deliver { p, retry } => match self.core.deliver(&p, &*self.writer, now, retry).await {
                    DeliverOutcome::Done | DeliverOutcome::Deferred(_) | DeliverOutcome::Obsolete => {}
                    DeliverOutcome::Transient(m) | DeliverOutcome::Failed(m) => {
                        eprintln!("herdr-graph: transcripts: delivery of {} failed: {m}", p.rq);
                    }
                },
                Action::PollReceipt { rq } => self.poll_receipt(&rq).await,
                Action::Remind { p, rq } => self.remind(&p, &rq, now).await,
            }
        }
        Ok(())
    }

    /// A service receipt that reports an acknowledgement records `dispatched_at` (still not success).
    async fn poll_receipt(&self, rq: &ProcessingRequest) {
        let Some(id) = rq.delivery.message_id.clone() else { return };
        let receipts = match self.threads.receipt_state(&[MessageRef(id)]).await {
            Ok(r) => r,
            Err(_) => return,
        };
        let acked = receipts
            .iter()
            .flat_map(|m| m.recipients.iter())
            .find_map(|r| match &r.state {
                ReceiptState::Acknowledged { at } => Some(*at),
                _ => None,
            });
        if let Some(at) = acked
            && let Err(e) = self.commit(bookkeeping_request("request_ack", json!({ "rq": rq.id, "at": at }))).await
        {
            eprintln!("herdr-graph: transcripts: cannot record receipt of {}: {}", rq.id, e.message);
        }
    }

    /// A reminder, never a re-send: the original message stays the only one under its key.
    async fn remind(&self, p: &Prepared, rq: &ProcessingRequest, now: Timestamp) {
        let Some(thread) = p.thread.clone() else { return };
        let original = rq.delivery.message_id.as_deref().map(|m| format!(" (message {m})")).unwrap_or_default();
        let body = format!(
            "reminder: transcript request {}{original} was delivered and has not been acknowledged. Run: herdr-graph \
             request ack {} when you have dispatched it.",
            rq.id, rq.id
        );
        let key = OpKey(format!("remind:{}:{}", rq.id, rq.delivery.attempts.len()));
        if let Err(e) = self.threads.notify(&thread, Severity::Warn, &body, &key).await {
            eprintln!("herdr-graph: transcripts: cannot remind about {}: {e}", rq.id);
            return;
        }
        if let Err(e) = self.commit(bookkeeping_request("request_delivery", json!({ "rq": rq.id, "reminded_at": now }))).await {
            eprintln!("herdr-graph: transcripts: cannot record reminder of {}: {}", rq.id, e.message);
        }
    }
}
