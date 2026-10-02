//! Template record (spec §5, §2.4).
use crate::model::common::{NameChange, Role, Timestamp};
use crate::model::harness::Harness;
use crate::model::ids::{MemberId, TemplateId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateRecord {
    pub schema: u32,
    pub id: TemplateId,
    pub rev: u64,
    pub name: String,
    #[serde(default)]
    pub name_history: Vec<NameChange>,
    #[serde(default)]
    pub defaults: MemberDefaults,
    #[serde(default)]
    pub members: Vec<TemplateMember>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copied_from: Option<CopiedFrom>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MemberDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<Harness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summaries: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateMember {
    pub id: MemberId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub startup: Startup,
    #[serde(default)]
    pub defaults: MemberDefaults,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Startup {
    Active,
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub kind: RelationshipKind,
    pub thread: String,
    pub members: MemberSelector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    ThreadParticipation,
}

/// TOML: `members = "all"` or `members = { ids = [...] }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberSelector {
    All,
    Ids(Vec<MemberId>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopiedFrom {
    pub template: TemplateId,
    pub at: Timestamp,
}
