//! `seat` — owned by hg-zmi.13.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Resolve and show the calling seat.
    Seat {
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Seat { .. } => super::not_implemented("seat"),
    }
}
