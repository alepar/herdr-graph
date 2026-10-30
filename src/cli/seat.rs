//! `seat` — owned by hg-zmi.13 (spec §9).
//!
//! `seat [--json]` asks the daemon (ensuring it) to resolve the calling pane and prints the reply: verbatim JSON
//! with `--json`, readable sections otherwise. The hidden `--hook-prompt` is what the SessionStart hook runs:
//! it tells graph panes to run `/seat`, reads the committed graph directly and never fails or starts a daemon.
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
        /// Tell a graph pane's session to run `/seat` (used by the SessionStart hook).
        #[arg(long, hide = true)]
        hook_prompt: bool,
        /// With `--hook-prompt`: print Codex's `hookSpecificOutput.additionalContext` JSON.
        #[arg(long, hide = true, requires = "hook_prompt")]
        codex: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Seat {
            hook_prompt: true,
            codex,
            ..
        } => {
            hook_prompt(codex);
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

/// What the SessionStart hook prints for a graph pane. It fires on startup, resume, clear and compact, and
/// says why: a bare `run /seat` was mostly ignored, above all after compaction.
pub const HOOK_PROMPT: &str = "herdr-graph: this pane is a graph seat. Before replying, run the seat skill (/seat) \
     to load your seat role, rules and instructions. Do this after every start, clear and compaction: a summary \
     of an earlier /seat run is not a substitute, so run it again.";

/// The hook's stdout: plain text for Claude, `additionalContext` JSON for Codex.
pub fn hook_output(codex: bool) -> String {
    if codex {
        serde_json::json!({"hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": HOOK_PROMPT,
        }})
        .to_string()
    } else {
        HOOK_PROMPT.to_owned()
    }
}

/// Never errors: any failure prints nothing.
fn hook_prompt(codex: bool) {
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
        println!("{}", hook_output(codex));
    }
}
