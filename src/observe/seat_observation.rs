//! SeatObservation: owns turning seat statuses into clone binding, availability, retirement, the committed
//! per-clone seat_fact and the key-conflict alert level, plus pre-D1 precedence over the old matcher. It reads
//! the seat intent but never writes it; new seat-state handling is added in the single match below; callers
//! never map seat states to graph changes elsewhere.
//!
//! A seat the read omitted (a per-seat refusal) is threads not speaking for that clone: the matcher's result
//! stands. A refusal that outlasts `SEAT_LOST_AFTER` makes the clone Unknown (seat lost). Clones of a Dormant
//! seat or teamspace are not inspected.
//!
//! Pre-D1 precedence: threads speaks only with Resolved, Retired or KeyConflict. Unresolved, an unreachable
//! threads service and a clone without `threads_seat` leave the old matcher's result untouched.
use super::matcher::Matches;
use crate::model::clone::{CloneRecord, SeatFact, SeatFactState, SeatIntent};
use crate::model::common::CloneLifecycle;
use crate::model::{CloneId, HerdrPaneId, Timestamp};
use crate::ports::threads::{RetirementKind, SeatStatus, ThreadsSeatRef};
use crate::reconcile::desired::DesiredRuntime;
use std::collections::{BTreeMap, BTreeSet};

/// How long a seat may stay refused before the clone is Unknown with a seat-lost alert. Equal to the Herdr
/// incarnation grace.
pub const SEAT_LOST_AFTER: chrono::Duration = chrono::Duration::seconds(90);

/// The remedy shown with a lost seat: threads refused it for good, so the graph never re-resolves it silently.
pub const SEAT_LOST_REMEDY: &str =
    "retire and purge the clone; a lost seat is never re-resolved silently";

/// The outcome of this step's threads read.
pub enum ThreadsRead {
    Statuses(BTreeMap<ThreadsSeatRef, SeatStatus>),
    /// Threads could not be read: no state is invented, nothing changes.
    Unavailable,
}

#[derive(Debug, Default)]
pub struct SeatOutcome {
    /// Clones whose seat threads retired; retired through the observed pane-closure cascade.
    pub retire: Vec<CloneId>,
    /// One fact per active clone with a `threads_seat`, from a successful read.
    pub facts: Vec<(CloneId, SeatFact)>,
    /// Clones threads took out of the matcher's result this step (retired, key-conflicted or displaced from a
    /// pane threads gave to another clone). They are Unknown, never "gone": the step also removes them from
    /// the baseline matches so the diff cannot report their pane as closed.
    pub unbound: Vec<CloneId>,
    /// Seated clones whose occupant is stale after an incarnation change; their occupancy ends this step.
    pub ended: Vec<CloneId>,
}

impl SeatOutcome {
    /// Unbound clones that are not being retired: marked Unknown.
    pub fn unknown(&self) -> impl Iterator<Item = &CloneId> {
        self.unbound.iter().filter(|c| !self.retire.contains(c))
    }
}

/// Active clones that carry a threads seat and have a desired pane: the ones this module speaks for. A clone of
/// a Dormant seat or teamspace has no desired pane and is not inspected, so neither its refusal clock nor a
/// Retired answer caused by the graph's own deactivation close can unbind or retire it; reactivation writes a
/// Restore intent (`SeatActivate`), which the Retired arm honours.
pub fn seated_clones(d: &DesiredRuntime) -> Vec<(CloneId, ThreadsSeatRef)> {
    d.clones
        .values()
        .filter(|c| c.lifecycle == CloneLifecycle::Active && !c.is_tombstone())
        .filter(|c| d.pane(&c.id).is_some())
        .filter_map(|c| {
            c.threads_seat
                .as_ref()
                .map(|s| (c.id.clone(), ThreadsSeatRef(s.clone())))
        })
        .collect()
}

/// Whether the committed fact must be rewritten: its state or seat differs, or it predates the last Herdr
/// incarnation change (so every clone has a fresh fact after a restart).
pub fn fact_needs_write(
    rec: &CloneRecord,
    fact: &SeatFact,
    incarnation_changed_at: Option<Timestamp>,
) -> bool {
    match &rec.seat_fact {
        None => true,
        Some(old) => {
            old.seat != fact.seat
                || old.state != fact.state
                || incarnation_changed_at.is_some_and(|at| old.read_at < at)
        }
    }
}

fn fact_state(status: &SeatStatus) -> SeatFactState {
    match status {
        SeatStatus::Resolved { pane } => SeatFactState::Resolved { pane: pane.clone() },
        SeatStatus::Unresolved => SeatFactState::Unresolved,
        SeatStatus::Retired { kind } => SeatFactState::Retired {
            reversible: *kind == RetirementKind::Reversible,
        },
        SeatStatus::KeyConflict { pane } => SeatFactState::KeyConflict { pane: pane.clone() },
    }
}

/// Drop `clone`'s pane from the matcher result; it then has no binding this step (Unknown).
fn unbind(m: &mut Matches, out: &mut SeatOutcome, clone: &CloneId) {
    if let Some(pane) = m.clone_pane.remove(clone)
        && m.panes.get(&pane) == Some(clone)
    {
        m.panes.remove(&pane);
    }
    if !out.unbound.contains(clone) {
        out.unbound.push(clone.clone());
    }
}

/// Single match on SeatStatus. Mutates `m` for the clones threads speaks for. `live` is the set of panes in
/// the snapshot: a Resolved pane that is not there yet cannot be bound, so the matcher's result stands.
/// `lost` holds the seats refused for longer than `SEAT_LOST_AFTER`: their clones are Unknown, never retired.
/// `bare` is the set of snapshot panes with no agent; `changed_at` is when the Herdr incarnation last changed.
#[allow(clippy::too_many_arguments)]
pub fn observe(
    d: &DesiredRuntime,
    read: &ThreadsRead,
    live: &BTreeSet<HerdrPaneId>,
    bare: &BTreeSet<HerdrPaneId>,
    changed_at: Option<Timestamp>,
    lost: &BTreeSet<ThreadsSeatRef>,
    m: &mut Matches,
    now: Timestamp,
) -> SeatOutcome {
    let mut out = SeatOutcome::default();
    let ThreadsRead::Statuses(statuses) = read else {
        return out;
    };
    let mut resolved: Vec<(CloneId, HerdrPaneId)> = Vec::new();
    let mut unresolved: Vec<CloneId> = Vec::new();
    for (clone, seat) in seated_clones(d) {
        let Some(status) = statuses.get(&seat) else {
            if lost.contains(&seat) {
                unbind(m, &mut out, &clone);
            }
            continue;
        };
        out.facts.push((
            clone.clone(),
            SeatFact {
                seat: seat.0.clone(),
                state: fact_state(status),
                read_at: now,
            },
        ));
        match status {
            SeatStatus::Resolved { pane } => {
                if live.contains(pane) {
                    resolved.push((clone, pane.clone()));
                }
            }
            SeatStatus::Unresolved => unresolved.push(clone),
            SeatStatus::Retired { .. } => {
                let pending_restore = matches!(
                    d.clones.get(&clone).and_then(|c| c.seat_intent.as_ref()),
                    Some(SeatIntent::Restore {
                        threads_done: false,
                        ..
                    })
                );
                if !pending_restore {
                    unbind(m, &mut out, &clone);
                    out.retire.push(clone);
                }
            }
            SeatStatus::KeyConflict { .. } => unbind(m, &mut out, &clone),
        }
    }
    for (clone, pane) in resolved {
        // Pane uniqueness: threads' claim wins; any other clone holding the pane has no binding this step.
        if let Some(other) = m.panes.get(&pane).cloned()
            && other != clone
        {
            unbind(m, &mut out, &other);
        }
        if let Some(old) = m.clone_pane.get(&clone).cloned()
            && old != pane
        {
            m.panes.remove(&old);
        }
        m.panes.insert(pane.clone(), clone.clone());
        m.clone_pane.insert(clone.clone(), pane);
        // An earlier answer may have displaced this clone; it is bound again now.
        out.unbound.retain(|c| c != &clone);
    }
    for clone in unresolved {
        let Some(rec) = d.clones.get(&clone) else {
            continue;
        };
        let stale = rec
            .occupant
            .as_ref()
            .zip(changed_at)
            .is_some_and(|(o, at)| o.since < at);
        let last_pane = rec.runtime.bound.as_ref().and_then(|b| b.pane_id.as_ref());
        if stale
            && !m.clone_pane.contains_key(&clone)
            && last_pane.is_some_and(|p| bare.contains(p) && !m.panes.contains_key(p))
        {
            out.ended.push(clone);
        }
    }
    out
}
