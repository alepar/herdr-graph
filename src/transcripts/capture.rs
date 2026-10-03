//! `session-report` (spec §8.1): session identity from graph's own Claude SessionStart hook, first in the
//! profile order. The CLI turns the hook payload into an IPC `session.report`; the daemon resolves the clone
//! and admits `observed.occupancy`, which ends the previous native session on a changed id (`/clear`) and
//! triggers request creation for the session that just ended.
use super::{Transcripts, clone_for_pane, internal};
use crate::daemon::registry::{CallerInfo, CommandError};
use crate::ipc::IpcErrorCode;
use crate::model::change::ChangeRequest;
use crate::model::common::CloneLifecycle;
use crate::model::harness::Harness;
use crate::model::native_session::NativeSession;
use crate::model::native_session::SessionEndReason;
use crate::model::operation::OpState;
use crate::model::{CloneId, OpId, SeatId, Timestamp};
use crate::observe::mutations::occupancy_request;
use crate::observe::{SessionCapture, SessionEnded};
use crate::threads::effects::Graph;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How often the daemon looks for reports the CLI spooled.
pub const SPOOL_INTERVAL: Duration = Duration::from_secs(5);

/// `<instance>/.graph-local/session-spool/`: reports the CLI could not hand to the daemon within its budget.
pub fn spool_dir(instance_root: &Path) -> PathBuf {
    instance_root.join(".graph-local").join("session-spool")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpooledReport {
    /// Format version, 1.
    pub version: u32,
    pub caller: CallerInfo,
    /// Exactly what `report_args` built.
    pub args: serde_json::Value,
    pub spooled_at: Timestamp,
}

/// Write one report as `<ulid>.json` (ULID order = spool order): a temp file in the same directory,
/// `sync_all`, then a rename, so the daemon never sees a half-written report.
pub fn spool_report(instance_root: &Path, r: &SpooledReport) -> std::io::Result<PathBuf> {
    let dir = spool_dir(instance_root);
    std::fs::create_dir_all(&dir)?;
    let name = ulid::Ulid::new().to_string();
    let tmp = dir.join(format!("{name}.tmp"));
    let dest = dir.join(format!("{name}.json"));
    let bytes = serde_json::to_vec(r).map_err(std::io::Error::other)?;
    let written = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, &dest)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written?;
    Ok(dest)
}

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
}

/// A resolved report that needs an `observed.occupancy` op.
struct Pending {
    clone: CloneId,
    seat: SeatId,
    /// The session that was the occupant when the report was resolved.
    previous: Option<NativeSession>,
    native: String,
    source: Option<String>,
}

/// A spooled report whose op is admitted and not yet finished.
struct InFlight {
    op: OpId,
    pending: Pending,
}

/// Spool ingestion state, behind `Transcripts::spool_lock`: the op each spooled file already has in flight, so
/// a slow commit or a halted writer never gets a second copy queued behind the first.
#[derive(Default)]
pub(crate) struct SpoolState {
    inflight: BTreeMap<PathBuf, InFlight>,
}

enum Resolved {
    /// Answered without a write.
    Done(serde_json::Value),
    Admit(Box<Pending>, Box<ChangeRequest>),
}

impl Transcripts {
    /// Handle `session.report`: resolve the clone (explicit id, else the pane's binding), record the session,
    /// and, when it replaced a running one, request the transcript of the session that just ended.
    /// Unresolvable reports answer `{resolved: false}` and change nothing.
    #[cfg(test)]
    pub(crate) async fn session_report(
        &self,
        caller: CallerInfo,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CommandError> {
        let at = self.clock.now();
        self.session_report_at(caller, args, at).await
    }

    /// `session.report` from a live caller: reports spooled before it are older, so they go first.
    pub(crate) async fn session_report_live(
        &self,
        caller: CallerInfo,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CommandError> {
        // Taken before the spool is ingested, so every spooled report really is older than this one.
        let at = self.clock.now();
        self.ingest_spool().await;
        self.session_report_at(caller, args, at).await
    }

    async fn session_report_at(
        &self,
        caller: CallerInfo,
        args: serde_json::Value,
        at: Timestamp,
    ) -> Result<serde_json::Value, CommandError> {
        match self.resolve_report(&caller, args, false, at)? {
            Resolved::Done(v) => Ok(v),
            Resolved::Admit(p, request) => {
                let op = self.commit(*request).await?;
                self.finish_report(&p, op).await
            }
        }
    }

    /// Hand every spooled report to the occupancy mutation, oldest first, stamped with the time the hook fired.
    /// A report whose op is admitted but not committed is never admitted again; the pass stops at it, to keep
    /// order. While the writer is halted nothing is admitted. Ok: delete the file. A report whose native
    /// session id is already in that clone's `sessions` was processed before (the CLI timed out after the daemon
    /// handled it): delete without reprocessing. A `bad_request`/`rejected` error (or an unreadable file) moves
    /// it to `session-spool/rejected/` and logs. Any other error keeps the file and stops this pass.
    /// Returns the number of reports applied.
    pub async fn ingest_spool(&self) -> usize {
        let mut state = self.spool_lock.lock().await;
        if matches!(
            self.journal.meta_get(crate::writer::WRITER_HALTED),
            Ok(Some(_))
        ) {
            return 0;
        }
        let dir = spool_dir(self.reconciler.instance());
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return 0;
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        state.inflight.retain(|p, _| files.contains(p));
        let mut applied = 0;
        for path in files {
            if let Some(op) = state.inflight.get(&path).map(|e| e.op.clone()) {
                match self.writer.status(&op) {
                    Ok(Some(OpState::Committed)) => {
                        let Some(entry) = state.inflight.remove(&path) else {
                            continue;
                        };
                        if self.finish_spooled(&entry.pending, op).await {
                            applied += 1;
                        }
                        if let Err(e) = std::fs::remove_file(&path) {
                            eprintln!(
                                "herdr-graph: transcripts: cannot remove spooled report {}: {e}",
                                path.display()
                            );
                            return applied;
                        }
                        continue;
                    }
                    Ok(Some(OpState::Admitted | OpState::Applying)) => return applied,
                    Ok(_) => {
                        // Rejected, failed, cancelled or unknown: the op is gone, so process the file afresh.
                        state.inflight.remove(&path);
                    }
                    Err(e) => {
                        eprintln!(
                            "herdr-graph: transcripts: spooled report {} kept: {e}",
                            path.display()
                        );
                        return applied;
                    }
                }
            }
            let parsed = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    serde_json::from_slice::<SpooledReport>(&b).map_err(|e| e.to_string())
                });
            let report = match parsed {
                Ok(r) => r,
                Err(e) => {
                    quarantine(&dir, &path, &e);
                    continue;
                }
            };
            let outcome =
                match self.resolve_report(&report.caller, report.args, true, report.spooled_at) {
                    Ok(Resolved::Done(v)) => Ok(v["changed"] == true),
                    Ok(Resolved::Admit(p, request)) => match self.writer.admit(*request) {
                        Ok(op) => {
                            state.inflight.insert(
                                path.clone(),
                                InFlight {
                                    op: op.clone(),
                                    pending: *p,
                                },
                            );
                            match self.wait_committed(op).await {
                                Ok(op) => match state.inflight.remove(&path) {
                                    Some(e) => Ok(self.finish_spooled(&e.pending, op).await),
                                    None => Ok(false),
                                },
                                Err(e) => {
                                    if matches!(
                                        e.code,
                                        IpcErrorCode::BadRequest | IpcErrorCode::Rejected
                                    ) {
                                        state.inflight.remove(&path);
                                    }
                                    Err(e)
                                }
                            }
                        }
                        Err(e) => Err(CommandError::unavailable(e.to_string())),
                    },
                    Err(e) => Err(e),
                };
            match outcome {
                Ok(changed) => {
                    if changed {
                        applied += 1;
                    }
                    if let Err(e) = std::fs::remove_file(&path) {
                        eprintln!(
                            "herdr-graph: transcripts: cannot remove spooled report {}: {e}",
                            path.display()
                        );
                        return applied;
                    }
                }
                Err(e) if matches!(e.code, IpcErrorCode::BadRequest | IpcErrorCode::Rejected) => {
                    quarantine(&dir, &path, &e.message);
                }
                Err(e) => {
                    eprintln!(
                        "herdr-graph: transcripts: spooled report {} kept: {}",
                        path.display(),
                        e.message
                    );
                    return applied;
                }
            }
        }
        applied
    }

    pub(crate) async fn run_spool(
        self: std::sync::Arc<Self>,
        mut sd: crate::daemon::registry::Shutdown,
    ) -> anyhow::Result<()> {
        loop {
            self.ingest_spool().await;
            tokio::select! {
                _ = sd.wait() => return Ok(()),
                _ = tokio::time::sleep(SPOOL_INTERVAL) => {}
            }
        }
    }

    /// Resolve the report's clone and decide whether it needs a write. `at` is when the report was made (the
    /// hook's time for a spooled one), not when it is processed, so two copies of one report carry one time.
    fn resolve_report(
        &self,
        caller: &CallerInfo,
        args: serde_json::Value,
        skip_known: bool,
        at: Timestamp,
    ) -> Result<Resolved, CommandError> {
        let a: ReportArgs =
            serde_json::from_value(args).map_err(|e| CommandError::bad_request(e.to_string()))?;
        let (clone, seat, previous, known) = {
            let view = self.view().map_err(internal)?;
            let g = Graph::load(&view).map_err(internal)?;
            let by_id = |raw: &str| {
                CloneId::parse(raw)
                    .ok()
                    .filter(|id| g.clones.contains_key(id))
            };
            let id = a
                .clone
                .as_deref()
                .and_then(by_id)
                .or_else(|| caller.graph_clone.as_deref().and_then(by_id))
                .or_else(|| {
                    a.pane
                        .as_deref()
                        .or(caller.pane_id.as_deref())
                        .and_then(|p| clone_for_pane(&g, p))
                });
            let Some(rec) = id
                .and_then(|id| g.clones.get(&id))
                .filter(|c| c.lifecycle == CloneLifecycle::Active)
            else {
                return Ok(Resolved::Done(json!({ "resolved": false })));
            };
            let previous = rec
                .occupant
                .as_ref()
                .and_then(|o| rec.sessions.iter().find(|s| s.id == o.native_session))
                .cloned();
            let known = rec
                .sessions
                .iter()
                .any(|s| s.native_session_id == a.capture.native_session_id);
            (rec.id.clone(), rec.seat.clone(), previous, known)
        };
        if skip_known && known {
            return Ok(Resolved::Done(
                json!({ "resolved": true, "clone": clone, "changed": false, "duplicate": true, "source": a.source }),
            ));
        }
        if previous
            .as_ref()
            .is_some_and(|p| p.native_session_id == a.capture.native_session_id)
        {
            return Ok(Resolved::Done(
                json!({ "resolved": true, "clone": clone, "changed": false, "source": a.source }),
            ));
        }
        let end = previous.as_ref().map(|_| SessionEndReason::SessionChanged);
        let request = occupancy_request(&clone, end, Some(&a.capture), at);
        let pending = Pending {
            clone,
            seat,
            previous,
            native: a.capture.native_session_id,
            source: a.source,
        };
        Ok(Resolved::Admit(Box::new(pending), Box::new(request)))
    }

    /// The occupancy op committed: when it ended the previous session, request that session's transcript.
    /// The mutation drops a duplicate or stale report, in which case nothing changed and nothing is requested.
    async fn finish_report(
        &self,
        p: &Pending,
        op: OpId,
    ) -> Result<serde_json::Value, CommandError> {
        let (changed, previous_ended) = {
            let view = self.view().map_err(internal)?;
            let g = Graph::load(&view).map_err(internal)?;
            let rec = g.clones.get(&p.clone);
            let occupant = rec.and_then(|r| {
                let o = r.occupant.as_ref()?;
                r.sessions.iter().find(|s| s.id == o.native_session)
            });
            let changed = occupant.is_some_and(|s| s.native_session_id == p.native);
            let previous_ended = p.previous.as_ref().is_some_and(|prev| {
                rec.is_some_and(|r| {
                    r.sessions
                        .iter()
                        .any(|s| s.id == prev.id && s.ended.is_some())
                })
            });
            (changed, previous_ended)
        };
        if let Some(prev) = p.previous.as_ref().filter(|_| previous_ended) {
            self.on_session_ended(SessionEnded {
                clone: p.clone.clone(),
                seat: p.seat.clone(),
                ns: prev.id.clone(),
                harness: prev.harness,
                transcript_path: prev.transcript_path.clone(),
                reason: SessionEndReason::SessionChanged,
                op,
            })
            .await;
        }
        Ok(json!({ "resolved": true, "clone": p.clone, "changed": changed, "source": p.source }))
    }

    /// [`Self::finish_report`] for the spool: whether the report changed anything.
    async fn finish_spooled(&self, p: &Pending, op: OpId) -> bool {
        match self.finish_report(p, op).await {
            Ok(v) => v["changed"] == true,
            Err(e) => {
                eprintln!(
                    "herdr-graph: transcripts: spooled report for {} not finished: {}",
                    p.clone, e.message
                );
                false
            }
        }
    }
}

fn quarantine(dir: &Path, path: &Path, why: &str) {
    eprintln!(
        "herdr-graph: transcripts: spooled report {} rejected: {why}",
        path.display()
    );
    let rejected = dir.join("rejected");
    let moved = std::fs::create_dir_all(&rejected)
        .and_then(|()| std::fs::rename(path, rejected.join(path.file_name().unwrap_or_default())));
    if let Err(e) = moved {
        eprintln!(
            "herdr-graph: transcripts: cannot quarantine {}: {e}",
            path.display()
        );
    }
}
