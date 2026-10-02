//! `show`, `list`, `path` — unassigned (see plan decision 7); likely hg-zmi.2 / hg-zmi.4.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show one object.
    Show { target: String },
    /// List objects, optionally of one kind.
    List { kind: Option<String> },
    /// Print an object's current repository path.
    Path { id: String },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Show { .. } => super::not_implemented("show"),
        Commands::List { .. } => super::not_implemented("list"),
        Commands::Path { .. } => super::not_implemented("path"),
    }
}
