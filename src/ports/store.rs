//! Committed-revision reads (spec §1 `store`). Implemented by hg-zmi.2 over git2.
use crate::model::{AnyId, BlobHash, CommitId};
use serde::de::DeserializeOwned;
use std::path::PathBuf;

/// Repository-relative path: forward slashes, no leading '/', no '..', no backslashes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoPath(String);
impl RepoPath {
    pub fn new(s: &str) -> Result<Self, StoreError> {
        let bad =
            s.starts_with('/') || s.contains('\\') || s.split('/').any(|c| c == ".." || c == ".");
        if bad {
            return Err(StoreError::InvalidPath(s.to_owned()));
        }
        Ok(Self(s.trim_end_matches('/').to_owned()))
    }
    pub fn root() -> Self {
        Self(String::new())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn join(&self, name: &str) -> Result<Self, StoreError> {
        if self.0.is_empty() {
            Self::new(name)
        } else {
            Self::new(&format!("{}/{name}", self.0))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
}

/// Where an object currently lives in a given revision (paths are never cached as authority, spec §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectLocation {
    pub id: AnyId,
    pub record_path: RepoPath,
    pub folder: RepoPath,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no graph instance at {0}")]
    NoInstance(PathBuf),
    #[error("unknown commit {0}")]
    UnknownCommit(String),
    #[error("invalid repository path {0:?}")]
    InvalidPath(String),
    #[error("corrupt record at {path}: {reason}")]
    Corrupt { path: String, reason: String },
    #[error("git: {0}")]
    Git(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Read-only access to complete committed revisions. Readers never see partial writes.
pub trait Store: Send + Sync {
    /// Current tip of `refs/heads/main`.
    fn head(&self) -> Result<CommitId, StoreError>;
    /// File bytes at `path` in commit `at`; None if absent.
    fn read_file(&self, at: &CommitId, path: &RepoPath) -> Result<Option<Vec<u8>>, StoreError>;
    /// Blob hash of an (opaque) file, used for `Version::Blob` preconditions.
    fn blob_hash(&self, at: &CommitId, path: &RepoPath) -> Result<Option<BlobHash>, StoreError>;
    /// Directory listing at `path` in commit `at`; empty if absent.
    fn list_dir(&self, at: &CommitId, path: &RepoPath) -> Result<Vec<DirEntry>, StoreError>;
    /// Resolve an object id to its record path and folder in commit `at` (archived folders included).
    fn locate(&self, at: &CommitId, id: &AnyId) -> Result<Option<ObjectLocation>, StoreError>;
}

/// Read and parse a TOML record at `path` in `at`.
pub fn read_record<R: DeserializeOwned>(
    store: &dyn Store,
    at: &CommitId,
    path: &RepoPath,
) -> Result<Option<R>, StoreError> {
    let Some(bytes) = store.read_file(at, path)? else {
        return Ok(None);
    };
    let text = String::from_utf8(bytes).map_err(|e| StoreError::Corrupt {
        path: path.as_str().into(),
        reason: e.to_string(),
    })?;
    toml::from_str(&text)
        .map(Some)
        .map_err(|e| StoreError::Corrupt {
            path: path.as_str().into(),
            reason: e.to_string(),
        })
}
