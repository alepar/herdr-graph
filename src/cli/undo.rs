//! `undo` — owned by hg-zmi.10 (spec §6, §10).
//!
//! `undo --list` / `undo --json` print the recent undoable actions (a read: with no daemon running they are
//! computed from the instance on disk). `undo` on a terminal lists them, asks for a number, shows the concrete
//! preview and asks `[y/n]`; there is no bypass. Without a terminal and without flags it prints the list and a hint.
use crate::config::{Env, InstancePaths, plugin_config_dir_via_herdr};
use crate::daemon::client::{CallMode, ClientError, call_daemon};
use crate::store::GitStore;
use crate::undo::candidates::DEFAULT_LIMIT;
use clap::Subcommand;
use serde_json::{Value, json};
use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Undo the most recent undoable action, or list candidates.
    Undo {
        #[arg(long)]
        list: bool,
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Undo { list, json } => undo(list, json),
    }
}

fn remote(e: ClientError) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

/// `{candidates, rendered}`: from the daemon when one runs, else straight from the committed graph.
fn candidates() -> anyhow::Result<Value> {
    match call_daemon(
        "undo.list",
        json!({ "limit": DEFAULT_LIMIT }),
        CallMode::NoEnsure,
    ) {
        Ok(v) => Ok(v),
        Err(ClientError::Unavailable(_)) => {
            let env = Env::from_process();
            let (root, _) = crate::config::locate_instance(&env, &plugin_config_dir_via_herdr)
                .ok_or(ClientError::NoInstance)?;
            let paths = InstancePaths::new(&root);
            let store = GitStore::open(&paths.root)?;
            crate::undo::commands::list_json(&store, DEFAULT_LIMIT)
                .map_err(|e| anyhow::anyhow!("{}", e.message))
        }
        Err(e) => Err(remote(e)),
    }
}

fn undo(list: bool, json: bool) -> anyhow::Result<ExitCode> {
    let reply = candidates()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&reply["candidates"])?);
        return Ok(ExitCode::SUCCESS);
    }
    print!("{}", reply["rendered"].as_str().unwrap_or_default());
    if list {
        return Ok(ExitCode::SUCCESS);
    }
    let cands = reply["candidates"].as_array().cloned().unwrap_or_default();
    if cands.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    if !std::io::stdin().is_terminal() {
        println!(
            "to undo an action, run `herdr-graph undo` in a terminal, or `herdr-graph plan undo <act>` and relay the plan to the user"
        );
        return Ok(ExitCode::SUCCESS);
    }
    interactive(&cands)
}

fn prompt(text: &str) -> anyhow::Result<String> {
    print!("{text}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

/// Terminal-state words of an operation.
fn finished(state: &str) -> bool {
    !matches!(state, "admitted" | "applying")
}

fn interactive(cands: &[Value]) -> anyhow::Result<ExitCode> {
    let answer = prompt("Select action number: ")?;
    let Some(pick) = answer
        .parse::<usize>()
        .ok()
        .and_then(|n| cands.iter().find(|c| c["index"] == n))
    else {
        println!("not applied: {answer:?} is not one of the listed numbers");
        return Ok(ExitCode::from(1));
    };
    let act = pick["act"].as_str().unwrap_or_default().to_owned();
    let plan = call_daemon(
        "plan.create",
        json!({ "words": ["undo", act] }),
        CallMode::Ensure,
    )
    .map_err(remote)?;
    print!("{}", plan["rendered"].as_str().unwrap_or_default());
    if plan["plan"]["repair_required"].is_string() {
        println!("not applied");
        return Ok(ExitCode::from(1));
    }
    if !matches!(
        prompt("Apply this undo? [y/n] ")?
            .to_ascii_lowercase()
            .as_str(),
        "y" | "yes"
    ) {
        println!("not applied");
        return Ok(ExitCode::SUCCESS);
    }
    let admitted = call_daemon(
        "undo.apply",
        json!({ "plan": plan["plan_id"], "confirm": plan["hash"], "mode": "tty" }),
        CallMode::Ensure,
    )
    .map_err(remote)?;
    // Printed before waiting: the undo may close the very pane this runs in.
    let op = admitted["op"].as_str().unwrap_or("<op>").to_owned();
    println!("op {op} admitted");
    std::io::stdout().flush()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let reply =
            call_daemon("ops.get", json!({ "op": op }), CallMode::NoEnsure).map_err(remote)?;
        let state = reply["op"]["state"].as_str().unwrap_or("unknown");
        if finished(state) || Instant::now() >= deadline {
            println!("{op} {state}");
            super::plan::print_if_still_running(&op, state);
            if let Some(c) = reply["commit"].as_str() {
                println!("commit {c}");
            }
            if let Some(r) = reply["op"]["rejection"].as_object() {
                println!(
                    "{}: {}",
                    r.get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("rejected"),
                    r.get("explanation")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                );
            }
            return Ok(super::plan::exit_for_state(state));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
