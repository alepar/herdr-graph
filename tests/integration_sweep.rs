//! Root integration sweep (hg-zmi.20): the unknown-unknowns net over the composed daemon.
//!
//! * `golden` (feature `private-herdr`): the goal's main flow end to end, real daemon and CLI against a PRIVATE
//!   Herdr, harness `shell` (the test plays the summarizer through the CLI).
//! * `wiring` (default tier): in-process `compose_with` against FakeHerdr / FakeThreads, one test per seam that no
//!   per-seam bead covers (config values, launch env, harness profiles, record envelopes, capability switch).
//! * `cli_sweep` (feature `test-support`): the real binary against a daemon with fake services.
//!
//! Nothing here touches a live Herdr, live threads or the memory observer.

#[cfg(any(feature = "private-herdr", feature = "test-support"))]
mod support;

// =============================================================================================
// golden path
// =============================================================================================

#[cfg(feature = "private-herdr")]
mod golden {
    use super::support;
    use herdr_graph::config::InstancePaths;
    use herdr_graph::daemon::client::Client;
    use herdr_graph::daemon::lock;
    use herdr_graph::herdr::HerdrClient;
    use herdr_graph::ports::herdr::*;
    use serde_json::{Value, json};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};
    use std::time::{Duration, Instant};
    use support::private_herdr::PrivateHerdr;

    const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");
    const WAIT: Duration = Duration::from_secs(40);

    struct Rig {
        herdr: PrivateHerdr,
        instance: PathBuf,
        rt: tokio::runtime::Runtime,
    }

    fn stdout(o: &Output) -> String {
        String::from_utf8_lossy(&o.stdout).into_owned()
    }
    fn stderr(o: &Output) -> String {
        String::from_utf8_lossy(&o.stderr).into_owned()
    }

    fn tstr(v: &toml::Value, path: &str) -> String {
        let mut cur = v;
        for key in path.split('.') {
            match cur.get(key) {
                Some(n) => cur = n,
                None => return String::new(),
            }
        }
        cur.as_str().unwrap_or_default().to_owned()
    }

    struct Row {
        id: String,
        name: String,
        state: String,
        path: PathBuf,
    }

    impl Rig {
        fn start() -> Option<Self> {
            let herdr = match PrivateHerdr::start() {
                Ok(h) => h,
                Err(e) if support::herdr_binary().is_none() => {
                    support::skip(&format!("{e}"));
                    return None;
                }
                Err(e) => panic!("private herdr failed to start: {e:#}"),
            };
            let instance = herdr.root.join("graph");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            Some(Self {
                herdr,
                instance,
                rt,
            })
        }

        fn command(&self) -> Command {
            let mut c = self.herdr.command(Path::new(BIN));
            c.env("HERDR_GRAPH_INSTANCE", &self.instance);
            c
        }

        fn cli(&self, args: &[&str]) -> Output {
            self.command().args(args).output().unwrap()
        }

        fn cli_ok(&self, args: &[&str]) -> String {
            let o = self.cli(args);
            assert!(
                o.status.success(),
                "{args:?} failed: {}{}",
                stderr(&o),
                stdout(&o)
            );
            stdout(&o)
        }

        /// `plan --json` then `apply --confirm <hash> --confirmed-by user-relay`; returns the plan reply and
        /// the apply output.
        fn plan_apply(&self, words: &[&str]) -> (Value, String) {
            let mut args = vec!["plan", "--json"];
            args.extend_from_slice(words);
            let plan: Value = serde_json::from_str(&self.cli_ok(&args))
                .unwrap_or_else(|e| panic!("plan {words:?}: {e}"));
            let (id, hash) = (
                plan["plan_id"].as_str().expect("plan_id"),
                plan["hash"].as_str().expect("hash"),
            );
            let out = self.cli(&[
                "apply",
                id,
                "--confirm",
                hash,
                "--confirmed-by",
                "user-relay",
            ]);
            let text = format!("{}{}", stdout(&out), stderr(&out));
            assert!(
                out.status.success(),
                "apply {words:?} did not commit: {text}"
            );
            (plan, text)
        }

        fn snapshot(&self) -> HerdrSnapshot {
            let c: HerdrClient = self.herdr.client();
            self.rt.block_on(c.snapshot()).expect("snapshot")
        }

        fn raw_ok(&self, method: &'static str, params: Value) -> Value {
            let c = self.herdr.client();
            self.rt
                .block_on(c.request(method, params))
                .unwrap_or_else(|e| panic!("{method}: {e}"))
        }

        fn tab_labelled(&self, label: &str) -> Option<TabInfo> {
            self.snapshot()
                .workspaces
                .into_iter()
                .flat_map(|w| w.tabs)
                .find(|t| t.label == label)
        }

        fn pane_of(&self, clone: &str) -> Option<PaneInfo> {
            let token = format!("hg={clone}");
            self.snapshot()
                .workspaces
                .iter()
                .flat_map(|w| &w.tabs)
                .flat_map(|t| &t.panes)
                .find(|p| p.metadata.get("hg").map(String::as_str) == Some(token.as_str()))
                .cloned()
        }

        fn wait_until(&self, desc: &str, mut pred: impl FnMut(&Self) -> bool) {
            let deadline = Instant::now() + WAIT;
            while Instant::now() < deadline {
                if pred(self) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            panic!(
                "timed out ({}s) waiting for {desc}\ndaemon log:\n{}",
                WAIT.as_secs(),
                self.daemon_log()
            );
        }

        fn daemon_log(&self) -> String {
            std::fs::read_to_string(self.instance.join(".graph-local/daemon.log"))
                .unwrap_or_default()
        }

        /// Effects that act on Herdr and are not terminal. Thread effects stay pending here (no threads
        /// service runs in this rig), so they are not waited for.
        fn open_herdr_effects(&self) -> Vec<(String, String)> {
            use herdr_graph::model::effect::EffectStatus::{NeedsRevision, Pending, Unknown};
            const THREADS: [&str; 6] = [
                "ensure_thread",
                "invite",
                "release_requirement",
                "notify",
                "set_topic",
                "deliver_request",
            ];
            let j =
                herdr_graph::journal::Journal::open(&InstancePaths::new(&self.instance).journal)
                    .expect("journal");
            j.effects_with_status(&[Pending, Unknown, NeedsRevision])
                .expect("effects")
                .into_iter()
                .filter(|e| !THREADS.contains(&e.kind.as_str()))
                .map(|e| (e.kind.as_str().to_owned(), e.object.to_string()))
                .collect()
        }

        fn wait_settled(&self) {
            self.wait_until("the reconciler to settle", |r| {
                Client::connect(
                    &InstancePaths::new(&r.instance).socket,
                    Duration::from_secs(30),
                )
                .is_ok()
                    && r.open_herdr_effects().is_empty()
            });
        }

        fn show(&self, id: &str) -> toml::Value {
            let text = self.cli_ok(&["show", id]);
            toml::Value::Table(
                text.parse::<toml::Table>()
                    .unwrap_or_else(|e| panic!("show {id} is not toml ({e}): {text}")),
            )
        }

        fn rows(&self, kind: &str) -> Vec<Row> {
            self.cli_ok(&["list", kind])
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| {
                    let mut f = l.split("  ");
                    Row {
                        id: f.next().unwrap().to_owned(),
                        name: f.next().unwrap_or_default().to_owned(),
                        state: f.next().unwrap_or_default().to_owned(),
                        path: PathBuf::from(f.next().unwrap_or_default()),
                    }
                })
                .collect()
        }

        fn row(&self, kind: &str, name: &str, state: &str) -> Option<Row> {
            self.rows(kind)
                .into_iter()
                .find(|r| r.name == name && r.state == state)
        }

        fn clone_of(&self, seat_id: &str, state: &str) -> Option<Row> {
            self.rows("clones")
                .into_iter()
                .find(|c| c.state == state && tstr(&self.show(&c.id), "seat") == seat_id)
        }

        fn undo_candidates(&self) -> Vec<Value> {
            let v: Value =
                serde_json::from_str(&self.cli_ok(&["undo", "--json"])).expect("undo --json");
            v.as_array().cloned().unwrap_or_default()
        }

        fn pane_sh(&self, pane: &str, cmd: &str) {
            self.raw_ok("pane.send_text", json!({ "pane_id": pane, "text": cmd }));
            self.raw_ok(
                "pane.send_keys",
                json!({ "pane_id": pane, "keys": ["Enter"] }),
            );
        }

        /// Run `cmd` in a graph pane; what it printed lands in a file under the private root.
        fn pane_capture(&self, pane: &str, name: &str, cmd: &str) -> String {
            let file = self.herdr.root.join(name);
            self.pane_sh(pane, &format!("{cmd} > {} 2>&1", file.display()));
            self.wait_until(&format!("{name} output"), |_| {
                std::fs::metadata(&file).is_ok_and(|m| m.len() > 0)
            });
            std::thread::sleep(Duration::from_millis(200));
            std::fs::read_to_string(&file).unwrap()
        }

        /// The CLI as the hook of a graph pane runs it: `HERDR_GRAPH_CLONE` set, a payload on stdin.
        fn session_report(&self, clone: &str, payload: &str) -> Output {
            let mut child = self
                .command()
                .env("HERDR_GRAPH_CLONE", clone)
                .args(["session-report", "--from-hook", "claude"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(payload.as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        }

        /// A shipped template (its `template.toml` in the instance) with every member's harness forced to
        /// `shell`, written to a file.
        fn shell_template(&self, name: &str) -> String {
            let row = self
                .row("templates", name, "-")
                .unwrap_or_else(|| panic!("no template {name}"));
            let text = std::fs::read_to_string(row.path.join("template.toml")).unwrap();
            let mut doc: toml::Table = text
                .parse()
                .unwrap_or_else(|e| panic!("template {name}: {e}"));
            let members = doc
                .get_mut("members")
                .and_then(toml::Value::as_array_mut)
                .expect("template members");
            for m in members {
                let t = m.as_table_mut().unwrap();
                let defaults = t
                    .entry("defaults")
                    .or_insert_with(|| toml::Value::Table(Default::default()));
                defaults
                    .as_table_mut()
                    .unwrap()
                    .insert("harness".into(), toml::Value::String("shell".into()));
            }
            let path = self.herdr.root.join(format!("{name}-shell.toml"));
            std::fs::write(&path, toml::to_string(&doc).unwrap()).unwrap();
            path.to_string_lossy().into_owned()
        }
    }

    impl Drop for Rig {
        /// The daemon is detached and would outlive the test: stop it after an argv check.
        fn drop(&mut self) {
            if let Some(pid) = lock::read_info(&self.instance.join(".graph-local/daemon.lock")).map(|i| i.pid)
                // SAFETY: signal 0 only probes existence.
                && unsafe { libc::kill(pid as i32, 0) } == 0
            {
                let ps = Command::new("/bin/ps")
                    .args(["-o", "command=", "-p", &pid.to_string()])
                    .output()
                    .unwrap();
                let cmdline = String::from_utf8_lossy(&ps.stdout).into_owned();
                if cmdline.contains(BIN) && cmdline.contains(" daemon") {
                    // SAFETY: argv-verified as this rig's daemon.
                    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                }
            }
            // Everything else that inherited the root marker (pane shells, hook-started daemons).
            support::isolated::reap_root(&self.herdr.root);
        }
    }

    #[test]
    fn sweep_golden_path() {
        let Some(rig) = Rig::start() else { return };

        // init --with-examples, daemon.
        let inst = rig.instance.to_str().unwrap().to_owned();
        let out = rig.cli(&["init", &inst, "--with-examples"]);
        assert!(
            out.status.success(),
            "init: {}{}",
            stderr(&out),
            stdout(&out)
        );
        rig.cli_ok(&["daemon", "--ensure"]);

        // Shell-harness versions of the shipped templates (no agent binaries at tier 3).
        for tpl in ["project-team", "system-summarizer"] {
            let doc = rig.shell_template(tpl);
            rig.plan_apply(&["template", "edit", tpl, "--from", &doc]);
        }

        // Teamspace and both applications.
        rig.plan_apply(&["teamspace", "create", "demo", "--active"]);
        rig.plan_apply(&[
            "application",
            "apply",
            "project-team",
            "--teamspace",
            "demo",
            "--name",
            "proj",
        ]);
        rig.plan_apply(&[
            "application",
            "apply",
            "system-summarizer",
            "--teamspace",
            "demo",
            "--name",
            "sum",
        ]);
        rig.wait_until("the foreman and summarizer tabs", |r| {
            r.tab_labelled("foreman").is_some() && r.tab_labelled("summarizer").is_some()
        });
        rig.wait_settled();
        assert!(
            rig.tab_labelled("researcher").is_none(),
            "a deferred member gets no tab"
        );

        // Seats activate: tab, pane and the four launch env values.
        let foreman = rig.row("seats", "foreman", "active").expect("foreman seat");
        let fclone = rig.clone_of(&foreman.id, "active").expect("foreman clone");
        let tab = rig.tab_labelled("foreman").unwrap();
        assert_eq!(tab.panes.len(), 1);
        let pane = rig
            .pane_of(&fclone.id)
            .expect("foreman pane carries the clone token")
            .id
            .0;
        assert_eq!(tab.panes[0].id.0, pane);
        let env = rig.pane_capture(&pane, "env-foreman.txt", "env | grep '^HERDR_GRAPH' | sort");
        for want in [
            "HERDR_GRAPH=1".to_owned(),
            format!("HERDR_GRAPH_INSTANCE={}", rig.instance.display()),
            format!("HERDR_GRAPH_SEAT={}", foreman.id),
            format!("HERDR_GRAPH_CLONE={}", fclone.id),
        ] {
            assert!(env.lines().any(|l| l == want), "missing {want} in {env}");
        }

        // /seat resolves from the pane.
        let seat: Value = serde_json::from_str(&rig.pane_capture(
            &pane,
            "seat.json",
            &format!("{BIN} seat --json"),
        ))
        .expect("seat --json");
        assert_eq!(seat["resolution"]["status"], "bound", "{seat}");
        assert_eq!(seat["resolution"]["seat"], foreman.id.as_str());
        assert_eq!(seat["resolution"]["clone"], fclone.id.as_str());

        // Rename the tab in Herdr: the seat is renamed, its folder moves, history is kept.
        rig.raw_ok(
            "tab.rename",
            json!({ "tab_id": tab.id.0, "label": "chief" }),
        );
        rig.wait_until("the seat to be renamed", |r| {
            r.row("seats", "chief", "active").is_some()
        });
        // Committed reads can observe the rename before the derived worktree fast-forward finishes.
        rig.wait_until("the renamed working-tree view", |r| {
            use herdr_graph::ports::store::Store;
            let store = herdr_graph::store::GitStore::open(&r.instance).unwrap();
            herdr_graph::writer::worktree::view_rev(&r.instance) == Some(store.head().unwrap())
        });
        let chief = rig.row("seats", "chief", "active").unwrap();
        assert_eq!(chief.id, foreman.id, "same seat, new name");
        assert!(
            chief.path.exists() && !foreman.path.exists(),
            "folder moved {} -> {}",
            foreman.path.display(),
            chief.path.display()
        );
        assert!(
            rig.show(&chief.id)["name_history"]
                .to_string()
                .contains("foreman")
        );
        rig.wait_settled();

        // The seat's session: a fake transcript, reported the way the SessionStart hook does.
        let transcript = rig.herdr.root.join("foreman-session.jsonl");
        let body =
            "{\"role\":\"user\",\"text\":\"hello\"}\n{\"role\":\"assistant\",\"text\":\"hi\"}\n";
        std::fs::write(&transcript, body).unwrap();
        let payload = format!(
            r#"{{"session_id":"sess-1","transcript_path":"{}","cwd":"/tmp","source":"startup"}}"#,
            transcript.display()
        );
        let rep = rig.session_report(&fclone.id, &payload);
        assert!(
            rep.status.success() && stderr(&rep).is_empty(),
            "session-report: {}",
            stderr(&rep)
        );
        rig.wait_until("the session on the clone", |r| {
            r.show(&fclone.id)["sessions"].as_array().is_some_and(|s| {
                s.iter()
                    .any(|s| s["native_session_id"].as_str() == Some("sess-1"))
            })
        });

        // Close the last pane: pane -> tab -> seat, retired once, one act_.
        rig.raw_ok("pane.close", json!({ "pane_id": pane }));
        rig.wait_until("the seat to retire", |r| {
            tstr(&r.show(&chief.id), "lifecycle") == "retired"
        });
        rig.wait_settled();
        let archived = rig
            .row("seats", "chief", "retired")
            .expect("retired seat listed")
            .path;
        assert!(
            archived.to_string_lossy().contains("/archive/"),
            "{}",
            archived.display()
        );
        let all_acts = rig.undo_candidates();
        let acts: Vec<Value> = all_acts
            .iter()
            .filter(|c| c["kind"] == "closure_cascade")
            .cloned()
            .collect();
        assert!(
            acts[0]["summary"]
                .as_str()
                .unwrap()
                .contains("1 seat, 1 clone"),
            "{acts:?}"
        );
        assert_eq!(acts.len(), 1, "exactly one act for the cascade: {acts:?}");
        let act = acts[0]["act"].as_str().expect("act id").to_owned();
        assert!(act.starts_with("act_"), "{act}");

        // A transcript request exists. No threads daemon runs here, and the summarizer shell has no agent
        // occupant, so the request cannot be delivered: it stays pending with a recorded reason (the delivery
        // path itself is tier 2 / tier 5).
        rig.wait_until("a pending request", |r| {
            r.cli_ok(&["request", "list", "--pending"]).contains("rq_")
        });
        let line = rig.cli_ok(&["request", "list", "--pending"]);
        let mut cols = line.split_whitespace();
        let (rq, status, tr) = (
            cols.next().unwrap().to_owned(),
            cols.next().unwrap().to_owned(),
            cols.next().unwrap().to_owned(),
        );
        assert!(rq.starts_with("rq_") && tr.starts_with("tr_"), "{line}");
        assert!(
            line.contains(&format!("0-{}", body.len())),
            "the request covers the transcript: {line}"
        );
        let rq_doc = rig.show(&rq);
        eprintln!(
            "GOLDEN delivery: status={status} undeliverable={:?} attempts={}",
            rq_doc.get("undeliverable"),
            rq_doc["delivery"]["attempts"]
        );
        assert!(
            status == "pending" || status == "delivered",
            "pending with a reason, or delivered by the configured capability: {line}"
        );

        // ACK records dispatch, never completion.
        assert_eq!(
            rig.cli_ok(&["request", "ack", &rq]).trim(),
            format!("{rq} dispatched")
        );
        let acked = rig.show(&rq);
        assert!(
            acked["delivery"].get("dispatched_at").is_some(),
            "dispatched_at set: {acked}"
        );
        assert_ne!(tstr(&acked, "status"), "completed");

        // The summarizer writes into the archived seat folder (summaries/ only), then completes.
        let summary = rig.herdr.root.join("summary.md");
        std::fs::write(&summary, "# hello\nthe user said hello\n").unwrap();
        let out = rig.cli_ok(&[
            "content",
            "write",
            "--object",
            &chief.id,
            "--rel",
            "summaries/x.md",
            "--from",
            summary.to_str().unwrap(),
        ]);
        assert!(out.contains("committed"), "{out}");
        assert!(
            archived.join("summaries/x.md").exists(),
            "landed in the archived folder {}",
            archived.display()
        );
        let covered = format!("0-{}", body.len());
        assert_eq!(
            rig.cli_ok(&[
                "request",
                "complete",
                &rq,
                "--output",
                "summaries/x.md",
                "--covered",
                &covered
            ])
            .trim(),
            format!("{rq} completed")
        );
        assert_eq!(tstr(&rig.show(&rq), "status"), "completed");
        let coverage = rig.show(&tr)["coverage"].to_string();
        assert!(
            coverage.contains(&format!("end = {}", body.len()))
                || coverage.contains(&body.len().to_string()),
            "coverage recorded: {coverage}"
        );
        assert!(
            !rig.cli_ok(&["request", "list", "--pending"]).contains(&rq),
            "no longer pending"
        );

        // Undo the cascade: the seat and its clone come back, in a NEW tab.
        let (plan, text) = rig.plan_apply(&["undo", &act]);
        assert!(
            plan["rendered"].as_str().unwrap().contains("chief"),
            "{}",
            plan["rendered"]
        );
        assert!(text.contains("committed"), "{text}");
        rig.wait_until("the seat to be active again", |r| {
            tstr(&r.show(&chief.id), "lifecycle") == "active"
        });
        rig.wait_until("the restored clone's pane", |r| {
            r.pane_of(&fclone.id).is_some()
        });
        rig.wait_settled();
        assert_eq!(tstr(&rig.show(&fclone.id), "lifecycle"), "active");
        let restored = rig.tab_labelled("chief").expect("restored tab");
        assert_ne!(restored.id, tab.id, "a new tab");
        assert_eq!(restored.panes.len(), 1);
        let again = rig
            .row("seats", "chief", "active")
            .expect("seat active again");
        assert!(again.path.exists(), "folder is back out of the archive");
        assert!(
            again.path.join("summaries/x.md").exists(),
            "the summary went back with the folder"
        );
    }
}

// =============================================================================================
// wiring sweep (default tier): the composed daemon against fakes
// =============================================================================================

mod wiring {
    use herdr_graph::config::InstancePaths;
    use herdr_graph::daemon::DaemonCtx;
    use herdr_graph::daemon::client::{Client, ClientError};
    use herdr_graph::daemon::compose::{Services, compose_with};
    use herdr_graph::daemon::registry::{Registry, Shutdown, shutdown_channel};
    use herdr_graph::herdr::FakeHerdr;
    use herdr_graph::herdr::fake::FakeCall;
    use herdr_graph::journal::Journal;
    use herdr_graph::model::HerdrPaneId;
    use herdr_graph::model::action::ActionRecord;
    use herdr_graph::model::clone::CloneRecord;
    use herdr_graph::model::effect::{EffectRecord, EffectStatus};
    use herdr_graph::model::harness::{Harness, profile};
    use herdr_graph::model::launch::{ENV_CLONE, ENV_GRAPH, ENV_INSTANCE, ENV_SEAT};
    use herdr_graph::model::seat::SeatRecord;
    use herdr_graph::ports::clock::ManualClock;
    use herdr_graph::ports::herdr::{AgentInfo, AgentSession, AgentStatus, HerdrApi};
    use herdr_graph::ports::store::Store;
    use herdr_graph::ports::threads::*;
    use herdr_graph::store::GitStore;
    use herdr_graph::store::init::init_instance;
    use herdr_graph::store::layout;
    use herdr_graph::store::tree::CommitView;
    use herdr_graph::threads::{FakePaneSeatMap, FakeThreads};
    use serde_json::{Value, json};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
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

    type Sent = (ThreadRef, Vec<ThreadsSeatRef>, String);

    /// FakeThreads that supports the ACK-required delivery path: `send_request` records its recipients,
    /// everything else is the plain fake. `capability` is what `delivery_capability` reports.
    struct AckThreads {
        inner: Arc<FakeThreads>,
        capability: DeliveryCapability,
        sent: Mutex<Vec<Sent>>,
    }

    #[async_trait::async_trait]
    impl ThreadsPort for AckThreads {
        async fn ensure_thread(
            &self,
            s: ChannelScope,
            t: &str,
            k: &OpKey,
        ) -> Result<ThreadRef, ThreadsError> {
            self.inner.ensure_thread(s, t, k).await
        }
        async fn invite(
            &self,
            t: &ThreadRef,
            s: &ThreadsSeatRef,
            c: herdr_graph::model::clone::InviteConstraint,
            k: &OpKey,
        ) -> Result<(), ThreadsError> {
            self.inner.invite(t, s, c, k).await
        }
        async fn membership(
            &self,
            t: &ThreadRef,
            s: &ThreadsSeatRef,
        ) -> Result<Option<herdr_graph::model::clone::InvitationState>, ThreadsError> {
            self.inner.membership(t, s).await
        }
        async fn notify(
            &self,
            t: &ThreadRef,
            sev: Severity,
            b: &str,
            k: &OpKey,
        ) -> Result<(), ThreadsError> {
            self.inner.notify(t, sev, b, k).await
        }
        async fn set_topic(
            &self,
            t: &ThreadRef,
            topic: &str,
            k: &OpKey,
        ) -> Result<(), ThreadsError> {
            self.inner.set_topic(t, topic, k).await
        }
        async fn release_requirement(
            &self,
            t: &ThreadRef,
            s: &ThreadsSeatRef,
            k: &OpKey,
        ) -> Result<(), ThreadsError> {
            self.inner.release_requirement(t, s, k).await
        }
        async fn send_request(
            &self,
            thread: &ThreadRef,
            recipients: &[ThreadsSeatRef],
            body: &str,
            _op_key: &OpKey,
        ) -> Result<MessageRef, ThreadsError> {
            let mut sent = self.sent.lock().unwrap();
            sent.push((thread.clone(), recipients.to_vec(), body.to_owned()));
            Ok(MessageRef(format!("msg-{}", sent.len())))
        }
        async fn receipt_state(
            &self,
            _messages: &[MessageRef],
        ) -> Result<Vec<MessageReceipts>, ThreadsError> {
            Ok(Vec::new())
        }
        async fn delivery_capability(&self) -> Result<DeliveryCapability, ThreadsError> {
            Ok(self.capability)
        }
    }

    #[derive(Clone)]
    struct Fakes {
        herdr: Arc<FakeHerdr>,
        threads: Arc<FakeThreads>,
        panes: Arc<FakePaneSeatMap>,
        clock: Arc<ManualClock>,
    }

    impl Fakes {
        fn new() -> Self {
            Self {
                herdr: FakeHerdr::new(),
                threads: Arc::new(FakeThreads::new()),
                panes: Arc::new(FakePaneSeatMap::new()),
                clock: Arc::new(ManualClock::new(chrono::Utc::now())),
            }
        }

        fn services(&self, threads: Arc<dyn ThreadsPort>) -> Services {
            let mut s = Services::new(
                self.herdr.clone(),
                threads,
                self.panes.clone(),
                self.clock.clone(),
            );
            s.reminder_period = Duration::from_millis(40);
            s
        }
    }

    /// A composed daemon serving on a temp socket, with its loops spawned.
    struct Daemon {
        root: PathBuf,
        sock: PathBuf,
        fakes: Fakes,
        stop: Arc<tokio::sync::watch::Sender<bool>>,
        tasks: Vec<tokio::task::JoinHandle<()>>,
    }

    impl Daemon {
        async fn start(root: &Path) -> Daemon {
            let fakes = Fakes::new();
            let threads = fakes.threads.clone();
            Self::start_with(root, fakes, threads).await
        }

        async fn start_with(root: &Path, fakes: Fakes, threads: Arc<dyn ThreadsPort>) -> Daemon {
            let ctx = DaemonCtx {
                paths: InstancePaths::new(root),
                herdr_socket: root.join("no-herdr.sock"),
                started_at: chrono::Utc::now(),
                claude_root: root.join("claude"),
            };
            let mut reg = Registry::default();
            compose_with(&mut reg, &ctx, fakes.services(threads))
                .await
                .expect("compose");
            let (stop, shutdown): (_, Shutdown) = shutdown_channel();
            let mut tasks: Vec<_> = reg
                .take_loops()
                .into_iter()
                .map(|(_, f)| {
                    let fut = f(shutdown.clone());
                    tokio::spawn(async move {
                        let _ = fut.await;
                    })
                })
                .collect();
            let sock = root.join("t.sock");
            let _ = std::fs::remove_file(&sock);
            let listener = tokio::net::UnixListener::bind(&sock).unwrap();
            let builtins = herdr_graph::daemon::server::Builtins {
                instance: root.to_path_buf(),
                herdr_socket: root.join("no-herdr.sock"),
                started_at: chrono::Utc::now(),
                shutdown_tx: stop.clone(),
            };
            tasks.push(tokio::spawn(herdr_graph::daemon::server::serve(
                listener,
                Arc::new(reg),
                builtins,
                shutdown,
            )));
            Daemon {
                root: root.to_path_buf(),
                sock,
                fakes,
                stop,
                tasks,
            }
        }

        async fn call(&self, kind: &str, args: Value) -> Result<Value, ClientError> {
            let (sock, kind) = (self.sock.clone(), kind.to_owned());
            tokio::task::spawn_blocking(move || {
                let mut c = Client::connect(&sock, Duration::from_secs(30))
                    .map_err(|e| ClientError::Unavailable(e.to_string()))?;
                c.call(&kind, args)
            })
            .await
            .unwrap()
        }

        /// `plan.create` then `plan.apply` with the relay confirmation; the op must commit.
        async fn committed(&self, words: &[&str]) -> Value {
            let plan = self
                .call("plan.create", json!({ "words": words }))
                .await
                .unwrap_or_else(|e| panic!("plan {words:?}: {e}"));
            let apply =
                json!({ "plan": plan["plan_id"], "confirm": plan["hash"], "mode": "relay" });
            let r = self
                .call("plan.apply", apply)
                .await
                .unwrap_or_else(|e| panic!("apply {words:?}: {e}"));
            assert_eq!(r["state"], "committed", "{words:?}: {r}");
            r
        }

        fn with_view<T>(&self, f: impl FnOnce(&dyn herdr_graph::store::tree::TreeRead) -> T) -> T {
            let store = GitStore::open(&self.root).unwrap();
            let at = store.head().unwrap();
            f(&CommitView { store: &store, at })
        }

        fn seat(&self, name: &str) -> SeatRecord {
            self.with_view(|v| {
                layout::all_seats(v)
                    .unwrap()
                    .into_iter()
                    .map(|(_, s)| s)
                    .find(|s| s.name == name)
            })
            .unwrap_or_else(|| panic!("no seat {name}"))
        }

        fn clones_of(&self, seat: &str) -> Vec<CloneRecord> {
            let id = self.seat(seat).id;
            self.with_view(|v| {
                layout::all_clones(v)
                    .unwrap()
                    .into_iter()
                    .map(|(_, c)| c)
                    .filter(|c| c.seat == id)
                    .collect()
            })
        }

        fn clone_of(&self, seat: &str) -> CloneRecord {
            self.clones_of(seat)
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("no clone of {seat}"))
        }

        fn pane(&self, seat: &str) -> Option<HerdrPaneId> {
            self.clone_of(seat).runtime.bound.and_then(|b| b.pane_id)
        }

        fn request_count(&self) -> usize {
            self.with_view(|v| layout::list_requests(v).unwrap().len())
        }

        fn resolve(&self, name: &str) -> herdr_graph::model::effective::EffectiveSeatConfig {
            self.with_view(|v| {
                herdr_graph::model::effective::resolve_in(v, &self.seat(name)).unwrap()
            })
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

    fn agent(kind: &str, session: AgentSession) -> Option<AgentInfo> {
        Some(AgentInfo {
            kind: kind.into(),
            status: AgentStatus::Idle,
            session: Some(session),
        })
    }

    fn write_transcript(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, "{\"n\":1}\n{\"n\":2}\n").unwrap();
        p
    }

    fn sorted(mut env: Vec<(String, String)>) -> Vec<(String, String)> {
        env.sort();
        env
    }

    // -----------------------------------------------------------------------------------------

    /// Every `RequestKind` has a mutation the writer can run: an exact key, or (for the kinds that select a
    /// handler by `sub`) at least one `<kind>.<sub>` key. The match has no wildcard, so a new variant fails to
    /// compile here until it is listed.
    #[test]
    fn sweep_every_request_kind_has_a_mutation() {
        use herdr_graph::daemon::compose::mutation_registry;
        use herdr_graph::model::change::RequestKind as K;
        use herdr_graph::plan::store::PlanStore;
        use herdr_graph::writer::mutation::kind_name;

        let dir = tempfile::tempdir().unwrap();
        let (_kinds, muts) = mutation_registry(Arc::new(PlanStore::new(dir.path().join("plans"))));
        let keys = muts.keys();
        let all = [
            K::TeamspaceCreate,
            K::TeamspaceRename,
            K::TeamspaceRetire,
            K::TeamspaceResurrect,
            K::SeatCreate,
            K::SeatActivate,
            K::SeatDeactivate,
            K::SeatRename,
            K::SeatRetire,
            K::SeatResurrect,
            K::SeatOverride,
            K::CloneAdd,
            K::CloneRetire,
            K::CloneRebind,
            K::ParticipationJoin,
            K::ParticipationLeave,
            K::TemplateCreate,
            K::TemplateEdit,
            K::TemplateCopy,
            K::ApplicationApply,
            K::ApplicationRetire,
            K::Undo,
            K::ContentWrite,
            K::Observed,
            K::Bookkeeping,
        ];
        for k in all {
            // Exhaustiveness guard: adding a variant breaks this match.
            match k {
                K::TeamspaceCreate
                | K::TeamspaceRename
                | K::TeamspaceRetire
                | K::TeamspaceResurrect => {}
                K::SeatCreate
                | K::SeatActivate
                | K::SeatDeactivate
                | K::SeatRename
                | K::SeatRetire
                | K::SeatResurrect
                | K::SeatOverride => {}
                K::CloneAdd
                | K::CloneRetire
                | K::CloneRebind
                | K::ParticipationJoin
                | K::ParticipationLeave => {}
                K::TemplateCreate
                | K::TemplateEdit
                | K::TemplateCopy
                | K::ApplicationApply
                | K::ApplicationRetire
                | K::Undo => {}
                K::ContentWrite | K::Observed | K::Bookkeeping => {}
            }
            let name = kind_name(k);
            let has_sub = matches!(k, K::Observed | K::Bookkeeping);
            let ok = if has_sub {
                keys.iter().any(|key| key.starts_with(&format!("{name}.")))
            } else {
                keys.contains(&name)
            };
            assert!(
                ok,
                "RequestKind {name} has no registered mutation; keys: {keys:?}"
            );
        }
    }

    // -----------------------------------------------------------------------------------------

    /// summaries default from the role: a summarizer member's own session end creates no request, an
    /// ordinary seat's does.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sweep_summaries_role_defaults_flow_end_to_end() {
        let (dir, root) = new_instance();
        let d = Daemon::start(&root).await;
        d.committed(&["teamspace", "create", "alpha", "--active"])
            .await;
        d.committed(&[
            "seat",
            "create",
            "worker",
            "--teamspace",
            "alpha",
            "--active",
            "--harness",
            "claude",
        ])
        .await;
        d.committed(&[
            "seat",
            "create",
            "sum",
            "--teamspace",
            "alpha",
            "--active",
            "--harness",
            "claude",
            "--role",
            "summarizer",
        ])
        .await;
        eventually("both panes bound", || {
            d.pane("worker").is_some() && d.pane("sum").is_some()
        })
        .await;
        let (worker_pane, sum_pane) = (d.pane("worker").unwrap(), d.pane("sum").unwrap());

        // Effective config: the role decides the default, no override is stored.
        assert!(
            !d.resolve("sum").summaries,
            "role summarizer: summaries default false"
        );
        assert!(
            d.resolve("worker").summaries,
            "ordinary seat: summaries default true"
        );
        assert_eq!(d.seat("sum").overrides.summaries, None);

        // The summarizer's session runs and ends: nothing is requested.
        let herdr = &d.fakes.herdr;
        herdr.set_agent(
            &sum_pane,
            agent(
                "claude",
                AgentSession::Path(write_transcript(dir.path(), "sum.jsonl")),
            ),
        );
        eventually("summarizer occupant", || {
            d.clone_of("sum").occupant.is_some()
        })
        .await;
        herdr.set_agent(&sum_pane, None);
        eventually("summarizer occupant to clear", || {
            d.clone_of("sum").occupant.is_none()
        })
        .await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(
            d.request_count(),
            0,
            "the summarizer's own session end is not summarized"
        );

        // The ordinary seat's session ends: exactly one request, for its transcript.
        herdr.set_agent(
            &worker_pane,
            agent(
                "claude",
                AgentSession::Path(write_transcript(dir.path(), "worker.jsonl")),
            ),
        );
        eventually("worker occupant", || {
            d.clone_of("worker").occupant.is_some()
        })
        .await;
        herdr.set_agent(&worker_pane, None);
        eventually("the worker's request", || d.request_count() == 1).await;
        let worker = d.seat("worker").id;
        let owners: Vec<_> = d.with_view(|v| {
            layout::list_transcripts(v)
                .unwrap()
                .into_iter()
                .map(|(_, t)| t.seat)
                .collect()
        });
        assert_eq!(
            owners,
            vec![worker],
            "the one transcript belongs to the worker"
        );
        d.stop().await;
    }

    /// Every create call carries the launch env of its level: a workspace two values, a tab and a pane the
    /// four ids of the clone they create.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sweep_launch_env_reaches_create_calls() {
        let (_dir, root) = new_instance();
        let d = Daemon::start(&root).await;
        d.committed(&["teamspace", "create", "t", "--active"]).await;
        d.committed(&[
            "seat",
            "create",
            "foreman",
            "--teamspace",
            "t",
            "--active",
            "--harness",
            "shell",
        ])
        .await;
        eventually("the first pane bound", || d.pane("foreman").is_some()).await;
        d.committed(&["clone", "add", "foreman"]).await;
        eventually("two bound clones", || {
            d.clones_of("foreman").len() == 2
                && d.clones_of("foreman")
                    .iter()
                    .all(|c| c.runtime.bound.is_some())
        })
        .await;

        let seat = d.seat("foreman");
        let clones = d.clones_of("foreman");
        let instance = root.display().to_string();
        let four = |clone: &CloneRecord| {
            sorted(vec![
                (ENV_CLONE.to_owned(), clone.id.to_string()),
                (ENV_GRAPH.to_owned(), "1".to_owned()),
                (ENV_INSTANCE.to_owned(), instance.clone()),
                (ENV_SEAT.to_owned(), seat.id.to_string()),
            ])
        };
        let calls = d.fakes.herdr.calls();
        let ws = calls
            .iter()
            .find_map(|c| {
                if let FakeCall::CreateWorkspace(w) = c {
                    Some(w.clone())
                } else {
                    None
                }
            })
            .expect("CreateWorkspace");
        assert_eq!(
            sorted(ws.env),
            sorted(vec![
                (ENV_GRAPH.into(), "1".into()),
                (ENV_INSTANCE.into(), instance.clone())
            ]),
            "a workspace knows no seat"
        );
        let tab = calls
            .iter()
            .find_map(|c| {
                if let FakeCall::CreateTab(t) = c {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .expect("CreateTab");
        let split = calls
            .iter()
            .find_map(|c| {
                if let FakeCall::SplitPane(s) = c {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .expect("SplitPane");
        let owner_of = |env: Vec<(String, String)>, what: &str| {
            let env = sorted(env);
            clones
                .iter()
                .find(|c| four(c) == env)
                .unwrap_or_else(|| {
                    panic!("{what} env {env:?} is not the four ids of a clone of foreman")
                })
                .clone()
        };
        let (first, second) = (owner_of(tab.env, "tab"), owner_of(split.env, "split"));
        assert_ne!(
            first.id, second.id,
            "the tab and the split create different clones"
        );
        // What the pane actually received is what the clone's binding says.
        for c in [&first, &second] {
            let pane = c
                .runtime
                .bound
                .as_ref()
                .and_then(|b| b.pane_id.clone())
                .unwrap();
            assert_eq!(
                sorted(d.fakes.herdr.pane_env(&pane).expect("pane env")),
                four(c)
            );
        }
        d.stop().await;
    }

    /// Launch args come from the harness profile: at the first start, and with a resume id when the seat is
    /// reactivated after a session ended.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sweep_harness_profiles_used_for_start_and_resume() {
        let (_dir, root) = new_instance();
        let d = Daemon::start(&root).await;
        d.committed(&["teamspace", "create", "t", "--active"]).await;
        d.committed(&[
            "seat",
            "create",
            "cl",
            "--teamspace",
            "t",
            "--active",
            "--harness",
            "claude",
            "--model",
            "sonnet",
        ])
        .await;
        d.committed(&[
            "seat",
            "create",
            "cx",
            "--teamspace",
            "t",
            "--active",
            "--harness",
            "codex",
            "--model",
            "gpt-x",
        ])
        .await;
        let started = || -> Vec<(String, Vec<String>)> {
            d.fakes
                .herdr
                .calls()
                .into_iter()
                .filter_map(|c| {
                    if let FakeCall::StartAgent(s) = c {
                        Some((s.kind, s.args))
                    } else {
                        None
                    }
                })
                .collect()
        };
        eventually("both agents started", || started().len() == 2).await;
        let all = started();
        let find = |kind: &str| {
            all.iter()
                .find(|(k, _)| k == kind)
                .cloned()
                .unwrap_or_else(|| panic!("no {kind} start in {all:?}"))
        };
        assert_eq!(
            find("claude").1,
            profile(Harness::Claude).argv(Some("sonnet"), None, &[])
        );
        assert_eq!(
            find("codex").1,
            profile(Harness::Codex).argv(Some("gpt-x"), None, &[])
        );
        assert_eq!(
            find("codex").1,
            ["--no-daemon", "-m", "gpt-x"],
            "the codex profile's launch args and model flag"
        );
        assert_eq!(find("claude").1, ["--model", "sonnet"]);

        // A session ends, the seat is deactivated and activated again: the new start resumes that session.
        eventually("the claude pane bound", || d.pane("cl").is_some()).await;
        let pane = d.pane("cl").unwrap();
        d.fakes
            .herdr
            .set_agent(&pane, agent("claude", AgentSession::Id("native-1".into())));
        eventually("occupant", || d.clone_of("cl").occupant.is_some()).await;
        d.fakes.herdr.set_agent(&pane, None);
        eventually("the session to end", || {
            d.clone_of("cl").occupant.is_none()
                && d.clone_of("cl").sessions.iter().any(|s| s.ended.is_some())
        })
        .await;
        d.committed(&["seat", "deactivate", "cl"]).await;
        eventually("the tab to close", || {
            d.fakes
                .herdr
                .calls()
                .iter()
                .any(|c| matches!(c, FakeCall::CloseTab(_)))
        })
        .await;
        d.committed(&["seat", "activate", "cl"]).await;
        eventually("a resumed start", || {
            started().iter().filter(|(k, _)| k == "claude").count() == 2
        })
        .await;
        let resumed = started()
            .into_iter()
            .filter(|(k, _)| k == "claude")
            .nth(1)
            .unwrap();
        assert_eq!(
            resumed.1,
            profile(Harness::Claude).argv(Some("sonnet"), Some("native-1"), &[])
        );
        assert_eq!(resumed.1, ["--resume", "native-1", "--model", "sonnet"]);
        d.stop().await;
    }

    /// Records written by the daemon parse as their envelopes: every action file as `ActionRecord`, every
    /// journal effect as `EffectRecord` (and round-trips JSON), and `undo.list` offers every action.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sweep_action_and_effect_envelopes_round_trip() {
        let (_dir, root) = new_instance();
        let d = Daemon::start(&root).await;
        d.committed(&["teamspace", "create", "t", "--active"]).await;
        d.committed(&[
            "seat",
            "create",
            "foreman",
            "--teamspace",
            "t",
            "--active",
            "--harness",
            "shell",
        ])
        .await;
        d.committed(&[
            "seat",
            "create",
            "scribe",
            "--teamspace",
            "t",
            "--active",
            "--harness",
            "shell",
            "--role",
            "summarizer",
        ])
        .await;
        eventually("both panes bound", || {
            d.pane("foreman").is_some() && d.pane("scribe").is_some()
        })
        .await;
        // A closure cascade (a user closes the tab) and a seat retire: two kinds of act. The close is a user
        // action on a settled Herdr, so wait until the creation effects (tokens, names) are all done.
        let settled = || {
            let j = Journal::open(&InstancePaths::new(&root).journal).unwrap();
            j.effects_with_status(&[EffectStatus::Pending, EffectStatus::Unknown])
                .unwrap()
                .is_empty()
        };
        eventually("the creation effects to settle", settled).await;
        let pane = d.pane("foreman").unwrap();
        let snap = d.fakes.herdr.snapshot().await.unwrap();
        let tab = snap
            .workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .find(|t| t.panes.iter().any(|p| p.id == pane))
            .expect("the foreman tab")
            .id
            .clone();
        d.fakes.herdr.user_close_tab(&tab);
        // Events are hints: one emitted while the observer is between subscriptions is only recovered by the
        // next periodic snapshot diff (60 s). Dropping the stream makes the observer resync now.
        d.fakes.herdr.disconnect_subscribers();
        eventually("the foreman seat to retire", || {
            d.seat("foreman").lifecycle == herdr_graph::model::Lifecycle::Retired
        })
        .await;
        d.committed(&["seat", "retire", "scribe"]).await;
        // Retiring the seat's last clone closes its pane (and with it the tab).
        eventually("scribe's pane to be closed by the reconciler", || {
            d.fakes
                .herdr
                .calls()
                .iter()
                .any(|c| matches!(c, FakeCall::ClosePane(_)))
        })
        .await;
        eventually("the effects to settle", settled).await;

        // Action files on disk parse as ActionRecord.
        let mut acts = Vec::new();
        for month in std::fs::read_dir(root.join("actions"))
            .expect("actions dir")
            .flatten()
        {
            for f in std::fs::read_dir(month.path()).unwrap().flatten() {
                let text = std::fs::read_to_string(f.path()).unwrap();
                let a: ActionRecord = toml::from_str(&text)
                    .unwrap_or_else(|e| panic!("{}: {e}\n{text}", f.path().display()));
                assert_eq!(
                    f.path().file_stem().unwrap().to_str().unwrap(),
                    a.id.to_string()
                );
                acts.push(a);
            }
        }
        assert!(acts.len() >= 2, "a cascade and a retire at least: {acts:?}");
        let again: Vec<ActionRecord> = acts
            .iter()
            .map(|a| toml::from_str(&toml::to_string(a).unwrap()).unwrap())
            .collect();
        assert_eq!(acts, again, "ActionRecord round-trips TOML");

        // Every journal effect row round-trips.
        let j = Journal::open(&InstancePaths::new(&root).journal).unwrap();
        let all = [
            EffectStatus::Pending,
            EffectStatus::Done,
            EffectStatus::Obsolete,
            EffectStatus::Failed,
            EffectStatus::Unknown,
            EffectStatus::NeedsRevision,
            EffectStatus::BlockedNeedsHuman,
        ];
        let effects = j.effects_with_status(&all).unwrap();
        assert!(
            effects.iter().any(|e| e.kind.as_str() == "create_tab")
                && effects.iter().any(|e| e.kind.as_str() == "close_pane"),
            "{effects:?}"
        );
        for e in &effects {
            let back: EffectRecord =
                serde_json::from_value(serde_json::to_value(e).unwrap()).unwrap();
            assert_eq!(&back, e);
        }

        // undo --json lists every act.
        let listed = d.call("undo.list", json!({ "limit": 100 })).await.unwrap();
        let ids: Vec<&str> = listed["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["act"].as_str())
            .collect();
        for a in &acts {
            assert!(
                ids.contains(&a.id.to_string().as_str()),
                "undo list misses {}: {ids:?}",
                a.id
            );
        }
        d.stop().await;
    }

    // -----------------------------------------------------------------------------------------
    // delivery capability switch
    // -----------------------------------------------------------------------------------------

    /// With `threads-service-ack` and a service that reports ServiceAck, a request goes through
    /// `send_request` to the summarizer's mapped threads seat; without the feature the same service is used
    /// through the Notify fallback only.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sweep_capability_switch() {
        let (dir, root) = new_instance();
        let fakes = Fakes::new();
        let ack = Arc::new(AckThreads {
            inner: fakes.threads.clone(),
            capability: DeliveryCapability::ServiceAck,
            sent: Default::default(),
        });
        let d = Daemon::start_with(&root, fakes, ack.clone()).await;
        d.committed(&["teamspace", "create", "alpha", "--active"])
            .await;
        d.committed(&[
            "seat",
            "create",
            "worker",
            "--teamspace",
            "alpha",
            "--active",
            "--harness",
            "claude",
        ])
        .await;
        d.committed(&[
            "seat",
            "create",
            "sum",
            "--teamspace",
            "alpha",
            "--active",
            "--harness",
            "claude",
            "--role",
            "summarizer",
        ])
        .await;
        eventually("both panes bound", || {
            d.pane("worker").is_some() && d.pane("sum").is_some()
        })
        .await;
        eventually("the summarizer channel", || {
            d.seat("sum").channel.thread_id.is_some()
        })
        .await;
        let (worker_pane, sum_pane) = (d.pane("worker").unwrap(), d.pane("sum").unwrap());
        d.fakes.panes.set(&sum_pane, "threads-seat-sum");
        d.fakes.herdr.set_agent(
            &sum_pane,
            agent("claude", AgentSession::Id("sum-session".into())),
        );
        eventually("summarizer occupant", || {
            d.clone_of("sum").occupant.is_some()
        })
        .await;
        d.fakes.herdr.set_agent(
            &worker_pane,
            agent(
                "claude",
                AgentSession::Path(write_transcript(dir.path(), "w.jsonl")),
            ),
        );
        eventually("worker occupant", || {
            d.clone_of("worker").occupant.is_some()
        })
        .await;
        d.fakes.herdr.set_agent(&worker_pane, None);

        let thread = d.seat("sum").channel.thread_id.clone().unwrap();
        let notified = || {
            d.fakes
                .threads
                .notifications()
                .iter()
                .any(|n| n.thread.0 == thread && n.body.contains("rq_"))
        };
        if cfg!(feature = "threads-service-ack") {
            eventually("a send_request delivery", || {
                !ack.sent.lock().unwrap().is_empty()
            })
            .await;
            let sent = ack.sent.lock().unwrap().clone();
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0].0.0, thread);
            assert_eq!(
                sent[0].1,
                vec![ThreadsSeatRef("threads-seat-sum".into())],
                "addressed to the summarizer's threads seat"
            );
            assert!(sent[0].2.contains("rq_"), "{}", sent[0].2);
            assert!(!notified(), "the ACK path does not also notify");
        } else {
            eventually("a Notify delivery", notified).await;
            assert!(
                ack.sent.lock().unwrap().is_empty(),
                "without the feature the service-ack path is never taken"
            );
        }
        d.stop().await;
    }

    // -----------------------------------------------------------------------------------------
    // graph.toml
    // -----------------------------------------------------------------------------------------

    /// `graph.toml` values change behaviour: `defaults.harness/model` decide the launch of a seat that sets
    /// neither, and `summarizer_seat` is the destination when no seat has the summarizer role.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sweep_config_values_are_read() {
        let (dir, root) = new_instance();
        let fakes = Fakes::new();
        let threads = fakes.threads.clone();
        let d = Daemon::start_with(&root, fakes.clone(), threads.clone()).await;
        d.committed(&["teamspace", "create", "t", "--active"]).await;
        d.committed(&[
            "seat",
            "create",
            "boss",
            "--teamspace",
            "t",
            "--active",
            "--harness",
            "claude",
        ])
        .await;
        d.committed(&[
            "seat",
            "create",
            "worker",
            "--teamspace",
            "t",
            "--active",
            "--harness",
            "claude",
        ])
        .await;
        eventually("both panes bound", || {
            d.pane("boss").is_some() && d.pane("worker").is_some()
        })
        .await;
        eventually("boss channel", || {
            d.seat("boss").channel.thread_id.is_some()
        })
        .await;
        let (boss_pane, worker_pane) = (d.pane("boss").unwrap(), d.pane("worker").unwrap());
        d.fakes.herdr.set_agent(
            &boss_pane,
            agent("claude", AgentSession::Id("boss-session".into())),
        );
        eventually("boss occupant", || d.clone_of("boss").occupant.is_some()).await;
        d.fakes.herdr.set_agent(
            &worker_pane,
            agent(
                "claude",
                AgentSession::Path(write_transcript(dir.path(), "w.jsonl")),
            ),
        );
        eventually("worker occupant", || {
            d.clone_of("worker").occupant.is_some()
        })
        .await;
        let boss = d.seat("boss");
        let boss_thread = boss.channel.thread_id.clone().unwrap();
        d.stop().await;

        // Edit graph.toml by hand and commit it (the file has no command).
        let path = root.join("graph.toml");
        let mut doc: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        doc.insert(
            "summarizer_seat".into(),
            toml::Value::String(boss.id.to_string()),
        );
        let defaults = doc
            .entry("defaults")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        defaults
            .as_table_mut()
            .unwrap()
            .insert("harness".into(), "codex".into());
        defaults
            .as_table_mut()
            .unwrap()
            .insert("model".into(), "m-from-graph".into());
        std::fs::write(&path, toml::to_string(&doc).unwrap()).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["add", "graph.toml"]);
        git(&[
            "-c",
            "user.name=sweep",
            "-c",
            "user.email=sweep@example.invalid",
            "commit",
            "-q",
            "-m",
            "config: graph defaults",
        ]);

        let d = Daemon::start_with(&root, fakes, threads).await;
        // A seat that sets neither harness nor model launches what graph.toml says.
        d.committed(&["seat", "create", "plain", "--teamspace", "t", "--active"])
            .await;
        let cfg = d.resolve("plain");
        assert_eq!(
            (cfg.harness, cfg.model.as_deref()),
            (Harness::Codex, Some("m-from-graph"))
        );
        assert_eq!(
            d.resolve("worker").harness,
            Harness::Claude,
            "a seat override beats graph.toml"
        );
        eventually("codex started with the graph defaults", || {
            d.fakes.herdr.calls().iter().any(|c| matches!(c, FakeCall::StartAgent(s) if s.kind == "codex" && s.args == ["--no-daemon", "-m", "m-from-graph"]))
        })
        .await;

        // No seat has the summarizer role: the request goes to the configured `summarizer_seat`.
        d.fakes.herdr.set_agent(&worker_pane, None);
        eventually("a delivery to boss's channel", || {
            d.fakes
                .threads
                .notifications()
                .iter()
                .any(|n| n.thread.0 == boss_thread && n.body.contains("rq_"))
        })
        .await;
        assert_eq!(d.request_count(), 1);
        d.stop().await;
    }
}

// =============================================================================================
// every §10 command reaches a handler (feature test-support: the binary with fake services)
// =============================================================================================

#[cfg(feature = "test-support")]
mod cli_sweep {
    use serde_json::Value;
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::process::{Command, Output};

    struct Fixture {
        /// Dropped first: shuts the daemon down and sweeps every process carrying its marker.
        root: super::support::isolated::TestRoot,
        instance: PathBuf,
        _listener: UnixListener,
    }

    impl Fixture {
        fn new() -> Self {
            let root = super::support::isolated::TestRoot::new();
            let listener = UnixListener::bind(root.herdr_socket()).unwrap();
            let l2 = listener.try_clone().unwrap();
            std::thread::spawn(move || for _conn in l2.incoming() {});
            Fixture {
                instance: root.instance(),
                root,
                _listener: listener,
            }
        }

        /// The binary with the root's scrubbed env: HOME and CLAUDE_CONFIG_DIR are inside the root, so `init`
        /// and `setup claude` write there.
        fn cmd(&self, args: &[&str]) -> Command {
            let mut c = self.root.command(env!("CARGO_BIN_EXE_herdr-graph"));
            c.args(args).env("HG_TEST_FAKE_SERVICES", "1");
            c
        }

        fn run(&self, args: &[&str]) -> Output {
            self.cmd(args).output().unwrap()
        }
    }

    fn text(o: &Output) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    }

    #[test]
    fn sweep_every_cli_command_reaches_a_handler() {
        let f = Fixture::new();
        let out = f.run(&["init", f.instance.to_str().unwrap(), "--with-examples"]);
        assert!(out.status.success(), "init: {}", text(&out));
        let out = f.run(&["daemon", "--ensure"]);
        assert!(out.status.success(), "daemon --ensure: {}", text(&out));

        // Objects for the commands that take ids.
        let relay = |words: &[&str]| {
            let mut args = vec!["plan", "--json"];
            args.extend_from_slice(words);
            let o = f.run(&args);
            assert!(o.status.success(), "plan {words:?}: {}", text(&o));
            let plan: Value = serde_json::from_slice(&o.stdout).expect("plan --json prints JSON");
            let o = f.run(&[
                "apply",
                plan["plan_id"].as_str().unwrap(),
                "--confirm",
                plan["hash"].as_str().unwrap(),
                "--confirmed-by",
                "user-relay",
            ]);
            assert!(o.status.success(), "apply {words:?}: {}", text(&o));
        };
        relay(&["teamspace", "create", "t", "--active"]);
        relay(&[
            "seat",
            "create",
            "foreman",
            "--teamspace",
            "t",
            "--active",
            "--harness",
            "shell",
        ]);
        let first_col = |args: &[&str]| -> String {
            let o = f.run(args);
            text(&o)
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned()
        };
        let seat = first_col(&["list", "seats"]);
        let clone = first_col(&["list", "clones"]);
        let op = first_col(&["ops"]);
        assert!(
            seat.starts_with("st_") && clone.starts_with("cl_") && op.starts_with("op_"),
            "{seat} {clone} {op}"
        );

        let body = f.root.path().join("body.md");
        std::fs::write(&body, "notes\n").unwrap();
        let body = body.to_str().unwrap().to_owned();
        let payload_free = ["session-report", "--from-hook", "claude"];

        // The §10 surface (the same list as `every_spec_command_parses`, with real ids).
        let commands: Vec<Vec<&str>> = vec![
            vec!["status"],
            vec!["doctor"],
            vec!["setup", "claude"],
            vec!["setup", "claude", "--uninstall"],
            vec!["seat", "--hook-prompt"],
            vec!["seat"],
            vec!["seat", "--json"],
            vec!["show", &seat],
            vec!["list"],
            vec!["list", "seats"],
            vec!["list", "templates"],
            vec!["path", &seat],
            vec!["plan", "--json", "seat", "rename", "foreman", "boss"],
            vec!["apply", "pl_does_not_exist"],
            vec![
                "content",
                "write",
                "--object",
                &seat,
                "--rel",
                "notes/a.md",
                "--from",
                &body,
            ],
            payload_free.to_vec(),
            vec!["rebind", &clone, "--pane", "p-none"],
            vec!["ops"],
            vec!["ops", "--unresolved"],
            vec!["op", &op],
            vec!["cancel", &op],
            vec!["reassign", &op, "--to", &seat],
            vec!["check-instruction", &op, &seat, "1"],
            vec!["undo", "--list"],
            vec!["undo", "--json"],
            vec!["request", "list"],
            vec!["request", "list", "--pending"],
            vec!["request", "list", "--unresolved"],
            vec!["request", "list", "--undispatched"],
            vec!["request", "ack", "rq_does_not_exist"],
            vec![
                "request",
                "complete",
                "rq_does_not_exist",
                "--output",
                "o.md",
                "--covered",
                "0-1",
            ],
            vec!["who", "p-none"],
            vec!["who", &seat],
        ];
        let mut failures = Vec::new();
        for argv in &commands {
            let mut cmd = f.cmd(argv);
            cmd.stdin(std::process::Stdio::null());
            let o = cmd.output().unwrap();
            let t = text(&o);
            if t.contains("not implemented")
                || t.contains("panicked")
                || o.status.code() == Some(101)
            {
                failures.push(format!("{argv:?}: {t}"));
            }
        }
        assert!(
            failures.is_empty(),
            "commands without a handler or that panic:\n{}",
            failures.join("\n")
        );

        // The reads that must work on a healthy instance, with their content.
        let status = text(&f.run(&["status"]));
        assert!(status.contains("daemon: running"), "{status}");
        let listed = text(&f.run(&["list", "seats"]));
        assert!(listed.contains("foreman"), "{listed}");
        assert!(f.run(&["ops"]).status.success());
        assert!(f.run(&["undo", "--json"]).status.success());
        assert!(f.run(&["request", "list", "--pending"]).status.success());
    }
}
