//! `instructions stack` binary smoke test: daemon-free, unknown seat is exit 1, help lists the subcommand.
mod support;

use herdr_graph::model::clone::CloneRecord;
use herdr_graph::model::seat::SeatRecord;
use herdr_graph::model::teamspace::TeamspaceRecord;
use herdr_graph::model::{CloneId, CloneLifecycle, Lifecycle, SCHEMA_VERSION, SeatId, TeamspaceId};
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
fn stack_unknown_seat_exits_1() {
    let root = TestRoot::new();
    init_instance(&root.instance()).unwrap();
    let out = cmd(&root, &["instructions", "stack", "nosuch"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.starts_with("herdr-graph: ") && err.contains("nosuch"),
        "{err}"
    );
    assert!(out.stdout.is_empty());
    assert!(
        !herdr_graph::config::InstancePaths::new(&root.instance())
            .socket
            .exists()
    );
}

#[test]
fn instructions_help_lists_stack() {
    let root = TestRoot::new();
    let out = cmd(&root, &["instructions", "--help"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("stack"));
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

/// Two seats in one teamspace, each with a clone named `main`. Returns the two clone ids.
fn two_seats_with_main_clones(root: &TestRoot) -> (String, String) {
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
    let mut e = EditSet::default();
    e.put(
        layout::teamspace_record(&ts_dir),
        record::to_toml_bytes(&ts).unwrap(),
    );
    let mut ids = Vec::new();
    for name in ["alpha", "beta"] {
        let seat = SeatRecord {
            schema: SCHEMA_VERSION,
            id: SeatId::new(),
            rev: 1,
            name: name.into(),
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
        let seat_dir = layout::seat_dir(&ts_dir, name);
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
        e.put(
            layout::seat_record(&seat_dir),
            record::to_toml_bytes(&seat).unwrap(),
        );
        e.put(
            layout::clone_record(&layout::clone_dir(&seat_dir, "main")),
            record::to_toml_bytes(&clone).unwrap(),
        );
        ids.push(clone.id.to_string());
    }
    commit(&store, &e, "two seats, each with a clone named main");
    (ids[0].clone(), ids[1].clone())
}

#[test]
fn stack_clone_name_shared_by_two_seats() {
    let root = TestRoot::new();
    init_instance(&root.instance()).unwrap();
    let (alpha_main, beta_main) = two_seats_with_main_clones(&root);
    for (seat, want, other) in [
        ("alpha", &alpha_main, &beta_main),
        ("beta", &beta_main, &alpha_main),
    ] {
        let out = cmd(
            &root,
            &["instructions", "stack", seat, "--clone", "main", "--json"],
        )
        .output()
        .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{seat}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let clones = v["clones"].as_array().unwrap();
        assert_eq!(clones.len(), 1, "{seat}: {clones:?}");
        assert_eq!(clones[0]["id"], want.as_str());
        assert_ne!(clones[0]["id"], other.as_str());
        // The same clone by id still works, and another seat's clone id is refused.
        let by_id = cmd(
            &root,
            &["instructions", "stack", seat, "--clone", want, "--json"],
        )
        .output()
        .unwrap();
        assert_eq!(by_id.status.code(), Some(0));
        let foreign = cmd(
            &root,
            &["instructions", "stack", seat, "--clone", other, "--json"],
        )
        .output()
        .unwrap();
        assert_eq!(foreign.status.code(), Some(1));
    }
}
