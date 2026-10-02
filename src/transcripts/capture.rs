//! `session-report` (spec §8.1): session identity from graph's own Claude SessionStart hook, first in the
//! profile order. The CLI turns the hook payload into an IPC `session.report`; the daemon resolves the clone
//! and admits `observed.occupancy`, which ends the previous native session on a changed id (`/clear`) and
//! triggers request creation for the session that just ended.
use super::{Transcripts, clone_for_pane, internal};
use crate::daemon::registry::{CallerInfo, CommandError};
use crate::model::harness::Harness;
use crate::model::native_session::SessionEndReason;
use crate::model::CloneId;
use crate::model::common::CloneLifecycle;
use crate::observe::mutations::occupancy_request;
use crate::observe::{SessionCapture, SessionEnded};
use crate::threads::effects::Graph;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

/// The fields graph needs from a Claude SessionStart hook payload.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClaudeHook {
    pub session_id: String,
    #[serde(default)]
    pub transcript_path: Option<PathBuf>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

pub fn parse_claude_hook(stdin: &str) -> Result<ClaudeHook, serde_json::Error> {
    serde_json::from_str(stdin)
}

/// The IPC arguments of `session.report`, or `None` when neither `HERDR_GRAPH_CLONE` nor `HERDR_PANE_ID`
/// can say which clone this is (the report is then a silent no-op).
pub fn report_args(
    hook: &ClaudeHook,
    graph_clone: Option<&str>,
    pane: Option<&str>,
    fallback_cwd: PathBuf,
) -> Option<serde_json::Value> {
    if graph_clone.is_none() && pane.is_none() {
        return None;
    }
    let capture = SessionCapture {
        harness: Harness::Claude,
        native_session_id: hook.session_id.clone(),
        transcript_path: hook.transcript_path.clone(),
        cwd: hook.cwd.clone().unwrap_or(fallback_cwd),
    };
    Some(json!({ "clone": graph_clone, "pane": pane, "capture": capture, "source": hook.source }))
}

#[derive(Deserialize)]
struct ReportArgs {
    #[serde(default)]
    clone: Option<String>,
    #[serde(default)]
    pane: Option<String>,
    capture: SessionCapture,
    #[serde(default)]
    source: Option<String>,
    #[serde(default, rename = "_caller")]
    caller: Option<CallerInfo>,
}

impl Transcripts {
    /// Handle `session.report`: resolve the clone (explicit id, else the pane's binding), record the session,
    /// and, when it replaced a running one, request the transcript of the session that just ended.
    /// Unresolvable reports answer `{resolved: false}` and change nothing.
    pub(crate) async fn session_report(&self, args: serde_json::Value) -> Result<serde_json::Value, CommandError> {
        let a: ReportArgs = serde_json::from_value(args).map_err(|e| CommandError::bad_request(e.to_string()))?;
        let caller = a.caller.unwrap_or_default();
        let (clone, seat, previous) = {
            let view = self.view().map_err(internal)?;
            let g = Graph::load(&view).map_err(internal)?;
            let by_id = |raw: &str| CloneId::parse(raw).ok().filter(|id| g.clones.contains_key(id));
            let id = a
                .clone
                .as_deref()
                .and_then(by_id)
                .or_else(|| caller.graph_clone.as_deref().and_then(by_id))
                .or_else(|| a.pane.as_deref().or(caller.pane_id.as_deref()).and_then(|p| clone_for_pane(&g, p)));
            let Some(rec) = id.and_then(|id| g.clones.get(&id)).filter(|c| c.lifecycle == CloneLifecycle::Active) else {
                return Ok(json!({ "resolved": false }));
            };
            let previous = rec
                .occupant
                .as_ref()
                .and_then(|o| rec.sessions.iter().find(|s| s.id == o.native_session))
                .cloned();
            (rec.id.clone(), rec.seat.clone(), previous)
        };
        if previous.as_ref().is_some_and(|p| p.native_session_id == a.capture.native_session_id) {
            return Ok(json!({ "resolved": true, "clone": clone, "changed": false, "source": a.source }));
        }
        let now = self.clock.now();
        let end = previous.as_ref().map(|_| SessionEndReason::SessionChanged);
        let op = self.commit(occupancy_request(&clone, end, Some(&a.capture), now)).await?;
        if let Some(prev) = previous {
            self.on_session_ended(SessionEnded {
                clone: clone.clone(),
                seat,
                ns: prev.id,
                harness: prev.harness,
                transcript_path: prev.transcript_path,
                reason: SessionEndReason::SessionChanged,
                op,
            })
            .await;
        }
        Ok(json!({ "resolved": true, "clone": clone, "changed": true, "source": a.source }))
    }
}
