//! LoadedInstructions through the real hook chain (hg-blw.12): the exact setup hook commands run through
//! `sh -c` from a clone with no occupant, the loaded record they write, and the status `roster` and
//! `instructions stack` read from it. Every child goes through `TestRoot`.
mod support;

use herdr_graph::bootstrap::setup_claude::{codex_hook_command, hook_command};
use herdr_graph::model::clone::CloneRecord;
use herdr_graph::model::harness::Harness;
use herdr_graph::model::native_session::NativeSession;
use herdr_graph::model::seat::SeatRecord;
use herdr_graph::model::teamspace::TeamspaceRecord;
use herdr_graph::model::{
    CloneId, CloneLifecycle, CommitId, Lifecycle, NsId, Occupant, SCHEMA_VERSION, SeatId,
    TeamspaceId,
};
use herdr_graph::ports::store::{RepoPath, Store};
use herdr_graph::store::git::graph_signature;
use herdr_graph::store::init::init_instance;
use herdr_graph::store::{EditSet, GitStore, layout, record};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use support::isolated::TestRoot;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");
const CLAUDE_PAYLOAD: &str = r#"{"session_id":"s-1","transcript_path":null,"source":"startup"}"#;
const STYLE: &str = "global style rule\n";

/// An instance with one teamspace, one seat and one clone that has no occupant, and a global rule file.
struct Graph {
    root: TestRoot,
    store: GitStore,
    clone: CloneRecord,
    clone_dir: RepoPath,
}

/// The test writes commits straight to the ref; bring the index and working tree to them so that
/// `git status` measures only what the hook did.
fn sync_worktree(store: &GitStore) {
    let out = Command::new("git")
        .arg("-C")
        .arg(store.root())
        .args(["reset", "--hard", "-q"])
        .output()
        .unwrap();
    assert!(out.status.success());
}

fn commit(store: &GitStore, edits: &EditSet, msg: &str) -> CommitId {
    let head = store.head().unwrap();
    let tree = store.build_tree(&head, edits).unwrap();
    let id = store
        .with_repo(|r| {
            let sig = graph_signature()?;
            let parent = r.find_commit(git2::Oid::from_str(&head.0)?)?;
            let oid = r.commit(None, &sig, &sig, msg, &r.find_tree(tree)?, &[&parent])?;
            r.reference("refs/heads/main", oid, true, msg)?;
            Ok(CommitId(oid.to_string()))
        })
        .unwrap();
    sync_worktree(store);
    id
}

impl Graph {
    fn new() -> Graph {
        let root = TestRoot::new();
        init_instance(&root.instance()).unwrap();
        let store = GitStore::open(&root.instance()).unwrap();
        let ts_id = TeamspaceId::new();
        let ts_dir = layout::teamspace_dir("proj");
        let ts = TeamspaceRecord {
            schema: SCHEMA_VERSION,
            id: ts_id.clone(),
            rev: 1,
            name: "proj".into(),
            name_history: vec![],
            lifecycle: Lifecycle::Active,
            retired: None,
            runtime: Default::default(),
            project_repo: None,
            default_manager: None,
            channel: Default::default(),
        };
        let seat_dir = layout::seat_dir(&ts_dir, "foreman");
        let seat = SeatRecord {
            schema: SCHEMA_VERSION,
            id: SeatId::new(),
            rev: 1,
            name: "foreman".into(),
            name_history: vec![],
            teamspace: ts_id,
            lifecycle: Lifecycle::Active,
            retired: None,
            system_duty: None,
            template_ref: None,
            manager: None,
            applications: vec![],
            overrides: Default::default(),
            participation: Default::default(),
            activation: Default::default(),
            runtime: Default::default(),
            channel: Default::default(),
            reload_required: false,
            moved_out: false,
        };
        let clone_dir = layout::clone_dir(&seat_dir, "main");
        let clone = CloneRecord {
            schema: SCHEMA_VERSION,
            id: CloneId::new(),
            rev: 1,
            seat: seat.id.clone(),
            name: "main".into(),
            name_history: vec![],
            lifecycle: CloneLifecycle::Active,
            retired: None,
            runtime: Default::default(),
            occupant: None,
            sessions: vec![],
            opt_outs: vec![],
            invitations: vec![],
            reload_required: false,
            threads_seat: None,
            threads_seat_history: vec![],
            seat_intent: None,
            seat_fact: None,
            key_reservation: None,
            keyless: false,
        };
        let mut e = EditSet::default();
        e.put(
            layout::teamspace_record(&ts_dir),
            record::to_toml_bytes(&ts).unwrap(),
        );
        e.put(
            layout::seat_record(&seat_dir),
            record::to_toml_bytes(&seat).unwrap(),
        );
        e.put(
            layout::clone_record(&clone_dir),
            record::to_toml_bytes(&clone).unwrap(),
        );
        e.put(
            RepoPath::new("rules/style.md").unwrap(),
            STYLE.as_bytes().to_vec(),
        );
        commit(&store, &e, "graph with a clone and no occupant");
        Graph {
            root,
            store,
            clone,
            clone_dir,
        }
    }

    fn instance(&self) -> PathBuf {
        self.root.instance()
    }

    fn record_file(&self) -> PathBuf {
        self.instance()
            .join(".graph-local/loaded")
            .join(format!("{}.json", self.clone.id))
    }

    fn record(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.record_file()).unwrap()).unwrap()
    }

    /// The occupant landing: what `session.report` commits, written straight to the graph.
    fn occupant_lands(&mut self, native_id: &str) {
        let ns = NsId::new();
        self.clone.sessions = vec![NativeSession {
            id: ns.clone(),
            harness: Harness::Claude,
            native_session_id: native_id.into(),
            transcript_path: None,
            transcript: None,
            cwd: PathBuf::from("/work"),
            started: chrono::Utc::now(),
            ended: None,
            end_reason: None,
        }];
        self.clone.occupant = Some(Occupant {
            harness: Harness::Claude,
            native_session: ns,
            since: chrono::Utc::now(),
        });
        self.clone.rev += 1;
        let mut e = EditSet::default();
        e.put(
            layout::clone_record(&self.clone_dir),
            record::to_toml_bytes(&self.clone).unwrap(),
        );
        commit(&self.store, &e, "occupant landed");
    }

    fn commit_style(&self, body: &str) {
        let mut e = EditSet::default();
        e.put(
            RepoPath::new("rules/style.md").unwrap(),
            body.as_bytes().to_vec(),
        );
        commit(&self.store, &e, "edit style");
    }

    fn cmd(&self, program: &str, args: &[&str]) -> Command {
        let mut c = self.root.command(program);
        c.args(args)
            .env("HERDR_GRAPH_INSTANCE", self.instance())
            .env("HERDR_GRAPH", "1")
            .env("HERDR_GRAPH_CLONE", self.clone.id.to_string());
        c
    }

    /// Run a shell hook command with `payload` on stdin.
    fn hook(&self, command: &str, payload: Option<&str>) -> Output {
        let mut c = self.cmd("sh", &["-c", command]);
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
        match payload {
            Some(p) => {
                c.stdin(Stdio::piped());
                let mut child = c.spawn().unwrap();
                child.stdin.take().unwrap().write_all(p.as_bytes()).unwrap();
                child.wait_with_output().unwrap()
            }
            None => c.stdin(Stdio::null()).output().unwrap(),
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(BIN, args).output().unwrap()
    }

    /// The clone's `instructions` object in `roster --json`.
    fn roster_instructions(&self) -> Value {
        let out = self.run(&["roster", "--json"]);
        assert!(out.status.success(), "{}", lossy(&out.stderr));
        let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
        doc["teamspaces"][0]["seats"][0]["clones"][0]["instructions"].clone()
    }

    fn stack_instructions(&self) -> Value {
        let out = self.run(&["instructions", "stack", "foreman", "--json"]);
        assert!(out.status.success(), "{}", lossy(&out.stderr));
        let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
        doc["clones"][0]["instructions"].clone()
    }
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn bin_path() -> &'static Path {
    Path::new(BIN)
}

fn git_status(g: &Graph) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(g.instance())
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    lossy(&out.stdout)
}

fn old_claude_command() -> String {
    format!(
        "{b} seat --hook-prompt; {b} session-report --from-hook claude # herdr-graph-owned",
        b = BIN
    )
}

#[test]
fn hook_chain_reaches_current_once_the_occupant_lands() {
    let mut g = Graph::new();
    let out = g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    assert!(out.status.success(), "{}", lossy(&out.stderr));
    assert!(lossy(&out.stdout).contains("global style rule"));

    let rec = g.record();
    assert_eq!(rec["native_session_id"], "s-1");
    assert_eq!(rec["stdin_payload"], true);
    assert_eq!(rec["source"], "hook");
    assert_eq!(rec["parts"][0]["label"], "global rules/style.md");
    assert_eq!(
        git_status(&g),
        "",
        "an injection leaves the graph repo clean"
    );

    // No occupant yet: the record is shown, the status is unknown.
    let before = g.roster_instructions();
    assert_eq!(before["status"], "unknown");
    assert_eq!(before["loaded_session"], "s-1");
    assert_eq!(before["parts"][0]["delivered"], "whole");
    assert!(
        before["omitted"].as_array().unwrap().is_empty() && !before["truncated"].as_bool().unwrap()
    );

    g.occupant_lands("s-1");
    assert_eq!(g.roster_instructions()["status"], "current");
    assert_eq!(g.stack_instructions()["status"], "current");
    let text = g.run(&["instructions", "stack", "foreman"]);
    assert!(
        lossy(&text.stdout).contains(" into s-1 via hook at "),
        "{}",
        lossy(&text.stdout)
    );

    // An edit to a delivered part makes it stale, with the commit that last touched the path.
    g.commit_style("global style rule, edited\n");
    let stale = g.roster_instructions();
    assert_eq!(stale["status"], "stale");
    assert_eq!(stale["stale_parts"][0]["change"], "modified");
    let touched = stale["stale_parts"][0]["path_last_touched"]
        .as_str()
        .unwrap();
    assert_eq!(touched.len(), 40);
    let text = g.run(&["instructions", "stack", "foreman"]);
    assert!(
        lossy(&text.stdout).contains("(path last touched "),
        "{}",
        lossy(&text.stdout)
    );

    // The next injection brings the record back to current.
    g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    assert_eq!(g.roster_instructions()["status"], "current");
}

#[test]
fn mismatched_and_null_ids_read_unknown() {
    let mut g = Graph::new();
    g.occupant_lands("s-2");
    g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    let v = g.roster_instructions();
    assert_eq!(v["status"], "unknown");
    assert_eq!(v["loaded_session"], "s-1");

    // A payload without an id records null.
    g.hook(&hook_command(bin_path()), Some(r#"{"source":"startup"}"#));
    assert!(g.record()["native_session_id"].is_null());
    assert_eq!(g.record()["stdin_payload"], true);
    let v = g.roster_instructions();
    assert_eq!(v["status"], "unknown");
    assert!(v["loaded_session"].is_null());
    assert!(v["loaded_rev"].is_string(), "the record is still shown");
}

#[test]
fn codex_rollout_stem_occupant_gets_the_form_note() {
    let mut g = Graph::new();
    let uuid = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
    let payload = format!(
        r#"{{"session_id":"{uuid}","hook_event_name":"SessionStart","cwd":"/work","source":"startup","transcript_path":null,"model":"gpt-x"}}"#
    );
    let out = g.hook(&codex_hook_command(bin_path()), Some(&payload));
    assert!(out.status.success(), "{}", lossy(&out.stderr));
    assert_eq!(g.record()["native_session_id"], uuid);
    // The occupant id observe/sessions.rs derives from Herdr's agent_session path: the rollout file stem.
    g.occupant_lands(&format!("rollout-2026-11-10T12-00-00-{uuid}"));
    let v = g.roster_instructions();
    assert_eq!(
        v["status"], "unknown",
        "the ids are not treated as matching"
    );
    assert_eq!(v["note"], "codex session id form differs");
    assert_eq!(g.stack_instructions()["note"], v["note"]);
}

#[test]
fn user_level_claude_md_under_the_cwd_ancestors_is_labelled_user() {
    let g = Graph::new();
    // The seat cwd is `<instance>/.graph-local/cwd/<seat>`; a user dir whose parent is an ancestor of it
    // is reached by the ancestor walk first.
    let user_dir = g.instance().join(".claude");
    std::fs::create_dir_all(&user_dir).unwrap();
    let md = user_dir.join("CLAUDE.md");
    std::fs::write(&md, "user rules").unwrap();
    let out = g
        .cmd(BIN, &["instructions", "stack", "foreman", "--json"])
        .env("CLAUDE_CONFIG_DIR", &user_dir) // isolation-ok: a dir inside the private root, overriding scrubbed_env's
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", lossy(&out.stderr));
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    let files = doc["harness_files"].as_array().unwrap();
    let f = files
        .iter()
        .find(|f| f["path"] == md.to_str().unwrap())
        .unwrap_or_else(|| panic!("{md:?} not listed: {files:?}"));
    assert_eq!(
        (f["origin"].as_str(), f["env"].as_str()),
        (Some("user"), Some("inspector"))
    );
}

#[test]
fn codex_hook_records_the_payload_session() {
    let mut g = Graph::new();
    let codex = codex_hook_command(bin_path());
    let payload = r#"{"session_id":"s-1","hook_event_name":"SessionStart","cwd":"/work","source":"startup","transcript_path":null,"model":"gpt-x"}"#;
    let out = g.hook(&codex, Some(payload));
    assert!(out.status.success(), "{}", lossy(&out.stderr));
    let printed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        printed["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("global style rule")
    );
    assert_eq!(g.record()["source"], "hook-codex");
    assert_eq!(g.record()["native_session_id"], "s-1");
    g.occupant_lands("s-1");
    let v = g.roster_instructions();
    assert_eq!(v["status"], "current");
    assert_eq!(v["source"], "hook-codex");

    // No session_id in the payload: null, unknown.
    g.hook(&codex, Some(r#"{"hook_event_name":"SessionStart"}"#));
    assert!(g.record()["native_session_id"].is_null());
    assert_eq!(g.roster_instructions()["status"], "unknown");
}

#[test]
fn older_hooks_without_the_flag_keep_working_and_say_so() {
    let mut g = Graph::new();
    g.occupant_lands("s-1");
    let new = g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    let old = g.hook(&old_claude_command(), Some(CLAUDE_PAYLOAD));
    assert!(old.status.success(), "{}", lossy(&old.stderr));
    assert_eq!(
        old.stdout, new.stdout,
        "the hook text does not change with the flag"
    );

    let rec = g.record();
    assert!(rec["native_session_id"].is_null());
    assert_eq!(rec["stdin_payload"], false);
    let v = g.roster_instructions();
    assert_eq!(v["status"], "unknown");
    assert_eq!(v["note"], "hook predates --stdin-payload; rerun setup");
    let text = g.run(&["roster"]);
    assert!(
        lossy(&text.stdout).contains("hook predates --stdin-payload; rerun setup"),
        "{}",
        lossy(&text.stdout)
    );

    // The older Codex command also runs.
    let old_codex = format!("{BIN} seat --hook-prompt --codex # herdr-graph-owned");
    let out = g.hook(&old_codex, Some(r#"{"session_id":"s-1"}"#));
    assert!(out.status.success());
    assert_eq!(g.record()["source"], "hook-codex");
    assert_eq!(g.record()["stdin_payload"], false);
}

#[test]
fn session_report_gets_the_same_payload_from_the_new_hook() {
    let g = Graph::new();
    let spool = herdr_graph::transcripts::capture::spool_dir(&g.instance());
    let spooled = || -> Vec<PathBuf> {
        std::fs::read_dir(&spool)
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    };
    let read_spool = || -> Vec<String> {
        let mut v: Vec<String> = spooled()
            .iter()
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect();
        v.sort();
        v
    };
    g.hook(&old_claude_command(), Some(CLAUDE_PAYLOAD));
    let from_old = read_spool();
    assert_eq!(from_old.len(), 1, "{from_old:?}");
    for p in spooled() {
        std::fs::remove_file(p).unwrap();
    }
    g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    let from_new = read_spool();
    assert_eq!(from_new.len(), 1, "{from_new:?}");
    assert!(from_new[0].contains("\"s-1\""), "{from_new:?}");
    let (a, b): (Value, Value) = (
        serde_json::from_str(&from_old[0]).unwrap(),
        serde_json::from_str(&from_new[0]).unwrap(),
    );
    assert_eq!(
        a["capture"], b["capture"],
        "same arguments reach session-report"
    );
}

#[test]
fn flag_and_no_flag_print_identical_bytes() {
    let g = Graph::new();
    let run = |args: &[&str]| {
        let mut c = g.cmd(BIN, args);
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = c.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(CLAUDE_PAYLOAD.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    for codex in [false, true] {
        let (mut plain, mut flagged) =
            (vec!["seat", "--hook-prompt"], vec!["seat", "--hook-prompt"]);
        if codex {
            plain.push("--codex");
            flagged.push("--codex");
        }
        flagged.push("--stdin-payload");
        let (a, b) = (run(&plain), run(&flagged));
        assert!(a.status.success() && b.status.success());
        assert!(!a.stdout.is_empty());
        assert_eq!(a.stdout, b.stdout, "codex={codex}");
    }
}

#[test]
fn held_open_stdin_still_prints_and_exits() {
    let g = Graph::new();
    let mut c = g.cmd(BIN, &["seat", "--hook-prompt", "--stdin-payload"]);
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let began = Instant::now();
    let mut child = c.spawn().unwrap();
    let _held_open = child.stdin.take().unwrap(); // never written, never closed
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", lossy(&out.stderr));
    assert!(
        began.elapsed() < Duration::from_secs(8),
        "took {:?}",
        began.elapsed()
    );
    assert!(lossy(&out.stdout).contains("global style rule"));
    assert!(g.record()["native_session_id"].is_null());
    assert_eq!(g.record()["stdin_payload"], true);
}

/// The command `setup claude` installed and its hook `timeout`, read back from the settings file it wrote.
fn installed_claude_hook(g: &Graph) -> (String, u64) {
    let out = g.run(&["setup", "claude"]);
    assert!(out.status.success(), "{}", lossy(&out.stderr));
    let settings = g.root.claude_dir().join("settings.json");
    let doc: Value = serde_json::from_slice(&std::fs::read(settings).unwrap()).unwrap();
    let hook = doc["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["hooks"].as_array().unwrap())
        .find(|h| h["command"].as_str().unwrap().contains("--hook-prompt"))
        .expect("installed SessionStart hook")
        .clone();
    (
        hook["command"].as_str().unwrap().to_owned(),
        hook["timeout"].as_u64().unwrap(),
    )
}

/// A hook that never gets EOF on stdin is bounded only by the harness `timeout`, before and after the
/// payload-once command: the base command's `session-report` already read stdin unbounded.
#[test]
fn installed_hook_with_held_open_stdin_is_bounded_like_the_base_hook() {
    let g = Graph::new();
    let (installed, timeout) = installed_claude_hook(&g);
    assert_eq!(timeout, 10, "the base hook budget");
    assert_ne!(installed, old_claude_command(), "setup installs the new command");
    for (label, command) in [("installed", installed), ("base", old_claude_command())] {
        let mut c = g.cmd("sh", &["-c", &command]);
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut hook = g.root.spawn(&mut c); // own process group; dropped on every path
        let held_open = hook.child_mut().stdin.take().unwrap();
        assert!(
            hook.wait_timeout(Duration::from_secs(1)).is_none(),
            "{label}: finished while stdin was held open"
        );
        drop(held_open); // EOF, as the harness delivers it
        let began = Instant::now();
        let status = hook
            .wait_timeout(Duration::from_secs(8))
            .unwrap_or_else(|| panic!("{label}: still running after stdin closed"));
        assert!(status.success(), "{label}: {status}");
        assert!(began.elapsed() < Duration::from_secs(8), "{label}");
        let mut stdout = String::new();
        hook.child_mut()
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        assert!(stdout.contains("global style rule"), "{label}: {stdout}");
    }
}

#[test]
fn readers_never_write_the_record() {
    let mut g = Graph::new();
    g.occupant_lands("s-1");
    g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    let before = std::fs::read(g.record_file()).unwrap();
    for args in [
        &["seat", "--instructions"][..],
        &["roster", "--json"],
        &["instructions", "stack", "foreman"],
    ] {
        g.run(args);
    }
    assert_eq!(std::fs::read(g.record_file()).unwrap(), before);
    // The record is also not recreated by a reader once removed.
    std::fs::remove_file(g.record_file()).unwrap();
    g.run(&["seat", "--instructions"]);
    assert!(!g.record_file().exists());
}

#[cfg(feature = "test-support")]
#[test]
fn seat_json_through_the_daemon_leaves_the_record_unchanged() {
    let mut g = Graph::new();
    g.occupant_lands("s-1");
    g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    let before = std::fs::read(g.record_file()).unwrap();
    // `seat --json` ensures the daemon (fake services, a Herdr socket that ignores connections) and
    // calls `seat.resolve`; the rig stops the daemon on drop.
    let listener = std::os::unix::net::UnixListener::bind(g.root.herdr_socket()).unwrap();
    let l2 = listener.try_clone().unwrap();
    std::thread::spawn(move || for _conn in l2.incoming() {});
    let out = g
        .cmd(BIN, &["seat", "--json"])
        .env("HG_TEST_FAKE_SERVICES", "1")
        .env("HERDR_PANE_ID", "p_unbound")
        .output()
        .unwrap();
    let code = out.status.code();
    assert_ne!(code, Some(5), "daemon unreachable: {}", lossy(&out.stderr));
    assert_eq!(std::fs::read(g.record_file()).unwrap(), before);
    assert_eq!(git_status(&g), "");
}

#[test]
fn unbound_caller_writes_nothing() {
    let g = Graph::new();
    let mut c = g.cmd(BIN, &["seat", "--hook-prompt", "--stdin-payload"]);
    c.env_remove("HERDR_GRAPH_CLONE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    let out = c.output().unwrap();
    assert!(out.status.success());
    assert!(lossy(&out.stdout).starts_with("herdr-graph: this pane is a graph seat"));
    assert!(!g.record_file().exists());
}

#[test]
fn write_failure_changes_neither_output_nor_exit_code() {
    let g = Graph::new();
    let ok = g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    std::fs::remove_file(g.record_file()).unwrap();
    // A file where the record directory should be: every write fails.
    let dir = g.instance().join(".graph-local/loaded");
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::write(&dir, b"not a directory").unwrap();
    let bad = g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    assert_eq!(bad.status.code(), ok.status.code());
    assert_eq!(bad.stdout, ok.stdout);
    assert_eq!(git_status(&g), "");
}

fn put_files(g: &Graph, files: &[(&str, String)], msg: &str) {
    let mut e = EditSet::default();
    for (path, body) in files {
        e.put(RepoPath::new(path).unwrap(), body.clone().into_bytes());
    }
    commit(&g.store, &e, msg);
}

/// 11-byte lines, so a shorter budget always moves the cut by whole lines.
fn lines(n: usize) -> String {
    (0..n).map(|i| format!("line {i:04}\n")).collect()
}

#[test]
fn truncated_injection_cut_omitted_then_stale_on_boundary_move() {
    let mut g = Graph::new();
    // Render order is global, then teamspace: the teamspace file is the last part and alone exceeds the cap.
    let big = lines(1200);
    assert!(big.len() > herdr_graph::bootstrap::instructions::HOOK_RENDER_CAP);
    put_files(
        &g,
        &[("teamspaces/proj/rules/big.md", big.clone())],
        "big teamspace rule",
    );
    let out = g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    assert!(out.status.success(), "{}", lossy(&out.stderr));
    let printed = lossy(&out.stdout);
    assert!(
        printed
            .contains("[truncated: omitted global rules/style.md, teamspace rules/big.md (cut);"),
        "{printed}"
    );
    assert!(!printed.contains("global style rule"), "{printed}");
    g.occupant_lands("s-1");

    for v in [g.roster_instructions(), g.stack_instructions()] {
        assert_eq!(v["status"], "current", "{v}");
        assert_eq!(v["truncated"], true);
        assert_eq!(v["omitted"], serde_json::json!(["global rules/style.md"]));
        assert_eq!(
            v["parts"],
            serde_json::json!([{"label": "teamspace rules/big.md", "delivered": "cut"}])
        );
    }
    let rec = g.record();
    assert_eq!(rec["parts"][0]["delivered"], "cut");
    let kept = rec["parts"][0]["bytes"].as_u64().unwrap() as usize;
    assert!(kept > 0 && kept < big.len() && kept % 11 == 0, "{kept}");

    // A new earlier part is omitted too: the marker grows, the cap boundary moves, and the cut part's own
    // text (unchanged) is delivered to a shorter prefix.
    put_files(
        &g,
        &[("rules/extra.md", "another global rule\n".into())],
        "earlier part",
    );
    let v = g.roster_instructions();
    assert_eq!(v["status"], "stale", "{v}");
    assert_eq!(v["truncated"], true);
    let stale: Vec<(&str, &str)> = v["stale_parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["label"].as_str().unwrap(), s["change"].as_str().unwrap()))
        .collect();
    assert_eq!(stale, [("teamspace rules/big.md", "modified")]);

    // The next injection delivers the new boundary and the record is current again, still truncated.
    g.hook(&hook_command(bin_path()), Some(CLAUDE_PAYLOAD));
    let v = g.roster_instructions();
    assert_eq!(v["status"], "current", "{v}");
    assert_eq!(
        v["omitted"],
        serde_json::json!(["global rules/extra.md", "global rules/style.md"])
    );
    assert!(g.record()["parts"][0]["bytes"].as_u64().unwrap() < kept as u64);
}

#[test]
fn doctor_harness_hook_reports_malformed_settings() {
    let root = TestRoot::new();
    let dir = root.claude_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.json"), "{ \"hooks\": [oops").unwrap();
    let checks = herdr_graph::daemon::doctor::harness_hook_checks(Some(&dir), None);
    assert_eq!(checks.len(), 1, "{checks:?}");
    let c = &checks[0];
    assert_eq!(c.name, "harness_hook claude");
    assert!(c.warn && c.ok, "a warning, not a failure: {c:?}");
    assert!(
        c.detail.contains("is not valid JSON") && c.detail.contains("settings.json"),
        "{c:?}"
    );
    assert_ne!(c.detail, "current");
}
