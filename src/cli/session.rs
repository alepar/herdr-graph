//! `session-report` — owned by hg-zmi.12 (spec §8.1; resolves plan decision 7).
//!
//! Run by the Claude SessionStart hook: reads the hook payload from stdin and reports the native session to
//! the daemon. It resolves the clone from `HERDR_GRAPH_CLONE`, else `HERDR_PANE_ID` (the daemon looks the
//! binding up); when neither is set it does nothing. It must never fail the hook: every error prints to
//! stderr and the exit status stays 0.
use crate::config::Env;
use crate::daemon::client::{CallMode, call_daemon};
use crate::transcripts::capture::{parse_claude_hook, report_args};
use clap::Subcommand;
use std::io::Read;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Report a native session from a harness hook.
    SessionReport {
        #[arg(long)]
        from_hook: String,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::SessionReport { from_hook } => {
            if let Err(e) = report(&from_hook) {
                eprintln!("herdr-graph: session-report: {e:#}");
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn report(from_hook: &str) -> anyhow::Result<()> {
    if from_hook != "claude" {
        anyhow::bail!("unsupported hook {from_hook:?} (only `claude`)");
    }
    let env = Env::from_process();
    if env.graph_clone.is_none() && env.pane_id.is_none() {
        return Ok(());
    }
    let mut stdin = String::new();
    std::io::stdin().read_to_string(&mut stdin)?;
    let hook = parse_claude_hook(&stdin)?;
    let cwd = std::env::current_dir().unwrap_or_default();
    let Some(args) = report_args(&hook, env.graph_clone.as_deref(), env.pane_id.as_deref(), cwd) else {
        return Ok(());
    };
    call_daemon("session.report", args, CallMode::Ensure)?;
    Ok(())
}
