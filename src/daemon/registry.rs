//! Component registry: command handlers, background loops and status providers (spec §1).
//! Feature modules expose `register_commands(reg: &mut Registry, …)`; only `compose::compose` calls them.
use crate::ipc::IpcErrorCode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

pub type BoxFut<T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Who is calling: filled by the CLI client from its environment, injected as `args["_caller"]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CallerInfo {
    pub pane_id: Option<String>,
    pub graph_clone: Option<String>,
    pub graph_seat: Option<String>,
    pub cwd: Option<PathBuf>,
    pub tty: bool,
    pub herdr_socket: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CommandCtx {
    pub request_id: String,
    pub caller: CallerInfo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandError {
    pub code: IpcErrorCode,
    pub message: String,
}

impl CommandError {
    pub fn bad_request(m: impl Into<String>) -> Self {
        Self { code: IpcErrorCode::BadRequest, message: m.into() }
    }
    pub fn rejected(m: impl Into<String>) -> Self {
        Self { code: IpcErrorCode::Rejected, message: m.into() }
    }
    pub fn unavailable(m: impl Into<String>) -> Self {
        Self { code: IpcErrorCode::Unavailable, message: m.into() }
    }
    pub fn internal(m: impl Into<String>) -> Self {
        Self { code: IpcErrorCode::Internal, message: m.into() }
    }
}

pub trait CommandHandler: Send + Sync + 'static {
    fn call(&self, cx: CommandCtx, args: serde_json::Value) -> BoxFut<Result<serde_json::Value, CommandError>>;
}

impl<F, Fut> CommandHandler for F
where
    F: Fn(CommandCtx, serde_json::Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<serde_json::Value, CommandError>> + Send + 'static,
{
    fn call(&self, cx: CommandCtx, args: serde_json::Value) -> BoxFut<Result<serde_json::Value, CommandError>> {
        Box::pin(self(cx, args))
    }
}

/// Shutdown signal handed to every background loop.
#[derive(Clone)]
pub struct Shutdown(tokio::sync::watch::Receiver<bool>);

impl Shutdown {
    pub fn is_set(&self) -> bool {
        *self.0.borrow()
    }
    /// Resolves once shutdown is requested (or the sender is gone).
    pub async fn wait(&mut self) {
        loop {
            if *self.0.borrow_and_update() {
                return;
            }
            if self.0.changed().await.is_err() {
                return;
            }
        }
    }
    pub fn receiver(&self) -> tokio::sync::watch::Receiver<bool> {
        self.0.clone()
    }
}

/// A shutdown channel: send `true` on the sender to stop everything holding the `Shutdown`.
pub fn shutdown_channel() -> (Arc<tokio::sync::watch::Sender<bool>>, Shutdown) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    (Arc::new(tx), Shutdown(rx))
}

pub type StatusFn = Arc<dyn Fn() -> serde_json::Value + Send + Sync>;
pub type LoopFn = Box<dyn FnOnce(Shutdown) -> BoxFut<anyhow::Result<()>> + Send>;

#[derive(Default)]
pub struct Registry {
    commands: BTreeMap<String, Arc<dyn CommandHandler>>,
    // Mutex only so `Registry` stays `Sync` (FnOnce boxes are not); never contended.
    loops: std::sync::Mutex<Vec<(String, LoopFn)>>,
    status: Vec<(String, StatusFn)>,
}

impl Registry {
    /// Register the handler for IPC command `kind` (`"<group>.<verb>"`). Panics on a duplicate kind.
    pub fn command(&mut self, kind: &str, h: impl CommandHandler) {
        let prev = self.commands.insert(kind.to_string(), Arc::new(h));
        assert!(prev.is_none(), "duplicate command kind {kind:?}");
    }

    /// Register a background loop; it is spawned at daemon start and must return when `Shutdown` fires.
    pub fn background<F, Fut>(&mut self, name: &str, f: F)
    where
        F: FnOnce(Shutdown) -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let item: (String, LoopFn) = (name.to_string(), Box::new(move |sd| Box::pin(f(sd))));
        self.loops.get_mut().unwrap_or_else(|e| e.into_inner()).push(item);
    }

    /// Contribute a value to the `status` reply under `components.<name>`.
    pub fn status_provider(&mut self, name: &str, f: StatusFn) {
        self.status.push((name.to_string(), f));
    }

    pub fn command_kinds(&self) -> Vec<String> {
        self.commands.keys().cloned().collect()
    }

    pub fn handler(&self, kind: &str) -> Option<Arc<dyn CommandHandler>> {
        self.commands.get(kind).cloned()
    }

    pub fn take_loops(&mut self) -> Vec<(String, LoopFn)> {
        std::mem::take(self.loops.get_mut().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn status_components(&self) -> serde_json::Map<String, serde_json::Value> {
        self.status.iter().map(|(n, f)| (n.clone(), f())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "duplicate command kind")]
    fn duplicate_kind_panics() {
        let mut r = Registry::default();
        r.command("a.b", |_cx: CommandCtx, _a: serde_json::Value| async { Ok(serde_json::json!(1)) });
        r.command("a.b", |_cx: CommandCtx, _a: serde_json::Value| async { Ok(serde_json::json!(2)) });
    }

    #[test]
    fn command_kinds_sorted() {
        let mut r = Registry::default();
        for k in ["z.z", "a.a"] {
            r.command(k, |_cx: CommandCtx, _a: serde_json::Value| async { Ok(serde_json::Value::Null) });
        }
        assert_eq!(r.command_kinds(), vec!["a.a", "z.z"]);
    }
}
