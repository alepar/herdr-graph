//! Tier-3 end-to-end flows (spec §11): the real daemon and CLI against a PRIVATE Herdr server.
//! Seats use harness `shell` (Herdr's agent detection needs real binaries; tier 4 covers agents). Every flow
//! has its own rig, a fresh private server and a fresh instance, so failures isolate. Run with:
//!
//!   cargo test --features private-herdr --test e2e_private_herdr -- --test-threads=1 --nocapture
//!
//! Test names are the row ids of docs/verification-matrix.md. Nothing here touches the user's live Herdr.
#![cfg(feature = "private-herdr")]

mod support;

use herdr_graph::config::InstancePaths;
use herdr_graph::daemon::client::Client;
use herdr_graph::daemon::lock;
use herdr_graph::herdr::HerdrClient;
use herdr_graph::ports::herdr::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use support::private_herdr::PrivateHerdr;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");
const WAIT: Duration = Duration::from_secs(40);

struct E2e {
    herdr: PrivateHerdr,
    instance: PathBuf,
    rt: tokio::runtime::Runtime,
}

/// Start a rig, or skip the test when `herdr` is not installed.
macro_rules! rig {
    () => {
        match E2e::start() {
            Some(r) => r,
            None => return,
        }
    };
}

struct Row {
    id: String,
    name: String,
    state: String,
    path: PathBuf,
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The test binary or the plugin's `bin/herdr-graph`, running `daemon`: a pid is signalled only when its
/// command line says so.
fn is_graph_daemon(cmdline: &str) -> bool {
    let plugin_bin = format!("{}/bin/herdr-graph", env!("CARGO_MANIFEST_DIR"));
    (cmdline.contains(BIN) || cmdline.contains(&plugin_bin)) && cmdline.contains(" daemon")
}

fn pane_infos(snap: &HerdrSnapshot) -> Vec<PaneInfo> {
    snap.workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .flat_map(|t| &t.panes)
        .cloned()
        .collect()
}

/// The value at a dotted path of a TOML document.
fn at<'a>(v: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    let mut cur = v;
    for key in path.split('.') {
        cur = cur.get(key)?;
    }
    Some(cur)
}

/// String at a dotted path ("" when absent).
fn tstr(v: &toml::Value, path: &str) -> String {
    at(v, path)
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn tbool(v: &toml::Value, path: &str) -> bool {
    at(v, path).and_then(toml::Value::as_bool).unwrap_or(false)
}

impl E2e {
    fn start() -> Option<Self> {
        Self::start_inner(true)
    }

    /// A rig with an initialised instance and a private Herdr but no daemon yet.
    fn start_without_daemon() -> Option<Self> {
        Self::start_inner(false)
    }

    fn start_inner(daemon: bool) -> Option<Self> {
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
        let rig = Self {
            herdr,
            instance,
            rt,
        };
        let out = rig.cli(&["init", rig.instance.to_str().unwrap()]);
        assert!(out.status.success(), "init: {}", stderr(&out));
        if daemon {
            let out = rig.cli(&["daemon", "--ensure"]);
            assert!(out.status.success(), "daemon --ensure: {}", stderr(&out));
        }
        Some(rig)
    }

    // ---- processes -------------------------------------------------------------------------------

    /// A command with the private env and nothing else, plus the instance.
    fn command(&self, program: &str) -> Command {
        let mut c = self.herdr.command(Path::new(program));
        c.env("HERDR_GRAPH_INSTANCE", &self.instance);
        c
    }

    fn cli(&self, args: &[&str]) -> Output {
        self.command(BIN).args(args).output().unwrap()
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

    /// The CLI as a graph pane would run it: `HERDR_GRAPH_SEAT` / `HERDR_GRAPH_CLONE` set.
    fn cli_as(&self, seat: &str, clone: &str, args: &[&str]) -> Output {
        self.command(BIN)
            .env("HERDR_GRAPH_SEAT", seat)
            .env("HERDR_GRAPH_CLONE", clone)
            .args(args)
            .output()
            .unwrap()
    }

    /// Relay flow: `plan --json`, then `apply --confirm <hash> --confirmed-by user-relay`.
    /// Returns (committed, apply stdout).
    fn plan_apply_raw(&self, words: &[&str]) -> (bool, String) {
        self.plan_apply_as(None, words)
    }

    fn plan_apply_as(&self, who: Option<(&str, &str)>, words: &[&str]) -> (bool, String) {
        let mut args = vec!["plan", "--json"];
        args.extend_from_slice(words);
        let run = |args: &[&str]| match who {
            Some((seat, clone)) => self.cli_as(seat, clone, args),
            None => self.cli(args),
        };
        let o = run(&args);
        assert!(
            o.status.success(),
            "plan {words:?}: {}{}",
            stderr(&o),
            stdout(&o)
        );
        let plan: Value =
            serde_json::from_str(&stdout(&o)).unwrap_or_else(|e| panic!("plan {words:?}: {e}"));
        let (id, hash) = (
            plan["plan_id"].as_str().expect("plan_id").to_owned(),
            plan["hash"].as_str().expect("hash").to_owned(),
        );
        let out = run(&[
            "apply",
            &id,
            "--confirm",
            &hash,
            "--confirmed-by",
            "user-relay",
        ]);
        (
            out.status.success(),
            format!("{}{}", stdout(&out), stderr(&out)),
        )
    }

    fn plan_apply(&self, words: &[&str]) -> String {
        let (ok, out) = self.plan_apply_raw(words);
        assert!(ok, "apply {words:?} did not commit: {out}");
        out
    }

    /// `plan --json` only: the stored plan reply.
    fn plan_json(&self, words: &[&str]) -> Value {
        let mut args = vec!["plan", "--json"];
        args.extend_from_slice(words);
        serde_json::from_str(&self.cli_ok(&args)).unwrap_or_else(|e| panic!("plan {words:?}: {e}"))
    }

    fn apply_plan(&self, plan: &Value) -> (bool, String) {
        let (id, hash) = (
            plan["plan_id"].as_str().unwrap(),
            plan["hash"].as_str().unwrap(),
        );
        let out = self.cli(&[
            "apply",
            id,
            "--confirm",
            hash,
            "--confirmed-by",
            "user-relay",
        ]);
        (
            out.status.success(),
            format!("{}{}", stdout(&out), stderr(&out)),
        )
    }

    // ---- Herdr -----------------------------------------------------------------------------------

    fn snapshot(&self) -> HerdrSnapshot {
        let c: HerdrClient = self.herdr.client();
        self.rt.block_on(c.snapshot()).expect("snapshot")
    }

    fn raw(&self, method: &'static str, params: Value) -> Result<Value, HerdrError> {
        let c = self.herdr.client();
        self.rt.block_on(c.request(method, params))
    }

    fn raw_ok(&self, method: &'static str, params: Value) -> Value {
        self.raw(method, params)
            .unwrap_or_else(|e| panic!("{method}: {e}"))
    }

    /// `(workspace id, tab)` whose tab label is `label`.
    fn tab_labelled(&self, label: &str) -> Option<(String, TabInfo)> {
        self.snapshot().workspaces.into_iter().find_map(|w| {
            w.tabs
                .into_iter()
                .find(|t| t.label == label)
                .map(|t| (w.id.0, t))
        })
    }

    /// The pane whose graph token names `clone`.
    fn pane_of(&self, clone: &str) -> Option<PaneInfo> {
        let token = format!("hg={clone}");
        pane_infos(&self.snapshot())
            .into_iter()
            .find(|p| p.metadata.get("hg").map(String::as_str) == Some(token.as_str()))
    }

    fn tab_count(&self) -> usize {
        self.snapshot()
            .workspaces
            .iter()
            .map(|w| w.tabs.len())
            .sum()
    }

    /// Type `cmd` into a pane's shell and press Enter.
    fn pane_sh(&self, pane: &str, cmd: &str) {
        self.raw_ok("pane.send_text", json!({ "pane_id": pane, "text": cmd }));
        self.raw_ok(
            "pane.send_keys",
            json!({ "pane_id": pane, "keys": ["Enter"] }),
        );
    }

    /// A plain pane (no graph env) in a new workspace of the private server.
    fn plain_pane(&self, label: &str) -> String {
        let v = self.raw_ok(
            "workspace.create",
            json!({ "label": label, "cwd": self.herdr.root.join("home"), "focus": false }),
        );
        v.pointer("/root_pane/pane_id")
            .or_else(|| v.pointer("/pane/pane_id"))
            .and_then(Value::as_str)
            .expect("pane id")
            .to_owned()
    }

    // ---- waiting ---------------------------------------------------------------------------------

    fn wait_until(&self, desc: &str, timeout: Duration, mut pred: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if pred(self) {
                return;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        panic!(
            "timed out ({}s) waiting for {desc}\ndaemon log:\n{}",
            timeout.as_secs(),
            self.daemon_log()
        );
    }

    fn daemon_log(&self) -> String {
        std::fs::read_to_string(self.instance.join(".graph-local/daemon.log")).unwrap_or_default()
    }

    /// The daemon answers and no Herdr effect is pending, unknown or awaiting revision: the reconciler has
    /// caught up with the committed graph.
    fn settled(&self) -> bool {
        Client::connect(
            &InstancePaths::new(&self.instance).socket,
            Duration::from_secs(30),
        )
        .is_ok()
            && self.open_herdr_effects().is_empty()
    }

    fn wait_settled(&self) {
        self.wait_until("the reconciler to settle", WAIT, |r| r.settled());
    }

    // ---- graph reads -----------------------------------------------------------------------------

    /// `herdr-graph show <id>` as TOML.
    fn show(&self, id: &str) -> toml::Value {
        let text = self.cli_ok(&["show", id]);
        toml::Value::Table(
            text.parse::<toml::Table>()
                .unwrap_or_else(|e| panic!("show {id} is not toml ({e}): {text}")),
        )
    }

    /// Effects that are not yet terminal: `(kind, object, status, last_error)`.
    fn open_effects(&self) -> Vec<(String, String, String, String)> {
        use herdr_graph::model::effect::EffectStatus::{NeedsRevision, Pending, Unknown};
        let j = herdr_graph::journal::Journal::open(&InstancePaths::new(&self.instance).journal)
            .expect("journal");
        j.effects_with_status(&[Pending, Unknown, NeedsRevision])
            .expect("effects")
            .into_iter()
            .map(|e| {
                (
                    e.kind.as_str().to_owned(),
                    e.object.to_string(),
                    format!("{:?}", e.status),
                    e.last_error.unwrap_or_default(),
                )
            })
            .collect()
    }

    /// Every effect recorded for an object: `(kind, status)`.
    fn effects_of(&self, id: &str) -> Vec<(String, String)> {
        let j = herdr_graph::journal::Journal::open(&InstancePaths::new(&self.instance).journal)
            .expect("journal");
        let any = herdr_graph::model::AnyId::parse(id).expect("object id");
        j.effects_for_object(&any)
            .expect("effects")
            .into_iter()
            .map(|e| (e.kind.as_str().to_owned(), format!("{:?}", e.status)))
            .collect()
    }

    /// Open effects that act on Herdr. Thread effects (ensure_thread, invite, ...) stay pending in every rig
    /// because no herdr-threads service runs here; threads are covered by tier 4 and the threads smoke test.
    fn open_herdr_effects(&self) -> Vec<(String, String, String, String)> {
        const THREADS: [&str; 6] = [
            "ensure_thread",
            "invite",
            "release_requirement",
            "notify",
            "set_topic",
            "deliver_request",
        ];
        self.open_effects()
            .into_iter()
            .filter(|e| !THREADS.contains(&e.0.as_str()))
            .collect()
    }

    /// `list <kind>` rows: `<id>  <name>  <state>  <path>`.
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

    /// The live (active or dormant) seat called `name`.
    fn seat(&self, name: &str) -> Row {
        self.row("seats", name, "active")
            .or_else(|| self.row("seats", name, "dormant"))
            .unwrap_or_else(|| {
                panic!(
                    "no live seat {name}: {:?}",
                    self.rows("seats")
                        .iter()
                        .map(|r| (&r.name, &r.state))
                        .collect::<Vec<_>>()
                )
            })
    }

    fn seat_state(&self, id: &str) -> String {
        tstr(&self.show(id), "lifecycle")
    }

    /// Every clone of `seat_id`, any lifecycle.
    fn clones_of(&self, seat_id: &str) -> Vec<Row> {
        self.rows("clones")
            .into_iter()
            .filter(|c| tstr(&self.show(&c.id), "seat") == seat_id)
            .collect()
    }

    fn live_clone(&self, seat_id: &str) -> Row {
        self.clones_of(seat_id)
            .into_iter()
            .find(|c| c.state == "active")
            .unwrap_or_else(|| panic!("seat {seat_id} has no active clone"))
    }

    fn ops_text(&self) -> String {
        self.cli_ok(&["ops"])
    }

    fn create_teamspace_with_seat(&self, ts: &str, seat: &str) {
        self.plan_apply(&["teamspace", "create", ts, "--active"]);
        self.plan_apply(&[
            "seat",
            "create",
            seat,
            "--teamspace",
            ts,
            "--active",
            "--harness",
            "shell",
        ]);
        self.wait_until(&format!("the tab of {seat}"), WAIT, |r| {
            r.tab_labelled(seat).is_some()
        });
        self.wait_settled();
    }

    /// `undo --json` candidates: (act id, whole candidate JSON), newest first.
    fn undo_candidates(&self) -> Vec<(String, String)> {
        let v: Value =
            serde_json::from_str(&self.cli_ok(&["undo", "--json"])).expect("undo --json");
        v.as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|c| {
                (
                    c["act"].as_str().unwrap_or_default().to_owned(),
                    c.to_string(),
                )
            })
            .collect()
    }

    // ---- daemon and Herdr lifecycle --------------------------------------------------------------

    fn daemon_pid(&self) -> Option<u32> {
        lock::read_info(&self.instance.join(".graph-local/daemon.lock")).map(|i| i.pid)
    }

    /// Verify the pid is this rig's daemon (argv check), then signal it. Never a stray process.
    fn signal_daemon(&self, sig: i32) -> u32 {
        let pid = self.daemon_pid().expect("daemon lock info");
        let ps = Command::new("/bin/ps")
            .args(["-o", "command=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let cmdline = String::from_utf8_lossy(&ps.stdout).into_owned();
        assert!(
            is_graph_daemon(&cmdline),
            "pid {pid} is not our daemon: {cmdline}"
        );
        // SAFETY: the pid was verified above to be this rig's daemon.
        assert_eq!(unsafe { libc::kill(pid as i32, sig) }, 0);
        pid
    }

    fn pid_alive(pid: u32) -> bool {
        // SAFETY: signal 0 only probes existence.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    /// SIGKILL the daemon and wait until it is gone.
    fn kill_daemon(&self) {
        let pid = self.signal_daemon(libc::SIGKILL);
        let deadline = Instant::now() + WAIT;
        while Self::pid_alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!Self::pid_alive(pid), "daemon {pid} survived SIGKILL");
    }

    fn start_daemon(&self) {
        self.cli_ok(&["daemon", "--ensure"]);
    }

    fn herdr_restart(&mut self) {
        self.herdr.restart().expect("herdr restart");
    }

    fn ensure_daemon_down_is_quiet(&self) {
        assert!(self.daemon_pid().is_none_or(|p| !Self::pid_alive(p)));
    }
}

impl Drop for E2e {
    /// The daemon is detached (its own session) and would outlive the test, a stopped one forever: stop it,
    /// after the same argv check as every other signal.
    fn drop(&mut self) {
        if let Some(pid) = self.daemon_pid().filter(|p| Self::pid_alive(*p)) {
            let ps = Command::new("/bin/ps")
                .args(["-o", "command=", "-p", &pid.to_string()])
                .output()
                .unwrap();
            let cmdline = String::from_utf8_lossy(&ps.stdout).into_owned();
            if is_graph_daemon(&cmdline) {
                // SAFETY: argv-verified as this rig's daemon; CONT first so a stopped process can take the KILL.
                unsafe {
                    libc::kill(pid as i32, libc::SIGCONT);
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
        // Everything else that inherited the root marker (pane shells, startup-hook daemons, stopped processes).
        support::isolated::reap_root(&self.herdr.root);
    }
}

// FLOWS-BEGIN

/// Run `cmd` in a graph pane and return what it printed to a file under the private root.
fn pane_capture(rig: &E2e, pane: &str, name: &str, cmd: &str) -> String {
    let file = rig.herdr.root.join(name);
    rig.pane_sh(pane, &format!("{cmd} > {} 2>&1", file.display()));
    rig.wait_until(&format!("{name} output"), WAIT, |_| {
        std::fs::metadata(&file).is_ok_and(|m| m.len() > 0)
    });
    std::thread::sleep(Duration::from_millis(200));
    std::fs::read_to_string(&file).unwrap()
}

#[test]
fn e2e_create_teamspace_seats_tabs_panes_env() {
    let rig = rig!();
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
    rig.plan_apply(&[
        "seat",
        "create",
        "worker",
        "--teamspace",
        "t",
        "--harness",
        "shell",
    ]);
    rig.wait_until("the foreman tab", WAIT, |r| {
        r.tab_labelled("foreman").is_some()
    });
    rig.wait_settled();

    let ts = rig.row("teamspaces", "t", "active").expect("teamspace t");
    let snap = rig.snapshot();
    let ws = snap
        .workspaces
        .iter()
        .find(|w| w.label == "t")
        .expect("workspace labelled with the teamspace");
    assert_eq!(
        ws.metadata.get("hg").map(String::as_str),
        Some(format!("hg={}", ts.id).as_str()),
        "workspace token"
    );

    let foreman = rig.seat("foreman");
    let clone = rig.live_clone(&foreman.id);
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    assert_eq!(tab.panes.len(), 1, "one clone, one pane");
    assert_eq!(
        tab.panes[0].metadata.get("hg").map(String::as_str),
        Some(format!("hg={}", clone.id).as_str()),
        "pane token"
    );
    assert!(
        rig.tab_labelled("worker").is_none(),
        "a dormant seat gets no tab"
    );
    assert_eq!(rig.seat("worker").state, "dormant");
    let rt = rig.show(&foreman.id);
    assert_eq!(tstr(&rt, "runtime.availability"), "present");
    assert_eq!(tstr(&rt, "runtime.bound.tab_id"), tab.id.0);

    let env = pane_capture(
        &rig,
        &tab.panes[0].id.0,
        "env-foreman.txt",
        "env | grep '^HERDR_GRAPH' | sort",
    );
    assert!(env.lines().any(|l| l == "HERDR_GRAPH=1"), "{env}");
    assert!(
        env.lines()
            .any(|l| l == format!("HERDR_GRAPH_INSTANCE={}", rig.instance.display())),
        "{env}"
    );
    assert!(
        env.lines()
            .any(|l| l == format!("HERDR_GRAPH_SEAT={}", foreman.id)),
        "{env}"
    );
    assert!(
        env.lines()
            .any(|l| l == format!("HERDR_GRAPH_CLONE={}", clone.id)),
        "{env}"
    );
}

/// A seat with `extra` additional clones, all with panes in the seat's tab.
fn seat_with_clones(rig: &E2e, ts: &str, seat: &str, extra: usize) -> (Row, Vec<Row>) {
    rig.create_teamspace_with_seat(ts, seat);
    let s = rig.seat(seat);
    for _ in 0..extra {
        let want = rig.clones_of(&s.id).len() + 1;
        rig.plan_apply(&["clone", "add", seat]);
        rig.wait_until("the added clone's pane", WAIT, |r| {
            r.clones_of(&s.id).len() == want
                && r.clones_of(&s.id)
                    .iter()
                    .all(|c| r.pane_of(&c.id).is_some())
        });
    }
    rig.wait_settled();
    let clones = rig.clones_of(&s.id);
    (s, clones)
}

#[test]
fn e2e_rename_tab_renames_seat_moves_path_history() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    let before = rig.seat("foreman");
    assert!(before.path.exists());
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    rig.raw_ok(
        "tab.rename",
        json!({ "tab_id": tab.id.0, "label": "chief" }),
    );

    rig.wait_until("the seat to be renamed", WAIT, |r| {
        r.row("seats", "chief", "active").is_some()
    });
    // Committed reads become visible before the derived working-tree fast-forward finishes.
    // Wait for that boundary before inspecting the directory move.
    rig.wait_until("the renamed working-tree view", WAIT, |r| {
        let store = herdr_graph::store::GitStore::open(&r.instance).unwrap();
        use herdr_graph::ports::store::Store;
        herdr_graph::writer::worktree::view_rev(&r.instance) == Some(store.head().unwrap())
    });
    let after = rig.seat("chief");
    assert_eq!(after.id, before.id, "same seat, new name");
    assert!(rig.row("seats", "foreman", "active").is_none());
    assert_ne!(after.path, before.path);
    assert!(
        after.path.exists(),
        "the seat directory moved to {}",
        after.path.display()
    );
    assert!(
        !before.path.exists(),
        "the old directory {} is gone",
        before.path.display()
    );
    assert!(
        rig.show(&after.id)["name_history"]
            .to_string()
            .contains("foreman"),
        "{}",
        rig.show(&after.id)
    );
    assert!(
        rig.ops_text()
            .lines()
            .any(|l| l.contains("observed") && l.contains("new=chief")),
        "{}",
        rig.ops_text()
    );
    assert_eq!(
        rig.tab_labelled("chief").map(|t| t.1.id),
        Some(tab.id.clone()),
        "the tab label is not reverted"
    );
}

#[test]
fn e2e_close_pane_retires_clone() {
    let rig = rig!();
    let (seat, clones) = seat_with_clones(&rig, "t", "foreman", 1);
    assert_eq!(clones.len(), 2);
    let victim = clones
        .iter()
        .find(|c| c.name != "foreman")
        .unwrap_or(&clones[1]);
    let survivor = clones.iter().find(|c| c.id != victim.id).unwrap();
    let pane = rig.pane_of(&victim.id).expect("victim pane").id.0;
    rig.raw_ok("pane.close", json!({ "pane_id": pane }));

    rig.wait_until("the closed pane's clone to retire", WAIT, |r| {
        r.show(&victim.id)["lifecycle"].as_str() == Some("retired")
    });
    assert_eq!(rig.show(&survivor.id)["lifecycle"].as_str(), Some("active"));
    assert_eq!(
        rig.seat_state(&seat.id),
        "active",
        "the seat survives while a clone does"
    );
    let (_, tab) = rig.tab_labelled("foreman").expect("the tab stays");
    assert_eq!(tab.panes.len(), 1);
    assert!(rig.pane_of(&survivor.id).is_some());
    let acts = rig.undo_candidates();
    assert!(
        acts.iter().any(|(_, c)| c.contains(&victim.id)),
        "pane closure is undoable: {acts:?}"
    );
}

#[test]
fn e2e_close_tab_retires_seat_and_clones() {
    let rig = rig!();
    let (seat, clones) = seat_with_clones(&rig, "t", "foreman", 1);
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    rig.raw_ok("tab.close", json!({ "tab_id": tab.id.0 }));

    rig.wait_until("the seat to retire", WAIT, |r| {
        r.seat_state(&seat.id) == "retired"
    });
    for c in &clones {
        rig.wait_until("the clone to retire", WAIT, |r| {
            r.show(&c.id)["lifecycle"].as_str() == Some("retired")
        });
    }
    let retired = rig
        .row("seats", "foreman", "retired")
        .expect("retired seat is listed");
    assert!(
        retired.path.to_string_lossy().contains("/archive/"),
        "archived: {}",
        retired.path.display()
    );
    assert!(
        rig.undo_candidates()
            .iter()
            .any(|(_, c)| c.contains("tab closure")),
        "{:?}",
        rig.undo_candidates()
    );
}

#[test]
fn e2e_close_workspace_retires_all_incl_dormant() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    rig.plan_apply(&[
        "seat",
        "create",
        "worker",
        "--teamspace",
        "t",
        "--harness",
        "shell",
    ]);
    let (foreman, worker) = (rig.seat("foreman"), rig.seat("worker"));
    assert_eq!(worker.state, "dormant");
    let ws = rig
        .snapshot()
        .workspaces
        .iter()
        .find(|w| w.label == "t")
        .unwrap()
        .id
        .0
        .clone();
    rig.raw_ok("workspace.close", json!({ "workspace_id": ws }));

    rig.wait_until("the active seat to retire", WAIT, |r| {
        r.seat_state(&foreman.id) == "retired"
    });
    rig.wait_until("the dormant seat to retire", WAIT, |r| {
        r.seat_state(&worker.id) == "retired"
    });
    for seat in [&foreman, &worker] {
        for c in rig.clones_of(&seat.id) {
            assert_eq!(
                rig.show(&c.id)["lifecycle"].as_str(),
                Some("retired"),
                "clone {} of {}",
                c.id,
                seat.name
            );
        }
    }
    assert!(
        rig.row("teamspaces", "t", "retired").is_some(),
        "{:?}",
        rig.rows("teamspaces")
            .iter()
            .map(|r| (&r.name, &r.state))
            .collect::<Vec<_>>()
    );
    assert!(rig.snapshot().workspaces.is_empty(), "nothing is recreated");
}

/// Close the foreman tab and return its undo act id.
fn close_tab_and_wait(rig: &E2e, seat: &Row, clones: &[Row], label: &str) -> String {
    let (_, tab) = rig.tab_labelled(label).unwrap();
    rig.raw_ok("tab.close", json!({ "tab_id": tab.id.0 }));
    rig.wait_until("the seat to retire", WAIT, |r| {
        r.seat_state(&seat.id) == "retired"
    });
    for c in clones {
        rig.wait_until("the clone to retire", WAIT, |r| {
            r.show(&c.id)["lifecycle"].as_str() == Some("retired")
        });
    }
    rig.wait_settled();
    let acts = rig.undo_candidates();
    acts.iter()
        .find(|(_, c)| c.contains("tab closure"))
        .unwrap_or_else(|| panic!("no tab-closure act: {acts:?}"))
        .0
        .clone()
}

#[test]
fn e2e_undo_tab_close_restores_in_new_tab() {
    let rig = rig!();
    let (seat, clones) = seat_with_clones(&rig, "t", "foreman", 1);
    let old_tab = rig.tab_labelled("foreman").unwrap().1.id;
    let act = close_tab_and_wait(&rig, &seat, &clones, "foreman");

    let preview = rig.plan_json(&["undo", &act]);
    assert!(
        preview["rendered"].as_str().unwrap().contains("foreman"),
        "{}",
        preview["rendered"]
    );
    let (ok, out) = rig.apply_plan(&preview);
    assert!(ok, "undo apply: {out}");

    rig.wait_until("the seat to be active again", WAIT, |r| {
        r.seat_state(&seat.id) == "active"
    });
    for c in &clones {
        rig.wait_until("the clone's pane", WAIT, |r| r.pane_of(&c.id).is_some());
        assert_eq!(rig.show(&c.id)["lifecycle"].as_str(), Some("active"));
    }
    rig.wait_settled();
    let (_, tab) = rig.tab_labelled("foreman").expect("restored tab");
    assert_ne!(tab.id, old_tab, "the restored tab is a new tab");
    assert_eq!(tab.panes.len(), clones.len());
    assert!(rig.seat("foreman").path.exists());
}

/// A plain pane of the private Herdr runs `herdr-graph undo`, picks the newest action and confirms its preview.
/// Returns the pane id and waits until the undo op committed.
fn undo_from_new_pane(rig: &E2e) -> String {
    let caller = rig.plain_pane("caller");
    undo_from_pane(rig, &caller);
    caller
}

/// `herdr-graph undo` run in `caller` (picks the newest action, confirms) until the undo committed. Returns the
/// text the command printed, preview included.
fn undo_from_pane(rig: &E2e, caller: &str) -> String {
    let log = rig.herdr.root.join("undo-pane.log");
    rig.pane_sh(
        caller,
        &format!(
            "HERDR_GRAPH_INSTANCE={} {BIN} undo 2>&1 | tee {}",
            rig.instance.display(),
            log.display()
        ),
    );
    let read = || std::fs::read_to_string(&log).unwrap_or_default();
    rig.wait_until("the action list", WAIT, |_| {
        read().contains("Select action number")
    });
    rig.raw_ok(
        "pane.send_text",
        json!({ "pane_id": caller, "text": "1\n" }),
    );
    rig.wait_until("the preview prompt", WAIT, |_| {
        read().contains("Apply this undo?")
    });
    rig.raw_ok(
        "pane.send_text",
        json!({ "pane_id": caller, "text": "y\n" }),
    );
    rig.wait_until("the undo to commit", WAIT, |_| read().contains("committed"));
    rig.wait_settled();
    read()
}

/// F2 (hg-zmi.54): undo run from a pane bound to another clone A adopts that pane for the restored clone B. A is
/// retired with its history, and the pane is never closed (no new pane, no closed pane).
#[test]
fn e2e_undo_from_bound_pane_adopts_and_keeps_it() {
    let rig = rig!();
    let (_, clones) = seat_with_clones(&rig, "t", "foreman", 1);
    let (a, b) = (&clones[0], &clones[1]);
    let a_pane = rig.pane_of(&a.id).unwrap().id.0;
    let b_pane = rig.pane_of(&b.id).unwrap().id.0;
    rig.raw_ok("pane.close", json!({ "pane_id": b_pane }));
    rig.wait_until("B to retire", WAIT, |r| {
        r.show(&b.id)["lifecycle"].as_str() == Some("retired")
    });
    rig.wait_settled();
    let sessions_a = rig.show(&a.id).get("sessions").cloned();
    let panes_before = pane_infos(&rig.snapshot()).len();

    let log = undo_from_pane(&rig, &a_pane);
    rig.wait_until("A's pane to carry the restored clone's token", WAIT, |r| {
        r.pane_of(&b.id).is_some_and(|p| p.id.0 == a_pane)
    });
    rig.wait_settled();

    assert!(
        pane_infos(&rig.snapshot()).iter().any(|p| p.id.0 == a_pane),
        "the adopted pane survives"
    );
    assert_eq!(
        pane_infos(&rig.snapshot()).len(),
        panes_before,
        "no pane created, none closed"
    );
    let (b_now, a_now) = (rig.show(&b.id), rig.show(&a.id));
    assert_eq!(b_now["lifecycle"].as_str(), Some("active"));
    assert_eq!(
        tstr(&b_now, "runtime.bound.pane_id"),
        a_pane,
        "B is bound to the adopted pane"
    );
    assert_eq!(a_now["lifecycle"].as_str(), Some("retired"));
    assert_eq!(tstr(&a_now, "retired.mechanism"), "undo");
    assert_eq!(
        a_now.get("sessions").cloned(),
        sessions_a,
        "the displaced clone keeps its history"
    );
    assert!(
        !log.contains("closes your pane"),
        "the preview must not warn about closing the pane: {log}"
    );
    // Later observations do not retire B either.
    std::thread::sleep(Duration::from_secs(2));
    rig.wait_settled();
    assert_eq!(rig.show(&b.id)["lifecycle"].as_str(), Some("active"));
}

/// D1 regression (hg-zmi.51): undo run from a pane commits with `undo.adopt_pane pane=<caller>` and the undo commit
/// itself binds the restored clone to that pane, so the reconciler woken by the commit has nothing to create. The
/// caller's pane becomes the restored clone (spec §6) and no second pane is split.
#[test]
fn e2e_undo_from_pane_adopts_caller_pane() {
    let rig = rig!();
    let (seat, clones) = seat_with_clones(&rig, "t", "foreman", 1);
    let victim = &clones[1];
    let pane = rig.pane_of(&victim.id).unwrap().id.0;
    rig.raw_ok("pane.close", json!({ "pane_id": pane }));
    rig.wait_until("the clone to retire", WAIT, |r| {
        r.show(&victim.id)["lifecycle"].as_str() == Some("retired")
    });
    rig.wait_settled();
    let panes_before = pane_infos(&rig.snapshot()).len();

    let caller = undo_from_new_pane(&rig);
    rig.wait_until(
        "the caller pane to carry the restored clone's token",
        WAIT,
        |r| r.pane_of(&victim.id).is_some_and(|p| p.id.0 == caller),
    );
    assert_eq!(rig.show(&victim.id)["lifecycle"].as_str(), Some("active"));
    assert_eq!(rig.seat_state(&seat.id), "active");
    assert_eq!(
        tstr(&rig.show(&victim.id), "runtime.bound.pane_id"),
        caller,
        "bound to the caller's pane"
    );
    // The caller's pane was adopted, not joined by a second one: one pane more than before the undo.
    assert_eq!(
        pane_infos(&rig.snapshot()).len(),
        panes_before + 1,
        "the caller pane itself (new), nothing else created"
    );
    assert_eq!(
        rig.tab_labelled("foreman").unwrap().1.panes.len(),
        1,
        "foreman's own tab keeps its surviving clone only"
    );
}

/// D1 regression, tab variant: the undone action closed a whole TAB, so the plan also carries `runtime.open_tab`;
/// the caller's pane is the restored clone's pane, so no tab is opened for it.
#[test]
fn e2e_undo_tab_close_from_pane_adopts_caller_pane() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    let seat = rig.seat("foreman");
    let clone = rig.live_clone(&seat.id);
    close_tab_and_wait(&rig, &seat, std::slice::from_ref(&clone), "foreman");
    let caller = undo_from_new_pane(&rig);
    rig.wait_until("the caller pane to carry the clone's token", WAIT, |r| {
        r.pane_of(&clone.id).is_some_and(|p| p.id.0 == caller)
    });
    assert_eq!(rig.seat_state(&seat.id), "active");
}

/// A template document with the given members, all active shell seats.
fn template_doc(rig: &E2e, file: &str, name: &str, members: &[&str]) -> String {
    let mut text = format!("name = \"{name}\"\n\n[defaults]\nharness = \"shell\"\n");
    for m in members {
        text.push_str(&format!(
            "\n[[members]]\nname = \"{m}\"\nstartup = \"active\"\n"
        ));
    }
    let path = rig.herdr.root.join(file);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn e2e_template_edit_adds_member_new_tab() {
    let rig = rig!();
    rig.plan_apply(&["teamspace", "create", "t", "--active"]);
    let v1 = template_doc(&rig, "tpl-v1.toml", "duo", &["alpha"]);
    rig.plan_apply(&["template", "create", "duo", "--from", &v1]);
    rig.plan_apply(&[
        "application",
        "apply",
        "duo",
        "--teamspace",
        "t",
        "--name",
        "app1",
    ]);
    rig.wait_until("alpha's tab", WAIT, |r| r.tab_labelled("alpha").is_some());
    rig.wait_settled();
    let tabs_before = rig.tab_count();
    assert!(rig.tab_labelled("beta").is_none());

    let v2 = template_doc(&rig, "tpl-v2.toml", "duo", &["alpha", "beta"]);
    rig.plan_apply(&["template", "edit", "duo", "--from", &v2]);
    rig.wait_until("beta's tab", WAIT, |r| r.tab_labelled("beta").is_some());
    rig.wait_settled();
    assert_eq!(rig.tab_count(), tabs_before + 1, "exactly one new tab");
    let beta = rig.seat("beta");
    assert_eq!(rig.seat_state(&beta.id), "active");
    assert!(rig.pane_of(&rig.live_clone(&beta.id).id).is_some());
    assert!(rig.tab_labelled("alpha").is_some(), "alpha is untouched");
}

#[test]
fn e2e_application_retire_closes_exclusive_seats() {
    let rig = rig!();
    rig.plan_apply(&["teamspace", "create", "t", "--active"]);
    let doc = template_doc(&rig, "tpl.toml", "duo", &["alpha", "beta"]);
    rig.plan_apply(&["template", "create", "duo", "--from", &doc]);
    rig.plan_apply(&[
        "application",
        "apply",
        "duo",
        "--teamspace",
        "t",
        "--name",
        "app1",
    ]);
    // A seat of the teamspace that the application does not own.
    rig.plan_apply(&[
        "seat",
        "create",
        "bystander",
        "--teamspace",
        "t",
        "--active",
        "--harness",
        "shell",
    ]);
    for label in ["alpha", "beta", "bystander"] {
        rig.wait_until(&format!("{label}'s tab"), WAIT, |r| {
            r.tab_labelled(label).is_some()
        });
    }
    rig.wait_settled();
    let (alpha, beta, bystander) = (rig.seat("alpha"), rig.seat("beta"), rig.seat("bystander"));

    rig.plan_apply(&["application", "retire", "app1"]);
    rig.wait_until("the exclusive seats to retire", WAIT, |r| {
        r.seat_state(&alpha.id) == "retired" && r.seat_state(&beta.id) == "retired"
    });
    rig.wait_until("their tabs to close", WAIT, |r| {
        r.tab_labelled("alpha").is_none() && r.tab_labelled("beta").is_none()
    });
    assert_eq!(
        rig.seat_state(&bystander.id),
        "active",
        "a seat outside the application stays"
    );
    assert!(rig.tab_labelled("bystander").is_some());
}

/// Teamspace `t` with two active seats, `foreman` and `bar`.
fn two_seats(rig: &E2e) -> (Row, Row) {
    rig.create_teamspace_with_seat("t", "foreman");
    rig.plan_apply(&[
        "seat",
        "create",
        "bar",
        "--teamspace",
        "t",
        "--active",
        "--harness",
        "shell",
    ]);
    rig.wait_until("bar's tab", WAIT, |r| r.tab_labelled("bar").is_some());
    rig.wait_settled();
    (rig.seat("foreman"), rig.seat("bar"))
}

fn retired_seat_count(rig: &E2e) -> usize {
    rig.rows("seats")
        .iter()
        .filter(|r| r.state == "retired")
        .count()
}

fn tab_labels(rig: &E2e) -> Vec<String> {
    let mut v: Vec<String> = rig
        .snapshot()
        .workspaces
        .iter()
        .flat_map(|w| w.tabs.iter().map(|t| t.label.clone()))
        .collect();
    v.sort();
    v
}

#[test]
fn e2e_retire_last_clone_induced_seat_retirement_once() {
    let rig = rig!();
    let (foreman, bar) = two_seats(&rig);
    let clone = rig.live_clone(&foreman.id);

    let plan = rig.plan_json(&["clone", "retire", &clone.id]);
    let text = plan["rendered"].as_str().unwrap();
    assert!(
        text.contains("seat.retire") && text.contains("(induced)"),
        "the plan explains the induced retirement:\n{text}"
    );
    assert!(
        text.contains("runtime.close_tab") && text.contains("last clone"),
        "{text}"
    );
    let (ok, out) = rig.apply_plan(&plan);
    assert!(ok, "{out}");

    rig.wait_until("the tab to close", WAIT, |r| {
        r.tab_labelled("foreman").is_none()
    });
    rig.wait_settled();
    assert_eq!(rig.seat_state(&foreman.id), "retired");
    assert_eq!(rig.show(&clone.id)["lifecycle"].as_str(), Some("retired"));
    // Give any (wrong) second retirement the time to show up, then count.
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(
        retired_seat_count(&rig),
        1,
        "retired exactly once: {:?}",
        rig.rows("seats")
            .iter()
            .map(|r| (&r.name, &r.state))
            .collect::<Vec<_>>()
    );
    let ops = rig.ops_text();
    assert!(
        !ops.lines().any(|l| l.contains("rule=tab")),
        "the induced tab close was explained, not observed as a user closure:\n{ops}"
    );
    assert_eq!(
        ops.lines().filter(|l| l.contains("seat_retire")).count(),
        0,
        "no separate seat_retire op: {ops}"
    );
    assert_eq!(rig.seat_state(&bar.id), "active");
}

#[test]
fn e2e_seat_deactivate_closes_tab_no_retirement() {
    let rig = rig!();
    let (foreman, bar) = two_seats(&rig);
    let bar_clones = rig.clones_of(&bar.id);
    rig.plan_apply(&["seat", "deactivate", "bar"]);

    rig.wait_until("bar's tab to close", WAIT, |r| {
        r.tab_labelled("bar").is_none()
    });
    rig.wait_settled();
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(rig.seat_state(&bar.id), "dormant");
    for c in &bar_clones {
        assert_ne!(
            rig.show(&c.id)["lifecycle"].as_str(),
            Some("retired"),
            "deactivation never retires clones"
        );
    }
    assert_eq!(retired_seat_count(&rig), 0);
    let ops = rig.ops_text();
    assert!(!ops.lines().any(|l| l.contains("rule=tab")), "{ops}");
    assert!(
        !rig.undo_candidates()
            .iter()
            .any(|(_, c)| c.contains("tab closure")),
        "{:?}",
        rig.undo_candidates()
    );
    assert_eq!(rig.seat_state(&foreman.id), "active");
    assert!(rig.tab_labelled("foreman").is_some());
}

#[test]
fn e2e_move_only_pane_moved_out_no_recreate() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    let seat = rig.seat("foreman");
    let clone = rig.live_clone(&seat.id);
    let pane = rig.pane_of(&clone.id).unwrap().id.0;
    // Herdr's own first tab of the workspace belongs to no seat.
    let (_, other) = rig.tab_labelled("1").expect("the workspace's own tab");
    rig.raw_ok("pane.move", json!({ "pane_id": pane, "destination": { "type": "tab", "tab_id": other.id.0, "split": "right" } }));

    rig.wait_until("the seat to be marked moved out", WAIT, |r| {
        tbool(&r.show(&seat.id), "moved_out")
    });
    rig.wait_settled();
    let (s, c) = (rig.show(&seat.id), rig.show(&clone.id));
    assert_eq!(tstr(&s, "lifecycle"), "active", "the seat is not retired");
    assert_eq!(tstr(&s, "runtime.availability"), "absent");
    assert!(tbool(&c, "reload_required"), "the clone must reload: {c}");
    assert_eq!(tstr(&c, "lifecycle"), "active");
    let tabs = rig.tab_count();
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(
        rig.tab_count(),
        tabs,
        "no tab is recreated for the moved-out seat"
    );
    assert_eq!(
        rig.snapshot().workspaces[0].tabs.len(),
        1,
        "only Herdr's own tab is left"
    );
    assert_eq!(retired_seat_count(&rig), 0);
}

/// Regression test for D3 (fixed by hg-zmi.49): moving a seat's pane into ANOTHER SEAT's tab must not make the
/// reconciler rename that tab to the moved-out seat's name (spec §4.3.4), which used to be observed as a user rename
/// of the other seat and produced duplicate seat names. See docs/verification-matrix.md.
#[test]
fn e2e_move_pane_into_other_seat_tab_keeps_that_seats_name() {
    let rig = rig!();
    let (foreman, _bar) = two_seats(&rig);
    let pane = rig.pane_of(&rig.live_clone(&foreman.id).id).unwrap().id.0;
    let (_, bar_tab) = rig.tab_labelled("bar").unwrap();
    rig.raw_ok("pane.move", json!({ "pane_id": pane, "destination": { "type": "tab", "tab_id": bar_tab.id.0, "split": "right" } }));
    rig.wait_until("the seat to be marked moved out", WAIT, |r| {
        tbool(&r.show(&foreman.id), "moved_out")
    });
    rig.wait_settled();
    std::thread::sleep(Duration::from_secs(4));
    assert!(
        rig.row("seats", "bar", "active").is_some(),
        "bar kept its name: {:?}",
        rig.rows("seats")
            .iter()
            .map(|r| (&r.name, &r.state))
            .collect::<Vec<_>>()
    );
    assert!(
        rig.tab_labelled("bar").is_some(),
        "bar's tab kept its label: {:?}",
        tab_labels(&rig)
    );
}

#[test]
fn e2e_close_while_daemon_down_unknown_not_recreated() {
    let rig = rig!();
    let (foreman, bar) = two_seats(&rig);
    rig.kill_daemon();
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    rig.raw_ok("tab.close", json!({ "tab_id": tab.id.0 }));
    rig.start_daemon();

    rig.wait_until("the daemon to settle", WAIT, |r| r.settled());
    std::thread::sleep(Duration::from_secs(5));
    let s = rig.show(&foreman.id);
    assert_eq!(
        tstr(&s, "lifecycle"),
        "active",
        "closure while the daemon was down is not a retirement: {s}"
    );
    assert_eq!(tstr(&s, "runtime.availability"), "unknown", "{s}");
    assert!(
        rig.tab_labelled("foreman").is_none(),
        "the tab is not recreated: {:?}",
        tab_labels(&rig)
    );
    assert_eq!(retired_seat_count(&rig), 0);
    assert_eq!(rig.seat_state(&bar.id), "active");
    assert!(rig.tab_labelled("bar").is_some());
}

#[test]
fn e2e_rename_while_daemon_down_recorded() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    let before = rig.seat("foreman");
    rig.kill_daemon();
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    rig.raw_ok(
        "tab.rename",
        json!({ "tab_id": tab.id.0, "label": "chief" }),
    );
    rig.start_daemon();

    rig.wait_until("the rename to be recorded", WAIT, |r| {
        r.row("seats", "chief", "active").is_some()
    });
    rig.wait_settled();
    assert_eq!(rig.seat("chief").id, before.id);
    assert_eq!(
        rig.tab_labelled("chief").map(|t| t.1.id),
        Some(tab.id),
        "the tab is not reverted to the old name"
    );
    assert!(rig.tab_labelled("foreman").is_none());
    let ops = rig.ops_text();
    assert!(
        ops.lines()
            .any(|l| l.contains("observed") && l.contains("new=chief")),
        "recorded as an observed rename:\n{ops}"
    );
    assert!(
        rig.show(&before.id)["name_history"]
            .to_string()
            .contains("foreman")
    );
}

#[test]
fn e2e_daemon_kill_mid_op_recovers_no_duplicates() {
    let rig = rig!();
    rig.plan_apply(&["teamspace", "create", "t", "--active"]);
    // Three seats admitted back to back; the daemon is killed the moment the last op commits, while its
    // effects (workspace, tabs, tokens) are still in flight.
    for name in ["s1", "s2", "s3"] {
        rig.plan_apply(&[
            "seat",
            "create",
            name,
            "--teamspace",
            "t",
            "--active",
            "--harness",
            "shell",
        ]);
    }
    rig.kill_daemon();
    rig.start_daemon();

    for name in ["s1", "s2", "s3"] {
        rig.wait_until(&format!("{name}'s tab"), WAIT, |r| {
            r.tab_labelled(name).is_some()
        });
    }
    rig.wait_settled();
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(
        tab_labels(&rig),
        ["1", "s1", "s2", "s3"],
        "one tab per seat, nothing duplicated"
    );
    assert_eq!(rig.snapshot().workspaces.len(), 1, "one workspace");
    let tokens: Vec<String> = pane_infos(&rig.snapshot())
        .iter()
        .filter_map(|p| p.metadata.get("hg").cloned())
        .collect();
    assert_eq!(tokens.len(), 3, "one stamped pane per clone: {tokens:?}");
    assert_eq!(pane_infos(&rig.snapshot()).len(), 4);
    for name in ["s1", "s2", "s3"] {
        let seat = rig.seat(name);
        assert_eq!(rig.clones_of(&seat.id).len(), 1);
        assert_eq!(tstr(&rig.show(&seat.id), "runtime.availability"), "present");
    }
    assert_eq!(retired_seat_count(&rig), 0);
}

#[test]
fn e2e_event_loss_reconnect_converges() {
    let rig = rig!();
    let (foreman, _bar) = two_seats(&rig);
    rig.plan_apply(&["clone", "add", "foreman"]);
    rig.wait_until("foreman's second pane", WAIT, |r| {
        r.tab_labelled("foreman")
            .is_some_and(|t| t.1.panes.len() == 2)
    });
    rig.wait_settled();
    let clones = rig.clones_of(&foreman.id);

    // The daemon sees none of the following events while it is stopped: a burst of renames, one final
    // label, and a pane closure. Whether Herdr buffers or drops them is not ours to know; the daemon must
    // converge from a fresh snapshot either way.
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    let pane = tab
        .panes
        .iter()
        .find_map(|p| rig.pane_of(&clones[1].id).filter(|q| q.id == p.id))
        .map(|p| p.id.0)
        .unwrap();
    let pid = rig.signal_daemon(libc::SIGSTOP);
    for i in 0..60 {
        rig.raw_ok(
            "tab.rename",
            json!({ "tab_id": tab.id.0, "label": format!("churn-{i}") }),
        );
    }
    rig.raw_ok(
        "tab.rename",
        json!({ "tab_id": tab.id.0, "label": "chief" }),
    );
    rig.raw_ok("pane.close", json!({ "pane_id": pane }));
    // SAFETY: pid was returned by signal_daemon, which verified it.
    unsafe { libc::kill(pid as i32, libc::SIGCONT) };

    rig.wait_until("the final rename to be recorded", WAIT, |r| {
        r.row("seats", "chief", "active").is_some()
    });
    rig.wait_until("the closed pane's clone to retire", WAIT, |r| {
        r.show(&clones[1].id)["lifecycle"].as_str() == Some("retired")
    });
    rig.wait_settled();
    assert_eq!(rig.seat("chief").id, foreman.id);
    assert_eq!(
        rig.show(&clones[0].id)["lifecycle"].as_str(),
        Some("active")
    );
    assert_eq!(rig.tab_labelled("chief").map(|t| t.1.panes.len()), Some(1));
    assert!(rig.pane_of(&clones[0].id).is_some());
    assert_eq!(retired_seat_count(&rig), 0);
}

#[test]
fn e2e_dropped_rename_recovered() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    let seat = rig.seat("foreman");
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    // The daemon is stopped for the rename (its event is not handled by this process), then killed: the
    // only way left to learn of the rename is the snapshot diff of the next incarnation.
    rig.signal_daemon(libc::SIGSTOP);
    rig.raw_ok(
        "tab.rename",
        json!({ "tab_id": tab.id.0, "label": "chief" }),
    );
    rig.kill_daemon();
    rig.start_daemon();

    rig.wait_until("the rename to be recovered", WAIT, |r| {
        r.row("seats", "chief", "active").is_some()
    });
    rig.wait_settled();
    assert_eq!(rig.seat("chief").id, seat.id);
    assert_eq!(rig.tab_labelled("chief").map(|t| t.1.id), Some(tab.id));
    let renames = rig
        .ops_text()
        .lines()
        .filter(|l| l.contains("observed") && l.contains("new=chief"))
        .count();
    assert_eq!(renames, 1, "recorded once:\n{}", rig.ops_text());
}

/// foreman (two clones) and bar (one), restarted under Herdr: returns each clone with the pane it had.
fn restarted_rig() -> Option<(E2e, Vec<(Row, String)>)> {
    let mut rig = E2e::start()?;
    let (foreman, bar) = two_seats(&rig);
    rig.plan_apply(&["clone", "add", "foreman"]);
    rig.wait_until("foreman's second pane", WAIT, |r| {
        r.tab_labelled("foreman")
            .is_some_and(|t| t.1.panes.len() == 2)
    });
    rig.wait_settled();
    let before: Vec<(Row, String)> = [&foreman, &bar]
        .iter()
        .flat_map(|s| rig.clones_of(&s.id))
        .map(|c| {
            let pane = rig.pane_of(&c.id).expect("pane before the restart").id.0;
            (c, pane)
        })
        .collect();
    rig.herdr_restart();
    Some((rig, before))
}

#[test]
fn e2e_herdr_restart_rebind_no_mass_retirement_no_duplicates() {
    let Some((rig, clones)) = restarted_rig() else {
        return;
    };
    // Herdr keeps labels but not tokens (docs/herdr-spikes.md, spike 3): the daemon cannot match anything
    // and, per spec §4.3.2, marks the bound objects `unknown` instead of retiring or recreating them.
    rig.wait_until("every clone to be marked unknown", WAIT, |r| {
        clones
            .iter()
            .all(|(c, _)| tstr(&r.show(&c.id), "runtime.availability") == "unknown")
    });
    rig.wait_settled();
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(
        retired_seat_count(&rig),
        0,
        "no mass retirement: {:?}",
        rig.rows("seats")
            .iter()
            .map(|r| (&r.name, &r.state))
            .collect::<Vec<_>>()
    );
    for (c, _) in &clones {
        assert_eq!(
            rig.show(&c.id)["lifecycle"].as_str(),
            Some("active"),
            "clone {}",
            c.name
        );
    }
    assert_eq!(
        tab_labels(&rig),
        ["1", "bar", "foreman"],
        "no tab was recreated or duplicated"
    );
    assert_eq!(
        pane_infos(&rig.snapshot()).len(),
        4,
        "no pane was recreated"
    );
    assert!(
        !rig.ops_text().lines().any(|l| l.contains("rule=")),
        "nothing was observed as closed:\n{}",
        rig.ops_text()
    );

    // The explicit rebind plan re-adopts each pane: the binding follows the pane and the clone is present again.
    for (c, pane) in &clones {
        rig.plan_apply(&["clone", "rebind", &c.id, "--pane", pane]);
    }
    rig.wait_settled();
    for (c, pane) in &clones {
        let s = rig.show(&c.id);
        assert_eq!(
            tstr(&s, "runtime.availability"),
            "present",
            "clone {}",
            c.name
        );
        assert_eq!(&tstr(&s, "runtime.bound.pane_id"), pane, "clone {}", c.name);
    }
    assert_eq!(
        tab_labels(&rig),
        ["1", "bar", "foreman"],
        "rebinding creates nothing"
    );
    assert_eq!(pane_infos(&rig.snapshot()).len(), 4);
    assert_eq!(retired_seat_count(&rig), 0);
}

/// D2 regression: after a Herdr restart dropped the pane tokens, `clone rebind` binds the pane and the reconciler
/// re-stamps the Herdr pane metadata token with a fresh effect identity (spec §4.2).
#[test]
fn e2e_rebind_restamps_token_after_herdr_restart() {
    let Some((rig, clones)) = restarted_rig() else {
        return;
    };
    rig.wait_until("every clone to be marked unknown", WAIT, |r| {
        clones
            .iter()
            .all(|(c, _)| tstr(&r.show(&c.id), "runtime.availability") == "unknown")
    });
    for (c, pane) in &clones {
        rig.plan_apply(&["clone", "rebind", &c.id, "--pane", pane]);
    }
    rig.wait_until("every pane to carry its clone's token again", WAIT, |r| {
        clones
            .iter()
            .all(|(c, p)| r.pane_of(&c.id).is_some_and(|lp| &lp.id.0 == p))
    });
}

fn json_of(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not json ({e}): {text}"))
}

#[test]
fn e2e_seat_resolution_in_bound_pane() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    let seat = rig.seat("foreman");
    let clone = rig.live_clone(&seat.id);
    let pane = rig.pane_of(&clone.id).unwrap().id.0;

    let r = json_of(&pane_capture(
        &rig,
        &pane,
        "seat.json",
        &format!("{BIN} seat --json"),
    ));
    assert_eq!(r["resolution"]["status"], "bound", "{r}");
    assert_eq!(r["resolution"]["seat"], seat.id.as_str());
    assert_eq!(r["resolution"]["clone"], clone.id.as_str());
    assert_eq!(r["paths"]["seat_folder"], seat.path.to_str().unwrap());
    assert_eq!(r["reload_required"], false);
    let text = pane_capture(&rig, &pane, "seat.txt", &format!("{BIN} seat"));
    assert!(text.contains(&clone.id) && text.contains("bound"), "{text}");

    // A pane that graph did not create and that is bound to nothing is not resolved to a seat.
    let stranger = rig.plain_pane("stranger");
    let r = json_of(&pane_capture(
        &rig,
        &stranger,
        "seat-stranger.json",
        &format!("{BIN} seat --json"),
    ));
    assert_ne!(r["resolution"]["status"], "bound", "{r}");
    assert!(r["resolution"]["clone"].is_null(), "{r}");
}

#[test]
fn e2e_restored_panes_resolve_without_env() {
    let Some((rig, clones)) = restarted_rig() else {
        return;
    };
    rig.wait_until("every clone to be marked unknown", WAIT, |r| {
        clones
            .iter()
            .all(|(c, _)| tstr(&r.show(&c.id), "runtime.availability") == "unknown")
    });
    for (c, pane) in &clones {
        rig.plan_apply(&["clone", "rebind", &c.id, "--pane", pane]);
    }
    rig.wait_settled();
    let (target, pane) = &clones[0];
    // The restored shell has no creation-time HERDR_GRAPH* env; it keeps only what Herdr gives every pane.
    let bare = format!(
        "env -u HERDR_GRAPH -u HERDR_GRAPH_SEAT -u HERDR_GRAPH_CLONE -u HERDR_GRAPH_INSTANCE {BIN}"
    );
    let r = json_of(&pane_capture(
        &rig,
        pane,
        "seat-restored.json",
        &format!("{bare} seat --json"),
    ));
    assert_eq!(r["resolution"]["status"], "bound", "{r}");
    assert_eq!(
        r["resolution"]["clone"],
        target.id.as_str(),
        "resolved through HERDR_PANE_ID and the binding: {r}"
    );

    let payload = rig.herdr.root.join("hook-restored.json");
    std::fs::write(
        &payload,
        r#"{"session_id":"restored-1","cwd":"/tmp","source":"resume"}"#,
    )
    .unwrap();
    let out = pane_capture(
        &rig,
        pane,
        "report-restored.txt",
        &format!(
            "{bare} session-report --from-hook claude < {}; echo rc=$?",
            payload.display()
        ),
    );
    assert!(out.contains("rc=0"), "{out}");
    rig.wait_until(
        "the session to be recorded on the restored clone",
        WAIT,
        |r| {
            r.show(&target.id)["sessions"].as_array().is_some_and(|s| {
                s.iter()
                    .any(|s| s["native_session_id"].as_str() == Some("restored-1"))
            })
        },
    );
}

#[test]
fn e2e_summary_lands_in_archived_seat_folder() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    rig.plan_apply(&[
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
    ]);
    rig.wait_until("scribe's tab", WAIT, |r| r.tab_labelled("scribe").is_some());
    rig.wait_settled();
    let seat = rig.seat("foreman");
    let clone = rig.live_clone(&seat.id);
    let pane = rig.pane_of(&clone.id).unwrap().id.0;
    assert_eq!(
        tstr(&rig.show(&seat.id), "overrides.summaries"),
        "",
        "summaries come from the default (true for an ordinary seat)"
    );

    // The pane reports a native session with a transcript, as Claude's SessionStart hook does.
    let transcript = rig.herdr.root.join("session.jsonl");
    let body = "{\"role\":\"user\",\"text\":\"hello\"}\n{\"role\":\"assistant\",\"text\":\"hi\"}\n";
    std::fs::write(&transcript, body).unwrap();
    let payload = rig.herdr.root.join("hook.json");
    std::fs::write(
        &payload,
        format!(
            r#"{{"session_id":"sess-1","transcript_path":"{}","cwd":"/tmp","source":"startup"}}"#,
            transcript.display()
        ),
    )
    .unwrap();
    pane_capture(
        &rig,
        &pane,
        "report.txt",
        &format!(
            "{BIN} session-report --from-hook claude < {}; echo rc=$?",
            payload.display()
        ),
    );
    rig.wait_until("a pending processing request", WAIT, |r| {
        r.cli_ok(&["request", "list", "--pending"])
            .contains("pending")
    });
    let line = rig.cli_ok(&["request", "list", "--pending"]);
    let rq = line.split_whitespace().next().unwrap().to_owned();
    assert!(
        line.contains("scribe") && line.contains(&format!("0-{}", body.len())),
        "routed to the summarizer, covering the transcript: {line}"
    );

    // The seat's tab closes: the seat retires and its folder moves to the archive.
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    rig.raw_ok("tab.close", json!({ "tab_id": tab.id.0 }));
    rig.wait_until("the seat to retire", WAIT, |r| {
        r.seat_state(&seat.id) == "retired"
    });
    let archived = rig.row("seats", "foreman", "retired").unwrap().path;
    assert!(
        archived.to_string_lossy().contains("/archive/"),
        "{}",
        archived.display()
    );

    // No real summarizer runs here: the test plays it through the CLI (ack, write, complete).
    assert_eq!(
        rig.cli_ok(&["request", "ack", &rq]).trim(),
        format!("{rq} dispatched")
    );
    let summary = rig.herdr.root.join("summary.md");
    std::fs::write(&summary, "# hello\nthe user said hello\n").unwrap();
    let out = rig.cli_ok(&[
        "content",
        "write",
        "--object",
        &seat.id,
        "--rel",
        "summaries/sess-1.md",
        "--from",
        summary.to_str().unwrap(),
    ]);
    assert!(out.contains("committed"), "{out}");
    assert_eq!(
        rig.cli_ok(&[
            "request",
            "complete",
            &rq,
            "--output",
            "summaries/sess-1.md",
            "--covered",
            &format!("0-{}", body.len())
        ])
        .trim(),
        format!("{rq} completed")
    );

    let landed = archived.join("summaries/sess-1.md");
    assert_eq!(
        std::fs::read_to_string(&landed).unwrap_or_else(|e| panic!("{}: {e}", landed.display())),
        "# hello\nthe user said hello\n"
    );
    assert!(
        !rig.cli_ok(&["request", "list", "--pending"]).contains(&rq),
        "the request is no longer pending"
    );
    let live_dir = archived
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("seats/foreman");
    assert!(
        !live_dir.exists(),
        "nothing was written to the old live folder {}",
        live_dir.display()
    );
}

#[test]
fn e2e_seat_override_model_change_recorded_shell_not_replaced() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    let seat = rig.seat("foreman");
    let clone = rig.live_clone(&seat.id);
    let pane_before = rig.pane_of(&clone.id).unwrap().id.0;
    assert!(
        !rig.effects_of(&clone.id)
            .iter()
            .any(|(k, _)| k == "replace_session")
    );

    let plan = rig.plan_json(&["seat", "override", "foreman", "--model", "sonnet"]);
    assert!(
        plan["rendered"].as_str().unwrap().contains("seat.override"),
        "{}",
        plan["rendered"]
    );
    let (ok, out) = rig.apply_plan(&plan);
    assert!(ok, "{out}");
    rig.wait_settled();

    assert_eq!(tstr(&rig.show(&seat.id), "overrides.model"), "sonnet");
    assert_eq!(
        tstr(&rig.show(&seat.id), "overrides.harness"),
        "shell",
        "the harness override is untouched"
    );
    // A shell clone has no occupant and Herdr detects no agent in it, so the reconciler plans no session
    // replacement (it needs a running agent whose launch shape differs from the new configuration). The real
    // replacement is asserted at tier 2 (src/reconcile/tests.rs) and tier 4; tier 3 asserts that the override is
    // recorded and that nothing is disturbed.
    std::thread::sleep(Duration::from_secs(4));
    assert!(
        !rig.effects_of(&clone.id)
            .iter()
            .any(|(k, _)| k == "replace_session"),
        "{:?}",
        rig.effects_of(&clone.id)
    );
    assert_eq!(
        rig.pane_of(&clone.id).unwrap().id.0,
        pane_before,
        "same pane, nothing recreated"
    );
    assert_eq!(rig.tab_labelled("foreman").unwrap().1.panes.len(), 1);
    assert_eq!(rig.show(&clone.id)["lifecycle"].as_str(), Some("active"));
    // Clearing the override is a plan of its own.
    rig.plan_apply(&["seat", "override", "foreman", "--clear", "model"]);
    assert_eq!(tstr(&rig.show(&seat.id), "overrides.model"), "");
}

#[test]
fn e2e_participation_join_leave_scopes() {
    let rig = rig!();
    rig.create_teamspace_with_seat("t", "foreman");
    let seat = rig.seat("foreman");
    let clone = rig.live_clone(&seat.id);
    let in_seat = |r: &E2e| {
        r.show(&seat.id)["participation"]["seat_wide"]
            .to_string()
            .contains("thr-7")
    };
    let opted_out = |r: &E2e| r.show(&clone.id)["opt_outs"].to_string().contains("thr-7");

    // Seat scope: join adds the thread to the seat, a second join is refused, leave removes it.
    rig.plan_apply(&[
        "participation",
        "join",
        "thr-7",
        "--scope",
        "seat",
        "--seat",
        "foreman",
    ]);
    assert!(in_seat(&rig));
    let again = rig.cli(&[
        "plan",
        "--json",
        "participation",
        "join",
        "thr-7",
        "--scope",
        "seat",
        "--seat",
        "foreman",
    ]);
    assert!(
        !again.status.success() && stderr(&again).contains("already participates"),
        "{}{}",
        stdout(&again),
        stderr(&again)
    );
    rig.plan_apply(&[
        "participation",
        "leave",
        "thr-7",
        "--scope",
        "seat",
        "--seat",
        "foreman",
    ]);
    assert!(!in_seat(&rig));

    // Clone scope: leave records an opt-out for this clone only, join lifts it.
    rig.plan_apply(&[
        "participation",
        "join",
        "thr-7",
        "--scope",
        "seat",
        "--seat",
        "foreman",
    ]);
    rig.plan_apply(&[
        "participation",
        "leave",
        "thr-7",
        "--scope",
        "clone",
        "--clone",
        &clone.id,
    ]);
    assert!(opted_out(&rig), "{}", rig.show(&clone.id));
    assert!(
        in_seat(&rig),
        "a clone opt-out leaves the seat's participation alone"
    );
    let nothing = rig.cli(&[
        "plan",
        "--json",
        "participation",
        "leave",
        "thr-7",
        "--scope",
        "clone",
        "--clone",
        &clone.id,
    ]);
    assert!(
        !nothing.status.success() && stderr(&nothing).contains("already opted out"),
        "{}",
        stderr(&nothing)
    );
    rig.plan_apply(&[
        "participation",
        "join",
        "thr-7",
        "--scope",
        "clone",
        "--clone",
        &clone.id,
    ]);
    assert!(!opted_out(&rig));
    // Scope mix-ups are usage errors, not silent no-ops.
    let mixed = rig.cli(&[
        "plan",
        "--json",
        "participation",
        "join",
        "thr-7",
        "--scope",
        "seat",
        "--clone",
        &clone.id,
    ]);
    assert!(
        !mixed.status.success() && stderr(&mixed).contains("--clone only applies"),
        "{}",
        stderr(&mixed)
    );
}

#[test]
fn e2e_cancel_and_reassign_rejected_op() {
    let rig = rig!();
    let (foreman, bar) = two_seats(&rig);
    let clone = rig.live_clone(&foreman.id);

    // foreman's clone plans a rename of bar; bar is renamed by someone else before the plan is applied.
    let run = |args: &[&str]| rig.cli_as(foreman.id.as_str(), clone.id.as_str(), args);
    let plan = json_of(&stdout(&run(&[
        "plan", "--json", "seat", "rename", "bar", "zwei",
    ])));
    rig.plan_apply(&["seat", "rename", "bar", "quux"]);
    let o = run(&[
        "apply",
        plan["plan_id"].as_str().unwrap(),
        "--confirm",
        plan["hash"].as_str().unwrap(),
        "--confirmed-by",
        "user-relay",
    ]);
    let out = stdout(&o);
    assert!(
        !o.status.success() && out.contains("rejected"),
        "the stale plan is rejected: {out}{}",
        stderr(&o)
    );
    let op = out.split_whitespace().next().unwrap().to_owned();

    let unresolved = rig.cli_ok(&["ops", "--unresolved"]);
    assert!(
        unresolved.contains(&op) && unresolved.contains("rejected"),
        "{unresolved}"
    );
    let detail = rig.cli_ok(&["op", &op]);
    assert!(
        detail.contains(&foreman.id),
        "requested by foreman: {detail}"
    );

    // Reassign hands the op to another live seat; it stays unresolved until cancelled.
    let msg = rig.cli_ok(&["reassign", &op, "--to", &bar.id]);
    assert!(msg.contains(&op) && msg.contains(&bar.id), "{msg}");
    rig.wait_until("the requester to change", WAIT, |r| {
        r.cli_ok(&["op", &op]).contains(&bar.id)
    });
    assert!(rig.cli_ok(&["ops", "--unresolved"]).contains(&op));
    let refused = rig.cli(&["reassign", &op, "--to", "no-such-seat"]);
    assert!(!refused.status.success(), "an unknown seat is refused");

    let msg = rig.cli_ok(&["cancel", &op]);
    assert!(msg.contains("reminders stopped"), "{msg}");
    assert!(
        !rig.cli_ok(&["ops", "--unresolved"]).contains(&op),
        "a cancelled rejection is resolved"
    );
    assert!(
        rig.cli_ok(&["ops"]).contains(&op),
        "it stays in the history"
    );
}

#[test]
fn e2e_resurrect_retired_seat() {
    let rig = rig!();
    let (foreman, bar) = two_seats(&rig);
    let clone = rig.live_clone(&foreman.id);
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    rig.raw_ok("tab.close", json!({ "tab_id": tab.id.0 }));
    rig.wait_until("the seat to retire", WAIT, |r| {
        r.seat_state(&foreman.id) == "retired"
    });
    rig.wait_settled();
    let archived = rig.row("seats", "foreman", "retired").unwrap().path;
    assert!(archived.exists());

    rig.plan_apply(&["seat", "resurrect", "foreman", "--active"]);
    rig.wait_until("the seat to be active", WAIT, |r| {
        r.seat_state(&foreman.id) == "active"
    });
    rig.wait_until("a tab for the resurrected seat", WAIT, |r| {
        r.tab_labelled("foreman").is_some()
    });
    rig.wait_settled();
    let live = rig.seat("foreman");
    assert_eq!(live.id, foreman.id, "the same seat, not a new one");
    assert!(
        live.path.exists() && !archived.exists(),
        "moved back out of the archive: {} / {}",
        live.path.display(),
        archived.display()
    );
    let (_, tab) = rig.tab_labelled("foreman").unwrap();
    assert!(!tab.panes.is_empty());
    assert_eq!(
        rig.show(&clone.id)["lifecycle"].as_str(),
        Some("active"),
        "clone {}",
        rig.show(&clone.id)
    );
    assert_eq!(rig.seat_state(&bar.id), "active");
    assert_eq!(retired_seat_count(&rig), 0);
}

#[test]
fn e2e_plugin_link_status_action_single_daemon() {
    let Some(mut rig) = E2e::start_without_daemon() else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let build = Command::new(repo.join("scripts/build.sh"))
        .current_dir(repo)
        .output()
        .unwrap(); // isolation-ok: cargo build of this crate
    assert!(
        build.status.success(),
        "scripts/build.sh: {}",
        stderr(&build)
    );
    let bin = repo.join("bin/herdr-graph");
    assert!(bin.is_file(), "{} was not built", bin.display());

    // The plugin's own state dir, never the rig's shared one; the instance is found the way a real install
    // finds it: `instance = ...` in the plugin's config dir.
    let state = rig.herdr.root.join("plugin-state");
    std::fs::create_dir_all(&state).unwrap();
    // `herdr` with exactly the private env (so it talks to the private socket) and the plugin's state dir.
    let (herdr_path, env, root) = (
        rig.herdr.herdr_path().to_path_buf(),
        rig.herdr.env.clone(),
        rig.herdr.root.clone(),
    );
    let herdr = move |args: &[&str]| -> Output {
        Command::new(&herdr_path)
            .env_clear()
            .envs(env.iter().cloned())
            .env("HERDR_PLUGIN_STATE_DIR", &state)
            .current_dir(&root)
            .args(args)
            .output()
            .unwrap() // isolation-ok: plugin state dir under the private root
    };
    let link = herdr(&["plugin", "link", repo.to_str().unwrap()]);
    assert!(
        link.status.success(),
        "plugin link failed: {}",
        stderr(&link)
    );
    let listed = stdout(&herdr(&["plugin", "list"]));
    assert!(listed.contains("herdr-graph"), "{listed}");
    let cfg_dir = stdout(&herdr(&["plugin", "config-dir", "herdr-graph"]))
        .trim()
        .to_owned();
    assert!(
        Path::new(&cfg_dir).starts_with(&rig.herdr.root),
        "the plugin config dir is private: {cfg_dir}"
    );
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(
        Path::new(&cfg_dir).join("config.toml"),
        format!("instance = \"{}\"\n", rig.instance.display()),
    )
    .unwrap();
    let actions = stdout(&herdr(&["plugin", "action", "list"]));
    assert!(
        actions.contains("\"action_id\":\"status\"")
            && actions.contains("\"action_id\":\"doctor\""),
        "{actions}"
    );
    rig.ensure_daemon_down_is_quiet();

    // [[startup]] runs when the server starts: it launches exactly one daemon.
    rig.herdr_restart();
    rig.wait_until("the startup hook's daemon", WAIT, |r| {
        r.daemon_pid().is_some_and(E2e::pid_alive)
    });
    let pid = rig.daemon_pid().unwrap();
    let ps = Command::new("/bin/ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    assert!(
        stdout(&ps).contains(&format!("{}/bin/herdr-graph daemon", repo.display())),
        "the hook ran the plugin's binary: {}",
        stdout(&ps)
    );
    let again = rig
        .command(bin.to_str().unwrap())
        .args(["daemon", "--ensure"])
        .output()
        .unwrap();
    assert!(
        stdout(&again).contains(&format!("already running (pid {pid})")),
        "{}{}",
        stdout(&again),
        stderr(&again)
    );
    assert_eq!(
        rig.daemon_pid(),
        Some(pid),
        "the lock pid is stable after a second --ensure"
    );
    let all = Command::new("/bin/ps")
        .args(["-axo", "pid=,command="])
        .output()
        .unwrap();
    let daemons = stdout(&all)
        .lines()
        .filter(|l| l.contains(&format!("{}/bin/herdr-graph daemon", repo.display())))
        .count();
    assert_eq!(
        daemons,
        1,
        "exactly one daemon runs for this instance:\n{}",
        stdout(&all)
    );

    // The startup command's own log says it started that daemon, and the status action reports it.
    let logs = |want: usize| -> Vec<Value> {
        let mut found = Vec::new();
        for _ in 0..100 {
            let v = json_of(&stdout(&herdr(&[
                "plugin",
                "log",
                "list",
                "--plugin",
                "herdr-graph",
            ])));
            found = v["result"]["logs"].as_array().cloned().unwrap_or_default();
            if found.len() >= want && found.iter().all(|l| l["status"] != "running") {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        found
    };
    let startup = logs(1)
        .into_iter()
        .find(|l| l["event"] == "startup")
        .expect("a startup log");
    assert_eq!(startup["exit_code"], 0, "{startup}");
    assert_eq!(
        startup["stdout"].as_str().unwrap().trim(),
        format!("daemon started (pid {pid})"),
        "{startup}"
    );

    let invoked = json_of(&stdout(&herdr(&[
        "plugin",
        "action",
        "invoke",
        "status",
        "--plugin",
        "herdr-graph",
    ])));
    assert_eq!(
        invoked["result"]["type"], "plugin_action_invoked",
        "{invoked}"
    );
    let action = logs(2)
        .into_iter()
        .find(|l| l["action_id"] == "status")
        .expect("the status action's log");
    assert_eq!(action["exit_code"], 0, "{action}");
    let printed = action["stdout"].as_str().unwrap();
    assert!(
        printed.contains(&format!("daemon: running (pid {pid}")),
        "status names the one daemon: {printed}"
    );
    assert!(
        printed.contains(&format!("instance: {}", rig.instance.display())),
        "{printed}"
    );
    assert_eq!(
        rig.daemon_pid(),
        Some(pid),
        "running the action started no second daemon"
    );
}

// FLOWS-END
