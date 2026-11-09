//! `ops`, `op`, `cancel`, `reassign`, `check-instruction` — owned by hg-zmi.17.
//!
//! Reads (`ops`, `op`, `check-instruction`) never start a daemon: with none running they exit 5 through
//! `cli::daemon_error`. `cancel` and `reassign` mutate, so they ensure the daemon.
use crate::cli::daemon_error;
use crate::daemon::client::{CallMode, call_daemon};
use crate::daemon::client::{EXIT_NO_INSTANCE, instance_or_none};
use crate::inspect::output::{Envelope, Format, View, print};
use crate::inspect::scope::{AllowClone, Scope, ScopeArg};
use crate::inspect::{fail, pin_head};
use crate::model::{AnyId, CommitId, OpId};
use clap::Subcommand;
use serde::Serialize;
use serde_json::{Value, json};
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List operations.
    Ops {
        #[arg(long)]
        unresolved: bool,
        /// Only ops requested by this seat (name or id; retired seats included).
        #[arg(long)]
        seat: Option<String>,
        /// Only ops requested from this teamspace (name or id).
        #[arg(long)]
        teamspace: Option<String>,
        /// Maximum rows.
        #[arg(long, default_value_t = 200)]
        limit: usize,
        #[arg(long)]
        json: bool,
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
    CheckInstruction {
        op: String,
        object: String,
        rev: u64,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Ops {
            unresolved,
            seat,
            teamspace,
            limit,
            json,
        } => list(
            unresolved,
            seat.as_deref(),
            teamspace.as_deref(),
            limit,
            json,
        ),
        Commands::Op { id } => show(&id),
        Commands::Cancel { op } => cancel(&op),
        Commands::Reassign { op, to } => reassign(&op, &to),
        Commands::CheckInstruction { op, object, rev } => check_instruction(&op, &object, rev),
    }
}

/// A call to the daemon; a failure is reported through `cli::daemon_error`.
fn call(kind: &str, args: Value, mode: CallMode) -> Result<Value, ExitCode> {
    call_daemon(kind, args, mode).map_err(daemon_error::report)
}

/// A read: ask the daemon, never start one.
fn read(kind: &str, args: Value) -> Result<Value, ExitCode> {
    call(kind, args, CallMode::NoEnsure)
}

fn parse_op(s: &str) -> anyhow::Result<OpId> {
    s.parse::<OpId>()
        .map_err(|e| anyhow::anyhow!("operation id {s:?}: {e}"))
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("?")
}

/// `<op> <state> <summary>` plus the rejection, one op per block.
fn render_op(o: &Value) -> String {
    let mut out = format!(
        "{} {} {}\n",
        text(&o["op"]),
        text(&o["state"]),
        text(&o["summary"])
    );
    if let Some(r) = o["rejection"].as_object() {
        let reason = r.get("reason").and_then(Value::as_str).unwrap_or("?");
        let explanation = r
            .get("explanation")
            .and_then(Value::as_str)
            .unwrap_or_default();
        out.push_str(&format!("  {reason}: {explanation}\n"));
    }
    if let Some(by) = o["superseded_by"].as_str() {
        out.push_str(&format!("  superseded by {by}\n"));
    }
    out
}

/// The `ops --json` body: one row per op, as the daemon's `ops.list` reply carries them.
#[derive(Serialize)]
struct OpsView {
    #[serde(skip)]
    unresolved: bool,
    ops: Vec<Value>,
}

impl View for OpsView {
    fn render_text(&self, out: &mut String) {
        if self.ops.is_empty() {
            out.push_str(&format!(
                "no {}operations\n",
                if self.unresolved { "unresolved " } else { "" }
            ));
        }
        for o in &self.ops {
            out.push_str(&render_op(o));
        }
    }
}

/// The envelope for a daemon `ops.list` reply; its `head` is the daemon's, not a fresh read.
fn envelope_from_reply(reply: &Value, unresolved: bool) -> anyhow::Result<Envelope<OpsView>> {
    let head = reply["head"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("daemon reply has no head"))?;
    let ops = reply["ops"].as_array().cloned().unwrap_or_default();
    Ok(Envelope::new(
        &CommitId(head.to_owned()),
        OpsView { unresolved, ops },
    ))
}

/// Resolves `--seat`/`--teamspace` (name or id, retired included) to ids at one pinned HEAD, so the daemon
/// matches on ids. Failures are exit 1 (unknown, ambiguous) or 6 (integrity) through `inspect::fail`.
fn resolve_filters(
    seat: Option<&str>,
    teamspace: Option<&str>,
) -> Result<(Option<String>, Option<String>), ExitCode> {
    if seat.is_none() && teamspace.is_none() {
        return Ok((None, None));
    }
    let Some(root) = instance_or_none() else {
        return Err(ExitCode::from(EXIT_NO_INSTANCE));
    };
    let resolve = || -> anyhow::Result<(Option<String>, Option<String>)> {
        let store = crate::store::GitStore::open(&root)?;
        let at = pin_head(&store)?;
        let one = |word: &str, name: &str| -> anyhow::Result<Scope> {
            let arg = ScopeArg::parse(&[word.to_owned(), name.to_owned()], AllowClone::No)?;
            Ok(Scope::resolve(&arg, &store, &at)?)
        };
        let seat = match seat {
            Some(s) => match one("seat", s)? {
                Scope::Seat(id) => Some(id.to_string()),
                _ => unreachable!("seat word resolves to a seat"),
            },
            None => None,
        };
        let teamspace = match teamspace {
            Some(t) => match one("teamspace", t)? {
                Scope::Teamspace(id) => Some(id.to_string()),
                _ => unreachable!("teamspace word resolves to a teamspace"),
            },
            None => None,
        };
        Ok((seat, teamspace))
    };
    resolve().map_err(|e| fail(&e))
}

fn list(
    unresolved: bool,
    seat: Option<&str>,
    teamspace: Option<&str>,
    limit: usize,
    json: bool,
) -> anyhow::Result<ExitCode> {
    let (seat, teamspace) = match resolve_filters(seat, teamspace) {
        Ok(ids) => ids,
        Err(code) => return Ok(code),
    };
    let mut args = json!({ "unresolved": unresolved, "limit": limit });
    if let Some(s) = seat {
        args["seat"] = json!(s);
    }
    if let Some(t) = teamspace {
        args["teamspace"] = json!(t);
    }
    let reply = match read("ops.list", args) {
        Ok(v) => v,
        Err(code) => return Ok(code),
    };
    let env = envelope_from_reply(&reply, unresolved)?;
    print(&env, Format::from_flag(json), &[])?;
    Ok(ExitCode::SUCCESS)
}

fn show(id: &str) -> anyhow::Result<ExitCode> {
    let op = parse_op(id)?;
    let reply = match read("ops.get", json!({ "op": op })) {
        Ok(v) => v,
        Err(code) => return Ok(code),
    };
    print!("{}", render_op(&reply["op"]));
    println!("  requester: {}", reply["op"]["requester"]);
    println!("  attempts: {}", reply["attempts"]);
    if let Some(c) = reply["commit"].as_str() {
        println!("  commit: {c}");
    }
    println!(
        "  reminders sent: {}{}",
        reply["reminders"]["sent"],
        if reply["reminders"]["stopped"] == true {
            " (stopped)"
        } else {
            ""
        }
    );
    Ok(ExitCode::SUCCESS)
}

fn cancel(op: &str) -> anyhow::Result<ExitCode> {
    let op = parse_op(op)?;
    let reply = match call("ops.cancel", json!({ "op": op }), CallMode::Ensure) {
        Ok(v) => v,
        Err(code) => return Ok(code),
    };
    println!("{}", text(&reply["message"]));
    Ok(ExitCode::SUCCESS)
}

fn reassign(op: &str, to: &str) -> anyhow::Result<ExitCode> {
    let op = parse_op(op)?;
    let reply = match call(
        "ops.reassign",
        json!({ "op": op, "to": to }),
        CallMode::Ensure,
    ) {
        Ok(v) => v,
        Err(code) => return Ok(code),
    };
    println!(
        "{} reassigned to {}",
        text(&reply["op"]),
        text(&reply["requester_seat"])
    );
    Ok(ExitCode::SUCCESS)
}

/// Prints exactly `current` or `obsolete`; both exit 0 (the caller branches on the word).
fn check_instruction(op: &str, object: &str, rev: u64) -> anyhow::Result<ExitCode> {
    let op = parse_op(op)?;
    let object = AnyId::parse(object).map_err(|e| anyhow::anyhow!("object id {object:?}: {e}"))?;
    let reply = match read(
        "ops.check_instruction",
        json!({ "op": op, "object": object, "rev": rev }),
    ) {
        Ok(v) => v,
        Err(code) => return Ok(code),
    };
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
        assert_eq!(
            render_op(&o),
            "op_X rejected seat_rename seat=st_1\n  stale_plan: effects changed\n"
        );
        let o = json!({ "op": "op_Y", "state": "superseded", "summary": "s", "rejection": null, "superseded_by": "op_Z" });
        assert_eq!(render_op(&o), "op_Y superseded s\n  superseded by op_Z\n");
    }

    #[test]
    fn envelope_head_is_reply_head() {
        let head = "0123456789abcdef0123456789abcdef01234567";
        let reply = json!({ "head": head, "ops": [{ "op": "op_A" }, { "op": "op_B" }] });
        let env = envelope_from_reply(&reply, false).unwrap();
        let v = serde_json::to_value(&env).unwrap();
        assert_eq!(v["schema"], 1);
        assert_eq!(v["head"], head);
        assert_eq!(v["ops"].as_array().unwrap().len(), 2);
        assert!(v.get("unresolved").is_none());
        assert!(envelope_from_reply(&json!({ "ops": [] }), false).is_err());
    }

    #[test]
    fn empty_listing_text_names_unresolved() {
        let mut out = String::new();
        OpsView {
            unresolved: true,
            ops: vec![],
        }
        .render_text(&mut out);
        assert_eq!(out, "no unresolved operations\n");
        out.clear();
        OpsView {
            unresolved: false,
            ops: vec![],
        }
        .render_text(&mut out);
        assert_eq!(out, "no operations\n");
    }

    #[test]
    fn ops_flags_parse() {
        use clap::Parser;
        #[derive(Parser)]
        struct P {
            #[command(subcommand)]
            c: Commands,
        }
        let p = P::try_parse_from([
            "x",
            "ops",
            "--seat",
            "s1",
            "--teamspace",
            "t1",
            "--limit",
            "5",
            "--json",
            "--unresolved",
        ])
        .unwrap();
        match p.c {
            Commands::Ops {
                unresolved,
                seat,
                teamspace,
                limit,
                json,
            } => {
                assert!(unresolved && json);
                assert_eq!(seat.as_deref(), Some("s1"));
                assert_eq!(teamspace.as_deref(), Some("t1"));
                assert_eq!(limit, 5);
            }
            _ => panic!("not ops"),
        }
        let p = P::try_parse_from(["x", "ops"]).unwrap();
        assert!(matches!(
            p.c,
            Commands::Ops {
                limit: 200,
                json: false,
                seat: None,
                ..
            }
        ));
    }

    #[test]
    fn id_arguments_are_validated_before_any_call() {
        assert!(parse_op("not-an-op").is_err());
        assert!(check_instruction("not-an-op", "st_x", 1).is_err());
    }
}
