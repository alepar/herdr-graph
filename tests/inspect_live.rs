//! Tier-3 `roster --live` against a PRIVATE Herdr server: the real daemon, one `roster.live` call, the
//! committed binding matched against the live snapshot. Run with:
//!
//!   cargo test --features private-herdr --test inspect_live -- --test-threads=1
//!
//! The rig is a fresh private server and instance; `Rig` reaps the daemon and every process under its root
//! on success, failure and panic (`PrivateHerdr` stops the server).
#![cfg(feature = "private-herdr")]

mod support;

use herdr_graph::ports::herdr::HerdrApi;
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

    fn roster_json(&self, extra: &[&str]) -> Value {
        let mut args = vec!["roster", "--live", "--json"];
        args.extend_from_slice(extra);
        let out = self.cli(&args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {}", stderr(&out));
        let docs: Vec<Value> = serde_json::Deserializer::from_slice(&out.stdout)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(docs.len(), 1, "one JSON document");
        docs.into_iter().next().unwrap()
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

#[test]
fn roster_live_matches_the_graph_pane_and_lists_only_unbound_objects() {
    let Some(rig) = Rig::start() else { return };
    rig.plan_apply(&["teamspace", "create", "t", "--active"]);
    rig.plan_apply(&[
        "seat",
        "create",
        "foreman",
        "--teamspace",
        "t",
        "--active",
        "--harness",
        "shell",
    ]);
    rig.wait_until("the foreman tab", |r| {
        r.snapshot()
            .workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .any(|t| t.label == "foreman")
    });
    // the reconciler has recorded the binding once the clone shows a pane
    rig.wait_until("the clone's pane binding", |r| {
        let v = r.roster_json(&[]);
        v["teamspaces"][0]["seats"][0]["clones"][0]["runtime"]["pane"].is_string()
    });
    let graph_pane = {
        let v = rig.roster_json(&[]);
        v["teamspaces"][0]["seats"][0]["clones"][0]["runtime"]["pane"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    // a pane the graph does not manage, made directly through the private Herdr
    let created = rig
        .rt
        .block_on(rig.herdr.client().request(
            "workspace.create",
            json!({"label": "stray", "cwd": rig.herdr.root.join("home"), "focus": false}),
        ))
        .expect("workspace.create");
    let stray_pane = created
        .pointer("/root_pane/pane_id")
        .or_else(|| created.pointer("/pane/pane_id"))
        .and_then(Value::as_str)
        .expect("pane id")
        .to_owned();

    rig.settle();
    // No clone is seated in threads, so once the observe loop has made its first cycle the threads half is
    // healthy and empty (before that it is unavailable).
    rig.wait_until("the observe loop's first seat read", |r| {
        r.roster_json(&[])["live_unavailable"].is_null()
    });
    let commits = rig.commit_count();
    let v = rig.roster_json(&["--unmanaged"]);
    assert_eq!(v["schema"], 1);
    assert_eq!(v["live"], true);
    assert_eq!(v["live_unavailable"], json!(null), "{v}");
    let clone = &v["teamspaces"][0]["seats"][0]["clones"][0];
    assert_eq!(clone["live"]["live_pane_matches"], true, "{v}");
    assert_eq!(clone["live"]["binding_from_earlier_server"], false);
    assert_eq!(v["threads_live"]["seat_refusals"], json!({}), "{v}");
    assert_eq!(v["threads_live"]["seat_lost"], json!([]), "{v}");
    let panes: Vec<&str> = v["unmanaged"]["panes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(panes.contains(&stray_pane.as_str()), "{panes:?}");
    assert!(!panes.contains(&graph_pane.as_str()), "{panes:?}");
    let workspaces: Vec<&str> = v["unmanaged"]["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|w| w["label"].as_str())
        .collect();
    assert!(
        workspaces.contains(&"stray") && !workspaces.contains(&"t"),
        "{workspaces:?}"
    );

    // without the flag the section is absent
    let plain = rig.roster_json(&[]);
    assert!(!plain.as_object().unwrap().contains_key("unmanaged"));

    // text form works and reports nothing degraded
    let text = rig.cli_ok(&["roster", "--live", "--unmanaged"]);
    assert!(!text.contains("live data unavailable"), "{text}");
    assert!(text.contains("unmanaged:"), "{text}");

    assert_eq!(rig.commit_count(), commits, "roster --live adds no commit");
}
