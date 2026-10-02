//! `plan`, `apply` — owned by hg-zmi.6. hg-zmi.6 owns the final `plan` grammar.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build a plan for a change (e.g. `plan seat create foreman`).
    Plan {
        #[arg(long)]
        json: bool,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        change: Vec<String>,
    },
    /// Apply a previously built plan.
    Apply {
        plan: String,
        #[arg(long)]
        confirm: Option<String>,
        #[arg(long)]
        confirmed_by: Option<String>,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Plan { .. } => super::not_implemented("plan"),
        Commands::Apply { .. } => super::not_implemented("apply"),
    }
}
