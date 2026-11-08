//! Desired-vs-live diff for the Herdr effect family (spec §4.4). Level-triggered and pure: the same
//! committed state and snapshot always yield the same effect ids.
use super::desired::{DesiredPane, DesiredRuntime, DesiredTab};
use super::executor::PlannedEffect;
use crate::journal::Journal;
use crate::model::Timestamp;
use crate::model::clone::{SeatFactState, SeatIntent};
use crate::model::common::{
    CloneLifecycle, HerdrPaneId, HerdrTabId, HerdrWorkspaceId, Incarnation,
};
use crate::model::effect::{
    ContainerKind, EffectKind, EffectRecord, EffectStatus, EndState, PredictedEnd,
};
use crate::model::launch::{nonce_label, parse_graph_token};
use crate::model::{AnyId, CloneId, EffectId, OpId, SeatId, TeamspaceId};
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

pub fn delete_live_ref(journal: &Journal, object: &AnyId) {
    let _ = journal.meta_delete(&format!("live:{object}"));
}

pub fn launched_key(clone: &CloneId) -> String {
    format!("launched:{clone}")
}

/// Journal meta: the pane and variable names the last start exported for `clone` (spec §5).
pub fn launch_env_key(clone: &CloneId) -> String {
    format!("launch_env:{clone}")
}

/// Journal meta: set while a clone's last pane is gone after an incarnation change, so `status` can name it.
pub fn resume_blocked_key(clone: &CloneId) -> String {
    format!("resume_blocked:{clone}")
}

/// Journal meta: `{"epoch","error","failures"}` while the launch mark of a pending Restore keeps failing, so
/// `status` can name it and the planner can give each retry a fresh effect identity.
pub fn restore_mark_failed_key(clone: &CloneId) -> String {
    format!("restore_mark_failed:{clone}")
}

/// Journal meta: `{"reason":"not_launchable"}`. No longer written: before D1 every acknowledged Restore of an
/// agent clone is launchable (resume, else a fresh agent), so the planner only clears a key an earlier build
/// left behind. `status` still reads it.
pub fn restore_stuck_key(clone: &CloneId) -> String {
    format!("restore_stuck:{clone}")
}

/// Write `want` under `key`, or delete the key when None; only when the stored value differs.
fn sync_meta(journal: &Journal, key: &str, want: Option<&str>) {
    let have = journal.meta_get(key).ok().flatten();
    if have.as_deref() == want {
        return;
    }
    let _ = match want {
        Some(v) => journal.meta_set(key, v),
        None => journal.meta_delete(key),
    };
}

/// Mark failures recorded for the Restore of `epoch` on `clone`.
pub fn restore_mark_failures(journal: &Journal, clone: &CloneId, epoch: &OpId) -> u64 {
    journal
        .meta_get(&restore_mark_failed_key(clone))
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .filter(|m| m["epoch"] == epoch.to_string())
        .and_then(|m| m["failures"].as_u64())
        .unwrap_or(0)
}

/// Journal meta: the `incarnation:changed_at` a restart relaunch of `clone` already ran for (one per change).
pub fn resumed_key(clone: &CloneId) -> String {
    format!("resumed_at:{clone}")
}

/// Journal meta key of the time threads last answered the key resolution of an unseated clone: the restart
/// trigger of an unseated clone waits for an answer taken after the restart (written by the threads executor).
pub fn threads_answered_key(clone: &CloneId) -> String {
    format!("threads_answered_at:{clone}")
}

fn threads_answered_at(journal: &Journal, clone: &CloneId) -> Option<Timestamp> {
    let raw = journal
        .meta_get(&threads_answered_key(clone))
        .ok()
        .flatten()?;
    chrono::DateTime::parse_from_rfc3339(&raw)
        .ok()
        .map(|t| t.to_utc())
}

pub fn incarnation_changed_at(journal: &Journal) -> Option<Timestamp> {
    let raw = journal.meta_get("incarnation:changed_at").ok().flatten()?;
    chrono::DateTime::parse_from_rfc3339(&raw)
        .ok()
        .map(|t| t.to_utc())
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

/// A late copy of a create of `object` (a teamspace's workspace or a seat's tab); see `LiveIndex::duplicates`.
pub struct Duplicate<'a> {
    pub object: AnyId,
    pub copy: Surplus<'a>,
}

pub enum Surplus<'a> {
    Workspace(&'a WorkspaceInfo),
    Tab(&'a TabInfo),
}

impl Surplus<'_> {
    /// The copy's label: the create's nonce label.
    pub fn label(&self) -> &str {
        match self {
            Surplus::Workspace(w) => &w.label,
            Surplus::Tab(t) => &t.label,
        }
    }

    /// The copy still has the shape Herdr created (one tab of one pane, no graph token, no agent).
    pub fn pristine_pane(&self) -> Option<&PaneInfo> {
        let tab = match self {
            Surplus::Workspace(w) => match w.tabs.as_slice() {
                [t] => t,
                _ => return None,
            },
            Surplus::Tab(t) => t,
        };
        match tab.panes.as_slice() {
            [p] if token_of(&p.metadata).is_none() && p.agent.is_none() => Some(p),
            _ => None,
        }
    }
}

/// Matches graph objects to live Herdr objects: by token, else by bound Herdr id (tabs and workspaces in any
/// incarnation, as Herdr preserves their ids; panes in the same incarnation only), else by the journal's live
/// refs. Tabs have no tokens (Herdr 0.9.1): a seat's tab is its bound tab id (absent in a newer incarnation:
/// none), else the tab holding its token-matched panes.
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
        Self {
            snap,
            desired,
            journal,
        }
    }

    fn ws_by_id(&self, id: &HerdrWorkspaceId) -> Option<&'a WorkspaceInfo> {
        self.snap.workspaces.iter().find(|w| &w.id == id)
    }
    fn tab_by_id(&self, id: &HerdrTabId) -> Option<(&'a WorkspaceInfo, &'a TabInfo)> {
        self.snap
            .workspaces
            .iter()
            .find_map(|w| w.tabs.iter().find(|t| &t.id == id).map(|t| (w, t)))
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

    /// The pane the clone's committed binding names, when this snapshot has a pane of that id (any
    /// incarnation: Herdr may keep pane ids across a restart). Tokens and live refs are not consulted.
    pub fn bound_pane(&self, clone: &CloneId) -> Option<LivePane<'a>> {
        let id = self
            .desired
            .clones
            .get(clone)?
            .runtime
            .bound
            .as_ref()?
            .pane_id
            .as_ref()?;
        self.pane_by_id(id).map(|lp| LivePane {
            stamped: false,
            ..lp
        })
    }

    /// The live workspace belongs to a teamspace other than `ts`: its graph token names another teamspace, or
    /// another desired teamspace's committed binding (this incarnation) names it. Such a workspace never places
    /// `ts`'s workspace nor holds its tabs (spec §4.9 Workspace placement).
    pub fn foreign_to(&self, ws: &WorkspaceInfo, ts: &TeamspaceId) -> bool {
        if token_of(&ws.metadata)
            .and_then(|t| TeamspaceId::parse(t.as_str()).ok())
            .is_some_and(|t| t != *ts)
        {
            return true;
        }
        self.desired.teamspaces.iter().any(|(id, t)| {
            id != ts
                && t.runtime.bound.as_ref().is_some_and(|b| {
                    b.incarnation == self.snap.incarnation
                        && b.workspace_id.as_ref() == Some(&ws.id)
                })
        })
    }

    pub fn workspace(&self, ts: &TeamspaceId) -> Option<LiveWorkspace<'a>> {
        let any = ts.to_any();
        if let Some(ws) = self
            .snap
            .workspaces
            .iter()
            .find(|w| token_of(&w.metadata).as_ref() == Some(&any))
        {
            return Some(LiveWorkspace { ws, stamped: true });
        }
        let bound = self
            .desired
            .teamspaces
            .get(ts)
            .and_then(|t| t.runtime.bound.as_ref());
        // Herdr preserves workspace ids across restore and handoff: a bound id matches in any incarnation, and
        // one bound in an older incarnation that is now absent leaves the workspace unplaced (no fallback).
        if let Some(b) = bound
            && let Some(id) = &b.workspace_id
        {
            if let Some(ws) = self.ws_by_id(id) {
                return Some(LiveWorkspace { ws, stamped: false });
            }
            // A restoring teamspace (spec §8 step 1) is the one exception: the workspace its own create made in
            // this incarnation is found by the journal live ref, below, so the create can be stamped and bound.
            if b.incarnation != self.snap.incarnation && !self.desired.restoring_teamspace(ts) {
                return None;
            }
        }
        let live = live_ref(self.journal, &any).filter(|l| l.incarnation == self.snap.incarnation);
        if let Some(ws) = live
            .and_then(|l| l.workspace)
            .and_then(|id| self.ws_by_id(&id))
        {
            return Some(LiveWorkspace { ws, stamped: false });
        }
        // A completed create can outlive its incarnation before token stamping (startup subscription,
        // disconnect or daemon crash). Its durable nonce identifies the workspace without trusting a
        // reused Herdr ID. Open creates use the same correlation after a lost response.
        let nonces: BTreeSet<_> = self
            .journal
            .effects_for_object(&any)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| {
                e.kind == EffectKind::CreateWorkspace
                    && matches!(
                        e.status,
                        EffectStatus::Pending | EffectStatus::Unknown | EffectStatus::Done
                    )
            })
            .filter_map(|e| e.nonce_label)
            .collect();
        let mut matches = self
            .snap
            .workspaces
            .iter()
            .filter(|w| token_of(&w.metadata).is_none() && nonces.contains(&w.label));
        if let Some(ws) = matches.next()
            && matches.next().is_none()
        {
            return Some(LiveWorkspace { ws, stamped: false });
        }
        // A rebound clone pane (see `pane`) places its teamspace's workspace, unless it sits in a workspace that
        // belongs to another teamspace (a moved clone lives in someone else's). Placement reads the pane's
        // workspace, never `moved_out`: a node manager with a clone elsewhere still has its own workspace.
        self.desired
            .panes
            .iter()
            .filter(|p| p.ts == *ts)
            .find_map(|p| self.pane(&p.clone).filter(|lp| !self.foreign_to(lp.ws, ts)))
            .map(|lp| LiveWorkspace {
                ws: lp.ws,
                stamped: false,
            })
    }

    /// Where the clone's pane is, by the one rule per clone kind (the matcher's). A seated clone
    /// (`threads_seat`): the pane its committed binding names, when that binding is of this snapshot's
    /// incarnation (SeatObservation wrote it); never its token or an older binding. A seatless clone: its token,
    /// else the pane id its committed binding names in any incarnation (Herdr preserves pane ids) unless the pane
    /// carries another clone's token. For both, last, the pane this incarnation's own create produced (the
    /// journal live ref): that is create correlation, not re-matching, and a seated clone needs it when the
    /// graph re-creates its pane (reactivation, undo) before its seat resolves.
    pub fn pane(&self, clone: &CloneId) -> Option<LivePane<'a>> {
        let any = clone.to_any();
        let rec = self.desired.clones.get(clone);
        let bound = rec.and_then(|c| c.runtime.bound.as_ref());
        let seated = rec.is_some_and(|c| c.threads_seat.is_some());
        if !seated {
            for w in &self.snap.workspaces {
                for t in &w.tabs {
                    if let Some(p) = t
                        .panes
                        .iter()
                        .find(|p| token_of(&p.metadata).as_ref() == Some(&any))
                    {
                        return Some(LivePane {
                            ws: w,
                            tab: t,
                            pane: p,
                            stamped: true,
                        });
                    }
                }
            }
        }
        if let Some(b) = bound
            && (!seated || b.incarnation == self.snap.incarnation)
            && let Some(id) = &b.pane_id
            && let Some(lp) = self.pane_by_id(id)
            && (seated || self.carries_no_other_clone_token(lp.pane, clone))
        {
            return Some(LivePane {
                stamped: token_of(&lp.pane.metadata).as_ref() == Some(&any),
                ..lp
            });
        }
        let live = live_ref(self.journal, &any)?;
        if live.incarnation != self.snap.incarnation {
            return None;
        }
        // A pane another active clone's committed binding claims (e.g. adopted by an undo from the pane of a
        // displaced clone) is no longer this clone's, whatever the stale live ref says.
        let lp = self.pane_by_id(live.pane.as_ref()?)?;
        if self.pane_claimed_by(&lp.pane.id, clone).is_some() {
            return None;
        }
        Some(LivePane {
            stamped: token_of(&lp.pane.metadata).as_ref() == Some(&any),
            ..lp
        })
    }

    /// The pane has no graph token, or carries `clone`'s own, or a retired clone's leftover (an undo adopted it).
    fn carries_no_other_clone_token(&self, pane: &PaneInfo, clone: &CloneId) -> bool {
        let Some(t) = token_of(&pane.metadata) else {
            return true;
        };
        t == clone.to_any()
            || CloneId::parse(t.as_str())
                .ok()
                .and_then(|c| self.desired.clones.get(&c))
                .is_some_and(|r| r.lifecycle == CloneLifecycle::Retired)
    }

    /// An active desired clone other than `except` whose committed binding names this pane. Bindings only (no
    /// `pane()` call) and no incarnation filter: an `unknown` adopted clone with a pane-only binding claims it.
    pub fn pane_claimed_by(&self, pane: &HerdrPaneId, except: &CloneId) -> Option<CloneId> {
        self.desired
            .panes
            .iter()
            .find(|q| {
                q.clone != *except
                    && q.bound
                        .as_ref()
                        .is_some_and(|b| b.pane_id.as_ref() == Some(pane))
            })
            .map(|q| q.clone.clone())
    }

    /// The seat whose committed binding (any incarnation) or journal live ref names this tab.
    pub fn tab_owner(&self, tab: &HerdrTabId) -> Option<SeatId> {
        for (seat, d) in &self.desired.seats {
            if let Some(b) = d.runtime.bound.as_ref()
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
        let bound = self
            .desired
            .seats
            .get(seat)
            .and_then(|s| s.runtime.bound.as_ref());
        if let Some(b) = bound
            && let Some(id) = &b.tab_id
        {
            if !self.owned_by_other(id, seat)
                && let Some(found) = self.tab_by_id(id)
            {
                return Some(found);
            }
            // Bound in an older incarnation and absent now: unplaced, never re-found by tokens or panes.
            if b.incarnation != self.snap.incarnation && self.tab_by_id(id).is_none() {
                return None;
            }
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
        if self
            .desired
            .tab(seat)
            .is_some_and(|t| gates_recreation(self.desired, t))
        {
            return None;
        }
        // Spec §4.9: a workspace stamped for or bound to another teamspace never holds this seat's tab, the same
        // exclusion `workspace` applies. The binding and live-ref branches above name tabs graph recorded itself.
        let ts = self.desired.tab(seat).map(|t| &t.ts);
        let foreign = |w: &WorkspaceInfo| ts.is_some_and(|ts| self.foreign_to(w, ts));
        let clone_seat = |p: &PaneInfo| {
            token_of(&p.metadata)
                .and_then(|a| CloneId::parse(a.as_str()).ok())
                .and_then(|c| self.desired.clones.get(&c))
                .map(|c| &c.seat)
        };
        // A tab another seat owns or holds a clone in is never this seat's tab.
        let others = |t: &TabInfo| {
            self.owned_by_other(&t.id, seat)
                || t.panes
                    .iter()
                    .any(|p| clone_seat(p).is_some_and(|s| s != seat))
        };
        for w in self.snap.workspaces.iter().filter(|w| !foreign(w)) {
            for t in &w.tabs {
                if others(t) {
                    continue;
                }
                if t.panes.iter().any(|p| clone_seat(p) == Some(seat)) {
                    return Some((w, t));
                }
            }
        }
        // A rebound clone pane (see `pane`) places its seat's tab, unless it sits in another teamspace's
        // workspace or in a tab another seat claims (a moved clone): the survivor's tab is not where the missing
        // clone reopens (§4.9).
        self.desired
            .panes_of(seat)
            .find_map(|p| {
                self.pane(&p.clone)
                    .filter(|lp| !foreign(lp.ws) && !others(lp.tab))
            })
            .map(|lp| (lp.ws, lp.tab))
    }

    /// Nonce labels of `object`'s completed creates of `kind`.
    fn done_nonces(&self, object: &AnyId, kind: &EffectKind) -> BTreeSet<String> {
        self.journal
            .effects_for_object(object)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| &e.kind == kind && e.status == EffectStatus::Done)
            .filter_map(|e| e.nonce_label)
            .collect()
    }

    /// Late copies of our own creates: Herdr can apply a request after its client died, so a create whose
    /// outcome was lost and that recovery re-issued may land a second time. A copy carries the nonce of a
    /// completed create of the same object while that object's live counterpart is another workspace or tab.
    pub fn duplicates(&self) -> Vec<Duplicate<'a>> {
        let mut out = Vec::new();
        for ws in &self.desired.workspaces {
            let any = ws.ts.to_any();
            let Some(live) = self.workspace(&ws.ts).filter(|l| l.stamped) else {
                continue;
            };
            let nonces = self.done_nonces(&any, &EffectKind::CreateWorkspace);
            for w in &self.snap.workspaces {
                if w.id != live.ws.id
                    && token_of(&w.metadata).is_none()
                    && nonces.contains(&w.label)
                {
                    out.push(Duplicate {
                        object: any.clone(),
                        copy: Surplus::Workspace(w),
                    });
                }
            }
        }
        for tab in &self.desired.tabs {
            let any = tab.seat.to_any();
            let Some((_, live)) = self.tab_for_seat(&tab.seat) else {
                continue;
            };
            let nonces = self.done_nonces(&any, &EffectKind::CreateTab);
            for t in self.snap.workspaces.iter().flat_map(|w| &w.tabs) {
                if t.id != live.id && nonces.contains(&t.label) && self.tab_owner(&t.id).is_none() {
                    out.push(Duplicate {
                        object: any.clone(),
                        copy: Surplus::Tab(t),
                    });
                }
            }
        }
        out
    }

    /// Active clones of `seat` that have no live pane and are not `unknown`, in clone-id order.
    pub fn missing_clones(&self, seat: &SeatId) -> Vec<&'a DesiredPane> {
        self.desired
            .panes_of(seat)
            .filter(|p| !p.is_unknown() && self.pane(&p.clone).is_none())
            .collect()
    }
}

/// `moved_out` gates tab and pane recreation for plain seats only; the node manager is reopened whatever it says (spec §4.9 Reopening).
fn gates_recreation(d: &DesiredRuntime, tab: &DesiredTab) -> bool {
    tab.moved_out && d.node_manager.as_ref() != Some(&tab.seat)
}

fn pred(object: AnyId, container: ContainerKind, end: EndState, induced: bool) -> PredictedEnd {
    PredictedEnd {
        object,
        container,
        end,
        induced,
    }
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
        self.mk_as(op, rev, rev, kind, object, predicted, nonce_name, deps)
    }

    /// `mk` with an explicit attribution op and identity revision (`fencing_rev` stays the object's).
    #[allow(clippy::too_many_arguments)]
    fn mk_as(
        &mut self,
        op: OpId,
        identity_rev: u64,
        rev: u64,
        kind: EffectKind,
        object: AnyId,
        predicted: Vec<PredictedEnd>,
        nonce_name: Option<&str>,
        deps: Vec<EffectId>,
    ) -> EffectId {
        let id = EffectRecord::identity(&op, &object, &kind, identity_rev);
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
                    sched: Default::default(),
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

/// Statically covered by the restart resume rule (spec §9): the clone had an agent and threads says its seat
/// is unresolved. With a resumable session the rule resumes it, without one it starts a fresh keyed agent
/// (the key reattaches the seat). Whether the rule fires now is `plan_resume`'s question.
fn restart_covered(p: &DesiredPane) -> bool {
    p.had_agent
        && p.threads_seat.is_some()
        && p.seat_fact.as_ref().is_some_and(|f| {
            Some(&f.seat) == p.threads_seat.as_ref() && f.state == SeatFactState::Unresolved
        })
}

/// An unseated clone that had an agent and holds a key reservation: the restart rule launches it so the keyed
/// check-in tells threads its seat (spec §2 "Unseated clone after D1", §9). Unlike `restart_covered` it does
/// not shut the ordinary start rule out: that rule only fires for a pane with no launch record (a fresh pane),
/// the restart rule only for a launch lost with the previous incarnation, and an agent in the pane stops both.
fn restart_unseated(p: &DesiredPane) -> bool {
    p.had_agent && p.threads_seat.is_none() && p.key_reservation.is_some()
}

/// An acknowledged `Restore` whose launch mark predates `changed_at`: the restore launch ran in an earlier
/// incarnation and the clone never reattached, so the restart trigger owns it again (spec §14.8, §16.7).
pub fn launched_before(intent: &Option<SeatIntent>, changed_at: Timestamp) -> bool {
    matches!(
        intent,
        Some(SeatIntent::Restore { threads_done: true, launched: Some(t), .. }) if *t < changed_at
    )
}

/// The ordinary start rule: only a clone no resume trigger covers and with no pending Restore intent. Every
/// keyed launch waits for the clone's recorded key reservation (spec §5, §14.4); a held-back clone is retried
/// next pass.
fn start_needed(journal: &Journal, p: &DesiredPane, has_agent: bool) -> bool {
    let any = p.clone.to_any();
    p.harness != crate::model::harness::Harness::Shell
        && p.key_reservation.is_some()
        && !matches!(p.seat_intent, Some(SeatIntent::Restore { .. }))
        && !restart_covered(p)
        && p.occupant.is_none()
        && !has_agent
        && journal
            .meta_get(&launched_key(&p.clone))
            .ok()
            .flatten()
            .is_none()
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
    let mut o = Out {
        desired,
        now,
        planned: Vec::new(),
        seen: BTreeSet::new(),
    };

    for ws in &desired.workspaces {
        let ts_any = ws.ts.to_any();
        let mut ws_dep: Vec<EffectId> = Vec::new();
        match idx.workspace(&ws.ts) {
            None => {
                // An empty workspace is pointless (and Herdr closes it with its last tab): only create one when
                // a seat tab is going into it.
                let wanted = desired.tabs.iter().filter(|t| t.ts == ws.ts).any(|t| {
                    !gates_recreation(desired, t)
                        && (!t.is_unknown() || desired.restoring_seat(&t.seat))
                        && !idx.missing_clones(&t.seat).is_empty()
                });
                if (ws.is_unknown() && !desired.restoring_teamspace(&ws.ts)) || !wanted {
                    continue;
                }
                let c = o.mk(
                    EffectKind::CreateWorkspace,
                    ts_any.clone(),
                    ws.rev,
                    vec![pred(
                        ts_any.clone(),
                        ContainerKind::Workspace,
                        EndState::Present,
                        false,
                    )],
                    Some(&ws.name),
                    vec![],
                );
                let s = o.mk(
                    EffectKind::StampToken,
                    ts_any.clone(),
                    ws.rev,
                    vec![],
                    None,
                    vec![c],
                );
                let r = o.mk(
                    EffectKind::RenameWorkspace,
                    ts_any.clone(),
                    ws.rev,
                    vec![pred(
                        ts_any.clone(),
                        ContainerKind::Workspace,
                        EndState::Renamed {
                            name: ws.name.clone(),
                        },
                        false,
                    )],
                    None,
                    vec![s],
                );
                ws_dep = vec![r];
            }
            Some(l) => {
                if !l.stamped {
                    o.mk(
                        EffectKind::StampToken,
                        ts_any.clone(),
                        ws.rev,
                        vec![],
                        None,
                        vec![],
                    );
                }
                if l.ws.label != ws.name {
                    o.mk(
                        EffectKind::RenameWorkspace,
                        ts_any.clone(),
                        ws.rev,
                        vec![pred(
                            ts_any.clone(),
                            ContainerKind::Workspace,
                            EndState::Renamed {
                                name: ws.name.clone(),
                            },
                            false,
                        )],
                        None,
                        vec![],
                    );
                }
            }
        }

        for tab in desired.tabs.iter().filter(|t| t.ts == ws.ts) {
            let seat_any = tab.seat.to_any();
            let panes: Vec<&DesiredPane> = desired
                .panes_of(&tab.seat)
                .filter(|p| !p.is_unknown())
                .collect();
            let tab_pred = |end: EndState| pred(seat_any.clone(), ContainerKind::Tab, end, false);
            match idx.tab_for_seat(&tab.seat) {
                None => {
                    // r2: no create_tab for a moved-out seat, nor when every active clone already lives elsewhere.
                    // Spec section 8 step 1: a pending Restore reopens the seat's tab even when it is Unknown (section 7:
                    // a bound id absent from the live snapshot vanished); any other Unknown tab gets no create.
                    if gates_recreation(desired, tab)
                        || (tab.is_unknown() && !desired.restoring_seat(&tab.seat))
                    {
                        continue;
                    }
                    // A clone adopted into a pane outside the seat's own tab (undo from the caller's pane) is bound
                    // to a live pane already: it needs its token, and no tab of its own.
                    for p in &panes {
                        if let Some(lp) = idx.pane(&p.clone)
                            && !lp.stamped
                        {
                            o.mk(
                                EffectKind::StampToken,
                                p.clone.to_any(),
                                p.rev,
                                vec![],
                                None,
                                vec![],
                            );
                        }
                    }
                    let missing = idx.missing_clones(&tab.seat);
                    let Some(first) = missing.first() else {
                        continue;
                    };
                    let ct = o.mk(
                        EffectKind::CreateTab,
                        seat_any.clone(),
                        tab.rev,
                        vec![
                            tab_pred(EndState::Present),
                            pred(
                                first.clone.to_any(),
                                ContainerKind::Pane,
                                EndState::Present,
                                false,
                            ),
                        ],
                        Some(&tab.name),
                        ws_dep.clone(),
                    );
                    let st = o.mk(
                        EffectKind::StampToken,
                        first.clone.to_any(),
                        first.rev,
                        vec![],
                        None,
                        vec![ct],
                    );
                    let rt = o.mk(
                        EffectKind::RenameTab,
                        seat_any.clone(),
                        tab.rev,
                        vec![tab_pred(EndState::Renamed {
                            name: tab.name.clone(),
                        })],
                        None,
                        vec![st.clone()],
                    );
                    if start_needed(journal, first, false) {
                        o.mk(
                            EffectKind::StartAgent,
                            first.clone.to_any(),
                            first.rev,
                            vec![],
                            None,
                            vec![rt],
                        );
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
                            vec![tab_pred(EndState::Renamed {
                                name: tab.name.clone(),
                            })],
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
                                    o.mk(
                                        EffectKind::StampToken,
                                        any.clone(),
                                        p.rev,
                                        vec![],
                                        None,
                                        vec![],
                                    );
                                }
                                if start_needed(journal, p, lp.pane.agent.is_some()) {
                                    o.mk(
                                        EffectKind::StartAgent,
                                        any.clone(),
                                        p.rev,
                                        vec![],
                                        None,
                                        vec![],
                                    );
                                }
                                if p.occupant.is_some()
                                    && lp.pane.agent.is_some()
                                    && !has_open(journal, &any, &EffectKind::ReplaceSession)
                                    && let Some(raw) =
                                        journal.meta_get(&launched_key(&p.clone)).ok().flatten()
                                    && serde_json::from_str::<serde_json::Value>(&raw)
                                        .ok()
                                        .as_ref()
                                        != Some(&p.launch_shape())
                                {
                                    let seat_rev =
                                        desired.seats.get(&p.seat).map_or(p.rev, |s| s.rev);
                                    o.mk(
                                        EffectKind::ReplaceSession,
                                        any,
                                        seat_rev,
                                        vec![],
                                        None,
                                        vec![],
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    plan_resume(&mut o, &idx, journal);
    plan_closes(&mut o, &idx);
    for dup in idx.duplicates() {
        // One effect per copy: the Herdr id and incarnation name it, the nonce label lets the executor find it.
        let id = match &dup.copy {
            Surplus::Workspace(w) => w.id.0.as_str(),
            Surplus::Tab(t) => t.id.0.as_str(),
        };
        let target = format!(
            "{} {id} {}",
            EffectKind::CloseDuplicate.as_str(),
            serde_json::to_string(&snap.incarnation).unwrap_or_default()
        );
        let op = desired.op_for(&dup.object);
        let rev = desired.rev_of(&dup.object).unwrap_or(0);
        let ef = EffectId::derive(&op, &dup.object, &target, rev);
        if o.seen.insert(ef.clone()) {
            o.planned.push(PlannedEffect {
                record: EffectRecord {
                    id: ef,
                    op,
                    object: dup.object,
                    kind: EffectKind::CloseDuplicate,
                    object_rev: rev,
                    fencing_rev: rev,
                    status: EffectStatus::Pending,
                    predicted: vec![],
                    nonce_label: Some(dup.copy.label().to_owned()),
                    attempts: 0,
                    last_error: None,
                    updated_at: now,
                    sched: Default::default(),
                },
                deps: vec![],
            });
        }
    }
    o.planned
}

/// What a planning pass must do to a journal meta key so it holds the wanted value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetaChange {
    Keep,
    Set,
    Delete,
}

pub(crate) fn meta_change(current: Option<&str>, wanted: Option<&str>) -> MetaChange {
    match (current, wanted) {
        (c, w) if c == w => MetaChange::Keep,
        (_, Some(_)) => MetaChange::Set,
        (_, None) => MetaChange::Delete,
    }
}

/// Writes or deletes `key` only when its value differs, so a steady state costs no fsync per pass.
fn put_meta_if_changed(journal: &Journal, key: &str, wanted: Option<&str>) {
    let current = journal.meta_get(key).ok().flatten();
    match meta_change(current.as_deref(), wanted) {
        MetaChange::Keep => {}
        MetaChange::Set => {
            if let Some(v) = wanted {
                let _ = journal.meta_set(key, v);
            }
        }
        MetaChange::Delete => {
            let _ = journal.meta_delete(key);
        }
    }
}

/// The resume driver (spec §9, §14.4): the single launcher for the restart and restore triggers. Both need
/// the clone's bound pane to exist in this snapshot with no agent in it; only the session path exists before
/// D1: both triggers resume the last session when there is one and otherwise launch a fresh keyed agent, which
/// the key reattaches to the same seat. `RelaunchOccupant` carries the 90 s incarnation grace and the
/// shell-foreground guard; the restore launch runs at once, once per Restore epoch.
fn plan_resume(o: &mut Out<'_>, idx: &LiveIndex<'_>, journal: &Journal) {
    let changed_at = incarnation_changed_at(journal);
    let desired = o.desired;
    for p in &desired.panes {
        let any = p.clone.to_any();
        let blocked_key = resume_blocked_key(&p.clone);
        let mut blocked: Option<serde_json::Value> = None;
        let startable = p.harness != crate::model::harness::Harness::Shell && p.occupant.is_none();
        let pane = idx.bound_pane(&p.clone);
        let key_conflict = p
            .seat_fact
            .as_ref()
            .is_some_and(|f| matches!(f.state, SeatFactState::KeyConflict { .. }));

        if startable
            && !key_conflict
            && let Some(SeatIntent::Restore {
                seat: restore_seat,
                epoch,
                threads_done: true,
                launched: None,
                ..
            }) = &p.seat_intent
            // The key must be bound to the Restore's seat before the restore launch (spec §14.4, §16.1): no
            // reservation, or one bound elsewhere, holds the launch back without spending the epoch's attempt.
            && p.key_reservation
                .as_ref()
                .is_some_and(|r| r.seat.as_deref() == Some(restore_seat.as_str()))
        {
            if pane.as_ref().is_some_and(|lp| lp.pane.agent.is_none()) {
                // One launch per epoch is enforced by the launch mark, not by the effect identity: the identity
                // revision counts failed marks, so a failed mark is retried without a launch.
                o.mk_as(
                    epoch.clone(),
                    restore_mark_failures(journal, &p.clone, epoch),
                    p.rev,
                    EffectKind::StartAgent,
                    any,
                    vec![],
                    None,
                    vec![],
                );
            }
        } else if startable
            && p.key_reservation.is_some()
            && let Some(changed) = changed_at
            && (p.seat_intent.is_none() || launched_before(&p.seat_intent, changed))
            // A seated clone waits for a threads fact read after the restart; an unseated one has none, so it
            // waits for an answer to its key resolution taken after the restart: with threads unreachable
            // nothing launches.
            && (restart_unseated(p)
                && threads_answered_at(journal, &p.clone).is_some_and(|at| at > changed)
                || restart_covered(p)
                    && p.seat_fact.as_ref().is_some_and(|f| f.read_at >= changed))
        {
            match &pane {
                Some(lp) => {
                    let done = journal.meta_get(&resumed_key(&p.clone)).ok().flatten();
                    if lp.pane.agent.is_none()
                        && done.as_deref() != Some(changed.to_rfc3339().as_str())
                        && !has_open(journal, &any, &EffectKind::RelaunchOccupant)
                    {
                        o.mk(
                            EffectKind::RelaunchOccupant,
                            any,
                            p.rev,
                            vec![],
                            None,
                            vec![],
                        );
                    }
                }
                None => {
                    let last = p
                        .bound
                        .as_ref()
                        .and_then(|b| b.pane_id.as_ref().map(|id| id.0.clone()));
                    if let Some(last) = last {
                        blocked = Some(serde_json::json!({
                            "pane": last,
                            "reason": "last pane missing",
                        }));
                    }
                }
            }
        }
        // An acknowledged Restore with nothing to resume launches fresh, so it is never "not launchable": clear
        // a key an earlier build wrote.
        sync_meta(journal, &restore_stuck_key(&p.clone), None);
        put_meta_if_changed(
            journal,
            &blocked_key,
            blocked.map(|v| v.to_string()).as_deref(),
        );
    }
}

fn split_chain(o: &mut Out<'_>, journal: &Journal, p: &DesiredPane, deps: Vec<EffectId>) {
    let any = p.clone.to_any();
    let sp = o.mk(
        EffectKind::SplitPane,
        any.clone(),
        p.rev,
        vec![pred(
            any.clone(),
            ContainerKind::Pane,
            EndState::Present,
            false,
        )],
        None,
        deps,
    );
    let st = o.mk(
        EffectKind::StampToken,
        any.clone(),
        p.rev,
        vec![],
        None,
        vec![sp],
    );
    let rn = o.mk(
        EffectKind::RenamePane,
        any.clone(),
        p.rev,
        vec![pred(
            any.clone(),
            ContainerKind::Pane,
            EndState::Renamed {
                name: p.name.clone(),
            },
            false,
        )],
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
        let Some(ts_any) = token_of(&ws.metadata) else {
            continue;
        };
        let Ok(ts) = TeamspaceId::parse(ts_any.as_str()) else {
            continue;
        };
        let Some(ts_rec) = d.teamspaces.get(&ts) else {
            continue;
        };
        // (clone named by the token, its seat, pane id): a pane another clone's committed binding claims
        // is adopted (undo from a bound pane), so the token's clone no longer owns it.
        let graph_panes = |t: &TabInfo| -> Vec<(CloneId, SeatId, HerdrPaneId)> {
            t.panes
                .iter()
                .filter_map(|p| Some((token_of(&p.metadata)?, p.id.clone())))
                .filter_map(|(a, pid)| Some((CloneId::parse(a.as_str()).ok()?, pid)))
                .filter_map(|(c, pid)| d.clones.get(&c).map(|r| (c, r.seat.clone(), pid)))
                .collect()
        };
        if d.workspace(&ts).is_none() {
            // Teamspace dormant or retired: the whole workspace goes; everything inside is induced.
            let mut predicted = vec![pred(
                ts_any.clone(),
                ContainerKind::Workspace,
                EndState::Closed,
                false,
            )];
            let mut seats_done = BTreeSet::new();
            for t in &ws.tabs {
                for (c, s, _) in graph_panes(t) {
                    predicted.push(pred(
                        c.to_any(),
                        ContainerKind::Pane,
                        EndState::Closed,
                        true,
                    ));
                    if seats_done.insert(s.clone()) {
                        predicted.push(pred(
                            s.to_any(),
                            ContainerKind::Tab,
                            EndState::Closed,
                            true,
                        ));
                    }
                }
            }
            o.mk(
                EffectKind::CloseWorkspace,
                ts_any,
                ts_rec.rev,
                predicted,
                None,
                vec![],
            );
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
            let Some((_, seat, _)) = gp.first().cloned() else {
                continue;
            };
            let claimed = |c: &CloneId, pid: &HerdrPaneId| idx.pane_claimed_by(pid, c).is_some();
            let any_claimed = gp.iter().any(|(c, _, pid)| claimed(c, pid));
            // A seat that is merely dormant keeps its active clones: the whole tab goes. A retired seat has only
            // retired clones, which close pane by pane (the last one takes the tab with it, induced).
            let any_active = gp.iter().any(|(c, _, _)| {
                d.clones
                    .get(c)
                    .is_some_and(|r| r.lifecycle == CloneLifecycle::Active)
            });
            if d.tab(&seat).is_none() && any_active && !any_claimed {
                closing.push(Closing {
                    tab: t.clone(),
                    seat,
                    whole_tab: true,
                    clones: gp.into_iter().map(|g| g.0).collect(),
                });
                continue;
            }
            let dead: Vec<CloneId> = gp
                .iter()
                .filter(|(c, _, pid)| d.pane(c).is_none() && !claimed(c, pid))
                .map(|(c, _, _)| c.clone())
                .collect();
            if !dead.is_empty() {
                closing.push(Closing {
                    tab: t.clone(),
                    seat,
                    whole_tab: false,
                    clones: dead,
                });
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
                let mut predicted = vec![pred(
                    c.seat.to_any(),
                    ContainerKind::Tab,
                    EndState::Closed,
                    false,
                )];
                predicted.extend(
                    c.clones
                        .iter()
                        .map(|cl| pred(cl.to_any(), ContainerKind::Pane, EndState::Closed, true)),
                );
                if ws_closes {
                    predicted.push(pred(
                        ts_any.clone(),
                        ContainerKind::Workspace,
                        EndState::Closed,
                        true,
                    ));
                }
                o.mk(
                    EffectKind::CloseTab,
                    c.seat.to_any(),
                    seat_rev,
                    predicted,
                    None,
                    vec![],
                );
            } else {
                for cl in &c.clones {
                    let rev = d.clones.get(cl).map_or(0, |r| r.rev);
                    let mut predicted = vec![pred(
                        cl.to_any(),
                        ContainerKind::Pane,
                        EndState::Closed,
                        false,
                    )];
                    if tab_closes {
                        predicted.push(pred(
                            c.seat.to_any(),
                            ContainerKind::Tab,
                            EndState::Closed,
                            true,
                        ));
                        if ws_closes {
                            predicted.push(pred(
                                ts_any.clone(),
                                ContainerKind::Workspace,
                                EndState::Closed,
                                true,
                            ));
                        }
                    }
                    o.mk(
                        EffectKind::ClosePane,
                        cl.to_any(),
                        rev,
                        predicted,
                        None,
                        vec![],
                    );
                }
            }
        }
    }
}
