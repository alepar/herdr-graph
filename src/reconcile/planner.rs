//! Desired-vs-live diff for the Herdr effect family (spec §4.4). Level-triggered and pure: the same
//! committed state and snapshot always yield the same effect ids.
use super::desired::{DesiredPane, DesiredRuntime};
use super::executor::PlannedEffect;
use crate::journal::Journal;
use crate::model::Timestamp;
use crate::model::common::{Availability, CloneLifecycle, HerdrPaneId, HerdrTabId, HerdrWorkspaceId, Incarnation};
use crate::model::effect::{ContainerKind, EffectKind, EffectRecord, EffectStatus, EndState, PredictedEnd};
use crate::model::launch::{nonce_label, parse_graph_token};
use crate::model::{AnyId, CloneId, EffectId, SeatId, TeamspaceId};
use crate::ports::herdr::{HerdrSnapshot, PaneInfo, TabInfo, WorkspaceInfo};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Metadata key the graph token is stamped under.
pub const TOKEN_KEY: &str = "hg";

/// Herdr ids returned by a create, kept in the journal until the observer commits the binding, so that
/// dependent effects of one chain can find their targets (valid only within the same incarnation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveRef {
    #[serde(default)]
    pub workspace: Option<HerdrWorkspaceId>,
    #[serde(default)]
    pub tab: Option<HerdrTabId>,
    #[serde(default)]
    pub pane: Option<HerdrPaneId>,
    pub incarnation: Incarnation,
}

pub fn live_ref(journal: &Journal, object: &AnyId) -> Option<LiveRef> {
    let raw = journal.meta_get(&format!("live:{object}")).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

pub fn set_live_ref(journal: &Journal, object: &AnyId, r: &LiveRef) {
    if let Ok(raw) = serde_json::to_string(r) {
        let _ = journal.meta_set(&format!("live:{object}"), &raw);
    }
}

pub fn launched_key(clone: &CloneId) -> String {
    format!("launched:{clone}")
}

pub struct LivePane<'a> {
    pub ws: &'a WorkspaceInfo,
    pub tab: &'a TabInfo,
    pub pane: &'a PaneInfo,
    /// The pane carries its graph token (false when found through a binding or a journal live ref).
    pub stamped: bool,
}

pub struct LiveWorkspace<'a> {
    pub ws: &'a WorkspaceInfo,
    pub stamped: bool,
}

/// Matches graph objects to live Herdr objects: by token, else by binding in the same incarnation, else by
/// the journal's live refs. Tabs have no tokens (Herdr 0.9.1): a seat's tab is its bound tab id, else the tab
/// holding its token-matched panes.
pub struct LiveIndex<'a> {
    pub snap: &'a HerdrSnapshot,
    desired: &'a DesiredRuntime,
    journal: &'a Journal,
}

pub fn token_of(md: &BTreeMap<String, String>) -> Option<AnyId> {
    md.get(TOKEN_KEY).and_then(|v| parse_graph_token(v))
}

impl<'a> LiveIndex<'a> {
    pub fn new(snap: &'a HerdrSnapshot, desired: &'a DesiredRuntime, journal: &'a Journal) -> Self {
        Self { snap, desired, journal }
    }

    fn ws_by_id(&self, id: &HerdrWorkspaceId) -> Option<&'a WorkspaceInfo> {
        self.snap.workspaces.iter().find(|w| &w.id == id)
    }
    fn tab_by_id(&self, id: &HerdrTabId) -> Option<(&'a WorkspaceInfo, &'a TabInfo)> {
        self.snap.workspaces.iter().find_map(|w| w.tabs.iter().find(|t| &t.id == id).map(|t| (w, t)))
    }
    fn pane_by_id(&self, id: &HerdrPaneId) -> Option<LivePane<'a>> {
        self.snap.workspaces.iter().find_map(|w| {
            w.tabs.iter().find_map(|t| {
                t.panes.iter().find(|p| &p.id == id).map(|p| LivePane {
                    ws: w,
                    tab: t,
                    pane: p,
                    stamped: token_of(&p.metadata).is_some(),
                })
            })
        })
    }

    pub fn workspace(&self, ts: &TeamspaceId) -> Option<LiveWorkspace<'a>> {
        let any = ts.to_any();
        if let Some(ws) = self.snap.workspaces.iter().find(|w| token_of(&w.metadata).as_ref() == Some(&any)) {
            return Some(LiveWorkspace { ws, stamped: true });
        }
        let bound = self.desired.teamspaces.get(ts).and_then(|t| t.runtime.bound.as_ref());
        if let Some(b) = bound
            && b.incarnation == self.snap.incarnation
            && let Some(id) = &b.workspace_id
            && let Some(ws) = self.ws_by_id(id)
        {
            return Some(LiveWorkspace { ws, stamped: false });
        }
        let live = live_ref(self.journal, &any).filter(|l| l.incarnation == self.snap.incarnation);
        if let Some(ws) = live.and_then(|l| l.workspace).and_then(|id| self.ws_by_id(&id)) {
            return Some(LiveWorkspace { ws, stamped: false });
        }
        // A rebound clone pane (see `pane`) places its teamspace's workspace.
        self.desired
            .panes
            .iter()
            .filter(|p| p.ts == *ts)
            .find_map(|p| self.pane(&p.clone))
            .map(|lp| LiveWorkspace { ws: lp.ws, stamped: false })
    }

    pub fn pane(&self, clone: &CloneId) -> Option<LivePane<'a>> {
        let any = clone.to_any();
        for w in &self.snap.workspaces {
            for t in &w.tabs {
                if let Some(p) = t.panes.iter().find(|p| token_of(&p.metadata).as_ref() == Some(&any)) {
                    return Some(LivePane { ws: w, tab: t, pane: p, stamped: true });
                }
            }
        }
        let rec = self.desired.clones.get(clone);
        let bound = rec.and_then(|c| c.runtime.bound.as_ref());
        if let Some(b) = bound
            && b.incarnation == self.snap.incarnation
            && let Some(id) = &b.pane_id
            && let Some(lp) = self.pane_by_id(id)
        {
            return Some(LivePane { stamped: false, ..lp });
        }
        // `clone rebind` is an explicit user assertion that a pane is the clone's: a binding it wrote while the
        // clone was `unknown` keeps the old incarnation but is `present`, so honor it when that pane exists in
        // this snapshot and carries no graph token of its own (spec 4.2: graph re-stamps tokens on rebind).
        if let Some(b) = bound
            && rec.is_some_and(|c| c.runtime.availability == Availability::Present)
            && let Some(id) = &b.pane_id
            && let Some(lp) = self.pane_by_id(id)
            && token_of(&lp.pane.metadata).is_none()
        {
            return Some(LivePane { stamped: false, ..lp });
        }
        let live = live_ref(self.journal, &any)?;
        if live.incarnation != self.snap.incarnation {
            return None;
        }
        self.pane_by_id(live.pane.as_ref()?).map(|lp| LivePane { stamped: false, ..lp })
    }

    /// The seat whose committed binding (same incarnation) or journal live ref names this tab.
    pub fn tab_owner(&self, tab: &HerdrTabId) -> Option<SeatId> {
        for (seat, d) in &self.desired.seats {
            if let Some(b) = d.runtime.bound.as_ref()
                && b.incarnation == self.snap.incarnation
                && b.tab_id.as_ref() == Some(tab)
            {
                return Some(seat.clone());
            }
            if let Some(live) = live_ref(self.journal, &seat.to_any())
                && live.incarnation == self.snap.incarnation
                && live.tab.as_ref() == Some(tab)
            {
                return Some(seat.clone());
            }
        }
        None
    }

    fn owned_by_other(&self, tab: &HerdrTabId, seat: &SeatId) -> bool {
        self.tab_owner(tab).is_some_and(|o| &o != seat)
    }

    pub fn tab_for_seat(&self, seat: &SeatId) -> Option<(&'a WorkspaceInfo, &'a TabInfo)> {
        let bound = self.desired.seats.get(seat).and_then(|s| s.runtime.bound.as_ref());
        if let Some(b) = bound
            && b.incarnation == self.snap.incarnation
            && let Some(id) = &b.tab_id
            && !self.owned_by_other(id, seat)
            && let Some(found) = self.tab_by_id(id)
        {
            return Some(found);
        }
        if let Some(live) = live_ref(self.journal, &seat.to_any())
            && live.incarnation == self.snap.incarnation
            && let Some(id) = &live.tab
            && !self.owned_by_other(id, seat)
            && let Some(found) = self.tab_by_id(id)
        {
            return Some(found);
        }
        // A moved-out seat's clones live in other tabs: a token search would find (and claim) someone else's tab.
        if self.desired.tab(seat).is_some_and(|t| t.moved_out) {
            return None;
        }
        let clone_seat = |p: &PaneInfo| {
            token_of(&p.metadata)
                .and_then(|a| CloneId::parse(a.as_str()).ok())
                .and_then(|c| self.desired.clones.get(&c))
                .map(|c| &c.seat)
        };
        for w in &self.snap.workspaces {
            for t in &w.tabs {
                if self.owned_by_other(&t.id, seat) || t.panes.iter().any(|p| clone_seat(p).is_some_and(|s| s != seat)) {
                    continue;
                }
                if t.panes.iter().any(|p| clone_seat(p) == Some(seat)) {
                    return Some((w, t));
                }
            }
        }
        // A rebound clone pane (see `pane`) places its seat's tab.
        self.desired.panes_of(seat).find_map(|p| self.pane(&p.clone)).map(|lp| (lp.ws, lp.tab))
    }

    /// Active clones of `seat` that have no live pane and are not `unknown`, in clone-id order.
    pub fn missing_clones(&self, seat: &SeatId) -> Vec<&'a DesiredPane> {
        self.desired.panes_of(seat).filter(|p| !p.is_unknown() && self.pane(&p.clone).is_none()).collect()
    }
}

fn pred(object: AnyId, container: ContainerKind, end: EndState, induced: bool) -> PredictedEnd {
    PredictedEnd { object, container, end, induced }
}

struct Out<'a> {
    desired: &'a DesiredRuntime,
    now: Timestamp,
    planned: Vec<PlannedEffect>,
    seen: BTreeSet<EffectId>,
}

impl Out<'_> {
    fn mk(
        &mut self,
        kind: EffectKind,
        object: AnyId,
        rev: u64,
        predicted: Vec<PredictedEnd>,
        nonce_name: Option<&str>,
        deps: Vec<EffectId>,
    ) -> EffectId {
        let op = self.desired.op_for(&object);
        let id = EffectRecord::identity(&op, &object, &kind, rev);
        if self.seen.insert(id.clone()) {
            self.planned.push(PlannedEffect {
                record: EffectRecord {
                    id: id.clone(),
                    op,
                    object,
                    kind,
                    object_rev: rev,
                    fencing_rev: rev,
                    status: EffectStatus::Pending,
                    predicted,
                    nonce_label: nonce_name.map(|n| nonce_label(n, &id)),
                    attempts: 0,
                    last_error: None,
                    updated_at: self.now,
                },
                deps,
            });
        }
        id
    }
}

fn has_open(journal: &Journal, object: &AnyId, kind: &EffectKind) -> bool {
    journal
        .effects_for_object(object)
        .map(|rows| {
            rows.iter().any(|r| {
                &r.kind == kind && matches!(r.status, EffectStatus::Pending | EffectStatus::Unknown)
            })
        })
        .unwrap_or(false)
}

fn start_needed(journal: &Journal, p: &DesiredPane, has_agent: bool) -> bool {
    let any = p.clone.to_any();
    p.harness != crate::model::harness::Harness::Shell
        && p.occupant.is_none()
        && !has_agent
        && journal.meta_get(&launched_key(&p.clone)).ok().flatten().is_none()
        && !has_open(journal, &any, &EffectKind::RelaunchOccupant)
}

/// Effects implied by `desired` against `snap`. `journal` supplies launch records, live refs and open rows.
pub fn plan_effects(
    desired: &DesiredRuntime,
    snap: &HerdrSnapshot,
    journal: &Journal,
    now: Timestamp,
) -> Vec<PlannedEffect> {
    let idx = LiveIndex::new(snap, desired, journal);
    let mut o = Out { desired, now, planned: Vec::new(), seen: BTreeSet::new() };

    for ws in &desired.workspaces {
        let ts_any = ws.ts.to_any();
        let mut ws_dep: Vec<EffectId> = Vec::new();
        match idx.workspace(&ws.ts) {
            None => {
                // An empty workspace is pointless (and Herdr closes it with its last tab): only create one when
                // a seat tab is going into it.
                let wanted = desired.tabs.iter().filter(|t| t.ts == ws.ts).any(|t| {
                    !t.moved_out && !t.is_unknown() && !idx.missing_clones(&t.seat).is_empty()
                });
                if ws.is_unknown() || !wanted {
                    continue;
                }
                let c = o.mk(
                    EffectKind::CreateWorkspace,
                    ts_any.clone(),
                    ws.rev,
                    vec![pred(ts_any.clone(), ContainerKind::Workspace, EndState::Present, false)],
                    Some(&ws.name),
                    vec![],
                );
                let s = o.mk(EffectKind::StampToken, ts_any.clone(), ws.rev, vec![], None, vec![c]);
                let r = o.mk(
                    EffectKind::RenameWorkspace,
                    ts_any.clone(),
                    ws.rev,
                    vec![pred(ts_any.clone(), ContainerKind::Workspace, EndState::Renamed { name: ws.name.clone() }, false)],
                    None,
                    vec![s],
                );
                ws_dep = vec![r];
            }
            Some(l) => {
                if !l.stamped {
                    o.mk(EffectKind::StampToken, ts_any.clone(), ws.rev, vec![], None, vec![]);
                }
                if l.ws.label != ws.name {
                    o.mk(
                        EffectKind::RenameWorkspace,
                        ts_any.clone(),
                        ws.rev,
                        vec![pred(ts_any.clone(), ContainerKind::Workspace, EndState::Renamed { name: ws.name.clone() }, false)],
                        None,
                        vec![],
                    );
                }
            }
        }

        for tab in desired.tabs.iter().filter(|t| t.ts == ws.ts) {
            let seat_any = tab.seat.to_any();
            let panes: Vec<&DesiredPane> = desired.panes_of(&tab.seat).filter(|p| !p.is_unknown()).collect();
            let tab_pred = |end: EndState| pred(seat_any.clone(), ContainerKind::Tab, end, false);
            match idx.tab_for_seat(&tab.seat) {
                None => {
                    // r2: no create_tab for a moved-out seat, nor when every active clone already lives elsewhere.
                    if tab.moved_out || tab.is_unknown() {
                        continue;
                    }
                    let missing = idx.missing_clones(&tab.seat);
                    let Some(first) = missing.first() else { continue };
                    let ct = o.mk(
                        EffectKind::CreateTab,
                        seat_any.clone(),
                        tab.rev,
                        vec![
                            tab_pred(EndState::Present),
                            pred(first.clone.to_any(), ContainerKind::Pane, EndState::Present, false),
                        ],
                        Some(&tab.name),
                        ws_dep.clone(),
                    );
                    let st = o.mk(EffectKind::StampToken, first.clone.to_any(), first.rev, vec![], None, vec![ct]);
                    let rt = o.mk(
                        EffectKind::RenameTab,
                        seat_any.clone(),
                        tab.rev,
                        vec![tab_pred(EndState::Renamed { name: tab.name.clone() })],
                        None,
                        vec![st.clone()],
                    );
                    if start_needed(journal, first, false) {
                        o.mk(EffectKind::StartAgent, first.clone.to_any(), first.rev, vec![], None, vec![rt]);
                    }
                    for p in &missing[1..] {
                        split_chain(&mut o, journal, p, vec![st.clone()]);
                    }
                }
                Some((_, live_tab)) => {
                    // §4.3.4: never relabel a tab bound to another seat, nor on behalf of a moved-out seat.
                    let foreign = idx.tab_owner(&live_tab.id).is_some_and(|o| o != tab.seat);
                    if live_tab.label != tab.name && !tab.moved_out && !foreign {
                        o.mk(
                            EffectKind::RenameTab,
                            seat_any.clone(),
                            tab.rev,
                            vec![tab_pred(EndState::Renamed { name: tab.name.clone() })],
                            None,
                            vec![],
                        );
                    }
                    for p in panes {
                        match idx.pane(&p.clone) {
                            None => split_chain(&mut o, journal, p, vec![]),
                            Some(lp) => {
                                let any = p.clone.to_any();
                                if !lp.stamped {
                                    o.mk(EffectKind::StampToken, any.clone(), p.rev, vec![], None, vec![]);
                                }
                                if start_needed(journal, p, lp.pane.agent.is_some()) {
                                    o.mk(EffectKind::StartAgent, any.clone(), p.rev, vec![], None, vec![]);
                                }
                                if p.occupant.is_some()
                                    && lp.pane.agent.is_some()
                                    && !has_open(journal, &any, &EffectKind::ReplaceSession)
                                    && let Some(raw) = journal.meta_get(&launched_key(&p.clone)).ok().flatten()
                                    && serde_json::from_str::<serde_json::Value>(&raw).ok().as_ref()
                                        != Some(&p.launch_shape())
                                {
                                    let seat_rev = desired.seats.get(&p.seat).map_or(p.rev, |s| s.rev);
                                    o.mk(EffectKind::ReplaceSession, any, seat_rev, vec![], None, vec![]);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    plan_closes(&mut o, &idx);
    o.planned
}

fn split_chain(o: &mut Out<'_>, journal: &Journal, p: &DesiredPane, deps: Vec<EffectId>) {
    let any = p.clone.to_any();
    let sp = o.mk(
        EffectKind::SplitPane,
        any.clone(),
        p.rev,
        vec![pred(any.clone(), ContainerKind::Pane, EndState::Present, false)],
        None,
        deps,
    );
    let st = o.mk(EffectKind::StampToken, any.clone(), p.rev, vec![], None, vec![sp]);
    let rn = o.mk(
        EffectKind::RenamePane,
        any.clone(),
        p.rev,
        vec![pred(any.clone(), ContainerKind::Pane, EndState::Renamed { name: p.name.clone() }, false)],
        None,
        vec![st],
    );
    if start_needed(journal, p, false) {
        o.mk(EffectKind::StartAgent, any, p.rev, vec![], None, vec![rn]);
    }
}

/// Close effects for live graph objects whose graph object is gone, dormant or retired, with induced closures.
fn plan_closes(o: &mut Out<'_>, idx: &LiveIndex<'_>) {
    let d = o.desired;
    for ws in &idx.snap.workspaces {
        let Some(ts_any) = token_of(&ws.metadata) else { continue };
        let Ok(ts) = TeamspaceId::parse(ts_any.as_str()) else { continue };
        let Some(ts_rec) = d.teamspaces.get(&ts) else { continue };
        let graph_panes = |t: &TabInfo| -> Vec<(CloneId, SeatId)> {
            t.panes
                .iter()
                .filter_map(|p| token_of(&p.metadata))
                .filter_map(|a| CloneId::parse(a.as_str()).ok())
                .filter_map(|c| d.clones.get(&c).map(|r| (c, r.seat.clone())))
                .collect()
        };
        if d.workspace(&ts).is_none() {
            // Teamspace dormant or retired: the whole workspace goes; everything inside is induced.
            let mut predicted = vec![pred(ts_any.clone(), ContainerKind::Workspace, EndState::Closed, false)];
            let mut seats_done = BTreeSet::new();
            for t in &ws.tabs {
                for (c, s) in graph_panes(t) {
                    predicted.push(pred(c.to_any(), ContainerKind::Pane, EndState::Closed, true));
                    if seats_done.insert(s.clone()) {
                        predicted.push(pred(s.to_any(), ContainerKind::Tab, EndState::Closed, true));
                    }
                }
            }
            o.mk(EffectKind::CloseWorkspace, ts_any, ts_rec.rev, predicted, None, vec![]);
            continue;
        }

        // Tabs and panes closing in this workspace this round.
        struct Closing {
            tab: TabInfo,
            seat: SeatId,
            whole_tab: bool,
            clones: Vec<CloneId>,
        }
        let mut closing: Vec<Closing> = Vec::new();
        for t in &ws.tabs {
            let gp = graph_panes(t);
            let Some((_, seat)) = gp.first().cloned() else { continue };
            // A seat that is merely dormant keeps its active clones: the whole tab goes. A retired seat has only
            // retired clones, which close pane by pane (the last one takes the tab with it, induced).
            let any_active = gp.iter().any(|(c, _)| d.clones.get(c).is_some_and(|r| r.lifecycle == CloneLifecycle::Active));
            if d.tab(&seat).is_none() && any_active {
                closing.push(Closing { tab: t.clone(), seat, whole_tab: true, clones: gp.into_iter().map(|g| g.0).collect() });
                continue;
            }
            let dead: Vec<CloneId> = gp.iter().filter(|(c, _)| d.pane(c).is_none()).map(|(c, _)| c.clone()).collect();
            if !dead.is_empty() {
                closing.push(Closing { tab: t.clone(), seat, whole_tab: false, clones: dead });
            }
        }
        let tabs_closing = closing
            .iter()
            .filter(|c| c.whole_tab || c.clones.len() == c.tab.panes.len())
            .count();
        let ws_closes = tabs_closing == ws.tabs.len();
        for c in closing {
            let seat_rev = d.seats.get(&c.seat).map_or(0, |s| s.rev);
            let tab_closes = c.whole_tab || c.clones.len() == c.tab.panes.len();
            if c.whole_tab {
                let mut predicted = vec![pred(c.seat.to_any(), ContainerKind::Tab, EndState::Closed, false)];
                predicted.extend(c.clones.iter().map(|cl| pred(cl.to_any(), ContainerKind::Pane, EndState::Closed, true)));
                if ws_closes {
                    predicted.push(pred(ts_any.clone(), ContainerKind::Workspace, EndState::Closed, true));
                }
                o.mk(EffectKind::CloseTab, c.seat.to_any(), seat_rev, predicted, None, vec![]);
            } else {
                for cl in &c.clones {
                    let rev = d.clones.get(cl).map_or(0, |r| r.rev);
                    let mut predicted = vec![pred(cl.to_any(), ContainerKind::Pane, EndState::Closed, false)];
                    if tab_closes {
                        predicted.push(pred(c.seat.to_any(), ContainerKind::Tab, EndState::Closed, true));
                        if ws_closes {
                            predicted.push(pred(ts_any.clone(), ContainerKind::Workspace, EndState::Closed, true));
                        }
                    }
                    o.mk(EffectKind::ClosePane, cl.to_any(), rev, predicted, None, vec![]);
                }
            }
        }
    }
}
