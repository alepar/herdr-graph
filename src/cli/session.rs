//! `session-report` — owned by hg-zmi.12 (spec §8.1; resolves plan decision 7).
//!
//! Run by the Claude SessionStart hook: reads the hook payload from stdin and reports the native session to
//! the daemon without ever waiting on it: a call of at most `HOOK_CALL_TIMEOUT` that never starts a daemon,
//! and a durable spool file under `.graph-local/` when that fails. It resolves the clone from `HERDR_GRAPH_CLONE`, else `HERDR_PANE_ID` (the daemon looks the
//! binding up); when neither is set it does nothing. It must never fail the hook: every error prints to
//! stderr and the exit status stays 0.
use crate::config::Env;
use crate::daemon::client::{
    CallMode, ClientError, call_daemon_with_timeout, caller_info_from_env,
};
use crate::transcripts::capture::{SpooledReport, parse_claude_hook, report_args, spool_report};
use clap::Subcommand;
use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

/// How long the hook waits for the daemon before spooling the report instead.
pub const HOOK_CALL_TIMEOUT: Duration = Duration::from_secs(1);

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
    let Some(args) = report_args(
        &hook,
        env.graph_clone.as_deref(),
        env.pane_id.as_deref(),
        cwd,
    ) else {
        return Ok(());
    };
    match call_daemon_with_timeout(
        "session.report",
        args.clone(),
        CallMode::NoEnsure,
        HOOK_CALL_TIMEOUT,
    ) {
        Ok(_) | Err(ClientError::NoInstance) => Ok(()),
        Err(e) => {
            let (root, _) =
                crate::config::locate_instance(&env, &crate::config::plugin_config_dir_via_herdr)
                    .ok_or_else(|| anyhow::anyhow!("no instance to spool into ({e})"))?;
            let spooled = SpooledReport {
                version: 1,
                caller: caller_info_from_env(),
                args,
                spooled_at: chrono::Utc::now(),
            };
            spool_report(&root, &spooled)?;
            eprintln!("herdr-graph: session-report: daemon not ready ({e}); spooled");
            Ok(())
        }
    }
}
