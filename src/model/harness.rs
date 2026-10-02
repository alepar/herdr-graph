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
    session_id_sources: &[SessionIdSource::GraphHookReport, SessionIdSource::HerdrAgentSession],
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

/// Claude project slug: every char not `[A-Za-z0-9]` becomes `-`.
pub fn claude_project_slug(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// `${CLAUDE_CONFIG_DIR:-<home>/.claude}`.
pub fn claude_config_root(claude_config_dir: Option<&Path>, home: &Path) -> PathBuf {
    claude_config_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".claude"))
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
    argv.windows(2).find(|w| w[0] == marker).map(|w| w[1].clone())
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
