//! Structural diff (spec §4.3.3): raw elements between two complete snapshots of one incarnation (`diff`),
//! or between a first snapshot and committed state (`rebind`). Classification against intent and grouping
//! into cascades happen in `classify`.
use super::matcher::{Matches, agent_session_id, current_native_id, pane_locs, recorded_cwd};
use super::sessions::{SessionCapture, capture_session};
use crate::model::common::CloneLifecycle;
use crate::model::effect::ContainerKind;
use crate::model::harness::{Harness, SessionIdSource, profile};
use crate::model::launch::parse_nonce_label;
use crate::model::native_session::SessionEndReason;
use crate::model::{AnyId, CloneId, HerdrPaneId, HerdrTabId};
use crate::ports::herdr::{HerdrSnapshot, ProcessInfo};
use crate::reconcile::desired::DesiredRuntime;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum DiffElement {
    /// A known pane now lives in another tab.
    Moved { clone: CloneId, from_tab: HerdrTabId, to_tab: HerdrTabId },
    /// A label differs from the committed name (`old`) and is not a creation nonce.
    Renamed { object: AnyId, old: String, new: String },
    /// The pane survives but its agent is gone (or was replaced by another session).
    OccupancyEnded { clone: CloneId, ns_native: Option<String>, reason: SessionEndReason },
    /// An agent with a capturable session appeared in a pane that has no occupant.
    OccupancyStarted { clone: CloneId, capture: SessionCapture },
    /// A graph object matched in the baseline has no live counterpart now.
    Disappeared { object: AnyId, container: ContainerKind },
}

pub struct DiffCx<'a> {
    pub desired: &'a DesiredRuntime,
    pub snap: &'a HerdrSnapshot,
    pub matches: &'a Matches,
    pub claude_root: &'a Path,
    pub procs: &'a BTreeMap<HerdrPaneId, ProcessInfo>,
}

fn is_nonce(label: &str) -> bool {
    parse_nonce_label(label).is_some()
}

fn agent_harness(d: &DesiredRuntime, clone: &CloneId, kind: &str) -> Option<Harness> {
    [Harness::Claude, Harness::Codex]
        .into_iter()
        .find(|h| profile(*h).agent_kind == Some(kind))
        .or_else(|| d.pane(clone).map(|p| p.harness).filter(|h| *h != Harness::Shell))
}

/// Panes whose agent has no Herdr session id and whose harness reads it from `process_info` argv: the caller
/// fetches these before building the diff.
pub fn needs_process(d: &DesiredRuntime, snap: &HerdrSnapshot, m: &Matches) -> Vec<HerdrPaneId> {
    let locs = pane_locs(snap);
    let mut out = Vec::new();
    for (cid, pid) in &m.clone_pane {
        let (Some(c), Some(loc)) = (d.clones.get(cid), locs.get(pid)) else { continue };
        let Some(agent) = loc.pane.agent.as_ref() else { continue };
        if c.lifecycle != CloneLifecycle::Active || agent_session_id(loc.pane).is_some() {
            continue;
        }
        let argv_source = agent_harness(d, cid, &agent.kind)
            .is_some_and(|h| profile(h).session_id_sources.iter().any(|s| matches!(s, SessionIdSource::ProcessInfoArgv { .. })));
        if argv_source {
            out.push(pid.clone());
        }
    }
    out
}

/// Level-triggered occupancy: committed occupant vs the live agent of every matched active clone.
/// `gone` is the reason recorded when the agent is missing from a surviving pane.
fn occupancy(cx: &DiffCx<'_>, gone: SessionEndReason) -> Vec<DiffElement> {
    let d = cx.desired;
    let locs = pane_locs(cx.snap);
    let mut out = Vec::new();
    for (cid, pid) in &cx.matches.clone_pane {
        let (Some(c), Some(loc)) = (d.clones.get(cid), locs.get(pid)) else { continue };
        if c.lifecycle != CloneLifecycle::Active {
            continue;
        }
        let occ_native = current_native_id(c).map(str::to_owned);
        match (&c.occupant, loc.pane.agent.as_ref()) {
            (None, None) => {}
            (Some(_), None) => out.push(DiffElement::OccupancyEnded { clone: cid.clone(), ns_native: occ_native, reason: gone }),
            (occupant, Some(agent)) => {
                let Some(h) = agent_harness(d, cid, &agent.kind) else { continue };
                let cwd = recorded_cwd(d, c).or_else(|| loc.pane.cwd.clone()).unwrap_or_else(|| PathBuf::from("/"));
                let Some(cap) = capture_session(h, loc.pane, cx.procs.get(pid), cx.claude_root, &cwd) else { continue };
                match occupant {
                    None => out.push(DiffElement::OccupancyStarted { clone: cid.clone(), capture: cap }),
                    Some(_) if occ_native.as_deref() != Some(cap.native_session_id.as_str()) => {
                        out.push(DiffElement::OccupancyEnded {
                            clone: cid.clone(),
                            ns_native: occ_native,
                            reason: SessionEndReason::SessionChanged,
                        });
                        out.push(DiffElement::OccupancyStarted { clone: cid.clone(), capture: cap });
                    }
                    Some(_) => {}
                }
            }
        }
    }
    out
}

/// Rebind pass: identity is matched elsewhere; here every matched object's label and occupant are compared
/// with COMMITTED state (not the baseline), so a rename or agent change during a gap is recorded.
pub fn rebind(cx: &DiffCx<'_>) -> Vec<DiffElement> {
    let d = cx.desired;
    let locs = pane_locs(cx.snap);
    let mut out = Vec::new();
    for (cid, pid) in &cx.matches.clone_pane {
        let (Some(c), Some(loc)) = (d.clones.get(cid), locs.get(pid)) else { continue };
        if c.lifecycle != CloneLifecycle::Active {
            continue;
        }
        if let Some(l) = loc.pane.label.as_deref()
            && l != c.name
            && !is_nonce(l)
        {
            out.push(DiffElement::Renamed { object: cid.to_any(), old: c.name.clone(), new: l.to_owned() });
        }
    }
    for (sid, tid) in &cx.matches.seat_tab {
        let (Some(s), Some((_, tab))) = (d.seats.get(sid), super::matcher::find_tab(cx.snap, tid)) else { continue };
        if tab.label != s.name && !is_nonce(&tab.label) {
            out.push(DiffElement::Renamed { object: sid.to_any(), old: s.name.clone(), new: tab.label.clone() });
        }
    }
    for (tsid, wid) in &cx.matches.ts_ws {
        let (Some(t), Some(ws)) = (d.teamspaces.get(tsid), super::matcher::find_ws(cx.snap, wid)) else { continue };
        if ws.label != t.name && !is_nonce(&ws.label) {
            out.push(DiffElement::Renamed { object: tsid.to_any(), old: t.name.clone(), new: ws.label.clone() });
        }
    }
    out.extend(occupancy(cx, SessionEndReason::Unknown));
    out
}

/// Diff of `base` (matched as `mb`) against the new snapshot in `cx`; both are complete snapshots of the
/// same incarnation.
pub fn diff(cx: &DiffCx<'_>, base: &HerdrSnapshot, mb: &Matches) -> Vec<DiffElement> {
    let d = cx.desired;
    let mn = cx.matches;
    let mut out = Vec::new();

    for (wid, t) in &mb.workspaces {
        let _ = wid;
        if !mn.ts_ws.contains_key(t) {
            out.push(DiffElement::Disappeared { object: t.to_any(), container: ContainerKind::Workspace });
        }
    }
    for s in mb.tabs.values() {
        if !mn.seat_tab.contains_key(s) {
            out.push(DiffElement::Disappeared { object: s.to_any(), container: ContainerKind::Tab });
        }
    }
    for c in mb.panes.values() {
        if !mn.clone_pane.contains_key(c) {
            out.push(DiffElement::Disappeared { object: c.to_any(), container: ContainerKind::Pane });
        }
    }

    let locs_b = pane_locs(base);
    let locs_n = pane_locs(cx.snap);
    for (cid, pb) in &mb.clone_pane {
        let Some(pn) = mn.clone_pane.get(cid) else { continue };
        let (Some(lb), Some(ln)) = (locs_b.get(pb), locs_n.get(pn)) else { continue };
        if lb.tab.id != ln.tab.id {
            out.push(DiffElement::Moved { clone: cid.clone(), from_tab: lb.tab.id.clone(), to_tab: ln.tab.id.clone() });
        }
        let Some(c) = d.clones.get(cid).filter(|c| c.lifecycle == CloneLifecycle::Active) else { continue };
        if lb.pane.label != ln.pane.label
            && let Some(l) = ln.pane.label.as_deref()
            && l != c.name
            && !is_nonce(l)
        {
            out.push(DiffElement::Renamed { object: cid.to_any(), old: c.name.clone(), new: l.to_owned() });
        }
    }
    for (sid, tn) in &mn.seat_tab {
        let Some(tb) = mb.seat_tab.get(sid) else { continue };
        let (Some((_, before)), Some((_, now)), Some(s)) =
            (super::matcher::find_tab(base, tb), super::matcher::find_tab(cx.snap, tn), d.seats.get(sid))
        else {
            continue;
        };
        if before.label != now.label && now.label != s.name && !is_nonce(&now.label) {
            out.push(DiffElement::Renamed { object: sid.to_any(), old: s.name.clone(), new: now.label.clone() });
        }
    }
    for (tsid, wn) in &mn.ts_ws {
        let Some(wb) = mb.ts_ws.get(tsid) else { continue };
        let (Some(before), Some(now), Some(t)) =
            (super::matcher::find_ws(base, wb), super::matcher::find_ws(cx.snap, wn), d.teamspaces.get(tsid))
        else {
            continue;
        };
        if before.label != now.label && now.label != t.name && !is_nonce(&now.label) {
            out.push(DiffElement::Renamed { object: tsid.to_any(), old: t.name.clone(), new: now.label.clone() });
        }
    }
    out.extend(occupancy(cx, SessionEndReason::AgentExited));
    out
}
