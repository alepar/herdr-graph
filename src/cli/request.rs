//! `request` — owned by hg-zmi.12 (spec §8.2, §10).
//!
//! `list` is a read: it asks the daemon and, when none is running, reads the committed graph on disk.
//! `ack` and `complete` are mutating (they start the daemon if needed). An ACK records dispatch only;
//! `complete` is what records a result.
use crate::config::{Env, plugin_config_dir_via_herdr};
use crate::daemon::client::{CallMode, ClientError, EXIT_NO_INSTANCE, call_daemon};
use crate::model::ByteRange;
use crate::transcripts::requests::{ListFilter, RequestRow, list_view};
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

/// `<start>-<end>` into a half-open byte range; start must not exceed end.
pub fn parse_covered(s: &str) -> Result<ByteRange, String> {
    let (a, b) = s.split_once('-').ok_or_else(|| format!("--covered takes <start>-<end>, not {s:?}"))?;
    let num = |v: &str| v.trim().parse::<u64>().map_err(|_| format!("--covered takes <start>-<end>, not {s:?}"));
    let (start, end) = (num(a)?, num(b)?);
    ByteRange::new(start, end).ok_or_else(|| format!("--covered start {start} exceeds its end {end}"))
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Request { action } => match action {
            RequestAction::List { pending, unresolved, undispatched } => {
                list(ListFilter { pending, unresolved, undispatched })
            }
            RequestAction::Ack { request } => mutate("request.ack", serde_json::json!({ "request": request })),
            RequestAction::Complete { request, output, covered } => {
                let covered = match parse_covered(&covered) {
                    Ok(c) => c,
                    Err(msg) => {
                        eprintln!("herdr-graph: {msg}");
                        return Ok(ExitCode::from(2));
                    }
                };
                mutate(
                    "request.complete",
                    serde_json::json!({ "request": request, "output": output, "covered": covered }),
                )
            }
        },
    }
}

fn no_instance() -> anyhow::Result<ExitCode> {
    eprintln!("{}", ClientError::NoInstance);
    Ok(ExitCode::from(EXIT_NO_INSTANCE))
}

fn mutate(kind: &str, args: serde_json::Value) -> anyhow::Result<ExitCode> {
    match call_daemon(kind, args, CallMode::Ensure) {
        Ok(v) => {
            println!("{} {}", v["request"].as_str().unwrap_or("?"), v["status"].as_str().unwrap_or("ok"));
            Ok(ExitCode::SUCCESS)
        }
        Err(ClientError::NoInstance) => no_instance(),
        Err(e) => Err(e.into()),
    }
}

/// Daemon down: read the requests straight from the committed graph.
fn direct(filter: ListFilter) -> anyhow::Result<Vec<RequestRow>> {
    let env = Env::from_process();
    let (root, _) = crate::config::locate_instance(&env, &plugin_config_dir_via_herdr).ok_or(ClientError::NoInstance)?;
    let store = crate::store::GitStore::open(&root)?;
    let view = crate::store::tree::CommitView { at: crate::ports::store::Store::head(&store)?, store: &store };
    Ok(list_view(&view, filter)?)
}

fn list(filter: ListFilter) -> anyhow::Result<ExitCode> {
    let rows: Vec<RequestRow> = match call_daemon("request.list", serde_json::to_value(filter)?, CallMode::NoEnsure) {
        Ok(v) => serde_json::from_value(v["requests"].clone())?,
        Err(ClientError::NoInstance) => return no_instance(),
        Err(ClientError::Unavailable(_)) => direct(filter)?,
        Err(e) => return Err(e.into()),
    };
    for row in rows {
        println!("{}", row.render());
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_parses_start_dash_end() {
        assert_eq!(parse_covered("10-20"), Ok(ByteRange { start: 10, end: 20 }));
        assert_eq!(parse_covered("5-5"), Ok(ByteRange { start: 5, end: 5 }));
    }

    #[test]
    fn covered_rejects_reversed_or_malformed() {
        assert!(parse_covered("20-10").unwrap_err().contains("exceeds"));
        assert!(parse_covered("abc").is_err());
        assert!(parse_covered("1-x").is_err());
        assert!(parse_covered("-5").is_err());
    }
}
