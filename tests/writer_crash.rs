//! Crash-injection: kill the writer at every failpoint, restart, and check nothing is lost or duplicated.
#![cfg(feature = "test-support")]
mod support;

use herdr_graph::journal::Journal;
use herdr_graph::model::change::{ChangeRequest, RequestKind, Requester};
use herdr_graph::model::operation::OpState;
use herdr_graph::ports::clock::SystemClock;
use herdr_graph::ports::store::{RepoPath, Store};
use herdr_graph::ports::writer::Writer;
use herdr_graph::store::GitStore;
use herdr_graph::store::init::init_instance;
use herdr_graph::store::tree::TreeRead;
use herdr_graph::writer::worktree::view_rev;
use herdr_graph::writer::{
    Applied, Mutation, MutationCx, MutationError, MutationRegistry, WriterConfig, WriterCore,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

const TOTAL_OPS: usize = 5;
const FAILPOINTS: [&str; 5] = [
    "writer.after_admit",
    "writer.after_tree_build",
    "writer.in_ref_transaction",
    "writer.after_cas_before_journal",
    "writer.after_journal_before_ff",
];

struct Pair;
impl Mutation for Pair {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let n = cx.request.args["n"].as_i64().unwrap().to_string();
        cx.tree
            .put_file(RepoPath::new("pair/a.txt").unwrap(), n.clone().into_bytes());
        cx.tree
            .put_file(RepoPath::new("pair/b.txt").unwrap(), n.clone().into_bytes());
        // Ordered, exactly-once evidence: a duplicate, lost or reordered op shows in the final log.
        let log_path = RepoPath::new("pair/log.txt").unwrap();
        let mut log = cx
            .tree
            .read_file(&log_path)
            .map_err(MutationError::Store)?
            .unwrap_or_default();
        log.extend_from_slice(format!("{n}\n").as_bytes());
        cx.tree.put_file(log_path, log);
        Ok(Applied {
            summary: "pair".into(),
            action: None,
        })
    }
}

fn writer(root: &Path) -> Arc<WriterCore> {
    let mut reg = MutationRegistry::default();
    reg.register("bookkeeping.pair", Arc::new(Pair));
    let store = Arc::new(GitStore::open(root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(root)).unwrap());
    WriterCore::new(
        store,
        journal,
        Arc::new(reg),
        Arc::new(SystemClock),
        WriterConfig::default(),
    )
}

/// Runs in the subprocess only: recover, admit ops, drain. `HG_CRASH_MODE` picks the shape:
/// `commit:<k>` admits up to `k` ops and drains; `queue:<k>` admits up to `k` ops and does not drain;
/// unset tops the journal up to TOTAL_OPS and drains.
#[test]
fn crash_child() {
    let Ok(dir) = std::env::var("HG_CRASH_CHILD") else {
        return;
    };
    herdr_graph::failpoint::arm_from_env();
    let w = writer(Path::new(&dir));
    w.recover().unwrap();
    let existing = w.journal().list(&[], 1000).unwrap().len();
    let mode = std::env::var("HG_CRASH_MODE").ok();
    let (upto, drain) = match mode
        .as_deref()
        .map(|m| m.split_once(':').expect("mode is kind:k"))
    {
        None => (TOTAL_OPS, true),
        Some(("commit", k)) => (k.parse().unwrap(), true),
        Some(("queue", k)) => (k.parse().unwrap(), false),
        Some((kind, _)) => panic!("unknown HG_CRASH_MODE {kind}"),
    };
    for n in existing + 1..=upto {
        w.admit(ChangeRequest {
            kind: RequestKind::Bookkeeping,
            args: json!({ "sub": "pair", "n": n }),
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
            confirmed: None,
        })
        .unwrap();
    }
    if drain {
        w.drain().unwrap();
    }
}

fn run_child(root: &Path, failpoints: Option<&str>, mode: Option<&str>) -> i32 {
    // A fresh isolated root per child: `env_clear` already drops any inherited HG_FAILPOINTS / HG_CRASH_MODE.
    let test_root = support::isolated::TestRoot::new();
    let mut cmd = test_root.command(std::env::current_exe().unwrap());
    cmd.args(["--exact", "crash_child", "--nocapture", "--test-threads=1"])
        .env("HG_CRASH_CHILD", root);
    if let Some(m) = mode {
        cmd.env("HG_CRASH_MODE", m);
    }
    if let Some(f) = failpoints {
        cmd.env("HG_FAILPOINTS", f);
    }
    let out = cmd.output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    if code != 0 && code != 42 {
        panic!(
            "child failed with {code}:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    code
}

#[test]
fn crash_at_every_failpoint_recovers() {
    for fp in FAILPOINTS {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("inst");
        init_instance(&root).unwrap();

        assert_eq!(
            run_child(&root, Some(&format!("{fp}=exit:42")), None),
            42,
            "failpoint {fp} never fired"
        );
        assert_eq!(
            run_child(&root, None, None),
            0,
            "restart after crash at {fp}"
        );

        let journal = Journal::open(&Journal::path_in(&root)).unwrap();
        let ops = journal.list(&[], 100).unwrap();
        assert_eq!(
            ops.len(),
            TOTAL_OPS,
            "{fp}: exactly the admitted ops are in the journal"
        );
        for op in &ops {
            assert_eq!(
                op.state,
                OpState::Committed,
                "{fp}: op {} ended {:?}",
                op.op,
                op.state
            );
        }

        let store = GitStore::open(&root).unwrap();
        let messages: Vec<String> = store
            .with_repo(|r| {
                let mut walk = r.revwalk()?;
                walk.push_head()?;
                walk.map(|o| Ok(r.find_commit(o?)?.message().unwrap_or_default().to_owned()))
                    .collect()
            })
            .unwrap();
        let mut seen = BTreeSet::new();
        for m in &messages {
            for line in m.lines().filter_map(|l| l.strip_prefix("Graph-Op: ")) {
                assert!(
                    seen.insert(line.to_owned()),
                    "{fp}: trailer for {line} appears twice"
                );
            }
        }
        let journal_ops: BTreeSet<String> = ops.iter().map(|o| o.op.to_string()).collect();
        assert_eq!(
            seen, journal_ops,
            "{fp}: every committed op has exactly one trailer"
        );

        let head = store.head().unwrap();
        for f in ["pair/a.txt", "pair/b.txt"] {
            let bytes = store
                .read_file(&head, &RepoPath::new(f).unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(bytes, TOTAL_OPS.to_string().as_bytes(), "{fp}: {f}");
        }
        assert_eq!(
            view_rev(&root),
            Some(head.clone()),
            "{fp}: working tree view is at head"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("pair/a.txt")).unwrap(),
            TOTAL_OPS.to_string(),
            "{fp}"
        );
        assert!(
            !root.join(".git/refs/heads/main.lock").exists(),
            "{fp}: stale ref lock left behind"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("pair/log.txt")).unwrap(),
            expected_log(),
            "{fp}: worktree log"
        );
        let log = store
            .read_file(&head, &RepoPath::new("pair/log.txt").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            String::from_utf8(log).unwrap(),
            expected_log(),
            "{fp}: each op applied exactly once, in order"
        );
    }
}

fn expected_log() -> String {
    (1..=TOTAL_OPS).map(|n| format!("{n}\n")).collect()
}

/// Graph-Op trailers on `refs/heads/main`, oldest commit first.
fn trailers_oldest_first(store: &GitStore) -> Vec<String> {
    store
        .with_repo(|r| {
            let mut walk = r.revwalk()?;
            walk.push_ref("refs/heads/main")?;
            walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)?;
            let mut out = Vec::new();
            for oid in walk {
                let msg = r
                    .find_commit(oid?)?
                    .message()
                    .unwrap_or_default()
                    .to_owned();
                out.extend(
                    msg.lines()
                        .filter_map(|l| l.strip_prefix("Graph-Op: "))
                        .map(str::to_owned),
                );
            }
            Ok(out)
        })
        .unwrap()
}

const HISTORY: usize = 2;

#[test]
fn crash_with_history_and_queue_recovers() {
    for fp in FAILPOINTS {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("inst");
        init_instance(&root).unwrap();

        // Committed history: ops 1..=HISTORY are applied and journalled.
        assert_eq!(
            run_child(&root, None, Some(&format!("commit:{HISTORY}"))),
            0,
            "{fp}: history run"
        );
        let journal = Journal::open(&Journal::path_in(&root)).unwrap();
        let checkpoint_before = journal.checkpoint().unwrap();
        let history_head = GitStore::open(&root).unwrap().head().unwrap();
        let history = journal.list(&[OpState::Committed], 100).unwrap();
        assert_eq!(
            history.len(),
            HISTORY,
            "{fp}: history committed before the crash"
        );

        // Crash with later ops queued behind (or, for after_admit, just admitted).
        let spec = format!("{fp}=exit:42");
        if fp == "writer.after_admit" {
            assert_eq!(
                run_child(&root, Some(&spec), Some("queue:5")),
                42,
                "{fp} never fired"
            );
        } else {
            assert_eq!(
                run_child(&root, None, Some("queue:5")),
                0,
                "{fp}: queue ops 3..=5"
            );
            assert_eq!(run_child(&root, Some(&spec), None), 42, "{fp} never fired");
        }

        assert_eq!(
            run_child(&root, None, None),
            0,
            "restart after crash at {fp}"
        );

        let journal = Journal::open(&Journal::path_in(&root)).unwrap();
        let mut ops = journal.list(&[], 100).unwrap();
        ops.sort_by_key(|o| o.seq);
        assert_eq!(
            ops.len(),
            TOTAL_OPS,
            "{fp}: exactly the admitted ops are in the journal"
        );
        for op in &ops {
            assert_eq!(
                op.state,
                OpState::Committed,
                "{fp}: op {} ended {:?}",
                op.op,
                op.state
            );
        }

        let store = GitStore::open(&root).unwrap();
        let committed_order = trailers_oldest_first(&store);
        let unique: BTreeSet<&String> = committed_order.iter().collect();
        assert_eq!(
            unique.len(),
            committed_order.len(),
            "{fp}: a trailer appears twice: {committed_order:?}"
        );
        let admission_order: Vec<String> = ops.iter().map(|o| o.op.to_string()).collect();
        assert_eq!(
            committed_order, admission_order,
            "{fp}: commit order differs from admission order"
        );

        let head = store.head().unwrap();
        let log = store
            .read_file(&head, &RepoPath::new("pair/log.txt").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            String::from_utf8(log).unwrap(),
            expected_log(),
            "{fp}: log.txt"
        );
        let a = store
            .read_file(&head, &RepoPath::new("pair/a.txt").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(a, TOTAL_OPS.to_string().as_bytes(), "{fp}: pair/a.txt");
        assert_eq!(
            view_rev(&root),
            Some(head.clone()),
            "{fp}: working tree view is at head"
        );
        assert!(
            !root.join(".git/refs/heads/main.lock").exists(),
            "{fp}: stale ref lock left behind"
        );

        let checkpoint = journal
            .checkpoint()
            .unwrap()
            .unwrap_or_else(|| panic!("{fp}: no checkpoint after restart"));
        assert_ne!(
            Some(&checkpoint),
            checkpoint_before.as_ref(),
            "{fp}: checkpoint did not advance"
        );
        let at_or_below = |lower: &str, upper: &str| {
            store
                .with_repo(|r| {
                    let (l, u) = (git2::Oid::from_str(lower)?, git2::Oid::from_str(upper)?);
                    Ok(l == u || r.graph_descendant_of(u, l)?)
                })
                .unwrap()
        };
        assert!(
            at_or_below(&checkpoint.0, &head.0),
            "{fp}: checkpoint {} is not an ancestor of head {}",
            checkpoint.0,
            head.0
        );
        assert!(
            at_or_below(&history_head.0, &checkpoint.0),
            "{fp}: checkpoint {} predates the committed history",
            checkpoint.0
        );
    }
}
