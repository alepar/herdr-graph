//! `teamspaces/<slug>/teamspace.toml` (spec §2.4).
use crate::model::common::{Channel, Lifecycle, NameChange, Retirement, Runtime};
use crate::model::ids::TeamspaceId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamspaceRecord {
    pub schema: u32,
    pub id: TeamspaceId,
    pub rev: u64,
    pub name: String,
    #[serde(default)]
    pub name_history: Vec<NameChange>,
    pub lifecycle: Lifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired: Option<Retirement>,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_repo: Option<PathBuf>,
    #[serde(default)]
    pub channel: Channel,
}
