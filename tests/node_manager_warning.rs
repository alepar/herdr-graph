//! The unassigned node-manager warning (spec §4.10) through the real binary under `TestRoot`. No daemon: the
//! instance is built with library calls and git2 only.
mod support;

use herdr_graph::cli::node_manager_warning::warning_text;
use herdr_graph::model::graph::GraphRecord;
use herdr_graph::store::init::init_instance;
use std::path::Path;
use std::process::{Command, Output};
use support::isolated::TestRoot;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");
const SEAT: &str = "st_01JABCDEFGHJKMNPQRSTVWXYZ0";
const TS: &str = "ts_01JABCDEFGHJKMNPQRSTVWXYZ0";
const GHOST: &str = "st_01JABCDEFGHJKMNPQRSTVWXYZ1";

fn commit(dir: &Path, files: &[(&str, String)]) {
    let repo = git2::Repository::open(dir).unwrap();
    let mut index = repo.index().unwrap();
    for (rel, text) in files {
        let target = dir.join(rel);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, text).unwrap();
        index.add_path(Path::new(rel)).unwrap();
    }
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = herdr_graph::store::git::graph_signature().unwrap();
    repo.commit(Some("refs/heads/main"), &sig, &sig, "t", &tree, &[&parent])
        .unwrap();
}

fn seat_files(lifecycle: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "teamspaces/t/teamspace.toml",
            format!("schema = 1\nid = \"{TS}\"\nrev = 1\nname = \"t\"\nlifecycle = \"active\"\n"),
        ),
        (
            "teamspaces/t/seats/s/seat.toml",
            format!(
                "schema = 1\nid = \"{SEAT}\"\nrev = 1\nname = \"s\"\nteamspace = \"{TS}\"\nlifecycle = \"{lifecycle}\"\n"
            ),
        ),
    ]
}

fn graph_with_node_manager(dir: &Path, id: &str) -> (&'static str, String) {
    let mut g: GraphRecord =
        toml::from_str(&std::fs::read_to_string(dir.join("graph.toml")).unwrap()).unwrap();
    g.node_manager_seat = Some(id.parse().unwrap());
    ("graph.toml", toml::to_string(&g).unwrap())
}

fn run(root: &TestRoot, args: &[&str]) -> Output {
    root.command(BIN)
        .args(args)
        .env("HERDR_GRAPH_INSTANCE", root.instance())
        .output()
        .unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn warned(o: &Output) -> usize {
    stderr(o).matches(&warning_text()).count()
}

fn fresh() -> TestRoot {
    let root = TestRoot::new();
    init_instance(&root.instance()).unwrap();
    root
}

#[test]
fn unassigned_graph_with_a_seat_warns_once_and_assignment_silences_it() {
    let root = fresh();
    commit(&root.instance(), &seat_files("active"));

    let before = run(&root, &["list", "seats"]);
    assert_eq!(warned(&before), 1, "{}", stderr(&before));
    assert!(stdout(&before).contains(SEAT), "{}", stdout(&before));
    assert!(!stdout(&before).contains("warning"), "stdout stays clean");

    let nm = graph_with_node_manager(&root.instance(), SEAT);
    commit(&root.instance(), &[nm]);
    let after = run(&root, &["list", "seats"]);
    assert_eq!(warned(&after), 0, "{}", stderr(&after));
    assert_eq!(stdout(&after), stdout(&before));
}

#[test]
fn empty_graph_does_not_warn() {
    let root = fresh();
    let out = run(&root, &["list", "seats"]);
    assert_eq!(warned(&out), 0, "{}", stderr(&out));
}

#[test]
fn dangling_node_manager_warns() {
    let root = fresh();
    let mut files = seat_files("active");
    files.push(graph_with_node_manager(&root.instance(), GHOST));
    commit(&root.instance(), &files);
    let out = run(&root, &["list", "seats"]);
    assert_eq!(warned(&out), 1, "{}", stderr(&out));
}

#[test]
fn retired_node_manager_warns_while_another_seat_is_live() {
    let root = fresh();
    let mut files = seat_files("retired");
    files.push((
        "teamspaces/t/seats/u/seat.toml",
        format!(
            "schema = 1\nid = \"{GHOST}\"\nrev = 1\nname = \"u\"\nteamspace = \"{TS}\"\nlifecycle = \"active\"\n"
        ),
    ));
    files.push(graph_with_node_manager(&root.instance(), SEAT));
    commit(&root.instance(), &files);
    let out = run(&root, &["list", "seats"]);
    assert_eq!(warned(&out), 1, "{}", stderr(&out));
}

#[test]
fn exempt_commands_and_no_instance_do_not_warn() {
    let root = fresh();
    commit(&root.instance(), &seat_files("active"));

    let hook = run(&root, &["seat", "--hook-prompt"]);
    assert_eq!(warned(&hook), 0, "{}", stderr(&hook));

    let other = root.path().join("other");
    let init = run(
        &root,
        &["init", other.to_str().unwrap(), "--no-user-config"],
    );
    assert_eq!(warned(&init), 0, "{}", stderr(&init));

    let mut c: Command = root.command(BIN);
    let none = c
        .args(["list", "seats"])
        .env_remove("HERDR_GRAPH_INSTANCE")
        .output()
        .unwrap();
    assert_eq!(warned(&none), 0, "{}", stderr(&none));
}
