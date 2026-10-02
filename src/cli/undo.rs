//! `undo` — owned by hg-zmi.10.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Undo the most recent undoable action, or list candidates.
    Undo {
        #[arg(long)]
        list: bool,
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Undo { .. } => super::not_implemented("undo"),
    }
}
