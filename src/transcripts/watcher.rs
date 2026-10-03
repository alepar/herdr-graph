//! Transcript watcher (spec §8.2), a daemon loop every 30 s. For each active clone with an occupant whose
//! session has a transcript path it stats the file (identity = device + inode, plus path; and its size):
//!
//! * the file at a path that already has a transcript record grew past that record's covered end while a new
//!   session on it is running (resume append) and has settled (the same size two passes in a row): request
//!   the new bytes only;
//! * the identity at a path changed (the file was replaced): a fresh `tr_` with empty coverage.
use super::mutations::covered_end;
use super::requests::stat_file;
use super::{Transcripts, internal};
use crate::daemon::registry::CommandError;
use crate::model::common::CloneLifecycle;
use crate::model::effective::resolve_in;
use crate::store::layout;
use crate::threads::effects::Graph;

impl Transcripts {
    /// Record one observation of a transcript's aligned size; true when the previous observation saw the same
    /// size (the file has settled: a transcript still being written has no boundary yet).
    pub(super) fn observe_size(&self, path: &std::path::Path, aligned: u64) -> bool {
        let key = format!("transcripts:size:{}", path.display());
        let previous = self
            .journal
            .meta_get(&key)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok());
        let _ = self.journal.meta_set(&key, &aligned.to_string());
        previous == Some(aligned)
    }

    /// One watcher pass.
    pub async fn watch_once(&self) -> Result<(), CommandError> {
        struct Candidate {
            clone: crate::model::CloneId,
            ns: crate::model::NsId,
            path: std::path::PathBuf,
            tr: Option<crate::model::transcript::TranscriptRecord>,
            covered: u64,
        }
        let candidates: Vec<Candidate> = {
            let view = self.view().map_err(internal)?;
            let g = Graph::load(&view).map_err(internal)?;
            let transcripts: Vec<_> = layout::list_transcripts(&view)
                .map_err(internal)?
                .into_iter()
                .map(|(_, t)| t)
                .collect();
            let requests: Vec<_> = layout::list_requests(&view)
                .map_err(internal)?
                .into_iter()
                .map(|(_, r)| r)
                .collect();
            let mut out = Vec::new();
            for clone in g
                .clones
                .values()
                .filter(|c| c.lifecycle == CloneLifecycle::Active && g.clone_active(c))
            {
                let Some(ns) = clone
                    .occupant
                    .as_ref()
                    .and_then(|o| clone.sessions.iter().find(|s| s.id == o.native_session))
                else {
                    continue;
                };
                let Some(path) = ns.transcript_path.clone() else {
                    continue;
                };
                let Some(seat) = g.seats.get(&clone.seat) else {
                    continue;
                };
                if !resolve_in(&view, seat).map_err(internal)?.summaries {
                    continue;
                }
                let tr = transcripts
                    .iter()
                    .filter(|t| t.transcript_path == path && t.seat == clone.seat)
                    .max_by(|a, b| a.id.cmp(&b.id))
                    .cloned();
                let covered = tr
                    .as_ref()
                    .map(|t| {
                        let own: Vec<_> = requests
                            .iter()
                            .filter(|r| r.transcript == t.id)
                            .cloned()
                            .collect();
                        covered_end(t, &own)
                    })
                    .unwrap_or(0);
                out.push(Candidate {
                    clone: clone.id.clone(),
                    ns: ns.id.clone(),
                    path,
                    tr,
                    covered,
                });
            }
            out
        };
        for c in candidates {
            let Ok(state) = stat_file(&c.path) else {
                continue;
            };
            let replaced = self.note_identity(&c.path, &state);
            let settled = self.observe_size(&c.path, state.aligned);
            if replaced {
                self.request_for(&c.clone, &c.ns, Some(c.path.clone()), None, true)
                    .await?;
                continue;
            }
            let Some(tr) = &c.tr else { continue };
            // Settled: a transcript that is still being written has no boundary yet.
            if state.aligned > c.covered && settled {
                self.request_for(&c.clone, &c.ns, Some(c.path.clone()), Some(&tr.id), false)
                    .await?;
            }
        }
        Ok(())
    }
}
