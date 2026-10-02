//! `daemon` — owned by hg-zmi.4.
use crate::config::{Env, plugin_config_dir_via_herdr};
use crate::daemon::client::EXIT_NO_INSTANCE;
use crate::daemon::ensure::{EnsureOutcome, ensure};
use clap::Subcommand;
use std::process::ExitCode;
use std::time::Duration;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the graph daemon (or ensure one is running).
    Daemon {
        #[arg(long)]
        ensure: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    let Commands::Daemon { ensure: ensure_only } = cmd;
    let env = Env::from_process();
    if ensure_only {
        match ensure(&env, Duration::from_secs(10))? {
            EnsureOutcome::NoInstance => {}
            EnsureOutcome::AlreadyRunning { pid } => println!("daemon already running (pid {pid})"),
            EnsureOutcome::Started { pid } => println!("daemon started (pid {pid})"),
        }
        return Ok(ExitCode::SUCCESS);
    }
    let Some((root, _)) = crate::config::locate_instance(&env, &plugin_config_dir_via_herdr) else {
        eprintln!("no instance configured — run herdr-graph init");
        return Ok(ExitCode::from(EXIT_NO_INSTANCE));
    };
    crate::daemon::run_foreground(&root, env.herdr_socket.as_deref())?;
    Ok(ExitCode::SUCCESS)
}
