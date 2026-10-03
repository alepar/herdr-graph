//! Plan store: `<instance>/.graph-local/plans/<pl_id>.json` (spec §3.3).
use super::types::StoredPlan;
use crate::model::{PlanId, Timestamp};
use std::path::{Path, PathBuf};

pub struct PlanStore {
    dir: PathBuf,
}

impl PlanStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The instance root this store belongs to (`<instance>/.graph-local/plans`).
    pub fn instance(&self) -> PathBuf {
        self.dir
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.dir.clone())
    }

    fn path(&self, id: &PlanId) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Crash-atomic write (temp + fsync + rename + directory fsync).
    pub fn put(&self, p: &StoredPlan) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let bytes = serde_json::to_vec_pretty(p).map_err(std::io::Error::other)?;
        crate::fsutil::write_atomic(&self.path(&p.plan.id), &bytes)
    }

    pub fn get(&self, id: &PlanId) -> std::io::Result<Option<StoredPlan>> {
        match std::fs::read(self.path(id)) {
            Ok(b) => serde_json::from_slice(&b)
                .map(Some)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Delete every stored plan created before `cutoff`; returns how many were removed.
    pub fn prune_older_than(&self, cutoff: Timestamp) -> std::io::Result<usize> {
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e),
        };
        let mut removed = 0;
        for entry in rd {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(stored) = serde_json::from_slice::<StoredPlan>(&std::fs::read(&path)?) else {
                continue;
            };
            if stored.created_at < cutoff {
                std::fs::remove_file(&path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}
