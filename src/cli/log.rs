//! `log` (spec §2.4): the scoped history of graph changes. The group and its args are registered here;
//! `run()` is implemented by hg-blw.5.
use crate::model::Timestamp;
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Scoped history of graph changes, newest first (daemon-free).
    Log {
        /// Scope: global | teamspace <t> | seat <s>; omitted = whole instance.
        #[arg(num_args = 0..=2)]
        scope: Vec<String>,
        /// Only changes at or after this time: RFC 3339, YYYY-MM-DD, or a duration like 2d.
        #[arg(long, value_parser = crate::inspect::parse_since)]
        since: Option<Timestamp>,
        /// Maximum rows (default 30).
        #[arg(long, default_value_t = 30)]
        limit: usize,
        /// Only changes requested by this seat (name or id); an AND filter with the scope.
        #[arg(long)]
        by: Option<String>,
        /// Include bookkeeping changes.
        #[arg(long)]
        all: bool,
        /// Commits to scan before stopping (default 5000).
        #[arg(long, default_value_t = 5000)]
        max_scan: usize,
        #[arg(long)]
        json: bool,
    },
}

pub fn run(_cmd: Commands) -> anyhow::Result<ExitCode> {
    anyhow::bail!("log: not implemented yet")
}
