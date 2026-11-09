//! `instructions stack` binary smoke test: daemon-free, unknown seat is exit 1, help lists the subcommand.
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
