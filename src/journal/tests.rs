use super::*;
use crate::model::change::RequestKind;
use crate::model::effect::EffectKind;
use crate::model::{SeatId, TeamspaceId};
use chrono::TimeZone;
use std::sync::Arc;

fn t0() -> Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

fn req(sub: &str) -> ChangeRequest {
    ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args: serde_json::json!({ "sub": sub }),
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    }
}

fn open() -> (tempfile::TempDir, Journal) {
    let tmp = tempfile::tempdir().unwrap();
    let j = Journal::open(&tmp.path().join("journal.sqlite3")).unwrap();
    (tmp, j)
}

fn effect(op: &OpId, object: &AnyId, rev: u64, status: EffectStatus) -> EffectRecord {
    EffectRecord {
        id: EffectRecord::identity(op, object, &EffectKind::CreateTab, rev),
        op: op.clone(),
        object: object.clone(),
        kind: EffectKind::CreateTab,
        object_rev: rev,
        fencing_rev: rev,
        status,
        predicted: vec![],
        nonce_label: None,
        attempts: 0,
        last_error: None,
        updated_at: t0(),
        sched: Default::default(),
    }
}

#[test]
fn admit_is_fifo_by_seq() {
    let (_t, j) = open();
    let a = j.admit(&req("a"), t0()).unwrap();
    let b = j.admit(&req("b"), t0()).unwrap();
    let c = j.admit(&req("c"), t0()).unwrap();
    let ra = j.get(&a).unwrap().unwrap();
    let rb = j.get(&b).unwrap().unwrap();
    assert_eq!((ra.seq, rb.seq), (1, 2));
    assert_eq!(ra.state, OpState::Admitted);
    assert_eq!(ra.request, req("a"));
    assert_eq!(j.next_admitted().unwrap().unwrap().op, a);
    j.begin_applying(&a, t0()).unwrap();
    assert_eq!(j.next_admitted().unwrap().unwrap().op, b);
    j.cancel(&b, t0()).unwrap();
    assert_eq!(j.next_admitted().unwrap().unwrap().op, c);
}

#[test]
fn begin_applying_is_cas() {
    let (_t, j) = open();
    let a = j.admit(&req("a"), t0()).unwrap();
    assert_eq!(j.begin_applying(&a, t0()).unwrap(), Some(1));
    assert_eq!(j.begin_applying(&a, t0()).unwrap(), None);
    assert_eq!(j.get(&a).unwrap().unwrap().attempts, 1);
}

#[test]
fn cancel_only_admitted_or_failed() {
    let (_t, j) = open();
    let a = j.admit(&req("a"), t0()).unwrap();
    j.begin_applying(&a, t0()).unwrap();
    assert_eq!(j.cancel(&a, t0()).unwrap(), CancelOutcome::NotCancellable(OpState::Applying));
    j.finish_committed(&a, &CommitId("c".repeat(40)), None, t0()).unwrap();
    assert_eq!(j.cancel(&a, t0()).unwrap(), CancelOutcome::NotCancellable(OpState::Committed));
    assert_eq!(j.get(&a).unwrap().unwrap().state, OpState::Committed);
    assert_eq!(j.cancel(&OpId::new(), t0()).unwrap(), CancelOutcome::Unknown);

    let b = j.admit(&req("b"), t0()).unwrap();
    assert_eq!(j.cancel(&b, t0()).unwrap(), CancelOutcome::Cancelled);
    assert_eq!(j.cancel(&b, t0()).unwrap(), CancelOutcome::NotCancellable(OpState::Cancelled));
    assert!(j.begin_applying(&b, t0()).unwrap().is_none());
}

#[test]
fn cancel_vs_begin_applying_race() {
    let (_t, j) = open();
    let j = Arc::new(j);
    for _ in 0..200 {
        let op = j.admit(&req("race"), t0()).unwrap();
        let (j1, o1) = (j.clone(), op.clone());
        let (j2, o2) = (j.clone(), op.clone());
        let cancel = std::thread::spawn(move || j1.cancel(&o1, t0()).unwrap());
        let begin = std::thread::spawn(move || j2.begin_applying(&o2, t0()).unwrap());
        let cancelled = cancel.join().unwrap() == CancelOutcome::Cancelled;
        let applying = begin.join().unwrap().is_some();
        assert!(cancelled != applying, "exactly one of cancel/begin_applying wins");
        let state = j.get(&op).unwrap().unwrap().state;
        assert_eq!(state, if cancelled { OpState::Cancelled } else { OpState::Applying });
    }
}

#[test]
fn supersede_transitions() {
    let (_t, j) = open();
    let new = j.admit(&req("new"), t0()).unwrap();
    let admitted = j.admit(&req("a"), t0()).unwrap();
    j.supersede(&admitted, &new, t0()).unwrap();
    let row = j.get(&admitted).unwrap().unwrap();
    assert_eq!((row.state, row.superseded_by), (OpState::Superseded, Some(new.clone())));

    let applying = j.admit(&req("b"), t0()).unwrap();
    j.begin_applying(&applying, t0()).unwrap();
    let err = j.supersede(&applying, &new, t0()).unwrap_err();
    assert!(matches!(err, JournalError::Transition { from: OpState::Applying, .. }), "{err:?}");

    let rejected = j.admit(&req("c"), t0()).unwrap();
    j.begin_applying(&rejected, t0()).unwrap();
    j.finish_rejected(&rejected, &Rejection { reason: "r".into(), explanation: "e".into(), current_revs: vec![] }, t0())
        .unwrap();
    j.supersede(&rejected, &new, t0()).unwrap();
    // superseded is terminal
    assert!(j.supersede(&rejected, &new, t0()).is_err());
}

#[test]
fn failed_reason_roundtrip() {
    let (_t, j) = open();
    let a = j.admit(&req("a"), t0()).unwrap();
    // finish_failed needs applying
    assert!(matches!(
        j.finish_failed(&a, "boom", t0()),
        Err(JournalError::Transition { from: OpState::Admitted, .. })
    ));
    j.begin_applying(&a, t0()).unwrap();
    j.finish_failed(&a, "boom", t0()).unwrap();
    let row = j.get(&a).unwrap().unwrap();
    assert_eq!(row.state, OpState::Failed);
    assert_eq!(row.rejection.unwrap().reason, "boom");
    assert_eq!(j.cancel(&a, t0()).unwrap(), CancelOutcome::Cancelled);
}

#[test]
fn rejection_roundtrip_with_current_revs() {
    let (_t, j) = open();
    let a = j.admit(&req("a"), t0()).unwrap();
    j.begin_applying(&a, t0()).unwrap();
    let seat = SeatId::new().to_any();
    let r = Rejection {
        reason: "precondition_failed".into(),
        explanation: "stale".into(),
        current_revs: vec![crate::model::change::ReliedOn { object: seat, version: crate::model::change::Version::Rev(7) }],
    };
    j.finish_rejected(&a, &r, t0()).unwrap();
    assert_eq!(j.get(&a).unwrap().unwrap().rejection, Some(r));
}

#[test]
fn requeue_infra_does_not_count_attempt() {
    let (_t, j) = open();
    let a = j.admit(&req("a"), t0()).unwrap();
    j.begin_applying(&a, t0()).unwrap();
    j.requeue(&a, false, t0()).unwrap();
    let row = j.get(&a).unwrap().unwrap();
    assert_eq!((row.state, row.attempts), (OpState::Admitted, 0));
    j.begin_applying(&a, t0()).unwrap();
    j.requeue(&a, true, t0()).unwrap();
    assert_eq!(j.get(&a).unwrap().unwrap().attempts, 1);
    // requeue is only legal from applying
    assert!(j.requeue(&a, true, t0()).is_err());
}

#[test]
fn effects_upsert_get_and_status() {
    let (_t, j) = open();
    let op = OpId::new();
    let obj = SeatId::new().to_any();
    let other = TeamspaceId::new().to_any();
    let e = effect(&op, &obj, 3, EffectStatus::Pending);
    j.upsert_effect(&e).unwrap();
    j.upsert_effect(&effect(&op, &other, 1, EffectStatus::Done)).unwrap();
    assert_eq!(j.get_effect(&e.id).unwrap().unwrap(), e);
    assert_eq!(j.effects_with_status(&[EffectStatus::Pending]).unwrap(), vec![e.clone()]);
    assert_eq!(j.effects_with_status(&[]).unwrap(), vec![]);
    assert_eq!(j.effects_for_object(&obj).unwrap(), vec![e.clone()]);

    let later = t0() + chrono::Duration::seconds(5);
    j.set_effect_status(&e.id, EffectStatus::Failed, Some("herdr down"), later).unwrap();
    let got = j.get_effect(&e.id).unwrap().unwrap();
    assert_eq!((got.status, got.last_error.as_deref(), got.updated_at), (EffectStatus::Failed, Some("herdr down"), later));
    assert!(j.effects_with_status(&[EffectStatus::Pending]).unwrap().is_empty());
    // upsert with the same identity replaces, not duplicates
    j.upsert_effect(&effect(&op, &obj, 3, EffectStatus::Done)).unwrap();
    assert_eq!(j.effects_for_object(&obj).unwrap().len(), 1);
    assert!(j.set_effect_status(&EffectId::derive(&op, &obj, "x", 9), EffectStatus::Done, None, later).is_err());
}

#[test]
fn reopen_persists_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("journal.sqlite3");
    let (a, e);
    {
        let j = Journal::open(&path).unwrap();
        a = j.admit(&req("a"), t0()).unwrap();
        j.begin_applying(&a, t0()).unwrap();
        e = effect(&a, &SeatId::new().to_any(), 1, EffectStatus::Pending);
        j.upsert_effect(&e).unwrap();
        j.meta_set("k", "v").unwrap();
    }
    let j = Journal::open(&path).unwrap();
    let row = j.get(&a).unwrap().unwrap();
    assert_eq!((row.state, row.attempts, row.admitted_at), (OpState::Applying, 1, t0()));
    assert_eq!(j.get_effect(&e.id).unwrap().unwrap(), e);
    assert_eq!(j.meta_get("k").unwrap().as_deref(), Some("v"));
    // seq continues after reopen
    let b = j.admit(&req("b"), t0()).unwrap();
    assert_eq!(j.get(&b).unwrap().unwrap().seq, 2);
}

#[test]
fn counts_per_state() {
    let (_t, j) = open();
    let a = j.admit(&req("a"), t0()).unwrap();
    j.admit(&req("b"), t0()).unwrap();
    j.admit(&req("c"), t0()).unwrap();
    j.cancel(&a, t0()).unwrap();
    let c = j.counts().unwrap();
    assert_eq!(c.get("admitted"), Some(&2));
    assert_eq!(c.get("cancelled"), Some(&1));
    assert_eq!(c.get("committed"), None);
    assert_eq!(j.list(&[OpState::Cancelled], 10).unwrap().len(), 1);
    let all = j.list(&[], 10).unwrap();
    assert_eq!(all.len(), 3);
    assert!(all[0].seq > all[2].seq, "newest first");
    assert_eq!(j.list(&[], 2).unwrap().len(), 2);
}

#[test]
fn meta_roundtrip() {
    let (_t, j) = open();
    assert_eq!(j.meta_get("writer_halted").unwrap(), None);
    j.meta_set("writer_halted", "x").unwrap();
    j.meta_set("writer_halted", "y").unwrap();
    assert_eq!(j.meta_get("writer_halted").unwrap().as_deref(), Some("y"));
    j.meta_delete("writer_halted").unwrap();
    assert_eq!(j.meta_get("writer_halted").unwrap(), None);
    assert_eq!(j.checkpoint().unwrap(), None);
    j.set_checkpoint(&CommitId("abc".into())).unwrap();
    assert_eq!(j.checkpoint().unwrap(), Some(CommitId("abc".into())));
}

#[test]
fn set_requester_and_supersedes_link() {
    let (_t, j) = open();
    let old = j.admit(&req("old"), t0()).unwrap();
    let mut r = req("new");
    r.supersedes = Some(old);
    let new = j.admit(&r, t0()).unwrap();
    let who = Requester { human: true, ..Default::default() };
    j.set_requester(&new, &who, t0()).unwrap();
    let row = j.get(&new).unwrap().unwrap();
    assert_eq!(row.request.requester, who);
    assert_eq!(row.request.supersedes, r.supersedes);
}

#[test]
fn journal_pragmas_wal_and_full() {
    let (_t, j) = open();
    let conn = j.conn();
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    let sync: i64 = conn.query_row("PRAGMA synchronous", [], |r| r.get(0)).unwrap();
    assert_eq!((mode.as_str(), sync), ("wal", 2));
}

#[test]
fn finish_committed_superseding_is_atomic() {
    let (_t, j) = open();
    let old = j.admit(&req("old"), t0()).unwrap();
    j.begin_applying(&old, t0()).unwrap();
    j.finish_committed(&old, &CommitId("c0".into()), None, t0()).unwrap();
    let new = j.admit(&req("new"), t0()).unwrap();
    j.begin_applying(&new, t0()).unwrap();
    let c1 = CommitId("c1".into());

    j.execute_batch_for_test(
        "CREATE TRIGGER t BEFORE UPDATE OF state ON ops WHEN NEW.state='superseded' \
         BEGIN SELECT RAISE(ABORT, 'injected'); END;",
    )
    .unwrap();
    assert!(j.finish_committed_superseding(&new, &c1, None, Some(&old), t0()).is_err());
    let r = j.get(&new).unwrap().unwrap();
    assert_eq!((r.state, r.commit), (OpState::Applying, None), "the commit half rolled back");
    assert_eq!(j.get(&old).unwrap().unwrap().state, OpState::Committed);

    j.execute_batch_for_test("DROP TRIGGER t;").unwrap();
    j.finish_committed_superseding(&new, &c1, None, Some(&old), t0()).unwrap();
    let r = j.get(&new).unwrap().unwrap();
    assert_eq!((r.state, r.commit), (OpState::Committed, Some(c1)));
    let o = j.get(&old).unwrap().unwrap();
    assert_eq!((o.state, o.superseded_by), (OpState::Superseded, Some(new)));
}

fn notice_for(e: &EffectRecord, key: &str) -> Notice {
    Notice {
        key: key.into(),
        effect: e.id.clone(),
        op: e.op.clone(),
        severity: Severity::Warn,
        text: "needs a look".into(),
        state: "pending".into(),
        attempts: 0,
        last_error: None,
        next_at: None,
    }
}

#[test]
fn effect_and_notice_written_together() {
    let (_t, j) = open();
    let op = OpId::new();
    let obj = SeatId::new().to_any();
    let mut e = effect(&op, &obj, 1, EffectStatus::NeedsRevision);
    e.last_error = Some("busy".into());
    let n = notice_for(&e, "k1");
    j.upsert_effect_with_notice(&e, &n).unwrap();
    assert_eq!(j.get_effect(&e.id).unwrap().unwrap().status, EffectStatus::NeedsRevision);
    assert_eq!(j.get_notice("k1").unwrap().unwrap(), n);
    // The same key again does not duplicate or reset the notice, even after it was delivered.
    j.notice_done("k1", "delivered", t0()).unwrap();
    j.upsert_effect_with_notice(&e, &n).unwrap();
    assert_eq!(j.notice_counts().unwrap(), BTreeMap::from([("delivered".to_owned(), 1)]));
}

#[test]
fn due_notices_respects_next_at() {
    let (_t, j) = open();
    let op = OpId::new();
    let e = effect(&op, &SeatId::new().to_any(), 1, EffectStatus::Failed);
    let mut n = notice_for(&e, "k1");
    n.next_at = Some(t0() + chrono::Duration::seconds(10));
    j.upsert_effect_with_notice(&e, &n).unwrap();
    assert!(j.due_notices(t0()).unwrap().is_empty());
    assert_eq!(j.next_notice_at().unwrap(), n.next_at);
    assert_eq!(j.due_notices(t0() + chrono::Duration::seconds(10)).unwrap().len(), 1);
}

#[test]
fn notice_retry_counts_attempts() {
    let (_t, j) = open();
    let op = OpId::new();
    let e = effect(&op, &SeatId::new().to_any(), 1, EffectStatus::Failed);
    j.upsert_effect_with_notice(&e, &notice_for(&e, "k1")).unwrap();
    let later = t0() + chrono::Duration::seconds(5);
    j.notice_retry("k1", "down", later, t0()).unwrap();
    j.notice_retry("k1", "still down", later, t0()).unwrap();
    let n = j.get_notice("k1").unwrap().unwrap();
    assert_eq!((n.attempts, n.last_error.as_deref(), n.next_at), (2, Some("still down"), Some(later)));
    assert_eq!(n.state, "pending");
}

#[test]
fn notice_done_removes_from_due() {
    let (_t, j) = open();
    let op = OpId::new();
    let e = effect(&op, &SeatId::new().to_any(), 1, EffectStatus::Failed);
    j.upsert_effect_with_notice(&e, &notice_for(&e, "k1")).unwrap();
    assert_eq!(j.due_notices(t0()).unwrap().len(), 1);
    j.notice_done("k1", "void", t0()).unwrap();
    assert!(j.due_notices(t0()).unwrap().is_empty());
    assert_eq!(j.get_notice("k1").unwrap().unwrap().state, "void");
    assert_eq!(j.next_notice_at().unwrap(), None);
}
