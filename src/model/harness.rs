//! Harness launch/resume profiles (spec §4.1). The single source for launch args, resume/model args,
//! exit sequence, session-id sources and transcript locator. §4.5 and §8.1 consumers read this table.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Harness {
    Shell,
    Claude,
    Codex,
}

/// One step of the exit sequence, executed with `agent.send_keys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStep {
    Key(&'static str),
    SubmitText(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitSequence {
    pub steps: &'static [KeyStep],
    /// Sent only if the agent is still running after `steps`.
    pub if_still_running: Option<KeyStep>,
}

/// Where the native session id comes from, in priority order (spec §4.1, §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIdSource {
    /// Graph's own Claude SessionStart hook → `herdr-graph session-report`.
    GraphHookReport,
    /// Herdr snapshot `agent_session` (kind id|path).
    HerdrAgentSession,
    /// `pane.process_info` argv: the token after `marker` (codex `resume <id>`).
    ProcessInfoArgv { marker: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptLocator {
    None,
    /// Path-kind agent_session if present; else `${CLAUDE_CONFIG_DIR:-~/.claude}/projects/<slug(cwd)>/<id>.jsonl`;
    /// else glob `projects/*/<id>.jsonl`.
    ClaudeProjects,
    /// Hook-reported transcript_path if any, else `unresolved` (MVP deferral).
    HookReportedOrUnresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessProfile {
    pub harness: Harness,
    /// `agent.start` kind; None = no agent (shell, tests).
    pub agent_kind: Option<&'static str>,
    pub launch_args: &'static [&'static str],
    /// Tokens preceding the session id for resume; None = resume unsupported.
    pub resume_prefix: Option<&'static [&'static str]>,
    pub model_flag: Option<&'static str>,
    pub exit: Option<ExitSequence>,
    pub session_id_sources: &'static [SessionIdSource],
    pub transcript: TranscriptLocator,
}

pub const SHELL: HarnessProfile = HarnessProfile {
    harness: Harness::Shell,
    agent_kind: None,
    launch_args: &[],
    resume_prefix: None,
    model_flag: None,
    exit: None,
    session_id_sources: &[],
    transcript: TranscriptLocator::None,
};
pub const CLAUDE: HarnessProfile = HarnessProfile {
    harness: Harness::Claude,
    agent_kind: Some("claude"),
    launch_args: &[],
    resume_prefix: Some(&["--resume"]),
    model_flag: Some("--model"),
    exit: Some(ExitSequence {
        steps: &[KeyStep::Key("ctrl+c"), KeyStep::Key("ctrl+c")],
        if_still_running: Some(KeyStep::SubmitText("/exit")),
    }),
    session_id_sources: &[
        SessionIdSource::GraphHookReport,
        SessionIdSource::HerdrAgentSession,
    ],
    transcript: TranscriptLocator::ClaudeProjects,
};
pub const CODEX: HarnessProfile = HarnessProfile {
    harness: Harness::Codex,
    agent_kind: Some("codex"),
    launch_args: &["--no-daemon"],
    resume_prefix: Some(&["resume"]),
    model_flag: Some("-m"),
    exit: Some(ExitSequence {
        steps: &[KeyStep::Key("ctrl+c"), KeyStep::Key("ctrl+c")],
        if_still_running: None,
    }),
    session_id_sources: &[
        SessionIdSource::HerdrAgentSession,
        SessionIdSource::ProcessInfoArgv { marker: "resume" },
    ],
    transcript: TranscriptLocator::HookReportedOrUnresolved,
};

pub fn profile(h: Harness) -> &'static HarnessProfile {
    match h {
        Harness::Shell => &SHELL,
        Harness::Claude => &CLAUDE,
        Harness::Codex => &CODEX,
    }
}

impl HarnessProfile {
    pub fn supports_resume(&self) -> bool {
        self.resume_prefix.is_some()
    }
    /// argv order: launch_args ++ resume (if any and supported) ++ model (if any and supported) ++ extra.
    pub fn argv(&self, model: Option<&str>, resume: Option<&str>, extra: &[String]) -> Vec<String> {
        let mut v: Vec<String> = self.launch_args.iter().map(|s| s.to_string()).collect();
        if let (Some(prefix), Some(id)) = (self.resume_prefix, resume) {
            v.extend(prefix.iter().map(|s| s.to_string()));
            v.push(id.to_string());
        }
        if let (Some(flag), Some(m)) = (self.model_flag, model) {
            v.push(flag.to_string());
            v.push(m.to_string());
        }
        v.extend(extra.iter().cloned());
        v
    }
}

/// The harness whose agent kind is `kind` (`agent.start` kind, Herdr's pane `agent.kind`).
pub fn for_agent_kind(kind: &str) -> Option<Harness> {
    [Harness::Claude, Harness::Codex]
        .into_iter()
        .find(|h| profile(*h).agent_kind == Some(kind))
}

impl HarnessProfile {
    /// The inverse of `argv` for a running agent's foreground argv: `(model, args)` after the agent token (its
    /// file name equals `agent_kind`, so `node …/claude` works) with the launch args, the resume prefix and
    /// session id, and the model flag and value removed. `None` when no agent token is found.
    pub fn observed_run(&self, argv: &[String]) -> Option<(Option<String>, Vec<String>)> {
        let kind = self.agent_kind?;
        let at = argv
            .iter()
            .position(|t| Path::new(t).file_name().is_some_and(|n| n == kind))?;
        let mut model = None;
        let mut rest = Vec::new();
        let mut it = argv[at + 1..].iter().peekable();
        while let Some(tok) = it.next() {
            if self.launch_args.contains(&tok.as_str()) {
                continue;
            }
            if self.harness == Harness::Claude {
                if matches!(tok.as_str(), "--continue" | "-c") || tok.starts_with("--resume=") {
                    continue;
                }
                if matches!(tok.as_str(), "--resume" | "-r") {
                    it.next();
                    continue;
                }
            }
            if let Some(prefix) = self.resume_prefix
                && self.harness == Harness::Codex
                && tok.as_str() == prefix[0]
            {
                it.next();
                continue;
            }
            let model_flag = self
                .model_flag
                .into_iter()
                .chain(["--model"])
                .find(|f| tok.as_str() == *f);
            if model_flag.is_some() {
                if let Some(v) = it.next() {
                    model = Some(v.clone());
                }
                continue;
            }
            if let Some(v) = tok.strip_prefix("--model=") {
                model = Some(v.to_owned());
                continue;
            }
            rest.push(tok.clone());
        }
        Some((model, rest))
    }
}

/// Claude project slug: every char not `[A-Za-z0-9]` becomes `-`.
pub fn claude_project_slug(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// `${CLAUDE_CONFIG_DIR:-<home>/.claude}`.
pub fn claude_config_root(claude_config_dir: Option<&Path>, home: &Path) -> PathBuf {
    let root = claude_config_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".claude"));
    crate::herdr::isolation::tripwire(&root, "claude_config_root");
    root
}

/// Primary Claude transcript candidate: `<root>/projects/<slug(cwd)>/<id>.jsonl`.
pub fn claude_transcript_path(root: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    root.join("projects")
        .join(claude_project_slug(cwd))
        .join(format!("{session_id}.jsonl"))
}

/// Fallback glob pattern: `<root>/projects/*/<id>.jsonl`.
pub fn claude_transcript_glob(root: &Path, session_id: &str) -> String {
    format!("{}/projects/*/{session_id}.jsonl", root.display())
}

/// Session id from argv for `ProcessInfoArgv { marker }`: the token right after `marker`.
pub fn session_id_from_argv(argv: &[String], marker: &str) -> Option<String> {
    argv.windows(2)
        .find(|w| w[0] == marker)
        .map(|w| w[1].clone())
}

/// Outcome of `agent.start` (spec §4.1 "Readiness and start outcomes"). Never blindly retried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum StartOutcome {
    /// Agent running; occupant recorded.
    Started,
    /// `agent_not_ready` (trust/auth dialog): blocked agent adopted as occupant; Notify seat channel.
    BlockedNeedsHuman,
    /// Precondition failed (foreground not the shell, agent present).
    NeedsRevision { reason: String },
    /// Timeout: inspect a snapshot before any retry.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn agent_kinds_map_to_harnesses() {
        assert_eq!(for_agent_kind("claude"), Some(Harness::Claude));
        assert_eq!(for_agent_kind("codex"), Some(Harness::Codex));
        assert_eq!(for_agent_kind("x"), None);
        assert_eq!(for_agent_kind(""), None);
    }

    #[test]
    fn observed_run_inverts_argv() {
        for h in [Harness::Claude, Harness::Codex] {
            let p = profile(h);
            let mut argv = vec![p.agent_kind.unwrap().to_owned()];
            argv.extend(p.argv(Some("m"), Some("id"), &v(&["--x"])));
            assert_eq!(
                p.observed_run(&argv),
                Some((Some("m".to_owned()), v(&["--x"]))),
                "{h:?}"
            );
            let mut bare = vec!["/usr/bin/".to_owned() + p.agent_kind.unwrap()];
            bare.extend(p.argv(None, None, &[]));
            assert_eq!(p.observed_run(&bare), Some((None, vec![])), "{h:?}");
        }
    }

    #[test]
    fn observed_run_reads_claude_forms() {
        let p = profile(Harness::Claude);
        let got = p.observed_run(&v(&[
            "node",
            "/usr/lib/claude",
            "--model=opus",
            "--resume=abc",
            "--dangerously-skip-permissions",
            "-c",
        ]));
        assert_eq!(
            got,
            Some((Some("opus".into()), v(&["--dangerously-skip-permissions"])))
        );
        let got = p.observed_run(&v(&["claude", "-r", "abc", "--model", "m", "--verbose"]));
        assert_eq!(got, Some((Some("m".into()), v(&["--verbose"]))));
        assert_eq!(
            p.observed_run(&v(&["claude", "--continue"])),
            Some((None, vec![]))
        );
    }

    #[test]
    fn observed_run_needs_an_agent_token() {
        let p = profile(Harness::Claude);
        assert_eq!(
            p.observed_run(&v(&["node", "/usr/lib/claude/cli.js"])),
            None
        );
        assert_eq!(p.observed_run(&[]), None);
        assert_eq!(profile(Harness::Shell).observed_run(&v(&["bash"])), None);
    }

    #[test]
    fn observed_run_drops_codex_launch_args_and_resume() {
        let p = profile(Harness::Codex);
        let got = p.observed_run(&v(&[
            "codex",
            "--no-daemon",
            "resume",
            "sid",
            "-m",
            "gpt",
            "--yolo",
        ]));
        assert_eq!(got, Some((Some("gpt".into()), v(&["--yolo"]))));
    }
}
