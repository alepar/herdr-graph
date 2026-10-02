//! `daemon` — owned by hg-zmi.4.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the graph daemon (or ensure one is running).
    Daemon {
        #[arg(long)]
        ensure: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Daemon { .. } => super::not_implemented("daemon"),
    }
}
