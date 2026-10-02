//! Daemon-owned liveness scan (spec §8.2), every 10 minutes and driven by the injected `Clock`:
//!
//! * pending and undelivered: deliver;
//! * delivered but not ACKed after 30 min: one reminder `Notify{Warn}` naming the request and its original
//!   message id; the exactly-once `Send` is never repeated under the same key;
//! * ACKed (dispatched) but not completed after 6 h: a new delivery marked `retry` under a fresh op key that
//!   records its own message id;
//! * `Unresolved` requests stay visible and are never touched;
//! * recovery first: an ended session of a `summaries = true` seat whose transcript end no request or coverage
//!   reaches gets its request re-derived from committed state (the in-memory `SessionEnded` can be lost).
//!
//! The summarizer role's own `/loop` scan is a convenience; this scan is the recovery mechanism.
use super::delivery::{DeliverOutcome, Prepared, prepare};
use super::mutations::{bookkeeping_request, covered_end, is_merged};
use super::requests::stat_file;
use super::{Transcripts, internal};
use crate::daemon::registry::CommandError;
use crate::model::effective::resolve_in;
use crate::model::{CloneId, NsId};
use crate::threads::effects::Graph;
use std::path::PathBuf;
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
        if let Err(e) = self.recover_session_requests().await {
            eprintln!("herdr-graph: transcripts: session-end recovery failed: {}", e.message);
        }
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

    /// Spec §8.2 / §3.6 recovery: the request a session end implies is derived from committed state, not from the
    /// in-memory `SessionEnded` event (which a crash, SIGTERM, halted writer or timeout can lose). Returns how many
    /// requests it asked for.
    pub async fn recover_session_requests(&self) -> Result<usize, CommandError> {
        let candidates: Vec<(CloneId, NsId, Option<PathBuf>)> = {
            let view = self.view().map_err(internal)?;
            let g = Graph::load(&view).map_err(internal)?;
            let transcripts: Vec<_> = layout::list_transcripts(&view).map_err(internal)?.into_iter().map(|(_, t)| t).collect();
            let requests: Vec<_> = layout::list_requests(&view).map_err(internal)?.into_iter().map(|(_, r)| r).collect();
            let mut out = Vec::new();
            for clone in g.clones.values() {
                for s in clone.sessions.iter().filter(|s| s.ended.is_some()) {
                    let Some(seat) = g.seats.get(&clone.seat) else { continue };
                    if !resolve_in(&view, seat).map_err(internal)?.summaries {
                        continue;
                    }
                    let tr = transcripts.iter().filter(|t| t.native_session == s.id).max_by(|a, b| a.id.cmp(&b.id));
                    let missing = match (tr, &s.transcript_path) {
                        (None, _) => true,
                        (Some(tr), Some(p)) => stat_file(p).is_ok_and(|state| {
                            let own: Vec<_> = requests.iter().filter(|r| r.transcript == tr.id).cloned().collect();
                            state.aligned > covered_end(tr, &own)
                        }),
                        (Some(_), None) => false,
                    };
                    if missing {
                        out.push((clone.id.clone(), s.id.clone(), s.transcript_path.clone()));
                    }
                }
            }
            out
        };
        let mut asked = 0;
        for (clone, ns, path) in candidates {
            match self.request_for(&clone, &ns, path, None, false).await {
                Ok(()) => asked += 1,
                Err(e) => eprintln!("herdr-graph: transcripts: recovery request for {ns} failed: {}", e.message),
            }
        }
        Ok(asked)
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
