//! `show`, `list`, `path` — owned by hg-zmi.13 (spec §9). Direct reads of the committed graph; no daemon.
use crate::bootstrap::show::{self as read, ShowError};
use crate::cli::daemon_error;
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

/// The exit code and stderr line for a `ShowError` that has a dedicated outcome (a graph integrity failure is
/// exit 6, as for the daemon-backed commands); None leaves it to the caller's generic error path.
fn exit_for(e: &ShowError) -> Option<(u8, String)> {
    match e {
        ShowError::Store(s) => daemon_error::store_failure(s).map(|f| (f.code, f.line)),
        ShowError::Invalid(_) => None,
    }
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    let Some(root) = instance_or_none() else {
        return Ok(ExitCode::from(EXIT_NO_INSTANCE));
    };
    let store = crate::store::GitStore::open(&root)?;
    let view = CommitView {
        store: &store,
        at: store.head()?,
    };
    match cmd {
        Commands::Show { target } => match read::show(&view, &root, &target) {
            Ok(text) => print!("{}", ensure_newline(text)),
            Err(e) => match exit_for(&e) {
                Some((code, line)) => {
                    eprintln!("{line}");
                    return Ok(ExitCode::from(code));
                }
                None => return Err(e.into()),
            },
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::store::{StoreError, unresolved};

    #[test]
    fn exit_for_integrity_is_6() {
        let s = unresolved("st_a", "manager", "st_b");
        let line = crate::cli::daemon_error::integrity_line(&s.to_string());
        assert_eq!(exit_for(&ShowError::Store(s)), Some((6, line)));
    }

    #[test]
    fn exit_for_other_is_none() {
        assert_eq!(exit_for(&ShowError::Invalid("x".into())), None);
        assert_eq!(
            exit_for(&ShowError::Store(StoreError::Git("boom".into()))),
            None
        );
    }
}
