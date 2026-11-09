//! CLI root (spec §10). One file per command group; each group's `Commands` enum is flattened here.
//! Feature beads edit only their own group file — do not add registration lines here.
use clap::{Parser, Subcommand};
use std::process::ExitCode;

pub mod content;
pub mod daemon;
pub mod daemon_error;
pub mod init;
pub mod instructions;
pub mod log;
pub mod node_manager_warning;
pub mod ops;
pub mod plan;
pub mod request;
pub mod roster;
pub mod seat;
pub mod session;
pub mod sessions;
pub mod setup;
pub mod show;
pub mod status;
pub mod template;
pub mod undo;
pub mod who;

#[derive(Parser, Debug)]
#[command(
    name = "herdr-graph",
    version,
    about = "Git-backed organization graph for Herdr"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(flatten)]
    Init(init::Commands),
    #[command(flatten)]
    Daemon(daemon::Commands),
    #[command(flatten)]
    Status(status::Commands),
    #[command(flatten)]
    Setup(setup::Commands),
    #[command(flatten)]
    Seat(seat::Commands),
    #[command(flatten)]
    Session(session::Commands),
    #[command(flatten)]
    Show(show::Commands),
    #[command(flatten)]
    Instructions(instructions::Commands),
    #[command(flatten)]
    Plan(plan::Commands),
    #[command(flatten)]
    Content(content::Commands),
    #[command(flatten)]
    Ops(ops::Commands),
    #[command(flatten)]
    Undo(undo::Commands),
    #[command(flatten)]
    Request(request::Commands),
    #[command(flatten)]
    Who(who::Commands),
    #[command(flatten)]
    Roster(roster::Commands),
    #[command(flatten)]
    Sessions(sessions::Commands),
    #[command(flatten)]
    Log(log::Commands),
}

pub fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    match cli.command {
        Command::Init(c) => init::run(c),
        Command::Daemon(c) => daemon::run(c),
        Command::Status(c) => status::run(c),
        Command::Setup(c) => setup::run(c),
        Command::Seat(c) => seat::run(c),
        Command::Session(c) => session::run(c),
        Command::Show(c) => show::run(c),
        Command::Instructions(c) => instructions::run(c),
        Command::Plan(c) => plan::run(c),
        Command::Content(c) => content::run(c),
        Command::Ops(c) => ops::run(c),
        Command::Undo(c) => undo::run(c),
        Command::Request(c) => request::run(c),
        Command::Who(c) => who::run(c),
        Command::Roster(c) => roster::run(c),
        Command::Sessions(c) => sessions::run(c),
        Command::Log(c) => log::run(c),
    }
}

pub fn main() -> ExitCode {
    let cli = Cli::parse();
    node_manager_warning::warn(&cli.command);
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("herdr-graph: {e:#}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_spec_command_parses() {
        let cases: &[&[&str]] = &[
            &["init", "/tmp/x"],
            &["daemon"],
            &["daemon", "--ensure"],
            &["status"],
            &["doctor"],
            &["setup", "claude"],
            &["setup", "claude", "--uninstall"],
            &["setup", "codex"],
            &["setup", "codex", "--uninstall"],
            &["seat", "--hook-prompt", "--codex"],
            &["seat", "--hook-prompt"],
            &["init", "/tmp/x", "--with-examples"],
            &["seat"],
            &["seat", "--json"],
            &["show", "st_x"],
            &["instructions", "show", "global"],
            &["instructions", "stack", "s1"],
            &["instructions", "stack", "s1", "--clone", "c1", "--json"],
            &["list"],
            &["list", "seats"],
            &["path", "st_x"],
            &["plan", "seat", "create", "foreman", "--json"],
            &["apply", "pl_x"],
            &[
                "apply",
                "pl_x",
                "--confirm",
                "abc",
                "--confirmed-by",
                "user-relay",
            ],
            &[
                "content",
                "write",
                "--object",
                "st_x",
                "--rel",
                "notes/a.md",
                "--from",
                "/tmp/a",
            ],
            &["session-report", "--from-hook", "claude"],
            &["ops"],
            &["ops", "--unresolved"],
            &["op", "op_x"],
            &["cancel", "op_x"],
            &["reassign", "op_x", "--to", "st_x"],
            &["check-instruction", "op_x", "st_x", "3"],
            &["undo"],
            &["undo", "--list"],
            &["undo", "--json"],
            &["request", "list", "--pending"],
            &["request", "ack", "rq_x"],
            &[
                "request",
                "complete",
                "rq_x",
                "--output",
                "o.md",
                "--covered",
                "0-10",
            ],
            &["who", "p1"],
            &["roster"],
            &["roster", "global"],
            &["roster", "seat", "s1", "--all", "--json"],
            &["roster", "--live", "--unmanaged"],
            &["sessions"],
            &[
                "sessions", "clone", "c1", "--since", "2d", "--limit", "5", "--json",
            ],
            &["sessions", "teamspace", "t1", "--since", "2026-11-01"],
            &["log"],
            &[
                "log",
                "seat",
                "s1",
                "--since",
                "2026-11-01T00:00:00Z",
                "--limit",
                "10",
                "--by",
                "s2",
                "--all",
                "--max-scan",
                "100",
                "--json",
            ],
        ];
        for argv in cases {
            let full: Vec<&str> = std::iter::once("herdr-graph")
                .chain(argv.iter().copied())
                .collect();
            Cli::try_parse_from(&full).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        }
    }

    #[test]
    fn inspect_stubs_not_implemented() {
        for argv in [&["sessions"][..]] {
            let full = std::iter::once("herdr-graph").chain(argv.iter().copied());
            let cli = Cli::try_parse_from(full).unwrap();
            let e = run(cli).unwrap_err().to_string();
            assert!(e.contains("not implemented"), "{argv:?}: {e}");
        }
    }

    #[test]
    fn unmanaged_requires_live() {
        assert!(Cli::try_parse_from(["herdr-graph", "roster", "--unmanaged"]).is_err());
    }

    #[test]
    fn bad_since_rejected() {
        assert!(Cli::try_parse_from(["herdr-graph", "log", "--since", "nonsense"]).is_err());
        assert!(Cli::try_parse_from(["herdr-graph", "sessions", "--since", "nonsense"]).is_err());
    }
}
