//! Tier-3 integration sweep of graph state inspection (hg-blw.11): every view end to end against a PRIVATE
//! Herdr server and the real daemon. Run with:
//!
//!   cargo test --features private-herdr --test inspect_sweep -- --test-threads=1
//!
//! Covers the read-only property (R12), the `--json` contract (one document, `schema: 1`, a 40-hex `head`,
//! notices on stderr), the single pinned HEAD under a concurrent writer, and the loaded-instructions chain
//! (R14) driven through the command `setup claude` installed. One rig per test; `Rig` reaps the daemon and
//! every process under its root on success, failure and panic (`PrivateHerdr` stops the server).
#![cfg(feature = "private-herdr")]

mod support;

use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use support::private_herdr::PrivateHerdr;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");
const WAIT: Duration = Duration::from_secs(40);
const SESSION: &str = "sess-r14";

struct Rig {
    herdr: PrivateHerdr,
    instance: PathBuf,
}

impl Rig {
    fn start() -> Option<Rig> {
        Self::start_with(&[])
    }

    /// A rig whose panes find a long-lived `claude` first on PATH: the stub execs a copy of `sleep` that is
    /// itself named `claude`, so Herdr sees a running Claude agent and the observer keeps the occupant.
    fn start_with_claude() -> Option<Rig> {
        let stub = "#!/bin/sh\nexec \"$(dirname \"$0\")/real/claude\" 3000\n";
        let rig = Self::start_with(&[("claude", stub)])?;
        let real = rig.herdr.root.join("bin/real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::copy("/usr/bin/sleep", real.join("claude")).unwrap();
        Some(rig)
    }

    fn start_with(stubs: &[(&str, &str)]) -> Option<Rig> {
        let herdr = match PrivateHerdr::start_with_agent_stubs(stubs) {
            Ok(h) => h,
            Err(e) if support::herdr_binary().is_none() => {
                support::skip(&format!("{e}"));
                return None;
            }
            Err(e) => panic!("private herdr failed to start: {e:#}"),
        };
        let instance = herdr.root.join("graph");
        let rig = Rig { herdr, instance };
        let out = rig.cli(&["init", rig.instance.to_str().unwrap()]);
        assert!(out.status.success(), "init: {}", stderr(&out));
        support::node_manager_opt_out(&rig.instance);
        let out = rig.cli(&["daemon", "--ensure"]);
        assert!(out.status.success(), "daemon --ensure: {}", stderr(&out));
        Some(rig)
    }

    fn command(&self, program: &str) -> Command {
        let mut c: Command = self.herdr.command(std::path::Path::new(program));
        c.env("HERDR_GRAPH_INSTANCE", &self.instance);
        c
    }

    fn cli(&self, args: &[&str]) -> Output {
        self.command(BIN).args(args).output().unwrap()
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

    /// `--json` output: the whole of stdout parsed as exactly one document, plus stderr.
    fn json(&self, args: &[&str]) -> (Value, String) {
        let mut full = args.to_vec();
        full.push("--json");
        let out = self.cli(&full);
        assert_eq!(out.status.code(), Some(0), "{full:?}: {}", stderr(&out));
        let docs: Vec<Value> = serde_json::Deserializer::from_slice(&out.stdout)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap_or_else(|e| panic!("{full:?}: stdout is not JSON: {e}: {}", stdout(&out)));
        assert_eq!(docs.len(), 1, "{full:?}: one JSON document on stdout");
        (docs.into_iter().next().unwrap(), stderr(&out))
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

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git") // isolation-ok: read-only git in the private instance
            .arg("-C")
            .arg(&self.instance)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", stderr(&out));
        stdout(&out)
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "refs/heads/main"])
            .trim()
            .to_owned()
    }

    /// Wait until the daemon has stopped committing and the derived work tree has caught up with HEAD.
    fn settle(&self) {
        let mut last = (String::new(), Instant::now());
        self.wait_until("the instance to settle", |r| {
            let head = r.head();
            if head != last.0 {
                last = (head, Instant::now());
                return false;
            }
            last.1.elapsed() >= Duration::from_millis(1500)
                && herdr_graph::writer::worktree::view_rev(&r.instance)
                    .is_some_and(|v| v.0 == last.0)
        });
    }

    /// An active teamspace `t` with one `shell` seat `foreman`; returns its clone id once the clone shows.
    fn teamspace_with_seat(&self) -> String {
        self.teamspace_with_harness("shell")
    }

    fn teamspace_with_harness(&self, harness: &str) -> String {
        self.plan_apply(&["teamspace", "create", "t", "--active"]);
        self.plan_apply(&[
            "seat",
            "create",
            "foreman",
            "--teamspace",
            "t",
            "--active",
            "--harness",
            harness,
        ]);
        let mut id = String::new();
        self.wait_until("the clone's pane binding", |r| {
            let (v, _) = r.json(&["roster"]);
            let clone = &v["teamspaces"][0]["seats"][0]["clones"][0];
            if clone["runtime"]["pane"].is_string() {
                id = clone["id"].as_str().unwrap().to_owned();
            }
            !id.is_empty()
        });
        id
    }

    /// The four facts R12 says no view may change.
    fn facts(&self) -> [String; 4] {
        [
            self.head(),
            self.git(&["rev-list", "--count", "main"]),
            self.git(&["ls-files", "-s"]),
            self.git(&["status", "--porcelain"]),
        ]
    }

    fn pane_sh(&self, pane: &str, cmd: &str) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let c = self.herdr.client();
        rt.block_on(c.request("pane.send_text", json!({ "pane_id": pane, "text": cmd })))
            .expect("pane.send_text");
        rt.block_on(c.request(
            "pane.send_keys",
            json!({ "pane_id": pane, "keys": ["Enter"] }),
        ))
        .expect("pane.send_keys");
    }

    fn set_file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.herdr.root.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    fn set_instruction(&self, scope_and_file: &[&str], text: &str) {
        let src = self.set_file("instruction.md", text);
        let mut words = vec!["instructions", "set"];
        words.extend_from_slice(scope_and_file);
        words.extend_from_slice(&["--from", src.to_str().unwrap()]);
        self.plan_apply(&words);
    }

    /// The command `setup claude` installed, read back from the settings file it wrote.
    fn setup_hook_command(&self) -> String {
        self.cli_ok(&["setup", "claude"]);
        let settings = self.herdr.root.join("claude/settings.json");
        let doc: Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
        let mut found = vec![];
        collect_commands(&doc["hooks"], &mut found);
        assert_eq!(found.len(), 1, "one owner-marked hook: {found:?}");
        found.remove(0)
    }

    /// Run `command` the way Claude runs a hook: `sh -c` with the payload on stdin, in a graph pane's env.
    fn run_hook(&self, command: &str, clone: &str, payload: &str) -> Output {
        let mut child = self
            .command("sh")
            .args(["-c", command])
            .env("HERDR_GRAPH", "1")
            .env("HERDR_GRAPH_CLONE", clone)
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
}

/// Every `command` string under a settings `hooks` value that carries the hook's marker.
fn collect_commands(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            if let Some(c) = m.get("command").and_then(Value::as_str)
                && c.contains("--hook-prompt")
            {
                out.push(c.to_owned());
            }
            m.values().for_each(|x| collect_commands(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| collect_commands(x, out)),
        _ => {}
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

fn is_hex40(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The JSON envelope every view carries.
fn assert_envelope(view: &str, v: &Value) {
    assert_eq!(v["schema"], 1, "{view}: {v}");
    let head = v["head"].as_str().unwrap_or_default();
    assert!(is_hex40(head), "{view}: head {head:?}");
}

/// Keys the spec names at one level of a view's JSON.
fn assert_keys(view: &str, v: &Value, keys: &[&str]) {
    for k in keys {
        assert!(
            v.get(*k).is_some(),
            "{view}: spec key {k:?} missing from {v}"
        );
    }
}

/// Every view the sweep runs, each as the argv before `--json`.
fn views() -> Vec<Vec<&'static str>> {
    vec![
        vec!["roster"],
        vec!["roster", "--all"],
        vec!["roster", "--live"],
        vec!["roster", "--live", "--unmanaged"],
        vec!["instructions", "stack", "foreman"],
        vec!["sessions"],
        vec!["log"],
        vec!["ops", "--seat", "foreman"],
    ]
}

#[test]
fn read_only_every_view_and_json_contract() {
    let Some(rig) = Rig::start() else { return };
    rig.teamspace_with_seat();
    rig.settle();
    let before = rig.facts();
    let local_before = rig.git(&["status", "--porcelain", "--ignored"]);

    for argv in views() {
        let text = rig.cli(&argv);
        assert_eq!(text.status.code(), Some(0), "{argv:?}: {}", stderr(&text));
        assert!(!stdout(&text).trim().is_empty(), "{argv:?}: empty text");
        let (doc, err) = rig.json(&argv);
        let view = argv.join(" ");
        assert_envelope(&view, &doc);
        if argv[0] == "log" {
            assert!(
                err.contains("ops"),
                "log --json: the ops pointer goes to stderr: {err:?}"
            );
        }
        assert_eq!(
            rig.facts(),
            before,
            "{view}: HEAD, count, index or tree changed"
        );
        assert_eq!(
            rig.git(&["status", "--porcelain", "--ignored"]),
            local_before,
            "{view}: ignored (.graph-local) state changed"
        );
    }
}

#[test]
fn spec_keys_and_flags_reach_their_views() {
    let Some(rig) = Rig::start() else { return };
    rig.teamspace_with_seat();
    rig.plan_apply(&["seat", "rename", "foreman", "boss"]);
    rig.plan_apply(&["seat", "rename", "boss", "foreman"]);
    rig.settle();

    let (roster, _) = rig.json(&["roster"]);
    assert_keys(
        "roster",
        &roster,
        &["live", "live_unavailable", "scan", "teamspaces"],
    );
    assert_eq!(roster["live"], false);
    assert_keys(
        "roster scan",
        &roster["scan"],
        &["scanned", "max_scan", "stopped_early"],
    );
    let seat = &roster["teamspaces"][0]["seats"][0];
    assert_keys(
        "roster seat",
        seat,
        &[
            "id",
            "name",
            "lifecycle",
            "manager",
            "is_node_manager",
            "reload_required",
            "last_change",
            "last_change_scanned",
            "clones",
        ],
    );
    let clone = &seat["clones"][0];
    let clone_name = clone["name"].as_str().unwrap().to_owned();
    assert_keys(
        "roster clone",
        clone,
        &[
            "id",
            "name",
            "lifecycle",
            "runtime",
            "occupant",
            "threads",
            "instructions",
            "live",
        ],
    );
    assert_keys(
        "roster instructions",
        &clone["instructions"],
        &[
            "status",
            "loaded_rev",
            "loaded_at",
            "loaded_session",
            "source",
            "truncated",
            "parts",
            "omitted",
            "stale_parts",
            "message",
        ],
    );

    let (live, _) = rig.json(&["roster", "--live", "--unmanaged"]);
    assert_eq!(
        live["live"], true,
        "roster.live registered and answered: {live}"
    );
    assert_eq!(live["live_unavailable"], Value::Null, "{live}");
    assert_keys("roster --unmanaged", &live, &["unmanaged"]);
    assert!(
        rig.cli(&["roster", "--unmanaged"]).status.code() != Some(0),
        "--unmanaged needs --live"
    );

    let (stack, _) = rig.json(&["instructions", "stack", "foreman"]);
    assert_keys(
        "stack",
        &stack,
        &[
            "parts",
            "clones",
            "harness_files",
            "harness_files_note",
            "imports_followed",
        ],
    );
    assert_eq!(stack["imports_followed"], false);
    let (one, _) = rig.json(&["instructions", "stack", "foreman", "--clone", &clone_name]);
    assert_eq!(
        one["clones"].as_array().unwrap().len(),
        1,
        "--clone reaches stack"
    );
    let bad = rig.cli(&["instructions", "stack", "foreman", "--clone", "nope"]);
    assert_ne!(
        bad.status.code(),
        Some(0),
        "--clone filters by a real clone"
    );

    let (sessions, _) = rig.json(&["sessions"]);
    assert_keys("sessions", &sessions, &["tombstones", "total", "rows"]);
    let (by_clone, _) = rig.json(&["sessions", "clone", &clone_name]);
    assert_keys("sessions clone", &by_clone, &["rows"]);
    let (limited, _) = rig.json(&["sessions", "--limit", "1", "--since", "1d"]);
    assert!(limited["rows"].as_array().unwrap().len() <= 1);
    let (future, _) = rig.json(&["sessions", "--since", "2999-01-01"]);
    assert_eq!(future["total"], 0, "--since reaches sessions");

    let (log, _) = rig.json(&["log"]);
    assert_keys("log", &log, &["scan", "rows"]);
    assert_keys(
        "log scan",
        &log["scan"],
        &["scanned", "max_scan", "stopped_early"],
    );
    let all_rows = log["rows"].as_array().unwrap().len();
    assert!(all_rows >= 3, "create + rename + rename: {log}");
    for k in [
        "commit",
        "at",
        "op",
        "kind",
        "summary",
        "requester",
        "confirmation",
        "action",
        "objects",
        "paths",
        "malformed",
    ] {
        assert!(
            log["rows"][0].get(k).is_some(),
            "log row key {k:?}: {}",
            log["rows"][0]
        );
    }
    let (limit1, _) = rig.json(&["log", "--limit", "1"]);
    assert_eq!(
        limit1["rows"].as_array().unwrap().len(),
        1,
        "--limit reaches log"
    );
    let (capped, _) = rig.json(&["log", "--max-scan", "1"]);
    assert_eq!(capped["scan"]["max_scan"], 1);
    assert_eq!(
        capped["scan"]["stopped_early"], true,
        "--max-scan reaches log: {capped}"
    );
    let (since, _) = rig.json(&["log", "--since", "2999-01-01"]);
    assert!(
        since["rows"].as_array().unwrap().is_empty(),
        "--since reaches log"
    );
    let (everything, _) = rig.json(&["log", "--all"]);
    assert!(
        everything["rows"].as_array().unwrap().len() >= all_rows,
        "--all reaches log"
    );
    let (scoped, _) = rig.json(&["log", "seat", "foreman"]);
    assert!(
        !scoped["rows"].as_array().unwrap().is_empty(),
        "seat scope finds the renames"
    );
    let (by_nobody, _) = rig.json(&["log", "--by", "foreman"]);
    assert!(
        by_nobody["rows"].as_array().unwrap().len() <= all_rows,
        "--by is a filter, never a widening"
    );

    let (ops, _) = rig.json(&["ops", "--seat", "foreman", "--limit", "5"]);
    assert_keys("ops", &ops, &["ops"]);
}

/// Seat name at `commit`, read from the tree that commit names.
fn seat_name_at(rig: &Rig, commit: &str) -> String {
    let listing = rig.git(&[
        "ls-tree",
        "--name-only",
        &format!("{commit}:teamspaces/t/seats"),
    ]);
    listing.trim().to_owned()
}

#[test]
fn every_component_reports_the_one_pinned_head() {
    let Some(rig) = Rig::start() else { return };
    rig.teamspace_with_seat();
    rig.settle();
    let seat_id = rig.json(&["roster"]).0["teamspaces"][0]["seats"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // A writer lands renames while the readers run; each document must describe the commit it names.
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut heads = std::collections::BTreeSet::new();
    std::thread::scope(|s| {
        let writer = s.spawn(|| {
            let mut n = 0;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) && n < 40 {
                let (from, to) = if n % 2 == 0 {
                    ("foreman", "boss")
                } else {
                    ("boss", "foreman")
                };
                rig.plan_apply(&["seat", "rename", from, to]);
                n += 1;
            }
        });
        for _ in 0..25 {
            let (roster, _) = rig.json(&["roster"]);
            let head = roster["head"].as_str().unwrap().to_owned();
            assert!(is_hex40(&head));
            assert_eq!(
                roster["teamspaces"][0]["seats"][0]["name"]
                    .as_str()
                    .unwrap(),
                seat_name_at(&rig, &head),
                "roster rows come from the tree at its head {head}"
            );
            // last_change names an op that exists at that head, and the envelope has one head only
            let seat = &roster["teamspaces"][0]["seats"][0];
            if let Some(op) = seat["last_change"]["op"].as_str() {
                let ops_at_head = rig.git(&["ls-tree", "-r", "--name-only", &head]);
                assert!(
                    ops_at_head.contains(op),
                    "last_change {op} is not in {head}"
                );
            }
            let (stack, _) = rig.json(&["instructions", "stack", &seat_id]);
            let (log, _) = rig.json(&["log", "--all", "--limit", "3"]);
            let log_head = log["head"].as_str().unwrap();
            assert_eq!(
                log["rows"][0]["commit"].as_str().unwrap(),
                log_head,
                "the newest row is the pinned head, whatever landed since"
            );
            for row in log["rows"].as_array().unwrap() {
                let c = row["commit"].as_str().unwrap();
                assert!(
                    Command::new("git") // isolation-ok: read-only git in the private instance
                        .arg("-C")
                        .arg(&rig.instance)
                        .args(["merge-base", "--is-ancestor", c, log_head])
                        .status()
                        .unwrap()
                        .success(),
                    "{c} is not reachable from the pinned head {log_head}"
                );
            }
            if let Some(seat_err) = stack["error"].as_str() {
                panic!(
                    "stack could not resolve the seat at {}: {seat_err}",
                    stack["head"]
                );
            }
            heads.insert(head);
            heads.insert(log_head.to_owned());
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().unwrap();
    });
    assert!(
        heads.len() >= 2,
        "the writer never landed a commit while a view ran: {heads:?}"
    );
}

#[test]
fn r14_chain_through_the_setup_hook() {
    let Some(rig) = Rig::start_with_claude() else {
        return;
    };
    let clone = rig.teamspace_with_harness("claude");
    rig.set_instruction(&["global", "rules/style.md"], "global style rule\n");
    rig.set_instruction(
        &["seat", "foreman", "AGENTS.md"],
        "seat rule, first version\n",
    );
    rig.settle();

    // A running Claude in the clone's pane, so the observer keeps the occupant the hook's report creates.
    let (v, _) = rig.json(&["roster"]);
    let pane = v["teamspaces"][0]["seats"][0]["clones"][0]["runtime"]["pane"]
        .as_str()
        .unwrap()
        .to_owned();
    rig.pane_sh(
        &pane,
        &format!("{}/bin/real/claude 3000", rig.herdr.root.display()),
    );
    rig.wait_until("Herdr to see a Claude agent in the pane", |r| {
        !r.json(&["roster", "--live"]).0["teamspaces"][0]["seats"][0]["clones"][0]["live"]["agent"]
            .is_null()
    });
    let hook = rig.setup_hook_command();
    assert!(
        hook.contains("--stdin-payload") && hook.contains("session-report"),
        "{hook}"
    );
    let payload =
        format!(r#"{{"session_id":"{SESSION}","transcript_path":null,"source":"startup"}}"#);

    let roster_instr = |rig: &Rig| -> Value {
        let (v, _) = rig.json(&["roster"]);
        v["teamspaces"][0]["seats"][0]["clones"][0]["instructions"].clone()
    };
    let stack_instr = |rig: &Rig| -> Value {
        let (v, _) = rig.json(&["instructions", "stack", "foreman"]);
        v["clones"][0]["instructions"].clone()
    };
    let occupant_is_session = |rig: &Rig| -> bool {
        let (v, _) = rig.json(&["roster"]);
        v["teamspaces"][0]["seats"][0]["clones"][0]["occupant"]["native_session_id"] == SESSION
    };

    let out = rig.run_hook(&hook, &clone, &payload);
    assert!(out.status.success(), "hook: {}", stderr(&out));
    assert!(
        stdout(&out).contains("seat rule, first version"),
        "{}",
        stdout(&out)
    );
    let record = rig
        .instance
        .join(".graph-local/loaded")
        .join(format!("{clone}.json"));
    assert!(record.exists(), "the injection wrote {}", record.display());

    // current once the occupant lands (session-report is the second half of the hook command)
    rig.wait_until("the occupant to land", |r| occupant_is_session(r));
    rig.settle();
    for (view, instr) in [("roster", roster_instr(&rig)), ("stack", stack_instr(&rig))] {
        let (r, _) = rig.json(&["roster"]);
        assert_eq!(
            instr["status"],
            "current",
            "{view}: {instr}\nclone: {}\nlog: {}",
            r["teamspaces"][0]["seats"][0]["clones"][0],
            rig.cli_ok(&["log", "--all"])
        );
        assert_eq!(instr["loaded_session"], SESSION, "{view}");
    }

    // a committed edit to one delivered part makes both stale, naming that part and its commit
    rig.set_instruction(
        &["seat", "foreman", "AGENTS.md"],
        "seat rule, second version\n",
    );
    rig.settle();
    let edit = rig.git(&[
        "log",
        "-1",
        "--format=%H",
        "--",
        "teamspaces/t/seats/foreman/AGENTS.md",
    ]);
    let edit = edit.trim().to_owned();
    for (view, instr) in [("roster", roster_instr(&rig)), ("stack", stack_instr(&rig))] {
        assert_eq!(instr["status"], "stale", "{view}: {instr}");
        let stale = instr["stale_parts"].as_array().unwrap();
        assert_eq!(stale.len(), 1, "{view}: only the edited part: {instr}");
        assert_eq!(stale[0]["label"], "seat AGENTS.md", "{view}");
        assert_eq!(stale[0]["change"], "modified", "{view}");
        assert_eq!(stale[0]["path_last_touched"], edit.as_str(), "{view}");
    }
    let text = rig.cli_ok(&["instructions", "stack", "foreman"]);
    assert!(
        text.contains("path last touched") && text.contains(&edit[..7]),
        "stack text names the commit: {text}"
    );

    // the log shows the edit
    let (log, _) = rig.json(&["log", "seat", "foreman"]);
    assert!(
        log["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["commit"] == edit.as_str()),
        "log seat foreman lacks the edit {edit}: {log}"
    );

    // the next injection brings it back to current
    rig.run_hook(&hook, &clone, &payload);
    assert_eq!(roster_instr(&rig)["status"], "current");
    assert_eq!(stack_instr(&rig)["status"], "current");
}
