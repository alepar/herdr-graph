//! `sessions` (spec §2.5): every session of the scoped clones, with its transcript. The group and its
//! args are registered here; `run()` is implemented by hg-blw.3.
use crate::model::Timestamp;
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Sessions of the scoped clones, newest first, with transcript paths (daemon-free).
    Sessions {
        /// Scope: global | teamspace <t> | seat <s> | clone <c>; omitted = whole instance.
        #[arg(num_args = 0..=2)]
        scope: Vec<String>,
        /// Only sessions started at or after this time: RFC 3339, YYYY-MM-DD, or a duration like 2d.
        #[arg(long, value_parser = crate::inspect::parse_since)]
        since: Option<Timestamp>,
        /// Maximum rows (default 50).
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
}

pub fn run(_cmd: Commands) -> anyhow::Result<ExitCode> {
    anyhow::bail!("sessions: not implemented yet")
}
