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
    /// A reference does not resolve at the committed revision (spec §4.2). Build only via `unresolved`.
    #[error("{holder}: {field} {target} {reason}")]
    Integrity {
        holder: String,
        field: String,
        target: String,
        reason: String,
    },
}

impl StoreError {
    /// Replace the reason of an `Integrity` error; identity for every other variant.
    pub fn because(self, reason: impl Into<String>) -> Self {
        match self {
            Self::Integrity {
                holder,
                field,
                target,
                ..
            } => Self::Integrity {
                holder,
                field,
                target,
                reason: reason.into(),
            },
            other => other,
        }
    }
}

/// The one constructor for `StoreError::Integrity`: `holder`'s `field` names `target`, which does not
/// resolve at the committed revision. Use `.because(..)` to replace the default reason
/// (e.g. wrong kind, broken team structure, a manager-chain cycle).
pub fn unresolved(
    holder: impl std::fmt::Display,
    field: &str,
    target: impl std::fmt::Display,
) -> StoreError {
    StoreError::Integrity {
        holder: holder.to_string(),
        field: field.to_string(),
        target: target.to_string(),
        reason: "not found at the committed revision".to_string(),
    }
}

/// Read-only access to complete committed revisions. Readers never see partial writes.
pub trait Store: Send + Sync {
    fn instance_root(&self) -> Option<PathBuf> {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_builds_integrity_with_default_reason() {
        let e = unresolved("st_a", "template_ref", "tp_b");
        match &e {
            StoreError::Integrity {
                holder,
                field,
                target,
                reason,
            } => {
                assert_eq!(
                    (holder.as_str(), field.as_str(), target.as_str()),
                    ("st_a", "template_ref", "tp_b")
                );
                assert_eq!(reason, "not found at the committed revision");
            }
            other => panic!("expected Integrity, got {other:?}"),
        }
        assert_eq!(
            e.to_string(),
            "st_a: template_ref tp_b not found at the committed revision"
        );
    }

    #[test]
    fn because_replaces_reason_only_on_integrity() {
        let e = unresolved("st_a", "template_ref", "tp_b").because("is not a seat template");
        assert_eq!(
            e.to_string(),
            "st_a: template_ref tp_b is not a seat template"
        );
        let other = StoreError::Git("boom".into()).because("ignored");
        assert!(matches!(other, StoreError::Git(m) if m == "boom"));
    }
}
