//! Commit creation and the compare-and-set of `refs/heads/main` (spec §3.6).
use super::WriterConfig;
use crate::failpoint;
use crate::store::git::graph_signature;
use git2::{ErrorCode, Oid, Repository};

pub const MAIN_REF: &str = "refs/heads/main";

#[derive(Debug)]
pub enum CasError {
    /// `refs/heads/main` is not at the expected commit.
    Mismatch { expected: Oid, found: Oid },
    /// The ref lock stayed held for every retry.
    LockContention,
    Git(git2::Error),
}

impl From<git2::Error> for CasError {
    fn from(e: git2::Error) -> Self {
        CasError::Git(e)
    }
}

impl std::fmt::Display for CasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CasError::Mismatch { expected, found } => {
                write!(f, "refs/heads/main moved: expected {expected}, found {found}")
            }
            CasError::LockContention => f.write_str("git lock contention on refs/heads/main"),
            CasError::Git(e) => write!(f, "git: {e}"),
        }
    }
}

/// Create (but do not publish) a commit on top of `parent` with `tree`.
pub fn create_commit(repo: &Repository, parent: Oid, tree: Oid, message: &str) -> Result<Oid, git2::Error> {
    let sig = graph_signature()?;
    let parent = repo.find_commit(parent)?;
    let tree = repo.find_tree(tree)?;
    repo.commit(None, &sig, &sig, message, &tree, &[&parent])
}

/// Move `refs/heads/main` from `expected` to `new` inside one ref transaction. A held ref lock is
/// retried `cfg.lock_retries` times, `cfg.lock_backoff` apart.
pub fn cas_main(repo: &Repository, expected: Oid, new: Oid, message: &str, cfg: &WriterConfig) -> Result<(), CasError> {
    let first_line = message.lines().next().unwrap_or("graph commit");
    let mut attempt = 0;
    loop {
        match cas_main_once(repo, expected, new, first_line) {
            Err(CasError::Git(e)) if e.code() == ErrorCode::Locked => {
                if attempt >= cfg.lock_retries {
                    return Err(CasError::LockContention);
                }
                attempt += 1;
                std::thread::sleep(cfg.lock_backoff);
            }
            other => return other,
        }
    }
}

fn cas_main_once(repo: &Repository, expected: Oid, new: Oid, reflog_msg: &str) -> Result<(), CasError> {
    let mut tx = repo.transaction()?;
    tx.lock_ref(MAIN_REF)?;
    failpoint!("writer.in_ref_transaction");
    let found = repo.refname_to_id(MAIN_REF)?;
    if found != expected {
        return Err(CasError::Mismatch { expected, found });
    }
    let sig = graph_signature()?;
    tx.set_target(MAIN_REF, new, Some(&sig), reflog_msg)?;
    tx.commit()?;
    Ok(())
}
