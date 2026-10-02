//! `ops`, `op`, `cancel`, `reassign`, `check-instruction`, `rebind` — owned by hg-zmi.17.
//!
//! Reads (`ops`, `op`, `check-instruction`) never start a daemon: with none running they read the journal and
//! the committed graph straight from the instance. `cancel` and `reassign` mutate, so they ensure the daemon.
use crate::config::{Env, InstancePaths, plugin_config_dir_via_herdr};
use crate::daemon::client::{CallMode, ClientError, call_daemon};
use crate::journal::Journal;
use crate::model::{AnyId, OpId};
use crate::plan::ops;
use clap::Subcommand;
use serde_json::{Value, json};
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List operations.
    Ops {
        #[arg(long)]
        unresolved: bool,
    },
    /// Show one operation.
    Op { id: String },
    /// Cancel an operation.
    Cancel { op: String },
    /// Reassign a rejected operation to another seat.
    Reassign {
        op: String,
        #[arg(long)]
        to: String,
    },
    /// Check whether an instruction's preconditions still hold.
    CheckInstruction { op: String, object: String, rev: u64 },
    /// Rebind a clone to a pane.
    Rebind {
        clone: String,
        #[arg(long)]
        pane: String,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Ops { unresolved } => list(unresolved),
        Commands::Op { id } => show(&id),
        Commands::Cancel { op } => cancel(&op),
        Commands::Reassign { op, to } => reassign(&op, &to),
        Commands::CheckInstruction { op, object, rev } => check_instruction(&op, &object, rev),
        Commands::Rebind { clone, pane } => {
            crate::cli::plan::plan_flow(vec!["clone".into(), "rebind".into(), clone, "--pane".into(), pane], false)
        }
    }
}

/// The instance's journal and graph, opened without a daemon.
struct Direct {
    paths: InstancePaths,
}

fn direct() -> anyhow::Result<Direct> {
    let env = Env::from_process();
    let (root, _) = crate::config::locate_instance(&env, &plugin_config_dir_via_herdr)
        .ok_or(ClientError::NoInstance)?;
    Ok(Direct { paths: InstancePaths::new(&root) })
}

impl Direct {
    /// The journal, or None when the instance has never admitted an op (opening would create it).
    fn journal(&self) -> anyhow::Result<Option<Journal>> {
        if !self.paths.journal.exists() {
            return Ok(None);
        }
        Ok(Some(Journal::open(&self.paths.journal)?))
    }
}

/// A read: ask the daemon, and when none is running compute the same answer from the instance on disk.
fn read(kind: &str, args: Value, fallback: impl FnOnce(&Direct) -> anyhow::Result<Value>) -> anyhow::Result<Value> {
    match call_daemon(kind, args, CallMode::NoEnsure) {
        Ok(v) => Ok(v),
        Err(ClientError::Unavailable(_)) => fallback(&direct()?),
        Err(e) => Err(anyhow::anyhow!("{e}")),
    }
}

fn remote(e: ClientError) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

fn parse_op(s: &str) -> anyhow::Result<OpId> {
    s.parse::<OpId>().map_err(|e| anyhow::anyhow!("operation id {s:?}: {e}"))
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("?")
}

/// `<op> <state> <summary>` plus the rejection, one op per block.
fn render_op(o: &Value) -> String {
    let mut out = format!("{} {} {}\n", text(&o["op"]), text(&o["state"]), text(&o["summary"]));
    if let Some(r) = o["rejection"].as_object() {
        let reason = r.get("reason").and_then(Value::as_str).unwrap_or("?");
        let explanation = r.get("explanation").and_then(Value::as_str).unwrap_or_default();
        out.push_str(&format!("  {reason}: {explanation}\n"));
    }
    if let Some(by) = o["superseded_by"].as_str() {
        out.push_str(&format!("  superseded by {by}\n"));
    }
    out
}

fn list(unresolved: bool) -> anyhow::Result<ExitCode> {
    let reply = read("ops.list", json!({ "unresolved": unresolved }), |d| {
        let Some(journal) = d.journal()? else { return Ok(json!({ "ops": [] })) };
        Ok(ops::list_json(&journal, unresolved, 200)?)
    })?;
    let list = reply["ops"].as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        println!("no {}operations", if unresolved { "unresolved " } else { "" });
    }
    for o in &list {
        print!("{}", render_op(o));
    }
    Ok(ExitCode::SUCCESS)
}

fn show(id: &str) -> anyhow::Result<ExitCode> {
    let op = parse_op(id)?;
    let reply = read("ops.get", json!({ "op": op }), |d| {
        let journal = d.journal()?.ok_or_else(|| anyhow::anyhow!("unknown operation {op}"))?;
        Ok(ops::op_detail(&journal, &op)?)
    })?;
    print!("{}", render_op(&reply["op"]));
    println!("  requester: {}", reply["op"]["requester"]);
    println!("  attempts: {}", reply["attempts"]);
    if let Some(c) = reply["commit"].as_str() {
        println!("  commit: {c}");
    }
    println!("  reminders sent: {}{}", reply["reminders"]["sent"], if reply["reminders"]["stopped"] == true { " (stopped)" } else { "" });
    Ok(ExitCode::SUCCESS)
}

fn cancel(op: &str) -> anyhow::Result<ExitCode> {
    let op = parse_op(op)?;
    let reply = call_daemon("ops.cancel", json!({ "op": op }), CallMode::Ensure).map_err(remote)?;
    println!("{}", text(&reply["message"]));
    Ok(ExitCode::SUCCESS)
}

fn reassign(op: &str, to: &str) -> anyhow::Result<ExitCode> {
    let op = parse_op(op)?;
    let reply = call_daemon("ops.reassign", json!({ "op": op, "to": to }), CallMode::Ensure).map_err(remote)?;
    println!("{} reassigned to {}", text(&reply["op"]), text(&reply["requester_seat"]));
    Ok(ExitCode::SUCCESS)
}

/// Prints exactly `current` or `obsolete`; both exit 0 (the caller branches on the word).
fn check_instruction(op: &str, object: &str, rev: u64) -> anyhow::Result<ExitCode> {
    let op = parse_op(op)?;
    let object = AnyId::parse(object).map_err(|e| anyhow::anyhow!("object id {object:?}: {e}"))?;
    let reply = read("ops.check_instruction", json!({ "op": op, "object": object, "rev": rev }), |d| {
        let journal = d.journal()?.ok_or_else(|| anyhow::anyhow!("unknown operation {op}"))?;
        let store = crate::store::GitStore::open(&d.paths.root)?;
        let status = ops::check_instruction(&journal, &store, &op, &object, rev)?;
        Ok(json!({ "status": status }))
    })?;
    println!("{}", text(&reply["status"]));
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_op_shows_state_summary_rejection_and_supersession() {
        let o = json!({
            "op": "op_X", "state": "rejected", "summary": "seat_rename seat=st_1",
            "rejection": { "reason": "stale_plan", "explanation": "effects changed" },
            "superseded_by": null,
        });
        assert_eq!(render_op(&o), "op_X rejected seat_rename seat=st_1\n  stale_plan: effects changed\n");
        let o = json!({ "op": "op_Y", "state": "superseded", "summary": "s", "rejection": null, "superseded_by": "op_Z" });
        assert_eq!(render_op(&o), "op_Y superseded s\n  superseded by op_Z\n");
    }

    #[test]
    fn id_arguments_are_validated_before_any_call() {
        assert!(parse_op("not-an-op").is_err());
        assert!(check_instruction("not-an-op", "st_x", 1).is_err());
    }
}
