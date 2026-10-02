//! `session-report` — unassigned (see plan decision 7); likely hg-zmi.8 / hg-zmi.13.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Report a native session from a harness hook.
    SessionReport {
        #[arg(long)]
        from_hook: String,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::SessionReport { .. } => super::not_implemented("session-report"),
    }
}
