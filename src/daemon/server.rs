//! UDS IPC server: 4-byte BE length JSON frames, dispatch to built-ins and registered command handlers.
use super::budget;
use super::registry::{CallerInfo, CommandCtx, CommandError, Registry, Shutdown};
use crate::ipc::{
    DaemonPhase, IPC_VERSION, IpcErrorCode, IpcRequest, IpcResponse, IpcResult, read_frame_async, write_frame_async,
};
use crate::model::Timestamp;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{UnixListener, UnixStream};

/// Facts the built-in commands (`hello`, `status`, `shutdown`) report.
pub struct Builtins {
    pub instance: PathBuf,
    pub herdr_socket: PathBuf,
    pub started_at: Timestamp,
    pub shutdown_tx: Arc<tokio::sync::watch::Sender<bool>>,
}

/// Between bind and the end of compose, requests other than the built-ins wait here for the composed registry.
#[derive(Clone)]
pub struct StartGate(Arc<tokio::sync::watch::Sender<GateState>>);

#[derive(Clone)]
pub enum GateState {
    Starting,
    Ready(Arc<Registry>),
    Failed(String),
}

impl StartGate {
    pub fn starting() -> Self {
        StartGate(Arc::new(tokio::sync::watch::channel(GateState::Starting).0))
    }

    pub fn ready(&self, reg: Arc<Registry>) {
        self.0.send_replace(GateState::Ready(reg));
    }

    pub fn fail(&self, why: impl Into<String>) {
        self.0.send_replace(GateState::Failed(why.into()));
    }

    pub fn phase(&self) -> DaemonPhase {
        match &*self.0.borrow() {
            GateState::Starting => DaemonPhase::Starting,
            GateState::Ready(_) => DaemonPhase::Ready,
            GateState::Failed(_) => DaemonPhase::Failed,
        }
    }

    /// Wait for Ready/Failed up to `hold`; None on timeout.
    async fn wait(&self, hold: Duration) -> Option<GateState> {
        let mut rx = self.0.subscribe();
        let settled = async {
            loop {
                let state = rx.borrow_and_update().clone();
                if !matches!(state, GateState::Starting) {
                    return Some(state);
                }
                if rx.changed().await.is_err() {
                    return None;
                }
            }
        };
        tokio::time::timeout(hold, settled).await.ok().flatten()
    }
}

/// How long a request is held while the daemon starts; derived from the one request budget.
pub use super::budget::START_HOLD;

/// Unchanged signature for existing callers/tests: a gate that is ready from the start.
pub async fn serve(listener: UnixListener, registry: Arc<Registry>, builtins: Builtins, shutdown: Shutdown) {
    let gate = StartGate::starting();
    gate.ready(registry);
    serve_gated(listener, gate, builtins, shutdown, START_HOLD).await
}

pub async fn serve_gated(
    listener: UnixListener,
    gate: StartGate,
    builtins: Builtins,
    mut shutdown: Shutdown,
    hold: Duration,
) {
    let builtins = Arc::new(builtins);
    loop {
        tokio::select! {
            _ = shutdown.wait() => return,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    tokio::spawn(connection(stream, gate.clone(), builtins.clone(), shutdown.clone(), hold));
                }
                Err(e) => {
                    eprintln!("herdr-graph daemon: accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            },
        }
    }
}

async fn connection(
    mut stream: UnixStream,
    gate: StartGate,
    builtins: Arc<Builtins>,
    mut shutdown: Shutdown,
    hold: Duration,
) {
    loop {
        let req = tokio::select! {
            _ = shutdown.wait() => return,
            r = read_frame_async::<_, IpcRequest>(&mut stream) => match r {
                Ok(Some(req)) => req,
                Ok(None) => return,
                Err(e) => {
                    eprintln!("herdr-graph daemon: bad frame: {e}");
                    return;
                }
            },
        };
        let deadline = tokio::time::Instant::now() + budget::SERVER_BUDGET;
        let request_id = req.request_id.clone();
        let result = tokio::select! {
            _ = shutdown.wait() => return,
            r = dispatch(&gate, &builtins, req, hold, deadline) => r,
        };
        let resp = IpcResponse { version: IPC_VERSION, request_id, result };
        if write_frame_async(&mut stream, &resp).await.is_err() {
            return;
        }
    }
}

fn err(code: IpcErrorCode, message: impl Into<String>) -> IpcResult {
    IpcResult::Error { code, message: message.into() }
}

async fn dispatch(
    gate: &StartGate,
    b: &Builtins,
    req: IpcRequest,
    hold: Duration,
    deadline: tokio::time::Instant,
) -> IpcResult {
    if req.version != IPC_VERSION {
        return err(
            IpcErrorCode::VersionMismatch,
            format!("request version {} but daemon speaks {IPC_VERSION}", req.version),
        );
    }
    let kind = req.command.kind.as_str();
    match kind {
        "hello" => {
            return IpcResult::Ok {
                value: serde_json::json!({
                    "version": IPC_VERSION,
                    "pid": std::process::id(),
                    "instance": b.instance,
                    "herdr_socket": b.herdr_socket,
                    "started_at": b.started_at,
                    "state": gate.phase(),
                }),
            };
        }
        "status" => {
            let uptime = (chrono::Utc::now() - b.started_at).num_seconds().max(0);
            return IpcResult::Ok {
                value: serde_json::json!({
                    "pid": std::process::id(),
                    "instance": b.instance,
                    "herdr_socket": b.herdr_socket,
                    "uptime_secs": uptime,
                    "state": gate.phase(),
                    "components": match &*gate.0.borrow() {
                        GateState::Ready(reg) => reg.status_components(),
                        _ => serde_json::Map::new(),
                    },
                }),
            };
        }
        "shutdown" => {
            let _ = b.shutdown_tx.send(true);
            return IpcResult::Ok { value: serde_json::json!({"ok": true}) };
        }
        _ => {}
    }
    let hold = hold.min(deadline.saturating_duration_since(tokio::time::Instant::now()));
    let registry = match gate.wait(hold).await {
        Some(GateState::Ready(reg)) => reg,
        Some(GateState::Failed(why)) => {
            return err(IpcErrorCode::Unavailable, format!("daemon failed to start: {why}"));
        }
        _ => return err(IpcErrorCode::Unavailable, "daemon is still starting (first pass); retry shortly"),
    };
    dispatch_registered_within(&registry, req.request_id, kind, req.command.args, Some(deadline)).await
}

/// Strip `_caller` into `CommandCtx` and run the registered handler: the exact path every IPC command takes.
pub async fn dispatch_registered(registry: &Registry, request_id: String, kind: &str, args: serde_json::Value) -> IpcResult {
    dispatch_registered_within(registry, request_id, kind, args, None).await
}

/// `dispatch_registered` with the request's deadline in scope for the handler (none: an internal caller).
async fn dispatch_registered_within(
    registry: &Registry,
    request_id: String,
    kind: &str,
    mut args: serde_json::Value,
    deadline: Option<tokio::time::Instant>,
) -> IpcResult {
    let Some(handler) = registry.handler(kind) else {
        return err(IpcErrorCode::UnknownCommand, format!("unknown command {kind:?}"));
    };
    let caller = match args.as_object_mut().and_then(|o| o.remove("_caller")) {
        None => CallerInfo::default(),
        Some(v) => match serde_json::from_value(v) {
            Ok(c) => c,
            Err(e) => return err(IpcErrorCode::BadRequest, format!("malformed _caller: {e}")),
        },
    };
    let cx = CommandCtx { request_id, caller };
    let outcome = match deadline {
        Some(deadline) => budget::within(deadline, handler.call(cx, args)).await,
        None => handler.call(cx, args).await,
    };
    match outcome {
        Ok(value) => IpcResult::Ok { value },
        Err(CommandError { code, message }) => IpcResult::Error { code, message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::registry::shutdown_channel;
    use crate::ipc::IpcCommand;
    use serde_json::json;

    struct Harness {
        sock: PathBuf,
        tx: Arc<tokio::sync::watch::Sender<bool>>,
        _dir: tempfile::TempDir,
    }

    async fn start(mut reg: Registry) -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("s.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, sd) = shutdown_channel();
        let loops = reg.take_loops();
        for (_, f) in loops {
            tokio::spawn(f(sd.clone()));
        }
        let builtins = Builtins {
            instance: "/inst".into(),
            herdr_socket: "/herdr".into(),
            started_at: chrono::Utc::now(),
            shutdown_tx: tx.clone(),
        };
        tokio::spawn(serve(listener, Arc::new(reg), builtins, sd));
        Harness { sock, tx, _dir: dir }
    }

    async fn start_gated(gate: StartGate, hold: Duration) -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("s.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, sd) = shutdown_channel();
        let builtins = Builtins {
            instance: "/inst".into(),
            herdr_socket: "/herdr".into(),
            started_at: chrono::Utc::now(),
            shutdown_tx: tx.clone(),
        };
        tokio::spawn(serve_gated(listener, gate, builtins, sd, hold));
        Harness { sock, tx, _dir: dir }
    }

    fn request(version: u32, id: &str, kind: &str, args: serde_json::Value) -> IpcRequest {
        IpcRequest { version, request_id: id.into(), command: IpcCommand { kind: kind.into(), args } }
    }

    async fn roundtrip(s: &mut UnixStream, req: IpcRequest) -> IpcResponse {
        write_frame_async(s, &req).await.unwrap();
        read_frame_async(s).await.unwrap().unwrap()
    }

    fn echo_registry() -> Registry {
        let mut reg = Registry::default();
        reg.command("test.echo", |cx: CommandCtx, args: serde_json::Value| async move {
            Ok(json!({"args": args, "caller": cx.caller, "request_id": cx.request_id}))
        });
        reg
    }

    #[tokio::test]
    async fn serve_dispatches_registered_command() {
        let h = start(echo_registry()).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let resp = roundtrip(&mut s, request(IPC_VERSION, "r1", "test.echo", json!({"x": 7}))).await;
        assert_eq!(resp.request_id, "r1");
        let IpcResult::Ok { value } = resp.result else { panic!("{resp:?}") };
        assert_eq!(value["args"], json!({"x": 7}));
        assert_eq!(value["request_id"], "r1");
    }

    #[tokio::test]
    async fn unknown_command_errors() {
        let h = start(echo_registry()).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let resp = roundtrip(&mut s, request(IPC_VERSION, "r", "nope.nope", json!({}))).await;
        assert!(matches!(resp.result, IpcResult::Error { code: IpcErrorCode::UnknownCommand, .. }), "{resp:?}");
    }

    #[tokio::test]
    async fn version_mismatch_errors() {
        let h = start(echo_registry()).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let resp = roundtrip(&mut s, request(IPC_VERSION + 1, "r", "hello", json!({}))).await;
        assert!(matches!(resp.result, IpcResult::Error { code: IpcErrorCode::VersionMismatch, .. }), "{resp:?}");
    }

    #[tokio::test]
    async fn caller_info_injected_and_stripped() {
        let h = start(echo_registry()).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let args = json!({"keep": 1, "_caller": {"pane_id": "p9", "graph_seat": "st_a", "tty": true}});
        let resp = roundtrip(&mut s, request(IPC_VERSION, "r", "test.echo", args)).await;
        let IpcResult::Ok { value } = resp.result else { panic!("{resp:?}") };
        assert_eq!(value["args"], json!({"keep": 1}), "_caller must be stripped from handler args");
        assert_eq!(value["caller"]["pane_id"], "p9");
        assert_eq!(value["caller"]["graph_seat"], "st_a");
        assert_eq!(value["caller"]["tty"], true);
        // malformed _caller is a bad request, not a silent default
        let resp = roundtrip(&mut s, request(IPC_VERSION, "r2", "test.echo", json!({"_caller": 5}))).await;
        assert!(matches!(resp.result, IpcResult::Error { code: IpcErrorCode::BadRequest, .. }), "{resp:?}");
    }

    #[tokio::test]
    async fn multiple_requests_per_connection() {
        let h = start(echo_registry()).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        for i in 0..3 {
            let id = format!("r{i}");
            let resp = roundtrip(&mut s, request(IPC_VERSION, &id, "test.echo", json!({"i": i}))).await;
            assert_eq!(resp.request_id, id);
            let IpcResult::Ok { value } = resp.result else { panic!() };
            assert_eq!(value["args"]["i"], i);
        }
        let resp = roundtrip(&mut s, request(IPC_VERSION, "st", "status", json!({}))).await;
        let IpcResult::Ok { value } = resp.result else { panic!() };
        assert_eq!(value["instance"], "/inst");
    }

    #[tokio::test]
    async fn background_loop_receives_shutdown() {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let mut reg = Registry::default();
        reg.background("probe", move |mut sd: Shutdown| async move {
            sd.wait().await;
            let _ = done_tx.send(());
            Ok(())
        });
        let h = start(reg).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let resp = roundtrip(&mut s, request(IPC_VERSION, "r", "shutdown", json!({}))).await;
        assert!(matches!(resp.result, IpcResult::Ok { .. }));
        tokio::time::timeout(std::time::Duration::from_secs(2), done_rx)
            .await
            .expect("loop saw shutdown")
            .unwrap();
        assert!(*h.tx.borrow());
    }

    #[tokio::test]
    async fn hello_reports_starting_then_ready() {
        let gate = StartGate::starting();
        let h = start_gated(gate.clone(), Duration::from_secs(5)).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let resp = roundtrip(&mut s, request(IPC_VERSION, "h1", "hello", json!({}))).await;
        let IpcResult::Ok { value } = resp.result else { panic!("{resp:?}") };
        assert_eq!(value["state"], "starting");
        assert_eq!(value["herdr_socket"], "/herdr", "hello keeps its other fields while starting");
        let resp = roundtrip(&mut s, request(IPC_VERSION, "st", "status", json!({}))).await;
        let IpcResult::Ok { value } = resp.result else { panic!("{resp:?}") };
        assert_eq!(value["state"], "starting");
        assert_eq!(value["components"], json!({}));
        gate.ready(Arc::new(echo_registry()));
        let resp = roundtrip(&mut s, request(IPC_VERSION, "h2", "hello", json!({}))).await;
        let IpcResult::Ok { value } = resp.result else { panic!("{resp:?}") };
        assert_eq!(value["state"], "ready");
    }

    #[tokio::test]
    async fn command_held_until_ready_then_served() {
        let gate = StartGate::starting();
        let h = start_gated(gate.clone(), Duration::from_secs(10)).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let g2 = gate.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            g2.ready(Arc::new(echo_registry()));
        });
        let began = std::time::Instant::now();
        let resp = roundtrip(&mut s, request(IPC_VERSION, "e1", "test.echo", json!({"x": 1}))).await;
        assert!(began.elapsed() >= Duration::from_millis(150), "request must wait for ready");
        let IpcResult::Ok { value } = resp.result else { panic!("{resp:?}") };
        assert_eq!(value["args"], json!({"x": 1}));
    }

    #[tokio::test]
    async fn held_command_times_out_unavailable() {
        let h = start_gated(StartGate::starting(), Duration::from_millis(200)).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let resp = roundtrip(&mut s, request(IPC_VERSION, "e1", "test.echo", json!({}))).await;
        let IpcResult::Error { code, message } = resp.result else { panic!("{resp:?}") };
        assert_eq!(code, IpcErrorCode::Unavailable);
        assert!(message.contains("still starting"), "{message}");
    }

    fn deadline_registry() -> Registry {
        let mut reg = Registry::default();
        reg.command("test.deadline", |_cx: CommandCtx, _args: serde_json::Value| async move {
            let d = budget::request_deadline().ok_or_else(|| CommandError::internal("no request deadline"))?;
            let left = d.saturating_duration_since(tokio::time::Instant::now());
            Ok(json!({"left_ms": left.as_millis() as u64}))
        });
        reg
    }

    #[tokio::test]
    async fn held_request_keeps_within_budget() {
        // Gate never ready: answers "still starting" at the (test) hold, not later.
        let h = start_gated(StartGate::starting(), Duration::from_millis(300)).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let began = std::time::Instant::now();
        let resp = roundtrip(&mut s, request(IPC_VERSION, "d0", "test.deadline", json!({}))).await;
        let IpcResult::Error { code, message } = resp.result else { panic!("{resp:?}") };
        assert_eq!(code, IpcErrorCode::Unavailable);
        assert!(message.contains("still starting"), "{message}");
        assert!(began.elapsed() < Duration::from_secs(2), "{:?}", began.elapsed());

        // Gate ready after 200 ms: the handler's deadline counts from frame receipt, not from readiness.
        let gate = StartGate::starting();
        let h = start_gated(gate.clone(), Duration::from_secs(10)).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            gate.ready(Arc::new(deadline_registry()));
        });
        let resp = roundtrip(&mut s, request(IPC_VERSION, "d1", "test.deadline", json!({}))).await;
        let IpcResult::Ok { value } = resp.result else { panic!("{resp:?}") };
        let left = Duration::from_millis(value["left_ms"].as_u64().unwrap());
        assert!(left > Duration::ZERO, "deadline must be in the future");
        assert!(left <= budget::SERVER_BUDGET - Duration::from_millis(150), "hold time must be charged: {left:?}");
    }

    #[tokio::test]
    async fn failed_start_releases_held_command() {
        let gate = StartGate::starting();
        let h = start_gated(gate.clone(), Duration::from_secs(10)).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            gate.fail("boom");
        });
        let resp = roundtrip(&mut s, request(IPC_VERSION, "e1", "test.echo", json!({}))).await;
        let IpcResult::Error { code, message } = resp.result else { panic!("{resp:?}") };
        assert_eq!(code, IpcErrorCode::Unavailable);
        assert!(message.contains("boom"), "{message}");
    }

    #[tokio::test]
    async fn shutdown_answered_while_starting() {
        let h = start_gated(StartGate::starting(), Duration::from_secs(10)).await;
        let mut s = UnixStream::connect(&h.sock).await.unwrap();
        let resp = roundtrip(&mut s, request(IPC_VERSION, "sd", "shutdown", json!({}))).await;
        assert!(matches!(resp.result, IpcResult::Ok { .. }), "{resp:?}");
        assert!(*h.tx.borrow());
    }
}
