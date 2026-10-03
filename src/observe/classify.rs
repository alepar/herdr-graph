//! Classification of diff elements against committed intent and journaled effects' predicted end states
//! (spec §4.3.4–4.3.5), and containment grouping of unexplained disappearances into cascades.
//!
//! No wall-clock window: an element is explained when the committed desired state no longer wants the
//! object, or when an unconsumed prediction of a journaled effect says its container closes.
use super::diff::DiffElement;
use super::matcher::Matches;
use super::sessions::SessionCapture;
use crate::journal::Journal;
use crate::model::clone::CloneRecord;
use crate::model::common::{CloneLifecycle, Lifecycle};
use crate::model::effect::{ContainerKind, EffectKind, EffectStatus, EndState, PredictedEnd};
use crate::model::harness::Harness;
use crate::model::native_session::SessionEndReason;
use crate::model::{AnyId, CloneId, EffectId, HerdrTabId, NsId, SeatId, TeamspaceId};
use crate::reconcile::desired::DesiredRuntime;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// An occupancy that is ending (for the session-ended hook and the cascade/occupancy mutations).
#[derive(Debug, Clone, PartialEq)]
pub struct EndedSession {
    pub clone: CloneId,
    pub seat: SeatId,
    pub ns: NsId,
    pub harness: Harness,
    pub transcript_path: Option<PathBuf>,
    pub reason: SessionEndReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CascadeRule {
    Pane,
    Tab,
    Workspace,
}

impl CascadeRule {
    pub fn as_str(self) -> &'static str {
        match self {
            CascadeRule::Pane => "pane",
            CascadeRule::Tab => "tab",
            CascadeRule::Workspace => "workspace",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cascade {
    pub rule: CascadeRule,
    /// Non-retired objects at the committed head the cascade retires, root first.
    pub retire: Vec<AnyId>,
    pub ends: Vec<EndedSession>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoveFact {
    pub clone: CloneId,
    pub to_tab: HerdrTabId,
    /// The destination is a tab graph knows (a seat's tab); otherwise the clone becomes unbound.
    pub known: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OccChange {
    pub clone: CloneId,
    pub end: Option<EndedSession>,
    pub start: Option<SessionCapture>,
}

#[derive(Debug, Default)]
pub struct Classified {
    pub cascades: Vec<Cascade>,
    /// Explained closures: runtime becomes absent (bookkeeping only).
    pub gone: Vec<AnyId>,
    pub gone_ends: Vec<EndedSession>,
    pub renames: Vec<(AnyId, String)>,
    pub moves: Vec<MoveFact>,
    pub moved_out: BTreeSet<SeatId>,
    pub occupancy: Vec<OccChange>,
    /// Effects whose prediction explained an element in this diff (consumed once the observation commits).
    pub consumed: BTreeSet<EffectId>,
}

impl Classified {
    pub fn retired_now(&self) -> BTreeSet<AnyId> {
        self.cascades
            .iter()
            .flat_map(|c| c.retire.iter().cloned())
            .collect()
    }
}

pub struct ClassifyCx<'a> {
    pub desired: &'a DesiredRuntime,
    pub journal: &'a Journal,
    /// Unconsumed predictions, read once before classification.
    pub preds: &'a [(EffectId, PredictedEnd)],
    pub matches: &'a Matches,
    /// Baseline matches in a diff pass; `None` in a rebind pass.
    pub baseline: Option<&'a Matches>,
}

pub fn ended_of(c: &CloneRecord, reason: SessionEndReason) -> Option<EndedSession> {
    let occ = c.occupant.as_ref()?;
    let ns = c.sessions.iter().find(|s| s.id == occ.native_session);
    Some(EndedSession {
        clone: c.id.clone(),
        seat: c.seat.clone(),
        ns: occ.native_session.clone(),
        harness: occ.harness,
        transcript_path: ns.and_then(|s| s.transcript_path.clone()),
        reason,
    })
}

fn intent_explains(d: &DesiredRuntime, object: &AnyId, container: ContainerKind) -> bool {
    match container {
        ContainerKind::Pane => CloneId::parse(object.as_str()).is_ok_and(|c| d.pane(&c).is_none()),
        ContainerKind::Tab => SeatId::parse(object.as_str()).is_ok_and(|s| d.tab(&s).is_none()),
        ContainerKind::Workspace => {
            TeamspaceId::parse(object.as_str()).is_ok_and(|t| d.workspace(&t).is_none())
        }
    }
}

fn predicted(
    preds: &[(EffectId, PredictedEnd)],
    object: &AnyId,
    container: ContainerKind,
    want: impl Fn(&EndState) -> bool,
) -> Option<EffectId> {
    preds
        .iter()
        .find(|(_, p)| &p.object == object && p.container == container && want(&p.end))
        .map(|(e, _)| e.clone())
}

fn is_retired(d: &DesiredRuntime, object: &AnyId) -> bool {
    if let Ok(c) = CloneId::parse(object.as_str()) {
        return d
            .clones
            .get(&c)
            .is_none_or(|r| r.lifecycle == CloneLifecycle::Retired);
    }
    if let Ok(s) = SeatId::parse(object.as_str()) {
        return d
            .seats
            .get(&s)
            .is_none_or(|r| r.lifecycle == Lifecycle::Retired);
    }
    if let Ok(t) = TeamspaceId::parse(object.as_str()) {
        return d
            .teamspaces
            .get(&t)
            .is_none_or(|r| r.lifecycle == Lifecycle::Retired);
    }
    true
}

fn rename_container(object: &AnyId) -> ContainerKind {
    match object.kind() {
        crate::model::IdKind::Teamspace => ContainerKind::Workspace,
        crate::model::IdKind::Seat => ContainerKind::Tab,
        _ => ContainerKind::Pane,
    }
}

fn has_open_rename(journal: &Journal, object: &AnyId) -> bool {
    journal
        .effects_for_object(object)
        .unwrap_or_default()
        .iter()
        .any(|r| {
            matches!(
                r.kind,
                EffectKind::RenameWorkspace | EffectKind::RenameTab | EffectKind::RenamePane
            ) && matches!(r.status, EffectStatus::Pending | EffectStatus::Unknown)
        })
}

/// Non-retired clones of `seat`.
fn live_clones<'a>(
    d: &'a DesiredRuntime,
    seat: &'a SeatId,
) -> impl Iterator<Item = &'a CloneRecord> + use<'a> {
    d.clones
        .values()
        .filter(move |c| &c.seat == seat && c.lifecycle != CloneLifecycle::Retired)
}

fn seat_group(d: &DesiredRuntime, seat: &SeatId, rule: CascadeRule, root: bool) -> Cascade {
    let mut retire = Vec::new();
    let mut ends = Vec::new();
    if root
        && d.seats
            .get(seat)
            .is_some_and(|s| s.lifecycle != Lifecycle::Retired)
    {
        retire.push(seat.to_any());
    }
    for c in live_clones(d, seat) {
        retire.push(c.id.to_any());
        ends.extend(ended_of(c, SessionEndReason::PaneClosed));
    }
    Cascade { rule, retire, ends }
}

pub fn classify(cx: &ClassifyCx<'_>, elements: Vec<DiffElement>) -> Classified {
    let d = cx.desired;
    let mut out = Classified::default();
    let mut ws_gone: BTreeSet<TeamspaceId> = BTreeSet::new();
    let mut tab_gone: BTreeSet<SeatId> = BTreeSet::new();
    let mut pane_gone: BTreeSet<CloneId> = BTreeSet::new();
    let mut renamed: Vec<(AnyId, String)> = Vec::new();
    let mut moved: Vec<(CloneId, HerdrTabId)> = Vec::new();
    let mut ends: BTreeMap<CloneId, SessionEndReason> = BTreeMap::new();
    let mut starts: BTreeMap<CloneId, SessionCapture> = BTreeMap::new();

    for el in elements {
        match el {
            DiffElement::Disappeared { object, container } => {
                let by_pred = predicted(cx.preds, &object, container, |e| *e == EndState::Closed);
                if intent_explains(d, &object, container) || by_pred.is_some() {
                    out.consumed.extend(by_pred);
                    out.gone.push(object);
                    continue;
                }
                match container {
                    ContainerKind::Workspace => {
                        ws_gone.extend(TeamspaceId::parse(object.as_str()));
                    }
                    ContainerKind::Tab => {
                        tab_gone.extend(SeatId::parse(object.as_str()));
                    }
                    ContainerKind::Pane => {
                        pane_gone.extend(CloneId::parse(object.as_str()));
                    }
                }
            }
            DiffElement::Renamed { object, new, .. } => renamed.push((object, new)),
            DiffElement::Moved { clone, to_tab, .. } => moved.push((clone, to_tab)),
            DiffElement::OccupancyEnded { clone, reason, .. } => {
                ends.insert(clone, reason);
            }
            DiffElement::OccupancyStarted { clone, capture } => {
                starts.insert(clone, capture);
            }
        }
    }

    // Explained closures end their clones' occupancy: the session is over even though nothing is retired.
    for obj in &out.gone {
        if let Ok(c) = CloneId::parse(obj.as_str())
            && let Some(rec) = d.clones.get(&c)
        {
            out.gone_ends
                .extend(ended_of(rec, SessionEndReason::PaneClosed));
        }
    }

    // Containment grouping: workspace > tab > pane.
    let seat_ts = |s: &SeatId| d.seats.get(s).map(|r| r.teamspace.clone());
    for ts in &ws_gone {
        let mut retire = Vec::new();
        let mut group_ends = Vec::new();
        if d.teamspaces
            .get(ts)
            .is_some_and(|t| t.lifecycle != Lifecycle::Retired)
        {
            retire.push(ts.to_any());
        }
        for seat in d
            .seats
            .values()
            .filter(|s| &s.teamspace == ts && s.lifecycle != Lifecycle::Retired)
        {
            let g = seat_group(d, &seat.id, CascadeRule::Workspace, true);
            retire.extend(g.retire);
            group_ends.extend(g.ends);
        }
        if !retire.is_empty() {
            out.cascades.push(Cascade {
                rule: CascadeRule::Workspace,
                retire,
                ends: group_ends,
            });
        }
    }
    let mut absorbed_seats: BTreeSet<SeatId> = BTreeSet::new();
    for seat in &tab_gone {
        if seat_ts(seat).is_some_and(|t| ws_gone.contains(&t)) {
            absorbed_seats.insert(seat.clone());
            continue;
        }
        // A pane that left the tab alive is a move, not a closure: the seat stays, flagged moved_out.
        let survivors = match cx.baseline {
            Some(mb) => live_clones(d, seat)
                .filter(|c| {
                    mb.clone_pane.contains_key(&c.id) && cx.matches.clone_pane.contains_key(&c.id)
                })
                .count(),
            None => 0,
        };
        if survivors > 0 {
            out.moved_out.insert(seat.clone());
            continue;
        }
        absorbed_seats.insert(seat.clone());
        let g = seat_group(d, seat, CascadeRule::Tab, true);
        if !g.retire.is_empty() {
            out.cascades.push(g);
        }
    }
    for clone in &pane_gone {
        let Some(rec) = d.clones.get(clone) else {
            continue;
        };
        if absorbed_seats.contains(&rec.seat)
            || seat_ts(&rec.seat).is_some_and(|t| ws_gone.contains(&t))
        {
            continue;
        }
        if rec.lifecycle == CloneLifecycle::Retired {
            continue;
        }
        out.cascades.push(Cascade {
            rule: CascadeRule::Pane,
            retire: vec![clone.to_any()],
            ends: ended_of(rec, SessionEndReason::PaneClosed)
                .into_iter()
                .collect(),
        });
    }
    let retired_now = out.retired_now();

    // Renames: explained by a predicted end state, by an in-flight rename (rebind only), or moot.
    let mut seen: BTreeSet<AnyId> = BTreeSet::new();
    for (object, new) in renamed {
        if is_retired(d, &object) || retired_now.contains(&object) || !seen.insert(object.clone()) {
            continue;
        }
        let container = rename_container(&object);
        if let Some(ef) = predicted(
            cx.preds,
            &object,
            container,
            |e| matches!(e, EndState::Renamed { name } if *name == new),
        ) {
            out.consumed.insert(ef);
            continue;
        }
        if cx.baseline.is_none() && has_open_rename(cx.journal, &object) {
            continue;
        }
        out.renames.push((object, new));
    }

    for (clone, to_tab) in moved {
        if retired_now.contains(&clone.to_any()) {
            continue;
        }
        let known = cx.matches.tabs.contains_key(&to_tab);
        out.moves.push(MoveFact {
            clone,
            to_tab,
            known,
        });
    }

    let mut occ_clones: BTreeSet<CloneId> = ends.keys().chain(starts.keys()).cloned().collect();
    occ_clones.retain(|c| !retired_now.contains(&c.to_any()));
    for clone in occ_clones {
        let end = ends
            .get(&clone)
            .and_then(|reason| d.clones.get(&clone).and_then(|c| ended_of(c, *reason)));
        let start = starts.remove(&clone);
        if end.is_some() || start.is_some() {
            out.occupancy.push(OccChange { clone, end, start });
        }
    }
    out
}
