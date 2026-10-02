//! Configuration smoke (spec §4.1, §4.5, §11 tiers 3-4): each harness configuration (shell, claude, codex)
//! is activated in a PRIVATE Herdr server by driving the real components in-process: plan engine, writer,
//! git store, journal and the reconciler with the real `HerdrClient` on the private socket. The composed
//! daemon (Task 17) is deliberately not involved, so this runs before that wiring lands.
//!
//! Tiers:
//!   cargo test --features private-herdr --test config_smoke -- --nocapture --test-threads=1
//!   HG_REAL_AGENTS=1 cargo test --features private-herdr --test config_smoke -- --nocapture --test-threads=1
//!
//! The shell configuration runs in the default private-herdr tier. claude and codex need `HG_REAL_AGENTS=1`,
//! the agent binary on PATH and credentials (explicit API key); otherwise they print `SKIPPED (<reason>)`.
//! Every test appends `{config, status, reason?}` to `<target>/hg-config-smoke.json`, which
//! `zz_config_matrix_summary` prints as a table (the verification matrix of Task 19 reads that file).
//!
//! Optional knobs: `HG_SMOKE_CLAUDE_MODEL` (default `sonnet`), `HG_SMOKE_CODEX_MODEL` (default `gpt-5`):
//! the model the replacement switches the seat to.
#![cfg(feature = "private-herdr")]

mod support;

use herdr_graph::daemon::registry::CallerInfo;
use herdr_graph::herdr::HerdrClient;
use herdr_graph::journal::Journal;
use herdr_graph::model::clone::CloneRecord;
use herdr_graph::model::common::Occupant;
use herdr_graph::model::effect::{EffectKind, EffectStatus};
use herdr_graph::model::harness::{Harness, profile};
use herdr_graph::model::native_session::NativeSession;
use herdr_graph::model::seat::SeatRecord;
use herdr_graph::model::{CloneId, HerdrPaneId, NsId, SeatId};
use herdr_graph::plan::commands::{PlanDeps, admit_apply, create_plan};
use herdr_graph::plan::core_kinds::register_core_kinds;
use herdr_graph::plan::kind::KindRegistry;
use herdr_graph::plan::store::PlanStore;
use herdr_graph::ports::clock::SystemClock;
use herdr_graph::ports::store::Store;
use herdr_graph::ports::herdr::{AgentSession, AgentStatus, HerdrApi, HerdrSnapshot, KeyInput, PaneInfo};
use herdr_graph::reconcile::{Reconciler, ReconcilerConfig, RequesterNotifier, register_mutations};
use herdr_graph::store::GitStore;
use herdr_graph::store::init::init_instance;
use herdr_graph::store::layout;
use herdr_graph::store::tree::CommitView;
use herdr_graph::writer::{Applied, Mutation, MutationCx, MutationError, MutationRegistry, WriterConfig, WriterCore};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::private_herdr::PrivateHerdr;

// ---------------------------------------------------------------------------------------------------------
// Result file

static RESULTS: Mutex<()> = Mutex::new(());

fn results_path() -> PathBuf {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    tmp.parent().map(Path::to_path_buf).unwrap_or(tmp).join("hg-config-smoke.json")
}

/// Print the `CONFIG` line and merge `{config, status, reason?}` into the result file (latest run wins).
fn record(config: &str, status: &str, reason: Option<&str>) {
    match reason {
        Some(r) => println!("CONFIG {config}: {status} ({r})"),
        None => println!("CONFIG {config}: {status}"),
    }
    let _guard = RESULTS.lock().unwrap_or_else(|e| e.into_inner());
    let path = results_path();
    let mut rows: Vec<serde_json::Value> =
        std::fs::read(&path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
    rows.retain(|r| r["config"] != config);
    let mut row = json!({ "config": config, "status": status });
    if let Some(r) = reason {
        row["reason"] = json!(r);
    }
    rows.push(row);
    rows.sort_by_key(|r| r["config"].as_str().unwrap_or_default().to_owned());
    std::fs::write(&path, serde_json::to_vec_pretty(&rows).expect("serialize results")).expect("write result file");
}

fn skipped(config: &str, reason: &str) {
    support::skip(&format!("{config}: {reason}"));
    record(config, "SKIPPED", Some(reason));
}

// ---------------------------------------------------------------------------------------------------------
// Test-only mutations: the `seat override` kind (hg-zmi.17) and occupant observation (hg-zmi.8) are not
// dependencies of this bead.

struct TestSetModel;
impl Mutation for TestSetModel {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let seat: SeatId = cx.request.args["seat"].as_str().unwrap().parse().unwrap();
        let loc = cx.tree.locate(&seat.to_any())?.unwrap();
        let mut rec: SeatRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        rec.overrides.model = cx.request.args["model"].as_str().map(str::to_owned);
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "set model".into(), action: None })
    }
}

struct TestSetOccupant;
impl Mutation for TestSetOccupant {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let clone: CloneId = cx.request.args["clone"].as_str().unwrap().parse().unwrap();
        let native = cx.request.args["native"].as_str().unwrap().to_owned();
        let harness: Harness = serde_json::from_value(cx.request.args["harness"].clone()).unwrap();
        let loc = cx.tree.locate(&clone.to_any())?.unwrap();
        let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.unwrap();
        let ns = NativeSession {
            id: NsId::new(),
            harness,
            native_session_id: native,
            transcript_path: None,
            transcript: None,
            cwd: "/".into(),
            started: cx.now,
            ended: None,
            end_reason: None,
        };
        rec.occupant = Some(Occupant { native_session: ns.id.clone(), harness, since: cx.now });
        rec.sessions.push(ns);
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "set occupant".into(), action: None })
    }
}

struct LogNotifier;
impl RequesterNotifier for LogNotifier {
    fn notify(&self, _op: &herdr_graph::model::OpId, _severity: herdr_graph::ports::threads::Severity, text: &str) {
        eprintln!("notify: {text}");
    }
}

// ---------------------------------------------------------------------------------------------------------
// Rig

struct Rig {
    herdr: PrivateHerdr,
    client: Arc<HerdrClient>,
    deps: PlanDeps,
    writer: Arc<WriterCore>,
    store: Arc<GitStore>,
    journal: Arc<Journal>,
    rec: Arc<Reconciler>,
    instance: PathBuf,
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

impl Rig {
    /// A private Herdr plus an in-process graph instance under its root. `None` (after a skip line) when
    /// `herdr` is not installed.
    async fn start() -> Option<Rig> {
        let herdr = match PrivateHerdr::start() {
            Ok(h) => h,
            Err(e) if support::herdr_binary().is_none() => {
                support::skip(&format!("{e}"));
                return None;
            }
            Err(e) => panic!("private herdr failed to start: {e:#}"),
        };
        let instance = herdr.root.join("graph");
        herdr.guard().check(&instance).expect("instance is inside the private root");
        init_instance(&instance).expect("init instance");

        let mut kinds = KindRegistry::default();
        register_core_kinds(&mut kinds);
        let kinds = Arc::new(kinds);
        let plans = Arc::new(PlanStore::new(instance.join(".graph-local/plans")));
        let mut reg = MutationRegistry::default();
        kinds.register_mutations(&mut reg, plans.clone());
        register_mutations(&mut reg);
        reg.register("bookkeeping.test_set_model", Arc::new(TestSetModel));
        reg.register("bookkeeping.test_set_occupant", Arc::new(TestSetOccupant));
        let store = Arc::new(GitStore::open(&instance).unwrap());
        let journal = Arc::new(Journal::open(&Journal::path_in(&instance)).unwrap());
        let clock = Arc::new(SystemClock);
        let writer = WriterCore::new(store.clone(), journal.clone(), Arc::new(reg), clock.clone(), WriterConfig::default());
        let deps = PlanDeps {
            kinds,
            plans,
            store: store.clone(),
            writer: writer.clone(),
            clock: clock.clone(),
            instance: instance.clone(),
        };
        let client = Arc::new(herdr.client());
        let mut cfg = ReconcilerConfig::new(instance.clone());
        cfg.idle_timeout = Duration::from_secs(60);
        cfg.exit_timeout = Duration::from_secs(20);
        cfg.exit_followup = Duration::from_secs(3);
        cfg.relaunch_grace = Duration::from_secs(0);
        let rec = Reconciler::new(store.clone(), journal.clone(), writer.clone(), client.clone(), clock, Arc::new(LogNotifier), cfg);
        Some(Rig { herdr, client, deps, writer, store, journal, rec, instance })
    }

    /// Plan and apply one change; it must commit.
    fn apply(&self, change: &str) {
        let v = create_plan(&self.deps, &CallerInfo::default(), words(change))
            .unwrap_or_else(|e| panic!("plan {change}: {}", e.message));
        let id: herdr_graph::model::PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
        let sp = self.deps.plans.get(&id).unwrap().unwrap();
        let op = admit_apply(&self.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay")
            .unwrap_or_else(|e| panic!("admit {change}: {}", e.message));
        self.writer.drain().unwrap();
        let row = self.writer.journal().get(&op).unwrap().unwrap();
        assert_eq!(row.state, herdr_graph::model::operation::OpState::Committed, "{change}: {:?}", row.rejection);
    }

    fn bookkeeping(&self, args: serde_json::Value) {
        use herdr_graph::model::change::{ChangeRequest, RequestKind};
        use herdr_graph::ports::writer::Writer;
        self.writer
            .admit(ChangeRequest {
                kind: RequestKind::Bookkeeping,
                args,
                relied_on: vec![],
                requester: Default::default(),
                supersedes: None,
                confirmed: None,
            })
            .unwrap();
        self.writer.drain().unwrap();
    }

    /// Reconcile and commit write-backs until no effect is waiting (at most 30 s). Terminal non-`done`
    /// effects (failed, needs revision, blocked) end the wait too: the caller asserts on them.
    async fn converge(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let report = self.rec.step_fresh().await;
            self.writer.drain().unwrap();
            let pending = self.journal.effects_with_status(&[EffectStatus::Pending, EffectStatus::Unknown]).unwrap();
            if pending.is_empty() && report.planned.is_empty() && report.executed.is_empty() {
                return;
            }
            assert!(Instant::now() < deadline, "did not converge within 30 s; pending: {pending:?}");
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    fn problem_effects(&self) -> Vec<String> {
        self.journal
            .effects_with_status(&[EffectStatus::Failed, EffectStatus::NeedsRevision, EffectStatus::BlockedNeedsHuman])
            .unwrap()
            .into_iter()
            .map(|r| format!("{} {} {:?}: {:?}", r.kind.as_str(), r.object, r.status, r.last_error))
            .collect()
    }

    fn view(&self) -> CommitView<'_> {
        CommitView { store: &*self.store, at: self.store.head().unwrap() }
    }

    fn seat(&self, name: &str) -> SeatRecord {
        let mut found: Vec<_> = layout::all_seats(&self.view()).unwrap().into_iter().filter(|(_, s)| s.name == name).collect();
        assert_eq!(found.len(), 1, "seat {name}");
        found.remove(0).1
    }

    fn only_clone(&self, seat: &SeatRecord) -> CloneRecord {
        let mut cs: Vec<_> =
            layout::all_clones(&self.view()).unwrap().into_iter().map(|(_, c)| c).filter(|c| c.seat == seat.id).collect();
        assert_eq!(cs.len(), 1, "one clone of {}", seat.name);
        cs.remove(0)
    }

    async fn snapshot(&self) -> HerdrSnapshot {
        self.client.snapshot().await.expect("snapshot")
    }

    /// The live pane carrying the clone's graph token.
    async fn pane_of(&self, clone: &CloneId) -> Option<(HerdrPaneId, PaneInfo)> {
        let want = format!("hg={clone}");
        self.snapshot()
            .await
            .workspaces
            .into_iter()
            .flat_map(|w| w.tabs)
            .flat_map(|t| t.panes)
            .find(|p| p.metadata.get("hg").is_some_and(|v| *v == want))
            .map(|p| (p.id.clone(), p))
    }

    /// Wait until the clone's pane reports an agent of `kind` in `status`.
    async fn wait_agent(&self, clone: &CloneId, kind: &str, status: AgentStatus, within: Duration) -> PaneInfo {
        let deadline = Instant::now() + within;
        loop {
            if let Some((_, p)) = self.pane_of(clone).await
                && p.agent.as_ref().is_some_and(|a| a.kind == kind && a.status == status)
            {
                return p;
            }
            assert!(Instant::now() < deadline, "no {kind} agent in {status:?} on {clone} within {within:?}");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// The env HERDR_GRAPH* of the pane's foreground process, read from `ps eww` (an agent owns the pane,
    /// so there is no shell to echo from).
    async fn foreground_env(&self, pane: &HerdrPaneId) -> String {
        let info = self.client.process_info(pane).await.expect("process_info");
        let pid = info.foreground_pid.expect("foreground pid");
        let out = std::process::Command::new("/bin/ps")
            .args(["eww", "-o", "command=", "-p", &pid.to_string()])
            .output()
            .expect("ps");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn expect_env(&self, seat: &SeatRecord, clone: &CloneRecord, env_dump: &str) {
        for (k, v) in [
            ("HERDR_GRAPH", "1".to_owned()),
            ("HERDR_GRAPH_SEAT", seat.id.to_string()),
            ("HERDR_GRAPH_CLONE", clone.id.to_string()),
            ("HERDR_GRAPH_INSTANCE", self.instance.to_string_lossy().into_owned()),
        ] {
            assert!(env_dump.contains(&format!("{k}={v}")), "{k}={v} missing from pane env: {env_dump}");
        }
    }
}

/// Executable named `name` on PATH.
fn on_path(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path)
            .any(|d| d.join(name).metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
    })
}

// ---------------------------------------------------------------------------------------------------------
// Shell

#[tokio::test]
async fn config_shell_seat_tab_pane_env() {
    const CONFIG: &str = "shell";
    let Some(rig) = Rig::start().await else {
        return skipped(CONFIG, "herdr is not installed");
    };
    rig.apply("teamspace create t");
    rig.apply("seat create s --teamspace t --active --harness shell");
    rig.converge().await;
    assert!(rig.problem_effects().is_empty(), "{:?}", rig.problem_effects());

    let seat = rig.seat("s");
    let clone = rig.only_clone(&seat);
    let snap = rig.snapshot().await;

    // Plain names: the creation nonce was removed by the rename.
    let ws = snap.workspaces.iter().find(|w| w.label == "t").unwrap_or_else(|| panic!("workspace t in {:?}", snap.workspaces));
    let tab = ws.tabs.iter().find(|t| t.label == "s").unwrap_or_else(|| panic!("tab s in {:?}", ws.tabs));
    assert!(
        snap.workspaces.iter().flat_map(|w| w.tabs.iter().map(|t| &t.label)).all(|l| !l.contains(" · ")),
        "a nonce label is left: {snap:?}"
    );
    let pane = tab.panes.iter().find(|p| p.metadata.get("hg").is_some_and(|v| *v == format!("hg={}", clone.id)));
    let pane = pane.unwrap_or_else(|| panic!("pane with hg={} in {tab:?}", clone.id));

    // No agent.start: no start effect in the journal and no agent on the pane.
    assert!(pane.agent.is_none(), "shell seat has an agent: {:?}", pane.agent);
    let all = [EffectStatus::Pending, EffectStatus::Done, EffectStatus::Obsolete, EffectStatus::Failed, EffectStatus::Unknown];
    assert!(
        rig.journal.effects_with_status(&all).unwrap().iter().all(|r| r.kind != EffectKind::StartAgent),
        "a StartAgent effect was journaled for a shell seat"
    );

    // Env, read from inside the pane's shell.
    let pane_id = pane.id.clone();
    let out = rig.herdr.root.join(format!("env-{}.txt", clone.id));
    rig.herdr.guard().check(&out).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    // The pane shell may still be starting up: wait for it, then type once.
    loop {
        let p = rig.client.process_info(&pane_id).await.expect("process_info");
        if p.is_shell {
            break;
        }
        assert!(Instant::now() < deadline, "pane never showed a shell: {p:?}");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let cmd = format!(
        "echo \"$HERDR_GRAPH|$HERDR_GRAPH_SEAT|$HERDR_GRAPH_CLONE|$HERDR_GRAPH_INSTANCE\" > {}",
        out.display()
    );
    rig.client.send_keys(&pane_id, &[KeyInput::Text(cmd), KeyInput::Key("enter".into())]).await.expect("send_keys");
    let content = loop {
        if let Ok(c) = std::fs::read_to_string(&out)
            && c.ends_with('\n')
        {
            break c;
        }
        assert!(Instant::now() < deadline, "env file {} never appeared", out.display());
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let got: Vec<&str> = content.trim_end().split('|').collect();
    let want = ["1".to_owned(), seat.id.to_string(), clone.id.to_string(), rig.instance.to_string_lossy().into_owned()];
    assert_eq!(got, want.iter().map(String::as_str).collect::<Vec<_>>(), "pane env");

    record(CONFIG, "VERIFIED", None);
}

// ---------------------------------------------------------------------------------------------------------
// Real agents (tier 4)

/// Why this agent configuration cannot run here; None when it can.
fn real_agent_gap(binary: &str, key_var: &str) -> Option<String> {
    if std::env::var("HG_REAL_AGENTS").as_deref() != Ok("1") {
        return Some("HG_REAL_AGENTS=1 not set".into());
    }
    if !on_path(binary) {
        return Some(format!("`{binary}` not on PATH"));
    }
    // Credentials reach the private server only from an explicit API key (see isolation::scrubbed_env);
    // the macOS keychain login is not probed.
    if std::env::var(key_var).map(|v| v.is_empty()).unwrap_or(true) {
        return Some(format!("{key_var} not set"));
    }
    None
}

/// The native session id an agent reports in the snapshot, if any.
fn reported_session(pane: &PaneInfo) -> Option<String> {
    match pane.agent.as_ref()?.session.as_ref()? {
        AgentSession::Id(id) => Some(id.clone()),
        AgentSession::Path(p) => p.file_stem().map(|s| s.to_string_lossy().into_owned()),
    }
}

/// Launch a seat of `harness`, capture the session, replace it (model change) and verify the resume.
async fn agent_launch_and_resume(config: &str, harness: Harness, model_var: &str, default_model: &str) {
    let prof = profile(harness);
    let kind = prof.agent_kind.expect("agent harness");
    let Some(rig) = Rig::start().await else {
        return skipped(config, "herdr is not installed");
    };
    let hname = kind;
    rig.apply("teamspace create t");
    rig.apply(&format!("seat create s --teamspace t --active --harness {hname}"));
    rig.converge().await;
    assert!(rig.problem_effects().is_empty(), "{:?}", rig.problem_effects());
    let seat = rig.seat("s");
    let clone = rig.only_clone(&seat);

    // agent.start detected idle.
    let pane = rig.wait_agent(&clone.id, kind, AgentStatus::Idle, Duration::from_secs(120)).await;
    let first_pid = rig.client.process_info(&pane.id).await.unwrap().foreground_pid;
    let env_dump = rig.foreground_env(&pane.id).await;
    rig.expect_env(&seat, &clone, &env_dump);

    let Some(native) = reported_session(&pane) else {
        // Spike 5: Herdr may not report agent_session (claude gets its id from the graph's SessionStart hook,
        // which this smoke does not install). Without an id there is no recorded occupant to resume.
        record(config, "PARTIAL", Some("launch+env verified; agent_session not reported, resume not exercised"));
        return;
    };
    println!("{config}: agent_session = {native}");

    // The occupant the observer (hg-zmi.8) would record, then a desired change that forces a replacement.
    rig.bookkeeping(json!({"sub": "test_set_occupant", "clone": clone.id, "native": native, "harness": harness}));
    let model = std::env::var(model_var).unwrap_or_else(|_| default_model.to_owned());
    rig.bookkeeping(json!({"sub": "test_set_model", "seat": seat.id, "model": model}));
    rig.converge().await;
    assert!(rig.problem_effects().is_empty(), "{:?}", rig.problem_effects());

    let all = [EffectStatus::Pending, EffectStatus::Done, EffectStatus::Failed, EffectStatus::Unknown, EffectStatus::NeedsRevision];
    let replaced = rig.journal.effects_with_status(&all).unwrap();
    assert!(
        replaced.iter().any(|r| r.kind == EffectKind::ReplaceSession && r.status == EffectStatus::Done),
        "no completed ReplaceSession: {replaced:?}"
    );

    // The new occupant is a fresh process started with the resume arguments, idle again, same env.
    let pane = rig.wait_agent(&clone.id, kind, AgentStatus::Idle, Duration::from_secs(120)).await;
    let info = rig.client.process_info(&pane.id).await.unwrap();
    assert_ne!(info.foreground_pid, first_pid, "the occupant was not replaced");
    let prefix = prof.resume_prefix.expect("resume supported");
    let mut resume: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
    resume.push(native.clone());
    assert!(
        info.foreground_argv.windows(resume.len()).any(|w| w == resume.as_slice()),
        "argv {:?} does not contain {resume:?}",
        info.foreground_argv
    );
    for a in prof.launch_args {
        assert!(info.foreground_argv.iter().any(|x| x == a), "launch arg {a} missing from {:?}", info.foreground_argv);
    }
    let env_dump = rig.foreground_env(&pane.id).await;
    rig.expect_env(&seat, &clone, &env_dump);

    record(config, "VERIFIED", None);
}

#[tokio::test]
async fn config_claude_launch_and_resume() {
    if let Some(gap) = real_agent_gap("claude", "ANTHROPIC_API_KEY") {
        return skipped("claude", &gap);
    }
    agent_launch_and_resume("claude", Harness::Claude, "HG_SMOKE_CLAUDE_MODEL", "sonnet").await;
}

#[tokio::test]
async fn config_codex_launch_and_resume() {
    if let Some(gap) = real_agent_gap("codex", "OPENAI_API_KEY") {
        return skipped("codex", &gap);
    }
    agent_launch_and_resume("codex", Harness::Codex, "HG_SMOKE_CODEX_MODEL", "gpt-5").await;
}

// ---------------------------------------------------------------------------------------------------------
// Summary

/// Not a behavior test: prints the matrix written by the three tests. Named `zz_` so that a serial run
/// (`--test-threads=1`, alphabetical order) reaches it after them.
#[test]
fn zz_config_matrix_summary() {
    let path = results_path();
    println!("config smoke results: {}", path.display());
    let Some(rows) = std::fs::read(&path).ok().and_then(|raw| serde_json::from_slice::<Vec<serde_json::Value>>(&raw).ok()) else {
        println!("(no results recorded yet)");
        return;
    };
    println!("{:<8} {:<10} reason", "config", "status");
    for r in rows {
        println!("{:<8} {:<10} {}", r["config"].as_str().unwrap_or("?"), r["status"].as_str().unwrap_or("?"), r["reason"].as_str().unwrap_or(""));
    }
}
