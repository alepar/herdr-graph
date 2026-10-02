//! Processing request `requests/<rq-id>.toml` (spec §8).
use crate::model::common::{ByteRange, Timestamp};
use crate::model::ids::{AnyId, OpId, RequestId, TranscriptId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessingRequest {
    pub schema: u32,
    pub id: RequestId,
    pub rev: u64,
    pub transcript: TranscriptId,
    pub range: ByteRange,
    pub status: RequestStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undeliverable: Option<String>,
    pub created_by_op: OpId,
    #[serde(default)]
    pub delivery: Delivery,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<RequestResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Pending,
    Delivered,
    Dispatched,
    Completed,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Delivery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched_at: Option<Timestamp>,
    #[serde(default)]
    pub attempts: Vec<DeliveryAttempt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminded_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    pub op_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub at: Timestamp,
    pub retry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestResult {
    pub output_ref: String,
    pub covered_range: ByteRange,
    pub reported_by: AnyId,
    pub at: Timestamp,
}
