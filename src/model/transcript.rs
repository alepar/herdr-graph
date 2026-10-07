//! Native transcript index; new records use `transcripts/<team-name>-<team-id>/<seat-name>-<seat-id>/<tr-id>.toml`.
//! Legacy indexes retain their original paths; native conversation files are never copied.
use crate::model::common::{ByteRange, is_false};
use crate::model::ids::{CloneId, NsId, SeatId, TeamspaceId, TranscriptId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptRecord {
    pub schema: u32,
    pub id: TranscriptId,
    pub rev: u64,
    pub transcript_path: PathBuf,
    pub native_session: NsId,
    pub seat: SeatId,
    pub clone: CloneId,
    /// Organization at initial registration. Absent for legacy records: history is unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_attribution: Option<CaptureAttribution>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub source_seat_summaries_enabled_at_capture: bool,
    /// Covered byte ranges `[start, end)`.
    #[serde(default)]
    pub coverage: Vec<ByteRange>,
    /// Unrecoverable gaps `[start, end)`.
    #[serde(default)]
    pub gaps: Vec<ByteRange>,
    /// Reason the transcript could not be resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<String>,
}

/// Immutable display names and team identity captured when the index is first registered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureAttribution {
    pub teamspace: TeamspaceId,
    pub teamspace_name: String,
    pub seat_name: String,
}
