//! Tier-3 `teamspace adopt` against a PRIVATE Herdr server: the real daemon adopts a workspace the graph did
//! not create, and the planner leaves the tabs in it alone. Run with:
//!
//!   cargo test --features private-herdr --test adopt_private_herdr -- --test-threads=1
//!
//! The rig is a fresh private server and instance; `Rig` reaps the daemon and every process under its root
//! on success, failure and panic (`PrivateHerdr` stops the server).
#![cfg(feature = "private-herdr")]

mod support;

use herdr_graph::ports::herdr::{CreateTab, CreateWorkspace, HerdrApi};
use serde_json::Value;
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
        let rig = Rig {
            herdr,
            instance,
            rt,
        };
        let out = rig.cli(&["init", rig.instance.to_str().unwrap()]);
        assert!(out.status.success(), "init: {}", stderr(&out));
        support::node_manager_opt_out(&rig.instance);
        let out = rig.cli(&["daemon", "--ensure"]);
        assert!(out.status.success(), "daemon --ensure: {}", stderr(&out));
        Some(rig)
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
