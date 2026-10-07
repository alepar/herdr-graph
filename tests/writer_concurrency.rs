//! Tier-2 writer tests: atomic visibility, cancel-vs-commit linearizability, precondition races.
use herdr_graph::journal::{CancelOutcome, Journal};
use herdr_graph::model::change::{ChangeRequest, ReliedOn, RequestKind, Requester, Version};
use herdr_graph::model::common::{NameChange, NameSource};
use herdr_graph::model::operation::OpState;
use herdr_graph::model::seat::SeatRecord;
use herdr_graph::model::teamspace::TeamspaceRecord;
use herdr_graph::model::{AnyId, Lifecycle, OpId, SCHEMA_VERSION, SeatId, TeamspaceId};
use herdr_graph::ports::clock::SystemClock;
use herdr_graph::ports::store::{RepoPath, Store};
use herdr_graph::ports::writer::Writer;
use herdr_graph::store::init::init_instance;
use herdr_graph::store::{GitStore, layout};
use herdr_graph::writer::{
    Applied, Mutation, MutationCx, MutationError, MutationRegistry, StepOutcome, WriterConfig,
    WriterCore,
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn rp(s: &str) -> RepoPath {
    RepoPath::new(s).unwrap()
}

struct Pair;
impl Mutation for Pair {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let n = cx.request.args["n"].as_i64().unwrap().to_string();
        cx.tree.put_file(rp("pair/a.txt"), n.clone().into_bytes());
        cx.tree.put_file(rp("pair/b.txt"), n.into_bytes());
        Ok(Applied {
            summary: "pair".into(),
            action: None,
        })
    }
}

struct Seed(TeamspaceId, SeatId);
impl Mutation for Seed {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let dir = layout::teamspace_dir("alpha");
        let mut ts = TeamspaceRecord {
            schema: SCHEMA_VERSION,
            id: self.0.clone(),
            rev: 0,
            name: "Alpha".into(),
            name_history: vec![],
            lifecycle: Lifecycle::Dormant,
            retired: None,
            runtime: Default::default(),
            project_repo: None,
            channel: Default::default(),
        };
        cx.tree
            .put_record(layout::teamspace_record(&dir), &mut ts)?;
        let mut seat = SeatRecord {
            schema: SCHEMA_VERSION,
            id: self.1.clone(),
            rev: 0,
            name: "one".into(),
            name_history: vec![],
            teamspace: self.0.clone(),
            lifecycle: Lifecycle::Dormant,
            retired: None,
            system_duty: None,
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
        cx.tree.put_record(
            layout::seat_record(&layout::seat_dir(&dir, "one")),
            &mut seat,
        )?;
        Ok(Applied {
            summary: "seed".into(),
            action: None,
        })
    }
}

struct Rename;
impl Mutation for Rename {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let id = AnyId::parse(cx.request.args["seat"].as_str().unwrap()).unwrap();
        let name = cx.request.args["name"].as_str().unwrap().to_owned();
        let loc = cx
            .tree
            .locate(&id)?
            .ok_or_else(|| MutationError::Bug("no seat".into()))?;
        let mut rec: SeatRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        rec.name_history.push(NameChange {
            old: rec.name.clone(),
            new: name.clone(),
            observed_at: cx.now,
            event_at: None,
            source: NameSource::Request,
        });
        rec.name = name;
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied {
            summary: "rename".into(),
            action: None,
        })
    }
}

struct Fx {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    seat: SeatId,
    w: Arc<WriterCore>,
}

fn fixture() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("inst");
    init_instance(&root).unwrap();
    let seat = SeatId::new();
    let mut reg = MutationRegistry::default();
    reg.register("bookkeeping.pair", Arc::new(Pair));
    reg.register(
        "bookkeeping.seed",
        Arc::new(Seed(TeamspaceId::new(), seat.clone())),
    );
    reg.register("seat_rename", Arc::new(Rename));
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let w = WriterCore::new(
        store,
        journal,
        Arc::new(reg),
        Arc::new(SystemClock),
        WriterConfig::default(),
    );
    let fx = Fx {
        _tmp: tmp,
        root,
        seat,
        w,
    };
    fx.w.admit(book("seed", json!({}))).unwrap();
    fx.w.drain().unwrap();
    fx
}

fn book(sub: &str, mut args: serde_json::Value) -> ChangeRequest {
    args["sub"] = json!(sub);
    ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args,
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    }
}

fn rename(seat: &SeatId, name: &str) -> ChangeRequest {
    ChangeRequest {
        kind: RequestKind::SeatRename,
        args: json!({ "seat": seat.as_str(), "name": name }),
        relied_on: vec![ReliedOn {
            object: seat.to_any(),
            version: Version::Rev(1),
        }],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    }
}

fn trailer_count(store: &GitStore, op: &OpId) -> usize {
    let needle = format!("Graph-Op: {op}");
    store
        .with_repo(|r| {
            let mut walk = r.revwalk()?;
            walk.push_head()?;
            let mut n = 0;
            for oid in walk {
                let msg = r
                    .find_commit(oid?)?
                    .message()
                    .unwrap_or_default()
                    .to_owned();
                n += msg.lines().filter(|l| *l == needle).count();
            }
            Ok(n)
        })
        .unwrap()
}

#[test]
fn reader_never_sees_partial_multi_file_change() {
    let fx = fixture();
    let done = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..2)
        .map(|_| {
            // Separate repository handle, as another process would have.
            let store = GitStore::open(&fx.root).unwrap();
            let done = done.clone();
            std::thread::spawn(move || {
                let mut reads = 0u32;
                let mut seen_values = std::collections::BTreeSet::new();
                while !done.load(Ordering::SeqCst) {
                    let head = store.head().unwrap();
                    let a = store.read_file(&head, &rp("pair/a.txt")).unwrap();
                    let b = store.read_file(&head, &rp("pair/b.txt")).unwrap();
                    assert_eq!(a, b, "partial multi-file change visible at {head:?}");
                    seen_values.insert(a);
                    reads += 1;
                }
                (reads, seen_values.len())
            })
        })
        .collect();
    for n in 1..=200 {
        fx.w.admit(book("pair", json!({ "n": n }))).unwrap();
        assert!(matches!(fx.w.step().unwrap(), StepOutcome::Committed(..)));
    }
    done.store(true, Ordering::SeqCst);
    for r in readers {
        let (reads, _distinct) = r.join().unwrap();
        assert!(reads > 0);
    }
    let head = fx.w.store().head().unwrap();
    assert_eq!(
        fx.w.store()
            .read_file(&head, &rp("pair/a.txt"))
            .unwrap()
            .unwrap(),
        b"200"
    );
}

#[test]
fn cancel_vs_commit_is_linearizable() {
    let fx = fixture();
    let (mut cancelled, mut committed) = (0, 0);
    for n in 0..100 {
        let op = fx.w.admit(book("pair", json!({ "n": n }))).unwrap();
        let j = fx.w.journal().clone();
        let o = op.clone();
        let canceller = std::thread::spawn(move || j.cancel(&o, chrono::Utc::now()).unwrap());
        let w = fx.w.clone();
        let stepper = std::thread::spawn(move || w.step().unwrap());
        let cancel_result = canceller.join().unwrap();
        let step_result = stepper.join().unwrap();
        let state = fx.w.status(&op).unwrap().unwrap();
        let in_log = trailer_count(fx.w.store(), &op);
        match state {
            OpState::Cancelled => {
                cancelled += 1;
                assert_eq!(cancel_result, CancelOutcome::Cancelled);
                assert_eq!(in_log, 0, "cancelled op must not commit");
                assert!(
                    matches!(step_result, StepOutcome::Idle | StepOutcome::Skipped(_)),
                    "stepper must not have applied a cancelled op: {step_result:?}"
                );
            }
            OpState::Committed => {
                committed += 1;
                assert!(
                    matches!(
                        cancel_result,
                        CancelOutcome::NotCancellable(OpState::Applying | OpState::Committed)
                    ),
                    "{cancel_result:?}"
                );
                assert!(
                    matches!(step_result, StepOutcome::Committed(..)),
                    "{step_result:?}"
                );
                assert_eq!(in_log, 1, "committed op appears exactly once");
            }
            other => panic!("unexpected state {other:?}"),
        }
        // The queue is empty again (a cancelled op that the stepper skipped leaves nothing behind).
        assert_eq!(fx.w.drain().unwrap(), vec![]);
    }
    assert_eq!(cancelled + committed, 100);
}

#[test]
fn two_renames_same_rev() {
    let fx = fixture();
    let (w1, w2) = (fx.w.clone(), fx.w.clone());
    let (r1, r2) = (rename(&fx.seat, "first"), rename(&fx.seat, "second"));
    let t1 = std::thread::spawn(move || w1.admit(r1).unwrap());
    let t2 = std::thread::spawn(move || w2.admit(r2).unwrap());
    let (a, b) = (t1.join().unwrap(), t2.join().unwrap());
    fx.w.drain().unwrap();
    let states = [
        fx.w.status(&a).unwrap().unwrap(),
        fx.w.status(&b).unwrap().unwrap(),
    ];
    assert_eq!(
        states.iter().filter(|s| **s == OpState::Committed).count(),
        1,
        "{states:?}"
    );
    assert_eq!(
        states.iter().filter(|s| **s == OpState::Rejected).count(),
        1,
        "{states:?}"
    );
    let loser = if states[0] == OpState::Rejected {
        &a
    } else {
        &b
    };
    let rej =
        fx.w.journal()
            .get(loser)
            .unwrap()
            .unwrap()
            .rejection
            .unwrap();
    assert_eq!(rej.reason, "precondition_failed");
    assert!(rej.explanation.contains(fx.seat.as_str()));
    let winner = if states[0] == OpState::Committed {
        &a
    } else {
        &b
    };
    assert_eq!(trailer_count(fx.w.store(), winner), 1);
    assert_eq!(trailer_count(fx.w.store(), loser), 0);
}
