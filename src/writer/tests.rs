use super::*;
use crate::journal::CancelOutcome;
use crate::model::change::{ChangeRequest, Requester};
use crate::model::common::{NameChange, NameSource};
use crate::model::operation::Confirmation;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{AnyId, Lifecycle, SCHEMA_VERSION, SeatId, TeamspaceId};
use crate::ports::clock::ManualClock;
use crate::store::init::init_instance;
use chrono::TimeZone;
use serde_json::json;

fn t0() -> Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

fn rp(s: &str) -> RepoPath {
    RepoPath::new(s).unwrap()
}

struct Seed {
    ts: TeamspaceId,
    a: SeatId,
    b: SeatId,
}

struct TestSeed(Seed);
impl Mutation for TestSeed {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let ts_dir = layout::teamspace_dir("alpha");
        let mut ts = TeamspaceRecord {
            schema: SCHEMA_VERSION,
            id: self.0.ts.clone(),
            rev: 0,
            name: "Alpha".into(),
            name_history: vec![],
            lifecycle: Lifecycle::Dormant,
            retired: None,
            runtime: Default::default(),
            project_repo: None,
            channel: Default::default(),
        };
        cx.tree.put_record(layout::teamspace_record(&ts_dir), &mut ts)?;
        for (slug, id) in [("one", &self.0.a), ("two", &self.0.b)] {
            let mut seat = SeatRecord {
                schema: SCHEMA_VERSION,
                id: id.clone(),
                rev: 0,
                name: slug.into(),
                name_history: vec![],
                teamspace: self.0.ts.clone(),
                lifecycle: Lifecycle::Dormant,
                retired: None,
                role: None,
                template_ref: None,
                applications: vec![],
                overrides: Default::default(),
                participation: Default::default(),
                activation: Default::default(),
                runtime: Default::default(),
                channel: Default::default(),
                reload_required: false,
                moved_out: false,
            };
            cx.tree.put_record(layout::seat_record(&layout::seat_dir(&ts_dir, slug)), &mut seat)?;
        }
        Ok(Applied { summary: "seed".into(), action: None })
    }
}

struct TestRename;
impl Mutation for TestRename {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let seat = cx.request.args["seat"].as_str().unwrap();
        let name = cx.request.args["name"].as_str().unwrap();
        let id = AnyId::parse(seat).map_err(|e| MutationError::Bug(e.to_string()))?;
        let loc = cx.tree.locate(&id)?.ok_or_else(|| MutationError::Bug("no such seat".into()))?;
        let mut rec: SeatRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        rec.name_history.push(NameChange {
            old: rec.name.clone(),
            new: name.into(),
            observed_at: cx.now,
            event_at: None,
            source: NameSource::Request,
        });
        rec.name = name.into();
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: format!("rename {seat}"), action: None })
    }
}

struct TestPair;
impl Mutation for TestPair {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let n = cx.request.args["n"].as_i64().unwrap();
        cx.tree.put_file(rp("pair/a.txt"), n.to_string().into_bytes());
        cx.tree.put_file(rp("pair/b.txt"), n.to_string().into_bytes());
        let action = cx.request.args["action"].as_str().and_then(|s| s.parse().ok());
        Ok(Applied { summary: format!("pair {n}"), action })
    }
}

struct TestMove;
impl Mutation for TestMove {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        cx.tree.move_dir(&rp("pair"), &rp("pair2"))?;
        Ok(Applied { summary: "move pair".into(), action: None })
    }
}

struct TestPanic;
impl Mutation for TestPanic {
    fn apply(&self, _: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        panic!("test panic");
    }
}

struct TestBug;
impl Mutation for TestBug {
    fn apply(&self, _: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        Err(MutationError::Bug("deterministic".into()))
    }
}

/// Fails with an infrastructure error the first `n` times.
struct TestFlaky(std::sync::atomic::AtomicU32);
impl Mutation for TestFlaky {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        if self.0.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| v.checked_sub(1)).is_ok() {
            return Err(MutationError::Store(StoreError::Git("transient".into())));
        }
        cx.tree.put_file(rp("flaky.txt"), b"ok".to_vec());
        Ok(Applied { summary: "flaky".into(), action: None })
    }
}

struct Fx {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
    seed: Seed,
    w: Arc<WriterCore>,
}

fn fx_with(cfg: WriterConfig) -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("inst");
    init_instance(&root).unwrap();
    let seed = Seed { ts: TeamspaceId::new(), a: SeatId::new(), b: SeatId::new() };
    let mut reg = MutationRegistry::default();
    reg.register("bookkeeping.seed", Arc::new(TestSeed(Seed { ts: seed.ts.clone(), a: seed.a.clone(), b: seed.b.clone() })));
    reg.register("seat_rename", Arc::new(TestRename));
    reg.register("bookkeeping.pair", Arc::new(TestPair));
    reg.register("bookkeeping.move", Arc::new(TestMove));
    reg.register("bookkeeping.panic", Arc::new(TestPanic));
    reg.register("bookkeeping.bug", Arc::new(TestBug));
    reg.register("bookkeeping.flaky", Arc::new(TestFlaky(std::sync::atomic::AtomicU32::new(5))));
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let w = WriterCore::new(store, journal, Arc::new(reg), Arc::new(ManualClock::new(t0())), cfg);
    let fx = Fx { _tmp: tmp, root, seed, w };
    let op = fx.w.admit(book("seed", json!({}))).unwrap();
    assert!(matches!(fx.w.step().unwrap(), StepOutcome::Committed(o, _) if o == op));
    fx
}

fn fx() -> Fx {
    fx_with(WriterConfig::default())
}

fn book(sub: &str, mut args: serde_json::Value) -> ChangeRequest {
    args["sub"] = json!(sub);
    ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args,
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
    }
}

fn pair(n: i64) -> ChangeRequest {
    book("pair", json!({ "n": n }))
}

fn rename(seat: &SeatId, name: &str, rev: Option<u64>) -> ChangeRequest {
    ChangeRequest {
        kind: RequestKind::SeatRename,
        args: json!({ "seat": seat.as_str(), "name": name }),
        relied_on: rev
            .map(|r| vec![ReliedOn { object: seat.to_any(), version: Version::Rev(r) }])
            .unwrap_or_default(),
        requester: Requester::default(),
        supersedes: None,
    }
}

fn seat_rev(fx: &Fx, seat: &SeatId) -> (u64, String) {
    let head = fx.w.store().head().unwrap();
    let loc = fx.w.store().locate(&head, &seat.to_any()).unwrap().unwrap();
    let rec: SeatRecord = crate::ports::store::read_record(&**fx.w.store(), &head, &loc.record_path).unwrap().unwrap();
    (rec.rev, rec.name)
}

fn log_messages(fx: &Fx) -> Vec<String> {
    fx.w.store()
        .with_repo(|r| {
            let mut walk = r.revwalk()?;
            walk.push_head()?;
            walk.map(|o| Ok(r.find_commit(o?)?.message().unwrap_or_default().to_owned())).collect()
        })
        .unwrap()
}

fn trailer_count(fx: &Fx, op: &OpId) -> usize {
    let needle = format!("Graph-Op: {op}");
    log_messages(fx).iter().filter(|m| m.lines().any(|l| l == needle)).count()
}

fn row(fx: &Fx, op: &OpId) -> crate::journal::OpRow {
    fx.w.journal().get(op).unwrap().unwrap()
}

#[test]
fn disjoint_seat_edits_both_commit() {
    let fx = fx();
    let a = fx.w.admit(rename(&fx.seed.a, "alpha-1", Some(1))).unwrap();
    let b = fx.w.admit(rename(&fx.seed.b, "beta-1", Some(1))).unwrap();
    let out = fx.w.drain().unwrap();
    assert!(matches!(out[..], [StepOutcome::Committed(..), StepOutcome::Committed(..)]), "{out:?}");
    assert_eq!(row(&fx, &a).state, OpState::Committed);
    assert_eq!(row(&fx, &b).state, OpState::Committed);
    assert_eq!(seat_rev(&fx, &fx.seed.a), (2, "alpha-1".into()));
    assert_eq!(seat_rev(&fx, &fx.seed.b), (2, "beta-1".into()));
}

#[test]
fn same_rev_renames_one_rejected_with_explanation() {
    let fx = fx();
    let first = fx.w.admit(rename(&fx.seed.a, "first", Some(1))).unwrap();
    let second = fx.w.admit(rename(&fx.seed.a, "second", Some(1))).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(row(&fx, &first).state, OpState::Committed);
    let r = row(&fx, &second);
    assert_eq!(r.state, OpState::Rejected);
    let rej = r.rejection.unwrap();
    assert_eq!(rej.reason, "precondition_failed");
    assert!(rej.explanation.contains(fx.seed.a.as_str()), "{}", rej.explanation);
    assert!(rej.explanation.contains("rev 1") && rej.explanation.contains("rev 2"), "{}", rej.explanation);
    assert_eq!(
        rej.current_revs,
        vec![ReliedOn { object: fx.seed.a.to_any(), version: Version::Rev(2) }]
    );
    assert_eq!(seat_rev(&fx, &fx.seed.a), (2, "first".into()));
    assert_eq!(trailer_count(&fx, &second), 0, "rejected ops are not committed to git");
}

#[test]
fn commit_message_has_trailers() {
    let fx = fx();
    let act = ActionId::new();
    let mut r = pair(1);
    r.args["action"] = json!(act.as_str());
    let op = fx.w.admit(r).unwrap();
    let outcome = fx.w.step().unwrap();
    let StepOutcome::Committed(_, commit) = outcome else { panic!("{outcome:?}") };
    let msg = fx
        .w
        .store()
        .with_repo(|r| Ok(r.find_commit(Oid::from_str(&commit.0)?)?.message().unwrap().to_owned()))
        .unwrap();
    assert!(msg.starts_with("bookkeeping: pair 1\n"), "{msg}");
    assert!(msg.contains(&format!("\nGraph-Op: {op}\n")), "{msg}");
    assert!(msg.contains(&format!("\nGraph-Action: {act}\n")), "{msg}");
    let r = row(&fx, &op);
    assert_eq!((r.commit, r.action), (Some(commit), Some(act)));
    // commit is what main points at, and a plain op has no action trailer
    let op2 = fx.w.admit(pair(2)).unwrap();
    fx.w.step().unwrap();
    assert!(!log_messages(&fx)[0].contains("Graph-Action"));
    assert_eq!(trailer_count(&fx, &op2), 1);
}

#[test]
fn operation_summary_written_with_confirmation_and_plan() {
    let fx = fx();
    let plan = PlanId::new();
    let conf = Confirmation { mode: crate::model::operation::ConfirmMode::Tty, plan_hash: "h".into(), at: t0() };
    let mut r = rename(&fx.seed.a, "renamed", Some(1));
    r.args["_plan"] = json!(plan.as_str());
    r.args["_confirmation"] = serde_json::to_value(&conf).unwrap();
    let op = fx.w.admit(r).unwrap();
    fx.w.step().unwrap();
    let head = fx.w.store().head().unwrap();
    let rec: OperationRecord =
        crate::ports::store::read_record(&**fx.w.store(), &head, &layout::operation_record(t0(), &op)).unwrap().unwrap();
    assert_eq!(rec.summary, format!("rename {}", fx.seed.a));
    assert_eq!(rec.plan, Some(plan));
    assert_eq!(rec.confirmation, Some(conf));
    assert_eq!((rec.state, rec.commit, rec.rev), (OpState::Committed, None, 1));
    assert_eq!(rec.kind, RequestKind::SeatRename);
}

#[test]
fn unregistered_kind_rejected_at_admit() {
    let fx = fx();
    let mut r = book("nope", json!({}));
    assert!(matches!(fx.w.admit(r.clone()), Err(WriterError::Invalid(m)) if m.contains("bookkeeping.nope")));
    r.kind = RequestKind::SeatCreate;
    assert!(matches!(fx.w.admit(r), Err(WriterError::Invalid(m)) if m.contains("seat_create")));
    assert_eq!(fx.w.journal().list(&[OpState::Admitted], 10).unwrap().len(), 0);
}

#[test]
fn relied_on_missing_object_rejected_at_admit() {
    let fx = fx();
    let ghost = SeatId::new();
    let err = fx.w.admit(rename(&ghost, "x", Some(1))).unwrap_err();
    assert!(matches!(err, WriterError::Invalid(m) if m.contains(ghost.as_str())));
}

#[test]
fn poison_panic_marks_failed_and_fifo_continues() {
    let fx = fx();
    let bad = fx.w.admit(book("panic", json!({}))).unwrap();
    let bug = fx.w.admit(book("bug", json!({}))).unwrap();
    let good = fx.w.admit(pair(1)).unwrap();
    let out = fx.w.drain().unwrap();
    assert!(matches!(out[..], [StepOutcome::Failed(..), StepOutcome::Failed(..), StepOutcome::Committed(..)]), "{out:?}");
    let r = row(&fx, &bad);
    assert_eq!(r.state, OpState::Failed);
    assert!(r.rejection.unwrap().reason.contains("test panic"));
    assert!(row(&fx, &bug).rejection.unwrap().reason.contains("deterministic"));
    assert_eq!(row(&fx, &good).state, OpState::Committed);
    assert_eq!(row(&fx, &bad).attempts, 1, "deterministic failure is terminal on the first attempt");
}

#[test]
fn poison_after_three_attempts() {
    let fx = fx();
    let op = fx.w.admit(pair(1)).unwrap();
    for i in 1..=3 {
        assert_eq!(fx.w.journal().begin_applying(&op, t0()).unwrap(), Some(i));
        fx.w.journal().requeue(&op, true, t0()).unwrap();
    }
    assert_eq!(fx.w.step().unwrap(), StepOutcome::Failed(op.clone()));
    let r = row(&fx, &op);
    assert_eq!(r.state, OpState::Failed);
    assert!(r.rejection.unwrap().reason.contains("poison"));
    assert_eq!(trailer_count(&fx, &op), 0);
    assert_eq!(fx.w.step().unwrap(), StepOutcome::Idle);
}

#[test]
fn cancel_admitted_never_commits() {
    let fx = fx();
    let op = fx.w.admit(pair(1)).unwrap();
    assert_eq!(fx.w.journal().cancel(&op, t0()).unwrap(), CancelOutcome::Cancelled);
    assert_eq!(fx.w.drain().unwrap(), vec![]);
    assert_eq!(row(&fx, &op).state, OpState::Cancelled);
    assert_eq!(trailer_count(&fx, &op), 0);
    assert_eq!(fx.w.status(&op).unwrap(), Some(OpState::Cancelled));
}

#[test]
fn cancel_failed_allowed() {
    let fx = fx();
    let op = fx.w.admit(book("bug", json!({}))).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(row(&fx, &op).state, OpState::Failed);
    assert_eq!(fx.w.journal().cancel(&op, t0()).unwrap(), CancelOutcome::Cancelled);
}

#[test]
fn supersedes_marks_original_superseded() {
    let fx = fx();
    let first = fx.w.admit(rename(&fx.seed.a, "one", Some(1))).unwrap();
    fx.w.drain().unwrap();
    let mut again = rename(&fx.seed.a, "two", Some(2));
    again.supersedes = Some(first.clone());
    let second = fx.w.admit(again).unwrap();
    fx.w.drain().unwrap();
    let r = row(&fx, &first);
    assert_eq!((r.state, r.superseded_by), (OpState::Superseded, Some(second.clone())));
    assert_eq!(row(&fx, &second).state, OpState::Committed);
    let head = fx.w.store().head().unwrap();
    let rec: OperationRecord =
        crate::ports::store::read_record(&**fx.w.store(), &head, &layout::operation_record(t0(), &second)).unwrap().unwrap();
    assert_eq!(rec.supersedes, Some(first));
}

#[test]
fn ff_updates_files_and_view_rev() {
    let fx = fx();
    let op = fx.w.admit(pair(7)).unwrap();
    let StepOutcome::Committed(_, c) = fx.w.step().unwrap() else { panic!() };
    assert_eq!(std::fs::read_to_string(fx.root.join("pair/a.txt")).unwrap(), "7");
    assert_eq!(std::fs::read_to_string(fx.root.join("pair/b.txt")).unwrap(), "7");
    assert_eq!(worktree::view_rev(&fx.root), Some(c));
    let op2 = fx.w.admit(pair(8)).unwrap();
    fx.w.step().unwrap();
    assert_eq!(std::fs::read_to_string(fx.root.join("pair/a.txt")).unwrap(), "8");
    assert!(worktree::read_dirty(&fx.root).is_empty());
    assert!(fx.root.join(format!("operations/2026-10/{op}.toml")).exists());
    assert!(fx.root.join(format!("operations/2026-10/{op2}.toml")).exists());
}

#[test]
fn ff_leaves_dirty_file_and_records_it() {
    let fx = fx();
    fx.w.admit(pair(1)).unwrap();
    fx.w.step().unwrap();
    std::fs::write(fx.root.join("pair/a.txt"), "mine").unwrap();
    let op = fx.w.admit(pair(2)).unwrap();
    fx.w.step().unwrap();
    assert_eq!(std::fs::read_to_string(fx.root.join("pair/a.txt")).unwrap(), "mine");
    assert_eq!(std::fs::read_to_string(fx.root.join("pair/b.txt")).unwrap(), "2");
    let dirty = worktree::read_dirty(&fx.root);
    assert_eq!(dirty.len(), 1);
    assert_eq!((dirty[0].path.as_str(), dirty[0].op.as_ref(), dirty[0].at), ("pair/a.txt", Some(&op), t0()));
    // git still has the new content
    let head = fx.w.store().head().unwrap();
    let bytes = fx.w.store().read_file(&head, &rp("pair/a.txt")).unwrap().unwrap();
    assert_eq!(bytes, b"2");
}

#[test]
fn ff_folder_move_orphans_untracked() {
    let fx = fx();
    fx.w.admit(pair(1)).unwrap();
    fx.w.step().unwrap();
    std::fs::write(fx.root.join("pair/notes.txt"), "untracked").unwrap();
    let op = fx.w.admit(book("move", json!({}))).unwrap();
    fx.w.step().unwrap();
    assert_eq!(std::fs::read_to_string(fx.root.join("pair2/a.txt")).unwrap(), "1");
    assert!(!fx.root.join("pair").exists(), "old folder is gone from the view");
    let orphan = fx.root.join(format!(".graph-local/orphans/{op}/pair/notes.txt"));
    assert_eq!(std::fs::read_to_string(orphan).unwrap(), "untracked");
}

#[test]
fn ff_is_idempotent() {
    let fx = fx();
    fx.w.admit(pair(3)).unwrap();
    let StepOutcome::Committed(_, c) = fx.w.step().unwrap() else { panic!() };
    let head = fx.w.store().tree_id(&c).unwrap();
    let before = fx.w.store().with_repo(|r| r.find_commit(Oid::from_str(&c.0)?)?.parent_id(0)).unwrap();
    let prev_tree = fx.w.store().with_repo(|r| Ok(r.find_commit(before)?.tree_id())).unwrap();
    let report = fx
        .w
        .store()
        .with_repo(|r| Ok(worktree::fast_forward(r, &fx.root, Some(prev_tree), head, &c, None, t0())))
        .unwrap()
        .unwrap();
    assert_eq!(report, worktree::FfReport::default());
    assert_eq!(std::fs::read_to_string(fx.root.join("pair/a.txt")).unwrap(), "3");
}

#[test]
fn recover_removes_stale_locks() {
    let fx = fx();
    let git = fx.root.join(".git");
    std::fs::write(git.join("refs/heads/main.lock"), "").unwrap();
    std::fs::write(git.join("index.lock"), "").unwrap();
    let report = fx.w.recover().unwrap();
    assert_eq!(report.removed_locks.len(), 2);
    assert!(!git.join("refs/heads/main.lock").exists() && !git.join("index.lock").exists());
    assert_eq!(report.checkpoint, Some(fx.w.store().head().unwrap()));
    // and the writer can commit again
    fx.w.admit(pair(1)).unwrap();
    assert!(matches!(fx.w.step().unwrap(), StepOutcome::Committed(..)));
}

#[test]
fn lock_contention_retried() {
    let fx = fx();
    let lock = fx.root.join(".git/refs/heads/main.lock");
    std::fs::write(&lock, "").unwrap();
    let l = lock.clone();
    let remover = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        std::fs::remove_file(l).unwrap();
    });
    let op = fx.w.admit(pair(1)).unwrap();
    let out = fx.w.step().unwrap();
    remover.join().unwrap();
    assert!(matches!(out, StepOutcome::Committed(..)), "{out:?}");
    assert_eq!(row(&fx, &op).state, OpState::Committed);
}

#[test]
fn lock_contention_exhausted_halts_writer() {
    let fx = fx_with(WriterConfig { lock_retries: 2, lock_backoff: Duration::from_millis(10), ..Default::default() });
    std::fs::write(fx.root.join(".git/refs/heads/main.lock"), "").unwrap();
    let op = fx.w.admit(pair(1)).unwrap();
    let err = fx.w.step().unwrap_err();
    assert!(matches!(&err, WriterError::Halted(m) if m.contains("lock contention")), "{err:?}");
    assert_eq!(row(&fx, &op).state, OpState::Applying);
    assert!(fx.w.journal().meta_get(WRITER_HALTED).unwrap().is_some());
    assert!(matches!(fx.w.step(), Err(WriterError::Halted(_))));
    // Daemon restart: recovery clears the lock and requeues; halt flag cleared by the operator.
    fx.w.recover().unwrap();
    fx.w.journal().meta_delete(WRITER_HALTED).unwrap();
    assert!(matches!(fx.w.step().unwrap(), StepOutcome::Committed(..)));
    assert_eq!(trailer_count(&fx, &op), 1);
}

fn halted_fx() -> (Fx, OpId, std::path::PathBuf) {
    let fx = fx_with(WriterConfig { lock_retries: 2, lock_backoff: Duration::from_millis(10), ..Default::default() });
    let lock = fx.root.join(".git/refs/heads/main.lock");
    std::fs::write(&lock, "").unwrap();
    let op = fx.w.admit(pair(1)).unwrap();
    assert!(matches!(fx.w.step(), Err(WriterError::Halted(_))));
    assert!(fx.w.journal().meta_get(WRITER_HALTED).unwrap().is_some());
    (fx, op, lock)
}

#[test]
fn resume_clears_halt_after_successful_probe() {
    let (fx, op, lock) = halted_fx();
    let rep = fx.w.resume().unwrap();
    assert!(rep.was_halted);
    assert!(rep.reason.unwrap().contains("lock contention"));
    assert_eq!(rep.recovery.unwrap().requeued, vec![op.clone()]);
    assert!(!lock.exists(), "recovery removed the stale lock");
    assert_eq!(fx.w.journal().meta_get(WRITER_HALTED).unwrap(), None);
    assert!(matches!(fx.w.drain().unwrap()[..], [StepOutcome::Committed(..)]));
    assert_eq!(trailer_count(&fx, &op), 1);
}

#[test]
fn resume_keeps_halt_when_probe_fails() {
    let (fx, _op, _lock) = halted_fx();
    let reason = fx.w.journal().meta_get(WRITER_HALTED).unwrap().unwrap();
    // Recovery cannot read the ref: the probe fails.
    let main = fx.root.join(".git/refs/heads/main");
    let parked = fx.root.join(".git/refs/heads/main.parked");
    std::fs::rename(&main, &parked).unwrap();
    assert!(fx.w.resume().is_err());
    assert_eq!(fx.w.journal().meta_get(WRITER_HALTED).unwrap(), Some(reason));
    assert!(matches!(fx.w.step(), Err(WriterError::Halted(_))));
    std::fs::rename(&parked, &main).unwrap();
    assert!(fx.w.resume().unwrap().was_halted);
    assert_eq!(fx.w.journal().meta_get(WRITER_HALTED).unwrap(), None);
}

#[test]
fn resume_when_not_halted_is_noop() {
    let fx = fx();
    let rep = fx.w.resume().unwrap();
    assert!(!rep.was_halted);
    assert!(rep.reason.is_none() && rep.recovery.is_none());
    assert_eq!(fx.w.journal().meta_get("writer_probe").unwrap(), None, "no probe ran");
}

#[test]
fn recover_clears_stale_halt() {
    let fx = fx();
    fx.w.journal().meta_set(WRITER_HALTED, "disk on fire").unwrap();
    let rep = fx.w.recover().unwrap();
    assert_eq!(rep.cleared_halt.as_deref(), Some("disk on fire"));
    assert_eq!(fx.w.journal().meta_get(WRITER_HALTED).unwrap(), None);
    assert_eq!(fx.w.recover().unwrap().cleared_halt, None);
}

#[test]
fn recover_marks_committed_from_trailer() {
    let fx = fx();
    let op = fx.w.admit(pair(9)).unwrap();
    // Simulate a crash after the ref moved but before the journal update.
    fx.w.journal().begin_applying(&op, t0()).unwrap();
    let store = fx.w.store();
    let head = store.head().unwrap();
    let mut edits = crate::store::EditSet::default();
    edits.put(rp("pair/a.txt"), b"9".to_vec());
    edits.put(rp("pair/b.txt"), b"9".to_vec());
    let tree = store.build_tree(&head, &edits).unwrap();
    let head_oid = Oid::from_str(&head.0).unwrap();
    let cfg = WriterConfig::default();
    let msg = format!("bookkeeping: pair 9\n\nGraph-Op: {op}\n");
    let new = store
        .with_repo(|r| {
            let c = commit::create_commit(r, head_oid, tree, &msg)?;
            Ok(commit::cas_main(r, head_oid, c, &msg, &cfg).map(|()| c))
        })
        .unwrap()
        .unwrap();

    let report = fx.w.recover().unwrap();
    assert_eq!(report.marked_committed, vec![op.clone()]);
    assert!(report.requeued.is_empty());
    let r = row(&fx, &op);
    assert_eq!((r.state, r.commit), (OpState::Committed, Some(CommitId(new.to_string()))));
    assert_eq!(fx.w.drain().unwrap(), vec![], "committed op is not re-applied");
    assert_eq!(trailer_count(&fx, &op), 1);
    // recovery also brought the working tree view up to date
    assert_eq!(std::fs::read_to_string(fx.root.join("pair/a.txt")).unwrap(), "9");
    assert_eq!(worktree::view_rev(&fx.root), Some(CommitId(new.to_string())));
}

#[test]
fn recover_requeues_applying_op_without_trailer_and_counts_attempt() {
    let fx = fx();
    let op = fx.w.admit(pair(1)).unwrap();
    fx.w.journal().begin_applying(&op, t0()).unwrap();
    let report = fx.w.recover().unwrap();
    assert_eq!(report.requeued, vec![op.clone()]);
    let r = row(&fx, &op);
    assert_eq!((r.state, r.attempts), (OpState::Admitted, 1));
    assert!(matches!(fx.w.step().unwrap(), StepOutcome::Committed(..)));
    assert_eq!(row(&fx, &op).attempts, 2);
}

#[test]
fn recover_without_view_rev_keeps_user_files_and_records_dirty() {
    let fx = fx();
    std::fs::write(fx.root.join("graph.toml"), "# user edit\n").unwrap();
    std::fs::remove_file(fx.root.join(".gitignore")).unwrap();
    std::fs::remove_file(fx.root.join(".graph-local/view_rev")).unwrap();
    let report = fx.w.recover().unwrap();
    assert_eq!(report.ff.dirty, vec!["graph.toml".to_string()]);
    assert_eq!(report.ff.updated, vec![".gitignore".to_string()]);
    assert_eq!(std::fs::read_to_string(fx.root.join("graph.toml")).unwrap(), "# user edit\n");
    assert!(fx.root.join(".gitignore").exists());
    assert_eq!(worktree::view_rev(&fx.root), Some(fx.w.store().head().unwrap()));
    assert_eq!(worktree::read_dirty(&fx.root)[0].path, "graph.toml");
}

#[test]
fn infra_errors_do_not_count_as_attempts() {
    let fx = fx();
    let op = fx.w.admit(book("flaky", json!({}))).unwrap();
    for _ in 0..5 {
        let err = fx.w.step().unwrap_err();
        assert!(matches!(err, WriterError::Journal(m) if m.contains("transient")));
        let r = row(&fx, &op);
        assert_eq!((r.state, r.attempts), (OpState::Admitted, 0));
    }
    assert_eq!(fx.w.infra_failures.load(Ordering::SeqCst), 5);
    assert!(matches!(fx.w.step().unwrap(), StepOutcome::Committed(..)));
    assert_eq!(fx.w.infra_failures.load(Ordering::SeqCst), 0);
    assert_eq!(row(&fx, &op).attempts, 1);
}

#[test]
fn blob_precondition_checks_hash_of_path() {
    let fx = fx();
    fx.w.admit(pair(1)).unwrap();
    fx.w.step().unwrap();
    // relied-on blob lives under the owning object's folder: put a file there directly.
    let head = fx.w.store().head().unwrap();
    let loc = fx.w.store().locate(&head, &fx.seed.a.to_any()).unwrap().unwrap();
    let file = loc.folder.join("notes.md").unwrap();
    let mut edits = crate::store::EditSet::default();
    edits.put(file.clone(), b"v1".to_vec());
    let tree = fx.w.store().build_tree(&head, &edits).unwrap();
    let head_oid = Oid::from_str(&head.0).unwrap();
    fx.w.store()
        .with_repo(|r| {
            let c = commit::create_commit(r, head_oid, tree, "content: notes")?;
            let _ = commit::cas_main(r, head_oid, c, "content: notes", &WriterConfig::default());
            Ok(())
        })
        .unwrap();
    let head = fx.w.store().head().unwrap();
    let good = fx.w.store().blob_hash(&head, &file).unwrap().unwrap();
    let mk = |h: crate::model::BlobHash| {
        let mut r = pair(5);
        r.args["path"] = json!("notes.md");
        r.relied_on = vec![ReliedOn { object: fx.seed.a.to_any(), version: Version::Blob(h) }];
        r
    };
    let stale = fx.w.admit(mk(crate::model::BlobHash("0".repeat(40)))).unwrap();
    let fresh = fx.w.admit(mk(good)).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(row(&fx, &stale).state, OpState::Rejected);
    assert!(row(&fx, &stale).rejection.unwrap().explanation.contains("blob"));
    assert_eq!(row(&fx, &fresh).state, OpState::Committed);
}

#[test]
fn events_are_emitted_and_wait_terminal_returns_state() {
    let fx = fx();
    let mut rx = fx.w.subscribe();
    let op = fx.w.admit(pair(1)).unwrap();
    let bad = fx.w.admit(book("bug", json!({}))).unwrap();
    fx.w.drain().unwrap();
    let e1 = rx.try_recv().unwrap();
    assert_eq!((e1.op, e1.state), (op.clone(), OpState::Committed));
    assert!(e1.commit.is_some());
    assert_eq!(rx.try_recv().unwrap().state, OpState::Failed);
    assert_eq!(fx.w.wait_terminal(&op, Duration::from_millis(100)).unwrap(), Some(OpState::Committed));
    assert_eq!(fx.w.wait_terminal(&bad, Duration::from_millis(100)).unwrap(), Some(OpState::Failed));
    let pending = fx.w.admit(pair(2)).unwrap();
    assert_eq!(fx.w.wait_terminal(&pending, Duration::from_millis(30)).unwrap(), None);
}

#[test]
fn registry_rejects_duplicates_and_keys_use_sub() {
    let mut reg = MutationRegistry::default();
    reg.register("a", Arc::new(TestPair));
    assert_eq!(reg.keys(), vec!["a".to_string()]);
    let dup = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let mut r2 = reg.clone();
        r2.register("a", Arc::new(TestPair));
    }));
    assert!(dup.is_err());
    assert_eq!(mutation_key(&book("binding", json!({}))), "bookkeeping.binding");
    assert_eq!(mutation_key(&rename(&SeatId::new(), "x", None)), "seat_rename");
    let mut r = rename(&SeatId::new(), "x", None);
    r.args["sub"] = json!("ignored");
    assert_eq!(mutation_key(&r), "seat_rename", "sub only applies to observed/bookkeeping/content_write");
}

#[tokio::test]
async fn run_loop_processes_admitted_ops_and_stops_on_shutdown() {
    let fx = fx();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn(fx.w.clone().run(rx));
    let op = fx.w.admit(pair(1)).unwrap();
    let w = fx.w.clone();
    let state = tokio::task::spawn_blocking(move || w.wait_terminal(&op, Duration::from_secs(5)).unwrap()).await.unwrap();
    assert_eq!(state, Some(OpState::Committed));
    tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(3), handle).await.unwrap().unwrap();
}
