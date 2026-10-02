//! `who` — owned by hg-zmi.11.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Resolve which graph object a pane or id refers to.
    Who { target: String },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Who { .. } => super::not_implemented("who"),
    }
}
