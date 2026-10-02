//! The composed daemon (hg-zmi.18): every component built by `compose_with` against FakeHerdr / FakeThreads /
//! ManualClock and a real temp instance, served over a real Unix socket. The subprocess group at the bottom runs
//! the real binary (feature `test-support`). Nothing here touches a live Herdr, live threads or the memory observer.
use herdr_graph::config::InstancePaths;
use herdr_graph::daemon::DaemonCtx;
use herdr_graph::daemon::client::{Client, ClientError};
use herdr_graph::daemon::compose::{STARTUP_STEPS, Services, compose_with, mutation_registry, production_threads};
use herdr_graph::daemon::registry::{CallerInfo, Registry, Shutdown, shutdown_channel};
use herdr_graph::herdr::FakeHerdr;
use herdr_graph::herdr::fake::FakeCall;
use herdr_graph::journal::Journal;
use herdr_graph::model::change::ChangeRequest;
use herdr_graph::model::operation::OpState;
use herdr_graph::plan::commands::PlanDeps;
use herdr_graph::plan::store::PlanStore;
use herdr_graph::ports::clock::ManualClock;
use herdr_graph::ports::herdr::{AgentInfo, AgentSession, AgentStatus, HerdrApi};
use herdr_graph::ports::store::Store;
use herdr_graph::ports::writer::{Writer, WriterError};
use herdr_graph::store::init::init_instance;
use herdr_graph::store::layout;
use herdr_graph::store::tree::CommitView;
use herdr_graph::store::GitStore;
use herdr_graph::threads::discovery::DiscoveryInputs;
use herdr_graph::threads::{FakePaneSeatMap, FakeThreads};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(20);

async fn eventually(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

fn ctx_for(root: &Path) -> DaemonCtx {
    DaemonCtx {
        paths: InstancePaths::new(root),
        herdr_socket: root.join("no-herdr.sock"),
        started_at: chrono::Utc::now(),
    }
}

struct Fakes {
    herdr: Arc<FakeHerdr>,
    threads: Arc<FakeThreads>,
    clock: Arc<ManualClock>,
}

fn fakes() -> (Fakes, Services) {
    let f = Fakes { herdr: FakeHerdr::new(), threads: Arc::new(FakeThreads::new()), clock: Arc::new(ManualClock::new(chrono::Utc::now())) };
    let mut s = Services::new(f.herdr.clone(), f.threads.clone(), Arc::new(FakePaneSeatMap::new()), f.clock.clone());
    s.reminder_period = Duration::from_millis(40);
    (f, s)
}

/// A composed daemon serving on a temp socket, with its loops spawned.
struct Daemon {
    root: PathBuf,
    sock: PathBuf,
    registry: Arc<Registry>,
    fakes: Fakes,
    stop: Arc<tokio::sync::watch::Sender<bool>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Daemon {
    /// Compose against `root` (an initialised instance) without serving or spawning anything.
    async fn compose_only(root: &Path, services: Services) -> Registry {
        let mut reg = Registry::default();
        compose_with(&mut reg, &ctx_for(root), services).await.expect("compose");
        reg
    }

    async fn start(root: &Path) -> Daemon {
        let (fakes, services) = fakes();
        Self::start_with(root, fakes, services).await
    }

    async fn start_with(root: &Path, fakes: Fakes, services: Services) -> Daemon {
        let mut reg = Self::compose_only(root, services).await;
        let (stop, shutdown): (_, Shutdown) = shutdown_channel();
        let tasks = reg.take_loops().into_iter().map(|(_, f)| {
            let fut = f(shutdown.clone());
            tokio::spawn(async move {
                let _ = fut.await;
            })
        });
        let tasks = tasks.collect();
        let sock = root.join("t.sock");
        let _ = std::fs::remove_file(&sock);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();
        let registry = Arc::new(reg);
        let builtins = herdr_graph::daemon::server::Builtins {
            instance: root.to_path_buf(),
            herdr_socket: root.join("no-herdr.sock"),
            started_at: chrono::Utc::now(),
            shutdown_tx: stop.clone(),
        };
        let server = tokio::spawn(herdr_graph::daemon::server::serve(listener, registry.clone(), builtins, shutdown));
        let mut tasks: Vec<_> = tasks;
        tasks.push(server);
        Daemon { root: root.to_path_buf(), sock, registry, fakes, stop, tasks }
    }

    async fn call(&self, kind: &str, args: Value) -> Result<Value, ClientError> {
        let (sock, kind) = (self.sock.clone(), kind.to_owned());
        tokio::task::spawn_blocking(move || {
            let mut c = Client::connect(&sock, Duration::from_secs(30)).map_err(|e| ClientError::Unavailable(e.to_string()))?;
            c.call(&kind, args)
        })
        .await
        .unwrap()
    }

    /// `plan.create` then `plan.apply` with the relay confirmation; returns the apply reply.
    async fn change(&self, words: &[&str], caller: Option<Value>) -> Value {
        let with_caller = |mut v: Value| {
            if let Some(c) = &caller {
                v["_caller"] = c.clone();
            }
            v
        };
        let plan = self.call("plan.create", with_caller(json!({ "words": words }))).await.unwrap_or_else(|e| panic!("plan {words:?}: {e}"));
        let apply = json!({ "plan": plan["plan_id"], "confirm": plan["hash"], "mode": "relay" });
        self.call("plan.apply", with_caller(apply)).await.unwrap_or_else(|e| panic!("apply {words:?}: {e}"))
    }

    async fn committed(&self, words: &[&str]) -> Value {
        let r = self.change(words, None).await;
        assert_eq!(r["state"], "committed", "{words:?}: {r}");
        r
    }

    fn store(&self) -> GitStore {
        GitStore::open(&self.root).unwrap()
    }

    fn with_view<T>(&self, f: impl FnOnce(&dyn herdr_graph::store::tree::TreeRead) -> T) -> T {
        let store = self.store();
        let at = store.head().unwrap();
        f(&CommitView { store: &store, at })
    }

    async fn stop(self) {
        let _ = self.stop.send(true);
        for t in self.tasks {
            let _ = tokio::time::timeout(Duration::from_secs(10), t).await;
        }
    }
}

fn new_instance() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("instance");
    init_instance(&root).unwrap();
    (dir, root)
}

// ---------------------------------------------------------------------------------------------
// in-process
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn composed_daemon_plan_apply_via_ipc_commits() {
    let (_dir, root) = new_instance();
    let d = Daemon::start(&root).await;

    let ts = d.committed(&["teamspace", "create", "t", "--active"]).await;
    assert!(ts["commit"].is_string(), "{ts}");
    let before = d.fakes.herdr.calls();
    assert!(before.len() <= 1, "{before:?}");

    d.committed(&["seat", "create", "foreman", "--teamspace", "t", "--active", "--harness", "shell"]).await;
    let herdr = d.fakes.herdr.clone();
    eventually("a CreateWorkspace and CreateTab from the loop step", || {
        let calls = herdr.calls();
        calls.iter().any(|c| matches!(c, FakeCall::CreateWorkspace(_))) && calls.iter().any(|c| matches!(c, FakeCall::CreateTab(_)))
    })
    .await;
    let tabs = herdr.calls().iter().filter(|c| matches!(c, FakeCall::CreateTab(_))).count();
    assert_eq!(tabs, 1, "one tab for one seat");

    // The op is in the journal as committed, and `ops.list` (a composed command) sees it.
    let ops = d.call("ops.list", json!({})).await.unwrap();
    assert!(ops["ops"].as_array().unwrap().iter().filter(|o| o["state"] == "committed").count() >= 2, "{ops}");
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn writer_resume_over_ipc_clears_halt() {
    let (_dir, root) = new_instance();
    let d = Daemon::start(&root).await;
    let journal = Journal::open(&InstancePaths::new(&root).journal).unwrap();
    journal.meta_set(herdr_graph::writer::WRITER_HALTED, "lock contention on main").unwrap();
    let status = d.call("status", json!({})).await.unwrap();
    assert_eq!(status["components"]["writer"]["writer_halted"], "lock contention on main");

    let r = d.call("writer.resume", json!({})).await.unwrap();
    assert_eq!(r["was_halted"], true, "{r}");
    assert_eq!(r["reason"], "lock contention on main", "{r}");
    assert!(r["requeued"].as_array().unwrap().is_empty(), "{r}");
    let status = d.call("status", json!({})).await.unwrap();
    assert!(status["components"]["writer"]["writer_halted"].is_null(), "{status}");
    assert_eq!(journal.meta_get(herdr_graph::writer::WRITER_HALTED).unwrap(), None);

    d.committed(&["teamspace", "create", "after-resume"]).await;
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn writer_resume_not_halted() {
    let (_dir, root) = new_instance();
    let d = Daemon::start(&root).await;
    let r = d.call("writer.resume", json!({})).await.unwrap();
    assert_eq!(r["was_halted"], false, "{r}");
    assert!(r["reason"].is_null(), "{r}");
    d.stop().await;
}

const ORG_KINDS: [&str; 22] = [
    "teamspace_create",
    "teamspace_rename",
    "teamspace_retire",
    "teamspace_resurrect",
    "seat_create",
    "seat_activate",
    "seat_deactivate",
    "seat_rename",
    "seat_retire",
    "seat_resurrect",
    "seat_override",
    "clone_add",
    "clone_retire",
    "clone_rebind",
    "participation_join",
    "participation_leave",
    "template_create",
    "template_edit",
    "template_copy",
    "application_apply",
    "application_retire",
    "undo",
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_kind_and_command_registered() {
    let (_dir, root) = new_instance();
    let d = Daemon::start(&root).await;
    let have = d.registry.command_kinds();
    for want in [
        "plan.create",
        "plan.apply",
        "plan.show",
        "ops.list",
        "ops.get",
        "ops.cancel",
        "ops.reassign",
        "ops.check_instruction",
        "undo.list",
        "undo.apply",
        "seat.resolve",
        "content.write",
        "request.list",
        "request.ack",
        "request.complete",
        "session.report",
        "who",
    ] {
        assert!(have.iter().any(|k| k == want), "command {want} not registered; have {have:?}");
    }

    let (_kinds, muts) = mutation_registry(Arc::new(PlanStore::new(root.join(".graph-local/plans"))));
    let keys = muts.keys();
    let mut want: Vec<String> = ORG_KINDS.iter().map(|s| s.to_string()).collect();
    want.extend(
        [
            "observed.cascade",
            "observed.rename",
            "observed.occupancy",
            "observed.move",
            "observed.availability",
            "bookkeeping.binding",
            "bookkeeping.runtime",
            "bookkeeping.channel",
            "bookkeeping.invitation",
            "bookkeeping.transcript",
            "bookkeeping.request_create",
            "bookkeeping.request_delivery",
            "bookkeeping.request_ack",
            "bookkeeping.request_complete",
            "bookkeeping.request_unresolved",
            "bookkeeping.reassign",
            "content_write",
        ]
        .map(String::from),
    );
    for k in &want {
        assert!(keys.contains(k), "mutation {k} not registered; have {keys:?}");
    }
    // The composed daemon's writer admits only registered kinds: its registry is this same function's output.
    d.stop().await;
}

/// A writer that only journals: admission leaves the op `applying`, as a crash mid-apply would.
struct JournalOnly(Arc<Journal>);

impl Writer for JournalOnly {
    fn admit(&self, request: ChangeRequest) -> Result<herdr_graph::model::OpId, WriterError> {
        let op = self.0.admit(&request, chrono::Utc::now()).map_err(|e| WriterError::Journal(e.to_string()))?;
        self.0.begin_applying(&op, chrono::Utc::now()).map_err(|e| WriterError::Journal(e.to_string()))?;
        Ok(op)
    }
    fn status(&self, op: &herdr_graph::model::OpId) -> Result<Option<OpState>, WriterError> {
        Ok(self.0.get(op).map_err(|e| WriterError::Journal(e.to_string()))?.map(|r| r.state))
    }
}

fn startup_record(journal: &Journal) -> Vec<String> {
    (0..STARTUP_STEPS.len()).filter_map(|n| journal.meta_get(&format!("startup:{n}")).unwrap()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn startup_convergence_order() {
    let (_dir, root) = new_instance();
    let paths = InstancePaths::new(&root);
    std::fs::create_dir_all(&paths.local).unwrap();
    let journal = Arc::new(Journal::open(&paths.journal).unwrap());

    // Simulate a crash mid-apply: a confirmed plan whose op was admitted and left `applying`.
    let store: Arc<dyn Store> = Arc::new(GitStore::open(&root).unwrap());
    let plans = Arc::new(PlanStore::new(paths.plans.clone()));
    let (kinds, _) = mutation_registry(plans.clone());
    let deps = PlanDeps {
        kinds,
        plans,
        store,
        writer: Arc::new(JournalOnly(journal.clone())),
        clock: Arc::new(ManualClock::new(chrono::Utc::now())),
        instance: root.clone(),
    };
    let caller = Default::default();
    let plan = herdr_graph::plan::commands::create_plan(&deps, &caller, vec!["teamspace".into(), "create".into(), "t".into()]).unwrap();
    let op = herdr_graph::plan::commands::admit_apply(&deps, &caller, plan["plan_id"].as_str().unwrap(), plan["hash"].as_str(), "relay").unwrap();
    assert_eq!(journal.get(&op).unwrap().unwrap().state, OpState::Applying);
    drop(deps);

    // Stale record from an earlier boot must not survive into this one.
    journal.meta_set("startup:9", "stale").unwrap();
    journal.meta_set("startup:0", "stale").unwrap();

    let (f, services) = fakes();
    let mut reg = Daemon::compose_only(&root, services).await;

    // compose_with returned but no loop has been spawned: everything below was done by startup itself.
    assert_eq!(startup_record(&journal), STARTUP_STEPS.to_vec());
    assert_eq!(journal.meta_get("startup:9").unwrap(), None, "records of an earlier boot are cleared");
    let row = journal.get(&op).unwrap().unwrap();
    assert_eq!(row.state, OpState::Committed, "recovery + drain commit the op before any loop exists");
    let loops: Vec<String> = reg.take_loops().into_iter().map(|(n, _)| n).collect();
    for want in ["writer", "observer", "plan.reminders", "transcripts.session_ended", "transcripts.watcher", "transcripts.liveness", "threads.connection"] {
        assert!(loops.iter().any(|n| n == want), "loop {want} missing from {loops:?}");
    }
    drop(f);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recomposing_against_a_live_herdr_creates_nothing_twice() {
    // The daemon restarts while Herdr keeps running: the first pass finds everything it created before, in
    // the snapshot, and executes nothing again.
    let (_dir, root) = new_instance();
    let (fakes, services) = fakes();
    let (herdr, threads, clock) = (fakes.herdr.clone(), fakes.threads.clone(), fakes.clock.clone());
    let d = Daemon::start_with(&root, fakes, services).await;
    d.committed(&["teamspace", "create", "t", "--active"]).await;
    d.committed(&["seat", "create", "foreman", "--teamspace", "t", "--active", "--harness", "shell"]).await;
    eventually("the tab and its binding", || {
        herdr.calls().iter().any(|c| matches!(c, FakeCall::CreateTab(_))) && bound_pane(&d, "foreman").is_some()
    })
    .await;
    d.stop().await;

    herdr.clear_calls();
    let again = Services::new(herdr.clone(), threads, Arc::new(FakePaneSeatMap::new()), clock);
    let _reg = Daemon::compose_only(&root, again).await;
    let creates = herdr
        .calls()
        .into_iter()
        .filter(|c| matches!(c, FakeCall::CreateWorkspace(_) | FakeCall::CreateTab(_) | FakeCall::SplitPane(_)))
        .count();
    assert_eq!(creates, 0, "{:?}", herdr.calls());
}

fn graph_seat(d: &Daemon, name: &str) -> herdr_graph::model::seat::SeatRecord {
    d.with_view(|v| layout::all_seats(v).unwrap().into_iter().map(|(_, s)| s).find(|s| s.name == name)).unwrap_or_else(|| panic!("no seat {name}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reminder_loop_fires_on_fake_clock() {
    let (_dir, root) = new_instance();
    let d = Daemon::start(&root).await;
    d.committed(&["teamspace", "create", "t", "--active"]).await;
    d.committed(&["seat", "create", "foreman", "--teamspace", "t", "--active", "--harness", "shell"]).await;
    eventually("the seat channel thread", || graph_seat(&d, "foreman").channel.thread_id.is_some()).await;
    let seat = graph_seat(&d, "foreman");
    let caller = serde_json::to_value(CallerInfo { graph_seat: Some(seat.id.to_string()), ..Default::default() }).unwrap();

    // Two plans against the same revision: the second one is stale once the first is applied, and is rejected.
    let plan = |new: &str| {
        let d = &d;
        let caller = caller.clone();
        let new = new.to_owned();
        async move { d.call("plan.create", json!({ "words": ["seat", "rename", "foreman", new], "_caller": caller })).await.unwrap() }
    };
    let (a, b) = (plan("boss").await, plan("chief").await);
    let apply = |p: &Value| json!({ "plan": p["plan_id"], "confirm": p["hash"], "mode": "relay", "_caller": caller.clone() });
    assert_eq!(d.call("plan.apply", apply(&a)).await.unwrap()["state"], "committed");
    let rejected = d.call("plan.apply", apply(&b)).await.unwrap();
    assert_eq!(rejected["state"], "rejected", "{rejected}");
    let op = rejected["op"].as_str().unwrap().to_owned();

    // The initial notice goes out at once; the +1 h reminder only after the fake clock moves.
    let thread = seat.channel.thread_id.clone().unwrap();
    let sent = |index: u32| {
        let want = format!("rem:{op}:{index}");
        let threads = d.fakes.threads.clone();
        let thread = thread.clone();
        move || threads.notifications().iter().any(|n| n.thread.0 == thread && n.op_key.0 == want)
    };
    eventually("the initial notice", sent(0)).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!sent(1)(), "no reminder before an hour passed");
    d.fakes.clock.advance(chrono::Duration::hours(1));
    eventually("the +1 h reminder", sent(1)).await;
    assert!(!sent(2)(), "the +6 h reminder is not due");
    d.stop().await;
}

fn claude(session: AgentSession) -> Option<AgentInfo> {
    Some(AgentInfo { kind: "claude".into(), status: AgentStatus::Idle, session: Some(session) })
}

fn clone_of(d: &Daemon, seat: &str) -> herdr_graph::model::clone::CloneRecord {
    let id = graph_seat(d, seat).id;
    d.with_view(|v| layout::all_clones(v).unwrap().into_iter().map(|(_, c)| c).find(|c| c.seat == id)).unwrap_or_else(|| panic!("no clone of {seat}"))
}

fn bound_pane(d: &Daemon, seat: &str) -> Option<herdr_graph::model::HerdrPaneId> {
    clone_of(d, seat).runtime.bound.and_then(|b| b.pane_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_end_flows_to_transcripts() {
    let (dir, root) = new_instance();
    let d = Daemon::start(&root).await;
    d.committed(&["teamspace", "create", "alpha", "--active"]).await;
    d.committed(&["seat", "create", "worker", "--teamspace", "alpha", "--active", "--harness", "claude"]).await;
    d.committed(&["seat", "create", "sum", "--teamspace", "alpha", "--active", "--harness", "claude", "--role", "summarizer"]).await;
    eventually("both panes bound", || bound_pane(&d, "worker").is_some() && bound_pane(&d, "sum").is_some()).await;
    eventually("channels", || graph_seat(&d, "sum").channel.thread_id.is_some()).await;
    let (worker_pane, sum_pane) = (bound_pane(&d, "worker").unwrap(), bound_pane(&d, "sum").unwrap());

    // The summarizer is staffed; the worker runs a session whose transcript is a real file.
    d.fakes.herdr.set_agent(&sum_pane, claude(AgentSession::Id("sum-session".into())));
    eventually("summarizer occupant", || clone_of(&d, "sum").occupant.is_some()).await;
    let transcript = dir.path().join("worker-session.jsonl");
    std::fs::write(&transcript, "{\"n\":1}\n{\"n\":2}\n").unwrap();
    d.fakes.herdr.set_agent(&worker_pane, claude(AgentSession::Path(transcript.clone())));
    eventually("worker occupant", || clone_of(&d, "worker").occupant.is_some()).await;

    // The agent exits: the observer commits the end, the hook reaches transcripts, a request is created and
    // delivered to the summarizer's channel (fallback Notify).
    d.fakes.herdr.set_agent(&worker_pane, None);
    let sum_thread = graph_seat(&d, "sum").channel.thread_id.clone().unwrap();
    let threads = d.fakes.threads.clone();
    eventually("a delivery notice naming the request", || {
        threads.notifications().iter().any(|n| n.thread.0 == sum_thread && n.body.contains("rq_"))
    })
    .await;
    let requests = d.with_view(|v| layout::list_requests(v).unwrap());
    assert_eq!(requests.len(), 1, "exactly one request for the ended session");
    d.stop().await;
}

// ---------------------------------------------------------------------------------------------
// undo adopts the caller's pane (spec §6; D1)
// ---------------------------------------------------------------------------------------------

fn clones_of_seat(d: &Daemon, seat: &str) -> Vec<herdr_graph::model::clone::CloneRecord> {
    let id = graph_seat(d, seat).id;
    d.with_view(|v| layout::all_clones(v).unwrap().into_iter().map(|(_, c)| c).filter(|c| c.seat == id).collect())
}

/// The newest undo candidate whose summary contains `what`.
async fn undo_act(d: &Daemon, what: &str) -> String {
    let list = d.call("undo.list", json!({})).await.unwrap();
    let c = list["candidates"].as_array().unwrap().iter().find(|c| c["summary"].as_str().is_some_and(|s| s.contains(what)));
    c.unwrap_or_else(|| panic!("no undo candidate containing {what:?}: {list}"))["act"].as_str().unwrap().to_owned()
}

fn herdr_creates(d: &Daemon) -> usize {
    d.fakes.herdr.calls().iter().filter(|c| matches!(c, FakeCall::SplitPane(_) | FakeCall::CreateTab(_) | FakeCall::CreateWorkspace(_))).count()
}

/// plan.create `["undo", act]` from `caller_pane`, then `undo.apply` with the plan's hash.
async fn undo_from(d: &Daemon, act: &str, caller_pane: &herdr_graph::model::HerdrPaneId) -> Value {
    let caller = serde_json::to_value(CallerInfo { pane_id: Some(caller_pane.0.clone()), ..Default::default() }).unwrap();
    let plan = d.call("plan.create", json!({ "words": ["undo", act], "_caller": caller })).await.unwrap();
    assert!(
        plan["plan"]["effects"].as_array().is_some_and(|e| e.iter().any(|e| e["kind"] == "undo.adopt_pane")),
        "the preview names the adoption: {plan}"
    );
    let r = d.call("undo.apply", json!({ "plan": plan["plan_id"], "confirm": plan["hash"], "mode": "relay", "_caller": caller })).await.unwrap();
    assert_eq!(r["state"], "admitted", "{r}");
    let deadline = Instant::now() + WAIT;
    loop {
        let op = d.call("ops.get", json!({ "op": r["op"] })).await.unwrap();
        let state = op["state"].as_str().or(op["op"]["state"].as_str()).unwrap_or("").to_owned();
        if !matches!(state.as_str(), "admitted" | "applying" | "") {
            assert_eq!(state, "committed", "{op}");
            return r;
        }
        assert!(Instant::now() < deadline, "the undo never finished: {op}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The daemon is quiet: no Herdr call for a while.
async fn settle(d: &Daemon) {
    let mut last = d.fakes.herdr.calls().len();
    let mut stable = 0;
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let n = d.fakes.herdr.calls().len();
        if n == last {
            stable += 1;
            if stable >= 8 {
                return;
            }
        } else {
            stable = 0;
            last = n;
        }
    }
    panic!("the daemon never went quiet");
}

async fn two_clone_seat(d: &Daemon) {
    d.committed(&["teamspace", "create", "t", "--active"]).await;
    d.committed(&["seat", "create", "foreman", "--teamspace", "t", "--active", "--harness", "shell"]).await;
    eventually("the first clone's pane", || bound_pane(d, "foreman").is_some()).await;
    d.committed(&["clone", "add", "foreman"]).await;
    eventually("both clones' panes", || {
        let cs = clones_of_seat(d, "foreman");
        cs.len() == 2 && cs.iter().all(|c| c.runtime.bound.as_ref().is_some_and(|b| b.pane_id.is_some()))
    })
    .await;
    settle(d).await;
}

fn tab_of_pane(snap: &herdr_graph::ports::herdr::HerdrSnapshot, pane: &herdr_graph::model::HerdrPaneId) -> herdr_graph::model::HerdrTabId {
    snap.workspaces.iter().flat_map(|w| w.tabs.iter()).find(|t| t.panes.iter().any(|p| &p.id == pane)).expect("pane in a tab").id.clone()
}

fn seat_lifecycle(d: &Daemon, name: &str) -> herdr_graph::model::common::Lifecycle {
    d.with_view(|v| layout::all_seats(v).unwrap().into_iter().map(|(_, s)| s).filter(|s| s.name == name).max_by_key(|s| s.rev))
        .map(|s| s.lifecycle)
        .unwrap_or_else(|| panic!("no seat {name}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn undo_from_pane_adopts_caller_pane_without_creating_one() {
    use herdr_graph::model::common::{Availability, CloneLifecycle};
    let (_dir, root) = new_instance();
    let d = Daemon::start(&root).await;
    two_clone_seat(&d).await;
    let victim = clones_of_seat(&d, "foreman").into_iter().find(|c| c.runtime.bound.as_ref().and_then(|b| b.pane_id.clone()) != bound_pane(&d, "foreman")).unwrap();
    let victim_pane = victim.runtime.bound.as_ref().and_then(|b| b.pane_id.clone()).unwrap();
    let tab = tab_of_pane(&d.fakes.herdr.snapshot().await.unwrap(), &victim_pane);

    d.fakes.herdr.user_close_pane(&victim_pane);
    eventually("the victim to retire", || clones_of_seat(&d, "foreman").iter().any(|c| c.id == victim.id && c.retired.is_some())).await;
    settle(&d).await;
    let act = undo_act(&d, "closure").await;
    let caller = d.fakes.herdr.add_user_pane(&tab, "caller");
    d.fakes.herdr.clear_calls();

    undo_from(&d, &act, &caller).await;
    eventually("the victim to be active again", || clones_of_seat(&d, "foreman").iter().any(|c| c.id == victim.id && c.retired.is_none())).await;
    settle(&d).await;

    assert_eq!(herdr_creates(&d), 0, "no pane or tab is created for the adopted clone: {:?}", d.fakes.herdr.calls());
    let now = clones_of_seat(&d, "foreman").into_iter().find(|c| c.id == victim.id).unwrap();
    assert_eq!(now.lifecycle, CloneLifecycle::Active);
    assert_eq!(now.runtime.availability, Availability::Present);
    assert_eq!(now.runtime.bound.as_ref().and_then(|b| b.pane_id.clone()), Some(caller.clone()));
    let snap = d.fakes.herdr.snapshot().await.unwrap();
    let pane = snap.workspaces.iter().flat_map(|w| w.tabs.iter()).flat_map(|t| t.panes.iter()).find(|p| p.id == caller).unwrap();
    assert!(pane.metadata.values().any(|v| v.contains(&victim.id.to_string())), "the caller pane carries hg=<clone>: {:?}", pane.metadata);
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn undo_tab_close_from_pane_adopts_caller_pane() {
    use herdr_graph::model::common::Lifecycle;
    let (_dir, root) = new_instance();
    let d = Daemon::start(&root).await;
    d.committed(&["teamspace", "create", "t", "--active"]).await;
    d.committed(&["seat", "create", "foreman", "--teamspace", "t", "--active", "--harness", "shell"]).await;
    eventually("the pane", || bound_pane(&d, "foreman").is_some()).await;
    settle(&d).await;
    let snap = d.fakes.herdr.snapshot().await.unwrap();
    let foreman_tab = tab_of_pane(&snap, &bound_pane(&d, "foreman").unwrap());
    // The caller's own tab: a plain tab of the same workspace that belongs to no seat.
    let ws = snap.workspaces[0].id.clone();
    let scratch = d
        .fakes
        .herdr
        .create_tab(herdr_graph::ports::herdr::CreateTab { workspace: ws, label: "scratch".into(), cwd: "/".into(), env: vec![] })
        .await
        .unwrap();
    let scratch_tab = scratch.tab.unwrap();
    let clone = clone_of(&d, "foreman");

    d.fakes.herdr.user_close_tab(&foreman_tab);
    eventually("the seat to retire", || seat_lifecycle(&d, "foreman") == Lifecycle::Retired).await;
    settle(&d).await;
    let act = undo_act(&d, "closure").await;
    let caller = d.fakes.herdr.add_user_pane(&scratch_tab, "caller");
    d.fakes.herdr.clear_calls();

    undo_from(&d, &act, &caller).await;
    eventually("the seat to be active again", || seat_lifecycle(&d, "foreman") == Lifecycle::Active).await;
    settle(&d).await;

    assert_eq!(herdr_creates(&d), 0, "no tab is created for the adopted clone: {:?}", d.fakes.herdr.calls());
    let now = clone_of(&d, "foreman");
    assert_eq!(now.id, clone.id);
    assert_eq!(now.runtime.bound.as_ref().and_then(|b| b.pane_id.clone()), Some(caller.clone()));
    assert_eq!(now.runtime.availability, herdr_graph::model::common::Availability::Present);
    let snap = d.fakes.herdr.snapshot().await.unwrap();
    let pane = snap.workspaces.iter().flat_map(|w| w.tabs.iter()).flat_map(|t| t.panes.iter()).find(|p| p.id == caller).unwrap();
    assert!(pane.metadata.values().any(|v| v.contains(&clone.id.to_string())), "the caller pane carries hg=<clone>: {:?}", pane.metadata);
    d.stop().await;
}

// ---------------------------------------------------------------------------------------------
// subprocess: the real binary
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "test-support")]
mod subprocess {
    use super::*;
    use herdr_graph::daemon::lock;
    use std::os::unix::net::UnixListener;
    use std::process::{Child, Command, Output, Stdio};

    struct Fixture {
        _dir: tempfile::TempDir,
        home: PathBuf,
        instance: PathBuf,
        herdr_socket: PathBuf,
        _listener: UnixListener,
        children: std::cell::RefCell<Vec<Child>>,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let home = dir.path().join("home");
            std::fs::create_dir_all(&home).unwrap();
            let instance = dir.path().join("instance");
            init_instance(&instance).unwrap();
            let herdr_socket = dir.path().join("herdr.sock");
            let listener = UnixListener::bind(&herdr_socket).unwrap();
            let l2 = listener.try_clone().unwrap();
            std::thread::spawn(move || for _conn in l2.incoming() {});
            Fixture { _dir: dir, home, instance, herdr_socket, _listener: listener, children: Default::default() }
        }

        fn cmd(&self, args: &[&str]) -> Command {
            let mut c = Command::new(env!("CARGO_BIN_EXE_herdr-graph"));
            c.args(args)
                .env_clear()
                .env("PATH", std::env::var("PATH").unwrap_or_default())
                .env("HOME", &self.home)
                .env("HERDR_GRAPH_INSTANCE", &self.instance)
                .env("HERDR_SOCKET_PATH", &self.herdr_socket)
                .env("HG_TEST_FAKE_SERVICES", "1");
            c
        }

        fn run(&self, args: &[&str]) -> Output {
            self.cmd(args).output().unwrap()
        }

        fn paths(&self) -> InstancePaths {
            InstancePaths::new(&self.instance)
        }

        fn client(&self) -> Client {
            Client::connect(&self.paths().socket, Duration::from_secs(30)).unwrap()
        }

        /// A foreground daemon child (so its exit status is observable), optionally with failpoints armed.
        fn spawn_daemon(&self, failpoints: Option<&str>) -> Child {
            let mut c = self.cmd(&["daemon"]);
            if let Some(fp) = failpoints {
                c.env("HG_FAILPOINTS", fp);
            }
            let log = std::fs::OpenOptions::new().create(true).append(true).open(self._dir.path().join("daemon.out")).unwrap();
            let child = c.stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log).spawn().unwrap();
            let deadline = Instant::now() + WAIT_SECS;
            while herdr_graph::daemon::client::hello(&self.paths().socket).is_none() {
                assert!(Instant::now() < deadline, "daemon did not come up; log: {}", self.daemon_log());
                std::thread::sleep(Duration::from_millis(50));
            }
            child
        }

        fn daemon_log(&self) -> String {
            std::fs::read_to_string(self._dir.path().join("daemon.out")).unwrap_or_default()
        }

        fn plan(&self, words: &[&str]) -> Value {
            let mut args = vec!["plan", "--json"];
            args.extend_from_slice(words);
            let out = self.run(&args);
            assert!(out.status.success(), "plan {words:?}: {}", String::from_utf8_lossy(&out.stderr));
            serde_json::from_slice(&out.stdout).expect("plan --json prints JSON")
        }

        fn apply(&self, plan: &Value) -> Output {
            self.run(&["apply", plan["plan_id"].as_str().unwrap(), "--confirm", plan["hash"].as_str().unwrap(), "--confirmed-by", "user-relay"])
        }

        fn fake_log(&self) -> Vec<Value> {
            std::fs::read(self.paths().local.join("fake-herdr.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default()
        }

        fn fake_count(&self, op: &str) -> usize {
            self.fake_log().iter().filter(|e| e["op"] == op).count()
        }

        fn journal(&self) -> Journal {
            Journal::open(&self.paths().journal).unwrap()
        }
    }

    const WAIT_SECS: Duration = Duration::from_secs(30);

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Ok(mut c) = Client::connect(&self.paths().socket, Duration::from_secs(2)) {
                let _ = c.call("shutdown", json!({}));
            }
            for mut child in self.children.borrow_mut().drain(..) {
                let deadline = Instant::now() + Duration::from_secs(5);
                while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(50));
                }
                if child.try_wait().ok().flatten().is_none() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
            let _ = std::fs::remove_file(self.paths().socket);
        }
    }

    fn wait_exit(child: &mut Child) -> std::process::ExitStatus {
        let deadline = Instant::now() + WAIT_SECS;
        loop {
            if let Some(s) = child.try_wait().unwrap() {
                return s;
            }
            assert!(Instant::now() < deadline, "daemon did not exit");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn pid_alive(pid: u32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[test]
    fn kill_and_restart_recovers_journal_before_loops() {
        let f = Fixture::new();
        let out = f.run(&["daemon", "--ensure"]);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let plan = f.plan(&["teamspace", "create", "t"]);
        let applied = f.apply(&plan);
        assert!(applied.status.success(), "{}", String::from_utf8_lossy(&applied.stderr));
        assert_eq!(startup_record(&f.journal()), STARTUP_STEPS.to_vec());

        // SIGKILL the daemon (pid from the lock info, argv-verified: never a stray process).
        let pid = lock::read_info(&f.paths().lock).expect("lock info").pid;
        let ps = Command::new("ps").args(["-o", "command=", "-p", &pid.to_string()]).output().unwrap();
        let cmdline = String::from_utf8_lossy(&ps.stdout).into_owned();
        assert!(cmdline.contains(env!("CARGO_BIN_EXE_herdr-graph")) && cmdline.contains(" daemon"), "not our daemon: {cmdline}");
        // SAFETY: the pid was verified above to be this test's daemon.
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGKILL) }, 0);
        let deadline = Instant::now() + WAIT_SECS;
        while pid_alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!pid_alive(pid));

        let out = f.run(&["daemon", "--ensure"]);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let status = f.client().call("status", json!({})).unwrap();
        assert_eq!(status["components"]["writer"]["ops"]["committed"], 1, "{status}");
        assert_ne!(status["pid"].as_u64().unwrap() as u32, pid);
        assert_eq!(startup_record(&f.journal()), STARTUP_STEPS.to_vec(), "the restart ran the startup order again");
    }

    #[test]
    fn crash_between_commit_and_journal_is_recovered_before_loops() {
        let f = Fixture::new();
        let mut daemon = f.spawn_daemon(Some("writer.after_cas_before_journal=exit:43"));
        let plan = f.plan(&["teamspace", "create", "t"]);
        let applied = f.apply(&plan);
        assert!(!applied.status.success(), "the daemon died mid-commit; apply cannot succeed");
        assert_eq!(wait_exit(&mut daemon).code(), Some(43), "{}", f.daemon_log());
        // Git has the commit but the journal still shows the op unfinished.
        let journal = f.journal();
        assert!(!journal.counts().unwrap().contains_key("committed"), "{:?}", journal.counts());
        drop(journal);

        let mut again = f.spawn_daemon(None);
        let status = f.client().call("status", json!({})).unwrap();
        assert_eq!(status["components"]["writer"]["ops"]["committed"], 1, "{status}");
        let store = GitStore::open(&f.instance).unwrap();
        let head = store.head().unwrap();
        let view = CommitView { store: &store, at: head };
        assert_eq!(layout::list_teamspaces(&view).unwrap().len(), 1);
        f.client().call("shutdown", json!({})).unwrap();
        wait_exit(&mut again);
    }

    #[test]
    fn crash_mid_effect_executes_at_most_once() {
        let f = Fixture::new();
        // Phase 1: a live teamspace (workspace and one tab) next to a dormant seat, shut down cleanly.
        let mut first = f.spawn_daemon(None);
        for words in [
            &["teamspace", "create", "t", "--active"][..],
            &["seat", "create", "keeper", "--teamspace", "t", "--active", "--harness", "shell"][..],
            &["seat", "create", "worker", "--teamspace", "t", "--harness", "shell"][..],
        ] {
            let out = f.apply(&f.plan(words));
            assert!(out.status.success(), "{words:?}: {}", String::from_utf8_lossy(&out.stderr));
        }
        let deadline = Instant::now() + WAIT_SECS;
        while f.fake_count("create_workspace") < 1 || f.fake_count("create_tab") < 1 {
            assert!(Instant::now() < deadline, "workspace and tab never created; log: {}", f.daemon_log());
            std::thread::sleep(Duration::from_millis(50));
        }
        // Let every effect of phase 1 finish: one still pending at shutdown would run first in phase 2 and take
        // the armed failpoint instead of the worker's CreateTab.
        loop {
            let status = f.client().call("status", json!({})).unwrap();
            let r = &status["components"]["reconciler"];
            if r["pending_effects"] == 0 && r["unknown_effects"] == 0 {
                break;
            }
            assert!(Instant::now() < deadline, "phase 1 effects never settled: {status}");
            std::thread::sleep(Duration::from_millis(50));
        }
        f.client().call("shutdown", json!({})).unwrap();
        wait_exit(&mut first);
        assert_eq!(f.fake_count("create_tab"), 1, "only the keeper's tab so far");

        // Phase 2: activating the seat dispatches CreateTab, and the daemon dies before it records the result.
        let mut doomed = f.spawn_daemon(Some("reconcile.mid_effect.create_tab=exit:42"));
        let _ = f.apply(&f.plan(&["seat", "activate", "worker"]));
        assert_eq!(wait_exit(&mut doomed).code(), Some(42), "{}", f.daemon_log());
        assert_eq!(f.fake_count("create_tab"), 2, "the worker's CreateTab reached Herdr before the crash");

        // Phase 3: restart without failpoints: the effect is re-correlated from the snapshot, not re-executed.
        let mut third = f.spawn_daemon(None);
        let deadline = Instant::now() + WAIT_SECS;
        loop {
            let status = f.client().call("status", json!({})).unwrap();
            let r = &status["components"]["reconciler"];
            if r["pending_effects"] == 0 && r["unknown_effects"] == 0 {
                break;
            }
            assert!(Instant::now() < deadline, "effects never settled: {status}");
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(f.fake_count("create_tab"), 2, "at most once: {:?}", f.fake_log());
        assert_eq!(f.fake_count("create_workspace"), 1, "{:?}", f.fake_log());
        f.client().call("shutdown", json!({})).unwrap();
        wait_exit(&mut third);
    }
}

// ---------------------------------------------------------------------------------------------
// the threads port built exactly as production builds it (hg-zmi.46): no HERDR_GRAPH_THREADS_* variable
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_threads_port_finds_default_state_dir_without_env() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let threads_dir = home.join(".local/state/herdr/plugins/herdr-threads");
    std::fs::create_dir_all(&threads_dir).unwrap();
    let inputs = DiscoveryInputs { home: Some(home), ..Default::default() };
    let (threads, _map) = production_threads(&ctx_for(dir.path()), inputs);
    let err = threads.delivery_capability().await.expect_err("no threads daemon runs in the empty state dir").to_string();
    assert!(err.contains(&threads_dir.display().to_string()), "discovery must name the directory it found: {err}");
    assert!(!err.contains("not found") && !err.contains("not configured"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_threads_port_reports_missing_install() {
    let dir = tempfile::tempdir().unwrap();
    let inputs = DiscoveryInputs { home: Some(dir.path().join("empty-home")), ..Default::default() };
    let (threads, _map) = production_threads(&ctx_for(dir.path()), inputs);
    let err = threads.delivery_capability().await.expect_err("nothing installed").to_string();
    assert!(err.contains("state directory not found") && err.contains("threads_state_dir"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn composed_daemon_status_shows_threads_discovery_error() {
    let (dir, root) = new_instance();
    let inputs = DiscoveryInputs { home: Some(dir.path().join("empty-home")), ..Default::default() };
    let (f, base) = fakes();
    let (threads, map) = production_threads(&ctx_for(&root), inputs);
    let mut services = Services::new(f.herdr.clone(), threads, map, f.clock.clone());
    services.reminder_period = base.reminder_period;
    let d = Daemon::start_with(&root, f, services).await;
    let mut shown = Value::Null;
    for _ in 0..800 {
        let status = d.call("status", json!({})).await.unwrap();
        shown = status["components"]["threads"].clone();
        if shown["error"].is_string() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(shown["connected"], false, "{shown}");
    let err = shown["error"].as_str().unwrap_or_default();
    assert!(err.contains("state directory not found") && err.contains("threads_state_dir"), "{shown}");
    d.stop().await;
}
