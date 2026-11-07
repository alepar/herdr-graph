//! who/ops/op/check-instruction/request list/undo have no daemon-free fallback (spec §4.13): with no daemon
//! running they exit 5 through `cli::daemon_error`. `status` keeps its direct read.
mod support;

use herdr_graph::cli::daemon_error::RETRY_LATER;
use herdr_graph::config::InstancePaths;
use herdr_graph::model::{OpId, SeatId};
use herdr_graph::store::init::init_instance;
use std::process::{Command, Output};
use support::isolated::TestRoot;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");

struct Fixture {
    root: TestRoot,
}

impl Fixture {
    fn new() -> Self {
        let root = TestRoot::new();
        init_instance(&root.instance()).unwrap();
        Fixture { root }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = self.root.command(BIN);
        c.args(args)
            .env("HERDR_GRAPH_INSTANCE", self.root.instance());
        c
    }

    fn sock_exists(&self) -> bool {
        InstancePaths::new(&self.root.instance()).socket.exists()
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn assert_exit_5(f: &Fixture, args: &[&str]) {
    assert_exit_5_cmd(f, f.cmd(args), args);
}

fn assert_exit_5_cmd(f: &Fixture, mut cmd: Command, args: &[&str]) {
    let out: Output = cmd.output().unwrap();
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(5), "{args:?}: {err}");
    assert!(
        err.starts_with("herdr-graph: graph daemon unavailable:"),
        "{args:?}: {err}"
    );
    assert!(err.trim_end().ends_with(RETRY_LATER), "{args:?}: {err}");
    assert!(
        out.stdout.is_empty(),
        "{args:?}: stdout {}",
        text(&out.stdout)
    );
    assert!(!f.sock_exists(), "{args:?} must not start a daemon");
}

#[test]
fn reads_without_daemon_exit_5() {
    let f = Fixture::new();
    let op = OpId::new().to_string();
    let seat = SeatId::new().to_string();
    for args in [
        vec!["who", "p1"],
        vec!["ops"],
        vec!["ops", "--unresolved"],
        vec!["op", &op],
        vec!["check-instruction", &op, &seat, "1"],
        vec!["request", "list"],
        vec!["request", "list", "--pending"],
        vec!["undo", "--list"],
        vec!["undo", "--json"],
    ] {
        assert_exit_5(&f, &args);
    }
}

#[test]
fn status_still_reads_directly() {
    let f = Fixture::new();
    let out = f.cmd(&["status"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("not running"));
}

#[test]
fn read_outside_herdr_exits_5() {
    let f = Fixture::new();
    let mut cmd = f.cmd(&["who", "p1"]);
    cmd.env_remove("HERDR_SOCKET_PATH");
    assert_exit_5_cmd(&f, cmd, &["who", "p1"]);
}
