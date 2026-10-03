//! `seat` — owned by hg-zmi.13 (spec §9).
//!
//! `seat [--json]` asks the daemon (ensuring it) to resolve the calling pane and prints the reply: verbatim JSON
//! with `--json`, readable sections otherwise. The hidden `--hook-prompt` is what the SessionStart hook runs:
//! it prints `run /seat` for graph panes, reads the committed graph directly and never fails or starts a daemon.
use crate::bootstrap::{render_seat, wants_seat_prompt};
use crate::config::{Env, plugin_config_dir_via_herdr};
use crate::daemon::client::{CallMode, call_daemon};
use crate::model::launch::ENV_GRAPH;
use crate::ports::store::Store;
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Resolve and show the calling seat.
    Seat {
        #[arg(long)]
        json: bool,
        /// Print `run /seat` when this pane is a graph pane (used by the SessionStart hook).
        #[arg(long, hide = true)]
        hook_prompt: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Seat {
            hook_prompt: true, ..
        } => {
            hook_prompt();
            Ok(ExitCode::SUCCESS)
        }
        Commands::Seat { json, .. } => {
            let reply = call_daemon("seat.resolve", serde_json::json!({}), CallMode::Ensure)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&reply)?);
            } else {
                print!("{}", render_seat(&reply));
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Never errors: any failure prints nothing.
fn hook_prompt() {
    let env = Env::from_process();
    let graph = std::env::var(ENV_GRAPH).ok();
    let prompt = if graph.as_deref() == Some("1") {
        true
    } else {
        (|| {
            let (root, _) = crate::config::locate_instance(&env, &plugin_config_dir_via_herdr)?;
            let store = crate::store::GitStore::open(&root).ok()?;
            let head = store.head().ok()?;
            let view = crate::store::tree::CommitView {
                store: &store,
                at: head,
            };
            Some(wants_seat_prompt(None, Some(&view), env.pane_id.as_deref()))
        })()
        .unwrap_or(false)
    };
    if prompt {
        println!("run /seat");
    }
}
