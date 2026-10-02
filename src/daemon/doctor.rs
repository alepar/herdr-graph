//! `doctor`: installation and instance diagnostics (spec §10).
use super::client::{Client, hello};
use crate::config::{Env, InstancePaths, InstanceSource, plugin_config_dir_via_herdr, socket_path};
use crate::journal::{Journal, OpRow};
use crate::model::CloneLifecycle;
use crate::model::common::{Availability, Lifecycle};
use crate::model::operation::OpState;
use crate::model::request::RequestStatus;
use crate::ports::store::Store;
use crate::store::layout;
use crate::store::tree::{CommitView, TreeRead};
use crate::threads::discovery::{DiscoveryInputs, Source, resolve_state_dir};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<Check>,
}

impl DoctorReport {
    pub fn all_ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }
}

fn check(name: &str, ok: bool, detail: impl Into<String>) -> Check {
    Check { name: name.into(), ok, detail: detail.into() }
}

/// Paths recorded in `.graph-local/worktree_dirty`: JSON lines `{"path":…,"op":…,"at":…}` (written by the writer).
/// Unparsable lines are skipped; a missing file is "no entries".
pub fn read_worktree_dirty(file: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v["path"].as_str().map(str::to_owned))
        .collect()
}

/// The `threads` check: where the herdr-threads state dir was found (or why not), then the daemon's own view
/// of the connection. With a daemon, `ok` is its `connected`; without one, `ok` is "the state dir was found".
fn threads_check(resolved: Result<Option<(PathBuf, Source)>, String>, component: Option<&Value>) -> Check {
    let (found, mut detail) = match &resolved {
        Ok(Some((dir, source))) => (true, format!("state dir {} ({source})", dir.display())),
        Ok(None) => (
            false,
            "not found (no herdr-threads state directory in the default places; install/start herdr-threads or set \
             threads_state_dir in config.toml)"
                .to_string(),
        ),
        Err(why) => (false, format!("not found ({why})")),
    };
    let Some(c) = component else {
        return check("threads", found, detail);
    };
    let connected = c["connected"].as_bool().unwrap_or(false);
    if connected {
        detail.push_str(&format!("; daemon connected, capability {}", c["capability"]));
    } else {
        let why = c["error"].as_str().or(c["note"].as_str()).unwrap_or("no reason reported");
        detail.push_str(&format!("; daemon not connected: {why}"));
    }
    check("threads", connected, detail)
}

const LIST_CAP: usize = 10;

/// `a; b; c; +N more` for lists longer than [`LIST_CAP`].
fn capped(items: &[String]) -> String {
    let mut out = items.iter().take(LIST_CAP).cloned().collect::<Vec<_>>().join("; ");
    if items.len() > LIST_CAP {
        out.push_str(&format!("; +{} more", items.len() - LIST_CAP));
    }
    out
}

/// `writer_halted` is a nullable reason string published by the daemon's `writer` status component.
fn writer_check(component: Option<&Value>) -> Check {
    match component {
        None => check("writer", true, "not reported by daemon"),
        Some(w) => match w["writer_halted"].as_str() {
            Some(reason) => check(
                "writer",
                false,
                format!("halted: {reason} — fix the cause, then run herdr-graph writer resume"),
            ),
            None => check("writer", true, "running"),
        },
    }
}

fn failed_ops_check(rows: &[OpRow]) -> Check {
    if rows.is_empty() {
        return check("failed ops", true, "none");
    }
    let items: Vec<String> = rows
        .iter()
        .map(|r| {
            let kind = serde_json::to_value(r.request.kind).ok().and_then(|v| v.as_str().map(str::to_owned));
            let reason = r.rejection.as_ref().map_or("no reason recorded", |x| x.reason.as_str());
            format!("{} {}: {reason}", r.op, kind.unwrap_or_else(|| format!("{:?}", r.request.kind)))
        })
        .collect();
    check(
        "failed ops",
        false,
        format!("{} failed: {} — herdr-graph cancel <op> to dismiss", rows.len(), capped(&items)),
    )
}

fn unknown_objects_check(tree: &dyn TreeRead) -> Check {
    let mut names = Vec::new();
    let mut clones = Vec::new();
    let read = (|| -> Result<(), crate::ports::store::StoreError> {
        for (_, t) in layout::list_teamspaces(tree)? {
            if t.lifecycle != Lifecycle::Retired && t.runtime.availability == Availability::Unknown {
                names.push(format!("teamspace {}", t.name));
            }
        }
        for (_, s) in layout::all_seats(tree)? {
            if s.lifecycle != Lifecycle::Retired && s.runtime.availability == Availability::Unknown {
                names.push(format!("seat {}", s.name));
            }
        }
        for (_, c) in layout::all_clones(tree)? {
            if c.lifecycle != CloneLifecycle::Retired && c.runtime.availability == Availability::Unknown {
                names.push(format!("clone {}", c.name));
                clones.push(c.name);
            }
        }
        Ok(())
    })();
    if let Err(e) = read {
        return check("unknown objects", false, format!("cannot read the graph: {e}"));
    }
    if names.is_empty() {
        return check("unknown objects", true, "none");
    }
    let hint = clones.first().map_or(String::new(), |c| format!(" — herdr-graph rebind {c} --pane <pane>"));
    check("unknown objects", false, format!("{} with unknown availability: {}{hint}", names.len(), capped(&names)))
}

fn undeliverable_check(tree: &dyn TreeRead) -> Check {
    let reqs = match layout::list_requests(tree) {
        Ok(r) => r,
        Err(e) => return check("undeliverable requests", false, format!("cannot read the graph: {e}")),
    };
    let items: Vec<String> = reqs
        .iter()
        .filter(|(_, r)| matches!(r.status, RequestStatus::Pending | RequestStatus::Delivered))
        .filter_map(|(_, r)| r.undeliverable.as_ref().map(|why| format!("{}: {why}", r.id)))
        .collect();
    if items.is_empty() {
        check("undeliverable requests", true, "none")
    } else {
        check("undeliverable requests", false, format!("{} stuck: {}", items.len(), capped(&items)))
    }
}

fn worktree_dirty_check(entries: &[String]) -> Check {
    if entries.is_empty() {
        check("worktree_dirty", true, "no entries")
    } else {
        check("worktree_dirty", false, format!("{} entries: {}", entries.len(), entries.join(", ")))
    }
}

/// Read side of the instance-derived lines: the graph at head and the journal, both read-only.
fn instance_checks(root: &Path, checks: &mut Vec<Check>) {
    let paths = InstancePaths::new(root);
    match crate::store::GitStore::open(root).and_then(|s| s.head().map(|h| (s, h))) {
        Ok((store, head)) => {
            let view = CommitView { store: &store, at: head };
            checks.push(unknown_objects_check(&view));
            checks.push(undeliverable_check(&view));
        }
        Err(e) => {
            checks.push(check("unknown objects", false, format!("cannot read the graph: {e}")));
            checks.push(check("undeliverable requests", false, format!("cannot read the graph: {e}")));
        }
    }
    if !paths.journal.exists() {
        checks.push(check("failed ops", true, "no journal yet"));
        return;
    }
    match Journal::open(&paths.journal).and_then(|j| j.list(&[OpState::Failed], 50)) {
        Ok(rows) => checks.push(failed_ops_check(&rows)),
        Err(e) => checks.push(check("failed ops", false, format!("cannot read the journal: {e}"))),
    }
}

pub fn doctor(env: &Env) -> DoctorReport {
    let mut checks = Vec::new();
    let Some((root, source)) = crate::config::locate_instance(env, &plugin_config_dir_via_herdr) else {
        checks.push(check("instance", false, "no instance configured — run herdr-graph init"));
        return DoctorReport { checks };
    };
    let src = match &source {
        InstanceSource::EnvVar => format!("{} via HERDR_GRAPH_INSTANCE", crate::config::ENV_INSTANCE),
        InstanceSource::PluginConfig(p) | InstanceSource::UserConfig(p) => format!("via {}", p.display()),
    };
    checks.push(check("instance", true, format!("{} ({src})", root.display())));

    match crate::store::GitStore::open(&root) {
        Ok(_) => checks.push(check("instance repo", true, "opens, graph.toml on main")),
        Err(e) => checks.push(check("instance repo", false, e.to_string())),
    }

    let sock = socket_path(&root);
    let hello_reply = hello(&sock);
    match &hello_reply {
        Some(h) => checks.push(check("daemon", true, format!("pid {} on {}", h["pid"], sock.display()))),
        None => checks.push(check("daemon", false, format!("not reachable at {}", sock.display()))),
    }

    match (&env.herdr_socket, hello_reply.as_ref().and_then(|h| h["herdr_socket"].as_str())) {
        (Some(mine), Some(theirs)) if Path::new(theirs) == mine.as_path() => {
            checks.push(check("daemon herdr socket", true, theirs.to_string()));
        }
        (Some(mine), Some(theirs)) => checks.push(check(
            "daemon herdr socket",
            false,
            format!("daemon uses {theirs}; this pane uses {}", mine.display()),
        )),
        (None, Some(theirs)) => checks.push(check("daemon herdr socket", true, format!("{theirs} (HERDR_SOCKET_PATH unset here)"))),
        (_, None) => checks.push(check("daemon herdr socket", false, "no daemon to compare")),
    }

    match &env.herdr_socket {
        None => checks.push(check("herdr socket", false, "HERDR_SOCKET_PATH unset (not inside Herdr)")),
        Some(p) => match std::os::unix::net::UnixStream::connect(p) {
            Ok(_) => checks.push(check("herdr socket", true, format!("{} reachable", p.display()))),
            Err(e) => checks.push(check("herdr socket", false, format!("{}: {e}", p.display()))),
        },
    }

    checks.push(worktree_dirty_check(&read_worktree_dirty(&InstancePaths::new(&root).worktree_dirty)));
    instance_checks(&root, &mut checks);

    let status = Client::connect(&sock, Duration::from_secs(2)).ok().and_then(|mut c| c.call("status", serde_json::json!({})).ok());
    let comp = |name: &str| status.as_ref().and_then(|s| s["components"].get(name)).cloned();
    checks.push(writer_check(comp("writer").as_ref()));
    let inputs = DiscoveryInputs {
        env_state_dir: env.threads_state_dir.clone(),
        config_state_dir: crate::config::read_threads_state_dir(env, &plugin_config_dir_via_herdr),
        own_plugin_state_dir: env.plugin_state_dir.clone(),
        xdg_state_home: env.xdg_state_home.clone(),
        home: env.home.clone(),
    };
    checks.push(threads_check(resolve_state_dir(&inputs), comp("threads").as_ref()));
    match comp("journal") {
        Some(j) => checks.push(check("journal", true, j.to_string())),
        None => checks.push(check("journal", true, "no counts reported")),
    }
    DoctorReport { checks }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_dirty_parses_lines_and_skips_garbage() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("worktree_dirty");
        std::fs::write(
            &f,
            "{\"path\":\"teamspaces/a/ts.toml\",\"op\":\"write\",\"at\":\"2026-01-01T00:00:00Z\"}\nnot json\n{\"op\":\"x\"}\n{\"path\":\"b\"}\n",
        )
        .unwrap();
        assert_eq!(read_worktree_dirty(&f), vec!["teamspaces/a/ts.toml", "b"]);
        assert!(read_worktree_dirty(&t.path().join("missing")).is_empty());
    }

    use crate::model::common::Runtime;
    use crate::model::common::ByteRange;
    use crate::model::request::{Delivery, ProcessingRequest};
    use crate::model::{OpId, RequestId, SCHEMA_VERSION, TeamspaceId, TranscriptId};
    use crate::store::{EditSet, GitStore};
    use serde_json::json;

    fn instance() -> (tempfile::TempDir, GitStore) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("inst");
        crate::store::init::init_instance(&dir).unwrap();
        let store = GitStore::open(&dir).unwrap();
        (tmp, store)
    }

    fn commit(store: &GitStore, e: &EditSet) {
        let head = store.head().unwrap();
        let tree = store.build_tree(&head, e).unwrap();
        store
            .with_repo(|r| {
                let sig = git2::Signature::now("t", "t@localhost")?;
                let parent = r.find_commit(git2::Oid::from_str(&head.0)?)?;
                let oid = r.commit(None, &sig, &sig, "test", &r.find_tree(tree)?, &[&parent])?;
                r.reference("refs/heads/main", oid, true, "test")?;
                Ok(())
            })
            .unwrap();
    }

    fn bytes<R: serde::Serialize>(r: &R) -> Vec<u8> {
        crate::store::record::to_toml_bytes(r).unwrap()
    }

    fn teamspace(name: &str, availability: Availability, lifecycle: Lifecycle) -> crate::model::teamspace::TeamspaceRecord {
        crate::model::teamspace::TeamspaceRecord {
            schema: SCHEMA_VERSION,
            id: TeamspaceId::new(),
            rev: 1,
            name: name.into(),
            name_history: vec![],
            lifecycle,
            retired: None,
            runtime: Runtime { availability, bound: None, observed_at: None },
            project_repo: None,
            channel: Default::default(),
        }
    }

    fn request(status: RequestStatus, undeliverable: Option<&str>) -> ProcessingRequest {
        ProcessingRequest {
            schema: SCHEMA_VERSION,
            id: RequestId::new(),
            rev: 1,
            transcript: TranscriptId::new(),
            range: ByteRange { start: 0, end: 1 },
            status,
            unresolved: None,
            undeliverable: undeliverable.map(str::to_owned),
            created_by_op: OpId::new(),
            delivery: Delivery::default(),
            result: None,
        }
    }

    #[test]
    fn writer_check_halted_reason_fails() {
        let c = writer_check(Some(&json!({"writer_halted": "lock contention on main", "ops": {}})));
        assert!(!c.ok);
        assert_eq!(
            c.detail,
            "halted: lock contention on main — fix the cause, then run herdr-graph writer resume"
        );
    }

    #[test]
    fn writer_check_null_is_running() {
        let c = writer_check(Some(&json!({"writer_halted": null})));
        assert!(c.ok);
        assert_eq!(c.detail, "running");
    }

    #[test]
    fn writer_check_absent() {
        let c = writer_check(None);
        assert!(c.ok);
        assert_eq!(c.detail, "not reported by daemon");
    }

    #[test]
    fn failed_ops_listed() {
        use crate::model::change::{ChangeRequest, RequestKind, Requester};
        use chrono::TimeZone;
        let t = tempfile::tempdir().unwrap();
        let j = Journal::open(&t.path().join("j.sqlite3")).unwrap();
        let now = chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap();
        assert!(failed_ops_check(&[]).ok);
        let mut ops = Vec::new();
        for i in 0..12 {
            let req = ChangeRequest {
                kind: RequestKind::Bookkeeping,
                args: json!({ "n": i }),
                relied_on: vec![],
                requester: Requester::default(),
                supersedes: None,
            };
            let op = j.admit(&req, now).unwrap();
            j.begin_applying(&op, now).unwrap();
            j.finish_failed(&op, &format!("poison {i}"), now).unwrap();
            ops.push(op);
        }
        let rows = j.list(&[OpState::Failed], 50).unwrap();
        let c = failed_ops_check(&rows);
        assert!(!c.ok);
        assert!(c.detail.starts_with("12 failed: "), "{}", c.detail);
        assert!(c.detail.contains("bookkeeping: poison 11"), "{}", c.detail);
        assert!(c.detail.contains("+2 more"), "{}", c.detail);
        assert!(c.detail.contains("herdr-graph cancel <op>"), "{}", c.detail);
        assert!(c.detail.contains(ops[11].as_str()));
        assert!(!c.detail.contains("poison 0;") && !c.detail.contains("poison 1;"), "only 10 are listed: {}", c.detail);
    }

    #[test]
    fn unknown_objects_listed() {
        let (_t, store) = instance();
        let mut e = EditSet::default();
        let mystery = teamspace("Mystery", Availability::Unknown, Lifecycle::Active);
        let fine = teamspace("Fine", Availability::Present, Lifecycle::Active);
        let gone = teamspace("Gone", Availability::Unknown, Lifecycle::Retired);
        e.put(layout::teamspace_record(&layout::teamspace_dir("mystery")), bytes(&mystery));
        e.put(layout::teamspace_record(&layout::teamspace_dir("fine")), bytes(&fine));
        e.put(layout::teamspace_record(&layout::teamspace_dir("gone")), bytes(&gone));
        commit(&store, &e);
        let view = CommitView { store: &store, at: store.head().unwrap() };
        let c = unknown_objects_check(&view);
        assert!(!c.ok);
        assert!(c.detail.contains("teamspace Mystery"), "{}", c.detail);
        assert!(!c.detail.contains("Fine") && !c.detail.contains("Gone"), "{}", c.detail);
        assert!(c.detail.starts_with("1 with unknown availability"), "{}", c.detail);

        let mut e = EditSet::default();
        e.put(layout::teamspace_record(&layout::teamspace_dir("mystery")), bytes(&teamspace("Mystery", Availability::Present, Lifecycle::Active)));
        commit(&store, &e);
        let view = CommitView { store: &store, at: store.head().unwrap() };
        assert!(unknown_objects_check(&view).ok);
    }

    #[test]
    fn undeliverable_requests_listed() {
        let (_t, store) = instance();
        let stuck = request(RequestStatus::Pending, Some("no seat bound"));
        let done = request(RequestStatus::Completed, Some("old reason"));
        let healthy = request(RequestStatus::Pending, None);
        let mut e = EditSet::default();
        for r in [&stuck, &done, &healthy] {
            e.put(layout::request_record(&r.id), bytes(r));
        }
        commit(&store, &e);
        let view = CommitView { store: &store, at: store.head().unwrap() };
        let c = undeliverable_check(&view);
        assert!(!c.ok);
        assert_eq!(c.detail, format!("1 stuck: {}: no seat bound", stuck.id));

        let (_t2, empty) = instance();
        let view = CommitView { store: &empty, at: empty.head().unwrap() };
        assert!(undeliverable_check(&view).ok);
    }

    #[test]
    fn worktree_dirty_check_lists_entries() {
        assert!(worktree_dirty_check(&[]).ok);
        let c = worktree_dirty_check(&["a.toml".into(), "b.toml".into()]);
        assert!(!c.ok);
        assert_eq!(c.detail, "2 entries: a.toml, b.toml");
    }

    #[test]
    fn instance_checks_without_journal_reports_none_yet() {
        let (t, _store) = instance();
        let mut checks = Vec::new();
        instance_checks(&t.path().join("inst"), &mut checks);
        let names: Vec<_> = checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["unknown objects", "undeliverable requests", "failed ops"]);
        assert_eq!(checks[2].detail, "no journal yet");
        assert!(checks.iter().all(|c| c.ok), "{checks:?}");
    }

    #[test]
    fn threads_check_connected_ok() {
        let found = Ok(Some((PathBuf::from("/s/herdr-threads"), Source::HomeDefault)));
        let c = threads_check(found, Some(&serde_json::json!({"connected": true, "capability": "service_ack"})));
        assert!(c.ok, "{c:?}");
        assert!(c.detail.contains("state dir /s/herdr-threads"), "{}", c.detail);
        assert!(c.detail.contains("daemon connected"), "{}", c.detail);
        // No daemon: ok follows the discovery result.
        assert!(threads_check(Ok(Some((PathBuf::from("/s"), Source::EnvVar))), None).ok);
    }

    #[test]
    fn threads_check_not_found_fails_with_hint() {
        let c = threads_check(Ok(None), None);
        assert!(!c.ok);
        assert!(c.detail.contains("not found") && c.detail.contains("threads_state_dir"), "{}", c.detail);
        let c = threads_check(Err("both a and b exist".into()), None);
        assert!(!c.ok);
        assert!(c.detail.contains("both a and b exist"), "{}", c.detail);
    }

    #[test]
    fn threads_check_daemon_disconnected_fails() {
        let found = Ok(Some((PathBuf::from("/s/herdr-threads"), Source::XdgDefault)));
        let c = threads_check(found, Some(&serde_json::json!({"connected": false, "error": "refused"})));
        assert!(!c.ok, "a found dir does not make a disconnected daemon ok");
        assert!(c.detail.contains("refused") && c.detail.contains("/s/herdr-threads"), "{}", c.detail);
    }

    #[test]
    fn doctor_without_instance_fails_first_check() {
        let home = tempfile::tempdir().unwrap();
        let env = Env { home: Some(home.path().into()), ..Default::default() };
        let r = doctor(&env);
        assert_eq!(r.checks.len(), 1);
        assert!(!r.all_ok());
        assert_eq!(r.checks[0].name, "instance");
    }
}
