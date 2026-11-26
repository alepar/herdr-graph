//! `adopt` (onboarding design): bringing existing Herdr/threads spaces under graph. This group holds the
//! read-only `inspect`; it plans and changes nothing.
use crate::cli::daemon_error;
use crate::daemon::client::{CallMode, call_daemon};
use clap::Subcommand;
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Read-only facts about an unmanaged tab or pane for the adopt skill (JSON; asks the daemon).
    Adopt {
        #[command(subcommand)]
        cmd: AdoptCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum AdoptCmd {
    /// Print ids, cwd, git toplevel, agent, process, native session and threads seat of a tab or pane.
    Inspect {
        /// Herdr pane id, tab id, or a tab label that is unique across workspaces.
        target: String,
    },
}

const MEMBERSHIPS_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    let Commands::Adopt {
        cmd: AdoptCmd::Inspect { target },
    } = cmd;
    let mut reply = match call_daemon(
        "adopt.inspect",
        json!({ "target": target }),
        CallMode::NoEnsure,
    ) {
        Ok(v) => v,
        Err(e) => return Ok(daemon_error::report(e)),
    };
    let bin = std::env::var_os("HERDR_THREADS_BIN")
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "herdr-threads".into());
    add_memberships(&mut reply, Path::new(&bin));
    println!("{}", serde_json::to_string_pretty(&reply)?);
    Ok(ExitCode::SUCCESS)
}

/// Fills `threads.memberships` (or `memberships` null plus `memberships_error`) of every pane that has a seat.
fn add_memberships(reply: &mut Value, bin: &Path) {
    let Some(panes) = reply.get_mut("panes").and_then(Value::as_array_mut) else {
        return;
    };
    for pane in panes {
        let Some(seat) = pane["threads"]["seat"].as_str().map(str::to_owned) else {
            continue;
        };
        let threads = &mut pane["threads"];
        match memberships_with(bin, &seat) {
            Ok(v) => threads["memberships"] = v,
            Err(e) => {
                threads["memberships"] = Value::Null;
                threads["memberships_error"] = json!(e);
            }
        }
    }
}

/// The seat's threads (first page, verbatim) from the user's threads CLI. The graph has no service operation
/// for memberships, so this is the one place that shells out; a failure is an error string, never "no threads".
fn memberships_with(bin: &Path, seat: &str) -> Result<Value, String> {
    let mut child = Command::new(bin)
        .args(["--json", "thread", "list", "--seat", seat])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", bin.display()))?;
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        stdout.read_to_string(&mut buf).map(|_| buf)
    });
    let deadline = Instant::now() + MEMBERSHIPS_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} timed out after {}s",
                    bin.display(),
                    MEMBERSHIPS_TIMEOUT.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("waiting for {}: {e}", bin.display())),
        }
    };
    let out = reader
        .join()
        .map_err(|_| "reader thread panicked".to_owned())?
        .map_err(|e| format!("reading {}: {e}", bin.display()))?;
    if !status.success() {
        return Err(format!("{} exited with {status}", bin.display()));
    }
    serde_json::from_str(&out).map_err(|e| format!("{} printed invalid JSON: {e}", bin.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn memberships_returns_the_cli_json_verbatim_and_passes_the_seat() {
        let d = tempfile::tempdir().unwrap();
        // echoes its argv into the JSON so the argv shape is asserted, not just the exit
        let bin = script(d.path(), "ok", r#"printf '{"items":[],"argv":"%s"}' "$*""#);
        let v = memberships_with(&bin, "s1").unwrap();
        assert_eq!(v["items"], json!([]));
        assert_eq!(v["argv"], json!("--json thread list --seat s1"));
    }

    #[test]
    fn memberships_failures_are_errors_not_empty() {
        let d = tempfile::tempdir().unwrap();
        let fail = script(d.path(), "fail", "exit 1");
        assert!(
            memberships_with(&fail, "s1")
                .unwrap_err()
                .contains("exited")
        );
        let junk = script(d.path(), "junk", "echo not-json");
        assert!(
            memberships_with(&junk, "s1")
                .unwrap_err()
                .contains("invalid JSON")
        );
        let missing = d.path().join("absent");
        assert!(
            memberships_with(&missing, "s1")
                .unwrap_err()
                .contains("cannot run")
        );
    }

    #[test]
    fn add_memberships_fills_only_panes_with_a_seat_and_records_failure() {
        let d = tempfile::tempdir().unwrap();
        let ok = script(d.path(), "ok", r#"echo '{"items":[]}'"#);
        let mut reply = json!({"panes": [
            {"threads": {"seat": "s1"}},
            {"threads": {"seat": null, "seat_note": "no agent in pane; not resolved"}},
        ]});
        add_memberships(&mut reply, &ok);
        assert_eq!(
            reply["panes"][0]["threads"]["memberships"],
            json!({"items": []})
        );
        assert!(reply["panes"][1]["threads"].get("memberships").is_none());
        let bad = script(d.path(), "bad", "exit 3");
        add_memberships(&mut reply, &bad);
        assert_eq!(reply["panes"][0]["threads"]["memberships"], Value::Null);
        assert!(
            reply["panes"][0]["threads"]["memberships_error"]
                .as_str()
                .unwrap()
                .contains("exited")
        );
    }
}
