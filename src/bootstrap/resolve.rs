//! Caller resolution for `/seat` (spec §9): which clone, seat and teamspace is the calling pane?
//!
//! `resolve_caller` is pure over a committed tree. Proposals (create / resurrect / rebind plans) are
//! computed as plan WORDS by `proposal_words`; the daemon turns the words into a stored plan through
//! `plan.create` and attaches it as a `Proposal`. `/seat` never applies a proposal.
use crate::daemon::registry::CallerInfo;
use crate::model::clone::CloneRecord;
use crate::model::common::{Availability, CloneLifecycle, Lifecycle};
use crate::model::launch::parse_nonce_label;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{CloneId, NsId, PlanId, SeatId, TeamspaceId};
use crate::ports::herdr::{AgentSession, PaneInfo};
use crate::ports::store::StoreError;
use crate::store::layout;
use crate::store::tree::TreeRead;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub clone: CloneId,
    pub seat: SeatId,
    pub reason: String,
}

/// A stored, unapplied plan that would fix the resolution.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proposal {
    pub plan_id: PlanId,
    pub hash: String,
    pub words: Vec<String>,
    pub rendered: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Resolution {
    Bound {
        teamspace: TeamspaceId,
        seat: SeatId,
        clone: CloneId,
        native_session: Option<NsId>,
        /// "binding" | "env" | "agent_session"
        via: String,
        /// A rebind plan when the pane binding is stale (via env / agent_session).
        #[serde(skip_serializing_if = "Option::is_none")]
        proposal: Option<Proposal>,
    },
    Unbound {
        candidates: Vec<Candidate>,
        proposal: Option<Proposal>,
    },
    Ambiguous {
        candidates: Vec<Candidate>,
        proposal: Option<Proposal>,
    },
}

impl Resolution {
    pub fn set_proposal(&mut self, p: Option<Proposal>) {
        match self {
            Resolution::Bound { proposal, .. }
            | Resolution::Unbound { proposal, .. }
            | Resolution::Ambiguous { proposal, .. } => *proposal = p,
        }
    }

    pub fn proposal(&self) -> Option<&Proposal> {
        match self {
            Resolution::Bound { proposal, .. }
            | Resolution::Unbound { proposal, .. }
            | Resolution::Ambiguous { proposal, .. } => proposal.as_ref(),
        }
    }
}

/// Everything known about one clone: its record and the seat and teamspace above it.
struct Fact {
    clone: CloneRecord,
    seat: SeatRecord,
    ts: TeamspaceRecord,
}

impl Fact {
    fn live(&self) -> bool {
        self.clone.lifecycle == CloneLifecycle::Active
            && self.seat.lifecycle != Lifecycle::Retired
            && self.ts.lifecycle != Lifecycle::Retired
    }
    fn bound_pane(&self) -> Option<&str> {
        self.clone.runtime.bound.as_ref().and_then(|b| b.pane_id.as_ref()).map(|p| p.0.as_str())
    }
    fn candidate(&self, reason: impl Into<String>) -> Candidate {
        Candidate { clone: self.clone.id.clone(), seat: self.seat.id.clone(), reason: reason.into() }
    }
}

fn facts(tree: &dyn TreeRead) -> Result<Vec<Fact>, StoreError> {
    let mut out = Vec::new();
    for (ts_loc, ts) in layout::list_teamspaces(tree)? {
        for (seat_loc, seat) in layout::list_seats(tree, &ts_loc.folder)? {
            for (_, clone) in layout::list_clones(tree, &seat_loc.folder)? {
                out.push(Fact { clone, seat: seat.clone(), ts: ts.clone() });
            }
        }
    }
    Ok(out)
}

fn session_id(pane: Option<&PaneInfo>) -> Option<&str> {
    match pane?.agent.as_ref()?.session.as_ref()? {
        AgentSession::Id(s) => Some(s.as_str()),
        AgentSession::Path(_) => None,
    }
}

/// The ns id of the clone's session with this native id (current occupant first).
fn ns_for_native(f: &Fact, native: &str) -> Option<NsId> {
    if let Some(occ) = &f.clone.occupant
        && f.clone.sessions.iter().any(|s| s.id == occ.native_session && s.native_session_id == native)
    {
        return Some(occ.native_session.clone());
    }
    None
}

fn bound(f: &Fact, via: &str) -> Resolution {
    Resolution::Bound {
        teamspace: f.ts.id.clone(),
        seat: f.seat.id.clone(),
        clone: f.clone.id.clone(),
        native_session: f.clone.occupant.as_ref().map(|o| o.native_session.clone()),
        via: via.to_owned(),
        proposal: None,
    }
}

/// Resolve the caller: pane binding, then `HERDR_GRAPH_CLONE`, then the pane's agent session id; else
/// unbound with candidates (clones in the caller's seat or teamspace whose binding is unknown or flagged
/// `reload_required`, i.e. moved panes). An unreadable tree resolves to unbound with no candidates.
pub fn resolve_caller(tree: &dyn TreeRead, caller: &CallerInfo, snapshot_pane: Option<&PaneInfo>) -> Resolution {
    try_resolve(tree, caller, snapshot_pane).unwrap_or(Resolution::Unbound { candidates: vec![], proposal: None })
}

fn try_resolve(tree: &dyn TreeRead, caller: &CallerInfo, snapshot_pane: Option<&PaneInfo>) -> Result<Resolution, StoreError> {
    let all = facts(tree)?;

    if let Some(pane) = caller.pane_id.as_deref() {
        let here: Vec<&Fact> = all.iter().filter(|f| f.live() && f.bound_pane() == Some(pane)).collect();
        match here.as_slice() {
            [] => {}
            [one] => return Ok(bound(one, "binding")),
            many => {
                return Ok(Resolution::Ambiguous {
                    candidates: many.iter().map(|f| f.candidate(format!("bound to pane {pane}"))).collect(),
                    proposal: None,
                });
            }
        }
    }

    let env_clone = caller.graph_clone.as_deref().and_then(|s| s.parse::<CloneId>().ok());
    if let Some(id) = &env_clone
        && let Some(f) = all.iter().find(|f| &f.clone.id == id && f.live())
    {
        return Ok(bound(f, "env"));
    }

    if let Some(sid) = session_id(snapshot_pane) {
        let by_session: Vec<(&Fact, NsId)> =
            all.iter().filter(|f| f.live()).filter_map(|f| ns_for_native(f, sid).map(|ns| (f, ns))).collect();
        match by_session.as_slice() {
            [] => {}
            [(f, ns)] => {
                let mut r = bound(f, "agent_session");
                if let Resolution::Bound { native_session, .. } = &mut r {
                    *native_session = Some(ns.clone());
                }
                return Ok(r);
            }
            many => {
                return Ok(Resolution::Ambiguous {
                    candidates: many.iter().map(|(f, _)| f.candidate(format!("session {sid}"))).collect(),
                    proposal: None,
                });
            }
        }
    }

    // Unbound: candidates are the moved or unconfirmed clones of the seat (or teamspace) the env names.
    let mut seats: Vec<SeatId> = Vec::new();
    if let Some(s) = caller.graph_seat.as_deref().and_then(|s| s.parse::<SeatId>().ok()) {
        seats.push(s);
    }
    if let Some(id) = &env_clone
        && let Some(f) = all.iter().find(|f| &f.clone.id == id)
        && !seats.contains(&f.seat.id)
    {
        seats.push(f.seat.id.clone());
    }
    let in_scope = |f: &Fact| -> bool {
        if seats.is_empty() {
            teamspace_by_cwd(&all, caller).is_some_and(|t| t == f.ts.id)
        } else {
            seats.contains(&f.seat.id)
        }
    };
    let candidates = all
        .iter()
        .filter(|f| f.live() && in_scope(f))
        .filter_map(|f| {
            let unknown = f.clone.runtime.availability == Availability::Unknown;
            match (f.clone.reload_required, unknown) {
                (true, _) => Some(f.candidate("reload_required: its pane moved or restarted")),
                (false, true) => Some(f.candidate("availability unknown: not confirmed in any pane")),
                _ => None,
            }
        })
        .collect();
    Ok(Resolution::Unbound { candidates, proposal: None })
}

/// The unique teamspace whose `project_repo` contains the caller's cwd.
fn teamspace_by_cwd(all: &[Fact], caller: &CallerInfo) -> Option<TeamspaceId> {
    let cwd = caller.cwd.as_ref()?;
    let mut hits: Vec<TeamspaceId> = Vec::new();
    for f in all {
        if let Some(repo) = &f.ts.project_repo
            && f.ts.lifecycle != Lifecycle::Retired
            && cwd.starts_with(repo)
            && !hits.contains(&f.ts.id)
        {
            hits.push(f.ts.id.clone());
        }
    }
    (hits.len() == 1).then(|| hits.remove(0))
}

fn pane_name(caller: &CallerInfo, pane: Option<&PaneInfo>) -> String {
    let label = pane.and_then(|p| p.label.as_deref()).map(|l| parse_nonce_label(l).map_or(l, |(n, _)| n));
    let from_cwd = caller.cwd.as_ref().and_then(|c| c.file_name()).and_then(|n| n.to_str());
    let name = label.or(from_cwd).map(str::trim).filter(|n| !n.is_empty()).unwrap_or("seat");
    if name.starts_with('-') { format!("seat{name}") } else { name.to_owned() }
}

/// The plan words that would fix `res`, for `plan.create`:
/// stale binding (env / agent session) or one moved candidate → `clone rebind <cl> --pane <pane>`;
/// a retired seat the env names → `seat resurrect <st>`; a retired clone of a live seat → `clone add <st>`;
/// nothing identifiable → `seat create <name> --teamspace <ts>` when a teamspace is identifiable.
pub fn proposal_words(
    tree: &dyn TreeRead,
    caller: &CallerInfo,
    snapshot_pane: Option<&PaneInfo>,
    res: &Resolution,
) -> Option<Vec<String>> {
    let all = facts(tree).ok()?;
    let w = |parts: &[&str]| parts.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let rebind = |clone: &CloneId, pane: &str| w(&["clone", "rebind", clone.as_str(), "--pane", pane]);
    match res {
        Resolution::Ambiguous { .. } => None,
        Resolution::Bound { via, clone, .. } if via != "binding" => {
            let pane = caller.pane_id.as_deref()?;
            let f = all.iter().find(|f| &f.clone.id == clone)?;
            (f.bound_pane() != Some(pane)).then(|| rebind(clone, pane))
        }
        Resolution::Bound { .. } => None,
        Resolution::Unbound { candidates, .. } => {
            if let [one] = candidates.as_slice() {
                return caller.pane_id.as_deref().map(|pane| rebind(&one.clone, pane));
            }
            if !candidates.is_empty() {
                return None; // several moved clones: the user chooses
            }
            let env_seat = caller.graph_seat.as_deref().and_then(|s| s.parse::<SeatId>().ok());
            let env_clone = caller.graph_clone.as_deref().and_then(|s| s.parse::<CloneId>().ok());
            let seat_of = |id: &SeatId| all.iter().find(|f| &f.seat.id == id).map(|f| (f.seat.clone(), f.ts.clone()));
            let named_seat = env_seat.clone().or_else(|| {
                env_clone.as_ref().and_then(|c| all.iter().find(|f| &f.clone.id == c)).map(|f| f.seat.id.clone())
            });
            if let Some((seat, _)) = named_seat.as_ref().and_then(seat_of) {
                if seat.lifecycle == Lifecycle::Retired {
                    return Some(w(&["seat", "resurrect", seat.id.as_str()]));
                }
                let clone_retired = env_clone
                    .as_ref()
                    .and_then(|c| all.iter().find(|f| &f.clone.id == c))
                    .is_some_and(|f| f.clone.lifecycle == CloneLifecycle::Retired);
                if clone_retired {
                    return Some(w(&["clone", "add", seat.id.as_str()]));
                }
            }
            let ts = named_seat
                .as_ref()
                .and_then(seat_of)
                .map(|(_, ts)| ts.id)
                .or_else(|| teamspace_by_cwd(&all, caller))?;
            let name = pane_name(caller, snapshot_pane);
            Some(vec!["seat".into(), "create".into(), name, "--teamspace".into(), ts.to_string()])
        }
    }
}
