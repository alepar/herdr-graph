//! Rejection reminder scheduler (spec §3.7): re-notify a requester of a rejected (or failed) op at +1 h, +6 h and
//! +24 h after the initial notice, stopped by a replacement (`supersedes`, which moves the op to `superseded`) or by
//! `cancel`. Owned by hg-zmi.17.
use super::ops::{reminder_count_key, reminders_stopped_key};
use crate::daemon::registry::{Registry, Shutdown};
use crate::journal::{Journal, OpRow};
use crate::model::common::Lifecycle;
use crate::model::operation::OpState;
use crate::model::seat::SeatRecord;
use crate::model::{OpId, Timestamp};
use crate::ports::clock::Clock;
use crate::ports::store::Store;
use crate::ports::threads::{OpKey, Severity, ThreadRef, ThreadsPort};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::CommitView;
use chrono::Duration as Span;
use std::sync::Arc;
use std::time::Duration;

/// When each notice is due, measured from the moment the op was rejected: the initial one, then +1 h, +6 h, +24 h.
pub const SCHEDULE_HOURS: [i64; 4] = [0, 1, 6, 24];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderSent {
    pub op: OpId,
    /// 0 = the initial notice, 1..=3 = the +1 h, +6 h, +24 h reminders.
    pub index: u32,
    pub thread: ThreadRef,
}

pub struct ReminderScheduler {
    pub journal: Arc<Journal>,
    pub store: Arc<dyn Store>,
    pub threads: Arc<dyn ThreadsPort>,
    pub clock: Arc<dyn Clock>,
}

/// A notice the sync pass decided to send.
struct Due {
    op: OpId,
    index: u32,
    thread: ThreadRef,
    body: String,
}

fn notice_body(row: &OpRow) -> String {
    let (reason, explanation) = row
        .rejection
        .as_ref()
        .map(|r| (r.reason.clone(), r.explanation.clone()))
        .unwrap_or_else(|| ("unknown".into(), String::new()));
    let what = if row.state == OpState::Failed {
        "failed"
    } else {
        "was rejected"
    };
    format!(
        "op {op} ({kind}) {what}: {reason} — {explanation}. Submit a replacement with --supersedes {op}, or run herdr-graph cancel {op}.",
        op = row.op,
        kind = crate::writer::kind_name(row.request.kind),
    )
}

impl ReminderScheduler {
    /// The requester's channel thread: its seat must exist, be live, and have a managed channel.
    fn thread_of(&self, row: &OpRow) -> Option<ThreadRef> {
        let seat = row.request.requester.seat.as_ref()?;
        let head = self.store.head().ok()?;
        let view = CommitView {
            store: &*self.store,
            at: head,
        };
        let loc = layout::locate(&view, &seat.to_any()).ok()??;
        let rec: SeatRecord = read_toml(&view, &loc.record_path).ok()??;
        if rec.lifecycle == Lifecycle::Retired {
            return None;
        }
        rec.channel.thread_id.map(ThreadRef)
    }

    fn sent_count(&self, op: &OpId) -> u32 {
        self.journal
            .meta_get(&reminder_count_key(op))
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    /// Pure decision pass: which ops owe a notice right now. After downtime only the latest due notice is sent.
    fn due(&self, now: Timestamp) -> Vec<Due> {
        let Ok(rows) = self
            .journal
            .list(&[OpState::Rejected, OpState::Failed], usize::MAX >> 1)
        else {
            return vec![];
        };
        let mut out = Vec::new();
        for row in rows {
            if self
                .journal
                .meta_get(&reminders_stopped_key(&row.op))
                .ok()
                .flatten()
                .is_some()
            {
                continue;
            }
            let sent = self.sent_count(&row.op);
            let elapsed = now - row.updated_at;
            let due_count = SCHEDULE_HOURS
                .iter()
                .filter(|h| elapsed >= Span::hours(**h))
                .count() as u32;
            if due_count <= sent {
                continue;
            }
            let Some(thread) = self.thread_of(&row) else {
                continue;
            };
            out.push(Due {
                op: row.op.clone(),
                index: due_count - 1,
                thread,
                body: notice_body(&row),
            });
        }
        out
    }

    /// One scheduler pass: send every due notice (Warn, op key `rem:<op>:<n>`) and record the count. A failed
    /// notify leaves the count alone so the next tick retries.
    pub async fn tick(&self) -> Vec<ReminderSent> {
        let mut sent = Vec::new();
        for due in self.due(self.clock.now()) {
            let key = OpKey(format!("rem:{}:{}", due.op, due.index));
            if let Err(e) = self
                .threads
                .notify(&due.thread, Severity::Warn, &due.body, &key)
                .await
            {
                eprintln!("herdr-graph: reminder for {} not delivered: {e}", due.op);
                continue;
            }
            if let Err(e) = self
                .journal
                .meta_set(&reminder_count_key(&due.op), &(due.index + 1).to_string())
            {
                eprintln!("herdr-graph: could not record reminder for {}: {e}", due.op);
            }
            sent.push(ReminderSent {
                op: due.op,
                index: due.index,
                thread: due.thread,
            });
        }
        sent
    }
}

/// Registers the scheduler as a daemon loop running `tick` every `period` (60 s in the daemon).
pub fn register_loop(reg: &mut Registry, sched: Arc<ReminderScheduler>, period: Duration) {
    reg.background("plan.reminders", move |mut shutdown: Shutdown| async move {
        loop {
            if shutdown.is_set() {
                return Ok(());
            }
            sched.tick().await;
            tokio::select! {
                _ = tokio::time::sleep(period) => {}
                _ = shutdown.wait() => return Ok(()),
            }
        }
    });
}

/// Test double for the threads port, shared with the `ops` tests.
#[cfg(test)]
pub(crate) mod stub {
    use crate::model::clone::InvitationState;
    use crate::model::clone::InviteConstraint;
    use crate::ports::threads::*;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct RecordingThreads {
        notes: Mutex<Vec<(ThreadRef, Severity, String, OpKey)>>,
        pub fail: Mutex<bool>,
    }

    impl RecordingThreads {
        pub fn sent(&self) -> Vec<(ThreadRef, Severity, String, OpKey)> {
            self.notes.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl ThreadsPort for RecordingThreads {
        async fn ensure_thread(
            &self,
            _: ChannelScope,
            _: &str,
            _: &OpKey,
        ) -> Result<ThreadRef, ThreadsError> {
            Err(ThreadsError::Unsupported)
        }
        async fn invite(
            &self,
            _: &ThreadRef,
            _: &ThreadsSeatRef,
            _: InviteConstraint,
            _: &OpKey,
        ) -> Result<(), ThreadsError> {
            Err(ThreadsError::Unsupported)
        }
        async fn membership(
            &self,
            _: &ThreadRef,
            _: &ThreadsSeatRef,
        ) -> Result<Option<InvitationState>, ThreadsError> {
            Err(ThreadsError::Unsupported)
        }
        async fn notify(
            &self,
            thread: &ThreadRef,
            severity: Severity,
            body: &str,
            op_key: &OpKey,
        ) -> Result<(), ThreadsError> {
            if *self.fail.lock().unwrap() {
                return Err(ThreadsError::ServiceBusy);
            }
            self.notes.lock().unwrap().push((
                thread.clone(),
                severity,
                body.to_owned(),
                op_key.clone(),
            ));
            Ok(())
        }
        async fn set_topic(&self, _: &ThreadRef, _: &str, _: &OpKey) -> Result<(), ThreadsError> {
            Err(ThreadsError::Unsupported)
        }
        async fn release_requirement(
            &self,
            _: &ThreadRef,
            _: &ThreadsSeatRef,
            _: &OpKey,
        ) -> Result<(), ThreadsError> {
            Err(ThreadsError::Unsupported)
        }
        async fn send_request(
            &self,
            _: &ThreadRef,
            _: &[ThreadsSeatRef],
            _: &str,
            _: &OpKey,
        ) -> Result<MessageRef, ThreadsError> {
            Err(ThreadsError::Unsupported)
        }
        async fn receipt_state(
            &self,
            _: &[MessageRef],
        ) -> Result<Vec<MessageReceipts>, ThreadsError> {
            Err(ThreadsError::Unsupported)
        }
        async fn delivery_capability(&self) -> Result<DeliveryCapability, ThreadsError> {
            Ok(DeliveryCapability::NotifyFallback)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::kinds_extra::testkit::*;
    use super::super::ops::{cancel, reassign};
    use super::stub::RecordingThreads;
    use super::*;
    use crate::model::change::Requester;

    struct Rig {
        fx: Fx,
        threads: Arc<RecordingThreads>,
        sched: ReminderScheduler,
        op: OpId,
    }

    /// Seats `one` (with a channel) and `two`; an op rejected for `requester(one)` at t0.
    fn rig_with(
        requester: impl Fn(&crate::model::seat::SeatRecord) -> Requester,
        channel: bool,
    ) -> Rig {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha --active");
        commit(&fx, "seat create two --teamspace alpha --active");
        if channel {
            set_channel(&fx, &seat(&fx, "one").id, "th_one");
        }
        let row = rejected_op(&fx, requester(&seat(&fx, "one")));
        // The writer stamps `updated_at` from the clock when it rejects, so all offsets are relative to t0.
        assert_eq!(row.updated_at, t0());
        let threads = Arc::new(RecordingThreads::default());
        let sched = ReminderScheduler {
            journal: fx.w.journal().clone(),
            store: fx.store.clone(),
            threads: threads.clone(),
            clock: fx.clock.clone(),
        };
        Rig {
            fx,
            threads,
            sched,
            op: row.op,
        }
    }

    fn rig() -> Rig {
        rig_with(requester_of, true)
    }

    fn advance(r: &Rig, d: chrono::Duration) {
        r.fx.clock.advance(d);
    }

    #[tokio::test]
    async fn reminders_fire_at_1h_6h_24h_then_stop() {
        let r = rig();
        let mut got: Vec<(i64, u32)> = Vec::new();
        let mut elapsed = 0;
        // 30-minute ticks over two days
        for _ in 0..96 {
            for s in r.sched.tick().await {
                assert_eq!(s.op, r.op);
                got.push((elapsed, s.index));
            }
            advance(&r, chrono::Duration::minutes(30));
            elapsed += 30;
        }
        assert_eq!(
            got,
            vec![(0, 0), (60, 1), (360, 2), (1440, 3)],
            "initial, +1h, +6h, +24h, nothing after"
        );
        let sent = r.threads.sent();
        assert_eq!(sent.len(), 4);
        for (i, (thread, sev, body, key)) in sent.iter().enumerate() {
            assert_eq!(thread, &ThreadRef("th_one".into()));
            assert_eq!(*sev, Severity::Warn);
            assert_eq!(key, &OpKey(format!("rem:{}:{i}", r.op)));
            assert!(
                body.contains(r.op.as_str()) && body.contains("seat_retire"),
                "{body}"
            );
            assert!(
                body.contains("unknown_plan"),
                "reason is in the body: {body}"
            );
            assert!(body.contains(&format!("--supersedes {}", r.op)), "{body}");
            assert!(
                body.contains(&format!("herdr-graph cancel {}", r.op)),
                "{body}"
            );
        }
        assert_eq!(
            r.fx.w
                .journal()
                .meta_get(&reminder_count_key(&r.op))
                .unwrap()
                .as_deref(),
            Some("4")
        );
    }

    #[tokio::test]
    async fn no_reminder_before_due() {
        let r = rig();
        assert_eq!(r.sched.tick().await.len(), 1, "initial");
        advance(&r, chrono::Duration::minutes(59));
        assert!(r.sched.tick().await.is_empty(), "59 min is before +1h");
        advance(&r, chrono::Duration::minutes(1));
        assert_eq!(r.sched.tick().await.len(), 1, "exactly +1h is due");
        advance(&r, chrono::Duration::hours(4));
        assert!(r.sched.tick().await.is_empty(), "+5h is before +6h");
        assert_eq!(r.threads.sent().len(), 2);
    }

    #[tokio::test]
    async fn after_downtime_only_the_latest_due_notice_is_sent() {
        let r = rig();
        advance(&r, chrono::Duration::hours(7));
        let s = r.sched.tick().await;
        assert_eq!(
            s.iter().map(|s| s.index).collect::<Vec<_>>(),
            vec![2],
            "one catch-up notice, for the +6h slot"
        );
        assert!(r.sched.tick().await.is_empty());
        advance(&r, chrono::Duration::hours(18));
        assert_eq!(
            r.sched
                .tick()
                .await
                .iter()
                .map(|s| s.index)
                .collect::<Vec<_>>(),
            vec![3]
        );
    }

    #[tokio::test]
    async fn failed_notify_is_retried_on_the_next_tick() {
        let r = rig();
        *r.threads.fail.lock().unwrap() = true;
        assert!(r.sched.tick().await.is_empty());
        assert_eq!(
            r.fx.w
                .journal()
                .meta_get(&reminder_count_key(&r.op))
                .unwrap(),
            None,
            "nothing recorded"
        );
        *r.threads.fail.lock().unwrap() = false;
        assert_eq!(r.sched.tick().await.len(), 1);
    }

    #[tokio::test]
    async fn replacement_stops_reminders() {
        let r = rig();
        assert_eq!(r.sched.tick().await.len(), 1);
        let one = seat(&r.fx, "one");
        let caller = crate::daemon::registry::CallerInfo {
            graph_seat: Some(one.id.to_string()),
            ..Default::default()
        };
        let sp = plan_as(
            &r.fx,
            &caller,
            &format!("seat rename two zwei --supersedes {}", r.op),
        );
        assert_eq!(
            apply_plan(&r.fx, &sp).state,
            crate::model::operation::OpState::Committed
        );
        advance(&r, chrono::Duration::hours(2));
        assert!(r.sched.tick().await.is_empty());
        advance(&r, chrono::Duration::hours(30));
        assert!(r.sched.tick().await.is_empty());
        assert_eq!(r.threads.sent().len(), 1);
    }

    #[tokio::test]
    async fn cancel_stops_reminders() {
        let r = rig();
        assert_eq!(r.sched.tick().await.len(), 1);
        cancel(r.fx.w.journal(), &r.op, r.fx.clock_now()).unwrap();
        advance(&r, chrono::Duration::hours(2));
        assert!(r.sched.tick().await.is_empty());
        advance(&r, chrono::Duration::hours(30));
        assert!(r.sched.tick().await.is_empty());
        assert_eq!(r.threads.sent().len(), 1);
    }

    #[tokio::test]
    async fn requester_without_channel_is_skipped() {
        // no channel on the requester's seat
        let r = rig_with(requester_of, false);
        assert!(r.sched.tick().await.is_empty());
        // a requester that is not a seat at all (a human at a terminal)
        let human = rig_with(
            |_| Requester {
                human: true,
                ..Default::default()
            },
            true,
        );
        advance(&human, chrono::Duration::hours(2));
        assert!(human.sched.tick().await.is_empty());
        assert!(human.threads.sent().is_empty());
        // once the seat gets a channel the pending notice goes out
        set_channel(&r.fx, &seat(&r.fx, "one").id, "th_late");
        let s = r.sched.tick().await;
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].thread, ThreadRef("th_late".into()));
    }

    #[tokio::test]
    async fn retired_requester_seat_is_not_reminded_and_reassign_restarts_the_schedule() {
        let r = rig();
        assert_eq!(r.sched.tick().await.len(), 1);
        advance(&r, chrono::Duration::hours(2));
        // reassign to `two`, which has a channel of its own
        set_channel(&r.fx, &seat(&r.fx, "two").id, "th_two");
        reassign(
            r.fx.w.journal(),
            &*r.fx.store,
            &r.op,
            &seat(&r.fx, "two").id,
            r.fx.clock_now(),
        )
        .unwrap();
        let s = r.sched.tick().await;
        assert_eq!(s.len(), 1, "the new requester gets its initial notice");
        assert_eq!(
            (s[0].index, s[0].thread.clone()),
            (0, ThreadRef("th_two".into()))
        );
        // retire `two`: nobody to remind
        commit(&r.fx, "seat retire two");
        advance(&r, chrono::Duration::hours(2));
        assert!(r.sched.tick().await.is_empty());
    }

    #[tokio::test]
    async fn loop_runs_and_stops_on_shutdown() {
        let r = rig();
        let mut reg = Registry::default();
        let threads = r.threads.clone();
        register_loop(&mut reg, Arc::new(r.sched), Duration::from_millis(10));
        let mut loops = reg.take_loops();
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].0, "plan.reminders");
        let (tx, shutdown) = crate::daemon::registry::shutdown_channel();
        let (_, f) = loops.remove(0);
        let handle = tokio::spawn(f(shutdown));
        for _ in 0..100 {
            if !threads.sent().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(threads.sent().len(), 1, "the loop ticked");
        tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("loop stops")
            .unwrap()
            .unwrap();
    }
}
