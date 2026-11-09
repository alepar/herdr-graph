//! HistoryWalker: the newest-first walk of `main` that `log`, the roster's `last_change` and the stack's
//! stale-part commits read (spec §2.4).
use crate::model::action::ActionRecord;
use crate::model::operation::OperationRecord;
use crate::model::{ActionId, CommitId, OpId, Timestamp};
use crate::ports::store::{RepoPath, StoreError};
use crate::store::GitStore;
use crate::undo::candidates::read_action;
use git2::{Delta, Oid, Repository, Sort};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::ControlFlow;

/// The `Graph-Op:` / `Graph-Action:` trailers of one commit message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trailers {
    pub op: Option<OpId>,
    pub action: Option<ActionId>,
}

/// The one `Graph-Op:` / `Graph-Action:` parser (writer::recovery and HistoryWalker both call it). A value
/// that does not parse as its id leaves that trailer `None`.
pub fn parse_trailers(msg: &str) -> Trailers {
    let mut t = Trailers::default();
    for line in msg.lines() {
        if let Some(v) = line.strip_prefix("Graph-Op:") {
            t.op = v.trim().parse::<OpId>().ok();
        } else if let Some(v) = line.strip_prefix("Graph-Action:") {
            t.action = v.trim().parse::<ActionId>().ok();
        }
    }
    t
}

/// What a walk covered. `stopped_early` is true only when the scan cap was reached with older commits left
/// unscanned (so an empty result with `false` means "searched everything", with `true` "gave up").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    pub scanned: usize,
    pub max_scan: usize,
    pub stopped_early: bool,
}

/// A record the commit should have carried but that did not parse (or was missing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Malformed {
    Operation,
    Action,
}

/// One commit as the walker reads it.
#[derive(Debug, Clone)]
pub struct Change {
    pub commit: CommitId,
    /// Commit time.
    pub at: Timestamp,
    /// First line of the commit message.
    pub summary: String,
    pub trailers: Trailers,
    pub operation: Option<OperationRecord>,
    /// The action record as this commit wrote it (undo state is read separately, at HEAD).
    pub action: Option<ActionRecord>,
    /// diff-tree against the first parent (every file, for a root commit).
    pub paths: Vec<RepoPath>,
    pub parent: Option<CommitId>,
    pub malformed: Vec<Malformed>,
}

/// The newest commit that touched a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Touch {
    pub commit: CommitId,
    pub at: Timestamp,
}

const OPS_DIR: &str = "mutations/graph-changes/";
const ACTIONS_DIR: &str = "mutations/undoable-actions/";

/// The newest-first walk of `main` from a given commit.
///
/// Owns: the walk, trailer parsing ([`parse_trailers`]), per-commit record reads, the scan cap and its
/// [`ScanReport`], last-touch-per-path and the read of an action's record at the walk's start commit. It
/// hides that the source is a live Git revwalk: a derived `.graph-local` cache would replace the internals
/// here without changing a caller.
///
/// Must not: filter by scope, kind or requester (that is `LogQuery`), read the journal, or resolve HEAD
/// itself: it starts at the `CommitId` it is given (the invocation's pinned HEAD, spec §2.1). Callers never
/// use a revwalk or parse trailers directly.
///
/// Where new behavior goes: a new per-commit fact is a new `Change` field read in `change_at`; a new
/// whole-history query (like `last_touches`) is a new method that reuses `commits`.
pub struct HistoryWalker<'s> {
    store: &'s GitStore,
    from: CommitId,
    max_scan: usize,
}

impl<'s> HistoryWalker<'s> {
    pub const DEFAULT_MAX_SCAN: usize = 5000;

    pub fn new(store: &'s GitStore, from: &CommitId, max_scan: usize) -> Self {
        HistoryWalker {
            store,
            from: from.clone(),
            max_scan,
        }
    }

    pub fn store(&self) -> &'s GitStore {
        self.store
    }

    pub fn from(&self) -> &CommitId {
        &self.from
    }

    pub fn max_scan(&self) -> usize {
        self.max_scan
    }

    /// Up to `max_scan` commit ids newest first, and whether older ones were left. Ids only, so the repo lock
    /// is not held while a caller's callback reads the store.
    fn commits(&self) -> Result<(Vec<Oid>, bool), StoreError> {
        let from = Oid::from_str(&self.from.0)
            .map_err(|_| StoreError::UnknownCommit(self.from.0.clone()))?;
        let cap = self.max_scan;
        self.store.with_repo(|repo| {
            let mut walk = repo.revwalk()?;
            walk.set_sorting(Sort::TOPOLOGICAL)?;
            walk.simplify_first_parent()?;
            walk.push(from)?;
            let mut ids = Vec::new();
            let mut more = false;
            for oid in walk {
                if ids.len() == cap {
                    more = true;
                    break;
                }
                ids.push(oid?);
            }
            Ok((ids, more))
        })
    }

    /// Newest first from `from`. Calls `f` per commit until it returns `ControlFlow::Break(())`, the cap, or
    /// the root. `stopped_early` is true only when the cap was reached with older commits left unscanned.
    pub fn walk(
        &self,
        mut f: impl FnMut(&Change) -> ControlFlow<()>,
    ) -> Result<ScanReport, StoreError> {
        let (ids, more) = self.commits()?;
        let mut scanned = 0;
        for oid in ids {
            let change = self.store.with_repo(|repo| change_at(repo, oid))?;
            scanned += 1;
            if f(&change).is_break() {
                return Ok(self.report(scanned, false));
            }
        }
        Ok(self.report(scanned, more))
    }

    fn report(&self, scanned: usize, stopped_early: bool) -> ScanReport {
        ScanReport {
            scanned,
            max_scan: self.max_scan,
            stopped_early,
        }
    }

    /// The newest commit (within the cap) whose diff touches each path (the path itself, or a file under it);
    /// one pass for the whole set. A path never touched within the cap is absent from the map.
    pub fn last_touches(
        &self,
        paths: &[RepoPath],
    ) -> Result<BTreeMap<RepoPath, Touch>, StoreError> {
        let (ids, _) = self.commits()?;
        let mut found: BTreeMap<RepoPath, Touch> = BTreeMap::new();
        for oid in ids {
            if found.len() == paths.len() {
                break;
            }
            let (touch, changed) = self.store.with_repo(|repo| {
                let commit = repo.find_commit(oid)?;
                Ok((
                    Touch {
                        commit: CommitId(oid.to_string()),
                        at: commit_time(&commit),
                    },
                    changed_paths(repo, &commit)?,
                ))
            })?;
            for p in paths {
                if !found.contains_key(p) && changed.iter().any(|c| is_under(c, p)) {
                    found.insert(p.clone(), touch.clone());
                }
            }
        }
        Ok(found)
    }

    /// The action's record at `from` (the pinned HEAD), for undoable/undone; `None` when it no longer exists
    /// there (or does not parse).
    pub fn action_at_head(&self, act: &ActionId) -> Result<Option<ActionRecord>, StoreError> {
        match read_action(&self.store.at(&self.from), act) {
            Err(StoreError::Corrupt { .. }) => Ok(None),
            other => other,
        }
    }
}

fn is_under(path: &RepoPath, base: &RepoPath) -> bool {
    let (p, b) = (path.as_str(), base.as_str());
    b.is_empty() || p == b || (p.starts_with(b) && p.as_bytes().get(b.len()) == Some(&b'/'))
}

fn commit_time(c: &git2::Commit<'_>) -> Timestamp {
    chrono::DateTime::from_timestamp(c.time().seconds(), 0).unwrap_or_default()
}

/// Paths changed against the first parent (all files of a root commit); a removed path counts under its old
/// name.
fn changed_paths(
    repo: &Repository,
    commit: &git2::Commit<'_>,
) -> Result<Vec<RepoPath>, git2::Error> {
    let tree = commit.tree()?;
    let parent_tree = match commit.parent(0) {
        Ok(p) => Some(p.tree()?),
        Err(_) => None,
    };
    let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;
    let mut out = Vec::new();
    for d in diff.deltas() {
        let file = if d.status() == Delta::Deleted {
            d.old_file()
        } else {
            d.new_file()
        };
        if let Some(p) = file.path().and_then(|p| p.to_str())
            && let Ok(p) = RepoPath::new(p)
        {
            out.push(p);
        }
    }
    Ok(out)
}

/// The record file under `dir` among the commit's changed paths whose stem is `id`, read from the commit's
/// tree and parsed. `None` when absent; `Err` text when present but unreadable.
fn read_record<R: serde::de::DeserializeOwned>(
    repo: &Repository,
    tree: &git2::Tree<'_>,
    paths: &[RepoPath],
    dir: &str,
    id: &str,
) -> Option<Result<R, ()>> {
    let want = format!("{id}.toml");
    let path = paths.iter().find(|p| {
        p.as_str()
            .strip_prefix(dir)
            .is_some_and(|rest| rest.rsplit('/').next() == Some(want.as_str()))
    })?;
    let parsed = (|| {
        let entry = tree.get_path(std::path::Path::new(path.as_str())).ok()?;
        let blob = repo.find_blob(entry.id()).ok()?;
        let text = std::str::from_utf8(blob.content()).ok()?;
        toml::from_str::<R>(text).ok()
    })();
    Some(parsed.ok_or(()))
}

fn change_at(repo: &Repository, oid: Oid) -> Result<Change, git2::Error> {
    let commit = repo.find_commit(oid)?;
    let tree = commit.tree()?;
    let msg = commit.message().unwrap_or_default();
    let trailers = parse_trailers(msg);
    let paths = changed_paths(repo, &commit)?;
    let mut malformed = Vec::new();
    let operation = match &trailers.op {
        None => None,
        Some(op) => match read_record::<OperationRecord>(repo, &tree, &paths, OPS_DIR, op.as_str())
        {
            Some(Ok(r)) => Some(r),
            _ => {
                malformed.push(Malformed::Operation);
                None
            }
        },
    };
    let action = match &trailers.action {
        None => None,
        Some(a) => {
            match read_record::<ActionRecord>(repo, &tree, &paths, ACTIONS_DIR, a.as_str()) {
                Some(Ok(r)) => Some(r),
                _ => {
                    malformed.push(Malformed::Action);
                    None
                }
            }
        }
    };
    Ok(Change {
        commit: CommitId(oid.to_string()),
        at: commit_time(&commit),
        summary: msg.lines().next().unwrap_or_default().to_owned(),
        trailers,
        operation,
        action,
        paths,
        parent: commit.parent_id(0).ok().map(|p| CommitId(p.to_string())),
        malformed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::fixture::Fx;
    use crate::model::Lifecycle;
    use crate::model::action::ActionKind;
    use crate::model::change::{RequestKind, Requester};
    use crate::ports::store::Store;
    use crate::store::layout;

    fn rp(s: &str) -> RepoPath {
        RepoPath::new(s).unwrap()
    }

    /// Init commit plus `n` plain commits, each adding one seat to a teamspace.
    fn linear(n: usize) -> (Fx, Vec<CommitId>) {
        let mut fx = Fx::new();
        let t = fx.teamspace("alpha", Lifecycle::Active);
        let mut ids = vec![];
        for i in 0..n {
            fx.seat(&t, &format!("s{i}"), Lifecycle::Active);
            ids.push(fx.commit(&format!("c{i}")));
        }
        (fx, ids)
    }

    #[test]
    fn parse_trailers_forms() {
        let op = OpId::new();
        let act = ActionId::new();
        let both = format!("retire: x\n\nGraph-Op: {op}\nGraph-Action: {act}\n");
        assert_eq!(
            parse_trailers(&both),
            Trailers {
                op: Some(op.clone()),
                action: Some(act.clone())
            }
        );
        let only = format!("x\n\nGraph-Op: {op}\n");
        assert_eq!(
            parse_trailers(&only),
            Trailers {
                op: Some(op),
                action: None
            }
        );
        assert_eq!(parse_trailers("just a message"), Trailers::default());
        assert_eq!(
            parse_trailers("x\n\nGraph-Op: not-an-id\nGraph-Action: act_nope\n"),
            Trailers::default()
        );
    }

    #[test]
    fn walk_newest_first_and_cap() {
        let (fx, ids) = linear(5);
        let head = fx.store.head().unwrap();
        let mut seen = vec![];
        let rep = HistoryWalker::new(&fx.store, &head, 3)
            .walk(|c| {
                seen.push(c.commit.clone());
                ControlFlow::Continue(())
            })
            .unwrap();
        assert_eq!(seen, vec![ids[4].clone(), ids[3].clone(), ids[2].clone()]);
        assert_eq!(
            rep,
            ScanReport {
                scanned: 3,
                max_scan: 3,
                stopped_early: true
            }
        );
        let mut n = 0;
        let rep = HistoryWalker::new(&fx.store, &head, 10)
            .walk(|_| {
                n += 1;
                ControlFlow::Continue(())
            })
            .unwrap();
        assert_eq!((n, rep.scanned, rep.stopped_early), (6, 6, false));
        // The cap equal to the history length is not an early stop.
        let rep = HistoryWalker::new(&fx.store, &head, 6)
            .walk(|_| ControlFlow::Continue(()))
            .unwrap();
        assert_eq!((rep.scanned, rep.stopped_early), (6, false));
        let mut n = 0;
        let rep = HistoryWalker::new(&fx.store, &head, 10)
            .walk(|_| {
                n += 1;
                if n == 2 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap();
        assert_eq!((rep.scanned, rep.stopped_early), (2, false));
    }

    #[test]
    fn walk_starts_at_given_commit() {
        let (mut fx, ids) = linear(2);
        let pinned = fx.store.head().unwrap();
        let later = fx.commit("after pin");
        let mut seen = vec![];
        HistoryWalker::new(&fx.store, &pinned, 100)
            .walk(|c| {
                seen.push(c.commit.clone());
                ControlFlow::Continue(())
            })
            .unwrap();
        assert!(!seen.contains(&later));
        assert_eq!(seen[0], ids[1]);
    }

    #[test]
    fn records_read_from_the_commit_tree() {
        let mut fx = Fx::new();
        let op = fx.operation(RequestKind::SeatCreate, "made", Requester::default());
        let act = fx.action(
            ActionKind::Retire,
            std::slice::from_ref(&op),
            vec![],
            vec![],
        );
        let c = fx.commit_with_trailers("seat_create: made", Some(&op), Some(&act));
        let head = fx.store.head().unwrap();
        let mut got = None;
        HistoryWalker::new(&fx.store, &head, 10)
            .walk(|ch| {
                if ch.commit == c {
                    got = Some(ch.clone());
                }
                ControlFlow::Continue(())
            })
            .unwrap();
        let ch = got.unwrap();
        assert_eq!(ch.operation.unwrap().summary, "made");
        assert_eq!(ch.action.unwrap().id, act);
        assert!(ch.malformed.is_empty());
        assert_eq!(ch.summary, "seat_create: made");
        assert!(ch.parent.is_some());
        assert!(
            ch.paths
                .iter()
                .any(|p| p.as_str().starts_with("mutations/graph-changes/"))
        );
    }

    #[test]
    fn malformed_records_flagged() {
        let mut fx = Fx::new();
        let bad = OpId::new();
        let at = crate::inspect::fixture::ts("2026-11-01T00:00:00Z");
        fx.put_file(layout::operation_record(at, &bad), b"not = [valid".to_vec());
        let c1 = fx.commit_with_trailers("bad op", Some(&bad), None);
        let good = fx.operation(RequestKind::SeatCreate, "fine", Requester::default());
        let bad_act = ActionId::new();
        fx.put_file(layout::action_record(at, &bad_act), b"\xff\xfe".to_vec());
        let c2 = fx.commit_with_trailers("bad action", Some(&good), Some(&bad_act));
        let c3 = fx.commit("after");
        let head = fx.store.head().unwrap();
        let mut by = std::collections::BTreeMap::new();
        HistoryWalker::new(&fx.store, &head, 10)
            .walk(|c| {
                by.insert(c.commit.clone(), c.clone());
                ControlFlow::Continue(())
            })
            .unwrap();
        assert!(by[&c3].malformed.is_empty());
        let a = &by[&c1];
        assert!(a.operation.is_none() && a.malformed == vec![Malformed::Operation]);
        let b = &by[&c2];
        assert!(b.operation.is_some() && b.action.is_none());
        assert_eq!(b.malformed, vec![Malformed::Action]);
    }

    #[test]
    fn unattributed_commit() {
        let (fx, _) = linear(0);
        let head = fx.store.head().unwrap();
        let mut got = vec![];
        HistoryWalker::new(&fx.store, &head, 10)
            .walk(|c| {
                got.push(c.clone());
                ControlFlow::Continue(())
            })
            .unwrap();
        let init = got.last().unwrap();
        assert!(init.parent.is_none());
        assert_eq!(init.trailers, Trailers::default());
        assert!(init.operation.is_none() && init.malformed.is_empty());
        assert!(init.paths.iter().any(|p| p.as_str() == "graph.toml"));
    }

    #[test]
    fn last_touches_one_pass() {
        let mut fx = Fx::new();
        fx.set_time("2026-11-01T00:00:00Z");
        fx.put_file(rp("rules/a.md"), b"1".to_vec());
        fx.commit("a");
        fx.set_time("2026-11-02T00:00:00Z");
        fx.put_file(rp("rules/b.md"), b"1".to_vec());
        fx.put_file(rp("rules/a.md"), b"2".to_vec());
        let c2 = fx.commit("a again and b");
        fx.set_time("2026-11-03T00:00:00Z");
        fx.put_file(rp("other.md"), b"1".to_vec());
        fx.commit("elsewhere");
        let head = fx.store.head().unwrap();
        let w = HistoryWalker::new(&fx.store, &head, 100);
        let t = w
            .last_touches(&[
                rp("rules/a.md"),
                rp("rules/b.md"),
                rp("rules"),
                rp("nope.md"),
            ])
            .unwrap();
        assert_eq!(t[&rp("rules/a.md")].commit, c2);
        assert_eq!(t[&rp("rules/b.md")].commit, c2);
        assert_eq!(t[&rp("rules")].commit, c2);
        assert!(!t.contains_key(&rp("nope.md")));
        assert_eq!(
            t[&rp("rules/a.md")].at,
            crate::inspect::fixture::ts("2026-11-02T00:00:00Z")
        );
        // A cap that excludes the touching commit finds nothing.
        let t = HistoryWalker::new(&fx.store, &head, 1)
            .last_touches(&[rp("rules/a.md")])
            .unwrap();
        assert!(t.is_empty());
    }

    #[test]
    fn action_at_head_reads_undone() {
        let mut fx = Fx::new();
        let op = fx.operation(RequestKind::SeatRetire, "retire", Requester::default());
        let act = fx.action(
            ActionKind::Retire,
            std::slice::from_ref(&op),
            vec![],
            vec![],
        );
        fx.commit_with_trailers("seat_retire: retire", Some(&op), Some(&act));
        let undo_op = OpId::new();
        fx.action_mut(&act).undone_by.push(undo_op);
        fx.commit("undo landed");
        let head = fx.store.head().unwrap();
        let w = HistoryWalker::new(&fx.store, &head, 10);
        assert!(
            !w.action_at_head(&act)
                .unwrap()
                .unwrap()
                .undone_by
                .is_empty()
        );
        assert!(w.action_at_head(&ActionId::new()).unwrap().is_none());
    }
}
