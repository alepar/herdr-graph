//! Working-tree view (spec §3.5): the checked-out files are a derived view of `refs/heads/main`.
//! The writer fast-forwards them after each commit, never overwriting a file the user changed.
use crate::model::{CommitId, OpId, Timestamp};
use crate::ports::store::StoreError;
use git2::{Delta, ObjectType, Oid, Repository};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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
            Ok(Some(
                Oid::hash_object(ObjectType::Blob, &bytes).map_err(git_err)?,
            ))
        }
        Ok(_) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// FF the working tree from tree `old` (None = unknown/empty) to `new` (spec §3.5): for each changed path,
/// write or delete only when the on-disk content equals the old blob (or already equals the new one);
/// otherwise leave it and record a DirtyEntry (JSON line) in .graph-local/worktree_dirty.
/// Each fast-forward prunes resolved entries, including files moved to orphans. Directories that
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
    let dirty_entries = read_dirty_preserving(root)?;
    let new_tree = repo.find_tree(new).map_err(git_err)?;
    let old_tree = old
        .map(|o| repo.find_tree(o))
        .transpose()
        .map_err(git_err)?;
    let diff = repo
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)
        .map_err(git_err)?;
    let mut report = FfReport::default();
    let mut deleted_dirs: BTreeSet<String> = BTreeSet::new();

    for delta in diff.deltas() {
        let (old_f, new_f) = (delta.old_file(), delta.new_file());
        let Some(rel) = new_f.path().or(old_f.path()).and_then(|p| p.to_str()) else {
            continue;
        };
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
                crate::fsutil::write_atomic_in(
                    &local_dir(root).join("tmp"),
                    &disk,
                    blob.content(),
                )?;
                report.updated.push(rel.to_owned());
            }
            _ => {}
        }
    }

    // Folders that disappeared from `new` but still hold files (untracked or dirty leftovers).
    let gone: Vec<&String> = deleted_dirs
        .iter()
        .filter(|d| new_tree.get_path(Path::new(d.as_str())).is_err())
        .collect();
    let key = op.map_or_else(
        || format!("recovery-{}", now.format("%Y%m%dT%H%M%S%.6fZ")),
        |o| o.as_str().to_owned(),
    );
    let orphan_root = local_dir(root).join("orphans").join(key);
    for dir in gone {
        let abs = root.join(dir);
        if !abs.is_dir() {
            continue; // already moved with an ancestor, or never existed
        }
        let mut files = Vec::new();
        collect_files(&abs, &mut files)?;
        for f in files {
            let rel = f
                .strip_prefix(root)
                .map_err(|e| StoreError::Io(std::io::Error::other(e.to_string())))?;
            let dest = crate::fsutil::unique_path(&orphan_root.join(rel));
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&f, &dest)?;
            report.orphaned.push((
                rel.to_string_lossy().into_owned(),
                dest.to_string_lossy().into_owned(),
            ));
        }
        std::fs::remove_dir_all(&abs)?;
    }

    // Index mirrors the view (not the user's staging area).
    let mut index = repo.index().map_err(git_err)?;
    index.read_tree(&new_tree).map_err(git_err)?;
    index.write().map_err(git_err)?;

    std::fs::create_dir_all(local_dir(root))?;
    // Retain per-op entries for unresolved paths so notifications keep their
    // idempotency keys, but remove markers whose disk content now matches main.
    let unresolved = |path: &str| -> Result<bool, StoreError> {
        let disk = root.join(path);
        let expected = new_tree
            .get_path(Path::new(path))
            .ok()
            .filter(|e| e.kind() == Some(ObjectType::Blob))
            .map(|e| e.id());
        Ok(disk_blob(&disk)? != expected
            || (expected.is_none() && std::fs::symlink_metadata(&disk).is_ok()))
    };
    let mut entries = Vec::new();
    for entry in dirty_entries {
        if unresolved(&entry.path)? {
            entries.push(entry);
        }
    }
    let mut active_dirty = Vec::new();
    for path in &report.dirty {
        if !unresolved(path)? {
            continue;
        }
        active_dirty.push(path.clone());
        if !entries
            .iter()
            .any(|e| e.path == *path && e.op.as_ref() == op)
        {
            entries.push(DirtyEntry {
                path: path.clone(),
                op: op.cloned(),
                at: now,
            });
        }
    }
    report.dirty = active_dirty;
    let mut bytes = Vec::new();
    for entry in entries {
        serde_json::to_writer(&mut bytes, &entry)
            .map_err(|e| StoreError::Io(std::io::Error::other(e)))?;
        bytes.push(b'\n');
    }
    crate::fsutil::write_atomic(&dirty_file(root), &bytes)?;
    crate::fsutil::write_atomic(&view_rev_file(root), commit.0.as_bytes())?;
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

/// The writer must not replace a marker file it could not read completely.
fn read_dirty_preserving(root: &Path) -> Result<Vec<DirtyEntry>, StoreError> {
    let text = match std::fs::read_to_string(dirty_file(root)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    text.lines()
        .map(|line| {
            serde_json::from_str(line).map_err(|e| StoreError::Io(std::io::Error::other(e)))
        })
        .collect()
}

/// Dirty entries, deduped by path, newest wins; a missing file means none.
pub fn read_dirty(root: &Path) -> Vec<DirtyEntry> {
    let Ok(text) = std::fs::read_to_string(dirty_file(root)) else {
        return Vec::new();
    };
    let mut by_path: std::collections::BTreeMap<String, DirtyEntry> = Default::default();
    for e in text
        .lines()
        .filter_map(|l| serde_json::from_str::<DirtyEntry>(l).ok())
    {
        match by_path.get(&e.path) {
            Some(prev) if prev.at > e.at => {}
            _ => {
                by_path.insert(e.path.clone(), e);
            }
        }
    }
    by_path.into_values().collect()
}

/// Every parsable dirty entry in file order, not deduped: two ops that dirty the same file are both kept.
pub fn read_dirty_all(root: &Path) -> Vec<DirtyEntry> {
    let Ok(text) = std::fs::read_to_string(dirty_file(root)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| serde_json::from_str::<DirtyEntry>(l).ok())
        .collect()
}

/// The commit the working tree currently mirrors (.graph-local/view_rev).
pub fn view_rev(root: &Path) -> Option<CommitId> {
    let text = std::fs::read_to_string(view_rev_file(root)).ok()?;
    let t = text.trim();
    (!t.is_empty()).then(|| CommitId(t.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn read_dirty_all_keeps_every_entry() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(local_dir(tmp.path())).unwrap();
        let at = chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap();
        let (a, b) = (OpId::new(), OpId::new());
        let lines = [
            DirtyEntry {
                path: "x.md".into(),
                op: Some(a),
                at,
            },
            DirtyEntry {
                path: "x.md".into(),
                op: Some(b),
                at,
            },
            DirtyEntry {
                path: "y.md".into(),
                op: None,
                at,
            },
        ];
        let mut text = String::new();
        for e in &lines {
            text.push_str(&serde_json::to_string(e).unwrap());
            text.push('\n');
        }
        text.push_str("not json\n");
        std::fs::write(dirty_file(tmp.path()), text).unwrap();
        assert_eq!(read_dirty_all(tmp.path()), lines.to_vec());
        assert_eq!(
            read_dirty(tmp.path()).len(),
            2,
            "the deduping reader still collapses by path"
        );
    }

    fn tree_of(repo: &Repository, files: &[(&str, &str)]) -> Oid {
        let mut idx = repo.index().unwrap();
        idx.clear().unwrap();
        for (path, content) in files {
            let entry = git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: 0o100644,
                uid: 0,
                gid: 0,
                file_size: 0,
                id: Oid::zero(),
                flags: 0,
                flags_extended: 0,
                path: path.as_bytes().to_vec(),
            };
            idx.add_frombuffer(&entry, content.as_bytes()).unwrap();
        }
        idx.write_tree().unwrap()
    }

    fn at(sec: u32) -> Timestamp {
        chrono::Utc
            .with_ymd_and_hms(2026, 10, 2, 12, 0, sec)
            .unwrap()
    }

    fn files_under(dir: &Path) -> Vec<(PathBuf, String)> {
        let mut v = Vec::new();
        if dir.exists() {
            let mut all = Vec::new();
            collect_files(dir, &mut all).unwrap();
            for f in all {
                v.push((f.clone(), std::fs::read_to_string(f).unwrap()));
            }
        }
        v
    }

    #[test]
    fn orphan_collision_preserves_both_copies() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = Repository::init(root).unwrap();
        let old = tree_of(&repo, &[("dir/x.md", "x"), ("keep.md", "k")]);
        let new = tree_of(&repo, &[("keep.md", "k")]);
        let c = CommitId("c".into());
        for (n, content) in [(1u32, "first copy"), (2, "second copy")] {
            std::fs::create_dir_all(root.join("dir")).unwrap();
            std::fs::write(root.join("dir/x.md"), "x").unwrap();
            std::fs::write(root.join("keep.md"), "k").unwrap();
            std::fs::write(root.join("dir/notes.md"), content).unwrap();
            let r = fast_forward(&repo, root, Some(old), new, &c, None, at(n)).unwrap();
            assert_eq!(r.orphaned.len(), 1, "run {n}: {r:?}");
            assert!(!root.join("dir").exists());
        }
        let mut copies: Vec<String> = files_under(&local_dir(root).join("orphans"))
            .into_iter()
            .map(|(_, c)| c)
            .collect();
        copies.sort();
        assert_eq!(
            copies,
            vec!["first copy".to_owned(), "second copy".to_owned()]
        );
    }

    #[test]
    fn orphan_destination_collision_gets_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = Repository::init(root).unwrap();
        let old = tree_of(&repo, &[("dir/x.md", "x"), ("keep.md", "k")]);
        let new = tree_of(&repo, &[("keep.md", "k")]);
        std::fs::create_dir_all(root.join("dir")).unwrap();
        std::fs::write(root.join("dir/x.md"), "x").unwrap();
        std::fs::write(root.join("keep.md"), "k").unwrap();
        std::fs::write(root.join("dir/notes.md"), "new copy").unwrap();
        let op = OpId::new();
        let dest = local_dir(root)
            .join("orphans")
            .join(op.as_str())
            .join("dir/notes.md");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, "original").unwrap();
        let r = fast_forward(
            &repo,
            root,
            Some(old),
            new,
            &CommitId("c".into()),
            Some(&op),
            at(1),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "original");
        let suffixed = PathBuf::from(format!("{}.1", dest.display()));
        assert_eq!(std::fs::read_to_string(&suffixed).unwrap(), "new copy");
        assert_eq!(r.orphaned[0].1, suffixed.to_string_lossy());
    }

    #[test]
    fn dirty_entries_clear_after_resolution_but_keep_unresolved_edits() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = Repository::init(root).unwrap();
        let old = tree_of(&repo, &[("a.md", "old"), ("b.md", "old")]);
        let new = tree_of(&repo, &[("a.md", "new"), ("b.md", "new")]);
        let c = CommitId("c".into());
        fast_forward(&repo, root, None, old, &c, None, at(1)).unwrap();
        std::fs::write(root.join("a.md"), "mine").unwrap();
        std::fs::write(root.join("b.md"), "mine").unwrap();
        fast_forward(&repo, root, Some(old), new, &c, None, at(2)).unwrap();
        assert_eq!(read_dirty(root).len(), 2);
        std::fs::write(root.join("a.md"), "new").unwrap();
        // No changed paths: stale markers must still be refreshed.
        fast_forward(&repo, root, Some(new), new, &c, None, at(3)).unwrap();
        let dirty = read_dirty(root);
        assert_eq!(
            dirty.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            vec!["b.md"]
        );
        assert_eq!(std::fs::read_to_string(root.join("b.md")).unwrap(), "mine");
        std::fs::write(root.join("b.md"), "new").unwrap();
        fast_forward(&repo, root, Some(new), new, &c, None, at(4)).unwrap();
        assert!(read_dirty(root).is_empty());
    }

    #[test]
    fn malformed_dirty_marker_is_preserved_on_refresh_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = Repository::init(root).unwrap();
        let tree = tree_of(&repo, &[("a.md", "old")]);
        let c = CommitId("c".into());
        fast_forward(&repo, root, None, tree, &c, None, at(1)).unwrap();
        let marker = "truncated unresolved entry\n";
        std::fs::write(dirty_file(root), marker).unwrap();
        assert!(fast_forward(&repo, root, Some(tree), tree, &c, None, at(2)).is_err());
        assert_eq!(std::fs::read_to_string(dirty_file(root)).unwrap(), marker);
    }

    #[test]
    fn orphaned_edits_do_not_leave_active_dirty_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = Repository::init(root).unwrap();
        let old = tree_of(&repo, &[("dir/a.md", "old")]);
        let new = tree_of(&repo, &[("new/a.md", "old")]);
        let c = CommitId("c".into());
        fast_forward(&repo, root, None, old, &c, None, at(1)).unwrap();
        std::fs::write(root.join("dir/a.md"), "mine").unwrap();
        let report = fast_forward(&repo, root, Some(old), new, &c, None, at(2)).unwrap();
        assert_eq!(report.orphaned.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&report.orphaned[0].1).unwrap(),
            "mine"
        );
        assert!(
            read_dirty(root).is_empty(),
            "the edited file is now in orphans"
        );
    }

    #[test]
    fn interrupted_ff_rerun_converges() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = Repository::init(root).unwrap();
        let old = tree_of(&repo, &[("a.md", "old a"), ("b.md", "old b")]);
        let new = tree_of(&repo, &[("a.md", "new a"), ("b.md", "new b")]);
        let (c_old, c_new) = (CommitId("old".into()), CommitId("new".into()));
        fast_forward(&repo, root, None, old, &c_old, None, at(1)).unwrap();
        // crash point (a): a.md still old (crash before rename); crash point (b): b.md already new, view_rev old.
        std::fs::write(root.join("b.md"), "new b").unwrap();
        assert_eq!(view_rev(root), Some(c_old.clone()));
        let r = fast_forward(&repo, root, Some(old), new, &c_new, None, at(2)).unwrap();
        assert_eq!(r.updated, vec!["a.md".to_owned()], "{r:?}");
        assert!(
            r.dirty.is_empty(),
            "no torn or half-applied file is classified as a user edit: {r:?}"
        );
        assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), "new a");
        assert_eq!(std::fs::read_to_string(root.join("b.md")).unwrap(), "new b");
        assert_eq!(view_rev(root), Some(c_new));
        let mut all = Vec::new();
        collect_files(root, &mut all).unwrap();
        let leftovers: Vec<_> = all
            .iter()
            .filter(|p| {
                !p.starts_with(root.join(".git")) && p.extension().is_some_and(|e| e == "tmp")
            })
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
