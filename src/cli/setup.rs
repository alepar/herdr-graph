//! `setup` — owned by hg-zmi.13 (spec §9).
use crate::bootstrap::setup_claude::{self, HookChange};
use clap::Subcommand;
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
        /// Remove exactly what setup installed.
        #[arg(long)]
        uninstall: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Setup { target: SetupTarget::Claude { uninstall } } => claude(uninstall),
    }
}

fn claude(uninstall: bool) -> anyhow::Result<ExitCode> {
    let env_dir = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = std::env::var_os("HOME").filter(|v| !v.is_empty()).map(PathBuf::from);
    let config = setup_claude::config_dir(env_dir.as_deref(), home.as_deref())
        .ok_or_else(|| anyhow::anyhow!("neither CLAUDE_CONFIG_DIR nor HOME is set"))?;
    if uninstall {
        let r = setup_claude::uninstall(&config)?;
        println!("claude config: {}", config.display());
        println!("hook: {}", if r.hook_removed { "removed" } else { "not installed" });
        if r.skills_removed.is_empty() {
            println!("skills: none installed");
        } else {
            println!("skills removed: {}", r.skills_removed.join(", "));
        }
        return Ok(ExitCode::SUCCESS);
    }
    let binary = std::env::current_exe()?.canonicalize()?;
    let r = setup_claude::install(&config, &binary)?;
    println!("claude config: {}", config.display());
    println!(
        "hook: {}",
        match r.hook {
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
