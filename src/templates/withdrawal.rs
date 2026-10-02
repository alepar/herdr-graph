//! Withdrawal of an application's contributions (spec §5, DESIGN-NOTES "Composition withdrawal"): which
//! seats are exclusive to the application (retired, mechanism `application_withdrawal`) and which are kept
//! because they are shared, pre-existing or independent. Used by application retire, live template edits
//! that remove a member, and (Task 10) hydration undo.
use crate::model::application::ApplicationRecord;
use crate::model::clone::CloneRecord;
use crate::model::common::{AppLifecycle, CloneLifecycle, Lifecycle};
use crate::model::seat::SeatRecord;
use crate::model::template::Relationship;
use crate::model::SeatId;
use crate::ports::store::StoreError;
use crate::store::layout;
use crate::templates::structure::read_seat_rec;
use crate::store::tree::TreeRead;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct WithdrawalPreview {
    /// Exclusive seats: retired with mechanism `application_withdrawal`.
    pub retire: Vec<SeatId>,
    /// Seats that stay, each with the reason.
    pub keep: Vec<(SeatId, String)>,
    /// Relationship contributions that end because no other live application contributes the same thread.
    pub relationships_removed: Vec<Relationship>,
    /// Set when an exclusive seat cannot be retired unambiguously; the plan must not be applied.
    pub repair_required: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Retire,
    Keep(String),
}

/// Whether `app` currently maps, adds or reuses `seat`.
pub fn references(app: &ApplicationRecord, seat: &SeatId) -> bool {
    app.member_map.values().any(|s| s == seat) || app.additions.contains(seat) || app.reused.iter().any(|r| &r.seat == seat)
}

/// Every seat `app` currently has, in a stable order, without duplicates.
pub fn membership(app: &ApplicationRecord) -> Vec<SeatId> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let all = app.member_map.values().chain(app.additions.iter()).chain(app.reused.iter().map(|r| &r.seat));
    for s in all {
        if seen.insert(s.clone()) {
            out.push(s.clone());
        }
    }
    out
}

/// Live applications other than `app`.
pub fn live_others(tree: &dyn TreeRead, app: &ApplicationRecord) -> Result<Vec<ApplicationRecord>, StoreError> {
    Ok(layout::list_applications(tree)?
        .into_iter()
        .map(|(_, a)| a)
        .filter(|a| a.id != app.id && a.lifecycle == AppLifecycle::Active)
        .collect())
}

/// Exclusive or kept, for one seat of `app` (`others` = the other live applications).
pub fn classify(app: &ApplicationRecord, others: &[ApplicationRecord], seat: &SeatId) -> Verdict {
    if app.reused.iter().any(|r| &r.seat == seat && r.from.is_none()) {
        return Verdict::Keep("pre-existing independent seat".into());
    }
    if app.additions.contains(seat) {
        return Verdict::Keep("added to the application independently".into());
    }
    let users: Vec<&ApplicationRecord> = others.iter().filter(|o| references(o, seat)).collect();
    if users.is_empty() {
        return Verdict::Retire;
    }
    let names = |v: &[&ApplicationRecord]| v.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", ");
    let borrowed_from: Vec<&ApplicationRecord> = users
        .iter()
        .copied()
        .filter(|o| app.reused.iter().any(|r| &r.seat == seat && r.from.as_ref() == Some(&o.id)))
        .collect();
    if borrowed_from.is_empty() {
        Verdict::Keep(format!("reused by {}", names(&users)))
    } else {
        Verdict::Keep(format!("reused from {}", names(&borrowed_from)))
    }
}

/// Why retiring `seat` on behalf of `app` would be ambiguous, if it would.
pub fn ambiguity(
    tree: &dyn TreeRead,
    app: &ApplicationRecord,
    seat: &SeatRecord,
    clones: &[CloneRecord],
    others: &[ApplicationRecord],
) -> Result<Option<String>, StoreError> {
    for other in layout::list_applications(tree)?.into_iter().map(|(_, a)| a).filter(|a| a.id != app.id) {
        let id = other.id.as_str();
        if seat.overrides.instructions_sections.iter().any(|s| s.name.contains(id) || s.body.contains(id)) {
            return Ok(Some(format!(
                "seat {} ({}) has instruction sections that name application {} ({id}); retiring it for {} is ambiguous",
                seat.name, seat.id, other.name, app.name
            )));
        }
    }
    let occupied = clones.iter().any(|c| c.lifecycle == CloneLifecycle::Active && c.occupant.is_some());
    if occupied {
        for thread in &seat.participation.seat_wide {
            if let Some(o) = others.iter().find(|o| o.contributions.relationships.iter().any(|r| &r.thread == thread)) {
                return Ok(Some(format!(
                    "occupied seat {} ({}) participates seat-wide in thread {thread:?}, which application {} also contributes; \
                     withdrawing {} is ambiguous",
                    seat.name, seat.id, o.name, app.name
                )));
            }
        }
    }
    Ok(None)
}

/// What withdrawing all of `app`'s contributions does, over its CURRENT membership (seats created by later
/// template edits included). Already-retired seats are left out.
pub fn withdraw_plan(tree: &dyn TreeRead, app: &ApplicationRecord) -> Result<WithdrawalPreview, StoreError> {
    let others = live_others(tree, app)?;
    let mut retire = Vec::new();
    let mut keep = Vec::new();
    let mut repair_required = None;
    for id in membership(app) {
        let Some((loc, seat)) = read_seat_rec(tree, &id)? else { continue };
        if seat.lifecycle == Lifecycle::Retired {
            continue;
        }
        match classify(app, &others, &id) {
            Verdict::Keep(why) => keep.push((id, why)),
            Verdict::Retire => {
                if repair_required.is_none() {
                    let clones: Vec<CloneRecord> =
                        layout::list_clones(tree, &loc.folder)?.into_iter().map(|(_, c)| c).collect();
                    repair_required = ambiguity(tree, app, &seat, &clones, &others)?;
                }
                retire.push(id);
            }
        }
    }
    let relationships_removed = app
        .contributions
        .relationships
        .iter()
        .filter(|r| !others.iter().any(|o| o.contributions.relationships.iter().any(|x| x.thread == r.thread)))
        .cloned()
        .collect();
    Ok(WithdrawalPreview { retire, keep, relationships_removed, repair_required })
}
