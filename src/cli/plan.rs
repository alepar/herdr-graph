//! `plan`, `apply` — owned by hg-zmi.6 (spec §3.3, §10).
//!
//! `plan [--json] <change…>` sends the raw words to the daemon, which parses them with the registered kinds.
//! A human at a terminal sees the plan and a `[y/n]` prompt; there is no bypass flag. An agent runs
//! `plan --json`, shows the plan to the user and, after an explicit yes, runs
//! `apply <pl> --confirm <hash> --confirmed-by user-relay`.
use crate::daemon::client::{CallMode, ClientError, call_daemon};
use clap::Subcommand;
use serde_json::{Value, json};
use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build a plan for a change (e.g. `plan seat create foreman`).
    Plan {
        #[arg(long)]
        json: bool,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        change: Vec<String>,
    },
    /// Apply a previously built plan.
    Apply {
        plan: String,
        #[arg(long)]
        confirm: Option<String>,
        #[arg(long)]
        confirmed_by: Option<String>,
    },
}

/// What `plan` does with the plan it just built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// `--json`: print the reply for an agent relay; never apply.
    Json,
    /// Terminal and no `--json`: show the plan and ask `[y/n]`.
    Prompt,
    /// Not a terminal and no `--json`: print the plan and a relay hint; never apply.
    PrintOnly,
}

pub fn decide(tty: bool, json: bool) -> Decision {
    if json {
        Decision::Json
    } else if tty {
        Decision::Prompt
    } else {
        Decision::PrintOnly
    }
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Plan { json, mut change } => {
            // `plan seat create foo --json` lands in the trailing words; honor it as the flag.
            let trailing_json = change.iter().any(|w| w == "--json");
            change.retain(|w| w != "--json");
            plan_flow(change, json || trailing_json)
        }
        Commands::Apply { plan, confirm, confirmed_by } => apply(plan, confirm, confirmed_by),
    }
}

fn remote(e: ClientError) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

/// The whole plan → confirm flow, reused by other command groups (e.g. `rebind`).
pub fn plan_flow(words: Vec<String>, json: bool) -> anyhow::Result<ExitCode> {
    let reply = call_daemon("plan.create", json!({ "words": words }), CallMode::Ensure).map_err(remote)?;
    let tty = std::io::stdin().is_terminal();
    match decide(tty, json) {
        Decision::Json => {
            println!("{}", serde_json::to_string_pretty(&reply)?);
            Ok(ExitCode::SUCCESS)
        }
        Decision::PrintOnly => {
            print!("{}", reply["rendered"].as_str().unwrap_or_default());
            println!(
                "to apply, show this plan to the user and after an explicit yes run: herdr-graph apply {} --confirm {} --confirmed-by user-relay",
                reply["plan_id"].as_str().unwrap_or("<pl>"),
                reply["hash"].as_str().unwrap_or("<hash>"),
            );
            Ok(ExitCode::SUCCESS)
        }
        Decision::Prompt => {
            print!("{}", reply["rendered"].as_str().unwrap_or_default());
            if reply["plan"]["repair_required"].is_string() {
                println!("not applied");
                return Ok(ExitCode::from(1));
            }
            print!("Apply this plan? [y/n] ");
            std::io::stdout().flush()?;
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            if !matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                println!("not applied");
                return Ok(ExitCode::SUCCESS);
            }
            let result = call_daemon(
                "plan.apply",
                json!({ "plan": reply["plan_id"], "confirm": reply["hash"], "mode": "tty" }),
                CallMode::Ensure,
            )
            .map_err(remote)?;
            report_apply(&result)
        }
    }
}

fn apply(plan: String, confirm: Option<String>, confirmed_by: Option<String>) -> anyhow::Result<ExitCode> {
    let (Some(confirm), Some("user-relay")) = (confirm, confirmed_by.as_deref()) else {
        anyhow::bail!("confirmation required; there is no bypass (apply <pl> --confirm <hash> --confirmed-by user-relay)");
    };
    let result = call_daemon("plan.apply", json!({ "plan": plan, "confirm": confirm, "mode": "relay" }), CallMode::Ensure)
        .map_err(remote)?;
    report_apply(&result)
}

/// Print the op id and final state; exit 1 unless the op committed.
fn report_apply(result: &Value) -> anyhow::Result<ExitCode> {
    let op = result["op"].as_str().unwrap_or("<op>");
    let state = result["state"].as_str().unwrap_or("unknown");
    println!("{op} {state}");
    if let Some(c) = result["commit"].as_str() {
        println!("commit {c}");
    }
    if let Some(r) = result.get("rejection").filter(|r| r.is_object()) {
        println!("{}: {}", r["reason"].as_str().unwrap_or("rejected"), r["explanation"].as_str().unwrap_or_default());
    }
    Ok(if state == "committed" { ExitCode::SUCCESS } else { ExitCode::from(1) })
}
