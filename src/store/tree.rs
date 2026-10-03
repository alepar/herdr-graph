//! Read-only tree views and the pending-edit overlay used by writer mutations.
use super::layout;
use super::record::{Record, parse_toml, to_toml_bytes};
use crate::model::{AnyId, CommitId};
use crate::ports::store::{DirEntry, EntryKind, ObjectLocation, RepoPath, Store, StoreError};
use serde::de::DeserializeOwned;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub trait TreeRead {
    fn read_file(&self, p: &RepoPath) -> Result<Option<Vec<u8>>, StoreError>;
    fn list_dir(&self, p: &RepoPath) -> Result<Vec<DirEntry>, StoreError>;
}

/// Any Store at one commit.
pub struct CommitView<'a> {
    pub store: &'a dyn Store,
    pub at: CommitId,
}

impl TreeRead for CommitView<'_> {
    fn read_file(&self, p: &RepoPath) -> Result<Option<Vec<u8>>, StoreError> {
        self.store.read_file(&self.at, p)
    }
    fn list_dir(&self, p: &RepoPath) -> Result<Vec<DirEntry>, StoreError> {
        self.store.list_dir(&self.at, p)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Put(Vec<u8>),
    Delete,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditSet(BTreeMap<RepoPath, Edit>);

impl EditSet {
    pub fn put(&mut self, p: RepoPath, b: Vec<u8>) {
        self.0.insert(p, Edit::Put(b));
    }
    pub fn delete(&mut self, p: RepoPath) {
        self.0.insert(p, Edit::Delete);
    }
    pub fn get(&self, p: &RepoPath) -> Option<&Edit> {
        self.0.get(p)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&RepoPath, &Edit)> {
        self.0.iter()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

fn under(dir: &RepoPath, p: &RepoPath) -> Option<String> {
    if dir.as_str().is_empty() {
        return Some(p.as_str().to_owned());
    }
    p.as_str()
        .strip_prefix(dir.as_str())
        .and_then(|r| r.strip_prefix('/'))
        .map(str::to_owned)
}

/// Base commit + pending edits. Reads see the edits. Used by writer mutations (hg-zmi.3+).
pub struct Overlay<'a> {
    base: CommitView<'a>,
    edits: EditSet,
    base_cache: RefCell<HashMap<AnyId, (u64, Option<RepoPath>)>>,
}

impl<'a> Overlay<'a> {
    pub fn new(store: &'a dyn Store, base: CommitId) -> Self {
        Self {
            base: CommitView { store, at: base },
            edits: EditSet::default(),
            base_cache: RefCell::new(HashMap::new()),
        }
    }
    pub fn base(&self) -> &CommitId {
        &self.base.at
    }
    pub fn put_file(&mut self, p: RepoPath, bytes: Vec<u8>) {
        self.edits.put(p, bytes);
    }
    pub fn delete_file(&mut self, p: &RepoPath) {
        self.edits.delete(p.clone());
    }
    pub fn read_record<R: DeserializeOwned>(&self, p: &RepoPath) -> Result<Option<R>, StoreError> {
        match self.read_file(p)? {
            None => Ok(None),
            Some(b) => parse_toml(p, &b).map(Some),
        }
    }

    /// rev (0 if the object does not exist there) and record path of `id` at the BASE commit.
    fn base_info(&self, id: &AnyId) -> Result<(u64, Option<RepoPath>), StoreError> {
        if let Some(r) = self.base_cache.borrow().get(id) {
            return Ok(r.clone());
        }
        let info = match self.base.store.locate(&self.base.at, id)? {
            None => (0, None),
            Some(loc) => match self.base.read_file(&loc.record_path)? {
                None => (0, Some(loc.record_path)),
                Some(b) => {
                    let t: toml::Table = parse_toml(&loc.record_path, &b)?;
                    (
                        t.get("rev")
                            .and_then(|v| v.as_integer())
                            .map_or(0, |v| v as u64),
                        Some(loc.record_path),
                    )
                }
            },
        };
        self.base_cache
            .borrow_mut()
            .insert(id.clone(), info.clone());
        Ok(info)
    }

    /// Writer-incremented rev (spec §2.3): rev = (rev of this id at the BASE commit, or 0) + 1, so several
    /// writes of one object in one op bump it once. Serializes to TOML and puts the file.
    pub fn put_record<R: Record>(&mut self, p: RepoPath, rec: &mut R) -> Result<(), StoreError> {
        let id = rec.any_id();
        let (base_rev, base_path) = self.base_info(&id)?;
        // A record whose id already lives at another, still-present path would be a second record for
        // one id. Moves stay legal: `move_dir` deletes the old path in the overlay before the rewrite.
        if let Some(b) = base_path
            && b != p
            && self.read_file(&b)?.is_some()
        {
            return Err(StoreError::Corrupt {
                path: p.as_str().into(),
                reason: format!("duplicate id {id}: already recorded at {}", b.as_str()),
            });
        }
        let rev = base_rev + 1;
        rec.set_rev(rev);
        let bytes = to_toml_bytes(rec).map_err(|e| match e {
            StoreError::Corrupt { reason, .. } => StoreError::Corrupt {
                path: p.as_str().into(),
                reason,
            },
            other => other,
        })?;
        self.edits.put(p, bytes);
        Ok(())
    }

    fn base_files(&self, dir: &RepoPath, out: &mut BTreeSet<RepoPath>) -> Result<(), StoreError> {
        for e in self.base.list_dir(dir)? {
            let child = dir.join(&e.name)?;
            match e.kind {
                EntryKind::Dir => self.base_files(&child, out)?,
                _ => {
                    out.insert(child);
                }
            }
        }
        Ok(())
    }

    /// Every file under `dir`, recursively (merged view).
    pub fn files_under(&self, dir: &RepoPath) -> Result<Vec<RepoPath>, StoreError> {
        let mut set = BTreeSet::new();
        self.base_files(dir, &mut set)?;
        set.retain(|p| !matches!(self.edits.get(p), Some(Edit::Delete)));
        for (p, e) in self.edits.iter() {
            if matches!(e, Edit::Put(_)) && under(dir, p).is_some() {
                set.insert(p.clone());
            }
        }
        Ok(set.into_iter().collect())
    }

    /// Move every file under `from` to the same relative path under `to`; returns (old, new) pairs.
    pub fn move_dir(
        &mut self,
        from: &RepoPath,
        to: &RepoPath,
    ) -> Result<Vec<(RepoPath, RepoPath)>, StoreError> {
        let mut moved = Vec::new();
        for old in self.files_under(from)? {
            let rel = under(from, &old).expect("file is under dir");
            let new = to.join(&rel)?;
            let bytes = self.read_file(&old)?.ok_or_else(|| StoreError::Corrupt {
                path: old.as_str().into(),
                reason: "file vanished during move".into(),
            })?;
            self.edits.put(new.clone(), bytes);
            self.edits.delete(old.clone());
            moved.push((old, new));
        }
        Ok(moved)
    }

    pub fn locate(&self, id: &AnyId) -> Result<Option<ObjectLocation>, StoreError> {
        layout::locate(self, id)
    }
    pub fn edits(&self) -> &EditSet {
        &self.edits
    }
    pub fn into_edits(self) -> EditSet {
        self.edits
    }
}

impl TreeRead for Overlay<'_> {
    fn read_file(&self, p: &RepoPath) -> Result<Option<Vec<u8>>, StoreError> {
        match self.edits.get(p) {
            Some(Edit::Put(b)) => Ok(Some(b.clone())),
            Some(Edit::Delete) => Ok(None),
            None => self.base.read_file(p),
        }
    }

    fn list_dir(&self, p: &RepoPath) -> Result<Vec<DirEntry>, StoreError> {
        let mut names: BTreeMap<String, EntryKind> = BTreeMap::new();
        for e in self.base.list_dir(p)? {
            let child = p.join(&e.name)?;
            let live = match e.kind {
                EntryKind::Dir => !self.files_under(&child)?.is_empty(),
                _ => !matches!(self.edits.get(&child), Some(Edit::Delete)),
            };
            if live {
                names.insert(e.name, e.kind);
            }
        }
        for (path, e) in self.edits.iter() {
            if !matches!(e, Edit::Put(_)) {
                continue;
            }
            if let Some(rel) = under(p, path) {
                match rel.split_once('/') {
                    Some((dir, _)) => {
                        names.insert(dir.to_owned(), EntryKind::Dir);
                    }
                    None => {
                        names.insert(rel, EntryKind::File);
                    }
                }
            }
        }
        Ok(names
            .into_iter()
            .map(|(name, kind)| DirEntry { name, kind })
            .collect())
    }
}
