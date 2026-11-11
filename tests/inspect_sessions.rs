//! `herdr-graph sessions` as a binary, daemon-free, under `TestRoot`.
mod support;

use herdr_graph::model::clone::{CloneRecord, SeatIntent};
use herdr_graph::model::harness::Harness;
use herdr_graph::model::native_session::{NativeSession, SessionEndReason};
use herdr_graph::model::seat::SeatRecord;
use herdr_graph::model::teamspace::TeamspaceRecord;
use herdr_graph::model::{
    CloneId, CloneLifecycle, Lifecycle, NsId, SCHEMA_VERSION, SeatId, TeamspaceId,
};
use herdr_graph::ports::store::Store;
use herdr_graph::store::git::graph_signature;
use herdr_graph::store::init::init_instance;
use herdr_graph::store::{EditSet, GitStore, layout, record};
use std::process::Command;
use support::isolated::TestRoot;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");

fn cmd(root: &TestRoot, args: &[&str]) -> Command {
    let mut c = root.command(BIN);
    c.args(args).env("HERDR_GRAPH_INSTANCE", root.instance());
    c
}

#[test]
fn sessions_json_and_errors_need_no_daemon() {
    let root = TestRoot::new();
    init_instance(&root.instance()).unwrap();

    let out = cmd(&root, &["sessions", "--json"]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema"], 1);
    assert_eq!(v["head"].as_str().unwrap().len(), 40);
    assert_eq!(v["tombstones"], 0);
    assert_eq!(v["total"], 0);
    assert_eq!(v["rows"], serde_json::json!([]));
    assert!(
        !herdr_graph::config::InstancePaths::new(&root.instance())
            .socket
            .exists()
    );

    let out = cmd(&root, &["sessions", "clone", "nosuch"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nosuch"));

    let out = cmd(&root, &["sessions", "--since", "nonsense"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

fn commit(store: &GitStore, edits: &EditSet, msg: &str) {
    let head = store.head().unwrap();
    let tree = store.build_tree(&head, edits).unwrap();
    store
        .with_repo(|r| {
            let sig = graph_signature()?;
            let parent = r.find_commit(git2::Oid::from_str(&head.0)?)?;
            let oid = r.commit(None, &sig, &sig, msg, &r.find_tree(tree)?, &[&parent])?;
            r.reference("refs/heads/main", oid, true, msg)?;
            Ok(())
        })
        .unwrap();
}

fn clone_rec(seat: &SeatRecord, name: &str, sessions: Vec<NativeSession>) -> CloneRecord {
    CloneRecord {
        schema: SCHEMA_VERSION,
        id: CloneId::new(),
        rev: 1,
        seat: seat.id.clone(),
        name: name.into(),
        name_history: vec![],
        lifecycle: CloneLifecycle::Active,
        retired: None,
        runtime: Default::default(),
        occupant: None,
        sessions,
        opt_outs: vec![],
        invitations: vec![],
        reload_required: false,
        threads_seat: None,
        threads_seat_history: vec![],
        seat_intent: None,
        seat_fact: None,
        key_reservation: None,
    }
}

fn native(native_id: &str, started: &str, ended: Option<&str>) -> NativeSession {
    NativeSession {
        id: NsId::new(),
        harness: Harness::Claude,
        native_session_id: native_id.into(),
        transcript_path: None,
        transcript: None,
        cwd: "/work".into(),
        started: started.parse().unwrap(),
        ended: ended.map(|e| e.parse().unwrap()),
        end_reason: ended.map(|_| SessionEndReason::AgentExited),
    }
}

/// One seat with clones `alpha` (s-old, s-new) and `beta` (s-mid), plus a purge tombstone `gone`.
/// Returns the ns ids of the three sessions in the order [s-old, s-mid, s-new] and alpha's id.
fn seed(root: &TestRoot) -> ([String; 3], String) {
    let store = GitStore::open(&root.instance()).unwrap();
    let ts = TeamspaceRecord {
        schema: SCHEMA_VERSION,
        id: TeamspaceId::new(),
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
    let ts_dir = layout::teamspace_dir("proj");
    let seat = SeatRecord {
        schema: SCHEMA_VERSION,
        id: SeatId::new(),
        rev: 1,
        name: "foreman".into(),
        name_history: vec![],
        teamspace: ts.id.clone(),
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
    let seat_dir = layout::seat_dir(&ts_dir, "foreman");
    let old = native(
        "s-old",
        "2026-11-01T10:00:00Z",
        Some("2026-11-01T10:30:00Z"),
    );
    let mid = native("s-mid", "2026-11-02T10:00:00Z", None);
    let new = native("s-new", "2026-11-03T10:00:00Z", None);
    let ids = [old.id.to_string(), mid.id.to_string(), new.id.to_string()];
    let alpha = clone_rec(&seat, "alpha", vec![old, new]);
    let beta = clone_rec(&seat, "beta", vec![mid]);
    let mut gone = clone_rec(&seat, "gone", vec![]);
    gone.lifecycle = CloneLifecycle::Retired;
    gone.threads_seat = Some("seat-1".into());
    gone.seat_intent = Some(SeatIntent::Finalize {
        seat: "seat-1".into(),
    });
    let mut e = EditSet::default();
    e.put(
        layout::teamspace_record(&ts_dir),
        record::to_toml_bytes(&ts).unwrap(),
    );
    e.put(
        layout::seat_record(&seat_dir),
        record::to_toml_bytes(&seat).unwrap(),
    );
    for c in [&alpha, &beta, &gone] {
        e.put(
            layout::clone_record(&layout::clone_dir(&seat_dir, &c.name)),
            record::to_toml_bytes(c).unwrap(),
        );
    }
    commit(&store, &e, "seat with two clones and a tombstone");
    (ids, alpha.id.to_string())
}

#[test]
fn sessions_rows_newest_first_with_clone_scope() {
    let root = TestRoot::new();
    init_instance(&root.instance()).unwrap();
    let ([old, mid, new], alpha) = seed(&root);

    let json = |args: &[&str]| -> serde_json::Value {
        let out = cmd(&root, args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };

    let v = json(&["sessions", "--json"]);
    assert_eq!(
        (v["total"].as_u64(), v["tombstones"].as_u64()),
        (Some(3), Some(1))
    );
    let rows = v["rows"].as_array().unwrap();
    let got: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|r| {
            (
                r["ns"].as_str().unwrap(),
                r["started"].as_str().unwrap(),
                r["clone"]["name"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (new.as_str(), "2026-11-03T10:00:00Z", "alpha"),
            (mid.as_str(), "2026-11-02T10:00:00Z", "beta"),
            (old.as_str(), "2026-11-01T10:00:00Z", "alpha"),
        ]
    );
    assert_eq!(rows[2]["ended"], "2026-11-01T10:30:00Z");
    assert_eq!(rows[2]["end_reason"], "agent_exited");
    assert!(rows[0]["ended"].is_null() && rows[0]["end_reason"].is_null());

    // A clone scope lists only that clone's rows, by name and by id.
    for arg in ["alpha", alpha.as_str()] {
        let v = json(&["sessions", "clone", arg, "--json"]);
        let ns: Vec<&str> = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["ns"].as_str().unwrap())
            .collect();
        assert_eq!(ns, [new.as_str(), old.as_str()], "clone {arg}");
        assert_eq!(v["total"], 2);
    }

    // The text form: newest first, then the tombstone footer; --limit adds the "showing" footer.
    let out = cmd(&root, &["sessions"]).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let started: Vec<&str> = text
        .lines()
        .skip(1)
        .take(3)
        .map(|l| l.split("  ").next().unwrap())
        .collect();
    assert_eq!(
        started,
        [
            "2026-11-03 10:00Z",
            "2026-11-02 10:00Z",
            "2026-11-01 10:00Z"
        ]
    );
    assert!(
        text.ends_with("tombstones in scope: 1 (purged clones' sessions are not listed)\n"),
        "{text}"
    );
    let out = cmd(&root, &["sessions", "--limit", "2"]).output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.ends_with(
            "tombstones in scope: 1 (purged clones' sessions are not listed)\nshowing 2 of 3; use --limit\n"
        ),
        "{text}"
    );
}
