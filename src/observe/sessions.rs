//! Native session capture per harness profile (spec §4.1 table, §8.1).
use crate::model::harness::{
    Harness, SessionIdSource, TranscriptLocator, claude_transcript_path, profile,
    session_id_from_argv,
};
use crate::ports::herdr::{AgentSession, PaneInfo, ProcessInfo};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionCapture {
    pub harness: Harness,
    pub native_session_id: String,
    pub transcript_path: Option<PathBuf>,
    pub cwd: PathBuf,
}

/// Profile-driven: walks the harness's `session_id_sources` in order. `GraphHookReport` is delivered by
/// `session-report` (hg-zmi.13), not discovered here. Claude: a path-kind `agent_session` is the transcript
/// path (id = file stem); an id alone resolves to `<root>/projects/<slug(cwd)>/<id>.jsonl` when that file
/// exists, else the first `<root>/projects/*/<id>.jsonl`. Codex transcripts stay unresolved (`None`).
pub fn capture_session(
    harness: Harness,
    pane: &PaneInfo,
    process: Option<&ProcessInfo>,
    claude_root: &Path,
    recorded_cwd: &Path,
) -> Option<SessionCapture> {
    let prof = profile(harness);
    let mut id: Option<String> = None;
    let mut from_path: Option<PathBuf> = None;
    for source in prof.session_id_sources {
        match source {
            SessionIdSource::GraphHookReport => {}
            SessionIdSource::HerdrAgentSession => {
                match pane.agent.as_ref().and_then(|a| a.session.as_ref()) {
                    Some(AgentSession::Id(s)) if !s.is_empty() => id = Some(s.clone()),
                    Some(AgentSession::Path(p)) => {
                        if let Some(stem) = p
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .filter(|s| !s.is_empty())
                        {
                            id = Some(stem.to_owned());
                            from_path = Some(p.clone());
                        }
                    }
                    _ => {}
                }
            }
            SessionIdSource::ProcessInfoArgv { marker } => {
                id = process
                    .and_then(|p| session_id_from_argv(&p.foreground_argv, marker))
                    .filter(|s| !s.is_empty());
            }
        }
        if id.is_some() {
            break;
        }
    }
    let native_session_id = id?;
    let transcript_path = match prof.transcript {
        TranscriptLocator::None | TranscriptLocator::HookReportedOrUnresolved => None,
        TranscriptLocator::ClaudeProjects => from_path.or_else(|| {
            let primary = claude_transcript_path(claude_root, recorded_cwd, &native_session_id);
            if primary.is_file() {
                Some(primary)
            } else {
                glob_transcript(claude_root, &native_session_id)
            }
        }),
    };
    Some(SessionCapture {
        harness,
        native_session_id,
        transcript_path,
        cwd: recorded_cwd.to_path_buf(),
    })
}

/// First match of `<root>/projects/*/<id>.jsonl` in directory-name order.
fn glob_transcript(root: &Path, id: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.join("projects"))
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.into_iter()
        .map(|d| d.join(format!("{id}.jsonl")))
        .find(|p| p.is_file())
}
