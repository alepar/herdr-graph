//! Uniform id/rev access over every TOML record type, and TOML helpers.
use super::tree::TreeRead;
use crate::model::action::ActionRecord;
use crate::model::application::ApplicationRecord;
use crate::model::clone::CloneRecord;
use crate::model::operation::OperationRecord;
use crate::model::request::ProcessingRequest;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::template::TemplateRecord;
use crate::model::transcript::TranscriptRecord;
use crate::model::AnyId;
use crate::ports::store::{RepoPath, StoreError};

pub trait Record: serde::Serialize + serde::de::DeserializeOwned {
    fn any_id(&self) -> AnyId;
    fn rev(&self) -> u64;
    fn set_rev(&mut self, rev: u64);
}

macro_rules! impl_record {
    ($($t:ty),* $(,)?) => {$(
        impl Record for $t {
            fn any_id(&self) -> AnyId { self.id.to_any() }
            fn rev(&self) -> u64 { self.rev }
            fn set_rev(&mut self, rev: u64) { self.rev = rev; }
        }
    )*};
}
impl_record!(
    TeamspaceRecord,
    SeatRecord,
    CloneRecord,
    TemplateRecord,
    ApplicationRecord,
    TranscriptRecord,
    ProcessingRequest,
    ActionRecord,
    OperationRecord,
);

pub fn to_toml_bytes<R: serde::Serialize>(r: &R) -> Result<Vec<u8>, StoreError> {
    toml::to_string(r).map(String::into_bytes).map_err(|e| StoreError::Corrupt {
        path: String::new(),
        reason: format!("serialize: {e}"),
    })
}

pub(crate) fn parse_toml<R: serde::de::DeserializeOwned>(path: &RepoPath, bytes: &[u8]) -> Result<R, StoreError> {
    let corrupt = |reason: String| StoreError::Corrupt { path: path.as_str().into(), reason };
    let text = std::str::from_utf8(bytes).map_err(|e| corrupt(e.to_string()))?;
    toml::from_str(text).map_err(|e| corrupt(e.to_string()))
}

/// Read and parse a TOML record through any tree view; None if the file is absent.
pub fn read_toml<R: serde::de::DeserializeOwned>(
    tr: &dyn TreeRead,
    path: &RepoPath,
) -> Result<Option<R>, StoreError> {
    match tr.read_file(path)? {
        None => Ok(None),
        Some(b) => parse_toml(path, &b).map(Some),
    }
}
