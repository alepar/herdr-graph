//! `roster` binary smoke test: daemon-free, one JSON document, usage error for `clone`.
mod support;

use herdr_graph::store::init::init_instance;
use std::process::Command;
use support::isolated::TestRoot;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");

fn cmd(root: &TestRoot, args: &[&str]) -> Command {
    let mut c = root.command(BIN);
    c.args(args).env("HERDR_GRAPH_INSTANCE", root.instance());
    c
}

fn main_ref(root: &TestRoot) -> String {
    let repo = git2::Repository::open(root.instance()).unwrap();
    repo.refname_to_id("refs/heads/main").unwrap().to_string()
}

#[test]
fn roster_json_without_daemon() {
    let root = TestRoot::new();
    init_instance(&root.instance()).unwrap();
    let head = main_ref(&root);
    for args in [&["roster", "--json"][..], &["roster", "global", "--json"]] {
        let out = cmd(&root, args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let docs: Vec<serde_json::Value> = serde_json::Deserializer::from_slice(&out.stdout)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(docs.len(), 1, "{args:?}");
        assert_eq!(docs[0]["schema"], 1);
        assert_eq!(docs[0]["head"], head.as_str());
        assert_eq!(docs[0]["live"], false);
        assert!(docs[0]["teamspaces"].is_array());
    }
    assert!(
        !herdr_graph::config::InstancePaths::new(&root.instance())
            .socket
            .exists()
    );
}

#[test]
fn roster_clone_scope_is_usage_error() {
    let root = TestRoot::new();
    init_instance(&root.instance()).unwrap();
    let out = cmd(&root, &["roster", "clone", "x"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("herdr-graph: "));
    assert!(out.stdout.is_empty());
}
