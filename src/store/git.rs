//! `GitStore`: committed-revision reads over git2. Never reads the working tree.
use super::tree::{CommitView, Edit, EditSet};
use crate::model::{AnyId, BlobHash, CommitId};
use crate::ports::store::{DirEntry, EntryKind, ObjectLocation, RepoPath, Store, StoreError};
use git2::build::TreeUpdateBuilder;
use git2::{FileMode, ObjectType, Oid, Repository};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const LOCATE_CACHE_MAX: usize = 10_000;

fn git_err(e: git2::Error) -> StoreError {
    StoreError::Git(e.to_string())
}

pub struct GitStore {
    root: PathBuf,
    repo: Mutex<Repository>,
    locate_cache: Mutex<HashMap<(CommitId, AnyId), Option<ObjectLocation>>>,
}

/// Signature used for every graph commit.
pub fn graph_signature() -> Result<git2::Signature<'static>, git2::Error> {
    git2::Signature::now("herdr-graph", "herdr-graph@localhost")
}

impl GitStore {
    /// Open an instance: a git repo whose refs/heads/main tree has graph.toml. Else StoreError::NoInstance(root).
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        crate::fsutil::enable_git_fsync();
        let no_instance = || StoreError::NoInstance(root.to_path_buf());
        let repo = Repository::open(root).map_err(|_| no_instance())?;
        let has_graph = (|| -> Result<bool, git2::Error> {
            let oid = repo.refname_to_id("refs/heads/main")?;
            let tree = repo.find_commit(oid)?.tree()?;
            Ok(tree.get_path(Path::new("graph.toml")).is_ok())
        })()
        .unwrap_or(false);
        if !has_graph {
            return Err(no_instance());
        }
        Ok(Self {
            root: root.to_path_buf(),
            repo: Mutex::new(repo),
            locate_cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn with_repo<T>(
        &self,
        f: impl FnOnce(&Repository) -> Result<T, git2::Error>,
    ) -> Result<T, StoreError> {
        let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        f(&repo).map_err(git_err)
    }

    pub fn at(&self, c: &CommitId) -> CommitView<'_> {
        CommitView {
            store: self,
            at: c.clone(),
        }
    }

    fn commit_oid(c: &CommitId) -> Result<Oid, StoreError> {
        Oid::from_str(&c.0).map_err(|_| StoreError::UnknownCommit(c.0.clone()))
    }

    fn commit_tree<'r>(repo: &'r Repository, c: &CommitId) -> Result<git2::Tree<'r>, StoreError> {
        let oid = Self::commit_oid(c)?;
        let commit = repo
            .find_commit(oid)
            .map_err(|_| StoreError::UnknownCommit(c.0.clone()))?;
        commit.tree().map_err(git_err)
    }

    pub fn tree_id(&self, c: &CommitId) -> Result<Oid, StoreError> {
        let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        Ok(Self::commit_tree(&repo, c)?.id())
    }

    /// Build a new tree = tree(base) + edits. Does not create a commit or move any ref.
    pub fn build_tree(&self, base: &CommitId, edits: &EditSet) -> Result<Oid, StoreError> {
        let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        let base_tree = Self::commit_tree(&repo, base)?;
        let mut b = TreeUpdateBuilder::new();
        for (path, edit) in edits.iter() {
            match edit {
                Edit::Put(bytes) => {
                    let oid = repo.blob(bytes).map_err(git_err)?;
                    b.upsert(path.as_str(), oid, FileMode::Blob);
                }
                Edit::Delete => {
                    // Deleting a path the base does not have is a no-op.
                    if base_tree.get_path(Path::new(path.as_str())).is_ok() {
                        b.remove(path.as_str());
                    }
                }
            }
        }
        b.create_updated(&repo, &base_tree).map_err(git_err)
    }
}

impl Store for GitStore {
    fn head(&self) -> Result<CommitId, StoreError> {
        self.with_repo(|r| r.refname_to_id("refs/heads/main"))
            .map(|o| CommitId(o.to_string()))
    }

    fn read_file(&self, at: &CommitId, path: &RepoPath) -> Result<Option<Vec<u8>>, StoreError> {
        if path.as_str().is_empty() {
            return Ok(None);
        }
        let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        let tree = Self::commit_tree(&repo, at)?;
        let Ok(entry) = tree.get_path(Path::new(path.as_str())) else {
            return Ok(None);
        };
        if entry.kind() != Some(ObjectType::Blob) {
            return Ok(None);
        }
        let blob = repo.find_blob(entry.id()).map_err(git_err)?;
        Ok(Some(blob.content().to_vec()))
    }

    fn blob_hash(&self, at: &CommitId, path: &RepoPath) -> Result<Option<BlobHash>, StoreError> {
        if path.as_str().is_empty() {
            return Ok(None);
        }
        let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        let tree = Self::commit_tree(&repo, at)?;
        Ok(tree
            .get_path(Path::new(path.as_str()))
            .ok()
            .filter(|e| e.kind() == Some(ObjectType::Blob))
            .map(|e| BlobHash(e.id().to_string())))
    }

    fn list_dir(&self, at: &CommitId, path: &RepoPath) -> Result<Vec<DirEntry>, StoreError> {
        let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        let root = Self::commit_tree(&repo, at)?;
        let tree = if path.as_str().is_empty() {
            root
        } else {
            let Ok(entry) = root.get_path(Path::new(path.as_str())) else {
                return Ok(Vec::new());
            };
            if entry.kind() != Some(ObjectType::Tree) {
                return Ok(Vec::new());
            }
            repo.find_tree(entry.id()).map_err(git_err)?
        };
        let mut out = Vec::new();
        for e in tree.iter() {
            let Some(name) = e.name() else { continue };
            let kind = if e.filemode() == 0o120000 {
                EntryKind::Symlink
            } else if e.kind() == Some(ObjectType::Tree) {
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            out.push(DirEntry {
                name: name.to_owned(),
                kind,
            });
        }
        Ok(out)
    }

    fn locate(&self, at: &CommitId, id: &AnyId) -> Result<Option<ObjectLocation>, StoreError> {
        let key = (at.clone(), id.clone());
        if let Some(hit) = self
            .locate_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            return Ok(hit.clone());
        }
        let found = super::layout::locate(
            &CommitView {
                store: self,
                at: at.clone(),
            },
            id,
        )?;
        let mut cache = self.locate_cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() > LOCATE_CACHE_MAX {
            cache.clear();
        }
        cache.insert(key, found.clone());
        Ok(found)
    }
}
