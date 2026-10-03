//! Request routing and views (spec §8.2): the deterministic summarizer destination, the file facts a request
//! is created from, and the `request list` rows.
use super::coverage::aligned_len;
use super::mutations::is_merged;
use crate::model::OpId;
use crate::model::common::Role;
use crate::model::effective::resolve_in;
use crate::model::request::{ProcessingRequest, RequestStatus};
use crate::model::{ByteRange, CloneId, HerdrPaneId, SeatId};
use crate::ports::store::StoreError;
use crate::ports::threads::ThreadRef;
use crate::store::layout;
use crate::store::tree::TreeRead;
use crate::threads::effects::Graph;
use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// Why a request cannot be delivered right now.
pub const NO_DESTINATION: &str = "no_destination";
pub const SUMMARIZER_RETIRED: &str = "summarizer_retired";
pub const SUMMARIZER_DORMANT: &str = "summarizer_dormant";
pub const SUMMARIZER_NO_CLONES: &str = "summarizer_no_clones";

/// Where a request goes, and what stands in the way.
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    /// An active summarizer clone has an occupant: deliver on the seat channel.
    Ready {
        seat: SeatId,
        seat_name: String,
        thread: Option<ThreadRef>,
        panes: Vec<HerdrPaneId>,
    },
    /// Active summarizer with an active clone but nobody in it: the reconciler relaunches the occupant
    /// (authority: the op that activated the seat), then delivery proceeds.
    Relaunch {
        seat: SeatId,
        seat_name: String,
        clone: CloneId,
        authority: OpId,
    },
    /// Stays pending, flagged `undeliverable` (retired, dormant, no clones, or no destination at all).
    Undeliverable {
        seat: Option<SeatId>,
        reason: &'static str,
    },
}

impl Destination {
    pub fn label(&self) -> String {
        match self {
            Destination::Ready { seat_name, .. } | Destination::Relaunch { seat_name, .. } => {
                seat_name.clone()
            }
            Destination::Undeliverable { .. } => "-".into(),
        }
    }
}

/// The teamspace's oldest non-retired seat with effective `role = summarizer` (ties by id), else
/// `graph.toml summarizer_seat`. A retired summarizer is never activated (retirement precedence).
pub(crate) fn resolve_destination(
    g: &Graph,
    tree: &dyn TreeRead,
    source_seat: &SeatId,
) -> Result<Destination, StoreError> {
    let Some(source) = g.seats.get(source_seat) else {
        return Ok(Destination::Undeliverable {
            seat: None,
            reason: NO_DESTINATION,
        });
    };
    let mut candidates = Vec::new();
    for seat in g.seats.values().filter(|s| s.teamspace == source.teamspace) {
        if seat.lifecycle == crate::model::common::Lifecycle::Retired {
            continue;
        }
        if resolve_in(tree, seat)?.role == Some(Role::Summarizer) {
            candidates.push(seat);
        }
    }
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    let dest = match candidates.first() {
        Some(seat) => *seat,
        None => match layout::read_graph(tree)?
            .summarizer_seat
            .and_then(|id| g.seats.get(&id))
        {
            Some(seat) => seat,
            None => {
                return Ok(Destination::Undeliverable {
                    seat: None,
                    reason: NO_DESTINATION,
                });
            }
        },
    };
    use crate::model::common::Lifecycle;
    if dest.lifecycle == Lifecycle::Retired {
        return Ok(Destination::Undeliverable {
            seat: Some(dest.id.clone()),
            reason: SUMMARIZER_RETIRED,
        });
    }
    if !g.seat_active(dest) {
        return Ok(Destination::Undeliverable {
            seat: Some(dest.id.clone()),
            reason: SUMMARIZER_DORMANT,
        });
    }
    let mut clones: Vec<_> = g
        .clones
        .values()
        .filter(|c| c.seat == dest.id && g.clone_active(c))
        .collect();
    clones.sort_by(|a, b| a.id.cmp(&b.id));
    if clones.is_empty() {
        return Ok(Destination::Undeliverable {
            seat: Some(dest.id.clone()),
            reason: SUMMARIZER_NO_CLONES,
        });
    }
    let occupied: Vec<_> = clones.iter().filter(|c| c.occupant.is_some()).collect();
    if occupied.is_empty() {
        let authority = dest
            .activation
            .last_op
            .clone()
            .unwrap_or_else(|| OpId::from_ulid(ulid::Ulid::nil()));
        return Ok(Destination::Relaunch {
            seat: dest.id.clone(),
            seat_name: dest.name.clone(),
            clone: clones[0].id.clone(),
            authority,
        });
    }
    Ok(Destination::Ready {
        seat: dest.id.clone(),
        seat_name: dest.name.clone(),
        thread: dest.channel.thread_id.clone().map(ThreadRef),
        panes: occupied
            .iter()
            .filter_map(|c| c.runtime.bound.as_ref().and_then(|b| b.pane_id.clone()))
            .collect(),
    })
}

/// A transcript file as the watcher and request creation see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileState {
    pub dev: u64,
    pub ino: u64,
    /// Newline-aligned length.
    pub aligned: u64,
}

impl FileState {
    pub fn identity(&self) -> String {
        format!("{}:{}", self.dev, self.ino)
    }
}

/// Stat and newline-align `path`; an error means missing or unreadable input.
pub fn stat_file(path: &Path) -> std::io::Result<FileState> {
    let mut file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(std::io::Error::other("not a regular file"));
    }
    let aligned = aligned_len(&mut file, meta.len())?;
    Ok(FileState {
        dev: meta.dev(),
        ino: meta.ino(),
        aligned,
    })
}

/// One row of `request list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestRow {
    pub id: String,
    pub status: RequestStatus,
    pub transcript: String,
    pub range: ByteRange,
    pub destination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undeliverable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<String>,
}

impl RequestRow {
    /// `rq  status  tr  range  destination  undeliverable?`
    pub fn render(&self) -> String {
        let status = serde_json::to_value(self.status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        let mut line = format!(
            "{}  {}  {}  {}-{}  {}",
            self.id, status, self.transcript, self.range.start, self.range.end, self.destination
        );
        if let Some(u) = &self.undeliverable {
            line.push_str(&format!("  undeliverable: {u}"));
        }
        if let Some(u) = &self.unresolved {
            line.push_str(&format!("  unresolved: {u}"));
        }
        line
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListFilter {
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub unresolved: bool,
    #[serde(default)]
    pub undispatched: bool,
}

impl ListFilter {
    fn keeps(&self, r: &ProcessingRequest) -> bool {
        if is_merged(r) {
            return false;
        }
        if !(self.pending || self.unresolved || self.undispatched) {
            return true;
        }
        let open = matches!(
            r.status,
            RequestStatus::Pending | RequestStatus::Delivered | RequestStatus::Dispatched
        );
        let undispatched = matches!(r.status, RequestStatus::Pending | RequestStatus::Delivered);
        let live = (self.pending || self.undispatched)
            && (!self.pending || open)
            && (!self.undispatched || undispatched);
        live || (self.unresolved && r.status == RequestStatus::Unresolved)
    }
}

/// `request list` rows of the committed tree (also the daemon-less read path of the CLI).
pub fn list_view(tree: &dyn TreeRead, filter: ListFilter) -> Result<Vec<RequestRow>, StoreError> {
    let g = Graph::load(tree)?;
    let transcripts: std::collections::BTreeMap<_, _> = layout::list_transcripts(tree)?
        .into_iter()
        .map(|(_, t)| (t.id.clone(), t))
        .collect();
    let mut rows = Vec::new();
    for (_, r) in layout::list_requests(tree)? {
        if !filter.keeps(&r) {
            continue;
        }
        let destination = match transcripts.get(&r.transcript) {
            Some(t) => resolve_destination(&g, tree, &t.seat)?.label(),
            None => "-".into(),
        };
        rows.push(RequestRow {
            id: r.id.to_string(),
            status: r.status,
            transcript: r.transcript.to_string(),
            range: r.range,
            destination,
            undeliverable: r.undeliverable.clone(),
            unresolved: r.unresolved.clone(),
        });
    }
    Ok(rows)
}
