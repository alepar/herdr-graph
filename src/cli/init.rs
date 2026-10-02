//! `init` — owned by hg-zmi.2.
use clap::Subcommand;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a graph instance (git repo) at PATH.
    Init { path: PathBuf },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Init { .. } => super::not_implemented("init"),
    }
}
