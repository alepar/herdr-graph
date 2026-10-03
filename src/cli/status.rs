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
    /// Operate on the single writer.
    Writer {
        #[command(subcommand)]
        action: WriterAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum WriterAction {
    /// Clear a halted writer after a successful probe.
    Resume,
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Status => status(),
        Commands::Doctor => doctor(),
        Commands::Writer {
            action: WriterAction::Resume,
        } => writer_resume(),
    }
}

fn writer_resume() -> anyhow::Result<ExitCode> {
    match call_daemon("writer.resume", serde_json::json!({}), CallMode::Ensure) {
        Ok(v) => {
            println!("{}", resume_message(&v));
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("writer resume failed: {e}");
            Ok(ExitCode::from(1))
        }
    }
}

fn resume_message(v: &serde_json::Value) -> String {
    if v["was_halted"].as_bool().unwrap_or(false) {
        format!(
            "writer resumed (was halted: {})",
            v["reason"].as_str().unwrap_or("unknown reason")
        )
    } else {
        "writer was not halted".to_string()
    }
}

fn status() -> anyhow::Result<ExitCode> {
    match call_daemon("status", serde_json::json!({}), CallMode::NoEnsure) {
        Ok(v) => {
            println!("instance: {}", v["instance"].as_str().unwrap_or("?"));
            println!(
                "daemon: running (pid {}, up {}s)",
                v["pid"], v["uptime_secs"]
            );
            println!(
                "herdr socket: {}",
                v["herdr_socket"].as_str().unwrap_or("?")
            );
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
    println!(
        "journal: {}",
        if paths.journal.exists() {
            "present"
        } else {
            "absent"
        }
    );
    let store = crate::store::GitStore::open(&root)?;
    println!("head: {}", store.head()?.0);
    Ok(ExitCode::SUCCESS)
}

fn doctor() -> anyhow::Result<ExitCode> {
    let report = crate::daemon::doctor::doctor(&Env::from_process());
    for c in &report.checks {
        println!(
            "[{}] {}: {}",
            if c.ok && c.warn {
                "WARN"
            } else if c.ok {
                "ok"
            } else {
                "FAIL"
            },
            c.name,
            c.detail
        );
    }
    Ok(if report.all_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn writer_resume_parses() {
        use clap::Parser;
        let cli = crate::cli::Cli::try_parse_from(["herdr-graph", "writer", "resume"]).unwrap();
        assert!(matches!(
            cli.command,
            crate::cli::Command::Status(Commands::Writer {
                action: WriterAction::Resume
            })
        ));
    }

    #[test]
    fn resume_message_names_reason_or_not_halted() {
        assert_eq!(
            resume_message(&json!({"was_halted": true, "reason": "lock contention"})),
            "writer resumed (was halted: lock contention)"
        );
        assert_eq!(
            resume_message(&json!({"was_halted": false, "reason": null})),
            "writer was not halted"
        );
    }
}
