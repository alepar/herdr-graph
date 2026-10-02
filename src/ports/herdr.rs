//! Herdr runtime port (spec §4.1). NDJSON client implemented by hg-zmi.5; FakeHerdr there too.
use crate::model::harness::StartOutcome;
use crate::model::{HerdrPaneId, HerdrTabId, HerdrTerminalId, HerdrWorkspaceId, Incarnation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum AgentSession {
    Id(String),
    Path(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    pub kind: String,
    pub status: AgentStatus,
    pub session: Option<AgentSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneInfo {
    pub id: HerdrPaneId,
    pub terminal_id: Option<HerdrTerminalId>,
    pub label: Option<String>,
    pub cwd: Option<PathBuf>,
    /// Pane metadata reported via `pane.report_metadata` (graph token lives here).
    pub metadata: BTreeMap<String, String>,
    pub agent: Option<AgentInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: HerdrTabId,
    pub label: String,
    pub panes: Vec<PaneInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: HerdrWorkspaceId,
    pub label: String,
    pub metadata: BTreeMap<String, String>,
    pub tabs: Vec<TabInfo>,
}

/// One complete `session.snapshot`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HerdrSnapshot {
    pub incarnation: Incarnation,
    pub workspaces: Vec<WorkspaceInfo>,
}

/// Herdr events only trigger snapshots (spec §4.3.1), so they stay opaque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HerdrEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

pub type HerdrEventStream = tokio::sync::mpsc::Receiver<HerdrEvent>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateWorkspace {
    pub label: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTab {
    pub workspace: HerdrWorkspaceId,
    pub label: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Right,
    Down,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPane {
    pub target: HerdrPaneId,
    pub direction: SplitDirection,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub workspace: Option<HerdrWorkspaceId>,
    pub tab: Option<HerdrTabId>,
    pub pane: Option<HerdrPaneId>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartAgent {
    pub pane: HerdrPaneId,
    pub kind: String,
    pub args: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub foreground_pid: Option<u32>,
    pub foreground_argv: Vec<String>,
    pub is_shell: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyInput {
    Key(String),
    Text(String),
}

#[derive(Debug, thiserror::Error)]
pub enum HerdrError {
    #[error("herdr socket unavailable: {0}")]
    Unavailable(String),
    #[error("herdr request timed out")]
    Timeout,
    #[error("herdr rejected {method}: {message}")]
    Rejected { method: String, message: String },
    #[error("herdr protocol: {0}")]
    Protocol(String),
}

#[async_trait::async_trait]
pub trait HerdrApi: Send + Sync {
    async fn snapshot(&self) -> Result<HerdrSnapshot, HerdrError>;
    /// `events.subscribe`; each new subscription bumps the incarnation generation.
    async fn subscribe(&self) -> Result<HerdrEventStream, HerdrError>;
    async fn create_workspace(&self, req: CreateWorkspace) -> Result<Created, HerdrError>;
    async fn rename_workspace(&self, id: &HerdrWorkspaceId, label: &str) -> Result<(), HerdrError>;
    async fn close_workspace(&self, id: &HerdrWorkspaceId) -> Result<(), HerdrError>;
    async fn create_tab(&self, req: CreateTab) -> Result<Created, HerdrError>;
    async fn rename_tab(&self, id: &HerdrTabId, label: &str) -> Result<(), HerdrError>;
    async fn close_tab(&self, id: &HerdrTabId) -> Result<(), HerdrError>;
    async fn split_pane(&self, req: SplitPane) -> Result<Created, HerdrError>;
    /// Pane label rename, if Herdr supports it (verify against `herdr api schema --json`).
    async fn rename_pane(&self, id: &HerdrPaneId, label: &str) -> Result<(), HerdrError>;
    async fn close_pane(&self, id: &HerdrPaneId) -> Result<(), HerdrError>;
    /// Stamp graph token: `pane.report_metadata`.
    async fn report_pane_metadata(&self, id: &HerdrPaneId, key: &str, value: &str) -> Result<(), HerdrError>;
    /// Stamp graph token: `workspace.report_metadata`.
    async fn report_workspace_metadata(
        &self,
        id: &HerdrWorkspaceId,
        key: &str,
        value: &str,
    ) -> Result<(), HerdrError>;
    /// `agent.start`; preconditions per spec §4.1 are checked by the caller.
    async fn start_agent(&self, req: StartAgent) -> Result<StartOutcome, HerdrError>;
    /// `agent.get`.
    async fn agent(&self, pane: &HerdrPaneId) -> Result<Option<AgentInfo>, HerdrError>;
    /// `pane.process_info`.
    async fn process_info(&self, pane: &HerdrPaneId) -> Result<ProcessInfo, HerdrError>;
    /// `agent.send_keys`.
    async fn send_keys(&self, pane: &HerdrPaneId, keys: &[KeyInput]) -> Result<(), HerdrError>;
}
