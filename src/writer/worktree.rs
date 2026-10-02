//! Working-tree view (spec §3.5): the checked-out files are a derived view of `refs/heads/main`.
//! The writer fast-forwards them after each commit, never overwriting a file the user changed.
use crate::model::{CommitId, OpId, Timestamp};
use crate::ports::store::StoreError;
use git2::{Delta, ObjectType, Oid, Repository};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

fn git_err(e: git2::Error) -> StoreError {
    StoreError::Git(e.to_string())
}

#[derive(Debug, Default, PartialEq)]
pub struct FfReport {
    pub updated: Vec<String>,
    pub dirty: Vec<String>,
    /// (path under the instance, destination under .graph-local/orphans/<op>/)
    pub orphaned: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirtyEntry {
    pub path: String,
    pub op: Option<OpId>,
    pub at: Timestamp,
}

fn local_dir(root: &Path) -> PathBuf {
    root.join(".graph-local")
}

fn dirty_file(root: &Path) -> PathBuf {
    local_dir(root).join("worktree_dirty")
}

fn view_rev_file(root: &Path) -> PathBuf {
    local_dir(root).join("view_rev")
}

/// Blob oid of the bytes currently on disk, None if the path is not a regular file.
fn disk_blob(path: &Path) -> Result<Option<Oid>, StoreError> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => {
            let bytes = std::fs::read(path)?;
            Ok(Some(Oid::hash_object(ObjectType::Blob, &bytes).map_err(git_err)?))
        }
        Ok(_) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// FF the working tree from tree `old` (None = unknown/empty) to `new` (spec §3.5): for each changed path,
/// write or delete only when the on-disk content equals the old blob (or already equals the new one);
/// otherwise leave it and append a DirtyEntry (JSON line) to .graph-local/worktree_dirty. Directories that
/// held deleted files and no longer exist in `new` have their remaining files moved to
/// .graph-local/orphans/<op>/<old path>. Then refresh the index from `new` and write .graph-local/view_rev.
/// Idempotent. With `old = None` every path of `new` is treated as added: missing files are written and
/// differing ones recorded as dirty.
pub fn fast_forward(
    repo: &Repository,
    root: &Path,
    old: Option<Oid>,
    new: Oid,
    commit: &CommitId,
    op: Option<&OpId>,
    now: Timestamp,
) -> Result<FfReport, StoreError> {
    let new_tree = repo.find_tree(new).map_err(git_err)?;
    let old_tree = old.map(|o| repo.find_tree(o)).transpose().map_err(git_err)?;
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None).map_err(git_err)?;
    let mut report = FfReport::default();
    let mut deleted_dirs: BTreeSet<String> = BTreeSet::new();

    for delta in diff.deltas() {
        let (old_f, new_f) = (delta.old_file(), delta.new_file());
        let Some(rel) = new_f.path().or(old_f.path()).and_then(|p| p.to_str()) else { continue };
        let disk = root.join(rel);
        let on_disk = disk_blob(&disk)?;
        match delta.status() {
            Delta::Deleted => {
                match on_disk {
                    None => {}
                    Some(h) if h == old_f.id() => {
                        std::fs::remove_file(&disk)?;
                        report.updated.push(rel.to_owned());
                    }
                    Some(_) => report.dirty.push(rel.to_owned()),
                }
                let mut dir = Path::new(rel).parent();
                while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
                    if let Some(s) = d.to_str() {
                        deleted_dirs.insert(s.to_owned());
                    }
                    dir = d.parent();
                }
            }
            Delta::Added | Delta::Modified | Delta::Typechange => {
                let writable = match on_disk {
                    None => true,
                    Some(h) if h == new_f.id() => false,
                    Some(h) => delta.status() == Delta::Modified && h == old_f.id(),
                };
                if on_disk == Some(new_f.id()) {
                    continue;
                }
                if !writable {
                    report.dirty.push(rel.to_owned());
                    continue;
                }
                let blob = repo.find_blob(new_f.id()).map_err(git_err)?;
                if let Some(parent) = disk.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&disk, blob.content())?;
                report.updated.push(rel.to_owned());
            }
            _ => {}
        }
    }

    // Folders that disappeared from `new` but still hold files (untracked or dirty leftovers).
    let gone: Vec<&String> =
        deleted_dirs.iter().filter(|d| new_tree.get_path(Path::new(d.as_str())).is_err()).collect();
    let orphan_root = local_dir(root).join("orphans").join(op.map_or("recovery", |o| o.as_str()));
    for dir in gone {
        let abs = root.join(dir);
        if !abs.is_dir() {
            continue; // already moved with an ancestor, or never existed
        }
        let mut files = Vec::new();
        collect_files(&abs, &mut files)?;
        for f in files {
            let rel = f.strip_prefix(root).map_err(|e| StoreError::Io(std::io::Error::other(e.to_string())))?;
            let dest = orphan_root.join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&f, &dest)?;
            report.orphaned.push((rel.to_string_lossy().into_owned(), dest.to_string_lossy().into_owned()));
        }
        std::fs::remove_dir_all(&abs)?;
    }

    // Index mirrors the view (not the user's staging area).
    let mut index = repo.index().map_err(git_err)?;
    index.read_tree(&new_tree).map_err(git_err)?;
    index.write().map_err(git_err)?;

    std::fs::create_dir_all(local_dir(root))?;
    if !report.dirty.is_empty() {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(dirty_file(root))?;
        for path in &report.dirty {
            let entry = DirtyEntry { path: path.clone(), op: op.cloned(), at: now };
            let line = serde_json::to_string(&entry).map_err(|e| StoreError::Io(std::io::Error::other(e)))?;
            writeln!(f, "{line}")?;
        }
    }
    std::fs::write(view_rev_file(root), &commit.0)?;
    Ok(report)
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), StoreError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Dirty entries, deduped by path, newest wins; a missing file means none.
pub fn read_dirty(root: &Path) -> Vec<DirtyEntry> {
    let Ok(text) = std::fs::read_to_string(dirty_file(root)) else { return Vec::new() };
    let mut by_path: std::collections::BTreeMap<String, DirtyEntry> = Default::default();
    for e in text.lines().filter_map(|l| serde_json::from_str::<DirtyEntry>(l).ok()) {
        match by_path.get(&e.path) {
            Some(prev) if prev.at > e.at => {}
            _ => {
                by_path.insert(e.path.clone(), e);
            }
        }
    }
    by_path.into_values().collect()
}

/// The commit the working tree currently mirrors (.graph-local/view_rev).
pub fn view_rev(root: &Path) -> Option<CommitId> {
    let text = std::fs::read_to_string(view_rev_file(root)).ok()?;
    let t = text.trim();
    (!t.is_empty()).then(|| CommitId(t.to_owned()))
}
