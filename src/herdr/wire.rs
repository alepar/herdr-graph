//! Herdr 0.9.1 NDJSON wire format (protocol 22), verified against `herdr api schema --json`.
//!
//! One request is one JSON line `{"id","method","params"}`; the reply is one line carrying the same
//! `id` and either `result` or `error{code,message}`. A subscription reply is followed by event
//! lines `{"event": "<kind>", "data": {...}}`. Event kinds use underscores (`tab_created`) while the
//! subscription request uses dots (`tab.created`).
use crate::model::{HerdrPaneId, HerdrTabId, HerdrTerminalId, HerdrWorkspaceId, Incarnation};
use crate::ports::herdr::{
    AgentInfo, AgentSession, AgentStatus, HerdrError, HerdrSnapshot, PaneInfo, ProcessInfo,
    TabInfo, WorkspaceInfo,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const M_SNAPSHOT: &str = "session.snapshot";
pub const M_SUBSCRIBE: &str = "events.subscribe";
pub const M_WORKSPACE_CREATE: &str = "workspace.create";
pub const M_WORKSPACE_RENAME: &str = "workspace.rename";
pub const M_WORKSPACE_CLOSE: &str = "workspace.close";
pub const M_WORKSPACE_REPORT_METADATA: &str = "workspace.report_metadata";
pub const M_TAB_CREATE: &str = "tab.create";
pub const M_TAB_RENAME: &str = "tab.rename";
pub const M_TAB_CLOSE: &str = "tab.close";
pub const M_PANE_SPLIT: &str = "pane.split";
pub const M_PANE_CLOSE: &str = "pane.close";
pub const M_PANE_RENAME: &str = "pane.rename";
pub const M_PANE_PROCESS_INFO: &str = "pane.process_info";
pub const M_PANE_GET: &str = "pane.get";
pub const M_PANE_CURRENT: &str = "pane.current";
pub const M_PANE_REPORT_METADATA: &str = "pane.report_metadata";
pub const M_PANE_SEND_TEXT: &str = "pane.send_text";
pub const M_PANE_SEND_KEYS: &str = "pane.send_keys";
pub const M_AGENT_START: &str = "agent.start";
pub const M_AGENT_GET: &str = "agent.get";
pub const M_AGENT_SEND_KEYS: &str = "agent.send_keys";

/// Metadata `source` field every graph report carries (Herdr scopes tokens per source).
pub const METADATA_SOURCE: &str = "herdr-graph";

/// Subscription topics that carry no parameters (dotted names, from the schema's `Subscription`).
/// `pane.agent_status_changed` needs a `pane_id` (no wildcard), so the client adds one per pane.
pub const GLOBAL_TOPICS: &[&str] = &[
    "workspace.created",
    "workspace.renamed",
    "workspace.closed",
    "workspace.metadata_updated",
    "tab.created",
    "tab.renamed",
    "tab.closed",
    "tab.moved",
    "pane.created",
    "pane.closed",
    "pane.moved",
    "pane.exited",
    "pane.updated",
    "pane.agent_detected",
];
pub const TOPIC_AGENT_STATUS: &str = "pane.agent_status_changed";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WireRequest {
    pub id: String,
    pub method: &'static str,
    pub params: Value,
}

impl WireRequest {
    /// One NDJSON line, newline included.
    pub fn to_line(&self) -> Result<Vec<u8>, HerdrError> {
        let mut line = serde_json::to_vec(self)
            .map_err(|e| HerdrError::Protocol(format!("encode request: {e}")))?;
        line.push(b'\n');
        Ok(line)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WireError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WireResponse {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<WireError>,
}

/// An event line of a subscription connection.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WireEvent {
    pub event: String,
    #[serde(default)]
    pub data: Value,
}

/// A line of a subscription connection is either an event or a response (the handshake ack/error).
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Event(WireEvent),
    Response(WireResponse),
}

pub fn parse_frame(line: &str) -> Result<Frame, HerdrError> {
    let v: Value = serde_json::from_str(line)
        .map_err(|e| HerdrError::Protocol(format!("bad json line: {e}")))?;
    if v.get("event").is_some() {
        serde_json::from_value(v)
            .map(Frame::Event)
            .map_err(|e| HerdrError::Protocol(format!("bad event: {e}")))
    } else {
        serde_json::from_value(v)
            .map(Frame::Response)
            .map_err(|e| HerdrError::Protocol(format!("bad response: {e}")))
    }
}

/// `error` member to the port error. The Herdr error code is kept as the message prefix
/// (`"<code>: <message>"`) so callers can still branch on it (`agent_not_ready`, `agent_not_found`).
pub fn error_to_rejected(method: &str, e: &WireError) -> HerdrError {
    HerdrError::Rejected {
        method: method.to_owned(),
        message: format!("{}: {}", e.code, e.message),
    }
}

/// `true` when a [`HerdrError::Rejected`] carries the given Herdr error code.
pub fn is_code(err: &HerdrError, code: &str) -> bool {
    matches!(err, HerdrError::Rejected { message, .. } if message.starts_with(&format!("{code}:")))
}

fn proto(msg: impl Into<String>) -> HerdrError {
    HerdrError::Protocol(msg.into())
}

fn str_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, HerdrError> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| proto(format!("missing string field `{key}`")))
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn tokens(v: &Value) -> BTreeMap<String, String> {
    v.get("tokens")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_status(s: Option<&str>) -> AgentStatus {
    match s {
        Some("idle") => AgentStatus::Idle,
        Some("working") => AgentStatus::Working,
        Some("blocked") => AgentStatus::Blocked,
        Some("done") => AgentStatus::Done,
        _ => AgentStatus::Unknown,
    }
}

fn parse_session(v: &Value) -> Option<AgentSession> {
    let s = v.get("agent_session").filter(|s| !s.is_null())?;
    let value = s.get("value")?.as_str()?;
    match s.get("kind")?.as_str()? {
        "id" => Some(AgentSession::Id(value.to_owned())),
        "path" => Some(AgentSession::Path(PathBuf::from(value))),
        _ => None,
    }
}

/// Agent info from a Herdr `AgentInfo` or `PaneInfo` object (both carry `agent`, `agent_status`,
/// `agent_session`). `None` when no agent kind is reported.
pub fn parse_agent(v: &Value) -> Option<AgentInfo> {
    let kind = v.get("agent").and_then(Value::as_str)?;
    Some(AgentInfo {
        kind: kind.to_owned(),
        status: parse_status(v.get("agent_status").and_then(Value::as_str)),
        session: parse_session(v),
    })
}

pub fn parse_pane(v: &Value) -> Result<PaneInfo, HerdrError> {
    Ok(PaneInfo {
        id: HerdrPaneId(str_field(v, "pane_id")?.to_owned()),
        terminal_id: opt_str(v, "terminal_id").map(HerdrTerminalId),
        label: opt_str(v, "label"),
        cwd: opt_str(v, "cwd").map(PathBuf::from),
        metadata: tokens(v),
        agent: parse_agent(v),
    })
}

/// Map the `result` of `session.snapshot` (`{type:"session_snapshot", snapshot:{...}}`, or the bare
/// snapshot object) into the port types. Herdr's snapshot is flat (workspaces, tabs, panes, agents);
/// the port's is nested. The incarnation is left at its default for the client to stamp.
pub fn parse_snapshot(v: &Value) -> Result<HerdrSnapshot, HerdrError> {
    let snap = v.get("snapshot").unwrap_or(v);
    let list = |key: &str| -> Result<&Vec<Value>, HerdrError> {
        snap.get(key)
            .and_then(Value::as_array)
            .ok_or_else(|| proto(format!("snapshot lacks `{key}` array")))
    };
    let agents = list("agents")?;
    let mut panes_by_tab: BTreeMap<String, Vec<PaneInfo>> = BTreeMap::new();
    for p in list("panes")? {
        let mut pane = parse_pane(p)?;
        // The dedicated agents list is authoritative when it names the pane.
        let id = pane.id.0.clone();
        if let Some(a) = agents
            .iter()
            .find(|a| a.get("pane_id").and_then(Value::as_str) == Some(id.as_str()))
            && let Some(info) = parse_agent(a)
        {
            pane.agent = Some(info);
        }
        panes_by_tab
            .entry(str_field(p, "tab_id")?.to_owned())
            .or_default()
            .push(pane);
    }
    let mut tabs_by_ws: BTreeMap<String, Vec<TabInfo>> = BTreeMap::new();
    for t in list("tabs")? {
        let id = str_field(t, "tab_id")?.to_owned();
        let tab = TabInfo {
            id: HerdrTabId(id.clone()),
            label: str_field(t, "label")?.to_owned(),
            panes: panes_by_tab.remove(&id).unwrap_or_default(),
        };
        tabs_by_ws
            .entry(str_field(t, "workspace_id")?.to_owned())
            .or_default()
            .push(tab);
    }
    let mut workspaces = Vec::new();
    for w in list("workspaces")? {
        let id = str_field(w, "workspace_id")?.to_owned();
        workspaces.push(WorkspaceInfo {
            id: HerdrWorkspaceId(id.clone()),
            label: str_field(w, "label")?.to_owned(),
            metadata: tokens(w),
            tabs: tabs_by_ws.remove(&id).unwrap_or_default(),
        });
    }
    Ok(HerdrSnapshot {
        incarnation: Incarnation::default(),
        workspaces,
    })
}

/// `pane.process_info` result (`{type, process_info:{...}}`) to the port type. `is_shell` is true when
/// the foreground process group is the shell's own (or nothing else is in the foreground).
pub fn parse_process_info(v: &Value) -> Result<ProcessInfo, HerdrError> {
    let p = v.get("process_info").unwrap_or(v);
    let pgid = p
        .get("foreground_process_group_id")
        .and_then(Value::as_u64)
        .map(|n| n as u32);
    let shell = p.get("shell_pid").and_then(Value::as_u64).map(|n| n as u32);
    let procs = p
        .get("foreground_processes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let lead = procs
        .iter()
        .find(|q| pgid.is_some() && q.get("pid").and_then(Value::as_u64).map(|n| n as u32) == pgid)
        .or_else(|| procs.first());
    let foreground_pid = lead
        .and_then(|q| q.get("pid").and_then(Value::as_u64))
        .map(|n| n as u32)
        .or(pgid);
    let foreground_argv = lead
        .and_then(|q| q.get("argv").and_then(Value::as_array))
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let is_shell = match (pgid, shell) {
        (Some(g), Some(s)) => g == s,
        _ => procs.is_empty(),
    };
    Ok(ProcessInfo {
        foreground_pid,
        foreground_argv,
        is_shell,
    })
}
