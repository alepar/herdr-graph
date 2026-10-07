//! Action-record envelope `mutations/undoable-actions/<yyyy-mm>/<act-id>.toml`, shared by observer/templates/plan
//! producers and undo (spec §6).
use crate::model::common::Timestamp;
use crate::model::ids::{ActionId, AnyId, OpId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRecord {
    pub schema: u32,
    pub id: ActionId,
    pub rev: u64,
    pub kind: ActionKind,
    pub at: Timestamp,
    #[serde(default)]
    pub ops: Vec<OpId>,
    #[serde(default)]
    pub affected: Vec<AffectedObject>,
    /// Objects this action retired.
    #[serde(default)]
    pub retired: Vec<AnyId>,
    /// Objects that were already retired when the action ran.
    #[serde(default)]
    pub already_retired: Vec<AnyId>,
    /// Compensation data used by undo.
    #[serde(default)]
    pub compensation: toml::Table,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undoes: Option<ActionId>,
    #[serde(default)]
    pub undone_by: Vec<OpId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    ClosureCascade,
    Retire,
    Resurrect,
    Hydrate,
    TemplateEdit,
    ApplicationRetire,
    Undo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AffectedObject {
    pub object: AnyId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<toml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<toml::Value>,
}
