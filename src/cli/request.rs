//! `request` — owned by hg-zmi.12.
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Transcript processing requests.
    Request {
        #[command(subcommand)]
        action: RequestAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum RequestAction {
    /// List processing requests.
    List {
        #[arg(long)]
        pending: bool,
        #[arg(long)]
        unresolved: bool,
        #[arg(long)]
        undispatched: bool,
    },
    /// Acknowledge receipt of a request.
    Ack { request: String },
    /// Complete a request with its output.
    Complete {
        request: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        covered: String,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Request { .. } => super::not_implemented("request"),
    }
}
