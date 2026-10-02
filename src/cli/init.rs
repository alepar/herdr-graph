//! `init` — owned by hg-zmi.2.
use crate::store::init::{init_instance, write_user_config, UserConfigOutcome};
use clap::Subcommand;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a graph instance (git repo) at PATH.
    Init {
        path: PathBuf,
        /// Also copy the shipped example templates into the instance.
        #[arg(long)]
        with_examples: bool,
        /// Do not write ~/.config/herdr-graph/config.toml (probes, CI); point at the instance with
        /// HERDR_GRAPH_INSTANCE instead.
        #[arg(long)]
        no_user_config: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Init { path, with_examples, no_user_config } => {
            std::fs::create_dir_all(&path)?;
            let path = path.canonicalize()?;
            let commit = init_instance(&path)?;
            println!("instance: {}", path.display());
            println!("commit:   {}", commit.0);
            if with_examples {
                let examples = crate::bootstrap::examples::install_examples(&path)?;
                println!("examples: {} (commit {})", crate::bootstrap::examples::EXAMPLES_DIR, examples.0);
            }
            if no_user_config {
                println!(
                    "config:   not written (--no-user-config); set HERDR_GRAPH_INSTANCE={} to use this instance",
                    path.display()
                );
                return Ok(ExitCode::SUCCESS);
            }
            let home = std::env::var_os("HOME").map(PathBuf::from);
            match home.map(|h| write_user_config(&h, &path)).transpose()? {
                Some(UserConfigOutcome::Written(c)) => println!("config:   wrote {}", c.display()),
                Some(UserConfigOutcome::AlreadyPointsHere(c)) => {
                    println!("config:   {} already points here", c.display())
                }
                Some(UserConfigOutcome::LeftExisting(c)) => println!(
                    "config:   left existing {} unchanged; set HERDR_GRAPH_INSTANCE={} to use this instance",
                    c.display(),
                    path.display()
                ),
                None => println!(
                    "config:   HOME is not set; set HERDR_GRAPH_INSTANCE={} to use this instance",
                    path.display()
                ),
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
