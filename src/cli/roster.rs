//! `roster` (spec §2.2): seats and clones with pane, occupant, threads state and instruction status.
//! The group and its args are registered here; `run()` is implemented by hg-blw.1 (`--live` and
//! `--unmanaged` by hg-blw.6).
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Seats and clones with pane, occupant, threads state and instruction status (daemon-free; --live asks the daemon).
    Roster {
        /// Scope: global | teamspace <t> | seat <s>; omitted = whole instance.
        #[arg(num_args = 0..=2)]
        scope: Vec<String>,
        /// Include retired seats and clones.
        #[arg(long)]
        all: bool,
        /// Add live Herdr agent state through the daemon (exit 5 when it is down).
        #[arg(long)]
        live: bool,
        /// With --live: also list Herdr workspaces, tabs and panes graph does not manage.
        #[arg(long, requires = "live")]
        unmanaged: bool,
        #[arg(long)]
        json: bool,
    },
}

pub fn run(_cmd: Commands) -> anyhow::Result<ExitCode> {
    anyhow::bail!("roster: not implemented yet")
}
