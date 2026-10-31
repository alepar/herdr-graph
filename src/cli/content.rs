//! `content` — removed in H1 (spec §1.5). The `write` subcommand keeps its old arguments so existing
//! callers get guidance instead of a usage error; it does nothing and exits 2.
use clap::Subcommand;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Removed: see the guidance printed when run.
    Content {
        #[command(subcommand)]
        action: ContentAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ContentAction {
    /// Removed: summaries use `request complete --summary-from`, instructions use `plan instructions`.
    Write {
        #[arg(long)]
        object: String,
        #[arg(long)]
        rel: PathBuf,
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        expect: Option<String>,
    },
}

pub const STUB_MESSAGE: &str = "content write was removed: summaries use 'herdr-graph request complete <rq> --summary-from <file> --covered <a-b>'; instructions use 'herdr-graph plan instructions set …' then 'herdr-graph apply'";

pub fn run(_cmd: Commands) -> anyhow::Result<ExitCode> {
    eprintln!("herdr-graph: {STUB_MESSAGE}");
    Ok(ExitCode::from(2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    #[test]
    fn content_write_stub_parses_the_old_arguments() {
        let cli = Cli::try_parse_from([
            "herdr-graph",
            "content",
            "write",
            "--object",
            "x",
            "--rel",
            "a",
            "--from",
            "f",
            "--expect",
            "h",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            crate::cli::Command::Content(Commands::Content {
                action: ContentAction::Write { .. }
            })
        ));
    }

    #[test]
    fn stub_message_is_the_guidance_text() {
        assert_eq!(
            STUB_MESSAGE,
            "content write was removed: summaries use 'herdr-graph request complete <rq> --summary-from <file> --covered <a-b>'; instructions use 'herdr-graph plan instructions set …' then 'herdr-graph apply'"
        );
    }

    #[test]
    fn stub_exits_two() {
        let cmd = Commands::Content {
            action: ContentAction::Write {
                object: "x".into(),
                rel: "a".into(),
                from: "f".into(),
                expect: None,
            },
        };
        assert_eq!(
            format!("{:?}", run(cmd).unwrap()),
            format!("{:?}", ExitCode::from(2))
        );
    }
}
