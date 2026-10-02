//! Native session record (`ns_`), embedded in `clone.sessions` (spec §2.4, §8.1).
use crate::model::common::Timestamp;
use crate::model::harness::Harness;
use crate::model::ids::{NsId, TranscriptId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSession {
    pub id: NsId,
    pub harness: Harness,
    pub native_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<TranscriptId>,
    /// Recorded cwd of the session.
    pub cwd: PathBuf,
    pub started: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<SessionEndReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndReason {
    AgentExited,
    PaneClosed,
    SessionChanged,
    Replaced,
    Unknown,
}
