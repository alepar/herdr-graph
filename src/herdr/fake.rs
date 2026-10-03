//! In-memory Herdr for unit tests of the other beads: implements [`HerdrApi`], records calls, injects
//! faults, and scripts the "user" side (closing, renaming, moving panes), emitting the matching events.
use crate::model::harness::StartOutcome;
use crate::model::{HerdrPaneId, HerdrTabId, HerdrTerminalId, HerdrWorkspaceId, Incarnation};
use crate::ports::herdr::*;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum FakeCall {
    CreateWorkspace(CreateWorkspace),
    CreateTab(CreateTab),
    SplitPane(SplitPane),
    RenameWorkspace(HerdrWorkspaceId, String),
    RenameTab(HerdrTabId, String),
    RenamePane(HerdrPaneId, String),
    CloseWorkspace(HerdrWorkspaceId),
    CloseTab(HerdrTabId),
    ClosePane(HerdrPaneId),
    ReportPaneMetadata(HerdrPaneId, String, String),
    ReportWorkspaceMetadata(HerdrWorkspaceId, String, String),
    StartAgent(StartAgent),
    SendKeys(HerdrPaneId, Vec<KeyInput>),
}

#[derive(Debug, Clone)]
pub enum Fault {
    Unavailable,
    Timeout,
    Rejected(String),
    /// Perform the call but report `Timeout` (lost response).
    LostResponse,
    /// Perform the call (state change and events), then never return: the caller's future has to be cancelled,
    /// as the daemon's first-pass timeout does. Honoured by every mutating call and `agent.start`.
    Hang,
    /// `start_agent` only: return this outcome without starting anything. Ignored by other methods.
    StartOutcome(StartOutcome),
}

#[derive(Default)]
struct FakeState {
    next_ws: u64,
    next_tab: u64,
    next_pane: u64,
    next_term: u64,
    /// Bumped by every subscribe and every restart.
    generation: u64,
    restarts: u64,
    workspaces: Vec<WorkspaceInfo>,
    envs: BTreeMap<HerdrPaneId, Vec<(String, String)>>,
    processes: HashMap<HerdrPaneId, ProcessInfo>,
    calls: Vec<FakeCall>,
    faults: HashMap<String, VecDeque<Fault>>,
    last_tab_closes_workspace: bool,
}

pub struct FakeHerdr {
    state: Mutex<FakeState>,
    events: Mutex<Vec<mpsc::Sender<HerdrEvent>>>,
}

fn shell() -> ProcessInfo {
    ProcessInfo {
        foreground_pid: None,
        foreground_argv: Vec::new(),
        is_shell: true,
    }
}

fn rejected(method: &str, msg: impl Into<String>) -> HerdrError {
    HerdrError::Rejected {
        method: method.to_owned(),
        message: msg.into(),
    }
}

type Events = Vec<(&'static str, serde_json::Value)>;

impl FakeState {
    fn pane_id(&mut self) -> HerdrPaneId {
        self.next_pane += 1;
        HerdrPaneId(format!("p{}", self.next_pane))
    }
    fn tab_id(&mut self) -> HerdrTabId {
        self.next_tab += 1;
        HerdrTabId(format!("t{}", self.next_tab))
    }
    fn ws_id(&mut self) -> HerdrWorkspaceId {
        self.next_ws += 1;
        HerdrWorkspaceId(format!("w{}", self.next_ws))
    }
    fn term_id(&mut self) -> HerdrTerminalId {
        self.next_term += 1;
        HerdrTerminalId(format!("term{}", self.next_term))
    }
    fn new_pane(
        &mut self,
        label: Option<String>,
        cwd: PathBuf,
        env: Vec<(String, String)>,
    ) -> PaneInfo {
        let id = self.pane_id();
        let terminal_id = Some(self.term_id());
        self.envs.insert(id.clone(), env);
        self.processes.insert(id.clone(), shell());
        PaneInfo {
            id,
            terminal_id,
            label,
            cwd: Some(cwd),
            metadata: BTreeMap::new(),
            agent: None,
        }
    }
    fn incarnation(&self) -> Incarnation {
        Incarnation {
            generation: self.generation,
            server_pid: Some(10_000 + self.restarts as u32),
            server_started: Some(format!("fake-start-{}", self.restarts)),
        }
    }
    fn find_pane(&mut self, id: &HerdrPaneId) -> Option<&mut PaneInfo> {
        self.workspaces
            .iter_mut()
            .flat_map(|w| w.tabs.iter_mut())
            .flat_map(|t| t.panes.iter_mut())
            .find(|p| &p.id == id)
    }
    fn find_ws(&mut self, id: &HerdrWorkspaceId) -> Option<&mut WorkspaceInfo> {
        self.workspaces.iter_mut().find(|w| &w.id == id)
    }
    fn find_tab(&mut self, id: &HerdrTabId) -> Option<&mut TabInfo> {
        self.workspaces
            .iter_mut()
            .flat_map(|w| w.tabs.iter_mut())
            .find(|t| &t.id == id)
    }
    fn ws_of_tab(&self, id: &HerdrTabId) -> Option<HerdrWorkspaceId> {
        self.workspaces
            .iter()
            .find(|w| w.tabs.iter().any(|t| &t.id == id))
            .map(|w| w.id.clone())
    }
    fn tab_of_pane(&self, id: &HerdrPaneId) -> Option<HerdrTabId> {
        self.workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .find(|t| t.panes.iter().any(|p| &p.id == id))
            .map(|t| t.id.clone())
    }

    fn drop_pane_state(&mut self, id: &HerdrPaneId) {
        self.envs.remove(id);
        self.processes.remove(id);
    }

    /// Remove a workspace and everything in it; events: pane_closed per pane, tab_closed per tab, workspace_closed.
    fn remove_ws(&mut self, id: &HerdrWorkspaceId, ev: &mut Events) {
        let Some(i) = self.workspaces.iter().position(|w| &w.id == id) else {
            return;
        };
        let ws = self.workspaces.remove(i);
        for t in &ws.tabs {
            for p in &t.panes {
                self.drop_pane_state(&p.id);
                ev.push((
                    "pane_closed",
                    json!({ "pane_id": p.id, "tab_id": t.id, "workspace_id": ws.id }),
                ));
            }
            ev.push((
                "tab_closed",
                json!({ "tab_id": t.id, "workspace_id": ws.id }),
            ));
        }
        ev.push(("workspace_closed", json!({ "workspace_id": ws.id })));
    }

    /// Remove a tab; closes the workspace too when it was the last tab and that policy is on.
    fn remove_tab(&mut self, id: &HerdrTabId, ev: &mut Events) {
        let Some(ws_id) = self.ws_of_tab(id) else {
            return;
        };
        let policy = self.last_tab_closes_workspace;
        let ws = self.find_ws(&ws_id).expect("workspace of tab");
        let i = ws
            .tabs
            .iter()
            .position(|t| &t.id == id)
            .expect("tab in workspace");
        let tab = ws.tabs.remove(i);
        let now_empty = ws.tabs.is_empty();
        for p in &tab.panes {
            self.drop_pane_state(&p.id);
            ev.push((
                "pane_closed",
                json!({ "pane_id": p.id, "tab_id": tab.id, "workspace_id": ws_id }),
            ));
        }
        ev.push((
            "tab_closed",
            json!({ "tab_id": tab.id, "workspace_id": ws_id }),
        ));
        if now_empty && policy {
            self.remove_ws(&ws_id, ev);
        }
    }

    /// Remove a pane; the tab goes with its last pane (and the workspace with its last tab by policy).
    fn remove_pane(&mut self, id: &HerdrPaneId, ev: &mut Events) {
        let Some(tab_id) = self.tab_of_pane(id) else {
            return;
        };
        let ws_id = self.ws_of_tab(&tab_id).expect("workspace of tab");
        let tab = self.find_tab(&tab_id).expect("tab of pane");
        tab.panes.retain(|p| &p.id != id);
        let empty = tab.panes.is_empty();
        self.drop_pane_state(id);
        ev.push((
            "pane_closed",
            json!({ "pane_id": id, "tab_id": tab_id, "workspace_id": ws_id }),
        ));
        if empty {
            self.remove_tab(&tab_id, ev);
        }
    }
}

impl FakeHerdr {
    pub fn new() -> Arc<Self> {
        let state = FakeState {
            last_tab_closes_workspace: true,
            ..FakeState::default()
        };
        Arc::new(Self {
            state: Mutex::new(state),
            events: Mutex::new(Vec::new()),
        })
    }

    /// Every call attempted so far, in order (faulted ones included).
    pub fn calls(&self) -> Vec<FakeCall> {
        self.state.lock().unwrap().calls.clone()
    }

    pub fn clear_calls(&self) {
        self.state.lock().unwrap().calls.clear();
    }

    /// Queue a fault for the next call of `method` (wire method name, e.g. "tab.create").
    pub fn fail_next(&self, method: &str, fault: Fault) {
        self.state
            .lock()
            .unwrap()
            .faults
            .entry(method.to_owned())
            .or_default()
            .push_back(fault);
    }

    /// Env a pane was created with (not part of the snapshot).
    pub fn pane_env(&self, pane: &HerdrPaneId) -> Option<Vec<(String, String)>> {
        self.state.lock().unwrap().envs.get(pane).cloned()
    }

    fn emit(&self, events: Events) {
        let mut subs = self.events.lock().unwrap();
        for (name, payload) in events {
            let ev = HerdrEvent {
                name: name.to_owned(),
                payload,
            };
            subs.retain(|tx| match tx.try_send(ev.clone()) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => true,
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            });
        }
    }

    /// Record the call, then apply a queued fault. `Ok(None)` = run normally; `Ok(Some(f))` = run, then
    /// fail with `f` afterwards (lost response) or short-circuit with a start outcome; `Err` = fail now.
    fn enter(&self, method: &str, call: FakeCall) -> Result<Option<Fault>, HerdrError> {
        let mut st = self.state.lock().unwrap();
        st.calls.push(call);
        let fault = st.faults.get_mut(method).and_then(VecDeque::pop_front);
        match fault {
            None => Ok(None),
            Some(Fault::Unavailable) => Err(HerdrError::Unavailable("fake: unavailable".into())),
            Some(Fault::Timeout) => Err(HerdrError::Timeout),
            Some(Fault::Rejected(m)) => Err(rejected(method, m)),
            Some(f @ (Fault::LostResponse | Fault::Hang | Fault::StartOutcome(_))) => Ok(Some(f)),
        }
    }

    /// Shared tail of every mutating call: run `f` on the state, emit its events, honour a lost response.
    async fn mutate<T>(
        &self,
        method: &str,
        call: FakeCall,
        f: impl FnOnce(&mut FakeState, &mut Events) -> Result<T, HerdrError>,
    ) -> Result<T, HerdrError> {
        let fault = self.enter(method, call)?;
        let mut ev = Vec::new();
        let out = f(&mut self.state.lock().unwrap(), &mut ev)?;
        self.emit(ev);
        if matches!(fault, Some(Fault::Hang)) {
            std::future::pending::<()>().await;
        }
        if matches!(fault, Some(Fault::LostResponse)) {
            return Err(HerdrError::Timeout);
        }
        Ok(out)
    }

    fn scripted(&self, f: impl FnOnce(&mut FakeState, &mut Events)) {
        let mut ev = Vec::new();
        f(&mut self.state.lock().unwrap(), &mut ev);
        self.emit(ev);
    }

    pub fn user_close_pane(&self, p: &HerdrPaneId) {
        self.scripted(|st, ev| st.remove_pane(p, ev));
    }

    pub fn user_close_tab(&self, t: &HerdrTabId) {
        self.scripted(|st, ev| st.remove_tab(t, ev));
    }

    pub fn user_close_workspace(&self, w: &HerdrWorkspaceId) {
        self.scripted(|st, ev| st.remove_ws(w, ev));
    }

    pub fn user_rename_tab(&self, t: &HerdrTabId, label: &str) {
        self.scripted(|st, ev| {
            let ws = st.ws_of_tab(t);
            if let (Some(tab), Some(ws)) = (st.find_tab(t), ws) {
                tab.label = label.to_owned();
                ev.push((
                    "tab_renamed",
                    json!({ "tab_id": t, "workspace_id": ws, "label": label }),
                ));
            }
        });
    }

    pub fn user_rename_workspace(&self, w: &HerdrWorkspaceId, label: &str) {
        self.scripted(|st, ev| {
            if let Some(ws) = st.find_ws(w) {
                ws.label = label.to_owned();
                ev.push((
                    "workspace_renamed",
                    json!({ "workspace_id": w, "label": label }),
                ));
            }
        });
    }

    /// Move a pane to another tab; its old tab closes (and the workspace by policy) when left empty.
    pub fn user_move_pane(&self, p: &HerdrPaneId, to_tab: &HerdrTabId) {
        self.scripted(|st, ev| {
            let (Some(from), Some(_)) = (st.tab_of_pane(p), st.ws_of_tab(to_tab)) else {
                return;
            };
            if &from == to_tab {
                return;
            }
            let from_tab = st.find_tab(&from).expect("source tab");
            let i = from_tab
                .panes
                .iter()
                .position(|q| &q.id == p)
                .expect("pane in source tab");
            let pane = from_tab.panes.remove(i);
            let emptied = from_tab.panes.is_empty();
            st.find_tab(to_tab).expect("target tab").panes.push(pane);
            ev.push((
                "pane_moved",
                json!({ "pane_id": p, "from_tab_id": from, "tab_id": to_tab }),
            ));
            if emptied {
                st.remove_tab(&from, ev);
            }
        });
    }

    /// An agent appears in (`Some`) or leaves (`None`) a pane.
    pub fn set_agent(&self, p: &HerdrPaneId, agent: Option<AgentInfo>) {
        self.scripted(|st, ev| {
            let appeared = agent.is_some();
            if let Some(pane) = st.find_pane(p) {
                pane.agent = agent;
                ev.push((
                    if appeared {
                        "pane_agent_detected"
                    } else {
                        "pane_updated"
                    },
                    json!({ "pane_id": p }),
                ));
            }
        });
    }

    pub fn set_process(&self, p: &HerdrPaneId, info: ProcessInfo) {
        self.state.lock().unwrap().processes.insert(p.clone(), info);
    }

    pub fn add_user_pane(&self, tab: &HerdrTabId, label: &str) -> HerdrPaneId {
        let mut id = None;
        self.scripted(|st, ev| {
            let Some(ws) = st.ws_of_tab(tab) else { return };
            let cwd = PathBuf::from("/");
            let pane = st.new_pane(Some(label.to_owned()), cwd, Vec::new());
            id = Some(pane.id.clone());
            ev.push((
                "pane_created",
                json!({ "pane_id": pane.id, "tab_id": tab, "workspace_id": ws }),
            ));
            st.find_tab(tab).expect("tab").panes.push(pane);
        });
        id.expect("add_user_pane: unknown tab")
    }

    /// Simulate a Herdr restart: new generation and server pid, every pane gets a fresh pane id, terminal
    /// ids are kept or regenerated, pane/workspace metadata is kept or dropped, and subscribers disconnect.
    pub fn restart(&self, keep_metadata: bool, keep_terminal_ids: bool) {
        {
            let mut st = self.state.lock().unwrap();
            st.generation += 1;
            st.restarts += 1;
            let mut workspaces = std::mem::take(&mut st.workspaces);
            let mut envs = std::mem::take(&mut st.envs);
            let mut processes = std::mem::take(&mut st.processes);
            for ws in &mut workspaces {
                if !keep_metadata {
                    ws.metadata.clear();
                }
                for pane in ws.tabs.iter_mut().flat_map(|t| t.panes.iter_mut()) {
                    let old = pane.id.clone();
                    pane.id = st.pane_id();
                    if !keep_metadata {
                        pane.metadata.clear();
                    }
                    if !keep_terminal_ids {
                        pane.terminal_id = Some(st.term_id());
                    }
                    if let Some(e) = envs.remove(&old) {
                        st.envs.insert(pane.id.clone(), e);
                    }
                    if let Some(pr) = processes.remove(&old) {
                        st.processes.insert(pane.id.clone(), pr);
                    }
                }
            }
            st.workspaces = workspaces;
        }
        self.disconnect_subscribers();
    }

    /// Subscribers' streams end (reconnect test).
    pub fn disconnect_subscribers(&self) {
        self.events.lock().unwrap().clear();
    }

    /// Closing the last tab closes the workspace when true (default true).
    pub fn set_last_tab_closes_workspace(&self, v: bool) {
        self.state.lock().unwrap().last_tab_closes_workspace = v;
    }
}

#[async_trait::async_trait]
impl HerdrApi for FakeHerdr {
    async fn snapshot(&self) -> Result<HerdrSnapshot, HerdrError> {
        let mut st = self.state.lock().unwrap();
        if let Some(f) = st
            .faults
            .get_mut("session.snapshot")
            .and_then(VecDeque::pop_front)
        {
            return match f {
                Fault::Unavailable => Err(HerdrError::Unavailable("fake: unavailable".into())),
                Fault::Timeout | Fault::LostResponse | Fault::Hang => Err(HerdrError::Timeout),
                Fault::Rejected(m) => Err(rejected("session.snapshot", m)),
                Fault::StartOutcome(_) => Ok(HerdrSnapshot {
                    incarnation: st.incarnation(),
                    workspaces: st.workspaces.clone(),
                }),
            };
        }
        Ok(HerdrSnapshot {
            incarnation: st.incarnation(),
            workspaces: st.workspaces.clone(),
        })
    }

    async fn subscribe(&self) -> Result<HerdrEventStream, HerdrError> {
        if let Some(Fault::Unavailable) = self
            .state
            .lock()
            .unwrap()
            .faults
            .get_mut("events.subscribe")
            .and_then(VecDeque::pop_front)
        {
            return Err(HerdrError::Unavailable("fake: unavailable".into()));
        }
        self.state.lock().unwrap().generation += 1;
        let (tx, rx) = mpsc::channel(1024);
        self.events.lock().unwrap().push(tx);
        Ok(rx)
    }

    async fn create_workspace(&self, req: CreateWorkspace) -> Result<Created, HerdrError> {
        self.mutate(
            "workspace.create",
            FakeCall::CreateWorkspace(req.clone()),
            |st, ev| {
                let ws_id = st.ws_id();
                let tab_id = st.tab_id();
                let pane = st.new_pane(None, req.cwd.clone(), req.env.clone());
                let pane_id = pane.id.clone();
                let tab = TabInfo {
                    id: tab_id.clone(),
                    label: "1".to_owned(),
                    panes: vec![pane],
                };
                st.workspaces.push(WorkspaceInfo {
                    id: ws_id.clone(),
                    label: req.label.clone(),
                    metadata: BTreeMap::new(),
                    tabs: vec![tab],
                });
                ev.push((
                    "workspace_created",
                    json!({ "workspace_id": ws_id, "label": req.label }),
                ));
                ev.push((
                    "tab_created",
                    json!({ "tab_id": tab_id, "workspace_id": ws_id }),
                ));
                ev.push((
                    "pane_created",
                    json!({ "pane_id": pane_id, "tab_id": tab_id, "workspace_id": ws_id }),
                ));
                Ok(Created {
                    workspace: Some(ws_id),
                    tab: Some(tab_id),
                    pane: Some(pane_id),
                })
            },
        )
        .await
    }

    async fn rename_workspace(&self, id: &HerdrWorkspaceId, label: &str) -> Result<(), HerdrError> {
        self.mutate(
            "workspace.rename",
            FakeCall::RenameWorkspace(id.clone(), label.to_owned()),
            |st, ev| {
                let ws = st.find_ws(id).ok_or_else(|| {
                    rejected("workspace.rename", format!("workspace {id} not found"))
                })?;
                ws.label = label.to_owned();
                ev.push((
                    "workspace_renamed",
                    json!({ "workspace_id": id, "label": label }),
                ));
                Ok(())
            },
        )
        .await
    }

    async fn close_workspace(&self, id: &HerdrWorkspaceId) -> Result<(), HerdrError> {
        self.mutate(
            "workspace.close",
            FakeCall::CloseWorkspace(id.clone()),
            |st, ev| {
                st.find_ws(id).ok_or_else(|| {
                    rejected("workspace.close", format!("workspace {id} not found"))
                })?;
                st.remove_ws(id, ev);
                Ok(())
            },
        )
        .await
    }

    async fn create_tab(&self, req: CreateTab) -> Result<Created, HerdrError> {
        self.mutate("tab.create", FakeCall::CreateTab(req.clone()), |st, ev| {
            let ws = req.workspace.clone();
            st.find_ws(&ws)
                .ok_or_else(|| rejected("tab.create", format!("workspace {ws} not found")))?;
            let tab_id = st.tab_id();
            let pane = st.new_pane(None, req.cwd.clone(), req.env.clone());
            let pane_id = pane.id.clone();
            st.find_ws(&ws).expect("workspace").tabs.push(TabInfo {
                id: tab_id.clone(),
                label: req.label.clone(),
                panes: vec![pane],
            });
            ev.push((
                "tab_created",
                json!({ "tab_id": tab_id, "workspace_id": ws, "label": req.label }),
            ));
            ev.push((
                "pane_created",
                json!({ "pane_id": pane_id, "tab_id": tab_id, "workspace_id": ws }),
            ));
            Ok(Created {
                workspace: Some(ws),
                tab: Some(tab_id),
                pane: Some(pane_id),
            })
        })
        .await
    }

    async fn rename_tab(&self, id: &HerdrTabId, label: &str) -> Result<(), HerdrError> {
        self.mutate(
            "tab.rename",
            FakeCall::RenameTab(id.clone(), label.to_owned()),
            |st, ev| {
                let ws = st.ws_of_tab(id);
                let tab = st
                    .find_tab(id)
                    .ok_or_else(|| rejected("tab.rename", format!("tab {id} not found")))?;
                tab.label = label.to_owned();
                ev.push((
                    "tab_renamed",
                    json!({ "tab_id": id, "workspace_id": ws, "label": label }),
                ));
                Ok(())
            },
        )
        .await
    }

    async fn close_tab(&self, id: &HerdrTabId) -> Result<(), HerdrError> {
        self.mutate("tab.close", FakeCall::CloseTab(id.clone()), |st, ev| {
            st.find_tab(id)
                .ok_or_else(|| rejected("tab.close", format!("tab {id} not found")))?;
            st.remove_tab(id, ev);
            Ok(())
        })
        .await
    }

    async fn split_pane(&self, req: SplitPane) -> Result<Created, HerdrError> {
        self.mutate("pane.split", FakeCall::SplitPane(req.clone()), |st, ev| {
            let tab_id = st
                .tab_of_pane(&req.target)
                .ok_or_else(|| rejected("pane.split", format!("pane {} not found", req.target)))?;
            let ws = st.ws_of_tab(&tab_id).expect("workspace of tab");
            let pane = st.new_pane(None, req.cwd.clone(), req.env.clone());
            let pane_id = pane.id.clone();
            st.find_tab(&tab_id).expect("tab").panes.push(pane);
            ev.push((
                "pane_created",
                json!({ "pane_id": pane_id, "tab_id": tab_id, "workspace_id": ws }),
            ));
            Ok(Created {
                workspace: Some(ws),
                tab: Some(tab_id),
                pane: Some(pane_id),
            })
        })
        .await
    }

    async fn rename_pane(&self, id: &HerdrPaneId, label: &str) -> Result<(), HerdrError> {
        self.mutate(
            "pane.rename",
            FakeCall::RenamePane(id.clone(), label.to_owned()),
            |st, ev| {
                let pane = st
                    .find_pane(id)
                    .ok_or_else(|| rejected("pane.rename", format!("pane {id} not found")))?;
                pane.label = Some(label.to_owned());
                ev.push(("pane_updated", json!({ "pane_id": id })));
                Ok(())
            },
        )
        .await
    }

    async fn close_pane(&self, id: &HerdrPaneId) -> Result<(), HerdrError> {
        self.mutate("pane.close", FakeCall::ClosePane(id.clone()), |st, ev| {
            st.find_pane(id)
                .ok_or_else(|| rejected("pane.close", format!("pane {id} not found")))?;
            st.remove_pane(id, ev);
            Ok(())
        })
        .await
    }

    async fn report_pane_metadata(
        &self,
        id: &HerdrPaneId,
        key: &str,
        value: &str,
    ) -> Result<(), HerdrError> {
        let call = FakeCall::ReportPaneMetadata(id.clone(), key.to_owned(), value.to_owned());
        self.mutate("pane.report_metadata", call, |st, ev| {
            let pane = st
                .find_pane(id)
                .ok_or_else(|| rejected("pane.report_metadata", format!("pane {id} not found")))?;
            pane.metadata.insert(key.to_owned(), value.to_owned());
            ev.push(("pane_updated", json!({ "pane_id": id })));
            Ok(())
        })
        .await
    }

    async fn report_workspace_metadata(
        &self,
        id: &HerdrWorkspaceId,
        key: &str,
        value: &str,
    ) -> Result<(), HerdrError> {
        let call = FakeCall::ReportWorkspaceMetadata(id.clone(), key.to_owned(), value.to_owned());
        self.mutate("workspace.report_metadata", call, |st, ev| {
            let ws = st.find_ws(id).ok_or_else(|| {
                rejected(
                    "workspace.report_metadata",
                    format!("workspace {id} not found"),
                )
            })?;
            ws.metadata.insert(key.to_owned(), value.to_owned());
            ev.push(("workspace_metadata_updated", json!({ "workspace_id": id })));
            Ok(())
        })
        .await
    }

    async fn start_agent(&self, req: StartAgent) -> Result<StartOutcome, HerdrError> {
        let fault = self.enter("agent.start", FakeCall::StartAgent(req.clone()))?;
        if let Some(Fault::StartOutcome(o)) = fault {
            return Ok(o);
        }
        let mut ev = Vec::new();
        {
            let mut st = self.state.lock().unwrap();
            let pane = st
                .find_pane(&req.pane)
                .ok_or_else(|| rejected("agent.start", format!("pane {} not found", req.pane)))?;
            pane.agent = Some(AgentInfo {
                kind: req.kind.clone(),
                status: AgentStatus::Idle,
                session: None,
            });
            ev.push((
                "pane_agent_detected",
                json!({ "pane_id": req.pane, "agent": req.kind }),
            ));
        }
        self.emit(ev);
        if matches!(fault, Some(Fault::Hang)) {
            std::future::pending::<()>().await;
        }
        if matches!(fault, Some(Fault::LostResponse)) {
            return Err(HerdrError::Timeout);
        }
        Ok(StartOutcome::Started)
    }

    async fn agent(&self, pane: &HerdrPaneId) -> Result<Option<AgentInfo>, HerdrError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .find_pane(pane)
            .and_then(|p| p.agent.clone()))
    }

    async fn process_info(&self, pane: &HerdrPaneId) -> Result<ProcessInfo, HerdrError> {
        let st = self.state.lock().unwrap();
        st.processes
            .get(pane)
            .cloned()
            .ok_or_else(|| rejected("pane.process_info", format!("pane {pane} not found")))
    }

    async fn send_keys(&self, pane: &HerdrPaneId, keys: &[KeyInput]) -> Result<(), HerdrError> {
        self.mutate(
            "agent.send_keys",
            FakeCall::SendKeys(pane.clone(), keys.to_vec()),
            |_, _| Ok(()),
        )
        .await
    }
}
