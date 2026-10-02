//! `who` — owned by hg-zmi.11.
//!
//! Resolves a Herdr pane id (or a threads seat, clone or seat id) to the graph identity it carries. A read:
//! it asks the daemon and, when none is running, answers from the committed graph on disk.
use crate::config::{Env, plugin_config_dir_via_herdr};
use crate::daemon::client::{CallMode, ClientError, EXIT_NO_INSTANCE, call_daemon};
use crate::ports::store::Store;
use crate::threads::{WhoReply, who};
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Resolve which graph object a pane or id refers to.
    Who { target: String },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Who { target } => run_who(&target),
    }
}

/// Daemon down: read the binding index straight from the committed graph.
fn direct(target: &str) -> anyhow::Result<WhoReply> {
    let env = Env::from_process();
    let (root, _) = crate::config::locate_instance(&env, &plugin_config_dir_via_herdr).ok_or(ClientError::NoInstance)?;
    let store = crate::store::GitStore::open(&root)?;
    let head = store.head()?;
    let view = crate::store::tree::CommitView { store: &store, at: head };
    Ok(who(&view, target)?)
}

fn run_who(target: &str) -> anyhow::Result<ExitCode> {
    let reply: WhoReply = match call_daemon("who", serde_json::json!({ "target": target }), CallMode::NoEnsure) {
        Ok(v) => serde_json::from_value(v)?,
        Err(ClientError::NoInstance) => {
            eprintln!("{}", ClientError::NoInstance);
            return Ok(ExitCode::from(EXIT_NO_INSTANCE));
        }
        Err(ClientError::Unavailable(_)) => direct(target)?,
        Err(e) => return Err(e.into()),
    };
    println!("{}", reply.render());
    Ok(ExitCode::SUCCESS)
}
