//! Opt-in tier-3 test of the real threads adapter against a real herdr-threads daemon and a private Herdr
//! (spec §11). Runs only with `HG_REAL_THREADS=1`:
//!
//!   HG_REAL_THREADS=1 cargo test --features private-herdr --test threads_real -- --nocapture
//!
//! The daemon uses a threads state directory under the private root; every path is checked by the isolation
//! guard before anything starts, and teardown stops only the daemon this test started (verified by pid, argv
//! and environment) and the private Herdr.
#![cfg(feature = "private-herdr")]

mod support;

use herdr_graph::herdr::isolation::IsolationError;
use herdr_graph::model::HerdrPaneId;
use herdr_graph::model::clone::{InvitationState, InviteConstraint};
use herdr_graph::ports::herdr::{CreateWorkspace, HerdrApi};
use herdr_graph::ports::threads::*;
use herdr_graph::threads::{PaneSeatMap, ServiceThreads, ThreadsSeatMap, discover};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::private_herdr::PrivateHerdr;

fn enabled() -> bool {
    std::env::var("HG_REAL_THREADS").as_deref() == Ok("1")
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The herdr-threads binary built from the symlinked checkout, into this worktree's target directory.
fn threads_binary() -> PathBuf {
    let target = crate_root().join("target/threads-bin");
    let bin = target.join("debug/herdr-threads");
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--manifest-path"])
        .arg(crate_root().join("third_party/herdr-threads/Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .stdin(Stdio::null())
        .status()
        .expect("cargo runs");
    assert!(status.success(), "cargo build of herdr-threads failed");
    assert!(bin.is_file(), "{} missing after the build", bin.display());
    bin
}

/// A herdr-threads daemon started (via `daemon ensure`) in the private environment; stopped on drop.
struct ThreadsDaemon<'a> {
    herdr: &'a PrivateHerdr,
    bin: PathBuf,
    state: PathBuf,
}

impl<'a> ThreadsDaemon<'a> {
    fn cli(&self, pane: Option<&HerdrPaneId>, args: &[&str]) -> Output {
        let mut c = self.herdr.command(&self.bin);
        c.args(["--state-dir"]).arg(&self.state).arg("--host-endpoint").arg(&self.herdr.socket);
        c.args(args);
        if let Some(p) = pane {
            c.env("HERDR_PANE_ID", &p.0);
        }
        c.stdin(Stdio::null()).output().expect("herdr-threads runs")
    }

    fn start(herdr: &'a PrivateHerdr, bin: PathBuf) -> Self {
        Self::start_at(herdr, bin, herdr.root.join("threads-state"))
    }

    fn start_at(herdr: &'a PrivateHerdr, bin: PathBuf, state: PathBuf) -> Self {
        herdr.guard().check(&state).expect("threads state is inside the private root");
        herdr.guard().check(&herdr.socket).expect("host endpoint is private");
        let d = Self { herdr, bin, state };
        let out = d.cli(None, &["daemon", "ensure"]);
        assert!(out.status.success(), "daemon ensure: {}", String::from_utf8_lossy(&out.stderr));
        d
    }
}

impl Drop for ThreadsDaemon<'_> {
    fn drop(&mut self) {
        // `daemon stop` only talks to the daemon this state directory belongs to.
        let _ = self.cli(None, &["daemon", "stop"]);
    }
}

async fn poll<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn ok(what: &str, o: &Output) {
    assert!(o.status.success(), "{what}: {} {}", stdout(o), String::from_utf8_lossy(&o.stderr));
}

#[tokio::test(flavor = "multi_thread")]
async fn real_threads_channel_invite_membership_notify_release() {
    if !enabled() {
        support::skip("set HG_REAL_THREADS=1");
        return;
    }
    let herdr = match PrivateHerdr::start() {
        Ok(h) => h,
        Err(e) if support::herdr_binary().is_none() => {
            support::skip(&format!("{e}"));
            return;
        }
        Err(e) => panic!("private herdr failed to start: {e:#}"),
    };
    let daemon = ThreadsDaemon::start(&herdr, threads_binary());

    // The graph's view of the daemon: discovered from its published descriptor, never the user's live one.
    let found = discover(&daemon.state, &herdr.socket).expect("daemon published its endpoint");
    // The daemon binds a short per-instance socket under /private/tmp/herdr-threads-<uid>/ when the state
    // path is too long for a socket address; the name is a hash of the private instance directory. Anything
    // else must be inside the private root, and nothing may be a live user resource.
    match herdr.guard().check(&found.socket) {
        Ok(()) => {}
        Err(IsolationError::OutsideRoot(p, _)) if p.to_string_lossy().starts_with("/private/tmp/herdr-threads-") => {}
        Err(e) => panic!("threads socket {} is not private: {e}", found.socket.display()),
    }
    let intents = herdr.root.join("graph").join(".graph-local").join("threads-intents");
    herdr.guard().check(&intents).expect("intents dir is private");
    let threads = ServiceThreads::with_system_clock(found.socket.clone(), intents, found.instance).unwrap();
    let clock: Arc<dyn herdr_threads::protocol::time::Clock> = Arc::new(herdr_threads::app::SystemClock::new());
    let map = ThreadsSeatMap::new(found.socket.clone(), found.instance, clock);

    // A pane in the private Herdr, registered as a threads seat (a person's seat: `me init`).
    let client = herdr.client();
    let created = client
        .create_workspace(CreateWorkspace { label: "threads-real".into(), cwd: herdr.root.join("home"), env: vec![] })
        .await
        .expect("workspace");
    let pane = created.pane.clone().expect("first pane");
    let init = daemon.cli(Some(&pane), &["me", "init"]);
    ok("me init", &init);
    let seat = poll("the pane's threads seat", async || map.seat_for(&pane).await.ok().flatten()).await;
    assert!(seat.0.starts_with("seat-"), "{seat:?}");

    // EnsureThread: idempotent by operation key.
    let key = OpKey("ensure:st_REALTEST".into());
    let thread = threads.ensure_thread(ChannelScope::Seat, "alpha/foreman", &key).await.expect("ensure");
    assert_eq!(thread, ThreadRef("hg-st_realtest".into()));
    assert_eq!(threads.ensure_thread(ChannelScope::Seat, "alpha/foreman", &key).await.unwrap(), thread);
    assert_eq!(threads.membership(&thread, &seat).await.unwrap(), None, "nobody invited yet");
    threads.set_topic(&thread, "alpha/boss", &OpKey("topic:real:1".into())).await.expect("set topic");

    // Invite Required: pending until the native side accepts; the graph side never accepts.
    threads.invite(&thread, &seat, InviteConstraint::Required, &OpKey("invite:real:1".into())).await.expect("invite");
    let d = threads.membership_detail(&thread, &seat).await.unwrap().expect("membership after invite");
    assert_eq!(d.state, InvitationState::Pending);
    let (inv, req, rev) = (d.invitation.clone().unwrap(), d.requirement.clone().unwrap(), d.revision.unwrap());
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(threads.membership(&thread, &seat).await.unwrap(), Some(InvitationState::Pending), "acceptance is never fabricated");

    // The agent accepts with the exact command graph prints for `/seat`.
    let accept = daemon.cli(
        Some(&pane),
        &["accept-required", &thread.0, "--invitation", &inv, "--requirement", &req, "--revision", &rev.to_string()],
    );
    ok("accept-required", &accept);
    poll("accepted membership", async || {
        (threads.membership(&thread, &seat).await.ok()? == Some(InvitationState::Accepted)).then_some(())
    })
    .await;

    // Notify lands in the thread history.
    threads.notify(&thread, Severity::Info, "graph says hello", &OpKey("notify:real:1".into())).await.expect("notify");
    let read = daemon.cli(Some(&pane), &["read", &thread.0, "--json"]);
    ok("read", &read);
    assert!(stdout(&read).contains("graph says hello"), "{}", stdout(&read));

    // ReleaseRequirement cleans up: the requirement episode ends, and a second release is clean.
    threads.release_requirement(&thread, &seat, &OpKey("release:real:1".into())).await.expect("release");
    assert_eq!(threads.membership(&thread, &seat).await.unwrap(), Some(InvitationState::Released));
    threads.release_requirement(&thread, &seat, &OpKey("release:real:2".into())).await.expect("second release");

    // A busy service (a second registration while the first lives) is reported, never taken over.
    let second = ServiceThreads::with_system_clock(found.socket, herdr.root.join("graph/.graph-local/threads-intents-2"), found.instance).unwrap();
    let r = second.ensure_thread(ChannelScope::Seat, "x", &OpKey("ensure:st_OTHER".into())).await;
    assert!(matches!(r, Err(ThreadsError::ServiceBusy)), "{r:?}");
    assert!(Path::new(&herdr.root).exists());
}

/// hg-zmi.46: the production wiring (default discovery, no `HERDR_GRAPH_THREADS_*` variable anywhere) reaches a
/// real herdr-threads daemon whose state dir sits at the default place under a private HOME.
#[tokio::test(flavor = "multi_thread")]
async fn production_discovery_reaches_real_threads_daemon() {
    use std::os::unix::fs::DirBuilderExt;
    if !enabled() {
        support::skip("set HG_REAL_THREADS=1");
        return;
    }
    let herdr = match PrivateHerdr::start() {
        Ok(h) => h,
        Err(e) if support::herdr_binary().is_none() => {
            support::skip(&format!("{e}"));
            return;
        }
        Err(e) => panic!("private herdr failed to start: {e:#}"),
    };
    let home = herdr.root.join("home");
    let state = home.join(".local/state/herdr/plugins/herdr-threads");
    herdr.guard().check(&state).expect("threads state is inside the private root");
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&state).unwrap();
    let _daemon = ThreadsDaemon::start_at(&herdr, threads_binary(), state);

    let graph_root = herdr.root.join("graph");
    let ctx = herdr_graph::daemon::DaemonCtx {
        paths: herdr_graph::config::InstancePaths::new(&graph_root),
        herdr_socket: herdr.socket.clone(),
        started_at: chrono::Utc::now(),
    };
    herdr.guard().check(&ctx.paths.threads_intents).expect("intents dir is private");
    let inputs = herdr_graph::threads::discovery::DiscoveryInputs { home: Some(home), ..Default::default() };
    let (threads, _map) = herdr_graph::daemon::compose::production_threads(&ctx, inputs);
    threads.delivery_capability().await.expect("default discovery reaches the real daemon");
}
