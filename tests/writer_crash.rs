//! Crash-injection: kill the writer at every failpoint, restart, and check nothing is lost or duplicated.
#![cfg(feature = "test-support")]
use herdr_graph::journal::Journal;
use herdr_graph::model::change::{ChangeRequest, RequestKind, Requester};
use herdr_graph::model::operation::OpState;
use herdr_graph::ports::clock::SystemClock;
use herdr_graph::ports::store::{RepoPath, Store};
use herdr_graph::ports::writer::Writer;
use herdr_graph::store::GitStore;
use herdr_graph::store::init::init_instance;
use herdr_graph::writer::worktree::view_rev;
use herdr_graph::writer::{Applied, Mutation, MutationCx, MutationError, MutationRegistry, WriterConfig, WriterCore};
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
        cx.tree.put_file(RepoPath::new("pair/a.txt").unwrap(), n.clone().into_bytes());
        cx.tree.put_file(RepoPath::new("pair/b.txt").unwrap(), n.into_bytes());
        Ok(Applied { summary: "pair".into(), action: None })
    }
}

fn writer(root: &Path) -> Arc<WriterCore> {
    let mut reg = MutationRegistry::default();
    reg.register("bookkeeping.pair", Arc::new(Pair));
    let store = Arc::new(GitStore::open(root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(root)).unwrap());
    WriterCore::new(store, journal, Arc::new(reg), Arc::new(SystemClock), WriterConfig::default())
}

/// Runs in the subprocess only: recover, top the journal up to TOTAL_OPS admitted ops, drain.
#[test]
fn crash_child() {
    let Ok(dir) = std::env::var("HG_CRASH_CHILD") else { return };
    herdr_graph::failpoint::arm_from_env();
    let w = writer(Path::new(&dir));
    w.recover().unwrap();
    let existing = w.journal().list(&[], 1000).unwrap().len();
    for n in existing + 1..=TOTAL_OPS {
        w.admit(ChangeRequest {
            kind: RequestKind::Bookkeeping,
            args: json!({ "sub": "pair", "n": n }),
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
        })
        .unwrap();
    }
    w.drain().unwrap();
}

fn run_child(root: &Path, failpoints: Option<&str>) -> i32 {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "crash_child", "--nocapture", "--test-threads=1"]).env("HG_CRASH_CHILD", root);
    cmd.env_remove("HG_FAILPOINTS");
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

        assert_eq!(run_child(&root, Some(&format!("{fp}=exit:42"))), 42, "failpoint {fp} never fired");
        assert_eq!(run_child(&root, None), 0, "restart after crash at {fp}");

        let journal = Journal::open(&Journal::path_in(&root)).unwrap();
        let ops = journal.list(&[], 100).unwrap();
        assert_eq!(ops.len(), TOTAL_OPS, "{fp}: exactly the admitted ops are in the journal");
        for op in &ops {
            assert_eq!(op.state, OpState::Committed, "{fp}: op {} ended {:?}", op.op, op.state);
        }

        let store = GitStore::open(&root).unwrap();
        let messages: Vec<String> = store
            .with_repo(|r| {
                let mut walk = r.revwalk()?;
                walk.push_head()?;
                walk.map(|o| Ok(r.find_commit(o?)?.message().unwrap_or_default().to_owned())).collect()
            })
            .unwrap();
        let mut seen = BTreeSet::new();
        for m in &messages {
            for line in m.lines().filter_map(|l| l.strip_prefix("Graph-Op: ")) {
                assert!(seen.insert(line.to_owned()), "{fp}: trailer for {line} appears twice");
            }
        }
        let journal_ops: BTreeSet<String> = ops.iter().map(|o| o.op.to_string()).collect();
        assert_eq!(seen, journal_ops, "{fp}: every committed op has exactly one trailer");

        let head = store.head().unwrap();
        for f in ["pair/a.txt", "pair/b.txt"] {
            let bytes = store.read_file(&head, &RepoPath::new(f).unwrap()).unwrap().unwrap();
            assert_eq!(bytes, TOTAL_OPS.to_string().as_bytes(), "{fp}: {f}");
        }
        assert_eq!(view_rev(&root), Some(head), "{fp}: working tree view is at head");
        assert_eq!(std::fs::read_to_string(root.join("pair/a.txt")).unwrap(), TOTAL_OPS.to_string(), "{fp}");
        assert!(!root.join(".git/refs/heads/main.lock").exists(), "{fp}: stale ref lock left behind");
    }
}
