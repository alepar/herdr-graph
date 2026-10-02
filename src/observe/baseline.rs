//! Persisted baseline: the last complete snapshot and its incarnation (spec §4.3.1).
//! `<instance>/.graph-local/baseline.json`, written atomically (temp file + rename).
use crate::model::{Incarnation, Timestamp};
use crate::ports::herdr::HerdrSnapshot;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub incarnation: Incarnation,
    pub snapshot: HerdrSnapshot,
    pub taken_at: Timestamp,
}

impl Baseline {
    pub fn new(snapshot: HerdrSnapshot, taken_at: Timestamp) -> Self {
        Self { incarnation: snapshot.incarnation.clone(), snapshot, taken_at }
    }
}

/// A missing or unreadable baseline is `None`: the next snapshot is then a rebind pass, never a diff.
pub fn load(path: &Path) -> Option<Baseline> {
    let raw = std::fs::read(path).ok()?;
    serde_json::from_slice(&raw).ok()
}

pub fn save(path: &Path, b: &Baseline) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let bytes = serde_json::to_vec(b).map_err(std::io::Error::other)?;
    crate::fsutil::write_atomic(path, &bytes)
}
