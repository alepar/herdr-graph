//! Daemon process model, IPC server, component registry (spec §1). Owned by hg-zmi.4.
use crate::config::InstancePaths;
use crate::model::Timestamp;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub mod budget;
pub mod client;
pub mod compose;
pub mod doctor;
pub mod ensure;
pub mod lock;
pub mod registry;
pub mod server;

#[derive(Debug, Clone)]
pub struct DaemonCtx {
    pub paths: InstancePaths,
    pub herdr_socket: PathBuf,
    pub started_at: Timestamp,
    /// `${CLAUDE_CONFIG_DIR:-~/.claude}`, resolved once by `run_foreground`; tests pass `<temp root>/claude`.
    pub claude_root: PathBuf,
}

/// Foreground daemon (spec §1): require a reachable Herdr socket (UnixStream::connect) else error
/// "daemon unavailable: not inside Herdr" / "Herdr socket <p> not reachable"; GitStore::open(instance) must succeed;
/// mkdir .graph-local; DaemonLock::try_acquire (held ⇒ print "daemon already running (pid N)" and return Ok);
/// remove a stale socket file; bind; build a multi-thread tokio runtime; serve (`hello` answers "starting"
/// and other requests are held); `compose(&mut reg, &ctx).await?`; spawn every registered loop with a Shutdown;
/// mark ready (order: bind -> serve (starting) -> compose -> ready); serve until SIGTERM/SIGINT (tokio::signal) or `shutdown`;
/// then flip shutdown, await loops (5 s timeout each), remove the socket file, drop the lock.
pub fn run_foreground(instance: &Path, herdr_socket: Option<&Path>) -> anyhow::Result<()> {
    use anyhow::Context;
    let herdr_socket = herdr_socket.ok_or_else(|| anyhow::anyhow!("daemon unavailable: not inside Herdr"))?;
    if let Err(e) = std::os::unix::net::UnixStream::connect(herdr_socket) {
        anyhow::bail!("daemon unavailable: Herdr socket {} not reachable: {e}", herdr_socket.display());
    }
    crate::store::GitStore::open(instance).with_context(|| format!("instance {}", instance.display()))?;

    let paths = InstancePaths::new(instance);
    std::fs::create_dir_all(&paths.local)?;
    if let Some(parent) = paths.socket.parent() {
        std::fs::create_dir_all(parent)?;
        if !parent.starts_with(&paths.root) {
            // /private/tmp fallback directory: owner-only.
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let started_at = chrono::Utc::now();
    let info = lock::LockInfo {
        pid: std::process::id(),
        herdr_socket: herdr_socket.to_path_buf(),
        socket: paths.socket.clone(),
        started_at,
    };
    let Some(_lock) = lock::DaemonLock::try_acquire(&paths.lock, &info)? else {
        match lock::read_info(&paths.lock) {
            Some(i) => println!("daemon already running (pid {})", i.pid),
            None => println!("daemon already running"),
        }
        return Ok(());
    };
    match std::fs::remove_file(&paths.socket) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context("removing stale socket"),
    }

    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let claude_root = crate::model::harness::claude_config_root(
        std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).as_deref(),
        &home,
    );
    let ctx = DaemonCtx { paths: paths.clone(), herdr_socket: herdr_socket.to_path_buf(), started_at, claude_root };
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let result = rt.block_on(run_async(ctx));
    let _ = std::fs::remove_file(&paths.socket);
    rt.shutdown_timeout(Duration::from_secs(1));
    result
}

async fn run_async(ctx: DaemonCtx) -> anyhow::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let listener = tokio::net::UnixListener::bind(&ctx.paths.socket)
        .map_err(|e| anyhow::anyhow!("binding {}: {e}", ctx.paths.socket.display()))?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    let (tx, mut shutdown) = registry::shutdown_channel();
    let builtins = server::Builtins {
        instance: ctx.paths.root.clone(),
        herdr_socket: ctx.herdr_socket.clone(),
        started_at: ctx.started_at,
        shutdown_tx: tx.clone(),
    };
    // Serve before compose: `hello` answers "starting" at once and other requests wait on the gate (hg-zmi.55).
    let gate = server::StartGate::starting();
    let server = tokio::spawn(server::serve_gated(
        listener,
        gate.clone(),
        builtins,
        shutdown.clone(),
        server::START_HOLD,
    ));

    let mut reg = registry::Registry::default();
    let composed = tokio::select! {
        r = compose::compose(&mut reg, &ctx) => Some(r),
        _ = sigterm.recv() => None,
        _ = sigint.recv() => None,
        _ = shutdown.wait() => None,
    };
    let loops: Vec<_> = match composed {
        None => {
            gate.fail("daemon stopped during startup");
            let _ = tx.send(true);
            let _ = tokio::time::timeout(Duration::from_secs(5), server).await;
            return Ok(());
        }
        Some(Err(e)) => {
            gate.fail(format!("{e:#}"));
            let _ = tx.send(true);
            let _ = tokio::time::timeout(Duration::from_secs(5), server).await;
            return Err(e);
        }
        Some(Ok(())) => reg
            .take_loops()
            .into_iter()
            .map(|(name, f)| (name, tokio::spawn(f(shutdown.clone()))))
            .collect(),
    };
    gate.ready(Arc::new(reg));

    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
        _ = shutdown.wait() => {}
    }
    let _ = tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), server).await;
    for (name, handle) in loops {
        match tokio::time::timeout(Duration::from_secs(5), handle).await {
            Err(_) => eprintln!("herdr-graph daemon: loop {name} did not stop within 5s"),
            Ok(Ok(Err(e))) => eprintln!("herdr-graph daemon: loop {name} failed: {e:#}"),
            Ok(Err(e)) => eprintln!("herdr-graph daemon: loop {name} panicked: {e}"),
            Ok(Ok(Ok(()))) => {}
        }
    }
    Ok(())
}
