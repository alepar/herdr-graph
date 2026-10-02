//! Application record `applications/<id>.toml` (spec §5, §2.4).
use crate::model::common::{AppLifecycle, Retirement};
use crate::model::ids::{ActionId, AppId, MemberId, OpId, SeatId, TeamspaceId, TemplateId};
use crate::model::template::Relationship;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationRecord {
    pub schema: u32,
    pub id: AppId,
    pub rev: u64,
    pub name: String,
    pub template: TemplateId,
    pub teamspace: TeamspaceId,
    pub lifecycle: AppLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired: Option<Retirement>,
    #[serde(default)]
    pub member_map: BTreeMap<MemberId, SeatId>,
    #[serde(default)]
    pub additions: Vec<SeatId>,
    #[serde(default)]
    pub exclusions: Vec<MemberId>,
    #[serde(default)]
    pub reused: Vec<Reuse>,
    #[serde(default)]
    pub contributions: Contributions,
    pub created_by: CreatedBy,
}

/// `from: None` = pre-existing independent seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reuse {
    pub seat: SeatId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<AppId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Contributions {
    #[serde(default)]
    pub relationships: Vec<Relationship>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedBy {
    pub op: OpId,
    pub action: ActionId,
}
