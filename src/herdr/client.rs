//! `HerdrClient`: NDJSON-over-Unix-socket implementation of [`HerdrApi`] (spec §4.1).
use super::incarnation;
use super::wire::{self, Frame, WireRequest};
use crate::model::harness::StartOutcome;
use crate::model::{HerdrPaneId, HerdrTabId, HerdrWorkspaceId, Incarnation};
use crate::ports::herdr::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

const EVENT_CHANNEL: usize = 1024;
const BACKOFF_START: Duration = Duration::from_millis(100);
const BACKOFF_MAX: Duration = Duration::from_secs(5);
/// `agent.start` waits for the agent to come up; give it more than a plain request.
const AGENT_START_DEADLINE: Duration = Duration::from_secs(30);
/// A pane can close between the snapshot and the subscribe; retry the handshake this often.
const SUBSCRIBE_ATTEMPTS: usize = 3;
/// Name of the synthetic event pushed after every successful reconnect.
pub const RECONNECTED_EVENT: &str = "herdr_graph.reconnected";

pub struct HerdrClient {
    socket: PathBuf,
    timeout: Duration,
    generation: Arc<AtomicU64>,
    incarnation: Arc<Mutex<Incarnation>>,
    /// The cached incarnation may describe a server that is gone: the connection was lost (or never probed)
    /// since it was taken. The next snapshot re-probes before it is stamped.
    live: Arc<Liveness>,
    next_id: AtomicU64,
}

fn unavailable(e: impl std::fmt::Display) -> HerdrError {
    HerdrError::Unavailable(e.to_string())
}

fn lines_of(v: &[(String, String)]) -> Value {
    Value::Object(v.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect())
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn created(v: &Value) -> Created {
    let id = |obj: &str, key: &str| v.get(obj).and_then(|o| o.get(key)).and_then(Value::as_str).map(str::to_owned);
    let pane = v.get("root_pane").or_else(|| v.get("pane"));
    let pane_field = |key: &str| pane.and_then(|p| p.get(key)).and_then(Value::as_str).map(str::to_owned);
    Created {
        workspace: id("workspace", "workspace_id")
            .or_else(|| id("tab", "workspace_id"))
            .or_else(|| pane_field("workspace_id"))
            .map(HerdrWorkspaceId),
        tab: id("tab", "tab_id").or_else(|| pane_field("tab_id")).map(HerdrTabId),
        pane: pane_field("pane_id").map(HerdrPaneId),
    }
}

/// One request = one connection: connect, write one NDJSON line, read lines until our response.
async fn rpc(socket: &Path, id: String, method: &'static str, params: Value, limit: Duration) -> Result<Value, HerdrError> {
    let line = WireRequest { id: id.clone(), method, params }.to_line()?;
    let exchange = async {
        let mut stream = UnixStream::connect(socket).await.map_err(|e| HerdrError::Unavailable(format!("{}: {e}", socket.display())))?;
        stream.write_all(&line).await.map_err(unavailable)?;
        let mut reader = BufReader::new(stream);
        let mut buf = String::new();
        loop {
            buf.clear();
            // The request was sent: a close or read failure now means the outcome is unknown.
            match reader.read_line(&mut buf).await {
                Ok(0) | Err(_) => return Err(HerdrError::Timeout),
                Ok(_) => {}
            }
            if buf.trim().is_empty() {
                continue;
            }
            let Frame::Response(resp) = wire::parse_frame(buf.trim())? else { continue };
            // Herdr answers unparsable requests with an empty id.
            if resp.id.as_deref().is_some_and(|r| r != id && !r.is_empty()) {
                continue;
            }
            return match (resp.result, resp.error) {
                (_, Some(e)) => Err(wire::error_to_rejected(method, &e)),
                (Some(r), None) => Ok(r),
                (None, None) => Err(HerdrError::Protocol(format!("{method}: response has neither result nor error"))),
            };
        }
    };
    tokio::time::timeout(limit, exchange).await.unwrap_or(Err(HerdrError::Timeout))
}

/// Connect, subscribe to every topic, and wait for the acknowledgement. Returns the live connection.
async fn open_subscription(socket: &Path, id: String, limit: Duration) -> Result<BufReader<UnixStream>, HerdrError> {
    let mut last = HerdrError::Timeout;
    for attempt in 0..SUBSCRIBE_ATTEMPTS {
        let snap = rpc(socket, format!("{id}-snap{attempt}"), wire::M_SNAPSHOT, json!({}), limit).await?;
        let mut subs: Vec<Value> = wire::GLOBAL_TOPICS.iter().map(|t| json!({ "type": t })).collect();
        // `pane.agent_status_changed` has no wildcard form: subscribe per pane known right now.
        for pane in snap.pointer("/snapshot/panes").and_then(Value::as_array).into_iter().flatten() {
            if let Some(pid) = pane.get("pane_id").and_then(Value::as_str) {
                subs.push(json!({ "type": wire::TOPIC_AGENT_STATUS, "pane_id": pid }));
            }
        }
        let line = WireRequest { id: id.clone(), method: wire::M_SUBSCRIBE, params: json!({ "subscriptions": subs }) }.to_line()?;
        let handshake = async {
            let mut stream = UnixStream::connect(socket).await.map_err(|e| HerdrError::Unavailable(format!("{}: {e}", socket.display())))?;
            stream.write_all(&line).await.map_err(unavailable)?;
            let mut reader = BufReader::new(stream);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf).await {
                    Ok(0) | Err(_) => return Err(HerdrError::Timeout),
                    Ok(_) => {}
                }
                if buf.trim().is_empty() {
                    continue;
                }
                if let Frame::Response(resp) = wire::parse_frame(buf.trim())? {
                    return match resp.error {
                        Some(e) => Err(wire::error_to_rejected(wire::M_SUBSCRIBE, &e)),
                        None => Ok(reader),
                    };
                }
            }
        };
        match tokio::time::timeout(limit, handshake).await.unwrap_or(Err(HerdrError::Timeout)) {
            Ok(r) => return Ok(r),
            Err(e) if wire::is_code(&e, "pane_not_found") => last = e,
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

/// The connection to the server was lost: whatever answers next may be a different server process, so the
/// generation moves now (once per loss) and the cached incarnation is stale until it is probed again. Without
/// this a snapshot of a restarted server, taken before the reader reconnected, would carry the dead server's
/// incarnation and match bound panes by their reused pane ids.
struct Liveness {
    stale: AtomicBool,
    /// Identity of the socket file the cached incarnation was probed under. A restarted server removes and
    /// re-binds its socket, so a different identity means a different server even when the reader has not
    /// noticed the old connection end yet (a dying server can hold it open past the restart).
    sock: Mutex<Option<SockId>>,
}

/// `(device, inode, ctime seconds, ctime nanoseconds)` of the socket file.
type SockId = (u64, u64, i64, i64);

fn sock_id(socket: &Path) -> Option<SockId> {
    std::fs::symlink_metadata(socket).ok().map(|m| (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()))
}

impl Liveness {
    fn new() -> Self {
        Self { stale: AtomicBool::new(true), sock: Mutex::new(None) }
    }

    fn mark_lost(&self, generation: &AtomicU64) {
        if !self.stale.swap(true, Ordering::SeqCst) {
            generation.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Called before a request: the socket file is not the one the incarnation was probed under.
    fn check_socket(&self, socket: &Path, generation: &AtomicU64) {
        let recorded = *self.sock.lock().unwrap();
        if let (Some(then), Some(now)) = (recorded, sock_id(socket))
            && then != now
        {
            self.mark_lost(generation);
        }
    }
}

/// Whether a failed request means the server went away (or is going away) rather than rejected the request.
fn server_lost(e: &HerdrError) -> bool {
    matches!(e, HerdrError::Unavailable(_) | HerdrError::Timeout) || wire::is_code(e, "server_unavailable")
}

async fn refresh_incarnation(socket: &Path, generation: &AtomicU64, slot: &Mutex<Incarnation>, live: &Liveness) {
    // The incarnation stays stale until the probe result is stored: a snapshot taken meanwhile re-probes
    // instead of reading the previous server's incarnation.
    let identity = sock_id(socket);
    *live.sock.lock().unwrap() = identity;
    let g = generation.load(Ordering::SeqCst);
    let sock = socket.to_path_buf();
    let inc = tokio::task::spawn_blocking(move || incarnation::probe(&sock, g))
        .await
        .unwrap_or(Incarnation { generation: g, server_pid: None, server_started: None });
    *slot.lock().unwrap() = inc;
    if sock_id(socket) == identity {
        live.stale.store(false, Ordering::SeqCst);
    } else {
        // The server was replaced while it was probed: this probe may describe either one.
        generation.fetch_add(1, Ordering::SeqCst);
    }
}

struct Reader {
    socket: PathBuf,
    timeout: Duration,
    id: String,
    generation: Arc<AtomicU64>,
    incarnation: Arc<Mutex<Incarnation>>,
    live: Arc<Liveness>,
    tx: mpsc::Sender<HerdrEvent>,
}

impl Reader {
    /// Forward events until the connection ends, then reconnect with backoff (100 ms doubling to 5 s),
    /// bumping the generation and pushing a synthetic marker so consumers take a fresh snapshot.
    /// Returns when the receiver is dropped.
    async fn run(self, mut conn: BufReader<UnixStream>) {
        let mut line = String::new();
        loop {
            loop {
                line.clear();
                tokio::select! {
                    _ = self.tx.closed() => return,
                    r = conn.read_line(&mut line) => match r {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            if let Ok(Frame::Event(e)) = wire::parse_frame(line.trim())
                                && self.tx.send(HerdrEvent { name: e.event, payload: e.data }).await.is_err()
                            {
                                return;
                            }
                        }
                    },
                }
            }
            self.live.mark_lost(&self.generation);
            let mut delay = BACKOFF_START;
            conn = loop {
                tokio::select! {
                    _ = self.tx.closed() => return,
                    _ = tokio::time::sleep(delay) => {}
                }
                match open_subscription(&self.socket, self.id.clone(), self.timeout).await {
                    Ok(c) => break c,
                    Err(_) => delay = (delay * 2).min(BACKOFF_MAX),
                }
            };
            // The loss already moved the generation; the probe now names the server that answered.
            refresh_incarnation(&self.socket, &self.generation, &self.incarnation, &self.live).await;
            let generation = self.generation.load(Ordering::SeqCst);
            let marker = HerdrEvent { name: RECONNECTED_EVENT.to_owned(), payload: json!({ "generation": generation }) };
            if self.tx.send(marker).await.is_err() {
                return;
            }
        }
    }
}

impl HerdrClient {
    /// Request timeout 10 s.
    pub fn new(socket: PathBuf) -> Self {
        Self {
            socket,
            timeout: Duration::from_secs(10),
            generation: Arc::new(AtomicU64::new(0)),
            incarnation: Arc::new(Mutex::new(Incarnation::default())),
            live: Arc::new(Liveness::new()),
            next_id: AtomicU64::new(0),
        }
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    fn new_id(&self) -> String {
        format!("hg-{}-{}", std::process::id(), self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Connect error is `Unavailable`; a missed deadline or a connection lost after the request was
    /// sent is `Timeout` (outcome unknown); an `error` member is `Rejected{method, "<code>: <message>"}`.
    pub async fn request(&self, method: &'static str, params: Value) -> Result<Value, HerdrError> {
        rpc(&self.socket, self.new_id(), method, params, self.timeout).await
    }

    /// `pane.current` (used by bootstrap/undo when `HERDR_PANE_ID` is absent). `None` when Herdr has no
    /// current pane for this caller.
    pub async fn current_pane(&self) -> Result<Option<HerdrPaneId>, HerdrError> {
        match self.request(wire::M_PANE_CURRENT, json!({})).await {
            Ok(v) => Ok(v.pointer("/pane/pane_id").and_then(Value::as_str).map(|s| HerdrPaneId(s.to_owned()))),
            Err(HerdrError::Rejected { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Re-probe the server pid and start time under the current generation.
    pub async fn refresh_incarnation(&self) {
        refresh_incarnation(&self.socket, &self.generation, &self.incarnation, &self.live).await;
    }

    async fn ok(&self, method: &'static str, params: Value) -> Result<(), HerdrError> {
        self.request(method, params).await.map(|_| ())
    }
}

#[async_trait::async_trait]
impl HerdrApi for HerdrClient {
    async fn snapshot(&self) -> Result<HerdrSnapshot, HerdrError> {
        self.live.check_socket(&self.socket, &self.generation);
        let v = match self.request(wire::M_SNAPSHOT, json!({})).await {
            Ok(v) => v,
            Err(e) => {
                if server_lost(&e) {
                    self.live.mark_lost(&self.generation);
                }
                return Err(e);
            }
        };
        let mut snap = wire::parse_snapshot(&v)?;
        if self.live.stale.load(Ordering::SeqCst) {
            self.refresh_incarnation().await;
        }
        snap.incarnation = self.incarnation.lock().unwrap().clone();
        Ok(snap)
    }

    async fn subscribe(&self) -> Result<HerdrEventStream, HerdrError> {
        let id = self.new_id();
        let conn = open_subscription(&self.socket, id.clone(), self.timeout).await?;
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.refresh_incarnation().await;
        let (tx, rx) = mpsc::channel(EVENT_CHANNEL);
        let reader = Reader {
            socket: self.socket.clone(),
            timeout: self.timeout,
            id,
            generation: self.generation.clone(),
            incarnation: self.incarnation.clone(),
            live: self.live.clone(),
            tx,
        };
        tokio::spawn(reader.run(conn));
        Ok(rx)
    }

    async fn create_workspace(&self, req: CreateWorkspace) -> Result<Created, HerdrError> {
        let p = json!({ "label": req.label, "cwd": path_str(&req.cwd), "env": lines_of(&req.env), "focus": false });
        Ok(created(&self.request(wire::M_WORKSPACE_CREATE, p).await?))
    }

    async fn rename_workspace(&self, id: &HerdrWorkspaceId, label: &str) -> Result<(), HerdrError> {
        self.ok(wire::M_WORKSPACE_RENAME, json!({ "workspace_id": id.0, "label": label })).await
    }

    async fn close_workspace(&self, id: &HerdrWorkspaceId) -> Result<(), HerdrError> {
        self.ok(wire::M_WORKSPACE_CLOSE, json!({ "workspace_id": id.0 })).await
    }

    async fn create_tab(&self, req: CreateTab) -> Result<Created, HerdrError> {
        let p = json!({
            "workspace_id": req.workspace.0, "label": req.label, "cwd": path_str(&req.cwd),
            "env": lines_of(&req.env), "focus": false,
        });
        Ok(created(&self.request(wire::M_TAB_CREATE, p).await?))
    }

    async fn rename_tab(&self, id: &HerdrTabId, label: &str) -> Result<(), HerdrError> {
        self.ok(wire::M_TAB_RENAME, json!({ "tab_id": id.0, "label": label })).await
    }

    async fn close_tab(&self, id: &HerdrTabId) -> Result<(), HerdrError> {
        self.ok(wire::M_TAB_CLOSE, json!({ "tab_id": id.0 })).await
    }

    async fn split_pane(&self, req: SplitPane) -> Result<Created, HerdrError> {
        let direction = match req.direction {
            SplitDirection::Right => "right",
            SplitDirection::Down => "down",
        };
        let p = json!({
            "target_pane_id": req.target.0, "direction": direction, "cwd": path_str(&req.cwd),
            "env": lines_of(&req.env), "focus": false,
        });
        Ok(created(&self.request(wire::M_PANE_SPLIT, p).await?))
    }

    async fn rename_pane(&self, id: &HerdrPaneId, label: &str) -> Result<(), HerdrError> {
        self.ok(wire::M_PANE_RENAME, json!({ "pane_id": id.0, "label": label })).await
    }

    async fn close_pane(&self, id: &HerdrPaneId) -> Result<(), HerdrError> {
        self.ok(wire::M_PANE_CLOSE, json!({ "pane_id": id.0 })).await
    }

    async fn report_pane_metadata(&self, id: &HerdrPaneId, key: &str, value: &str) -> Result<(), HerdrError> {
        let p = json!({ "pane_id": id.0, "source": wire::METADATA_SOURCE, "tokens": { key: value } });
        self.ok(wire::M_PANE_REPORT_METADATA, p).await
    }

    async fn report_workspace_metadata(&self, id: &HerdrWorkspaceId, key: &str, value: &str) -> Result<(), HerdrError> {
        let p = json!({ "workspace_id": id.0, "source": wire::METADATA_SOURCE, "tokens": { key: value } });
        self.ok(wire::M_WORKSPACE_REPORT_METADATA, p).await
    }

    /// `agent.start` takes a `name`; the trait carries none, so the pane's label is used (its kind when
    /// the pane is unlabelled). Success is `Started`, `agent_not_ready` is `BlockedNeedsHuman`, a missed
    /// deadline is `Unknown` (never retried here), any other rejection is `NeedsRevision`.
    async fn start_agent(&self, req: StartAgent) -> Result<StartOutcome, HerdrError> {
        let reject = |e: HerdrError| match e {
            HerdrError::Timeout => Ok(StartOutcome::Unknown),
            HerdrError::Rejected { message, .. } if message.contains("agent_not_ready") => Ok(StartOutcome::BlockedNeedsHuman),
            HerdrError::Rejected { message, .. } => Ok(StartOutcome::NeedsRevision { reason: message }),
            other => Err(other),
        };
        let name = match self.request(wire::M_PANE_GET, json!({ "pane_id": req.pane.0 })).await {
            Ok(v) => v.pointer("/pane/label").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned),
            Err(e) => return reject(e),
        }
        .unwrap_or_else(|| req.kind.clone());
        let p = json!({ "name": name, "kind": req.kind, "pane_id": req.pane.0, "args": req.args });
        let limit = self.timeout.max(AGENT_START_DEADLINE);
        match rpc(&self.socket, self.new_id(), wire::M_AGENT_START, p, limit).await {
            Ok(_) => Ok(StartOutcome::Started),
            Err(e) => reject(e),
        }
    }

    async fn agent(&self, pane: &HerdrPaneId) -> Result<Option<AgentInfo>, HerdrError> {
        match self.request(wire::M_AGENT_GET, json!({ "target": pane.0 })).await {
            Ok(v) => Ok(v.get("agent").and_then(wire::parse_agent)),
            Err(e) if wire::is_code(&e, "agent_not_found") => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn process_info(&self, pane: &HerdrPaneId) -> Result<ProcessInfo, HerdrError> {
        wire::parse_process_info(&self.request(wire::M_PANE_PROCESS_INFO, json!({ "pane_id": pane.0 })).await?)
    }

    /// Text goes through `pane.send_text`, key names through `pane.send_keys` (consecutive keys are
    /// batched). Both work on any pane, agent or not; `agent.send_keys` would reject a plain shell.
    async fn send_keys(&self, pane: &HerdrPaneId, keys: &[KeyInput]) -> Result<(), HerdrError> {
        let mut batch: Vec<&str> = Vec::new();
        for k in keys {
            match k {
                KeyInput::Key(name) => batch.push(name),
                KeyInput::Text(text) => {
                    if !batch.is_empty() {
                        self.ok(wire::M_PANE_SEND_KEYS, json!({ "pane_id": pane.0, "keys": std::mem::take(&mut batch) })).await?;
                    }
                    self.ok(wire::M_PANE_SEND_TEXT, json!({ "pane_id": pane.0, "text": text })).await?;
                }
            }
        }
        if !batch.is_empty() {
            self.ok(wire::M_PANE_SEND_KEYS, json!({ "pane_id": pane.0, "keys": batch })).await?;
        }
        Ok(())
    }
}
