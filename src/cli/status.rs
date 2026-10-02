//! `status`, `doctor` — owned by hg-zmi.4.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show daemon and instance status.
    Status,
    /// Diagnose the installation and instance.
    Doctor,
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Status => super::not_implemented("status"),
        Commands::Doctor => super::not_implemented("doctor"),
    }
}
