//! CLI root (spec §10). One file per command group; each group's `Commands` enum is flattened here.
//! Feature beads edit only their own group file — do not add registration lines here.
use clap::{Parser, Subcommand};
use std::process::ExitCode;

pub mod content;
pub mod daemon;
pub mod init;
pub mod ops;
pub mod plan;
pub mod request;
pub mod seat;
pub mod session;
pub mod setup;
pub mod show;
pub mod status;
pub mod template;
pub mod undo;
pub mod who;

#[derive(Parser, Debug)]
#[command(name = "herdr-graph", version, about = "Git-backed organization graph for Herdr")]
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
        Command::Plan(c) => plan::run(c),
        Command::Content(c) => content::run(c),
        Command::Ops(c) => ops::run(c),
        Command::Undo(c) => undo::run(c),
        Command::Request(c) => request::run(c),
        Command::Who(c) => who::run(c),
    }
}

pub fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("herdr-graph: {e:#}");
            ExitCode::from(1)
        }
    }
}

/// Placeholder result for command groups whose bead has not landed yet.
pub fn not_implemented(what: &str) -> anyhow::Result<ExitCode> {
    anyhow::bail!("`herdr-graph {what}` is not implemented yet")
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
            &["seat"],
            &["seat", "--json"],
            &["show", "st_x"],
            &["list"],
            &["list", "seats"],
            &["path", "st_x"],
            &["plan", "seat", "create", "foreman", "--json"],
            &["apply", "pl_x"],
            &["apply", "pl_x", "--confirm", "abc", "--confirmed-by", "user-relay"],
            &["content", "write", "--object", "st_x", "--rel", "notes/a.md", "--from", "/tmp/a"],
            &["session-report", "--from-hook", "claude"],
            &["rebind", "cl_x", "--pane", "p1"],
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
            &["request", "complete", "rq_x", "--output", "o.md", "--covered", "0-10"],
            &["who", "p1"],
        ];
        for argv in cases {
            let full: Vec<&str> = std::iter::once("herdr-graph").chain(argv.iter().copied()).collect();
            Cli::try_parse_from(&full).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        }
    }

    #[test]
    fn stub_commands_report_not_implemented() {
        let cli = Cli::try_parse_from(["herdr-graph", "who", "p1"]).unwrap();
        let err = run(cli).unwrap_err();
        assert!(err.to_string().contains("not implemented"));
    }
}
