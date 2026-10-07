//! `seats/<slug>/seat.toml` (spec §2.4).
use crate::model::common::{
    Channel, Lifecycle, NameChange, Retirement, Runtime, SystemDuty, is_false,
};
use crate::model::harness::Harness;
use crate::model::ids::{AppId, MemberId, OpId, SeatId, TeamspaceId, TemplateId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatRecord {
    pub schema: u32,
    pub id: SeatId,
    pub rev: u64,
    pub name: String,
    #[serde(default)]
    pub name_history: Vec<NameChange>,
    pub teamspace: TeamspaceId,
    pub lifecycle: Lifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired: Option<Retirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "role")]
    pub system_duty: Option<SystemDuty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_ref: Option<TemplateRef>,
    #[serde(default)]
    pub applications: Vec<AppId>,
    #[serde(default)]
    pub overrides: SeatOverrides,
    #[serde(default)]
    pub participation: Participation,
    #[serde(default)]
    pub activation: Activation,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default)]
    pub channel: Channel,
    #[serde(default, skip_serializing_if = "is_false")]
    pub reload_required: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub moved_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateRef {
    pub template: TemplateId,
    pub member: MemberId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SeatOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<Harness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// The seat `summaries` config key (spec §2.4, §8.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summaries: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub instructions_sections: Vec<InstructionSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionSection {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Participation {
    /// Thread refs.
    #[serde(default)]
    pub seat_wide: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Activation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_op: Option<OpId>,
}
