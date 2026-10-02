//! `ops`, `op`, `cancel`, `reassign`, `check-instruction`, `rebind` — owned by hg-zmi.17.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List operations.
    Ops {
        #[arg(long)]
        unresolved: bool,
    },
    /// Show one operation.
    Op { id: String },
    /// Cancel an operation.
    Cancel { op: String },
    /// Reassign a rejected operation to another seat.
    Reassign {
        op: String,
        #[arg(long)]
        to: String,
    },
    /// Check whether an instruction's preconditions still hold.
    CheckInstruction { op: String, object: String, rev: u64 },
    /// Rebind a clone to a pane.
    Rebind {
        clone: String,
        #[arg(long)]
        pane: String,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Ops { .. } => super::not_implemented("ops"),
        Commands::Op { .. } => super::not_implemented("op"),
        Commands::Cancel { .. } => super::not_implemented("cancel"),
        Commands::Reassign { .. } => super::not_implemented("reassign"),
        Commands::CheckInstruction { .. } => super::not_implemented("check-instruction"),
        Commands::Rebind { .. } => super::not_implemented("rebind"),
    }
}
