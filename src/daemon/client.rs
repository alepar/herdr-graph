//! Synchronous daemon client used by the CLI (spec §1): connect, call, locate-and-ensure.
use super::registry::CallerInfo;
use crate::config::{Env, plugin_config_dir_via_herdr, socket_path};
use crate::ipc::{
    FrameError, IPC_VERSION, IpcCommand, IpcErrorCode, IpcRequest, IpcResponse, IpcResult, read_frame, write_frame,
};
use std::io::IsTerminal;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const EXIT_NO_INSTANCE: u8 = 2;
const ENSURE_TIMEOUT: Duration = super::ensure::STARTUP_WAIT;
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("no instance configured — run herdr-graph init")]
    NoInstance,
    #[error("daemon unavailable: {0}")]
    Unavailable(String),
    #[error("{message}")]
    Remote { code: IpcErrorCode, message: String },
    #[error(transparent)]
    Frame(#[from] FrameError),
}

pub struct Client {
    stream: UnixStream,
}

impl Client {
    /// Connect to the daemon socket; `timeout` bounds every later read and write.
    pub fn connect(socket: &Path, timeout: Duration) -> std::io::Result<Client> {
        let stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Client { stream })
    }

    pub fn call(&mut self, kind: &str, args: serde_json::Value) -> Result<serde_json::Value, ClientError> {
        let request_id = ulid::Ulid::new().to_string();
        let req = IpcRequest {
            version: IPC_VERSION,
            request_id: request_id.clone(),
            command: IpcCommand { kind: kind.to_string(), args },
        };
        write_frame(&mut self.stream, &req)?;
        let resp: IpcResponse = read_frame(&mut self.stream)?;
        if resp.request_id != request_id {
            return Err(ClientError::Unavailable(format!(
                "daemon answered request {} to request {request_id}",
                resp.request_id
            )));
        }
        match resp.result {
            IpcResult::Ok { value } => Ok(value),
            IpcResult::Error { code, message } => Err(ClientError::Remote { code, message }),
        }
    }
}

/// `hello` against a socket path: Some(reply) iff a daemon answers.
pub fn hello(socket: &Path) -> Option<serde_json::Value> {
    Client::connect(socket, Duration::from_secs(2)).ok()?.call("hello", serde_json::json!({})).ok()
}

pub fn caller_info_from_env() -> CallerInfo {
    let env = Env::from_process();
    CallerInfo {
        pane_id: env.pane_id,
        graph_clone: env.graph_clone,
        graph_seat: env.graph_seat,
        cwd: std::env::current_dir().ok(),
        tty: std::io::stdin().is_terminal(),
        herdr_socket: env.herdr_socket,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallMode {
    /// mutating, seat, undo, request commands (spec §1)
    Ensure,
    /// reads: never start a daemon
    NoEnsure,
}

/// Locate instance → connect → on failure with CallMode::Ensure: if HERDR_SOCKET_PATH is unset →
/// Unavailable("not inside Herdr"); else run ensure (STARTUP_WAIT) once and retry once; still failing → Unavailable(<reason>).
/// Verifies `hello.herdr_socket` matches this process's HERDR_SOCKET_PATH when both are set (decision 1).
/// Injects args["_caller"].
pub fn call_daemon(
    kind: &str,
    mut args: serde_json::Value,
    mode: CallMode,
) -> Result<serde_json::Value, ClientError> {
    let env = Env::from_process();
    let (root, _) = crate::config::locate_instance(&env, &plugin_config_dir_via_herdr).ok_or(ClientError::NoInstance)?;
    if mode == CallMode::Ensure && env.herdr_socket.is_none() {
        return Err(ClientError::Unavailable("not inside Herdr".into()));
    }
    let socket = socket_path(&root);
    let mut client = match Client::connect(&socket, CALL_TIMEOUT) {
        Ok(c) => c,
        Err(first) => {
            if mode == CallMode::NoEnsure {
                return Err(ClientError::Unavailable(format!("not running ({first})")));
            }
            super::ensure::ensure(&env, ENSURE_TIMEOUT).map_err(|e| ClientError::Unavailable(format!("{e:#}")))?;
            Client::connect(&socket, CALL_TIMEOUT)
                .map_err(|e| ClientError::Unavailable(format!("not running after ensure ({e})")))?
        }
    };
    if let Some(mine) = &env.herdr_socket {
        let reply = client.call("hello", serde_json::json!({}))?;
        let theirs = reply["herdr_socket"].as_str().map(PathBuf::from);
        if theirs.as_deref() != Some(mine.as_path()) {
            return Err(ClientError::Unavailable(format!(
                "daemon for this instance is attached to Herdr socket {}; this pane uses {}",
                theirs.map(|p| p.display().to_string()).unwrap_or_else(|| "<unknown>".into()),
                mine.display()
            )));
        }
    }
    if let Some(obj) = args.as_object_mut() {
        obj.insert("_caller".into(), serde_json::to_value(caller_info_from_env()).expect("caller info serializes"));
    }
    client.call(kind, args)
}

/// For read-only commands: Some(instance root), or None after printing "no instance configured — run herdr-graph init"
/// (the caller exits `EXIT_NO_INSTANCE`).
pub fn instance_or_none() -> Option<PathBuf> {
    let env = Env::from_process();
    match crate::config::locate_instance(&env, &plugin_config_dir_via_herdr) {
        Some((root, _)) => Some(root),
        None => {
            eprintln!("no instance configured — run herdr-graph init");
            None
        }
    }
}
