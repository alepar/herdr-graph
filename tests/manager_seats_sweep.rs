//! Root integration sweep for the seat failure path and manager seats (hg-k4l.22): the real binary under
//! `TestRoot`, against a daemon with fake services (`HG_TEST_FAKE_SERVICES`, feature `test-support`) and a
//! Herdr socket that accepts and ignores connections. Plans go through the relay path of the root sweep:
//! `plan --json`, then `apply --confirm <hash> --confirmed-by user-relay`.
#![cfg(feature = "test-support")]
mod support;

use herdr_graph::cli::daemon_error::{NOT_IN_HERDR, RETRY_LATER};
use herdr_graph::cli::node_manager_warning::warning_text;
use herdr_graph::plan::managers::NODE_MANAGER_SET_USAGE;
use serde_json::Value;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use support::isolated::TestRoot;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");

struct Rig {
    /// Dropped first: shuts the daemon down and sweeps every process carrying its marker.
    root: TestRoot,
    _listener: UnixListener,
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn text(o: &Output) -> String {
    format!("{}{}", stdout(o), stderr(o))
}

/// One seat record read from the working tree: `teamspaces/<ts>/seats/<name>/seat.toml`.
struct Seat {
    id: String,
    manager: Option<String>,
}

impl Rig {
    fn new() -> Self {
        let root = TestRoot::new();
        let listener = UnixListener::bind(root.herdr_socket()).unwrap();
        let l2 = listener.try_clone().unwrap();
        std::thread::spawn(move || for _conn in l2.incoming() {});
        Rig {
            root,
            _listener: listener,
        }
    }

    fn instance(&self) -> PathBuf {
        self.root.instance()
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = self.root.command(BIN);
        c.args(args).env("HG_TEST_FAKE_SERVICES", "1");
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let o = self.run(args);
        assert!(o.status.success(), "{args:?} failed: {}", text(&o));
        stdout(&o)
    }

    fn init_with_daemon(&self, with_examples: bool) {
        let inst = self.instance();
        let mut args = vec!["init", inst.to_str().unwrap()];
        if with_examples {
            args.push("--with-examples");
        }
        self.ok(&args);
        self.ok(&["daemon", "--ensure"]);
    }

    /// `plan --json <words>` then the relayed apply; returns the plan reply.
    fn plan_apply(&self, words: &[&str]) -> Value {
        let mut args = vec!["plan", "--json"];
        args.extend_from_slice(words);
        let plan: Value = serde_json::from_str(&self.ok(&args))
            .unwrap_or_else(|e| panic!("plan {words:?} is not json: {e}"));
        let (id, hash) = (
            plan["plan_id"].as_str().expect("plan_id"),
            plan["hash"].as_str().expect("hash"),
        );
        let out = self.run(&[
            "apply",
            id,
            "--confirm",
            hash,
            "--confirmed-by",
            "user-relay",
        ]);
        assert!(
            out.status.success(),
            "apply {words:?} did not commit: {}",
            text(&out)
        );
        plan
    }

    /// All seat records in the working tree, by seat name.
    fn seats(&self) -> Vec<(String, Seat)> {
        let mut out = Vec::new();
        let teamspaces = self.instance().join("teamspaces");
        for ts in std::fs::read_dir(&teamspaces)
            .into_iter()
            .flatten()
            .flatten()
        {
            let seats = ts.path().join("seats");
            for s in std::fs::read_dir(&seats).into_iter().flatten().flatten() {
                let Ok(raw) = std::fs::read_to_string(s.path().join("seat.toml")) else {
                    continue;
                };
                let v: toml::Table = raw.parse().unwrap();
                out.push((
                    s.file_name().to_string_lossy().into_owned(),
                    Seat {
                        id: v["id"].as_str().unwrap().to_owned(),
                        manager: v.get("manager").and_then(|m| m.as_str()).map(str::to_owned),
                    },
                ));
            }
        }
        out
    }

    fn seat(&self, name: &str) -> Seat {
        self.seats()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no seat {name} in the working tree"))
            .1
    }

    fn node_manager_seat(&self) -> Option<String> {
        let raw = std::fs::read_to_string(self.instance().join("graph.toml")).unwrap();
        let v: toml::Table = raw.parse().unwrap();
        v.get("node_manager_seat")
            .and_then(|m| m.as_str())
            .map(str::to_owned)
    }

    /// The Herdr pane the seat's active clone is bound to, waited for (the reconciler binds it after the
    /// activating plan commits).
    fn bind_pane_of(&self, seat_id: &str) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(40);
        loop {
            if let Some(p) = self.bound_pane(seat_id) {
                return p;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "seat {seat_id} never got a bound pane"
            );
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    }

    fn bound_pane(&self, seat_id: &str) -> Option<String> {
        let (name, _) = self.seats().into_iter().find(|(_, s)| s.id == seat_id)?;
        let teamspaces = self.instance().join("teamspaces");
        for ts in std::fs::read_dir(&teamspaces).ok()?.flatten() {
            let clones = ts.path().join("seats").join(&name).join("clones");
            for c in std::fs::read_dir(&clones).into_iter().flatten().flatten() {
                let Ok(raw) = std::fs::read_to_string(c.path().join("clone.toml")) else {
                    continue;
                };
                let v: toml::Table = raw.parse().ok()?;
                let pane = v
                    .get("runtime")
                    .and_then(|r| r.get("bound"))
                    .and_then(|b| b.get("pane_id"))
                    .and_then(|p| p.as_str());
                if let Some(p) = pane {
                    return Some(p.to_owned());
                }
            }
        }
        None
    }

    /// `seat --json` as the agent in `pane` runs it.
    fn seat_json(&self, pane: &str) -> Value {
        let out = self
            .cmd(&["seat", "--json"])
            .env("HERDR_PANE_ID", pane)
            .output()
            .unwrap();
        assert!(out.status.success(), "seat --json: {}", text(&out));
        serde_json::from_str(&stdout(&out)).unwrap()
    }

    /// Shut the daemon down and wait until its process is gone (a successor would otherwise race its exit).
    fn stop_daemon(&self) {
        let status = stdout(&self.run(&["status"]));
        let pid: u32 = status
            .split("pid ")
            .nth(1)
            .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|n| n.parse().ok())
            .expect("a running daemon with a pid");
        let sock = herdr_graph::config::InstancePaths::new(&self.instance()).socket;
        let mut c =
            herdr_graph::daemon::client::Client::connect(&sock, std::time::Duration::from_secs(5))
                .expect("connect to the daemon");
        let _ = c.call("shutdown", serde_json::json!({}));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while Path::new(&format!("/proc/{pid}")).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "daemon {pid} did not exit"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    /// The newest undoable action's id.
    /// The newest undoable retirement (observed cascades and hydrations are undoable too and land asynchronously).
    fn latest_retire(&self) -> String {
        let v: Value = serde_json::from_str(&self.ok(&["undo", "--json"])).unwrap();
        let first = v
            .as_array()
            .and_then(|a| a.iter().find(|c| c["kind"] == "retire"))
            .expect("a retire candidate");
        first["act"]
            .as_str()
            .or_else(|| first["id"].as_str())
            .unwrap_or_else(|| panic!("no act id in {first}"))
            .to_owned()
    }
}

/// Commit `files` to the instance's `main` (direct git, for states no plan produces). The daemon must be stopped.
fn commit(dir: &Path, files: &[(&str, String)]) {
    let repo = git2::Repository::open(dir).unwrap();
    let mut index = repo.index().unwrap();
    for (rel, body) in files {
        let target = dir.join(rel);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, body).unwrap();
        index.add_path(Path::new(rel)).unwrap();
    }
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = herdr_graph::store::git::graph_signature().unwrap();
    repo.commit(Some("refs/heads/main"), &sig, &sig, "t", &tree, &[&parent])
        .unwrap();
}

// =============================================================================================
// /seat failure flows
// =============================================================================================

#[test]
fn seat_failure_flows() {
    let rig = Rig::new();
    rig.init_with_daemon(true);
    rig.plan_apply(&["teamspace", "create", "hq", "--active"]);
    rig.plan_apply(&[
        "application",
        "apply",
        "office",
        "--teamspace",
        "hq",
        "--name",
        "office",
    ]);
    rig.plan_apply(&[
        "seat",
        "create",
        "dev",
        "--teamspace",
        "hq",
        "--active",
        "--harness",
        "shell",
    ]);
    let nm_id = rig
        .node_manager_seat()
        .expect("office assigned a node manager");

    // Daemon unavailable (the running daemon serves another Herdr socket, so `hello` refuses it): exit 5 with the
    // seat text.
    let dead = rig.root.home().join("dead.sock");
    let out = rig
        .cmd(&["seat"])
        .env("HERDR_SOCKET_PATH", &dead)
        .output()
        .unwrap();
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(5), "{err}");
    assert!(
        err.contains("herdr-graph: graph daemon unavailable:"),
        "{err}"
    );
    assert!(
        err.contains(RETRY_LATER) && err.contains("Do not guess your seat."),
        "{err}"
    );

    // Not inside Herdr: exit 2.
    let out = rig
        .cmd(&["seat"])
        .env_remove("HERDR_SOCKET_PATH")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert_eq!(stderr(&out).trim(), NOT_IN_HERDR);

    // Integrity: a bound seat whose template_ref names a template that is not there. The daemon is stopped
    // while the record is committed directly (no plan writes a dangling reference).
    let dev = rig.seat("dev");
    let ghost_tpl = herdr_graph::model::TemplateId::new();
    let ghost_member = herdr_graph::model::MemberId::new();
    let pane = rig.bind_pane_of(&dev.id);
    rig.stop_daemon();
    let path = "teamspaces/hq/seats/dev/seat.toml";
    let mut body = std::fs::read_to_string(rig.instance().join(path)).unwrap();
    body.push_str(&format!(
        "\n[template_ref]\ntemplate = \"{ghost_tpl}\"\nmember = \"{ghost_member}\"\n"
    ));
    commit(&rig.instance(), &[(path, body)]);

    let out = rig
        .cmd(&["seat", "--json"])
        .env("HERDR_PANE_ID", &pane)
        .output()
        .unwrap();
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(6), "{err}");
    assert!(err.contains("herdr-graph: graph integrity error:"), "{err}");
    assert!(err.contains(&ghost_tpl.to_string()), "{err}");
    assert!(
        err.contains(&format!(
            "report it to the node manager seat main ({nm_id})"
        )),
        "names the node manager: {err}"
    );
}

// =============================================================================================
// manager flow on a fresh graph
// =============================================================================================

#[test]
fn manager_flow_on_fresh_graph() {
    let rig = Rig::new();
    rig.init_with_daemon(true);
    rig.plan_apply(&["teamspace", "create", "hq", "--active"]);
    rig.plan_apply(&[
        "application",
        "apply",
        "office",
        "--teamspace",
        "hq",
        "--name",
        "office",
    ]);

    // The office's first seat is the node manager.
    let main = rig.seat("main");
    assert_eq!(rig.node_manager_seat().as_deref(), Some(main.id.as_str()));
    let shown = rig.ok(&["show", "graph.toml"]);
    assert!(shown.contains(&main.id), "show graph names main: {shown}");

    for name in ["lead", "dev", "aux"] {
        rig.plan_apply(&[
            "seat",
            "create",
            name,
            "--teamspace",
            "hq",
            "--active",
            "--harness",
            "shell",
        ]);
    }
    let (lead, dev) = (rig.seat("lead"), rig.seat("dev"));
    assert_eq!(dev.manager, None, "creating a seat does not set a manager");
    rig.plan_apply(&["manager", "set", "dev", "lead"]);
    rig.plan_apply(&["manager", "set", "aux", "main"]);
    assert_eq!(rig.seat("dev").manager.as_deref(), Some(lead.id.as_str()));

    // who / show chains: dev -> lead -> main (lead has no manager, so the node manager is its default).
    let chain = format!("manager: lead ({}) -> main ({})", lead.id, main.id);
    let who = rig.ok(&["who", &dev.id]);
    assert!(who.contains(&chain), "who: {who}");
    let shown = rig.ok(&["show", &dev.id]);
    assert!(shown.contains(&chain), "show: {shown}");

    // seat.resolve from dev's pane: manager, node manager, reports.
    let pane = rig.bind_pane_of(&dev.id);
    let reply = rig.seat_json(&pane);
    assert_eq!(reply["manager"]["seat"], lead.id, "{reply}");
    assert_eq!(reply["node_manager"]["seat"], main.id, "{reply}");
    let lead_pane = rig.bind_pane_of(&lead.id);
    let lead_reply = rig.seat_json(&lead_pane);
    let reports: Vec<&str> = lead_reply["reports"]
        .as_array()
        .expect("reports")
        .iter()
        .filter_map(|r| r["seat"].as_str())
        .collect();
    assert_eq!(reports, [dev.id.as_str()], "{lead_reply}");

    // Retire the middle manager: dev is reparented to lead's manager (the node manager).
    rig.plan_apply(&["seat", "retire", "lead"]);
    assert_eq!(rig.seat("dev").manager.as_deref(), Some(main.id.as_str()));
    // Undo restores the pointer.
    let act = rig.latest_retire();
    rig.plan_apply(&["undo", &act]);
    assert_eq!(rig.seat("dev").manager.as_deref(), Some(lead.id.as_str()));

    // Moving the node-manager role rewrites the pointers that named the old holder.
    rig.plan_apply(&["node-manager", "set", "lead"]);
    assert_eq!(rig.node_manager_seat().as_deref(), Some(lead.id.as_str()));
    assert_eq!(rig.seat("main").manager.as_deref(), Some(lead.id.as_str()));
    assert_eq!(rig.seat("aux").manager.as_deref(), Some(lead.id.as_str()));

    // The node manager can be neither retired nor have its teamspace retired.
    for words in [
        &["seat", "retire", "lead"][..],
        &["teamspace", "retire", "hq"][..],
    ] {
        let mut args = vec!["plan", "--json"];
        args.extend_from_slice(words);
        let out = rig.run(&args);
        assert!(
            !out.status.success(),
            "{words:?} was planned: {}",
            text(&out)
        );
        assert!(
            text(&out).contains(NODE_MANAGER_SET_USAGE),
            "{words:?}: {}",
            text(&out)
        );
    }
}

// =============================================================================================
// unassigned graph
// =============================================================================================

#[test]
fn unassigned_warning_flow() {
    const TS: &str = "ts_01JABCDEFGHJKMNPQRSTVWXYZ0";
    const SEAT: &str = "st_01JABCDEFGHJKMNPQRSTVWXYZ0";
    let rig = Rig::new();
    let inst = rig.instance();
    rig.ok(&["init", inst.to_str().unwrap()]);
    commit(
        &inst,
        &[
            (
                "teamspaces/t/teamspace.toml",
                format!(
                    "schema = 1\nid = \"{TS}\"\nrev = 1\nname = \"t\"\nlifecycle = \"active\"\n"
                ),
            ),
            (
                "teamspaces/t/seats/s/seat.toml",
                format!(
                    "schema = 1\nid = \"{SEAT}\"\nrev = 1\nname = \"s\"\nteamspace = \"{TS}\"\nlifecycle = \"active\"\n"
                ),
            ),
        ],
    );

    let before = rig.run(&["list", "seats"]);
    assert_eq!(
        stderr(&before).matches(&warning_text()).count(),
        1,
        "{}",
        stderr(&before)
    );

    rig.ok(&["daemon", "--ensure"]);
    rig.plan_apply(&["node-manager", "set", "s"]);
    assert_eq!(rig.node_manager_seat().as_deref(), Some(SEAT));
    let after = rig.run(&["list", "seats"]);
    assert!(after.status.success(), "{}", text(&after));
    assert!(
        !stderr(&after).contains(&warning_text()),
        "{}",
        stderr(&after)
    );
    assert_eq!(stdout(&after), stdout(&before));
}

// =============================================================================================
// daemonless reads
// =============================================================================================

#[test]
fn daemonless_reads_exit_5() {
    let rig = Rig::new();
    let inst = rig.instance();
    rig.ok(&["init", inst.to_str().unwrap()]);
    let op = herdr_graph::model::OpId::new().to_string();
    for args in [
        vec!["who", "p1"],
        vec!["ops"],
        vec!["op", &op],
        vec!["request", "list"],
        vec!["undo", "--list"],
    ] {
        let out = rig.run(&args);
        let err = stderr(&out);
        assert_eq!(out.status.code(), Some(5), "{args:?}: {err}");
        assert!(err.contains("graph daemon unavailable"), "{args:?}: {err}");
        assert!(err.contains(RETRY_LATER), "{args:?}: {err}");
    }
    let status = rig.run(&["status"]);
    assert_eq!(status.status.code(), Some(0), "{}", text(&status));
    assert!(
        stdout(&status).contains("not running"),
        "{}",
        stdout(&status)
    );
}
