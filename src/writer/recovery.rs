//! Crash recovery (spec §3.6): reconcile the journal with the commits actually present in Git.
use super::worktree::FfReport;
use crate::model::{ActionId, CommitId, OpId};
use git2::{Oid, Repository};
use std::path::PathBuf;

#[derive(Debug, Default, PartialEq)]
pub struct RecoveryReport {
    pub removed_locks: Vec<PathBuf>,
    pub marked_committed: Vec<OpId>,
    pub requeued: Vec<OpId>,
    /// Committed ops whose `supersedes` target had not been marked superseded; recovery re-applied it.
    pub resuperseded: Vec<OpId>,
    pub checkpoint: Option<CommitId>,
    pub ff: FfReport,
    /// The `writer_halted` reason this recovery cleared (a restart is a successful probe).
    pub cleared_halt: Option<String>,
}

/// Remove <git dir>/refs/heads/main.lock and <git dir>/index.lock (r2). Only valid while holding the daemon flock.
pub fn remove_stale_git_locks(repo: &Repository) -> Vec<PathBuf> {
    let git_dir = repo.path();
    let mut removed = Vec::new();
    for rel in ["refs/heads/main.lock", "index.lock"] {
        let p = git_dir.join(rel);
        if p.exists() && std::fs::remove_file(&p).is_ok() {
            removed.push(p);
        }
    }
    removed
}

/// Revwalk from `head`, hiding `checkpoint` (all history if None); parse `Graph-Op:` / `Graph-Action:` trailer lines.
pub fn trailers_since(
    repo: &Repository,
    head: Oid,
    checkpoint: Option<Oid>,
) -> Result<Vec<(OpId, CommitId, Option<ActionId>)>, git2::Error> {
    let mut walk = repo.revwalk()?;
    walk.push(head)?;
    if let Some(cp) = checkpoint {
        // A checkpoint that is not (or no longer) in the object store hides nothing.
        if repo.find_commit(cp).is_ok() {
            walk.hide(cp)?;
        }
    }
    let mut out = Vec::new();
    for oid in walk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let msg = commit.message().unwrap_or_default();
        let mut op = None;
        let mut action = None;
        for line in msg.lines() {
            if let Some(v) = line.strip_prefix("Graph-Op:") {
                op = v.trim().parse::<OpId>().ok();
            } else if let Some(v) = line.strip_prefix("Graph-Action:") {
                action = v.trim().parse::<ActionId>().ok();
            }
        }
        if let Some(op) = op {
            out.push((op, CommitId(oid.to_string()), action));
        }
    }
    Ok(out)
}
