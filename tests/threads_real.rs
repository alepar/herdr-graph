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
use herdr_graph::model::clone::{InvitationState, InviteConstraint};
use herdr_graph::model::{HerdrPaneId, HerdrWorkspaceId};
use herdr_graph::ports::herdr::{CreateWorkspace, HerdrApi};
use herdr_graph::ports::threads::*;
use herdr_graph::threads::{PaneSeatMap, ServiceThreads, ThreadsSeatMap, discover};
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::private_herdr::PrivateHerdr;

fn enabled() -> bool {
    std::env::var("HG_REAL_THREADS").as_deref() == Ok("1")
}

/// The pinned herdr-threads binary: built and version-checked by the shared helper.
fn threads_binary() -> PathBuf {
    support::threads_bin::threads_binary()
}

/// A herdr-threads daemon started (via `daemon ensure`) in the private environment; stopped on drop. It keeps
/// copies of the private Herdr's root, socket and env (not a borrow), so a test can restart that Herdr while
/// the daemon runs. Declare it after the `PrivateHerdr` so it drops first.
struct ThreadsDaemon {
    root: PathBuf,
    socket: PathBuf,
    env: Vec<(String, String)>,
    bin: PathBuf,
    state: PathBuf,
    /// Extra variables for every call, `daemon ensure` included (the spawned daemon inherits them).
    extra: Vec<(String, String)>,
}

impl ThreadsDaemon {
    /// A command with the private Herdr's env and nothing else (`PrivateHerdr::command`).
    fn command(&self) -> std::process::Command {
        let mut c = support::private_herdr::private_command(&self.root, &self.env, &self.bin);
        c.envs(self.extra.iter().cloned());
        c
    }

    fn cli(&self, pane: Option<&HerdrPaneId>, args: &[&str]) -> Output {
        let mut c = self.command();
        c.args(["--state-dir"])
            .arg(&self.state)
            .arg("--host-endpoint")
            .arg(&self.socket);
        c.args(args)
            .env("CLAUDE_PID", std::process::id().to_string())
            .env("CLAUDE_CODE_SESSION_ID", "graph-threads-test");
        if let Some(p) = pane {
            c.env("HERDR_PANE_ID", &p.0)
                // Public cooperative harness markers name only this isolated test process.
                .env("CLAUDE_PID", std::process::id().to_string())
                .env("CLAUDE_CODE_SESSION_ID", "graph-threads-test");
        }
        c.env_remove("ANTHROPIC_API_KEY")
            .env_remove("OPENAI_API_KEY");
        c.stdin(Stdio::null()).output().expect("herdr-threads runs")
    }

    /// An operator command in threads' `human` namespace (`herdr-threads human [GLOBALS] COMMAND`).
    fn human_cli(&self, args: &[&str]) -> Output {
        self.command()
            .arg("human")
            .args(["--state-dir"])
            .arg(&self.state)
            .arg("--host-endpoint")
            .arg(&self.socket)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("herdr-threads runs")
    }

    /// Register a synthetic native session through the public hook, without credentials.
    fn register_agent(&self, pane: &HerdrPaneId) {
        self.register_agent_as(pane, "graph-threads-test");
    }

    /// `register_agent` under the native session id `session` (one per agent seat in a rig).
    fn register_agent_as(&self, pane: &HerdrPaneId, session: &str) {
        use std::os::unix::fs::PermissionsExt;
        let bin = self.root.join("fixture-bin");
        std::fs::create_dir_all(&bin).unwrap();
        let claude = bin.join("claude");
        std::fs::write(&claude, "#!/bin/sh\necho '2.1.283 (Claude Code)'\n").unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let transcript = self.root.join(format!("fixture-{session}.jsonl"));
        std::fs::write(&transcript, "").unwrap();
        let payload = self.root.join(format!("fixture-hook-{session}.json"));
        std::fs::write(
            &payload,
            serde_json::to_vec(&serde_json::json!({
                "hook_event_name": "SessionStart", "session_id": session,
                "transcript_path": transcript, "cwd": self.root.join("home"), "source": "startup"
            }))
            .unwrap(),
        )
        .unwrap();
        let path = self
            .env
            .iter()
            .find(|(key, _)| key == "PATH")
            .unwrap()
            .1
            .clone();
        let out = self
            .command()
            .args(["--state-dir"])
            .arg(&self.state)
            .arg("--host-endpoint")
            .arg(&self.socket)
            .args(["hook", "claude"])
            .env("CLAUDE_PID", std::process::id().to_string())
            .env("CLAUDE_CODE_SESSION_ID", session)
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env("HERDR_ENV", "1")
            .env("HERDR_PANE_ID", &pane.0)
            .env("HERDR_SOCKET_PATH", &self.socket)
            .env("PATH", format!("{}:{path}", bin.display()))
            .stdin(Stdio::from(std::fs::File::open(payload).unwrap()))
            .output()
            .unwrap();
        ok("native session hook", &out);
        assert!(
            !stdout(&out).is_empty(),
            "native hook did not register: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn agent_cli(&self, pane: &HerdrPaneId, seat: &ThreadsSeatRef, args: &[&str]) -> Output {
        let mut native = vec![
            "--cooperative-seat",
            &seat.0,
            "--cooperative-target",
            &pane.0,
            "--cooperative-harness",
            "claude",
            "--cooperative-role",
            "top-level",
        ];
        native.extend_from_slice(args);
        self.cli(Some(pane), &native)
    }

    fn start(herdr: &PrivateHerdr, bin: PathBuf) -> Self {
        Self::start_at(herdr, bin, herdr.root.join("threads-state"))
    }

    fn start_at(herdr: &PrivateHerdr, bin: PathBuf, state: PathBuf) -> Self {
        Self::start_with(herdr, bin, state, &[])
    }

    /// `start_at` with `extra` set for the daemon and every later call (the tier-5 `HT_TEST_*` clocks).
    fn start_with(
        herdr: &PrivateHerdr,
        bin: PathBuf,
        state: PathBuf,
        extra: &[(&str, &str)],
    ) -> Self {
        herdr
            .guard()
            .check(&state)
            .expect("threads state is inside the private root");
        herdr
            .guard()
            .check(&herdr.socket)
            .expect("host endpoint is private");
        let d = Self {
            root: herdr.root.clone(),
            socket: herdr.socket.clone(),
            env: herdr.env.clone(),
            bin,
            state,
            extra: extra
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        };
        let out = d.cli(None, &["daemon", "ensure"]);
        assert!(
            out.status.success(),
            "daemon ensure: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        d
    }
}

impl Drop for ThreadsDaemon {
    fn drop(&mut self) {
        // `daemon stop` only talks to the daemon this state directory belongs to.
        let _ = self.cli(None, &["daemon", "stop"]);
        support::isolated::reap_root(&self.root);
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
    assert!(
        o.status.success(),
        "{what}: {} {}",
        stdout(o),
        String::from_utf8_lossy(&o.stderr)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn real_threads_channel_invite_membership_notify_release() {
    if !enabled() {
        support::skip("set HG_REAL_THREADS=1");
        return;
    }
    let herdr = match PrivateHerdr::start_without_credentials() {
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
    // The daemon binds a short per-instance socket under <tmp_dir>/herdr-threads-<uid>/ when the state
    // path is too long for a socket address; the name is a hash of the private instance directory. Anything
    // else must be inside the private root, and nothing may be a live user resource.
    match herdr.guard().check(&found.socket) {
        Ok(()) => {}
        Err(IsolationError::OutsideRoot(p, _))
            if p.starts_with(herdr_graph::config::tmp_dir())
                && p.to_string_lossy().contains("/herdr-threads-") => {}
        Err(e) => panic!(
            "threads socket {} is not private: {e}",
            found.socket.display()
        ),
    }
    let intents = herdr
        .root
        .join("graph")
        .join(".graph-local")
        .join("threads-intents");
    herdr
        .guard()
        .check(&intents)
        .expect("intents dir is private");
    let threads = ServiceThreads::with_system_clock(
        found.socket.clone(),
        intents.clone(),
        found.instance,
        herdr_threads::protocol::ids::HostLocator::parse(herdr.socket.to_str().unwrap()).unwrap(),
    )
    .unwrap();
    let clock: Arc<dyn herdr_threads::protocol::time::Clock> =
        Arc::new(herdr_threads::app::SystemClock::new());
    let map = ThreadsSeatMap::new(
        found.socket.clone(),
        found.instance,
        clock,
        herdr_threads::protocol::ids::HostLocator::parse(herdr.socket.to_str().unwrap()).unwrap(),
    );

    // A pane in the private Herdr, registered as a synthetic native agent seat.
    let client = herdr.client();
    let created = client
        .create_workspace(CreateWorkspace {
            label: "threads-real".into(),
            cwd: herdr.root.join("home"),
            env: vec![],
        })
        .await
        .expect("workspace");
    let pane = created.pane.clone().expect("first pane");
    // Resolution creates only a bare local seat, even before a native session enrolls.
    let bare = map
        .seat_for(&pane)
        .await
        .expect("host-qualified resolve")
        .unwrap();
    assert_eq!(map.seat_for(&pane).await.unwrap(), Some(bare.clone()));
    let inspection = daemon.cli(None, &["seat", "inspect", &bare.0, "--json"]);
    ok("inspect bare seat", &inspection);
    let inspected: serde_json::Value = serde_json::from_slice(&inspection.stdout).unwrap();
    assert!(
        inspected.to_string().contains("\"open_binding\":null"),
        "resolution must not enroll any occupant binding: {inspected}"
    );

    // One node serves two private Herdr hosts whose local pane IDs collide.
    let other = PrivateHerdr::start_without_credentials().expect("second private host");
    let other_created = other
        .client()
        .create_workspace(CreateWorkspace {
            label: "threads-other-host".into(),
            cwd: other.root.join("home"),
            env: vec![],
        })
        .await
        .unwrap();
    let other_pane = other_created.pane.unwrap();
    assert_eq!(
        pane, other_pane,
        "fixture must exercise identical pane text"
    );
    let other_map = ThreadsSeatMap::new(
        found.socket.clone(),
        found.instance,
        Arc::new(herdr_threads::app::SystemClock::new()),
        herdr_threads::protocol::ids::HostLocator::parse(other.socket.to_str().unwrap()).unwrap(),
    );
    let other_seat = other_map.seat_for(&other_pane).await.unwrap().unwrap();
    assert_ne!(bare, other_seat, "different hosts must not alias");
    assert_eq!(map.seat_for(&pane).await.unwrap(), Some(bare.clone()));

    daemon.register_agent(&pane);
    let seat = poll("the pane's threads seat", async || {
        map.seat_for(&pane).await.ok().flatten()
    })
    .await;
    assert!(!seat.0.is_empty(), "{seat:?}"); // Seat IDs are opaque.

    // EnsureThread: idempotent by operation key.
    let key = OpKey("ensure:st_REALTEST".into());
    let thread = threads
        .ensure_thread(ChannelScope::Seat, "alpha/foreman", &key)
        .await
        .expect("ensure");
    assert_eq!(thread, ThreadRef("hg-st_realtest".into()));
    assert_eq!(
        threads
            .ensure_thread(ChannelScope::Seat, "alpha/foreman", &key)
            .await
            .unwrap(),
        thread
    );
    assert_eq!(
        threads.membership(&thread, &seat).await.unwrap(),
        None,
        "nobody invited yet"
    );
    threads
        .set_topic(&thread, "alpha/boss", &OpKey("topic:real:1".into()))
        .await
        .expect("set topic");

    // Invite Required: pending until the native side accepts; the graph side never accepts.
    threads
        .invite(
            &thread,
            &seat,
            InviteConstraint::Required,
            &OpKey("invite:real:1".into()),
        )
        .await
        .expect("invite");
    let d = threads
        .membership_detail(&thread, &seat)
        .await
        .unwrap()
        .expect("membership after invite");
    assert_eq!(d.state, InvitationState::Pending);
    let (inv, req, rev) = (
        d.invitation.clone().unwrap(),
        d.requirement.clone().unwrap(),
        d.revision.unwrap(),
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        threads.membership(&thread, &seat).await.unwrap(),
        Some(InvitationState::Pending),
        "acceptance is never fabricated"
    );

    // The agent accepts with the exact command graph prints for `/seat`.
    let accept = daemon.agent_cli(
        &pane,
        &seat,
        &[
            "accept-required",
            &thread.0,
            "--invitation",
            &inv,
            "--requirement",
            &req,
            "--revision",
            &rev.to_string(),
        ],
    );
    ok("accept-required", &accept);
    poll("accepted membership", async || {
        (threads.membership(&thread, &seat).await.ok()? == Some(InvitationState::Accepted))
            .then_some(())
    })
    .await;

    // Notify lands in the thread history.
    threads
        .notify(
            &thread,
            Severity::Info,
            "graph says hello",
            &OpKey("notify:real:1".into()),
        )
        .await
        .expect("notify");
    let read = daemon.agent_cli(&pane, &seat, &["read", &thread.0, "--json"]);
    ok("read", &read);
    assert!(
        stdout(&read).contains("graph says hello"),
        "{}",
        stdout(&read)
    );

    // Service-ACK delivery (herdr-threads ht-5nb): a v2 registration, an ACK-required request to the joined
    // seat, pending until the native agent ACKs it by exact id; ACK means received, never processed.
    assert_eq!(
        threads.delivery_capability().await.unwrap(),
        DeliveryCapability::ServiceAck
    );
    let msg = threads
        .send_request(
            &thread,
            std::slice::from_ref(&seat),
            "graph request rq_REAL",
            &OpKey("deliver:rq_REAL".into()),
        )
        .await
        .expect("service send");
    let r = threads
        .receipt_state(std::slice::from_ref(&msg))
        .await
        .expect("receipts");
    assert_eq!(r[0].recipients.len(), 1, "{r:?}");
    assert_eq!(r[0].recipients[0].seat, seat);
    assert_eq!(
        r[0].recipients[0].state,
        ReceiptState::Pending,
        "no ACK is ever fabricated"
    );
    let ack = daemon.agent_cli(&pane, &seat, &["ack", &msg.0]);
    ok("ack", &ack);
    poll("acknowledged receipt", async || {
        let r = threads
            .receipt_state(std::slice::from_ref(&msg))
            .await
            .ok()?;
        matches!(r[0].recipients[0].state, ReceiptState::Acknowledged { .. }).then_some(())
    })
    .await;

    // ReleaseRequirement cleans up: the requirement episode ends, and a second release is clean.
    threads
        .release_requirement(&thread, &seat, &OpKey("release:real:1".into()))
        .await
        .expect("release");
    assert_eq!(
        threads.membership(&thread, &seat).await.unwrap(),
        Some(InvitationState::Released)
    );
    threads
        .release_requirement(&thread, &seat, &OpKey("release:real:2".into()))
        .await
        .expect("second release");

    // A busy service (a second registration while the first lives) is reported, never taken over.
    let second = ServiceThreads::with_system_clock(
        found.socket.clone(),
        herdr.root.join("graph/.graph-local/threads-intents-2"),
        found.instance,
        herdr_threads::protocol::ids::HostLocator::parse(herdr.socket.to_str().unwrap()).unwrap(),
    )
    .unwrap();
    let r = second
        .ensure_thread(ChannelScope::Seat, "x", &OpKey("ensure:st_OTHER".into()))
        .await;
    assert!(matches!(r, Err(ThreadsError::ServiceBusy)), "{r:?}");
    assert!(Path::new(&herdr.root).exists());
    drop(second);
    drop(threads);
    ok(
        "stop private threads for restart",
        &daemon.cli(None, &["daemon", "stop"]),
    );
    ok(
        "restart private threads",
        &daemon.cli(None, &["daemon", "ensure"]),
    );
    let restarted = discover(&daemon.state, &herdr.socket).unwrap();
    assert_eq!(restarted.instance, found.instance);
    let threads = ServiceThreads::with_system_clock(
        restarted.socket,
        intents,
        restarted.instance,
        herdr_threads::protocol::ids::HostLocator::parse(herdr.socket.to_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        threads
            .ensure_thread(ChannelScope::Seat, "alpha/foreman", &key)
            .await
            .unwrap(),
        thread
    );
    let receipts = threads
        .receipt_state(std::slice::from_ref(&msg))
        .await
        .unwrap();
    assert!(matches!(
        receipts[0].recipients[0].state,
        ReceiptState::Acknowledged { .. }
    ));
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
    let herdr = match PrivateHerdr::start_without_credentials() {
        Ok(h) => h,
        Err(e) if support::herdr_binary().is_none() => {
            support::skip(&format!("{e}"));
            return;
        }
        Err(e) => panic!("private herdr failed to start: {e:#}"),
    };
    let home = herdr.root.join("home");
    let state = home.join(".local/state/herdr/plugins/herdr-threads");
    herdr
        .guard()
        .check(&state)
        .expect("threads state is inside the private root");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&state)
        .unwrap();
    let daemon = ThreadsDaemon::start_at(&herdr, threads_binary(), state);

    let graph_root = herdr.root.join("graph");
    herdr_graph::store::init::init_instance(&graph_root).unwrap();
    let ctx = herdr_graph::daemon::DaemonCtx {
        paths: herdr_graph::config::InstancePaths::new(&graph_root),
        herdr_socket: herdr.socket.clone(),
        started_at: chrono::Utc::now(),
        claude_root: herdr.root.join("claude"),
    };
    herdr
        .guard()
        .check(&ctx.paths.threads_intents)
        .expect("intents dir is private");
    let inputs = herdr_graph::threads::discovery::DiscoveryInputs {
        home: Some(home),
        ..Default::default()
    };
    let (threads, _map) = herdr_graph::daemon::compose::production_threads(&ctx, inputs);
    threads
        .delivery_capability()
        .await
        .expect("default discovery reaches the real daemon");
    let old = discover(&daemon.state, &herdr.socket).unwrap();
    let old_channel = threads
        .ensure_thread(
            ChannelScope::Seat,
            "recovery channel",
            &OpKey("ensure:recovery".into()),
        )
        .await
        .unwrap();
    let old_intents = ctx.paths.threads_intents.clone();

    ok(
        "stop private threads before fresh store",
        &daemon.cli(None, &["daemon", "stop"]),
    );
    let saved = daemon.state.join("saved-old-node");
    std::fs::rename(daemon.state.join("node"), &saved).unwrap();
    ok(
        "start fresh private node",
        &daemon.cli(None, &["daemon", "ensure"]),
    );
    let new = discover(&daemon.state, &herdr.socket).unwrap();
    assert_ne!(old.instance, new.instance);
    assert!(
        saved.join("herdr-threads.db").is_file(),
        "old artifacts must remain recoverable"
    );
    assert!(
        matches!(
            threads.delivery_capability().await,
            Err(ThreadsError::Rejected(_))
        ),
        "cached endpoint must not register or dispatch against a fresh node"
    );
    // Exercise the production reviewed writer and endpoint recovery, not a replacement binding fixture.
    use herdr_graph::{
        plan::{
            commands::{PlanDeps, admit_apply, create_plan},
            store::PlanStore,
        },
        ports::store::Store,
    };
    let store = Arc::new(herdr_graph::store::GitStore::open(&graph_root).unwrap());
    let plans = Arc::new(PlanStore::new(graph_root.join(".graph-local/plans")));
    let (kinds, mutations) = herdr_graph::daemon::compose::mutation_registry(plans.clone());
    let journal = Arc::new(
        herdr_graph::journal::Journal::open(&herdr_graph::journal::Journal::path_in(&graph_root))
            .unwrap(),
    );
    let clock = Arc::new(herdr_graph::ports::clock::SystemClock);
    let writer = herdr_graph::writer::WriterCore::new(
        store.clone(),
        journal.clone(),
        Arc::new(mutations),
        clock.clone(),
        Default::default(),
    );
    let deps = PlanDeps {
        kinds,
        plans: plans.clone(),
        store: store.clone(),
        writer: writer.clone(),
        clock,
        instance: graph_root.clone(),
    };
    let command = format!(
        "threads reconnect --from {} --to {} --head {}",
        old.instance,
        new.instance,
        store.head().unwrap().0
    );
    let response = create_plan(
        &deps,
        &Default::default(),
        command.split_whitespace().map(str::to_owned).collect(),
    )
    .unwrap();
    let id = response["plan_id"].as_str().unwrap();
    let hash = response["hash"].as_str().unwrap();
    let op = admit_apply(&deps, &Default::default(), id, Some(hash), "relay").unwrap();
    writer.drain().unwrap();
    assert_eq!(
        journal.get(&op).unwrap().unwrap().state,
        herdr_graph::model::operation::OpState::Committed
    );
    let epoch = herdr_graph::threads::namespace::committed(&graph_root)
        .unwrap()
        .unwrap()
        .epoch;
    assert!(
        threads
            .ensure_thread(
                ChannelScope::Seat,
                "recovery channel",
                &OpKey("ensure:recovery".into())
            )
            .await
            .is_err(),
        "stale snapshots cannot silently adopt the new binding"
    );
    let key = OpKey(format!("{epoch}:ensure:recovery"));
    let fresh_channel = herdr_graph::threads::namespace::EXPECTED_EPOCH
        .scope(
            epoch.clone(),
            threads.ensure_thread(ChannelScope::Seat, "recovery channel", &key),
        )
        .await
        .unwrap();
    assert_ne!(old_channel, fresh_channel);
    assert_eq!(
        herdr_graph::threads::namespace::bound(&old_intents).unwrap(),
        Some(new.instance)
    );
    assert!(
        old_intents.is_dir(),
        "old service intents remain under their original identity"
    );
    assert!(
        herdr_graph::threads::namespace::intent_dir(&old_intents, &graph_root)
            .unwrap()
            .is_dir()
    );
    let again = herdr_graph::threads::namespace::EXPECTED_EPOCH
        .scope(
            epoch,
            threads.ensure_thread(ChannelScope::Seat, "recovery channel", &key),
        )
        .await
        .unwrap();
    assert_eq!(again, fresh_channel);
    assert!(saved.join("herdr-threads.db").is_file());

    // Exercise the installed background probe, not a manually scoped direct call.
    let mut registry = herdr_graph::daemon::registry::Registry::default();
    herdr_graph::daemon::compose::register_threads_connection(
        &mut registry,
        threads.clone(),
        graph_root.clone(),
    );
    let (stop, shutdown) = herdr_graph::daemon::registry::shutdown_channel();
    let (_, run) = registry.take_loops().pop().unwrap();
    let handle = tokio::spawn(run(shutdown));
    for _ in 0..200 {
        if registry.status_components()["threads"]
            .get("note")
            .is_none()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let component = registry.status_components()["threads"].clone();
    stop.send(true).unwrap();
    handle.await.unwrap().unwrap();
    assert_eq!(
        component["connected"], true,
        "normal post-reconnect component probe: {component}"
    );
}

/// `seat adopt` takes the pane's threads seat from ResolveSeat (`seat_for`): for a checked-in agent that is the
/// seat its session is bound to, stable across calls, and resolving allocates no second seat.
#[tokio::test(flavor = "multi_thread")]
async fn adopt_resolves_the_checked_in_seat() {
    if !enabled() {
        support::skip("set HG_REAL_THREADS=1");
        return;
    }
    let herdr = match PrivateHerdr::start_without_credentials() {
        Ok(h) => h,
        Err(e) if support::herdr_binary().is_none() => {
            support::skip(&format!("{e}"));
            return;
        }
        Err(e) => panic!("private herdr failed to start: {e:#}"),
    };
    let daemon = ThreadsDaemon::start(&herdr, threads_binary());
    let found = discover(&daemon.state, &herdr.socket).expect("daemon published its endpoint");
    let map = ThreadsSeatMap::new(
        found.socket.clone(),
        found.instance,
        Arc::new(herdr_threads::app::SystemClock::new()),
        herdr_threads::protocol::ids::HostLocator::parse(herdr.socket.to_str().unwrap()).unwrap(),
    );
    let created = herdr
        .client()
        .create_workspace(CreateWorkspace {
            label: "adopt-real".into(),
            cwd: herdr.root.join("home"),
            env: vec![],
        })
        .await
        .expect("workspace");
    let pane = created.pane.clone().expect("first pane");

    daemon.register_agent(&pane);
    let seat = poll("the checked-in agent's seat binding", async || {
        let s = map.seat_for(&pane).await.ok().flatten()?;
        let out = daemon.cli(None, &["--json", "seat", "inspect", &s.0]);
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
        (!v.to_string().contains("\"open_binding\":null")).then_some(s)
    })
    .await;
    assert_eq!(map.seat_for(&pane).await.unwrap(), Some(seat.clone()));
    let list = daemon.cli(None, &["--json", "seat", "list", "--include-retired"]);
    ok("seat list", &list);
    let v: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let ids: Vec<&str> = v["result"]["data"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("no seat items: {v}"))
        .iter()
        .filter_map(|s| s["seat"].as_str())
        .collect();
    assert_eq!(ids, [seat.0.as_str()], "resolving allocated an extra seat");
}

// ---- Tier 5 (hg-lnw.8): ResurrectSeat in each window and the held-seat rebind, through graph's real adapter ----

/// Graph's production threads ports over `daemon`: `production_ports` (`ServiceThreads` + `ThreadsSeatMap` behind
/// the lazy endpoint, with the Herdr host from `RuntimeContext::claim_host`, as the graph daemon composes them).
struct Tier5 {
    threads: Arc<dyn ThreadsPort>,
    map: Arc<dyn PaneSeatMap>,
    view: Arc<dyn ThreadsSeatView>,
}

impl Tier5 {
    fn new(herdr: &PrivateHerdr, daemon: &ThreadsDaemon) -> Self {
        let graph_root = herdr.root.join("graph");
        herdr_graph::store::init::init_instance(&graph_root).unwrap();
        let ctx = herdr_graph::daemon::DaemonCtx {
            paths: herdr_graph::config::InstancePaths::new(&graph_root),
            herdr_socket: herdr.socket.clone(),
            started_at: chrono::Utc::now(),
            claude_root: herdr.root.join("claude"),
        };
        herdr
            .guard()
            .check(&ctx.paths.threads_intents)
            .expect("intents dir is private");
        let inputs = herdr_graph::threads::discovery::DiscoveryInputs {
            env_state_dir: Some(daemon.state.clone()),
            ..Default::default()
        };
        let (threads, map, view, _, _) =
            herdr_graph::daemon::compose::production_ports(&ctx, inputs);
        Self { threads, map, view }
    }

    async fn status(&self, seat: &ThreadsSeatRef) -> Option<SeatStatus> {
        self.view
            .inspect(std::slice::from_ref(seat))
            .await
            .ok()?
            .remove(seat)
    }

    async fn receipt(&self, msg: &MessageRef) -> Option<ReceiptState> {
        let r = self
            .threads
            .receipt_state(std::slice::from_ref(msg))
            .await
            .ok()?;
        Some(r.first()?.recipients.first()?.state.clone())
    }
}

/// A private Herdr for a tier-5 test, or `None` (skipped) when tier 5 is off or Herdr is not installed.
fn tier5_herdr() -> Option<PrivateHerdr> {
    if !enabled() {
        support::skip("set HG_REAL_THREADS=1");
        return None;
    }
    match PrivateHerdr::start_without_credentials() {
        Ok(h) => Some(h),
        Err(e) if support::herdr_binary().is_none() => {
            support::skip(&format!("{e}"));
            None
        }
        Err(e) => panic!("private herdr failed to start: {e:#}"),
    }
}

/// The test-support threads daemon: only that build honours the `HT_TEST_*` clocks set in `clocks`.
fn tier5_daemon(herdr: &PrivateHerdr, clocks: &[(&str, &str)]) -> ThreadsDaemon {
    let bin = support::threads_bin::test_support_binary().unwrap_or_else(|e| panic!("{e}"));
    let mut extra = vec![("HT_TEST_LANE_TICK_MS", "200")];
    extra.extend_from_slice(clocks);
    ThreadsDaemon::start_with(herdr, bin, herdr.root.join("threads-state"), &extra)
}

async fn poll_for<T>(what: &str, timeout: Duration, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A new workspace whose first pane holds a checked-in synthetic agent (native session `session`): a keyless
/// seat with an open binding, as `seat adopt` finds it.
async fn agent_seat(
    herdr: &PrivateHerdr,
    daemon: &ThreadsDaemon,
    ports: &Tier5,
    label: &str,
    session: &str,
) -> (HerdrWorkspaceId, HerdrPaneId, ThreadsSeatRef) {
    let created = herdr
        .client()
        .create_workspace(CreateWorkspace {
            label: label.into(),
            cwd: herdr.root.join("home"),
            env: vec![],
        })
        .await
        .expect("workspace");
    let pane = created.pane.clone().expect("first pane");
    daemon.register_agent_as(&pane, session);
    let seat = poll("the checked-in agent's seat binding", async || {
        let s = ports.map.seat_for(&pane).await.ok().flatten()?;
        let out = daemon.cli(None, &["--json", "seat", "inspect", &s.0]);
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
        (!v.to_string().contains("\"open_binding\":null")).then_some(s)
    })
    .await;
    (created.workspace.expect("workspace id"), pane, seat)
}

/// What the seat is owed before its tab closes: an accepted Required membership, a pending ordinary invitation
/// to a seat-wide participation thread, and a pending ACK request.
struct Owed {
    required: ThreadRef,
    participation: ThreadRef,
    msg: MessageRef,
}

async fn owe(
    daemon: &ThreadsDaemon,
    ports: &Tier5,
    pane: &HerdrPaneId,
    seat: &ThreadsSeatRef,
) -> Owed {
    let t = &ports.threads;
    let required = t
        .ensure_thread(
            ChannelScope::Seat,
            "tier5/required",
            &OpKey("ensure:st_T5REQ".into()),
        )
        .await
        .expect("ensure required");
    t.invite(
        &required,
        seat,
        InviteConstraint::Required,
        &OpKey("invite:t5:req".into()),
    )
    .await
    .expect("invite required");
    let d = t
        .membership_detail(&required, seat)
        .await
        .unwrap()
        .expect("required membership");
    let accept = daemon.agent_cli(
        pane,
        seat,
        &[
            "accept-required",
            &required.0,
            "--invitation",
            d.invitation.as_deref().unwrap(),
            "--requirement",
            d.requirement.as_deref().unwrap(),
            "--revision",
            &d.revision.unwrap().to_string(),
        ],
    );
    ok("accept-required", &accept);
    poll("accepted required membership", async || {
        (t.membership(&required, seat).await.ok()? == Some(InvitationState::Accepted)).then_some(())
    })
    .await;
    let participation = t
        .ensure_thread(
            ChannelScope::Seat,
            "tier5/participation",
            &OpKey("ensure:st_T5PART".into()),
        )
        .await
        .expect("ensure participation");
    t.invite(
        &participation,
        seat,
        InviteConstraint::Ordinary,
        &OpKey("invite:t5:part:1".into()),
    )
    .await
    .expect("invite participation");
    assert_eq!(
        t.membership(&participation, seat).await.unwrap(),
        Some(InvitationState::Pending)
    );
    let msg = t
        .send_request(
            &required,
            std::slice::from_ref(seat),
            "tier5 request",
            &OpKey("deliver:rq_T5".into()),
        )
        .await
        .expect("service send");
    assert_eq!(ports.receipt(&msg).await, Some(ReceiptState::Pending));
    Owed {
        required,
        participation,
        msg,
    }
}

/// `seat`'s participant row on `thread` as threads reports it (`thread participants --json`): `effective_state`
/// and the invitation `episode`, which a freshly minted invitation increments.
fn participant(daemon: &ThreadsDaemon, thread: &ThreadRef, seat: &ThreadsSeatRef) -> (String, u64) {
    let out = daemon.cli(None, &["--json", "thread", "participants", &thread.0]);
    ok("thread participants", &out);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let row = v["result"]["data"]["items"]
        .as_array()
        .and_then(|items| items.iter().find(|i| i["seat"] == seat.0.as_str()))
        .unwrap_or_else(|| panic!("no participant row for {seat:?}: {v}"))
        .clone();
    (
        row["effective_state"].as_str().unwrap().to_owned(),
        row["episode"].as_u64().unwrap(),
    )
}

/// The stored (physical) receipt status of `msg`'s one recipient (`delivery recipients --json`). Graph's port
/// reads the effective status, which is `retired` for any retired recipient; the physical one turns `retired`
/// only when the obligation expiry settles it.
fn physical_receipt(daemon: &ThreadsDaemon, msg: &MessageRef) -> String {
    let out = daemon.cli(None, &["--json", "delivery", "recipients", &msg.0]);
    ok("delivery recipients", &out);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["result"]["data"]["items"][0]["physical_status"]
        .as_str()
        .unwrap_or_else(|| panic!("no recipient: {v}"))
        .to_owned()
}

/// Close the seat's workspace and wait until threads reads the seat keyless `Retired{Reversible}`.
async fn close_until_reversible(
    herdr: &PrivateHerdr,
    ports: &Tier5,
    ws: &HerdrWorkspaceId,
    seat: &ThreadsSeatRef,
) {
    herdr
        .client()
        .close_workspace(ws)
        .await
        .expect("close workspace");
    poll_for(
        "keyless Retired{Reversible}",
        Duration::from_secs(30),
        async || {
            matches!(
                ports.status(seat).await?,
                SeatStatus::Retired {
                    kind: RetirementKind::Reversible,
                    keyed: false,
                    ..
                }
            )
            .then_some(())
        },
    )
    .await;
}

/// Graph's undo reopens the tab: a new pane, with the bare seat threads mints for it on resolution.
async fn reopen(herdr: &PrivateHerdr, ports: &Tier5, label: &str) -> (HerdrPaneId, ThreadsSeatRef) {
    let created = herdr
        .client()
        .create_workspace(CreateWorkspace {
            label: label.into(),
            cwd: herdr.root.join("home"),
            env: vec![],
        })
        .await
        .expect("reopened workspace");
    let pane = created.pane.expect("reopened pane");
    let bare = ports.map.seat_for(&pane).await.unwrap().expect("bare seat");
    (pane, bare)
}

#[tokio::test(flavor = "multi_thread")]
async fn resurrect_before_24h_keeps_memberships_and_pending_receipt() {
    let Some(herdr) = tier5_herdr() else { return };
    let daemon = tier5_daemon(&herdr, &[]);
    let ports = Tier5::new(&herdr, &daemon);
    let (ws, pane, seat) = agent_seat(&herdr, &daemon, &ports, "t5-before", "t5-before").await;
    let owed = owe(&daemon, &ports, &pane, &seat).await;
    assert_eq!(
        participant(&daemon, &owed.participation, &seat),
        ("invited".to_owned(), 1)
    );
    close_until_reversible(&herdr, &ports, &ws, &seat).await;
    // Inside the window nothing has settled: the stored receipt is still pending.
    assert_eq!(physical_receipt(&daemon, &owed.msg), "pending");
    let (reopened, bare) = reopen(&herdr, &ports, "t5-before-undo").await;
    assert_ne!(bare, seat);

    let outcome = ports
        .threads
        .resurrect_seat(
            &seat,
            &reopened,
            Some(&bare),
            &OpKey("resurrect:t5:before".into()),
        )
        .await;
    assert_eq!(outcome, SeatRestoreOutcome::Done);
    assert_eq!(
        ports.status(&seat).await,
        Some(SeatStatus::Resolved {
            pane: reopened.clone()
        })
    );
    assert_eq!(
        ports.map.seat_for(&reopened).await.unwrap(),
        Some(seat.clone())
    );
    assert!(
        matches!(ports.status(&bare).await, Some(SeatStatus::Retired { .. })),
        "the replaced bare seat retires in the same decision: {:?}",
        ports.status(&bare).await
    );
    let t = &ports.threads;
    assert_eq!(
        t.membership(&owed.required, &seat).await.unwrap(),
        Some(InvitationState::Accepted)
    );
    assert_eq!(ports.receipt(&owed.msg).await, Some(ReceiptState::Pending));
    assert_eq!(
        t.membership(&owed.participation, &seat).await.unwrap(),
        Some(InvitationState::Pending)
    );
    // Graph's reset re-issues the ordinary invitation: before 24 h threads reuses the pending one.
    t.invite(
        &owed.participation,
        &seat,
        InviteConstraint::Ordinary,
        &OpKey("invite:t5:part:2".into()),
    )
    .await
    .expect("re-issued invite");
    assert_eq!(
        participant(&daemon, &owed.participation, &seat),
        ("invited".to_owned(), 1),
        "the pending invitation is reused, not duplicated"
    );
    // A replay under the same key is the same answer.
    assert_eq!(
        t.resurrect_seat(
            &seat,
            &reopened,
            Some(&bare),
            &OpKey("resurrect:t5:before".into())
        )
        .await,
        SeatRestoreOutcome::Done
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn resurrect_between_24h_and_7d_keeps_memberships_and_settles_receipt() {
    let Some(herdr) = tier5_herdr() else { return };
    // "24 h" is 1 s and "7 d" is 10 min: the test resurrects after the expiry and well before finalization.
    let daemon = tier5_daemon(
        &herdr,
        &[
            ("HT_TEST_OBLIGATION_EXPIRY_MS", "1000"),
            ("HT_TEST_KEYLESS_FINALIZE_MS", "600000"),
        ],
    );
    let ports = Tier5::new(&herdr, &daemon);
    let (ws, pane, seat) = agent_seat(&herdr, &daemon, &ports, "t5-window", "t5-window").await;
    let owed = owe(&daemon, &ports, &pane, &seat).await;
    close_until_reversible(&herdr, &ports, &ws, &seat).await;
    // Past the expiry: what the seat was owed settles as recipient-retired; the retirement stays reversible.
    poll_for(
        "the obligation expiry to settle the receipt",
        Duration::from_secs(30),
        async || (physical_receipt(&daemon, &owed.msg) == "retired").then_some(()),
    )
    .await;
    assert!(matches!(
        ports.status(&seat).await,
        Some(SeatStatus::Retired {
            kind: RetirementKind::Reversible,
            keyed: false,
            ..
        })
    ));
    let (reopened, bare) = reopen(&herdr, &ports, "t5-window-undo").await;

    let t = &ports.threads;
    assert_eq!(
        t.resurrect_seat(
            &seat,
            &reopened,
            Some(&bare),
            &OpKey("resurrect:t5:window".into())
        )
        .await,
        SeatRestoreOutcome::Done
    );
    assert_eq!(
        ports.status(&seat).await,
        Some(SeatStatus::Resolved {
            pane: reopened.clone()
        })
    );
    assert_eq!(
        t.membership(&owed.required, &seat).await.unwrap(),
        Some(InvitationState::Accepted),
        "the requirement episode returns"
    );
    assert_eq!(
        ports.receipt(&owed.msg).await,
        Some(ReceiptState::Retired),
        "the receipt stays settled"
    );
    assert_eq!(physical_receipt(&daemon, &owed.msg), "retired");
    // Graph's invitation reset re-issues its ordinary invitation onto the seat-wide participation thread, and
    // threads mints a fresh pending invitation for the seat (episode 2; the settled one was episode 1).
    t.invite(
        &owed.participation,
        &seat,
        InviteConstraint::Ordinary,
        &OpKey("invite:t5:part:2".into()),
    )
    .await
    .expect("re-issued invite");
    assert_eq!(
        participant(&daemon, &owed.participation, &seat),
        ("invited".to_owned(), 2),
        "a fresh pending invitation"
    );
    assert_eq!(
        t.membership(&owed.participation, &seat).await.unwrap(),
        Some(InvitationState::Pending)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn resurrect_after_finalization_is_not_found() {
    let Some(herdr) = tier5_herdr() else { return };
    let daemon = tier5_daemon(
        &herdr,
        &[
            ("HT_TEST_OBLIGATION_EXPIRY_MS", "500"),
            ("HT_TEST_KEYLESS_FINALIZE_MS", "1500"),
        ],
    );
    let ports = Tier5::new(&herdr, &daemon);
    let (ws, _pane, seat) = agent_seat(&herdr, &daemon, &ports, "t5-final", "t5-final").await;
    herdr
        .client()
        .close_workspace(&ws)
        .await
        .expect("close workspace");
    poll_for(
        "keyless Retired{Final}",
        Duration::from_secs(40),
        async || {
            matches!(
                ports.status(&seat).await?,
                SeatStatus::Retired {
                    kind: RetirementKind::Final,
                    keyed: false,
                    ..
                }
            )
            .then_some(())
        },
    )
    .await;
    let (reopened, bare) = reopen(&herdr, &ports, "t5-final-undo").await;
    assert_eq!(
        ports
            .threads
            .resurrect_seat(
                &seat,
                &reopened,
                Some(&bare),
                &OpKey("resurrect:t5:final".into())
            )
            .await,
        SeatRestoreOutcome::refused(SeatRestoreRefusal::NotFound)
    );
    // Nothing changed: the old seat stays final and the reopened pane keeps its bare seat (graph's succession).
    assert!(matches!(
        ports.status(&seat).await,
        Some(SeatStatus::Retired {
            kind: RetirementKind::Final,
            ..
        })
    ));
    assert_eq!(ports.map.seat_for(&reopened).await.unwrap(), Some(bare));
}

/// Restart the private Herdr under the running threads daemon and wait until threads holds `seat` (keyless,
/// checked in on `pane` before the restart) `Unresolved` for its restored pane.
async fn restart_until_held(
    herdr: &mut PrivateHerdr,
    ports: &Tier5,
    pane: &HerdrPaneId,
    seat: &ThreadsSeatRef,
) {
    herdr.restart().expect("herdr restart");
    poll_for(
        "the pane restored at its saved id",
        Duration::from_secs(15),
        async || {
            let snapshot = herdr.client().snapshot().await.ok()?;
            snapshot
                .workspaces
                .iter()
                .flat_map(|w| &w.tabs)
                .flat_map(|t| &t.panes)
                .any(|p| &p.id == pane)
                .then_some(())
        },
    )
    .await;
    poll_for(
        "the restart-held seat",
        Duration::from_secs(30),
        async || (ports.status(seat).await? == SeatStatus::Unresolved).then_some(()),
    )
    .await;
}

/// U2: a Herdr restart leaves the keyless seat unresolved and held; graph's `rebind_held_seat` (ResurrectSeat with
/// `replace: None`) places it back on its own pane, and a later rebind reads it already placed.
#[tokio::test(flavor = "multi_thread")]
async fn rebind_places_a_restart_held_seat_on_its_pane() {
    let Some(mut herdr) = tier5_herdr() else {
        return;
    };
    let daemon = tier5_daemon(&herdr, &[]);
    let ports = Tier5::new(&herdr, &daemon);
    let (_ws, pane, seat) = agent_seat(&herdr, &daemon, &ports, "t5-held", "t5-held").await;
    restart_until_held(&mut herdr, &ports, &pane, &seat).await;

    // Until threads' recovery reconciliation of the new host epoch finishes it answers ServiceBusy (transient).
    let outcome = poll_for(
        "the held-seat rebind",
        Duration::from_secs(30),
        async || match ports
            .threads
            .rebind_held_seat(&seat, &pane, &OpKey("rebind:t5:held".into()))
            .await
        {
            SeatRestoreOutcome::Refused {
                retryable: true, ..
            } => None,
            other => Some(other),
        },
    )
    .await;
    assert_eq!(outcome, SeatRestoreOutcome::Done);
    assert_eq!(
        ports.status(&seat).await,
        Some(SeatStatus::Resolved { pane: pane.clone() })
    );
    assert_eq!(ports.map.seat_for(&pane).await.unwrap(), Some(seat.clone()));
    assert_eq!(
        ports
            .threads
            .rebind_held_seat(&seat, &pane, &OpKey("rebind:t5:held:2".into()))
            .await,
        SeatRestoreOutcome::AlreadyResolved
    );
}

/// The operator remedy graph's status rows name (`herdr-threads human seat rebind SEAT --pane PANE --operator`) on
/// 3.6.0: it resurrects a keyless `Retired{Reversible}` seat onto a reopened pane, and places a restart-held seat.
/// Threads 3.6.0 runs it only in its `human` namespace; the bare `seat rebind ...` form is refused.
#[tokio::test(flavor = "multi_thread")]
async fn operator_rebind_resurrects_and_places() {
    let Some(mut herdr) = tier5_herdr() else {
        return;
    };
    let daemon = tier5_daemon(&herdr, &[]);
    let ports = Tier5::new(&herdr, &daemon);
    let remedy = |seat: &ThreadsSeatRef, pane: &HerdrPaneId| {
        vec![
            "seat".to_owned(),
            "rebind".into(),
            seat.0.clone(),
            "--pane".into(),
            pane.0.clone(),
            "--operator".into(),
        ]
    };
    let rebind = |seat: &ThreadsSeatRef, pane: &HerdrPaneId| {
        let args = remedy(seat, pane);
        daemon.human_cli(&args.iter().map(String::as_str).collect::<Vec<_>>())
    };

    // A closed tab's seat, resurrected onto a reopened pane that has no seat yet.
    let (ws, _pane, closed) =
        agent_seat(&herdr, &daemon, &ports, "t5-op-closed", "t5-op-closed").await;
    close_until_reversible(&herdr, &ports, &ws, &closed).await;
    let reopened = herdr
        .client()
        .create_workspace(CreateWorkspace {
            label: "t5-op-reopened".into(),
            cwd: herdr.root.join("home"),
            env: vec![],
        })
        .await
        .expect("reopened workspace")
        .pane
        .expect("reopened pane");
    let bare_form = remedy(&closed, &reopened);
    let refused = daemon.cli(
        None,
        &bare_form.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    assert!(
        !refused.status.success()
            && String::from_utf8_lossy(&refused.stderr)
                .contains("require immediate human namespace"),
        "3.6.0 refuses the remedy outside the human namespace: {} {}",
        stdout(&refused),
        String::from_utf8_lossy(&refused.stderr)
    );
    ok(
        "operator rebind of a reversible seat",
        &rebind(&closed, &reopened),
    );
    assert_eq!(
        ports.status(&closed).await,
        Some(SeatStatus::Resolved {
            pane: reopened.clone()
        })
    );

    // A restart-held seat, placed back on its own pane.
    let (_ws, pane, held) = agent_seat(&herdr, &daemon, &ports, "t5-op-held", "t5-op-held").await;
    restart_until_held(&mut herdr, &ports, &pane, &held).await;
    poll_for(
        "the operator rebind of the held seat",
        Duration::from_secs(30),
        async || rebind(&held, &pane).status.success().then_some(()),
    )
    .await;
    assert_eq!(
        ports.status(&held).await,
        Some(SeatStatus::Resolved { pane: pane.clone() })
    );
}
