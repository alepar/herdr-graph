//! Identity matching between a complete snapshot and committed graph objects (spec §4.2).
//!
//! Panes: graph token, else `terminal_id` (same incarnation always; across incarnations only when cwd and, if
//! both are known, the agent session also match: live handoff), else the clone's current native session id.
//! Tabs carry no token: within one incarnation a seat's tab is its bound tab id; otherwise the tab holding
//! its matched panes. Workspaces: token, else bound id (same incarnation), else the workspace holding the
//! teamspace's matched panes.
use crate::model::clone::CloneRecord;
use crate::model::common::{CloneLifecycle, Lifecycle};
use crate::model::{AnyId, CloneId, HerdrPaneId, HerdrTabId, HerdrWorkspaceId, IdKind, SeatId, TeamspaceId};
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

pub fn find_tab<'a>(snap: &'a HerdrSnapshot, id: &HerdrTabId) -> Option<(&'a WorkspaceInfo, &'a TabInfo)> {
    snap.workspaces.iter().find_map(|w| w.tabs.iter().find(|t| &t.id == id).map(|t| (w, t)))
}

pub fn find_ws<'a>(snap: &'a HerdrSnapshot, id: &HerdrWorkspaceId) -> Option<&'a WorkspaceInfo> {
    snap.workspaces.iter().find(|w| &w.id == id)
}

/// Native id of the clone's current occupant session.
pub fn current_native_id(c: &CloneRecord) -> Option<&str> {
    let occ = c.occupant.as_ref()?;
    c.sessions.iter().find(|s| s.id == occ.native_session).map(|s| s.native_session_id.as_str())
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

pub fn match_snapshot(snap: &HerdrSnapshot, d: &DesiredRuntime) -> Matches {
    let mut m = Matches::default();
    let locs = pane_locs(snap);

    // Panes, pass 1: graph token.
    for (pid, loc) in &locs {
        let Some(any) = token_of(&loc.pane.metadata) else { continue };
        if any.kind() != IdKind::Clone {
            continue;
        }
        let Ok(c) = CloneId::parse(any.as_str()) else { continue };
        if d.clones.contains_key(&c) && !m.clone_pane.contains_key(&c) {
            m.link_pane(pid, &c);
        }
    }
    // Pass 2: terminal id (and, within an incarnation, the bound pane id).
    for (pid, loc) in &locs {
        if m.panes.contains_key(pid) {
            continue;
        }
        let pane = loc.pane;
        let pane_session = agent_session_id(pane);
        for (cid, c) in &d.clones {
            if c.lifecycle != CloneLifecycle::Active || m.clone_pane.contains_key(cid) {
                continue;
            }
            let Some(b) = c.runtime.bound.as_ref() else { continue };
            let term_eq = pane.terminal_id.is_some() && b.terminal_id == pane.terminal_id;
            let ok = if b.incarnation == snap.incarnation {
                term_eq || b.pane_id.as_ref() == Some(pid)
            } else {
                let cwd_ok = pane.cwd.is_some() && pane.cwd == recorded_cwd(d, c);
                let session_ok = match (&pane_session, current_native_id(c)) {
                    (Some(a), Some(b)) => a == b,
                    _ => true,
                };
                term_eq && cwd_ok && session_ok
            };
            if ok {
                m.link_pane(pid, cid);
                break;
            }
        }
    }
    // Pass 3: native session id.
    for (pid, loc) in &locs {
        if m.panes.contains_key(pid) {
            continue;
        }
        let Some(sid) = agent_session_id(loc.pane) else { continue };
        let hit = d
            .clones
            .iter()
            .find(|(cid, c)| {
                c.lifecycle == CloneLifecycle::Active
                    && !m.clone_pane.contains_key(*cid)
                    && current_native_id(c) == Some(sid.as_str())
            })
            .map(|(cid, _)| cid.clone());
        if let Some(cid) = hit {
            m.link_pane(pid, &cid);
        }
    }

    // Tabs. Within the bound incarnation the bound tab id decides (and a vanished tab stays vanished);
    // otherwise the tab holding the seat's matched panes.
    let mut need_containment: Vec<&SeatId> = Vec::new();
    for (sid, seat) in &d.seats {
        // A moved-out seat has no tab on purpose; its clones live elsewhere and must not drag a tab back in.
        if seat.lifecycle == Lifecycle::Retired || seat.moved_out {
            continue;
        }
        match seat.runtime.bound.as_ref() {
            Some(b) if b.incarnation == snap.incarnation => {
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
        let best = votes.into_iter().filter(|(t, _)| !m.tabs.contains_key(t)).max_by_key(|(t, n)| (*n, std::cmp::Reverse(t.clone())));
        if let Some((tid, _)) = best {
            m.link_tab(&tid, sid);
        }
    }

    // Workspaces.
    for ws in &snap.workspaces {
        let Some(any) = token_of(&ws.metadata) else { continue };
        if any.kind() != IdKind::Teamspace {
            continue;
        }
        let Ok(t) = TeamspaceId::parse(any.as_str()) else { continue };
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
            Some(b) if b.incarnation == snap.incarnation => {
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
        if ts.lifecycle != Lifecycle::Retired && ts.runtime.bound.is_some() && !m.ts_ws.contains_key(id) {
            unmatched.insert(id.to_any());
        }
    }
    for (id, s) in &d.seats {
        if s.lifecycle != Lifecycle::Retired && s.runtime.bound.is_some() && !m.seat_tab.contains_key(id) {
            unmatched.insert(id.to_any());
        }
    }
    for (id, c) in &d.clones {
        if c.lifecycle != CloneLifecycle::Retired && c.runtime.bound.is_some() && !m.clone_pane.contains_key(id) {
            unmatched.insert(id.to_any());
        }
    }
    m.unmatched = unmatched.into_iter().collect();
    m
}
