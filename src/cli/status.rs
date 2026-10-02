//! `status`, `doctor` — owned by hg-zmi.4.
use crate::config::{Env, InstancePaths, plugin_config_dir_via_herdr};
use crate::daemon::client::{CallMode, ClientError, EXIT_NO_INSTANCE, call_daemon};
use crate::ports::store::Store;
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show daemon and instance status.
    Status,
    /// Diagnose the installation and instance.
    Doctor,
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Status => status(),
        Commands::Doctor => doctor(),
    }
}

fn status() -> anyhow::Result<ExitCode> {
    match call_daemon("status", serde_json::json!({}), CallMode::NoEnsure) {
        Ok(v) => {
            println!("instance: {}", v["instance"].as_str().unwrap_or("?"));
            println!("daemon: running (pid {}, up {}s)", v["pid"], v["uptime_secs"]);
            println!("herdr socket: {}", v["herdr_socket"].as_str().unwrap_or("?"));
            if let Some(c) = v["components"].as_object() {
                for (name, value) in c {
                    println!("{name}: {value}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(ClientError::NoInstance) => {
            eprintln!("{}", ClientError::NoInstance);
            Ok(ExitCode::from(EXIT_NO_INSTANCE))
        }
        // Daemon-less read: report straight from the instance on disk.
        Err(ClientError::Unavailable(reason)) => direct_status(&reason),
        Err(e) => Err(e.into()),
    }
}

fn direct_status(reason: &str) -> anyhow::Result<ExitCode> {
    let env = Env::from_process();
    let Some((root, _)) = crate::config::locate_instance(&env, &plugin_config_dir_via_herdr) else {
        eprintln!("no instance configured — run herdr-graph init");
        return Ok(ExitCode::from(EXIT_NO_INSTANCE));
    };
    let paths = InstancePaths::new(&root);
    println!("instance: {}", root.display());
    println!("daemon: not running ({reason})");
    println!("journal: {}", if paths.journal.exists() { "present" } else { "absent" });
    let store = crate::store::GitStore::open(&root)?;
    println!("head: {}", store.head()?.0);
    Ok(ExitCode::SUCCESS)
}

fn doctor() -> anyhow::Result<ExitCode> {
    let report = crate::daemon::doctor::doctor(&Env::from_process());
    for c in &report.checks {
        println!("[{}] {}: {}", if c.ok { "ok" } else { "FAIL" }, c.name, c.detail);
    }
    Ok(if report.all_ok() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}
