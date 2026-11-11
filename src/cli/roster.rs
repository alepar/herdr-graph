//! `roster` (spec §2.2): seats and clones with pane, occupant, threads state and instruction status.
//! The group and its args are registered here; `run()` is implemented by hg-blw.1 (`--live` and
//! `--unmanaged` by hg-blw.6).
use crate::cli::daemon_error;
use crate::daemon::client::{CallMode, EXIT_NO_INSTANCE, call_daemon, instance_or_none};
use crate::inspect::output::{Envelope, Format, emit, print};
use crate::inspect::roster::{Roster, RosterQuery, build};
use crate::inspect::scope::{AllowClone, Scope, ScopeArg};
use crate::inspect::{fail, pin_head};
use crate::model::CommitId;
use crate::store::GitStore;
use clap::Subcommand;
use std::io::Write;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Seats and clones with pane, occupant, threads state and instruction status (daemon-free; --live asks the daemon).
    Roster {
        /// Scope: global | teamspace <t> | seat <s>; omitted = whole instance.
        #[arg(num_args = 0..=2)]
        scope: Vec<String>,
        /// Include retired seats and clones.
        #[arg(long)]
        all: bool,
        /// Add live Herdr agent state through the daemon (exit 5 when it is down).
        #[arg(long)]
        live: bool,
        /// With --live: also list Herdr workspaces, tabs and panes graph does not manage.
        #[arg(long, requires = "live")]
        unmanaged: bool,
        /// Print one JSON document (schema 1) on stdout; notices go to stderr.
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    if let Commands::Roster { live: true, .. } = &cmd {
        return live(&cmd);
    }
    let Some(root) = instance_or_none() else {
        return Ok(ExitCode::from(EXIT_NO_INSTANCE));
    };
    let store = match GitStore::open(&root) {
        Ok(s) => s,
        Err(e) => return Ok(fail(&e.into())),
    };
    Ok(
        match execute(
            &store,
            &cmd,
            &mut std::io::stdout().lock(),
            &mut std::io::stderr().lock(),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&e),
        },
    )
}

/// `--live`: one `roster.live` call; the daemon builds the roster at its pinned HEAD and overlays the live
/// data. A down daemon is exit 5 through `daemon_error`; degraded live data still exits 0.
fn live(cmd: &Commands) -> anyhow::Result<ExitCode> {
    let Commands::Roster {
        scope,
        all,
        unmanaged,
        json,
        ..
    } = cmd;
    let args = serde_json::json!({ "scope": scope, "all": all, "unmanaged": unmanaged });
    let reply = match call_daemon("roster.live", args, CallMode::NoEnsure) {
        Ok(v) => v,
        Err(e) => return Ok(daemon_error::report(e)),
    };
    let head = reply["head"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("roster.live reply has no head"))?;
    let roster: Roster = serde_json::from_value(reply["roster"].clone())?;
    print(
        &Envelope::new(&CommitId(head.to_owned()), roster).with_live(true),
        Format::from_flag(*json),
        &[],
    )?;
    Ok(ExitCode::SUCCESS)
}

/// The daemon-free roster: pins HEAD once, resolves the scope and builds the one roster at that commit.
fn execute(
    store: &GitStore,
    cmd: &Commands,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> anyhow::Result<()> {
    let Commands::Roster {
        scope, all, json, ..
    } = cmd;
    let head = pin_head(store)?;
    let arg = ScopeArg::parse(scope, AllowClone::No)?;
    let scope: Scope = Scope::resolve(&arg, store, &head)?;
    let roster = build(store, &head, &RosterQuery { scope, all: *all })?;
    emit(
        &Envelope::new(&head, roster).with_live(false),
        Format::from_flag(*json),
        &[],
        out,
        err,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::fixture::Fx;
    use crate::model::{CloneLifecycle, Lifecycle};
    use serde_json::Value;
    use std::collections::BTreeMap;

    type Snapshot = (String, Vec<(String, String)>, BTreeMap<String, u32>);

    fn snapshot(store: &GitStore) -> Snapshot {
        use crate::ports::store::Store;
        let head = store.head().unwrap().0;
        store
            .with_repo(|r| {
                let idx = r.index()?;
                let entries = idx
                    .iter()
                    .map(|e| {
                        (
                            String::from_utf8_lossy(&e.path).into_owned(),
                            e.id.to_string(),
                        )
                    })
                    .collect();
                let st = r
                    .statuses(None)?
                    .iter()
                    .map(|e| (e.path().unwrap_or("").to_owned(), e.status().bits()))
                    .collect();
                Ok((head.clone(), entries, st))
            })
            .unwrap()
    }

    fn cmd(json: bool) -> Commands {
        Commands::Roster {
            scope: vec![],
            all: true,
            live: false,
            unmanaged: false,
            json,
        }
    }

    #[test]
    fn roster_is_read_only() {
        let mut fx = Fx::new();
        let t = fx.teamspace("proj", Lifecycle::Active);
        let s = fx.seat(&t, "foreman", Lifecycle::Active);
        fx.clone_rec(&s, "main", CloneLifecycle::Active, vec![], None);
        fx.commit("c");
        let before = snapshot(&fx.store);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        execute(&fx.store, &cmd(false), &mut out, &mut err).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("foreman"));
        let (mut jout, mut jerr) = (Vec::new(), Vec::new());
        execute(&fx.store, &cmd(true), &mut jout, &mut jerr).unwrap();
        let docs: Vec<Value> = serde_json::Deserializer::from_slice(&jout)
            .into_iter::<Value>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0]["schema"], 1);
        assert_eq!(docs[0]["live"], false);
        assert!(jerr.is_empty() && err.is_empty());
        assert_eq!(snapshot(&fx.store), before);
    }

    /// Every visible argument of `sub` of the group built from `augment`, with its help line.
    fn missing_help(augment: fn(clap::Command) -> clap::Command, path: &[&str]) -> Vec<String> {
        let mut command = augment(clap::Command::default());
        for name in path {
            command = command
                .find_subcommand(name)
                .unwrap_or_else(|| panic!("no subcommand {name} in {path:?}"))
                .clone();
        }
        assert!(
            command.get_arguments().any(|a| a.get_id() == "json"),
            "{path:?} has no --json"
        );
        command
            .get_arguments()
            .filter(|a| !a.is_hide_set())
            .filter(|a| a.get_help().is_none_or(|h| h.to_string().trim().is_empty()))
            .map(|a| format!("{path:?} {}", a.get_id()))
            .collect()
    }

    #[test]
    fn help_lines_present() {
        use clap::Subcommand as _;
        let mut missing = missing_help(Commands::augment_subcommands, &["roster"]);
        missing.extend(missing_help(
            crate::cli::sessions::Commands::augment_subcommands,
            &["sessions"],
        ));
        missing.extend(missing_help(
            crate::cli::log::Commands::augment_subcommands,
            &["log"],
        ));
        missing.extend(missing_help(
            crate::cli::instructions::Commands::augment_subcommands,
            &["instructions", "stack"],
        ));
        missing.extend(missing_help(
            crate::cli::ops::Commands::augment_subcommands,
            &["ops"],
        ));
        assert!(missing.is_empty(), "arguments without a help line: {missing:?}");
    }

    #[test]
    fn clone_scope_is_a_usage_error() {
        let fx = Fx::new();
        let c = Commands::Roster {
            scope: vec!["clone".into(), "x".into()],
            all: false,
            live: false,
            unmanaged: false,
            json: false,
        };
        let e = execute(&fx.store, &c, &mut Vec::new(), &mut Vec::new()).unwrap_err();
        assert!(e.to_string().contains("clone"), "{e}");
    }
}
