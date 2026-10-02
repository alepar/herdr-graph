//! Serialized writer port (spec §3.4–3.5). Implemented by hg-zmi.3.
use crate::model::OpId;
use crate::model::change::ChangeRequest;
use crate::model::operation::OpState;

#[derive(Debug, thiserror::Error)]
pub enum WriterError {
    #[error("request invalid: {0}")]
    Invalid(String),
    #[error("writer halted: {0}")]
    Halted(String),
    #[error("journal: {0}")]
    Journal(String),
}

pub trait Writer: Send + Sync {
    /// Durable admission (SQLite insert, WAL + synchronous=FULL). Best-effort validation only;
    /// the writer rechecks preconditions when applying.
    fn admit(&self, request: ChangeRequest) -> Result<OpId, WriterError>;
    /// Current state of an op; None if unknown.
    fn status(&self, op: &OpId) -> Result<Option<OpState>, WriterError>;
}
