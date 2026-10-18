//! Daemon composition root (spec §1): the one place that builds every component, registers every mutation kind,
//! command handler and background loop, and wires the hooks between them. Owned by hg-zmi.18.
//!
//! Startup convergence order (normative): open store + journal → build registries → journal recovery
//! (`WriterCore::recover`) → drain re-queued ops → first observer/reconciler pass (a rebind pass against a fresh
//! complete snapshot: in-flight effects are re-correlated by token/nonce, then the committed intent is reconciled)
//! → only then register the background loops and command handlers. Each step is recorded in the journal meta as
//! `startup:<n>` so a restart test can assert the order.
use crate::daemon::{DaemonCtx, registry::Registry};
use crate::journal::Journal;
use crate::observe::RuntimeLoop;
use crate::plan::commands::PlanDeps;
use crate::plan::kind::KindRegistry;
use crate::plan::ops::OpsDeps;
use crate::plan::reminders::ReminderScheduler;
use crate::plan::store::PlanStore;
use crate::ports::clock::{Clock, SystemClock};
use crate::ports::herdr::HerdrApi;
use crate::ports::store::Store;
use crate::ports::threads::*;
use crate::ports::writer::{Writer, WriterError};
use crate::reconcile::{Reconciler, ReconcilerConfig, ThreadsNotifier};
use crate::store::GitStore;
use crate::threads::PaneSeatMap;
use crate::threads::discovery::DiscoveryInputs;
use crate::transcripts::Transcripts;
use crate::writer::{MutationRegistry, WRITER_HALTED, WriterConfig, WriterCore};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The reminder scheduler's tick in the daemon (spec §3.7).
pub const REMINDER_PERIOD: Duration = Duration::from_secs(60);
/// How often the threads connection loop re-probes the service.
const THREADS_PROBE_PERIOD: Duration = Duration::from_secs(30);
/// A silent Herdr must not keep the daemon from serving: the first pass gives up after this long.
pub const FIRST_PASS_LIMIT: Duration = Duration::from_secs(20);

/// Startup steps, recorded in journal meta `startup:<n>` in this order.
pub const STARTUP_STEPS: [&str; 6] = [
    "open",
    "registries",
    "recover",
    "drain",
    "first_pass",
    "loops",
];

/// Services the composition needs; production builds them from the environment, tests inject fakes.
pub struct Services {
    pub herdr: Arc<dyn HerdrApi>,
    pub threads: Arc<dyn ThreadsPort>,
    pub pane_seat_map: Arc<dyn crate::threads::mapping::PaneSeatMap>,
    pub clock: Arc<dyn Clock>,
    /// Period of the reminder loop (`REMINDER_PERIOD` in the daemon; tests shrink it).
    pub reminder_period: Duration,
}

impl Services {
    pub fn new(
        herdr: Arc<dyn HerdrApi>,
        threads: Arc<dyn ThreadsPort>,
        pane_seat_map: Arc<dyn PaneSeatMap>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            herdr,
            threads,
            pane_seat_map,
            clock,
            reminder_period: REMINDER_PERIOD,
        }
    }

    /// Production services: the Herdr socket from the context, the threads service located by the default
    /// discovery chain (`crate::threads::discovery`: `HERDR_GRAPH_THREADS_STATE_DIR`, `threads_state_dir` in
    /// config.toml, the herdr-threads sibling of the plugin state dir, then the XDG / HOME defaults), or the
    /// explicit `HERDR_GRAPH_THREADS_SOCKET` + `HERDR_GRAPH_THREADS_INSTANCE` pair.
    /// A missing or stopped threads service never blocks startup: its calls fail and the effects retry.
    pub fn from_env(ctx: &DaemonCtx) -> anyhow::Result<Self> {
        #[cfg(feature = "test-support")]
        if std::env::var("HG_TEST_FAKE_SERVICES").as_deref() == Ok("1") {
            return Ok(fakes::services(ctx));
        }
        let herdr = Arc::new(crate::herdr::HerdrClient::new(ctx.herdr_socket.clone()));
        let env = crate::config::Env::from_process();
        let configured = crate::config::read_threads_state_dir(
            &env,
            &crate::config::plugin_config_dir_via_herdr,
        );
        let (threads, pane_seat_map) =
            production_threads(ctx, DiscoveryInputs::from_process(configured));
        Ok(Self::new(
            herdr,
            threads,
            pane_seat_map,
            Arc::new(SystemClock),
        ))
    }
}

/// The production threads port and pane-seat map over one lazily located endpoint. `inputs` drive the state
/// directory discovery, re-run on every call (the threads daemon may be installed or started after graph).
pub fn production_threads(
    ctx: &DaemonCtx,
    inputs: DiscoveryInputs,
) -> (Arc<dyn ThreadsPort>, Arc<dyn PaneSeatMap>) {
    let endpoint = Arc::new(lazy::Endpoint::from_env(ctx, inputs));
    // Bind a genuinely unused graph before its first writer commit. Discovery failure remains
    // a visible per-call error, not a startup failure or permission to adopt legacy records.
    let _ = endpoint.locate();
    (
        Arc::new(lazy::LazyThreads::new(endpoint.clone())),
        Arc::new(lazy::LazyMap::new(endpoint)),
    )
}

/// Every organizational and internal mutation, registered once. Returns the kind registry (shared with the
/// plan, undo and bootstrap commands) next to the writer's mutation registry.
pub fn mutation_registry(plans: Arc<PlanStore>) -> (Arc<KindRegistry>, MutationRegistry) {
    let mut kinds = KindRegistry::default();
    crate::plan::core_kinds::register_core_kinds(&mut kinds);
    crate::templates::register_kinds(&mut kinds);
    crate::undo::register_kinds(&mut kinds);
    crate::plan::kinds_extra::register_kinds(&mut kinds);
    let kinds = Arc::new(kinds);

    let mut muts = MutationRegistry::default();
    kinds.register_mutations(&mut muts, plans);
    crate::reconcile::bookkeeping::register_mutations(&mut muts);
    crate::observe::register_mutations(&mut muts);
    crate::threads::register_mutations(&mut muts);
    crate::transcripts::register_mutations(&mut muts);
    crate::bootstrap::register_mutations(&mut muts);
    crate::plan::ops::register_mutations(&mut muts);
    (kinds, muts)
}

/// Registers every component. Production entry point.
pub async fn compose(reg: &mut Registry, ctx: &DaemonCtx) -> anyhow::Result<()> {
    #[cfg(feature = "test-support")]
    if let Some(ms) = std::env::var("HG_TEST_COMPOSE_DELAY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
    compose_with(reg, ctx, Services::from_env(ctx)?).await
}

/// Journal meta recorder for the startup order.
struct Startup<'a> {
    journal: &'a Journal,
    next: usize,
}

impl<'a> Startup<'a> {
    fn begin(journal: &'a Journal) -> anyhow::Result<Self> {
        // The previous boot's record must not mask this one.
        for n in 0..STARTUP_STEPS.len() + 4 {
            journal.meta_delete(&format!("startup:{n}"))?;
        }
        Ok(Self { journal, next: 0 })
    }
    fn step(&mut self, name: &str) -> anyhow::Result<()> {
        self.journal
            .meta_set(&format!("startup:{}", self.next), name)?;
        self.next += 1;
        Ok(())
    }
}

/// See the module docs for the normative startup order.
pub async fn compose_with(
    reg: &mut Registry,
    ctx: &DaemonCtx,
    services: Services,
) -> anyhow::Result<()> {
    let paths = &ctx.paths;
    let Services {
        herdr,
        threads,
        pane_seat_map,
        clock,
        reminder_period,
    } = services;
    std::fs::create_dir_all(&paths.local)?;

    // 1. Store and journal.
    let git = Arc::new(GitStore::open(&paths.root)?);
    let journal = Arc::new(Journal::open(&paths.journal)?);
    let mut startup = Startup::begin(&journal)?;
    startup.step("open")?;
    let store: Arc<dyn Store> = git.clone();

    // 2. Registries.
    let plans = Arc::new(PlanStore::new(paths.plans.clone()));
    let (kinds, muts) = mutation_registry(plans.clone());
    let writer = WriterCore::new(
        git.clone(),
        journal.clone(),
        Arc::new(muts),
        clock.clone(),
        WriterConfig::default(),
    );
    let writer_port: Arc<dyn Writer> = writer.clone();
    let reconciler = Reconciler::new(
        store.clone(),
        journal.clone(),
        writer_port.clone(),
        herdr.clone(),
        clock.clone(),
        Arc::new(ThreadsNotifier {
            threads: threads.clone(),
            journal: journal.clone(),
            store: store.clone(),
        }),
        ReconcilerConfig::new(paths.root.clone()),
    );
    crate::threads::register_with(&reconciler, threads.clone(), pane_seat_map.clone());
    let runtime = RuntimeLoop::new(
        herdr.clone(),
        store.clone(),
        writer_port.clone(),
        journal.clone(),
        reconciler.clone(),
        clock.clone(),
        paths.clone(),
        Duration::from_secs(60),
        Some(writer.subscribe()),
        ctx.claude_root.clone(),
    );
    let transcripts = Transcripts::new(
        store.clone(),
        writer_port.clone(),
        reconciler.clone(),
        threads.clone(),
        pane_seat_map.clone(),
        clock.clone(),
        Some(runtime.subscribe_session_ended()),
    );
    transcripts.register_with(&reconciler);
    startup.step("registries")?;

    // 3. Journal recovery and worktree fast-forward, before anything can run.
    let w = writer.clone();
    let report = tokio::task::spawn_blocking(move || w.recover()).await??;
    if !report.requeued.is_empty() || !report.marked_committed.is_empty() {
        eprintln!(
            "herdr-graph daemon: recovery marked {} op(s) committed and requeued {}",
            report.marked_committed.len(),
            report.requeued.len()
        );
    }
    startup.step("recover")?;

    // 4. Drain what recovery re-queued (and anything admitted while the daemon was down).
    let w = writer.clone();
    match tokio::task::spawn_blocking(move || w.drain()).await? {
        Ok(_) => {}
        Err(WriterError::Halted(why)) => eprintln!("herdr-graph daemon: writer is halted: {why}"),
        Err(e) => eprintln!("herdr-graph daemon: drain failed: {e}"),
    }
    startup.step("drain")?;

    // 5. First rebind pass: a fresh complete snapshot re-correlates in-flight effects, then the reconciler
    // drives Herdr to the latest committed intent. Observed ops need a running writer to commit, so a
    // temporary one serves this pass; the real writer loop is registered below.
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let temp_writer = tokio::spawn(writer.clone().run(stop_rx));
    match tokio::time::timeout(FIRST_PASS_LIMIT, runtime.step_once()).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => eprintln!("herdr-graph daemon: first observer pass failed: {e:#}"),
        Err(_) => eprintln!(
            "herdr-graph daemon: first observer pass did not finish in {FIRST_PASS_LIMIT:?}"
        ),
    }
    let _ = stop_tx.send(true);
    let _ = temp_writer.await;
    startup.step("first_pass")?;

    // 6. Loops, commands and status providers.
    register_commands(
        reg,
        &kinds,
        &plans,
        &store,
        &writer_port,
        &clock,
        &journal,
        &herdr,
        &threads,
        paths.root.clone(),
    );
    transcripts.register_commands(reg);
    register_writer_commands(reg, &writer);

    let w = writer.clone();
    reg.background("writer", move |sd| async move {
        w.run(sd.receiver()).await;
        Ok(())
    });
    let rt = runtime.clone();
    reg.background("observer", move |sd| async move {
        rt.run(sd.receiver()).await;
        Ok(())
    });
    transcripts.register_loops(reg);
    let sched = Arc::new(ReminderScheduler {
        journal: journal.clone(),
        store: store.clone(),
        threads: threads.clone(),
        clock: clock.clone(),
    });
    crate::plan::reminders::register_loop(reg, sched, reminder_period);
    register_threads_connection(reg, threads.clone());
    register_status(reg, &journal);
    startup.step("loops")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn register_commands(
    reg: &mut Registry,
    kinds: &Arc<KindRegistry>,
    plans: &Arc<PlanStore>,
    store: &Arc<dyn Store>,
    writer: &Arc<dyn Writer>,
    clock: &Arc<dyn Clock>,
    journal: &Arc<Journal>,
    herdr: &Arc<dyn HerdrApi>,
    _threads: &Arc<dyn ThreadsPort>,
    instance: PathBuf,
) {
    let plan_deps = |instance: &PathBuf| PlanDeps {
        kinds: kinds.clone(),
        plans: plans.clone(),
        store: store.clone(),
        writer: writer.clone(),
        clock: clock.clone(),
        instance: instance.clone(),
    };
    crate::plan::commands::register_commands(reg, plan_deps(&instance));
    crate::plan::ops::register_commands(
        reg,
        OpsDeps {
            journal: journal.clone(),
            store: store.clone(),
            clock: clock.clone(),
        },
    );
    crate::undo::register_commands(
        reg,
        crate::undo::UndoDeps {
            store: store.clone(),
            kinds: kinds.clone(),
            plans: plans.clone(),
            writer: writer.clone(),
            clock: clock.clone(),
            instance: instance.clone(),
            herdr: herdr.clone(),
        },
    );
    crate::threads::register_commands(reg, store.clone());
    crate::bootstrap::register_commands(
        reg,
        crate::bootstrap::BootstrapDeps {
            plan: Arc::new(plan_deps(&instance)),
            journal: journal.clone(),
            herdr: Some(herdr.clone()),
        },
    );
}

/// Registers the threads connection: a loop that keeps the service registration warm and records what the
/// connection supports, shown by `status` as `components.threads`.
fn register_threads_connection(reg: &mut Registry, threads: Arc<dyn ThreadsPort>) {
    let state = Arc::new(Mutex::new(
        json!({ "connected": false, "note": "not probed yet" }),
    ));
    let shown = state.clone();
    reg.status_provider(
        "threads",
        Arc::new(move || shown.lock().unwrap_or_else(|e| e.into_inner()).clone()),
    );
    reg.background("threads.connection", move |mut sd| async move {
        loop {
            let v = match threads.delivery_capability().await {
                Ok(c) => json!({ "connected": true, "capability": c }),
                Err(e) => json!({ "connected": false, "error": e.to_string() }),
            };
            *state.lock().unwrap_or_else(|e| e.into_inner()) = v;
            tokio::select! {
                _ = sd.wait() => return Ok(()),
                _ = tokio::time::sleep(THREADS_PROBE_PERIOD) => {}
            }
        }
    });
}

/// `writer.resume`: clear a halted writer after a successful probe (recovery plus a journal round-trip).
fn register_writer_commands(reg: &mut Registry, writer: &Arc<WriterCore>) {
    let w = writer.clone();
    reg.command(
        "writer.resume",
        move |_cx: crate::daemon::registry::CommandCtx, _args: serde_json::Value| {
            let w = w.clone();
            async move {
                use crate::daemon::registry::CommandError;
                let report = tokio::task::spawn_blocking(move || w.resume())
                    .await
                    .map_err(|e| CommandError::internal(format!("writer.resume task failed: {e}")))?
                    .map_err(|e| {
                        CommandError::unavailable(format!("probe failed, writer stays halted: {e}"))
                    })?;
                Ok(json!({
                    "was_halted": report.was_halted,
                    "reason": report.reason,
                    "requeued": report.recovery.map(|r| r.requeued).unwrap_or_default(),
                }))
            }
        },
    );
}

/// Status components owned by the composition: `writer` and `reconciler` (`transcripts` and `threads` come
/// from their own registrations).
fn register_status(reg: &mut Registry, journal: &Arc<Journal>) {
    let j = journal.clone();
    reg.status_provider(
        "writer",
        Arc::new(move || {
            json!({
                "writer_halted": j.meta_get(WRITER_HALTED).ok().flatten(),
                "ops": j.counts().unwrap_or_default(),
            })
        }),
    );
    let j = journal.clone();
    reg.status_provider("reconciler", Arc::new(move || reconciler_status(&j)));
}

/// The `reconciler` status component: open effects, and every effect that needs a human with its last error.
pub(crate) fn reconciler_status(j: &Journal) -> serde_json::Value {
    use crate::model::effect::EffectStatus::{
        BlockedNeedsHuman, Failed, NeedsRevision, Pending, Unknown,
    };
    let n = |s| j.effects_with_status(&[s]).map(|v| v.len()).unwrap_or(0);
    let mut attention = j
        .effects_with_status(&[NeedsRevision, BlockedNeedsHuman, Failed])
        .unwrap_or_default();
    attention.reverse();
    let attention: Vec<_> = attention
        .iter()
        .take(10)
        .map(|e| {
            json!({
                "effect": e.id.to_string(),
                "kind": e.kind.as_str(),
                "object": e.object.to_string(),
                "status": e.status,
                "last_error": e.last_error,
            })
        })
        .collect();
    let pending_notices = j
        .notice_counts()
        .ok()
        .and_then(|c| c.get("pending").copied())
        .unwrap_or(0);
    json!({
        "pending_effects": n(Pending),
        "unknown_effects": n(Unknown),
        "needs_revision_effects": n(NeedsRevision),
        "blocked_effects": n(BlockedNeedsHuman),
        "failed_effects": n(Failed),
        "pending_notices": pending_notices,
        "attention": attention,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Notice;
    use crate::model::effect::{EffectKind, EffectRecord, EffectStatus};
    use crate::model::{OpId, SeatId};
    use chrono::TimeZone;

    #[test]
    fn reconciler_status_counts_attention_effects() {
        let t = tempfile::tempdir().unwrap();
        let j = Journal::open(&t.path().join("j.sqlite3")).unwrap();
        let now = chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap();
        let op = OpId::new();
        let mut first = None;
        for (i, (status, err)) in [
            (EffectStatus::Failed, Some("boom")),
            (EffectStatus::BlockedNeedsHuman, None),
            (EffectStatus::NeedsRevision, Some("occupant busy")),
        ]
        .into_iter()
        .enumerate()
        {
            let object = SeatId::new().to_any();
            let e = EffectRecord {
                id: EffectRecord::identity(&op, &object, &EffectKind::CreateTab, 1),
                op: op.clone(),
                object,
                kind: EffectKind::CreateTab,
                object_rev: 1,
                fencing_rev: 1,
                status,
                predicted: vec![],
                nonce_label: None,
                attempts: 1,
                last_error: err.map(str::to_owned),
                updated_at: now + chrono::Duration::seconds(i as i64),
                sched: Default::default(),
            };
            let n = Notice {
                key: format!("k{i}"),
                effect: e.id.clone(),
                op: op.clone(),
                severity: crate::ports::threads::Severity::Warn,
                text: "t".into(),
                state: "pending".into(),
                attempts: 0,
                last_error: None,
                next_at: None,
            };
            j.upsert_effect_with_notice(&e, &n).unwrap();
            if i == 0 {
                first = Some(n.key);
            }
        }
        j.notice_done(&first.unwrap(), "delivered", now).unwrap();
        j.notice_done("k1", "delivered", now).unwrap();
        let v = reconciler_status(&j);
        assert_eq!(v["failed_effects"], 1);
        assert_eq!(v["blocked_effects"], 1);
        assert_eq!(v["needs_revision_effects"], 1);
        assert_eq!(v["pending_notices"], 1);
        let a = v["attention"].as_array().unwrap();
        let rows: Vec<_> = a
            .iter()
            .map(|r| (r["status"].as_str().unwrap(), r["last_error"].as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("needs_revision", Some("occupant busy")),
                ("blocked_needs_human", None),
                ("failed", Some("boom"))
            ],
            "newest first, with last_error"
        );
    }
}

/// Production ports that find the herdr-threads service lazily: it may start after the daemon, and a stopped
/// service must only fail calls (their effects retry), never startup.
mod lazy {
    use super::*;
    use crate::model::HerdrPaneId;
    use crate::model::clone::{InvitationState, InviteConstraint};
    use crate::threads::discovery::resolve_state_dir;
    use crate::threads::{Discovered, ServiceThreads, ThreadsSeatMap};

    pub struct Endpoint {
        inputs: DiscoveryInputs,
        explicit: Result<Option<Discovered>, String>,
        herdr_socket: PathBuf,
        intents: PathBuf,
        graph_root: PathBuf,
    }

    impl Endpoint {
        pub fn from_env(ctx: &DaemonCtx, inputs: DiscoveryInputs) -> Self {
            let explicit = parse_explicit(
                std::env::var_os("HERDR_GRAPH_THREADS_SOCKET"),
                std::env::var_os("HERDR_GRAPH_THREADS_INSTANCE"),
            );
            Self {
                inputs,
                explicit,
                herdr_socket: ctx.herdr_socket.clone(),
                intents: ctx.paths.threads_intents.clone(),
                graph_root: ctx.paths.root.clone(),
            }
        }

        pub(super) fn locate(&self) -> Result<Discovered, ThreadsError> {
            if let Some(d) = self
                .explicit
                .as_ref()
                .map_err(|e| ThreadsError::Rejected(e.clone()))?
            {
                self.bind(d.instance)?;
                return Ok(d.clone());
            }
            let found = resolve_state_dir(&self.inputs).map_err(ThreadsError::Disconnected)?;
            let Some((dir, source)) = found else {
                return Err(ThreadsError::Disconnected(
                    "herdr-threads state directory not found (looked at HERDR_GRAPH_THREADS_STATE_DIR, threads_state_dir in config.toml, \
                     the herdr-threads sibling of the plugin state dir, $XDG_STATE_HOME/herdr/plugins/herdr-threads and \
                     ~/.local/state/herdr/plugins/herdr-threads); install/start herdr-threads or set threads_state_dir in config.toml"
                        .into(),
                ));
            };
            let discovered = crate::threads::discover(&dir, &self.herdr_socket).map_err(|e| {
                ThreadsError::Disconnected(format!(
                    "herdr-threads is not running (state dir {} from {source}): {e}",
                    dir.display()
                ))
            })?;
            self.bind(discovered.instance)?;
            Ok(discovered)
        }

        async fn guard(&self) -> Result<std::fs::File, ThreadsError> {
            let guard = crate::threads::namespace::call_guard(&self.graph_root)
                .await
                .map_err(|e| ThreadsError::Rejected(e.to_string()))?;
            if self.graph_root.join(".git").exists() {
                crate::threads::namespace::check_call(&self.graph_root)
                    .map_err(|e| ThreadsError::Rejected(e.to_string()))?;
            }
            Ok(guard)
        }

        fn bind(&self, node: herdr_threads::node::NodeUuid) -> Result<(), ThreadsError> {
            crate::threads::namespace::ensure(&self.intents, node, Some(&self.graph_root))
                .map_err(|e| ThreadsError::Rejected(e.to_string()))
        }
    }

    fn parse_explicit(
        socket: Option<std::ffi::OsString>,
        node: Option<std::ffi::OsString>,
    ) -> Result<Option<Discovered>, String> {
        match (socket, node) {
            (None, None) => Ok(None),
            (Some(socket), Some(node)) => {
                let path = PathBuf::from(socket);
                if !path.is_absolute() {
                    return Err("HERDR_GRAPH_THREADS_SOCKET must be an absolute socket path".into());
                }
                let instance = node.to_str().ok_or("HERDR_GRAPH_THREADS_INSTANCE must be UTF-8")?
                    .parse().map_err(|e| format!("invalid HERDR_GRAPH_THREADS_INSTANCE: {e}"))?;
                Ok(Some(Discovered { socket: path, instance }))
            }
            _ => Err("HERDR_GRAPH_THREADS_SOCKET and HERDR_GRAPH_THREADS_INSTANCE must be configured together".into()),
        }
    }

    pub struct LazyThreads {
        endpoint: Arc<Endpoint>,
        inner: Arc<Mutex<Option<(Discovered, Arc<ServiceThreads>)>>>,
    }

    impl LazyThreads {
        pub fn new(endpoint: Arc<Endpoint>) -> Self {
            Self {
                endpoint,
                inner: Arc::new(Mutex::new(None)),
            }
        }

        async fn get_async(&self) -> Result<Arc<ServiceThreads>, ThreadsError> {
            let me = Self {
                endpoint: self.endpoint.clone(),
                inner: self.inner.clone(),
            };
            tokio::task::spawn_blocking(move || me.get())
                .await
                .map_err(|e| ThreadsError::Disconnected(e.to_string()))?
        }

        fn get(&self) -> Result<Arc<ServiceThreads>, ThreadsError> {
            let mut slot = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            let d = self.endpoint.locate()?;
            if let Some((cached, t)) = &*slot {
                if cached == &d {
                    return Ok(t.clone());
                }
            }
            *slot = None;
            let t = ServiceThreads::with_system_clock(
                d.socket.clone(),
                crate::threads::namespace::intent_dir(
                    &self.endpoint.intents,
                    &self.endpoint.graph_root,
                )
                .or_else(|e| {
                    if !self.endpoint.graph_root.join(".git").exists() {
                        Ok(self.endpoint.intents.clone())
                    } else {
                        Err(e)
                    }
                })
                .map_err(|e| ThreadsError::Rejected(e.to_string()))?,
                d.instance,
            )
            .map_err(|e| ThreadsError::Disconnected(format!("threads intent journal: {e}")))?;
            let t = Arc::new(t);
            *slot = Some((d, t.clone()));
            Ok(t)
        }
    }

    #[async_trait::async_trait]
    impl ThreadsPort for LazyThreads {
        async fn ensure_thread(
            &self,
            scope: ChannelScope,
            topic: &str,
            op_key: &OpKey,
        ) -> Result<ThreadRef, ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async()
                .await?
                .ensure_thread(scope, topic, op_key)
                .await
        }
        async fn invite(
            &self,
            thread: &ThreadRef,
            seat: &ThreadsSeatRef,
            constraint: InviteConstraint,
            op_key: &OpKey,
        ) -> Result<(), ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async()
                .await?
                .invite(thread, seat, constraint, op_key)
                .await
        }
        async fn membership(
            &self,
            thread: &ThreadRef,
            seat: &ThreadsSeatRef,
        ) -> Result<Option<InvitationState>, ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async().await?.membership(thread, seat).await
        }
        async fn membership_detail(
            &self,
            thread: &ThreadRef,
            seat: &ThreadsSeatRef,
        ) -> Result<Option<MembershipDetail>, ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async()
                .await?
                .membership_detail(thread, seat)
                .await
        }
        async fn notify(
            &self,
            thread: &ThreadRef,
            severity: Severity,
            body: &str,
            op_key: &OpKey,
        ) -> Result<(), ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async()
                .await?
                .notify(thread, severity, body, op_key)
                .await
        }
        async fn set_topic(
            &self,
            thread: &ThreadRef,
            topic: &str,
            op_key: &OpKey,
        ) -> Result<(), ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async()
                .await?
                .set_topic(thread, topic, op_key)
                .await
        }
        async fn release_requirement(
            &self,
            thread: &ThreadRef,
            seat: &ThreadsSeatRef,
            op_key: &OpKey,
        ) -> Result<(), ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async()
                .await?
                .release_requirement(thread, seat, op_key)
                .await
        }
        async fn send_request(
            &self,
            thread: &ThreadRef,
            recipients: &[ThreadsSeatRef],
            body: &str,
            op_key: &OpKey,
        ) -> Result<MessageRef, ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async()
                .await?
                .send_request(thread, recipients, body, op_key)
                .await
        }
        async fn receipt_state(
            &self,
            messages: &[MessageRef],
        ) -> Result<Vec<MessageReceipts>, ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async().await?.receipt_state(messages).await
        }
        async fn delivery_capability(&self) -> Result<DeliveryCapability, ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            self.get_async().await?.delivery_capability().await
        }
    }

    pub struct LazyMap {
        endpoint: Arc<Endpoint>,
        inner: Arc<Mutex<Option<(Discovered, Arc<ThreadsSeatMap>)>>>,
    }

    impl LazyMap {
        pub fn new(endpoint: Arc<Endpoint>) -> Self {
            Self {
                endpoint,
                inner: Arc::new(Mutex::new(None)),
            }
        }
    }

    #[async_trait::async_trait]
    impl PaneSeatMap for LazyMap {
        async fn seat_for(
            &self,
            pane: &HerdrPaneId,
        ) -> Result<Option<ThreadsSeatRef>, ThreadsError> {
            let _guard = self.endpoint.guard().await?;
            let me = Self {
                endpoint: self.endpoint.clone(),
                inner: self.inner.clone(),
            };
            let map = tokio::task::spawn_blocking(move || {
                let mut slot = me.inner.lock().unwrap_or_else(|e| e.into_inner());
                let d = me.endpoint.locate()?;
                if slot.as_ref().is_none_or(|(cached, _)| cached != &d) {
                    *slot = None;
                    let clock: Arc<dyn herdr_threads::protocol::time::Clock> =
                        Arc::new(herdr_threads::app::SystemClock::new());
                    // RuntimeContext canonicalizes the selected host parent consistently with threads.
                    let context = herdr_threads::daemon::paths::RuntimeContext::explicit(
                        me.endpoint.graph_root.clone(),
                        me.endpoint.herdr_socket.clone(),
                        None,
                    )
                    .map_err(|e| ThreadsError::Rejected(e.to_string()))?;
                    let host = context
                        .claim_host()
                        .map_err(|e| ThreadsError::Rejected(e.to_string()))?;
                    let m = Arc::new(ThreadsSeatMap::new(
                        d.socket.clone(),
                        d.instance,
                        clock,
                        host,
                    ));
                    *slot = Some((d, m));
                }
                Ok::<_, ThreadsError>(slot.as_ref().unwrap().1.clone())
            })
            .await
            .map_err(|e| ThreadsError::Disconnected(e.to_string()))??;
            map.seat_for(pane).await
        }
    }
    #[cfg(test)]
    mod namespace_tests {
        use super::*;
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        #[test]
        fn explicit_node_override_is_canonical_and_never_silently_falls_back() {
            assert!(parse_explicit(None, None).unwrap().is_none());
            for node in [
                "",
                "123e4567-e89b-12d3-a456-426614174001",
                "ABCDEF1234567890abcdef1234567890",
                "not-a-node",
            ] {
                assert!(parse_explicit(Some("/threads.sock".into()), Some(node.into())).is_err());
            }
            assert!(parse_explicit(Some("/threads.sock".into()), None).is_err());
            assert!(parse_explicit(None, Some("a".repeat(32).into())).is_err());
            assert!(
                parse_explicit(Some("relative.sock".into()), Some("a".repeat(32).into())).is_err()
            );
            let d = parse_explicit(Some("/threads.sock".into()), Some("a".repeat(32).into()))
                .unwrap()
                .unwrap();
            assert_eq!(d.instance.to_string(), "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        }

        #[tokio::test]
        async fn cached_client_rediscovers_and_refuses_a_fresh_node_at_same_socket() {
            use herdr_threads::daemon::paths::NodePaths;
            use herdr_threads::node::NodeUuid;
            let dir = tempfile::tempdir().unwrap();
            let state = dir.path().join("state");
            std::fs::create_dir_all(&state).unwrap();
            let state = state.canonicalize().unwrap();
            let paths = NodePaths::resolve(&state).unwrap();
            paths.prepare_node_dir().unwrap();
            let _socket = std::os::unix::net::UnixListener::bind(&paths.socket_path).unwrap();
            let meta = std::fs::metadata(&paths.socket_path).unwrap();
            let publish =
                |node: NodeUuid| {
                    std::fs::write(&paths.node_json_path, serde_json::to_vec(&json!({
                    "format": 1, "node_uuid": node, "tag": "abc", "tag_k": 0, "created_at": 1
                })).unwrap()).unwrap();
                    std::fs::write(
                        &paths.descriptor_path,
                        serde_json::to_vec(&herdr_threads::daemon::ownership::EndpointDescriptor {
                            software_version: "2.0.3".into(),
                            protocol_version: herdr_threads::protocol::wire::PROTOCOL_VERSION,
                            node_uuid: node,
                            boot_id: uuid::Uuid::new_v4(),
                            endpoint: paths.socket_path.clone(),
                            pid: std::process::id(),
                            socket_device: meta.dev(),
                            socket_inode: meta.ino(),
                        })
                        .unwrap(),
                    )
                    .unwrap();
                };
            publish(NodeUuid::from_bytes([1; 16]));
            for path in [
                &paths.node_json_path,
                &paths.descriptor_path,
                &paths.socket_path,
            ] {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            let endpoint = Arc::new(Endpoint {
                inputs: DiscoveryInputs {
                    env_state_dir: Some(state),
                    ..Default::default()
                },
                explicit: Ok(None),
                herdr_socket: dir.path().join("host.sock"),
                intents: dir.path().join("graph/.graph-local/threads-intents"),
                graph_root: dir.path().join("graph"),
            });
            crate::store::init::init_instance(&endpoint.graph_root).unwrap();
            let ctx = DaemonCtx {
                paths: crate::config::InstancePaths::new(&endpoint.graph_root),
                herdr_socket: endpoint.herdr_socket.clone(),
                started_at: chrono::Utc::now(),
                claude_root: dir.path().join("claude"),
            };
            let _assembled = production_threads(&ctx, endpoint.inputs.clone());
            assert!(
                crate::threads::namespace::binding_path(&endpoint.intents).exists(),
                "production assembly binds the fresh graph before any first mutation or external call"
            );
            let lazy = LazyThreads::new(endpoint.clone());
            let first = lazy.get().unwrap();
            assert!(Arc::ptr_eq(&first, &lazy.get().unwrap()));
            let binding =
                std::fs::read(crate::threads::namespace::binding_path(&endpoint.intents)).unwrap();
            let lazy_map = LazyMap::new(endpoint.clone());
            let found = endpoint.locate().unwrap();
            *lazy_map.inner.lock().unwrap() = Some((
                found.clone(),
                Arc::new(ThreadsSeatMap::new(
                    found.socket,
                    found.instance,
                    Arc::new(herdr_threads::app::SystemClock::new()),
                    herdr_threads::protocol::ids::HostLocator::parse("/host.sock").unwrap(),
                )),
            ));
            publish(NodeUuid::from_bytes([2; 16]));
            assert!(matches!(lazy.get(), Err(ThreadsError::Rejected(_))));
            assert!(matches!(
                lazy_map.seat_for(&HerdrPaneId("w1:p1".into())).await,
                Err(ThreadsError::Rejected(_))
            ));
            assert_eq!(
                std::fs::read(crate::threads::namespace::binding_path(&endpoint.intents)).unwrap(),
                binding
            );
        }
    }
}

/// `HG_TEST_FAKE_SERVICES=1` (feature `test-support`): FakeHerdr whose state survives a daemon restart,
/// FakeThreads, FakePaneSeatMap and the system clock.
#[cfg(feature = "test-support")]
mod fakes {
    use super::*;
    use crate::herdr::FakeHerdr;
    use crate::model::harness::StartOutcome;
    use crate::model::{HerdrPaneId, HerdrTabId, HerdrWorkspaceId};
    use crate::ports::herdr::*;
    use serde::{Deserialize, Serialize};

    /// One mutating Herdr call that succeeded, in order. The log is `.graph-local/fake-herdr.json`; replaying
    /// it against a fresh `FakeHerdr` (ids are counters) rebuilds the state a restarted daemon finds.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "op", rename_all = "snake_case")]
    pub enum Logged {
        CreateWorkspace {
            label: String,
            cwd: PathBuf,
            env: Vec<(String, String)>,
        },
        CreateTab {
            workspace: HerdrWorkspaceId,
            label: String,
            cwd: PathBuf,
            env: Vec<(String, String)>,
        },
        SplitPane {
            target: HerdrPaneId,
            down: bool,
            cwd: PathBuf,
            env: Vec<(String, String)>,
        },
        RenameWorkspace {
            id: HerdrWorkspaceId,
            label: String,
        },
        RenameTab {
            id: HerdrTabId,
            label: String,
        },
        RenamePane {
            id: HerdrPaneId,
            label: String,
        },
        CloseWorkspace {
            id: HerdrWorkspaceId,
        },
        CloseTab {
            id: HerdrTabId,
        },
        ClosePane {
            id: HerdrPaneId,
        },
        PaneMetadata {
            id: HerdrPaneId,
            key: String,
            value: String,
        },
        WorkspaceMetadata {
            id: HerdrWorkspaceId,
            key: String,
            value: String,
        },
        StartAgent {
            pane: HerdrPaneId,
            kind: String,
            args: Vec<String>,
        },
    }

    pub struct PersistentFakeHerdr {
        inner: Arc<FakeHerdr>,
        path: PathBuf,
        log: Mutex<Vec<Logged>>,
        replayed: tokio::sync::OnceCell<()>,
    }

    impl PersistentFakeHerdr {
        pub fn open(path: PathBuf) -> Self {
            let log = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            Self {
                inner: FakeHerdr::new(),
                path,
                log: Mutex::new(log),
                replayed: tokio::sync::OnceCell::new(),
            }
        }

        async fn replay(&self) {
            self.replayed
                .get_or_init(|| async {
                    let entries = self.log.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    for e in entries {
                        if let Err(err) = self.apply(&e).await {
                            eprintln!(
                                "herdr-graph daemon: fake herdr replay of {e:?} failed: {err}"
                            );
                        }
                    }
                })
                .await;
        }

        async fn apply(&self, e: &Logged) -> Result<(), HerdrError> {
            let h = &self.inner;
            match e.clone() {
                Logged::CreateWorkspace { label, cwd, env } => h
                    .create_workspace(CreateWorkspace { label, cwd, env })
                    .await
                    .map(drop),
                Logged::CreateTab {
                    workspace,
                    label,
                    cwd,
                    env,
                } => h
                    .create_tab(CreateTab {
                        workspace,
                        label,
                        cwd,
                        env,
                    })
                    .await
                    .map(drop),
                Logged::SplitPane {
                    target,
                    down,
                    cwd,
                    env,
                } => {
                    let direction = if down {
                        SplitDirection::Down
                    } else {
                        SplitDirection::Right
                    };
                    h.split_pane(SplitPane {
                        target,
                        direction,
                        cwd,
                        env,
                    })
                    .await
                    .map(drop)
                }
                Logged::RenameWorkspace { id, label } => h.rename_workspace(&id, &label).await,
                Logged::RenameTab { id, label } => h.rename_tab(&id, &label).await,
                Logged::RenamePane { id, label } => h.rename_pane(&id, &label).await,
                Logged::CloseWorkspace { id } => h.close_workspace(&id).await,
                Logged::CloseTab { id } => h.close_tab(&id).await,
                Logged::ClosePane { id } => h.close_pane(&id).await,
                Logged::PaneMetadata { id, key, value } => {
                    h.report_pane_metadata(&id, &key, &value).await
                }
                Logged::WorkspaceMetadata { id, key, value } => {
                    h.report_workspace_metadata(&id, &key, &value).await
                }
                Logged::StartAgent { pane, kind, args } => h
                    .start_agent(StartAgent { pane, kind, args })
                    .await
                    .map(drop),
            }
        }

        /// Record a call that succeeded; the file is written before the caller sees the result, so a crash
        /// right after the call still leaves it on disk.
        fn record<T>(&self, entry: Logged, result: Result<T, HerdrError>) -> Result<T, HerdrError> {
            if result.is_ok() {
                let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
                log.push(entry);
                if let Ok(bytes) = serde_json::to_vec_pretty(&*log) {
                    let tmp = self.path.with_extension("json.tmp");
                    if std::fs::write(&tmp, bytes).is_ok() {
                        let _ = std::fs::rename(&tmp, &self.path);
                    }
                }
            }
            result
        }
    }

    #[async_trait::async_trait]
    impl HerdrApi for PersistentFakeHerdr {
        async fn snapshot(&self) -> Result<HerdrSnapshot, HerdrError> {
            self.replay().await;
            self.inner.snapshot().await
        }
        async fn subscribe(&self) -> Result<HerdrEventStream, HerdrError> {
            self.replay().await;
            self.inner.subscribe().await
        }
        async fn create_workspace(&self, req: CreateWorkspace) -> Result<Created, HerdrError> {
            self.replay().await;
            let e = Logged::CreateWorkspace {
                label: req.label.clone(),
                cwd: req.cwd.clone(),
                env: req.env.clone(),
            };
            self.record(e, self.inner.create_workspace(req).await)
        }
        async fn rename_workspace(
            &self,
            id: &HerdrWorkspaceId,
            label: &str,
        ) -> Result<(), HerdrError> {
            self.replay().await;
            self.record(
                Logged::RenameWorkspace {
                    id: id.clone(),
                    label: label.into(),
                },
                self.inner.rename_workspace(id, label).await,
            )
        }
        async fn close_workspace(&self, id: &HerdrWorkspaceId) -> Result<(), HerdrError> {
            self.replay().await;
            self.record(
                Logged::CloseWorkspace { id: id.clone() },
                self.inner.close_workspace(id).await,
            )
        }
        async fn create_tab(&self, req: CreateTab) -> Result<Created, HerdrError> {
            self.replay().await;
            let e = Logged::CreateTab {
                workspace: req.workspace.clone(),
                label: req.label.clone(),
                cwd: req.cwd.clone(),
                env: req.env.clone(),
            };
            self.record(e, self.inner.create_tab(req).await)
        }
        async fn rename_tab(&self, id: &HerdrTabId, label: &str) -> Result<(), HerdrError> {
            self.replay().await;
            self.record(
                Logged::RenameTab {
                    id: id.clone(),
                    label: label.into(),
                },
                self.inner.rename_tab(id, label).await,
            )
        }
        async fn close_tab(&self, id: &HerdrTabId) -> Result<(), HerdrError> {
            self.replay().await;
            self.record(
                Logged::CloseTab { id: id.clone() },
                self.inner.close_tab(id).await,
            )
        }
        async fn split_pane(&self, req: SplitPane) -> Result<Created, HerdrError> {
            self.replay().await;
            let e = Logged::SplitPane {
                target: req.target.clone(),
                down: req.direction == SplitDirection::Down,
                cwd: req.cwd.clone(),
                env: req.env.clone(),
            };
            self.record(e, self.inner.split_pane(req).await)
        }
        async fn rename_pane(&self, id: &HerdrPaneId, label: &str) -> Result<(), HerdrError> {
            self.replay().await;
            self.record(
                Logged::RenamePane {
                    id: id.clone(),
                    label: label.into(),
                },
                self.inner.rename_pane(id, label).await,
            )
        }
        async fn close_pane(&self, id: &HerdrPaneId) -> Result<(), HerdrError> {
            self.replay().await;
            self.record(
                Logged::ClosePane { id: id.clone() },
                self.inner.close_pane(id).await,
            )
        }
        async fn report_pane_metadata(
            &self,
            id: &HerdrPaneId,
            key: &str,
            value: &str,
        ) -> Result<(), HerdrError> {
            self.replay().await;
            let e = Logged::PaneMetadata {
                id: id.clone(),
                key: key.into(),
                value: value.into(),
            };
            self.record(e, self.inner.report_pane_metadata(id, key, value).await)
        }
        async fn report_workspace_metadata(
            &self,
            id: &HerdrWorkspaceId,
            key: &str,
            value: &str,
        ) -> Result<(), HerdrError> {
            self.replay().await;
            let e = Logged::WorkspaceMetadata {
                id: id.clone(),
                key: key.into(),
                value: value.into(),
            };
            self.record(
                e,
                self.inner.report_workspace_metadata(id, key, value).await,
            )
        }
        async fn start_agent(&self, req: StartAgent) -> Result<StartOutcome, HerdrError> {
            self.replay().await;
            let e = Logged::StartAgent {
                pane: req.pane.clone(),
                kind: req.kind.clone(),
                args: req.args.clone(),
            };
            self.record(e, self.inner.start_agent(req).await)
        }
        async fn agent(&self, pane: &HerdrPaneId) -> Result<Option<AgentInfo>, HerdrError> {
            self.replay().await;
            self.inner.agent(pane).await
        }
        async fn process_info(&self, pane: &HerdrPaneId) -> Result<ProcessInfo, HerdrError> {
            self.replay().await;
            self.inner.process_info(pane).await
        }
        async fn send_keys(&self, pane: &HerdrPaneId, keys: &[KeyInput]) -> Result<(), HerdrError> {
            self.replay().await;
            self.inner.send_keys(pane, keys).await
        }
    }

    pub fn services(ctx: &DaemonCtx) -> Services {
        Services::new(
            Arc::new(PersistentFakeHerdr::open(
                ctx.paths.local.join("fake-herdr.json"),
            )),
            Arc::new(crate::threads::FakeThreads::new()),
            Arc::new(crate::threads::FakePaneSeatMap::new()),
            Arc::new(SystemClock),
        )
    }
}
