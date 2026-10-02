//! `content` — owned by hg-zmi.13.
use clap::Subcommand;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Write content files belonging to a graph object.
    Content {
        #[command(subcommand)]
        action: ContentAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ContentAction {
    /// Write FROM to REL inside the object's folder.
    Write {
        #[arg(long)]
        object: String,
        #[arg(long)]
        rel: PathBuf,
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        expect: Option<String>,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Content { .. } => super::not_implemented("content"),
    }
}
