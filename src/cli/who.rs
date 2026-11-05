//! `who` — owned by hg-zmi.11.
//!
//! Resolves a Herdr pane id (or a threads seat, clone or seat id) to the graph identity it carries. A read:
//! it asks the daemon; with none running it exits 5 through `cli::daemon_error`.
use crate::cli::daemon_error;
use crate::daemon::client::{CallMode, call_daemon};
use crate::threads::WhoReply;
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Resolve which graph object a pane or id refers to.
    Who { target: String },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Who { target } => run_who(&target),
    }
}

fn run_who(target: &str) -> anyhow::Result<ExitCode> {
    let reply: WhoReply = match call_daemon(
        "who",
        serde_json::json!({ "target": target }),
        CallMode::NoEnsure,
    ) {
        Ok(v) => serde_json::from_value(v)?,
        Err(e) => return Ok(daemon_error::report(e)),
    };
    println!("{}", reply.render());
    Ok(ExitCode::SUCCESS)
}
