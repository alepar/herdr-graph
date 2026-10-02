//! `setup` — owned by hg-zmi.13.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Install integration for a harness.
    Setup {
        #[command(subcommand)]
        target: SetupTarget,
    },
}

#[derive(Subcommand, Debug)]
pub enum SetupTarget {
    /// Install the Claude hook and /seat skill.
    Claude,
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Setup { .. } => super::not_implemented("setup"),
    }
}
