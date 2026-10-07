//! `graph.toml`: instance metadata (no `rev`).
use crate::model::harness::Harness;
use crate::model::ids::SeatId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphRecord {
    pub schema: u32,
    pub instance_id: String,
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summarizer_seat: Option<SeatId>,
    #[serde(default)]
    pub defaults: GraphDefaults,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GraphDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<Harness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summaries: Option<bool>,
}
