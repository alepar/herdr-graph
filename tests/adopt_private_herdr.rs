//! Tier-3 `teamspace adopt` against a PRIVATE Herdr server: the real daemon adopts a workspace the graph did
//! not create, and the planner leaves the tabs in it alone. Run with:
//!
//!   cargo test --features private-herdr --test adopt_private_herdr -- --test-threads=1
//!
//! The rig is a fresh private server and instance; `Rig` reaps the daemon and every process under its root
//! on success, failure and panic (`PrivateHerdr` stops the server).
#![cfg(feature = "private-herdr")]

mod support;

use herdr_graph::daemon::lock;
use herdr_graph::ports::herdr::{CreateTab, CreateWorkspace, HerdrApi};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use support::private_herdr::PrivateHerdr;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");
const WAIT: Duration = Duration::from_secs(40);

struct Rig {
    herdr: PrivateHerdr,
    instance: PathBuf,
    rt: tokio::runtime::Runtime,
}

impl Rig {
    fn start() -> Option<Rig> {
        Self::start_with(false)
    }

    /// A rig whose graph daemon reaches a real herdr-threads daemon (state under `<root>/threads-state`, found
    /// through `HERDR_GRAPH_THREADS_STATE_DIR`). Drop reaps it with everything else under the root. A `claude`
    /// stub that exits makes any graph start attempt harmless.
    fn start_threads() -> Option<Rig> {
        Self::start_with(true)
    }

    fn start_with(threads: bool) -> Option<Rig> {
        if threads {
            support::threads_bin::threads_binary();
        }
        let started = if threads {
            PrivateHerdr::start_with_agent_stubs(&[("claude", "#!/bin/sh\nexit 0\n")])
        } else {
            PrivateHerdr::start()
        };
        let herdr = match started {
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
        let mut rig = Rig {
            herdr,
            instance,
            rt,
        };
        if threads {
            let state = rig.threads_state();
            rig.herdr
                .guard()
                .check(&state)
                .expect("threads state is inside the private root");
            rig.herdr.env.push((
                "HERDR_GRAPH_THREADS_STATE_DIR".into(),
                state.display().to_string(),
            ));
        }
        let out = rig.cli(&["init", rig.instance.to_str().unwrap()]);
        assert!(out.status.success(), "init: {}", stderr(&out));
        support::node_manager_opt_out(&rig.instance);
        if threads {
            let out = rig.threads(&["daemon", "ensure"], &[]);
            assert!(
                out.status.success(),
                "threads daemon ensure: {}",
                stderr(&out)
            );
        }
        let out = rig.cli(&["daemon", "--ensure"]);
        assert!(out.status.success(), "daemon --ensure: {}", stderr(&out));
        Some(rig)
    }

    /// SIGKILL this rig's graph daemon (its pid from the lock file, argv-checked) and wait until it is gone.
    fn kill_daemon(&self) {
        let pid = lock::read_info(&self.instance.join(".graph-local/daemon.lock"))
            .expect("daemon lock info")
            .pid;
        let ps = Command::new("/bin/ps")
            .args(["-o", "command=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let cmdline = String::from_utf8_lossy(&ps.stdout).into_owned();
        assert!(
            cmdline.contains(BIN) && cmdline.contains(" daemon"),
            "pid {pid} is not our daemon: {cmdline}"
        );
        // SAFETY: the pid was verified above to be this rig's daemon.
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGKILL) }, 0);
        let deadline = Instant::now() + WAIT;
        // SAFETY: signal 0 only probes existence.
        while unsafe { libc::kill(pid as i32, 0) } == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn threads_state(&self) -> PathBuf {
        self.herdr.root.join("threads-state")
    }

    /// The pinned `herdr-threads` against the rig's state dir and private Herdr, with `env` added.
    fn threads(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let bin = support::threads_bin::threads_binary();
        self.herdr
            .command(&bin)
            .arg("--state-dir")
            .arg(self.threads_state())
            .arg("--host-endpoint")
            .arg(&self.herdr.socket)
            .args(args)
            .envs(env.iter().copied())
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    }

    fn threads_json(&self, args: &[&str], env: &[(&str, &str)]) -> Value {
        let mut a = vec!["--json"];
        a.extend_from_slice(args);
        let out = self.threads(&a, env);
        assert!(
            out.status.success(),
            "threads {args:?}: {}{}",
            stderr(&out),
            stdout(&out)
        );
        serde_json::from_str(&stdout(&out))
            .unwrap_or_else(|e| panic!("threads {args:?}: {e}: {}", stdout(&out)))
    }

    /// Every seat id the rig's threads daemon knows, retired ones included, sorted.
    fn threads_seat_ids(&self) -> Vec<String> {
        let v = self.threads_json(&["seat", "list", "--include-retired"], &[]);
        let mut ids: Vec<String> = v["result"]["data"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("no seat items: {v}"))
            .iter()
            .map(|s| s["seat"].as_str().expect("seat id").to_owned())
            .collect();
        ids.sort();
        ids
    }

    /// A keyless native check-in in `pane`: the public `herdr-threads hook claude` with a synthetic SessionStart
    /// payload (the rig has no agent binary or credentials), and no continuity key in the environment.
    fn keyless_check_in(&self, pane: &str, session: &str) {
        use std::os::unix::fs::PermissionsExt;
        let fixtures = self.herdr.root.join("fixture-bin");
        std::fs::create_dir_all(&fixtures).unwrap();
        let claude = fixtures.join("claude");
        std::fs::write(&claude, "#!/bin/sh\necho '2.1.283 (Claude Code)'\n").unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let transcript = self.herdr.root.join("fixture-session.jsonl");
        std::fs::write(&transcript, "").unwrap();
        let payload = self.herdr.root.join("fixture-hook.json");
        std::fs::write(
            &payload,
            serde_json::to_vec(&json!({
                "hook_event_name": "SessionStart", "session_id": session,
                "transcript_path": transcript, "cwd": self.herdr.root.join("home"), "source": "startup"
            }))
            .unwrap(),
        )
        .unwrap();
        let path = self
            .herdr
            .env
            .iter()
            .find(|(k, _)| k == "PATH")
            .expect("PATH in the private env")
            .1
            .clone();
        let bin = support::threads_bin::threads_binary();
        let out = self
            .herdr
            .command(&bin)
            .arg("--state-dir")
            .arg(self.threads_state())
            .arg("--host-endpoint")
            .arg(&self.herdr.socket)
            .args(["hook", "claude"])
            .env("CLAUDE_PID", std::process::id().to_string())
            .env("CLAUDE_CODE_SESSION_ID", session)
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env("HERDR_ENV", "1")
            .env("HERDR_PANE_ID", pane)
            .env("HERDR_SOCKET_PATH", &self.herdr.socket)
            .env("PATH", format!("{}:{path}", fixtures.display()))
            .stdin(std::process::Stdio::from(
                std::fs::File::open(payload).unwrap(),
            ))
            .output()
            .unwrap();
        assert!(out.status.success(), "check-in: {}", stderr(&out));
    }

    /// `(id, show TOML)` of every clone the graph records.
    fn clone_records(&self) -> Vec<(String, toml::Table)> {
        self.cli_ok(&["list", "clones"])
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let id = l.split("  ").next().unwrap().to_owned();
                let text = self.cli_ok(&["show", &id]);
                let rec: toml::Table = text
                    .parse()
                    .unwrap_or_else(|e| panic!("show {id} is not toml ({e}): {text}"));
                (id, rec)
            })
            .collect()
    }

    fn cli(&self, args: &[&str]) -> Output {
        let mut c: Command = self.herdr.command(std::path::Path::new(BIN));
        c.env("HERDR_GRAPH_INSTANCE", &self.instance);
        c.args(args).output().unwrap()
    }

    fn cli_ok(&self, args: &[&str]) -> String {
        let o = self.cli(args);
        assert!(o.status.success(), "{args:?}: {}{}", stderr(&o), stdout(&o));
        stdout(&o)
    }

    fn plan_apply(&self, words: &[&str]) {
        let mut args = vec!["plan", "--json"];
        args.extend_from_slice(words);
        let plan: Value = serde_json::from_str(&self.cli_ok(&args)).unwrap();
        self.cli_ok(&[
            "apply",
            plan["plan_id"].as_str().unwrap(),
            "--confirm",
            plan["hash"].as_str().unwrap(),
            "--confirmed-by",
            "user-relay",
        ]);
    }

    fn snapshot(&self) -> herdr_graph::ports::herdr::HerdrSnapshot {
        self.rt
            .block_on(self.herdr.client().snapshot())
            .expect("snapshot")
    }

    fn wait_until(&self, desc: &str, mut pred: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if pred(self) {
                return;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        let log = std::fs::read_to_string(self.instance.join(".graph-local/daemon.log"))
            .unwrap_or_default();
        panic!("timed out waiting for {desc}\ndaemon log:\n{log}");
    }

    /// Wait until the daemon has stopped committing (HEAD unchanged for 1.5 s), so a later commit count
    /// measures only what the test itself did.
    fn settle(&self) {
        let mut last = (self.commit_count(), Instant::now());
        self.wait_until("the daemon to stop committing", |r| {
            let n = r.commit_count();
            if n != last.0 {
                last = (n, Instant::now());
                return false;
            }
            last.1.elapsed() >= Duration::from_millis(1500)
        });
    }

    fn commit_count(&self) -> usize {
        let repo = git2::Repository::open(&self.instance).unwrap();
        let mut walk = repo.revwalk().unwrap();
        walk.push_ref("refs/heads/main").unwrap();
        walk.count()
    }
}

impl Drop for Rig {
    /// The daemon is detached and would outlive the test: reap everything that inherited the root marker.
    fn drop(&mut self) {
        support::isolated::reap_root(&self.herdr.root);
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// `(tab id, label, pane ids)` of every tab in the workspace labeled `label`.
fn tabs_of(
    snap: &herdr_graph::ports::herdr::HerdrSnapshot,
    label: &str,
) -> Vec<(String, String, Vec<String>)> {
    let ws = snap
        .workspaces
        .iter()
        .find(|w| w.label == label)
        .unwrap_or_else(|| panic!("no workspace {label}"));
    ws.tabs
        .iter()
        .map(|t| {
            (
                t.id.0.clone(),
                t.label.clone(),
                t.panes.iter().map(|p| p.id.0.clone()).collect(),
            )
        })
        .collect()
}

#[test]
fn adopted_workspace_leaves_unmanaged_tabs_alone() {
    let Some(rig) = Rig::start() else { return };
    let cwd = rig.herdr.root.join("home");
    let client = rig.herdr.client();
    let ws = rig
        .rt
        .block_on(client.create_workspace(CreateWorkspace {
            label: "legacy".into(),
            cwd: cwd.clone(),
            env: vec![],
        }))
        .expect("create workspace");
    let ws_id = ws.workspace.unwrap();
    rig.rt
        .block_on(client.create_tab(CreateTab {
            workspace: ws_id.clone(),
            label: "notes".into(),
            cwd: cwd.clone(),
            env: vec![],
        }))
        .expect("create tab");
    let before = tabs_of(&rig.snapshot(), "legacy");
    assert_eq!(
        before.len(),
        2,
        "the workspace starts with two tabs: {before:?}"
    );

    let repo = rig.herdr.root.join("repo");
    rig.plan_apply(&[
        "teamspace",
        "adopt",
        "legacy",
        "--project-repo",
        repo.to_str().unwrap(),
    ]);
    rig.wait_until("the graph token on the adopted workspace", |r| {
        r.snapshot()
            .workspaces
            .iter()
            .find(|w| w.id == ws_id)
            .and_then(|w| w.metadata.get("hg"))
            .is_some_and(|v| v.starts_with("hg=ts_"))
    });

    rig.plan_apply(&[
        "seat",
        "create",
        "worker",
        "--teamspace",
        "legacy",
        "--active",
        "--harness",
        "shell",
    ]);
    rig.wait_until("the worker tab in the adopted workspace", |r| {
        tabs_of(&r.snapshot(), "legacy")
            .iter()
            .any(|t| t.1 == "worker")
    });
    rig.settle();
    rig.settle();

    let after = tabs_of(&rig.snapshot(), "legacy");
    for original in &before {
        assert!(
            after.contains(original),
            "unmanaged tab {original:?} changed or vanished; now {after:?}"
        );
    }
    let worker: Vec<_> = after.iter().filter(|t| t.1 == "worker").collect();
    assert_eq!(worker.len(), 1, "exactly one graph tab: {after:?}");
    assert!(
        !before.iter().any(|t| t.0 == worker[0].0),
        "the seat's tab is a new tab, not a reused one"
    );
    assert_eq!(after.len(), before.len() + 1, "{after:?}");
    assert!(
        rig.snapshot()
            .workspaces
            .iter()
            .any(|w| w.id == ws_id && w.label == "legacy")
    );
}

fn pane_count(snap: &herdr_graph::ports::herdr::HerdrSnapshot) -> usize {
    snap.workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .map(|t| t.panes.len())
        .sum()
}

fn tab_count(snap: &herdr_graph::ports::herdr::HerdrSnapshot) -> usize {
    snap.workspaces.iter().map(|w| w.tabs.len()).sum()
}

/// What `seat adopt` must keep: the clone, its threads seat and pane binding.
#[derive(Debug, PartialEq)]
struct Adopted {
    clone: String,
    threads_seat: String,
    pane: String,
    keyless: bool,
}

fn adopted_clone(rig: &Rig) -> Option<Adopted> {
    let recs = rig.clone_records();
    let (id, rec) = recs.first()?;
    Some(Adopted {
        clone: id.clone(),
        threads_seat: rec.get("threads_seat")?.as_str()?.to_owned(),
        pane: rec
            .get("runtime")?
            .get("bound")?
            .get("pane_id")?
            .as_str()?
            .to_owned(),
        keyless: rec.get("keyless")?.as_bool()?,
    })
}

#[test]
fn adopted_seat_keeps_its_threads_seat_and_memberships_across_a_daemon_restart() {
    let Some(rig) = Rig::start_threads() else {
        return;
    };
    let cwd = rig.herdr.root.join("home");
    let client = rig.herdr.client();
    let ws = rig
        .rt
        .block_on(client.create_workspace(CreateWorkspace {
            label: "legacy".into(),
            cwd: cwd.clone(),
            env: vec![],
        }))
        .expect("create workspace");
    let tab = rig
        .rt
        .block_on(client.create_tab(CreateTab {
            workspace: ws.workspace.unwrap(),
            label: "worker".into(),
            cwd,
            env: vec![],
        }))
        .expect("create tab");
    let pane = tab.pane.unwrap().0;

    // A checked-in agent with a membership graph does not own.
    rig.keyless_check_in(&pane, "adopt-session");
    let seats = rig.threads_seat_ids();
    assert_eq!(
        seats.len(),
        1,
        "one threads seat after the check-in: {seats:?}"
    );
    let seat = seats[0].clone();
    let me = [
        ("CLAUDE_PID", std::process::id().to_string()),
        ("CLAUDE_CODE_SESSION_ID", "adopt-session".to_owned()),
        ("HERDR_PANE_ID", pane.clone()),
    ];
    let env: Vec<(&str, &str)> = me.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let created = rig.threads_json(
        &[
            "--cooperative-seat",
            &seat,
            "--cooperative-target",
            &pane,
            "--cooperative-harness",
            "claude",
            "--cooperative-role",
            "top-level",
            "thread",
            "create",
            "--topic",
            "legacy chat",
        ],
        &env,
    );
    let thread = created.to_string();
    let thread_id = created["result"]["data"]
        .as_str()
        .unwrap_or_else(|| panic!("no thread id in {thread}"))
        .to_owned();
    let participates = |rig: &Rig| {
        rig.threads_json(&["thread", "participants", &thread_id], &[])
            .to_string()
            .contains(&seat)
    };
    assert!(participates(&rig), "the creating seat participates");

    rig.plan_apply(&["teamspace", "adopt", "legacy"]);
    rig.plan_apply(&["seat", "adopt", "worker", "--harness", "claude"]);
    let mut got = None;
    rig.wait_until(
        "the adopted clone bound to its threads seat and pane",
        |r| {
            got = adopted_clone(r);
            got.is_some()
        },
    );
    let first = got.unwrap();
    assert_eq!(
        first,
        Adopted {
            clone: first.clone.clone(),
            threads_seat: seat.clone(),
            pane: pane.clone(),
            keyless: true
        }
    );
    rig.settle();
    assert!(participates(&rig), "membership kept after adopt");
    let (tabs, panes) = (tab_count(&rig.snapshot()), pane_count(&rig.snapshot()));

    rig.kill_daemon();
    rig.cli_ok(&["daemon", "--ensure"]);
    rig.settle();
    rig.settle();
    assert_eq!(
        adopted_clone(&rig),
        Some(first),
        "same clone, threads seat and pane after restart"
    );
    assert!(participates(&rig), "membership kept after restart");
    assert_eq!(rig.threads_seat_ids(), vec![seat], "no new threads seat");
    let snap = rig.snapshot();
    assert_eq!(
        (tab_count(&snap), pane_count(&snap)),
        (tabs, panes),
        "nothing launched or closed"
    );
}

/// The tabs of the workspace labeled `label` with their pane ids, sorted: what a release must leave alone.
fn layout_of(rig: &Rig, label: &str) -> Vec<(String, String, Vec<String>)> {
    let mut t = tabs_of(&rig.snapshot(), label);
    t.sort();
    t
}

#[test]
fn release_leaves_the_tab_and_agent_running() {
    let Some(rig) = Rig::start() else { return };
    let cwd = rig.herdr.root.join("home");
    let client = rig.herdr.client();
    let ws = rig
        .rt
        .block_on(client.create_workspace(CreateWorkspace {
            label: "legacy".into(),
            cwd: cwd.clone(),
            env: vec![],
        }))
        .expect("create workspace");
    rig.rt
        .block_on(client.create_tab(CreateTab {
            workspace: ws.workspace.unwrap(),
            label: "worker".into(),
            cwd,
            env: vec![],
        }))
        .expect("create tab");
    let before = layout_of(&rig, "legacy");
    assert!(before.iter().any(|t| t.1 == "worker"), "{before:?}");

    // Adopt the workspace and the shell tab, release the seat, then the teamspace.
    rig.plan_apply(&["teamspace", "adopt", "legacy"]);
    rig.plan_apply(&["seat", "adopt", "worker"]);
    rig.wait_until("the adopted clone bound to its pane", |r| {
        r.clone_records().len() == 1
    });
    rig.settle();
    assert_eq!(layout_of(&rig, "legacy"), before, "adoption changed Herdr");

    rig.plan_apply(&["seat", "release", "worker"]);
    rig.settle();
    rig.settle();
    assert_eq!(
        layout_of(&rig, "legacy"),
        before,
        "seat release closed or moved something"
    );
    let recs = rig.clone_records();
    assert_eq!(recs.len(), 1, "{recs:?}");
    let retired = recs[0].1.get("retired").and_then(|r| r.as_table());
    assert_eq!(
        retired
            .and_then(|r| r.get("mechanism"))
            .and_then(|m| m.as_str()),
        Some("released"),
        "{:?}",
        recs[0].1
    );

    rig.plan_apply(&["teamspace", "release", "legacy"]);
    rig.settle();
    rig.settle();
    assert_eq!(
        layout_of(&rig, "legacy"),
        before,
        "teamspace release closed or moved something"
    );

    // A fresh adoption of the same tab, undone: the undo is a release and the tab survives.
    rig.plan_apply(&["teamspace", "adopt", "legacy"]);
    rig.plan_apply(&["seat", "adopt", "worker"]);
    rig.settle();
    let listed: Value = serde_json::from_str(&rig.cli_ok(&["undo", "--json"])).unwrap();
    let act = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "adopt" && c["summary"].as_str().unwrap().contains("seat"))
        .unwrap_or_else(|| panic!("no seat adoption to undo: {listed}"))["act"]
        .as_str()
        .unwrap()
        .to_owned();
    rig.plan_apply(&["undo", &act]);
    rig.settle();
    rig.settle();
    assert_eq!(
        layout_of(&rig, "legacy"),
        before,
        "undo of an adoption closed something"
    );
}
