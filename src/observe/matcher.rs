//! Identity matching between a complete snapshot and committed graph objects (spec §4.2).
//!
//! Panes. A seatless clone (no `threads_seat`): its graph token, else the pane whose id its committed binding
//! names, in any incarnation (Herdr preserves pane ids, as for tabs and workspaces), unless that pane carries
//! another clone's token; an absent id is Unknown. A seated clone: SeatObservation decides; the matcher links
//! only the pane the committed binding names when that binding is from the snapshot's own incarnation, never by
//! token, terminal id or native session. No terminal-id, cwd or native-session evidence locates any clone.
//! Tabs carry no token: a seat's tab is its bound tab id in any incarnation (Herdr preserves tab ids across
//! restore and handoff; an absent id is Unknown, never voted); a seat with no bound tab id takes the tab
//! holding its matched panes. Workspaces: token, else bound id (any incarnation, same rule), else, with no
//! bound id, the workspace holding the teamspace's matched panes.
use crate::model::clone::CloneRecord;
use crate::model::common::{CloneLifecycle, Incarnation, Lifecycle};
use crate::model::{
    AnyId, CloneId, HerdrPaneId, HerdrTabId, HerdrWorkspaceId, IdKind, SeatId, TeamspaceId,
};
use crate::ports::herdr::{AgentSession, HerdrSnapshot, PaneInfo, TabInfo, WorkspaceInfo};
use crate::reconcile::desired::DesiredRuntime;
use crate::reconcile::planner::token_of;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Matches {
    pub panes: BTreeMap<HerdrPaneId, CloneId>,
    pub tabs: BTreeMap<HerdrTabId, SeatId>,
    pub workspaces: BTreeMap<HerdrWorkspaceId, TeamspaceId>,
    /// Reverse lookups of the three maps above.
    pub clone_pane: BTreeMap<CloneId, HerdrPaneId>,
    pub seat_tab: BTreeMap<SeatId, HerdrTabId>,
    pub ts_ws: BTreeMap<TeamspaceId, HerdrWorkspaceId>,
    /// Non-retired graph objects that carry a binding but have no live counterpart in the snapshot.
    pub unmatched: Vec<AnyId>,
}

impl Matches {
    fn link_pane(&mut self, p: &HerdrPaneId, c: &CloneId) {
        self.panes.insert(p.clone(), c.clone());
        self.clone_pane.insert(c.clone(), p.clone());
    }
    fn link_tab(&mut self, t: &HerdrTabId, s: &SeatId) {
        self.tabs.insert(t.clone(), s.clone());
        self.seat_tab.insert(s.clone(), t.clone());
    }
    fn link_ws(&mut self, w: &HerdrWorkspaceId, t: &TeamspaceId) {
        self.workspaces.insert(w.clone(), t.clone());
        self.ts_ws.insert(t.clone(), w.clone());
    }
}

pub struct PaneLoc<'a> {
    pub ws: &'a WorkspaceInfo,
    pub tab: &'a TabInfo,
    pub pane: &'a PaneInfo,
}

pub fn pane_locs(snap: &HerdrSnapshot) -> BTreeMap<HerdrPaneId, PaneLoc<'_>> {
    let mut out = BTreeMap::new();
    for ws in &snap.workspaces {
        for tab in &ws.tabs {
            for pane in &tab.panes {
                out.insert(pane.id.clone(), PaneLoc { ws, tab, pane });
            }
        }
    }
    out
}

pub fn find_tab<'a>(
    snap: &'a HerdrSnapshot,
    id: &HerdrTabId,
) -> Option<(&'a WorkspaceInfo, &'a TabInfo)> {
    snap.workspaces
        .iter()
        .find_map(|w| w.tabs.iter().find(|t| &t.id == id).map(|t| (w, t)))
}

pub fn find_ws<'a>(snap: &'a HerdrSnapshot, id: &HerdrWorkspaceId) -> Option<&'a WorkspaceInfo> {
    snap.workspaces.iter().find(|w| &w.id == id)
}

/// Native id of the clone's current occupant session.
pub fn current_native_id(c: &CloneRecord) -> Option<&str> {
    let occ = c.occupant.as_ref()?;
    c.sessions
        .iter()
        .find(|s| s.id == occ.native_session)
        .map(|s| s.native_session_id.as_str())
}

/// The cwd graph recorded for the clone: its latest native session's, else the cwd it was created with.
pub fn recorded_cwd(d: &DesiredRuntime, c: &CloneRecord) -> Option<PathBuf> {
    c.sessions
        .iter()
        .max_by_key(|s| s.started)
        .map(|s| s.cwd.clone())
        .or_else(|| d.pane(&c.id).map(|p| p.cwd.clone()))
}

pub fn agent_session_id(p: &PaneInfo) -> Option<String> {
    match p.agent.as_ref()?.session.as_ref()? {
        AgentSession::Id(s) if !s.is_empty() => Some(s.clone()),
        AgentSession::Path(path) => path.file_stem().and_then(|s| s.to_str()).map(str::to_owned),
        AgentSession::Id(_) => None,
    }
}

/// The pane the clone's committed binding names, when that is evidence of where the clone is. A seatless
/// clone's pane id holds in any incarnation (Herdr preserves pane ids across restore, as it does tab and
/// workspace ids; an absent id is Unknown). A seated clone is located by SeatObservation, so only the binding
/// SeatObservation last wrote (the snapshot's own incarnation) counts: never one from before a restart.
fn bound_pane_claim<'a>(c: &'a CloneRecord, inc: &Incarnation) -> Option<&'a HerdrPaneId> {
    let b = c.runtime.bound.as_ref()?;
    if c.threads_seat.is_some() && &b.incarnation != inc {
        return None;
    }
    b.pane_id.as_ref()
}

/// The pane carries the token of a graph object other than `clone` that is not a retired clone (a retired
/// clone's leftover token does not hold the pane; see pass 1).
fn carries_other_token(
    d: &DesiredRuntime,
    metadata: &BTreeMap<String, String>,
    clone: &CloneId,
) -> bool {
    let Some(t) = token_of(metadata) else {
        return false;
    };
    if t == clone.to_any() {
        return false;
    }
    !CloneId::parse(t.as_str())
        .ok()
        .and_then(|c| d.clones.get(&c))
        .is_some_and(|r| r.lifecycle == CloneLifecycle::Retired)
}

pub fn match_snapshot(snap: &HerdrSnapshot, d: &DesiredRuntime) -> Matches {
    let mut m = Matches::default();
    let locs = pane_locs(snap);

    // Panes, pass 1: graph token. A seated active clone is never located by token; threads decides.
    for (pid, loc) in &locs {
        let Some(any) = token_of(&loc.pane.metadata) else {
            continue;
        };
        if any.kind() != IdKind::Clone {
            continue;
        }
        let Ok(c) = CloneId::parse(any.as_str()) else {
            continue;
        };
        let Some(rec) = d.clones.get(&c) else {
            continue;
        };
        if rec.lifecycle == CloneLifecycle::Active && rec.threads_seat.is_some() {
            continue;
        }
        // A retired clone's leftover token must not hold a pane an active clone's committed binding names (an
        // undo adopted it): pass 2 links the pane to that clone, so it is never taken for gone before re-stamping.
        if rec.lifecycle == CloneLifecycle::Retired
            && d.clones.values().any(|o| {
                o.lifecycle == CloneLifecycle::Active
                    && bound_pane_claim(o, &snap.incarnation) == Some(pid)
            })
        {
            continue;
        }
        if !m.clone_pane.contains_key(&c) {
            m.link_pane(pid, &c);
        }
    }
    // Pass 2: the pane the committed binding names. Seatless clone: the preserved pane id, unless the pane
    // carries another clone's token. Seated clone: the binding of this incarnation only.
    for (cid, c) in &d.clones {
        if c.lifecycle != CloneLifecycle::Active || m.clone_pane.contains_key(cid) {
            continue;
        }
        let Some(pid) = bound_pane_claim(c, &snap.incarnation) else {
            continue;
        };
        let Some(loc) = locs.get(pid) else {
            continue;
        };
        if m.panes.contains_key(pid) {
            continue;
        }
        if c.threads_seat.is_none() && carries_other_token(d, &loc.pane.metadata, cid) {
            continue;
        }
        m.link_pane(pid, cid);
    }

    // Tabs. A bound tab id decides in any incarnation (Herdr preserves it; a vanished id stays vanished, no
    // vote); only a seat with no bound tab id takes the tab holding its matched panes (first bind).
    let mut need_containment: Vec<&SeatId> = Vec::new();
    for (sid, seat) in &d.seats {
        // A moved-out seat has no tab on purpose; its clones live elsewhere and must not drag a tab back in.
        if seat.lifecycle == Lifecycle::Retired || seat.moved_out {
            continue;
        }
        match seat.runtime.bound.as_ref() {
            // Herdr preserves tab ids across restore and handoff: a bound id links by equality in any
            // incarnation. Absent -> no link and no vote (Unknown).
            Some(b) if b.tab_id.is_some() => {
                if let Some(tid) = b.tab_id.as_ref()
                    && find_tab(snap, tid).is_some()
                    && !m.tabs.contains_key(tid)
                {
                    m.link_tab(tid, sid);
                }
            }
            _ => need_containment.push(sid),
        }
    }
    for sid in need_containment {
        let mut votes: BTreeMap<HerdrTabId, usize> = BTreeMap::new();
        for (cid, _) in d.clones.iter().filter(|(_, c)| &c.seat == sid) {
            if let Some(loc) = m.clone_pane.get(cid).and_then(|p| locs.get(p)) {
                *votes.entry(loc.tab.id.clone()).or_default() += 1;
            }
        }
        let best = votes
            .into_iter()
            .filter(|(t, _)| !m.tabs.contains_key(t))
            .max_by_key(|(t, n)| (*n, std::cmp::Reverse(t.clone())));
        if let Some((tid, _)) = best {
            m.link_tab(&tid, sid);
        }
    }

    // Workspaces.
    for ws in &snap.workspaces {
        let Some(any) = token_of(&ws.metadata) else {
            continue;
        };
        if any.kind() != IdKind::Teamspace {
            continue;
        }
        let Ok(t) = TeamspaceId::parse(any.as_str()) else {
            continue;
        };
        if d.teamspaces.contains_key(&t) && !m.ts_ws.contains_key(&t) {
            m.link_ws(&ws.id, &t);
        }
    }
    let mut ws_need: Vec<&TeamspaceId> = Vec::new();
    for (tid, ts) in &d.teamspaces {
        if ts.lifecycle == Lifecycle::Retired || m.ts_ws.contains_key(tid) {
            continue;
        }
        match ts.runtime.bound.as_ref() {
            // As for tabs: a bound workspace id links by equality in any incarnation; absent -> Unknown.
            Some(b) if b.workspace_id.is_some() => {
                if let Some(wid) = b.workspace_id.as_ref()
                    && find_ws(snap, wid).is_some()
                    && !m.workspaces.contains_key(wid)
                {
                    m.link_ws(wid, tid);
                }
            }
            _ => ws_need.push(tid),
        }
    }
    for tid in ws_need {
        let mut votes: BTreeMap<HerdrWorkspaceId, usize> = BTreeMap::new();
        for (cid, c) in &d.clones {
            let in_ts = d.seats.get(&c.seat).is_some_and(|s| &s.teamspace == tid);
            if !in_ts {
                continue;
            }
            if let Some(loc) = m.clone_pane.get(cid).and_then(|p| locs.get(p)) {
                *votes.entry(loc.ws.id.clone()).or_default() += 1;
            }
        }
        let best = votes
            .into_iter()
            .filter(|(w, _)| !m.workspaces.contains_key(w))
            .max_by_key(|(w, n)| (*n, std::cmp::Reverse(w.clone())));
        if let Some((wid, _)) = best {
            m.link_ws(&wid, tid);
        }
    }

    // Bound, non-retired objects with no live counterpart.
    let mut unmatched: BTreeSet<AnyId> = BTreeSet::new();
    for (id, ts) in &d.teamspaces {
        if ts.lifecycle != Lifecycle::Retired
            && ts.runtime.bound.is_some()
            && !m.ts_ws.contains_key(id)
        {
            unmatched.insert(id.to_any());
        }
    }
    for (id, s) in &d.seats {
        if s.lifecycle != Lifecycle::Retired
            && s.runtime.bound.is_some()
            && !m.seat_tab.contains_key(id)
        {
            unmatched.insert(id.to_any());
        }
    }
    for (id, c) in &d.clones {
        if c.lifecycle != CloneLifecycle::Retired
            && c.runtime.bound.is_some()
            && !m.clone_pane.contains_key(id)
        {
            unmatched.insert(id.to_any());
        }
    }
    m.unmatched = unmatched.into_iter().collect();
    m
}
