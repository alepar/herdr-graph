//! `content` — owned by hg-zmi.13 (spec §3.5).
use crate::bootstrap::content::b64_encode;
use crate::daemon::client::{CallMode, call_daemon};
use clap::Subcommand;
use serde_json::json;
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
        /// Refuse the write unless the file's current blob hash is this one.
        #[arg(long)]
        expect: Option<String>,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Content { action: ContentAction::Write { object, rel, from, expect } } => {
            let rel = rel.to_str().ok_or_else(|| anyhow::anyhow!("--rel is not valid UTF-8"))?.to_owned();
            let bytes = std::fs::read(&from).map_err(|e| anyhow::anyhow!("cannot read {}: {e}", from.display()))?;
            let reply = call_daemon(
                "content.write",
                json!({ "object": object, "rel": rel, "bytes_b64": b64_encode(&bytes), "expect": expect }),
                CallMode::Ensure,
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            let state = reply["state"].as_str().unwrap_or("unknown");
            println!("{} {state}", reply["op"].as_str().unwrap_or("<op>"));
            if let Some(c) = reply["commit"].as_str() {
                println!("commit {c}");
            }
            if let Some(r) = reply.get("rejection").filter(|r| r.is_object()) {
                println!("{}: {}", r["reason"].as_str().unwrap_or("rejected"), r["explanation"].as_str().unwrap_or_default());
            }
            Ok(if state == "committed" { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
    }
}
