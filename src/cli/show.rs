//! `show`, `list`, `path` — owned by hg-zmi.13 (spec §9). Direct reads of the committed graph; no daemon.
use crate::bootstrap::show as read;
use crate::daemon::client::{EXIT_NO_INSTANCE, instance_or_none};
use crate::ports::store::Store;
use crate::store::tree::CommitView;
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show one object (by id or name) or a committed path.
    Show { target: String },
    /// List objects, optionally of one kind (teamspaces, seats, clones, applications, templates).
    List { kind: Option<String> },
    /// Print an object's current working-tree path.
    Path { id: String },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    let Some(root) = instance_or_none() else { return Ok(ExitCode::from(EXIT_NO_INSTANCE)) };
    let store = crate::store::GitStore::open(&root)?;
    let view = CommitView { store: &store, at: store.head()? };
    match cmd {
        Commands::Show { target } => print!("{}", ensure_newline(read::show(&view, &root, &target)?)),
        Commands::List { kind } => {
            for row in read::list(&view, &root, kind.as_deref())? {
                println!("{}", row.render());
            }
        }
        Commands::Path { id } => println!("{}", read::path(&view, &root, &id)?.display()),
    }
    Ok(ExitCode::SUCCESS)
}

fn ensure_newline(mut s: String) -> String {
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}
