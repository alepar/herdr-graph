use super::client::{HerdrClient, RECONNECTED_EVENT};
use super::fake::{FakeCall, FakeHerdr, Fault};
use super::isolation::{
    IsolationError, IsolationGuard, live_resources_from, scrubbed_env_from, tripwire_verdict,
};
use super::{incarnation, wire};
use crate::model::harness::StartOutcome;
use crate::model::{HerdrPaneId, HerdrTabId, HerdrTerminalId, HerdrWorkspaceId};
use crate::ports::herdr::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

/// `session.snapshot` result captured from a private Herdr 0.9.1 server (protocol 22): one workspace with a
/// workspace token, two tabs, a labelled pane carrying `hg=cl_...`, and a pane with a `claude` agent.
/// Layouts were dropped (the parser ignores them) and `agent_session` was added by hand in the schema's
/// shape: a bare Herdr only reports it when an integration supplies it.
const SNAPSHOT_FIXTURE: &str = r#"{
 "type": "session_snapshot",
 "snapshot": {
  "version": "0.9.1",
  "protocol": 22,
  "focused_workspace_id": "w2",
  "focused_tab_id": "w2:t1",
  "focused_pane_id": "w2:p1",
  "workspaces": [
   {
    "workspace_id": "w2",
    "number": 1,
    "label": "fx",
    "focused": true,
    "pane_count": 2,
    "tab_count": 2,
    "active_tab_id": "w2:t1",
    "agent_status": "working",
    "tokens": {
     "hg": "ts_01HZZZZZZZZZZZZZZZZZZZZZZZ"
    }
   }
  ],
  "tabs": [
   {
    "tab_id": "w2:t1",
    "workspace_id": "w2",
    "number": 1,
    "label": "1",
    "focused": true,
    "pane_count": 1,
    "agent_status": "unknown"
   },
   {
    "tab_id": "w2:t2",
    "workspace_id": "w2",
    "number": 2,
    "label": "work",
    "focused": false,
    "pane_count": 1,
    "agent_status": "working"
   }
  ],
  "panes": [
   {
    "pane_id": "w2:p1",
    "terminal_id": "term_65cd82d92bdc57",
    "workspace_id": "w2",
    "tab_id": "w2:t1",
    "focused": true,
    "cwd": "/private/tmp/hg-explore",
    "foreground_cwd": "/private/tmp/hg-explore",
    "label": "planner",
    "agent_status": "unknown",
    "tokens": {
     "hg": "cl_01HYYYYYYYYYYYYYYYYYYYYYYY"
    },
    "scroll": {
     "offset_from_bottom": 0,
     "max_offset_from_bottom": 0,
     "viewport_rows": 40
    },
    "revision": 1
   },
   {
    "pane_id": "w2:p2",
    "terminal_id": "term_65cd82d92cad38",
    "workspace_id": "w2",
    "tab_id": "w2:t2",
    "focused": false,
    "cwd": "/private/tmp",
    "foreground_cwd": "/private/tmp",
    "agent": "claude",
    "agent_status": "working",
    "scroll": {
     "offset_from_bottom": 0,
     "max_offset_from_bottom": 0,
     "viewport_rows": 40
    },
    "revision": 0,
    "agent_session": {
     "source": "herdr-graph",
     "agent": "claude",
     "kind": "id",
     "value": "sess-123"
    }
   }
  ],
  "layouts": [],
  "agents": [
   {
    "terminal_id": "term_65cd82d92cad38",
    "agent": "claude",
    "agent_status": "working",
    "workspace_id": "w2",
    "tab_id": "w2:t2",
    "pane_id": "w2:p2",
    "focused": false,
    "state_change_seq": 1,
    "cwd": "/private/tmp",
    "foreground_cwd": "/private/tmp",
    "revision": 0,
    "agent_session": {
     "source": "herdr-graph",
     "agent": "claude",
     "kind": "id",
     "value": "sess-123"
    }
   }
  ]
 }
}"#;

fn pane(id: &str) -> HerdrPaneId {
    HerdrPaneId(id.to_owned())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = PathBuf::from(format!(
        "/private/tmp/hg-ut-{tag}-{}",
        ulid::Ulid::new().to_string().to_lowercase()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------- wire

#[test]
fn parse_snapshot_fixture() {
    let v: Value = serde_json::from_str(SNAPSHOT_FIXTURE).unwrap();
    let snap = wire::parse_snapshot(&v).unwrap();
    assert_eq!(snap.workspaces.len(), 1);
    let ws = &snap.workspaces[0];
    assert_eq!(ws.id, HerdrWorkspaceId("w2".into()));
    assert_eq!(ws.label, "fx");
    assert_eq!(
        ws.metadata.get("hg").map(String::as_str),
        Some("ts_01HZZZZZZZZZZZZZZZZZZZZZZZ")
    );
    assert_eq!(
        ws.tabs.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(),
        ["1", "work"]
    );
    assert_eq!(ws.tabs[0].id, HerdrTabId("w2:t1".into()));

    let labelled = &ws.tabs[0].panes[0];
    assert_eq!(labelled.id, pane("w2:p1"));
    assert_eq!(labelled.label.as_deref(), Some("planner"));
    assert_eq!(
        labelled.terminal_id,
        Some(HerdrTerminalId("term_65cd82d92bdc57".into()))
    );
    assert_eq!(
        labelled.cwd.as_deref(),
        Some(Path::new("/private/tmp/hg-explore"))
    );
    assert_eq!(
        labelled.metadata.get("hg").map(String::as_str),
        Some("cl_01HYYYYYYYYYYYYYYYYYYYYYYY")
    );
    assert_eq!(labelled.agent, None);

    let agent_pane = &ws.tabs[1].panes[0];
    assert_eq!(agent_pane.id, pane("w2:p2"));
    assert_eq!(agent_pane.label, None);
    assert!(agent_pane.metadata.is_empty());
    assert_eq!(
        agent_pane.agent,
        Some(AgentInfo {
            kind: "claude".into(),
            status: AgentStatus::Working,
            session: Some(AgentSession::Id("sess-123".into()))
        })
    );
}

#[test]
fn parse_snapshot_rejects_malformed_and_maps_path_sessions() {
    let err = wire::parse_snapshot(&json!({"snapshot": {"workspaces": []}})).unwrap_err();
    assert!(
        matches!(err, HerdrError::Protocol(m) if m.contains("tabs") || m.contains("agents") || m.contains("panes"))
    );
    let agent = wire::parse_agent(&json!({
        "agent": "codex", "agent_status": "blocked",
        "agent_session": {"source": "s", "agent": "codex", "kind": "path", "value": "/x/rollout.jsonl"}
    }))
    .unwrap();
    assert_eq!(agent.status, AgentStatus::Blocked);
    assert_eq!(
        agent.session,
        Some(AgentSession::Path(PathBuf::from("/x/rollout.jsonl")))
    );
    assert_eq!(
        wire::parse_agent(&json!({"agent": null, "agent_status": "idle"})),
        None
    );
}

#[test]
fn wire_error_maps_to_rejected() {
    let line =
        r#"{"id":"hg-1","error":{"code":"agent_not_ready","message":"trust dialog showing"}}"#;
    let wire::Frame::Response(resp) = wire::parse_frame(line).unwrap() else {
        panic!("not a response")
    };
    let err = wire::error_to_rejected(wire::M_AGENT_START, resp.error.as_ref().unwrap());
    match &err {
        HerdrError::Rejected { method, message } => {
            assert_eq!(method, "agent.start");
            assert_eq!(message, "agent_not_ready: trust dialog showing");
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
    assert!(wire::is_code(&err, "agent_not_ready"));
    assert!(!wire::is_code(&err, "agent_not_found"));
}

#[test]
fn request_envelope_shape() {
    let req = wire::WireRequest {
        id: "hg-7".into(),
        method: wire::M_TAB_CREATE,
        params: json!({"label": "t"}),
    };
    let line = req.to_line().unwrap();
    assert_eq!(*line.last().unwrap(), b'\n');
    assert_eq!(line.iter().filter(|b| **b == b'\n').count(), 1);
    let v: Value = serde_json::from_slice(&line).unwrap();
    assert_eq!(
        v,
        json!({"id": "hg-7", "method": "tab.create", "params": {"label": "t"}})
    );
}

#[test]
fn event_frames_are_distinguished_from_responses() {
    let ev = wire::parse_frame(r#"{"event":"tab_created","data":{"type":"tab_created"}}"#).unwrap();
    assert!(matches!(ev, wire::Frame::Event(e) if e.event == "tab_created"));
    let ack = wire::parse_frame(r#"{"id":"s","result":{"type":"subscription_started"}}"#).unwrap();
    assert!(matches!(ack, wire::Frame::Response(r) if r.result.is_some()));
    assert!(wire::parse_frame("not json").is_err());
}

#[test]
fn process_info_parses_shell_and_foreground_program() {
    let shell = wire::parse_process_info(&json!({"process_info": {
        "pane_id": "p", "shell_pid": 10, "foreground_process_group_id": 10,
        "foreground_processes": [{"pid": 10, "name": "bash", "argv": ["-sh"]}]}}))
    .unwrap();
    assert!(shell.is_shell);
    assert_eq!(shell.foreground_pid, Some(10));
    let busy = wire::parse_process_info(&json!({"process_info": {
        "pane_id": "p", "shell_pid": 10, "foreground_process_group_id": 55,
        "foreground_processes": [{"pid": 55, "name": "vim", "argv": ["vim", "a.txt"]}]}}))
    .unwrap();
    assert!(!busy.is_shell);
    assert_eq!(busy.foreground_argv, ["vim", "a.txt"]);
}

// ---------------------------------------------------------------- incarnation

#[test]
fn probe_on_missing_socket_is_generation_only() {
    let inc = incarnation::probe(Path::new("/private/tmp/hg-ut-no-such-socket.sock"), 4);
    assert_eq!(inc.generation, 4);
    assert_eq!(inc.server_pid, None);
    assert_eq!(inc.server_started, None);
}

// ---------------------------------------------------------------- client against a scripted server

enum Reply {
    Result(Value),
    Error(&'static str, &'static str),
    Silent,
    /// Ack the subscription, send these event lines, then close (`hold` keeps the connection open).
    Subscribe {
        events: Vec<Value>,
        hold: bool,
    },
}

/// Minimal Herdr stand-in: one request per connection, answered by `handler(method, params, nth_connection)`.
fn mini_server(
    dir: &Path,
    handler: impl Fn(&str, &Value, usize) -> Reply + Send + Sync + 'static,
) -> PathBuf {
    let socket = dir.join("mini.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let handler = std::sync::Arc::new(handler);
    tokio::spawn(async move {
        let mut n = 0usize;
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let nth = n;
            n += 1;
            let handler = handler.clone();
            tokio::spawn(async move {
                let (r, mut w) = stream.into_split();
                let mut line = String::new();
                if BufReader::new(r).read_line(&mut line).await.unwrap_or(0) == 0 {
                    return;
                }
                let req: Value = serde_json::from_str(&line).unwrap();
                let id = req["id"].clone();
                match handler(req["method"].as_str().unwrap(), &req["params"], nth) {
                    Reply::Result(v) => {
                        let _ = w
                            .write_all(format!("{}\n", json!({"id": id, "result": v})).as_bytes())
                            .await;
                    }
                    Reply::Error(code, message) => {
                        let body = json!({"id": id, "error": {"code": code, "message": message}});
                        let _ = w.write_all(format!("{body}\n").as_bytes()).await;
                    }
                    Reply::Silent => tokio::time::sleep(Duration::from_secs(30)).await,
                    Reply::Subscribe { events, hold } => {
                        let ack = json!({"id": id, "result": {"type": "subscription_started"}});
                        let _ = w.write_all(format!("{ack}\n").as_bytes()).await;
                        for e in events {
                            let _ = w.write_all(format!("{e}\n").as_bytes()).await;
                        }
                        if hold {
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                    }
                }
            });
        }
    });
    socket
}

fn empty_snapshot() -> Value {
    json!({"type": "session_snapshot", "snapshot": {"workspaces": [], "tabs": [], "panes": [], "agents": [], "layouts": []}})
}

#[tokio::test]
async fn client_connect_failure_is_unavailable() {
    let c = HerdrClient::new(PathBuf::from("/private/tmp/hg-ut-absent.sock"));
    assert!(matches!(
        c.request(wire::M_SNAPSHOT, json!({})).await,
        Err(HerdrError::Unavailable(_))
    ));
}

#[tokio::test]
async fn client_deadline_is_timeout() {
    let dir = scratch("timeout");
    let socket = mini_server(&dir, |_, _, _| Reply::Silent);
    let c = HerdrClient::new(socket).with_timeout(Duration::from_millis(150));
    assert!(matches!(
        c.request(wire::M_SNAPSHOT, json!({})).await,
        Err(HerdrError::Timeout)
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn client_sends_schema_params_and_returns_created_ids() {
    let dir = scratch("create");
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, Value)>::new()));
    let log = seen.clone();
    let socket = mini_server(&dir, move |m, p, _| {
        log.lock().unwrap().push((m.to_owned(), p.clone()));
        Reply::Result(json!({
            "type": "workspace_created",
            "workspace": {"workspace_id": "w9"}, "tab": {"tab_id": "w9:t1", "workspace_id": "w9"},
            "root_pane": {"pane_id": "w9:p1", "tab_id": "w9:t1", "workspace_id": "w9"}
        }))
    });
    let c = HerdrClient::new(socket);
    let made = c
        .create_workspace(CreateWorkspace {
            label: "ws".into(),
            cwd: "/work".into(),
            env: vec![("A".into(), "1".into())],
        })
        .await
        .unwrap();
    assert_eq!(made.workspace, Some(HerdrWorkspaceId("w9".into())));
    assert_eq!(made.tab, Some(HerdrTabId("w9:t1".into())));
    assert_eq!(made.pane, Some(pane("w9:p1")));
    let calls = seen.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "workspace.create");
    assert_eq!(
        calls[0].1,
        json!({"label": "ws", "cwd": "/work", "env": {"A": "1"}, "focus": false})
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn client_start_agent_maps_outcomes() {
    let dir = scratch("agent");
    let socket = mini_server(&dir, |m, p, _| match (m, p["pane_id"].as_str().unwrap()) {
        ("pane.get", _) => Reply::Result(json!({"pane": {"pane_id": "x", "label": "planner"}})),
        ("agent.start", "ready") => {
            assert_eq!(p["name"], "planner");
            assert_eq!(p["kind"], "claude");
            Reply::Result(json!({"type": "agent_started"}))
        }
        ("agent.start", "trust") => Reply::Error("agent_not_ready", "trust dialog"),
        ("agent.start", _) => Reply::Error("pane_busy", "foreground is not a shell"),
        _ => Reply::Error("unexpected", "unexpected"),
    });
    let c = HerdrClient::new(socket);
    let start = |p: &str| StartAgent {
        pane: pane(p),
        kind: "claude".into(),
        args: vec![],
    };
    assert_eq!(
        c.start_agent(start("ready")).await.unwrap(),
        StartOutcome::Started
    );
    assert_eq!(
        c.start_agent(start("trust")).await.unwrap(),
        StartOutcome::BlockedNeedsHuman
    );
    let StartOutcome::NeedsRevision { reason } = c.start_agent(start("busy")).await.unwrap() else {
        panic!("expected NeedsRevision")
    };
    assert!(reason.contains("pane_busy"));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn client_agent_not_found_is_none() {
    let dir = scratch("agentget");
    let socket = mini_server(&dir, |_, _, _| {
        Reply::Error("agent_not_found", "agent target p not found")
    });
    let c = HerdrClient::new(socket);
    assert_eq!(c.agent(&pane("p")).await.unwrap(), None);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn client_send_keys_batches_keys_and_sends_text_separately() {
    let dir = scratch("keys");
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, Value)>::new()));
    let log = seen.clone();
    let socket = mini_server(&dir, move |m, p, _| {
        log.lock().unwrap().push((m.to_owned(), p.clone()));
        Reply::Result(json!({"type": "ok"}))
    });
    let c = HerdrClient::new(socket);
    c.send_keys(
        &pane("p1"),
        &[
            KeyInput::Text("echo hi".into()),
            KeyInput::Key("Enter".into()),
            KeyInput::Key("Tab".into()),
        ],
    )
    .await
    .unwrap();
    let calls = seen.lock().unwrap().clone();
    assert_eq!(
        calls[0],
        (
            "pane.send_text".to_owned(),
            json!({"pane_id": "p1", "text": "echo hi"})
        )
    );
    assert_eq!(
        calls[1],
        (
            "pane.send_keys".to_owned(),
            json!({"pane_id": "p1", "keys": ["Enter", "Tab"]})
        )
    );
    assert_eq!(calls.len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn subscribe_forwards_events_then_reconnects_with_marker() {
    let dir = scratch("sub");
    let socket = mini_server(&dir, |m, p, _| match m {
        "session.snapshot" => Reply::Result(empty_snapshot()),
        "events.subscribe" => {
            // Every topic is requested; `pane.agent_status_changed` is per pane, so none for an empty snapshot.
            let topics: Vec<&str> = p["subscriptions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["type"].as_str().unwrap())
                .collect();
            assert!(topics.contains(&"tab.created") && topics.contains(&"pane.agent_detected"));
            assert!(!topics.contains(&"pane.agent_status_changed"));
            // The first connection delivers one event and drops; later ones stay open.
            Reply::Subscribe {
                events: vec![json!({"event": "tab_created", "data": {"tab_id": "t1"}})],
                hold: false,
            }
        }
        _ => Reply::Error("unexpected", m.to_owned().leak()),
    });
    let c = HerdrClient::new(socket);
    let mut rx = c.subscribe().await.unwrap();
    assert!(
        c.snapshot().await.unwrap().incarnation.generation >= 1,
        "subscribe bumps the generation"
    );
    let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.name, "tab_created");
    assert_eq!(first.payload["tab_id"], "t1");
    // The server dropped the connection after the event: the reader reconnects and says so.
    let mut saw_marker = None;
    for _ in 0..10 {
        let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        if ev.name == RECONNECTED_EVENT {
            saw_marker = Some(ev);
            break;
        }
    }
    let marker = saw_marker.expect("reconnect marker event");
    assert!(
        marker.payload["generation"].as_u64().unwrap() >= 2,
        "reconnect bumps the generation again"
    );
    drop(rx);
    let _ = std::fs::remove_dir_all(dir);
}

/// A server that goes away and comes back between two snapshots is a new incarnation even when nothing
/// reconnected a subscription: the snapshot after the loss must not carry the dead server's incarnation, or
/// the matcher binds panes by their reused ids (hg-zmi.76).
#[tokio::test]
async fn snapshot_after_a_server_loss_is_a_new_incarnation() {
    let dir = scratch("lost");
    let n = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let socket = mini_server(&dir, move |m, _, _| match m {
        "session.snapshot" => match n.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
            1 => Reply::Error("server_unavailable", "server is shutting down"),
            _ => Reply::Result(empty_snapshot()),
        },
        _ => Reply::Error("unexpected", m.to_owned().leak()),
    });
    let c = HerdrClient::new(socket);
    let before = c.snapshot().await.unwrap().incarnation;
    assert!(
        c.snapshot().await.is_err(),
        "the second snapshot is refused while the server shuts down"
    );
    let after = c.snapshot().await.unwrap().incarnation;
    assert_ne!(
        before, after,
        "a snapshot after a lost server is not stamped with the old incarnation"
    );
    // A healthy stream of snapshots keeps one incarnation.
    assert_eq!(c.snapshot().await.unwrap().incarnation, after);
    let _ = std::fs::remove_dir_all(dir);
}

/// A server restarted in place (socket removed and re-bound) is a new incarnation on the very next snapshot,
/// with no failed request and no subscription to notice it: the dying server can hold a subscription open
/// past the restart (hg-zmi.76).
#[tokio::test]
async fn snapshot_after_the_socket_was_rebound_is_a_new_incarnation() {
    let dir = scratch("rebound");
    let serve = |dir: &Path| {
        mini_server(dir, |m, _, _| match m {
            "session.snapshot" => Reply::Result(empty_snapshot()),
            _ => Reply::Error("unexpected", m.to_owned().leak()),
        })
    };
    let socket = serve(&dir);
    let c = HerdrClient::new(socket.clone());
    let before = c.snapshot().await.unwrap().incarnation;
    assert_eq!(
        c.snapshot().await.unwrap().incarnation,
        before,
        "the same server keeps its incarnation"
    );
    std::fs::remove_file(&socket).unwrap();
    assert_eq!(serve(&dir), socket);
    let after = c.snapshot().await.unwrap().incarnation;
    assert_ne!(before, after, "a re-bound socket is a different server");
    assert_eq!(c.snapshot().await.unwrap().incarnation, after);
    let _ = std::fs::remove_dir_all(dir);
}

/// The reader notices the dropped subscription before the new server is reachable: the generation moves at the
/// loss, and the reconnect marker reports the incarnation the new server was probed under.
#[tokio::test]
async fn subscription_loss_moves_the_generation_once() {
    let dir = scratch("loss-gen");
    let socket = mini_server(&dir, |m, _, nth| match m {
        "session.snapshot" => Reply::Result(empty_snapshot()),
        // The first subscription drops at once; the retries are held open.
        "events.subscribe" => Reply::Subscribe {
            events: vec![],
            hold: nth > 1,
        },
        _ => Reply::Error("unexpected", m.to_owned().leak()),
    });
    let c = HerdrClient::new(socket);
    let mut rx = c.subscribe().await.unwrap();
    let marker = loop {
        let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        if ev.name == RECONNECTED_EVENT {
            break ev;
        }
    };
    let g = marker.payload["generation"].as_u64().unwrap();
    assert_eq!(
        g, 2,
        "one subscribe plus one loss is two generations, not three"
    );
    assert_eq!(
        c.snapshot().await.unwrap().incarnation.generation,
        g,
        "the snapshot after the reconnect carries the marker's generation"
    );
    drop(rx);
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------- FakeHerdr

fn ws_req(label: &str) -> CreateWorkspace {
    CreateWorkspace {
        label: label.into(),
        cwd: "/w".into(),
        env: vec![("HERDR_GRAPH".into(), "1".into())],
    }
}

fn drain(rx: &mut HerdrEventStream) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(e) = rx.try_recv() {
        out.push(e.name);
    }
    out
}

#[tokio::test]
async fn fake_create_and_snapshot() {
    let fake = FakeHerdr::new();
    let made = fake.create_workspace(ws_req("ws")).await.unwrap();
    let tab = fake
        .create_tab(CreateTab {
            workspace: made.workspace.clone().unwrap(),
            label: "seat".into(),
            cwd: "/t".into(),
            env: vec![("K".into(), "V".into())],
        })
        .await
        .unwrap();
    let split = fake
        .split_pane(SplitPane {
            target: tab.pane.clone().unwrap(),
            direction: SplitDirection::Right,
            cwd: "/s".into(),
            env: vec![],
        })
        .await
        .unwrap();
    let snap = fake.snapshot().await.unwrap();
    assert_eq!(snap.workspaces.len(), 1);
    let ws = &snap.workspaces[0];
    assert_eq!((ws.id.0.as_str(), ws.label.as_str()), ("w1", "ws"));
    assert_eq!(ws.tabs.len(), 2);
    assert_eq!(ws.tabs[1].label, "seat");
    assert_eq!(ws.tabs[1].panes.len(), 2);
    let root = &ws.tabs[0].panes[0];
    assert_eq!(
        (root.id.0.as_str(), root.cwd.as_deref()),
        ("p1", Some(Path::new("/w")))
    );
    assert_eq!(root.terminal_id, Some(HerdrTerminalId("term1".into())));
    assert_eq!(ws.tabs[1].panes[1].id, split.pane.unwrap());
    assert_eq!(
        fake.pane_env(&tab.pane.unwrap()),
        Some(vec![("K".to_owned(), "V".to_owned())])
    );
    assert!(fake.process_info(&pane("p1")).await.unwrap().is_shell);
}

#[tokio::test]
async fn fake_close_last_pane_closes_tab() {
    let fake = FakeHerdr::new();
    let ws = fake
        .create_workspace(ws_req("ws"))
        .await
        .unwrap()
        .workspace
        .unwrap();
    let seat = fake
        .create_tab(CreateTab {
            workspace: ws.clone(),
            label: "seat".into(),
            cwd: "/".into(),
            env: vec![],
        })
        .await
        .unwrap();
    let extra = fake.add_user_pane(seat.tab.as_ref().unwrap(), "extra");
    fake.close_pane(seat.pane.as_ref().unwrap()).await.unwrap();
    assert_eq!(
        fake.snapshot().await.unwrap().workspaces[0].tabs.len(),
        2,
        "tab survives while a pane remains"
    );
    fake.close_pane(&extra).await.unwrap();
    let tabs = fake.snapshot().await.unwrap().workspaces[0]
        .tabs
        .iter()
        .map(|t| t.label.clone())
        .collect::<Vec<_>>();
    assert_eq!(tabs, ["1"]);
}

#[tokio::test]
async fn fake_close_last_tab_closes_workspace_by_default() {
    let fake = FakeHerdr::new();
    let a = fake.create_workspace(ws_req("a")).await.unwrap();
    fake.close_tab(a.tab.as_ref().unwrap()).await.unwrap();
    assert!(fake.snapshot().await.unwrap().workspaces.is_empty());

    fake.set_last_tab_closes_workspace(false);
    let b = fake.create_workspace(ws_req("b")).await.unwrap();
    fake.close_tab(b.tab.as_ref().unwrap()).await.unwrap();
    let snap = fake.snapshot().await.unwrap();
    assert_eq!(snap.workspaces.len(), 1);
    assert!(snap.workspaces[0].tabs.is_empty());
}

#[tokio::test]
async fn fake_events_emitted_to_subscribers() {
    let fake = FakeHerdr::new();
    let mut rx = fake.subscribe().await.unwrap();
    let made = fake.create_workspace(ws_req("ws")).await.unwrap();
    assert_eq!(
        drain(&mut rx),
        ["workspace_created", "tab_created", "pane_created"]
    );
    fake.rename_tab(made.tab.as_ref().unwrap(), "renamed")
        .await
        .unwrap();
    fake.user_rename_workspace(made.workspace.as_ref().unwrap(), "ws2");
    assert_eq!(drain(&mut rx), ["tab_renamed", "workspace_renamed"]);
    fake.user_close_pane(made.pane.as_ref().unwrap());
    assert_eq!(
        drain(&mut rx),
        ["pane_closed", "tab_closed", "workspace_closed"]
    );
    fake.disconnect_subscribers();
    assert!(rx.recv().await.is_none(), "stream ends after disconnect");
}

#[tokio::test]
async fn fake_fault_injection_lost_response_still_creates() {
    let fake = FakeHerdr::new();
    fake.fail_next("workspace.create", Fault::LostResponse);
    assert!(matches!(
        fake.create_workspace(ws_req("lost")).await,
        Err(HerdrError::Timeout)
    ));
    assert_eq!(
        fake.snapshot().await.unwrap().workspaces.len(),
        1,
        "call was performed"
    );

    fake.fail_next("workspace.create", Fault::Unavailable);
    assert!(matches!(
        fake.create_workspace(ws_req("never")).await,
        Err(HerdrError::Unavailable(_))
    ));
    assert_eq!(
        fake.snapshot().await.unwrap().workspaces.len(),
        1,
        "unavailable call was not performed"
    );

    fake.fail_next("workspace.create", Fault::Rejected("nope".into()));
    assert!(
        matches!(fake.create_workspace(ws_req("x")).await, Err(HerdrError::Rejected { message, .. }) if message == "nope")
    );
    // Faults are one-shot.
    fake.create_workspace(ws_req("fine")).await.unwrap();
    assert_eq!(fake.snapshot().await.unwrap().workspaces.len(), 2);

    fake.fail_next(
        "agent.start",
        Fault::StartOutcome(StartOutcome::BlockedNeedsHuman),
    );
    let out = fake
        .start_agent(StartAgent {
            pane: pane("p1"),
            kind: "claude".into(),
            args: vec![],
        })
        .await
        .unwrap();
    assert_eq!(out, StartOutcome::BlockedNeedsHuman);
    assert_eq!(
        fake.agent(&pane("p1")).await.unwrap(),
        None,
        "scripted outcome starts nothing"
    );
}

#[tokio::test]
async fn fake_restart_changes_pane_ids_keeps_terminal_ids() {
    let fake = FakeHerdr::new();
    let made = fake.create_workspace(ws_req("ws")).await.unwrap();
    let p = made.pane.unwrap();
    fake.report_pane_metadata(&p, "hg", "cl_1").await.unwrap();
    let before = fake.snapshot().await.unwrap();
    fake.restart(true, true);
    let after = fake.snapshot().await.unwrap();
    let (old, new) = (
        &before.workspaces[0].tabs[0].panes[0],
        &after.workspaces[0].tabs[0].panes[0],
    );
    assert_ne!(old.id, new.id);
    assert_eq!(old.terminal_id, new.terminal_id);
    assert_eq!(new.metadata.get("hg").map(String::as_str), Some("cl_1"));
    assert_ne!(before.incarnation, after.incarnation);
    assert!(
        fake.process_info(&new.id).await.unwrap().is_shell,
        "per-pane state follows the new id"
    );

    fake.restart(false, false);
    let dropped = &fake.snapshot().await.unwrap().workspaces[0].tabs[0].panes[0].clone();
    assert!(dropped.metadata.is_empty());
    assert_ne!(dropped.terminal_id, new.terminal_id);
}

#[tokio::test]
async fn fake_metadata_stamps_visible_in_snapshot() {
    let fake = FakeHerdr::new();
    let made = fake.create_workspace(ws_req("ws")).await.unwrap();
    fake.report_pane_metadata(made.pane.as_ref().unwrap(), "hg", "cl_X")
        .await
        .unwrap();
    fake.report_workspace_metadata(made.workspace.as_ref().unwrap(), "hg", "ts_X")
        .await
        .unwrap();
    fake.rename_pane(made.pane.as_ref().unwrap(), "planner")
        .await
        .unwrap();
    let snap = fake.snapshot().await.unwrap();
    assert_eq!(
        snap.workspaces[0].metadata.get("hg").map(String::as_str),
        Some("ts_X")
    );
    let p = &snap.workspaces[0].tabs[0].panes[0];
    assert_eq!(p.metadata.get("hg").map(String::as_str), Some("cl_X"));
    assert_eq!(p.label.as_deref(), Some("planner"));
    assert!(matches!(
        fake.report_pane_metadata(&pane("nope"), "hg", "v").await,
        Err(HerdrError::Rejected { .. })
    ));
}

#[tokio::test]
async fn fake_calls_recorded_in_order() {
    let fake = FakeHerdr::new();
    let made = fake.create_workspace(ws_req("ws")).await.unwrap();
    let ws = made.workspace.unwrap();
    fake.rename_workspace(&ws, "new").await.unwrap();
    fake.send_keys(
        made.pane.as_ref().unwrap(),
        &[KeyInput::Text("ls".into()), KeyInput::Key("Enter".into())],
    )
    .await
    .unwrap();
    fake.close_workspace(&ws).await.unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 4);
    assert!(matches!(&calls[0], FakeCall::CreateWorkspace(r) if r.label == "ws"));
    assert_eq!(
        calls[1],
        FakeCall::RenameWorkspace(ws.clone(), "new".into())
    );
    assert!(matches!(&calls[2], FakeCall::SendKeys(p, k) if p.0 == "p1" && k.len() == 2));
    assert_eq!(calls[3], FakeCall::CloseWorkspace(ws));
    fake.clear_calls();
    assert!(fake.calls().is_empty());
}

#[tokio::test]
async fn fake_scripted_user_actions_mutate_and_emit() {
    let fake = FakeHerdr::new();
    let a = fake.create_workspace(ws_req("ws")).await.unwrap();
    let seat = fake
        .create_tab(CreateTab {
            workspace: a.workspace.clone().unwrap(),
            label: "seat".into(),
            cwd: "/".into(),
            env: vec![],
        })
        .await
        .unwrap();
    let mut rx = fake.subscribe().await.unwrap();
    fake.user_move_pane(seat.pane.as_ref().unwrap(), a.tab.as_ref().unwrap());
    assert_eq!(drain(&mut rx), ["pane_moved", "tab_closed"]);
    let snap = fake.snapshot().await.unwrap();
    assert_eq!(
        snap.workspaces[0].tabs.len(),
        1,
        "emptied source tab closed"
    );
    assert_eq!(snap.workspaces[0].tabs[0].panes.len(), 2);
    fake.set_agent(
        seat.pane.as_ref().unwrap(),
        Some(AgentInfo {
            kind: "claude".into(),
            status: AgentStatus::Idle,
            session: None,
        }),
    );
    assert_eq!(drain(&mut rx), ["pane_agent_detected"]);
    assert!(
        fake.agent(seat.pane.as_ref().unwrap())
            .await
            .unwrap()
            .is_some()
    );
}

// ---------------------------------------------------------------- isolation guard

#[test]
fn guard_accepts_paths_under_root() {
    let root = scratch("guard-ok");
    let guard = IsolationGuard::with_live(
        &root,
        vec![(PathBuf::from("/private/tmp/hg-ut-live-x"), "live")],
    );
    assert_eq!(guard.check(&root.join("herdr.sock")), Ok(()));
    assert_eq!(guard.check(&root.join("a/b/c/not-yet-created")), Ok(()));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn guard_rejects_outside_root() {
    let root = scratch("guard-out");
    let other = scratch("guard-other");
    let guard = IsolationGuard::with_live(&root, vec![]);
    assert!(matches!(
        guard.check(&other.join("x.sock")),
        Err(IsolationError::OutsideRoot(..))
    ));
    // `..` in a not-yet-existing tail must not climb out of the root.
    assert!(matches!(
        guard.check(&root.join("missing/../../escape")),
        Err(IsolationError::OutsideRoot(..))
    ));
    let live_dir = scratch("guard-live");
    let guard = IsolationGuard::with_live(&root, vec![(live_dir.clone(), "the user's thing")]);
    assert_eq!(
        guard.check(&live_dir.join("s.sock")),
        Err(IsolationError::LiveResource(
            live_dir.canonicalize().unwrap().join("s.sock"),
            "the user's thing"
        ))
    );
    for d in [root, other, live_dir] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn guard_rejects_live_socket_even_if_symlinked_into_root() {
    let root = scratch("guard-link");
    let live_dir = scratch("guard-livesock");
    let live_sock = live_dir.join("herdr.sock");
    std::fs::write(&live_sock, b"").unwrap();
    let link = root.join("herdr.sock");
    std::os::unix::fs::symlink(&live_sock, &link).unwrap();
    let guard = IsolationGuard::with_live(&root, vec![(live_sock, "the user's Herdr API socket")]);
    assert!(matches!(
        guard.check(&link),
        Err(IsolationError::LiveResource(
            _,
            "the user's Herdr API socket"
        ))
    ));
    for d in [root, live_dir] {
        let _ = std::fs::remove_dir_all(d);
    }
}

fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn scrubbed_env_drops_inherited_herdr_and_claude_vars() {
    let root = Path::new("/private/tmp/hg-ut-root");
    let env = scrubbed_env_from(
        root,
        vars(&[
            ("PATH", "/usr/bin"),
            ("HOME", "/Users/real"),
            ("HERDR_SOCKET_PATH", "/Users/real/.config/herdr/herdr.sock"),
            ("HERDR_PANE_ID", "w1:p1"),
            ("HERDR_ENV", "1"),
            ("CLAUDE_CONFIG_DIR", "/Users/real/.claude"),
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_ENTRYPOINT", "cli"),
            ("XDG_CONFIG_HOME", "/Users/real/.config"),
            ("XDG_RUNTIME_DIR", "/run/user/1"),
        ]),
    );
    let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    for k in [
        "HERDR_PANE_ID",
        "HERDR_ENV",
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
    ] {
        assert_eq!(get(k), None, "{k} must not be inherited");
    }
    for k in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
        "HERDR_CONFIG_PATH",
        "HERDR_SOCKET_PATH",
        "HERDR_PLUGIN_STATE_DIR",
        "HERDR_PLUGIN_CONFIG_DIR",
        "CLAUDE_CONFIG_DIR",
        "HERDR_GRAPH_INSTANCE",
        "HG_TEST_ROOT",
    ] {
        let v = get(k).unwrap_or_else(|| panic!("{k} missing"));
        assert!(
            Path::new(v).starts_with(root),
            "{k}={v} is not under the private root"
        );
    }
    assert_eq!(get("PATH"), Some("/usr/bin"));
    assert_eq!(
        get("HG_TEST_ROOT"),
        Some("/private/tmp/hg-ut-root"),
        "the marker is the root itself"
    );
    // No variable appears twice.
    let mut names: Vec<_> = env.iter().map(|(k, _)| k.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), env.len());
}

#[test]
fn scrubbed_env_passes_only_explicit_credentials() {
    let env = scrubbed_env_from(
        Path::new("/private/tmp/hg-ut-root"),
        vars(&[
            ("ANTHROPIC_API_KEY", "sk-a"),
            ("OPENAI_API_KEY", "sk-o"),
            ("ANTHROPIC_AUTH_TOKEN", "tok"),
            ("AWS_SECRET_ACCESS_KEY", "aws"),
            ("GITHUB_TOKEN", "gh"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "oauth"),
        ]),
    );
    let names: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
    assert!(names.contains(&"ANTHROPIC_API_KEY") && names.contains(&"OPENAI_API_KEY"));
    for k in [
        "ANTHROPIC_AUTH_TOKEN",
        "AWS_SECRET_ACCESS_KEY",
        "GITHUB_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ] {
        assert!(!names.contains(&k), "{k} must not reach private children");
    }
    let none = scrubbed_env_from(Path::new("/private/tmp/hg-ut-root"), vars(&[]));
    assert!(
        none.iter()
            .all(|(k, _)| k != "ANTHROPIC_API_KEY" && k != "OPENAI_API_KEY")
    );
    // A host-provided plugin config dir or test-root marker is replaced, never passed through.
    let host = scrubbed_env_from(
        Path::new("/private/tmp/hg-ut-root"),
        vars(&[
            ("HERDR_PLUGIN_CONFIG_DIR", "/Users/real/plugin-cfg"),
            ("HG_TEST_ROOT", "/Users/real"),
        ]),
    );
    let get = |k: &str| host.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    assert_eq!(
        get("HERDR_PLUGIN_CONFIG_DIR"),
        Some("/private/tmp/hg-ut-root/plugin-config")
    );
    assert_eq!(get("HG_TEST_ROOT"), Some("/private/tmp/hg-ut-root"));
}

#[test]
fn live_resources_cover_the_users_default_session_threads_claude_and_observer() {
    let live = live_resources_from(vars(&[
        ("HOME", "/Users/real"),
        ("HERDR_SOCKET_PATH", "/Users/real/.config/herdr/herdr.sock"),
        ("HERDR_THREADS_STATE", "/Users/real/threads"),
        ("SOMETHING", "/Users/real/memory-observer/db"),
    ]));
    let has = |p: &str| live.iter().any(|(q, _)| q == Path::new(p));
    for p in [
        "/Users/real/.config/herdr",
        "/Users/real/.config/herdr/herdr.sock",
        "/Users/real/.local/state/herdr-threads",
        "/Users/real/threads",
        "/Users/real/.claude",
        "/Users/real/.claude-mem",
        "/Users/real/memory-observer/db",
    ] {
        assert!(has(p), "{p} should be a live resource");
    }
}

// ---------------------------------------------------------------- tripwire (hg-zmi.77)

fn live_for_x() -> Vec<(PathBuf, &'static str)> {
    live_resources_from(vars(&[
        ("HOME", "/Users/x"),
        ("HERDR_SOCKET_PATH", "/Users/x/.config/herdr/herdr.sock"),
    ]))
}

#[test]
fn tripwire_root_mode_rejects_outside_root() {
    let p = Path::new("/Users/x/.config/herdr-graph/config.toml");
    let root = Path::new("/private/tmp/hgt-r");
    let err = tripwire_verdict(p, Some(root), &[]).unwrap_err();
    assert!(
        err.contains("/Users/x/.config/herdr-graph/config.toml"),
        "{err}"
    );
    assert!(err.contains("/private/tmp/hgt-r"), "{err}");
}

#[test]
fn tripwire_root_mode_allows_inside_root() {
    let p = Path::new("/private/tmp/hgt-r/home/.config/herdr-graph/config.toml");
    assert_eq!(
        tripwire_verdict(p, Some(Path::new("/private/tmp/hgt-r")), &[]),
        Ok(())
    );
}

#[test]
fn tripwire_live_mode_rejects_user_graph_config_and_herdr_socket() {
    let live = live_for_x();
    let cfg = tripwire_verdict(
        Path::new("/Users/x/.config/herdr-graph/config.toml"),
        None,
        &live,
    )
    .unwrap_err();
    assert!(cfg.contains("the user's herdr-graph config"), "{cfg}");
    let sock =
        tripwire_verdict(Path::new("/Users/x/.config/herdr/herdr.sock"), None, &live).unwrap_err();
    assert!(sock.contains("Herdr"), "{sock}");
    let claude =
        tripwire_verdict(Path::new("/Users/x/.claude/settings.json"), None, &live).unwrap_err();
    assert!(claude.contains("Claude"), "{claude}");
    assert_eq!(
        tripwire_verdict(Path::new("/private/tmp/hg-ut-root/x"), None, &live),
        Ok(())
    );
}

#[test]
fn armed_unit_tests_panic_on_real_home_config() {
    let home = std::env::var_os("HOME"); // isolation-ok: the tripwire must see the real HOME
    let Some(home) = home.filter(|h| !h.is_empty()) else {
        eprintln!("HOME unset: skipping");
        return;
    };
    let r = std::panic::catch_unwind(|| crate::config::user_config_path(Path::new(&home)));
    assert!(
        r.is_err(),
        "resolving the real ~/.config/herdr-graph must trip the armed tripwire"
    );
}

#[test]
fn armed_unit_tests_panic_on_real_herdr_socket() {
    let home = std::env::var_os("HOME"); // isolation-ok: the tripwire must see the real HOME
    let Some(home) = home.filter(|h| !h.is_empty()) else {
        eprintln!("HOME unset: skipping");
        return;
    };
    let r = std::panic::catch_unwind(|| {
        HerdrClient::new(PathBuf::from(&home).join(".config/herdr/herdr.sock"))
    });
    assert!(
        r.is_err(),
        "constructing a client for the user's live socket must trip the armed tripwire"
    );
}
