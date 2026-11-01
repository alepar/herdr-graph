//! `setup` — owned by hg-zmi.13 (spec §9).
use crate::bootstrap::setup_claude::{self, HookChange, Target};
use clap::Subcommand;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Install integration for a harness.
    Setup {
        #[command(subcommand)]
        target: SetupTarget,
    },
}

#[derive(Subcommand, Debug)]
pub enum SetupTarget {
    /// Install the Claude SessionStart hook and the seat and graph skills.
    Claude {
        /// Remove what setup installed; pre-existing artifacts stay.
        #[arg(long)]
        uninstall: bool,
    },
    /// Install the Codex SessionStart hook and the seat and graph skills.
    Codex {
        /// Remove what setup installed; pre-existing artifacts stay.
        #[arg(long)]
        uninstall: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Setup {
            target: SetupTarget::Claude { uninstall },
        } => {
            let config = setup_claude::config_dir(
                env_path("CLAUDE_CONFIG_DIR").as_deref(),
                home().as_deref(),
            )
            .ok_or_else(|| anyhow::anyhow!("neither CLAUDE_CONFIG_DIR nor HOME is set"))?;
            setup(&setup_claude::CLAUDE, "claude", &config, uninstall)
        }
        Commands::Setup {
            target: SetupTarget::Codex { uninstall },
        } => {
            let config =
                setup_claude::codex_dir(env_path("CODEX_HOME").as_deref(), home().as_deref())
                    .ok_or_else(|| anyhow::anyhow!("neither CODEX_HOME nor HOME is set"))?;
            let code = setup(&setup_claude::CODEX, "codex", &config, uninstall)?;
            if !uninstall {
                println!(
                    "note: Codex runs a new or changed hook only after you trust it: start codex interactively and \
                     review it (or open /hooks)"
                );
            }
            Ok(code)
        }
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home() -> Option<PathBuf> {
    env_path("HOME")
}

fn setup(target: &Target, name: &str, config: &Path, uninstall: bool) -> anyhow::Result<ExitCode> {
    if uninstall {
        let r = setup_claude::uninstall_for(target, config)?;
        println!("{name} config: {}", config.display());
        println!(
            "hook: {}",
            if r.hook_removed {
                "removed"
            } else if r.hook_kept {
                "left in place (present before setup)"
            } else {
                "not installed"
            }
        );
        println!(
            "apply gate: {}",
            if r.gate_removed {
                "removed"
            } else {
                "not installed"
            }
        );
        if r.skills_removed.is_empty() {
            println!("skills: none installed");
        } else {
            println!("skills removed: {}", r.skills_removed.join(", "));
        }
        return Ok(ExitCode::SUCCESS);
    }
    let binary = std::env::current_exe()?.canonicalize()?;
    let r = setup_claude::install_for(target, config, &binary)?;
    println!("{name} config: {}", config.display());
    println!(
        "hook: {}",
        match r.hook {
            HookChange::Installed => "installed",
            HookChange::Updated => "updated",
            HookChange::Unchanged => "already installed",
        }
    );
    println!(
        "apply gate: {}",
        match r.gate {
            HookChange::Installed => "installed",
            HookChange::Updated => "updated",
            HookChange::Unchanged => "already installed",
        }
    );
    if r.skills_written.is_empty() {
        println!("skills: already installed");
    } else {
        println!("skills installed: {}", r.skills_written.join(", "));
    }
    Ok(ExitCode::SUCCESS)
}
