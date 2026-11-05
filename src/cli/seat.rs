//! `seat` — owned by hg-zmi.13 (spec §9).
//!
//! `seat [--json]` asks the daemon (ensuring it) to resolve the calling pane and prints the reply: verbatim JSON
//! with `--json`, readable sections otherwise. A failed call goes through `cli::daemon_error` (exit 2, 5, 6 or 1)
//! and exits 5 and 6 add the seat sentences ([`seat_failure`]). The hidden `--hook-prompt` is what the SessionStart hook runs:
//! it tells graph panes to run `/seat`, reads the committed graph directly and never fails or starts a daemon.
use crate::bootstrap::instructions::{HOOK_RENDER_CAP, fit_hook, render_instructions};
use crate::bootstrap::resolve::{Resolution, resolve_caller};
use crate::bootstrap::{render_seat, wants_seat_prompt};
use crate::cli::daemon_error::{Outcome, classify};
use crate::config::{Env, plugin_config_dir_via_herdr};
use crate::daemon::client::{CallMode, ClientError, call_daemon, caller_info_from_env};
use crate::daemon::registry::CallerInfo;
use crate::model::CommitId;
use crate::model::launch::ENV_GRAPH;
use crate::ports::store::Store;
use crate::store::tree::TreeRead;
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Resolve and show the calling seat.
    Seat {
        #[arg(long)]
        json: bool,
        /// Print only this seat's instruction render (committed revision; no daemon).
        #[arg(long, conflicts_with_all = ["json", "hook_prompt"])]
        instructions: bool,
        /// Tell a graph pane's session to run `/seat` (used by the SessionStart hook).
        #[arg(long, hide = true)]
        hook_prompt: bool,
        /// With `--hook-prompt`: print Codex's `hookSpecificOutput.additionalContext` JSON.
        #[arg(long, hide = true, requires = "hook_prompt")]
        codex: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    match cmd {
        Commands::Seat {
            hook_prompt: true,
            codex,
            ..
        } => {
            hook_prompt(codex);
            Ok(ExitCode::SUCCESS)
        }
        Commands::Seat {
            instructions: true, ..
        } => Ok(print_instructions()),
        Commands::Seat { json, .. } => {
            let reply = match call_daemon("seat.resolve", serde_json::json!({}), CallMode::Ensure) {
                Ok(v) => v,
                Err(e) => {
                    let (code, line) = seat_failure(&e, Env::from_process().herdr_socket.is_some());
                    eprintln!("{line}");
                    return Ok(ExitCode::from(code));
                }
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&reply)?);
            } else {
                print!("{}", render_seat(&reply));
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Appended to the generic failure line when the daemon is unavailable (exit 5).
pub const SEAT_UNAVAILABLE: &str = "Your seat context is not loaded. Tell the user, carry on with what you can without seat-specific rules, and run /seat again later. Do not guess your seat.";
/// Appended to the generic failure line on a graph integrity error (exit 6).
pub const SEAT_INTEGRITY: &str = "Your seat context is not loaded. Tell the user, carry on with what you can without seat-specific rules, and do not retry. Do not guess your seat.";

/// The full stderr text and exit code of a failed `seat`: the generic failure plus the seat sentences for exits 5 and 6.
pub fn seat_failure(e: &ClientError, in_herdr: bool) -> (u8, String) {
    let f = classify(e, in_herdr);
    let line = match f.outcome {
        Outcome::Unavailable => format!("{} {SEAT_UNAVAILABLE}", f.line),
        Outcome::Integrity => format!("{} {SEAT_INTEGRITY}", f.line),
        Outcome::NotInHerdr | Outcome::NoInstance | Outcome::Other => f.line,
    };
    (f.code, line)
}

/// What the SessionStart hook prints for a graph pane without a render (an unbound pane that is still a graph
/// pane). It fires on startup, resume, clear and compact, and says why: a bare `run /seat` was mostly ignored,
/// above all after compaction.
pub const HOOK_PROMPT: &str = "herdr-graph: this pane is a graph seat. Before replying, run the seat skill (/seat) \
     to load your seat role, rules and instructions. Do this after every start, clear and compaction: a summary \
     of an earlier /seat run is not a substitute, so run it again. If /seat fails, follow the skill's failure step.";

/// The hook's stdout: plain text for Claude, `additionalContext` JSON for Codex.
///
/// Trigger coverage. Claude: the installed group has an empty matcher, so it fires on startup, resume, clear and
/// compact. Codex (0.163.0 and 0.166.0, live `/compact` runs): the matcher-less group fires on startup, resume,
/// clear and compact as well; the hook runs only after the user trusts it. If it did not fire, the seat
/// skill tells Codex agents to run `herdr-graph seat --instructions`.
/// Size: no documented Claude Code hook output limit was found in `claude --help` or the installed binary, so
/// `HOOK_RENDER_CAP` keeps its conservative 9000 bytes.
pub fn hook_output_text(codex: bool, text: &str) -> String {
    if codex {
        serde_json::json!({"hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": text,
        }})
        .to_string()
    } else {
        text.to_owned()
    }
}

pub fn hook_output(codex: bool) -> String {
    hook_output_text(codex, HOOK_PROMPT)
}

/// The caller's full instruction render, or None when the caller is not a bound seat or the render fails.
pub fn instructions_text(
    tree: &dyn TreeRead,
    at: &CommitId,
    caller: &CallerInfo,
) -> Option<String> {
    let Resolution::Bound { seat, .. } = resolve_caller(tree, caller, None) else {
        return None;
    };
    render_instructions(tree, at, &seat).ok().map(|r| r.text)
}

/// What the hook prints: header + render (capped) for a bound caller, today's prompt for a graph pane, nothing
/// otherwise. Never errors; any failure falls back to today's prompt.
pub fn hook_text(
    tree: Option<(&dyn TreeRead, &CommitId)>,
    caller: &CallerInfo,
    graph_env: Option<&str>,
) -> Option<String> {
    if let Some((tree, at)) = tree
        && let Resolution::Bound { seat, .. } = resolve_caller(tree, caller, None)
        && let Ok(r) = render_instructions(tree, at, &seat)
    {
        return Some(fit_hook(&r, HOOK_RENDER_CAP).text);
    }
    wants_seat_prompt(graph_env, tree.map(|(t, _)| t), caller.pane_id.as_deref())
        .then(|| HOOK_PROMPT.to_owned())
}

/// Open the located instance's committed head and run `f` on it; None when anything is unavailable.
fn with_head<T>(env: &Env, f: impl FnOnce(&dyn TreeRead, &CommitId) -> T) -> Option<T> {
    let (root, _) = crate::config::locate_instance(env, &plugin_config_dir_via_herdr)?;
    let store = crate::store::GitStore::open(&root).ok()?;
    let head = store.head().ok()?;
    let view = crate::store::tree::CommitView {
        store: &store,
        at: head.clone(),
    };
    Some(f(&view, &head))
}

/// `seat --instructions`: the render, untruncated; exit 1 when this pane is not a bound seat.
fn print_instructions() -> ExitCode {
    let env = Env::from_process();
    let caller = caller_info_from_env();
    match with_head(&env, |t, at| instructions_text(t, at, &caller)).flatten() {
        Some(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("herdr-graph: not a bound graph seat");
            ExitCode::FAILURE
        }
    }
}

/// Never errors: any failure prints today's prompt or nothing.
fn hook_prompt(codex: bool) {
    let env = Env::from_process();
    let caller = caller_info_from_env();
    let graph = std::env::var(ENV_GRAPH).ok();
    let text = with_head(&env, |t, at| {
        hook_text(Some((t, at)), &caller, graph.as_deref())
    })
    .unwrap_or_else(|| hook_text(None, &caller, graph.as_deref()));
    if let Some(t) = text {
        println!("{}", hook_output_text(codex, &t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    #[test]
    fn seat_failure_appends_retry_sentences_for_5() {
        let (code, line) = seat_failure(&ClientError::Unavailable("x".into()), true);
        assert_eq!(code, 5);
        assert_eq!(
            line,
            format!(
                "{} {SEAT_UNAVAILABLE}",
                crate::cli::daemon_error::unavailable_line("x")
            )
        );
    }

    #[test]
    fn seat_failure_appends_no_retry_sentences_for_6() {
        let e = ClientError::Remote {
            code: crate::ipc::IpcErrorCode::Integrity,
            message: "st_a: teamspace ts_b not found".into(),
        };
        let (code, line) = seat_failure(&e, true);
        assert_eq!(code, 6);
        assert_eq!(
            line,
            format!(
                "{} {SEAT_INTEGRITY}",
                crate::cli::daemon_error::integrity_line("st_a: teamspace ts_b not found")
            )
        );
        assert!(!line.contains("run /seat again later"));
    }

    #[test]
    fn seat_failure_leaves_other_codes_alone() {
        assert_eq!(
            seat_failure(&ClientError::NotInHerdr, false),
            (2, crate::cli::daemon_error::NOT_IN_HERDR.to_owned())
        );
        assert_eq!(
            seat_failure(&ClientError::NoInstance, true),
            (2, ClientError::NoInstance.to_string())
        );
    }

    #[test]
    fn hook_prompt_names_the_failure_step() {
        let clause = "If /seat fails, follow the skill's failure step.";
        assert!(HOOK_PROMPT.ends_with(clause));
        let v: serde_json::Value = serde_json::from_str(&hook_output(true)).unwrap();
        assert!(
            v["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .contains(clause)
        );
    }

    #[test]
    fn claude_and_codex_shapes() {
        assert_eq!(hook_output_text(false, "x"), "x");
        let v: serde_json::Value = serde_json::from_str(&hook_output_text(true, "x")).unwrap();
        assert_eq!(v["hookSpecificOutput"]["additionalContext"], "x");
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
    }

    #[test]
    fn seat_instructions_flag_parses() {
        assert!(Cli::try_parse_from(["herdr-graph", "seat", "--instructions"]).is_ok());
        assert!(Cli::try_parse_from(["herdr-graph", "seat", "--instructions", "--json"]).is_err());
    }

    #[test]
    fn claude_hook_group_has_empty_matcher() {
        let tmp = tempfile::tempdir().unwrap();
        crate::bootstrap::setup_claude::install(
            tmp.path(),
            std::path::Path::new("/bin/herdr-graph"),
        )
        .unwrap();
        let s: serde_json::Value =
            serde_json::from_slice(&std::fs::read(tmp.path().join("settings.json")).unwrap())
                .unwrap();
        let groups = s["hooks"]["SessionStart"].as_array().unwrap();
        assert!(
            groups.iter().any(|g| g["matcher"] == ""
                && g["hooks"][0]["command"]
                    .as_str()
                    .unwrap()
                    .contains("seat --hook-prompt")),
            "{s}"
        );
    }
}
