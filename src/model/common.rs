//! Shared value types: lifecycle, runtime/binding, occupancy, retirement, names, roles.
use crate::model::ids::{ActionId, NsId, OpId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type Timestamp = chrono::DateTime<chrono::Utc>;
pub const SCHEMA_VERSION: u32 = 1;
pub fn is_false(b: &bool) -> bool {
    !*b
}

macro_rules! herdr_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
        }
    };
}
herdr_id!(
    /// Herdr workspace id (Herdr-native, changes across incarnations).
    HerdrWorkspaceId
);
herdr_id!(
    /// Herdr tab id.
    HerdrTabId
);
herdr_id!(
    /// Herdr pane id.
    HerdrPaneId
);
herdr_id!(
    /// Herdr terminal id (survives live handoff).
    HerdrTerminalId
);

/// Git commit oid (hex) of a committed graph revision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommitId(pub String);
/// Git blob hash (hex) of an opaque file; used as a precondition for files without `rev`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlobHash(pub String);

/// Teamspace and seat lifecycle (spec §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    #[default]
    Dormant,
    Active,
    Retired,
}

/// Clone lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloneLifecycle {
    #[default]
    Active,
    Retired,
}

/// Application lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AppLifecycle {
    #[default]
    Active,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Present,
    Absent,
    #[default]
    Unknown,
}

/// Herdr connection incarnation (spec §4.3.1): diffs run only within one incarnation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Incarnation {
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_started: Option<String>,
}

/// Binding of a graph object to live Herdr objects (spec §4.2). Tabs carry no token (Herdr 0.9.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Binding {
    /// Graph-owned token `hg=<id>` (see `launch::graph_token`); None for seat tabs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<HerdrWorkspaceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<HerdrTabId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<HerdrPaneId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<HerdrTerminalId>,
    pub incarnation: Incarnation,
}

/// Observed runtime state; never changes lifecycle by itself (spec §2.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Runtime {
    pub availability: Availability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound: Option<Binding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<Timestamp>,
}

/// Current occupant of a clone (spec §2.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occupant {
    pub native_session: NsId,
    pub harness: crate::model::harness::Harness,
    pub since: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetireMechanism {
    UserCli,
    AgentRequest,
    ObservedPaneClose,
    ObservedTabClose,
    ObservedWorkspaceClose,
    ApplicationWithdrawal,
    Undo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retirement {
    pub op: OpId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ActionId>,
    pub at: Timestamp,
    pub mechanism: RetireMechanism,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameSource {
    Observed,
    Request,
    Undo,
}

/// One rename (spec §2.1): observed time always; event time only when Herdr supplied one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameChange {
    pub old: String,
    pub new: String,
    pub observed_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_at: Option<Timestamp>,
    pub source: NameSource,
}

/// SystemDuty marker on seats / template members. Absent = ordinary seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemDuty {
    Summarizer,
    System,
    Cron,
    Dispatcher,
}

/// Default for the `summaries` seat config key: ordinary seats true; every system_duty marker false (spec §2.4, §8.2).
pub fn default_summaries(system_duty: Option<SystemDuty>) -> bool {
    system_duty.is_none()
}

/// Managed threads channel reference stored on teamspace and seat records (spec §7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Channel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

/// Half-open byte range `[start, end)` into a JSONL transcript; `end` is newline-aligned (spec §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}
impl ByteRange {
    pub fn new(start: u64, end: u64) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }
    pub fn len(&self) -> u64 {
        self.end - self.start
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Path relative to an object's folder, as used by `content write --rel` (validated by hg-zmi.13).
pub type RelPath = PathBuf;
