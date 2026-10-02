//! Pure preview computation for undo (spec §6): what an undo restores, which pane it adopts, which template
//! fields it patches back, and why it cannot run. `UndoKind::plan` assembles these; `UndoKind::mutate`
//! recomputes the same facts against the writer's overlay.
use super::adopt::{Adoption, adoption};
use crate::model::action::{ActionKind, ActionRecord};
use crate::model::application::ApplicationRecord;
use crate::model::clone::CloneRecord;
use crate::model::common::{AppLifecycle, CloneLifecycle, Lifecycle, Retirement};
use crate::model::seat::SeatRecord;
use crate::model::{AnyId, AppId, CloneId, IdKind, SeatId, TeamspaceId, TemplateId};
use crate::plan::core_kinds::{open_clone_effects, read_seat, read_ts, rev_of};
use crate::plan::kind::{PlanBody, PlanCx, PlanError};
use crate::plan::types::{PlanEffect, Reserved};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::TreeRead;
use crate::templates::document::{DocumentMember, TemplateDocument};
use crate::templates::structure::read_template;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const CLOSES_PANE_WARNING: &str = "this undo closes your pane; the op id is printed before it closes";

fn invalid(m: impl Into<String>) -> PlanError {
    PlanError::Invalid(m.into())
}

/// The lifecycle `id` had before `act` ran (`affected.before.lifecycle`).
pub fn before_state(act: &ActionRecord, id: &AnyId) -> Option<String> {
    act.affected
        .iter()
        .find(|a| &a.object == id)
        .and_then(|a| a.before.as_ref())
        .and_then(|b| b.get("lifecycle"))
        .and_then(|l| l.as_str())
        .map(str::to_owned)
}

fn retired_by_act(r: &Option<Retirement>, act: &ActionRecord) -> bool {
    r.as_ref().and_then(|x| x.action.as_ref()) == Some(&act.id)
}

pub(crate) fn read_clone(tree: &dyn TreeRead, id: &CloneId) -> Result<Option<CloneRecord>, PlanError> {
    let Some(loc) = layout::locate(tree, &id.to_any())? else { return Ok(None) };
    Ok(read_toml::<CloneRecord>(tree, &loc.record_path)?)
}

pub(crate) fn read_application(tree: &dyn TreeRead, id: &AppId) -> Result<Option<ApplicationRecord>, PlanError> {
    let Some(loc) = layout::locate(tree, &id.to_any())? else { return Ok(None) };
    Ok(read_toml::<ApplicationRecord>(tree, &loc.record_path)?)
}

// ---------------------------------------------------------------------------------------------
// closure cascades, retirements, application retirements: resurrect exactly what the action retired
// ---------------------------------------------------------------------------------------------

/// What an undo of a retiring action brings back: exactly the ids in the action's `retired` that are still
/// retired by that action (never `already_retired`, never objects another action retired since).
#[derive(Debug, Default)]
pub struct Restore {
    /// `(teamspace, active)`: teamspaces come back in the lifecycle they had before.
    pub teamspaces: Vec<(TeamspaceId, bool)>,
    /// `(seat, active)`: a seat that was dormant before stays dormant.
    pub seats: Vec<(SeatId, bool)>,
    /// Every clone coming back (through its seat or on its own).
    pub clones: Vec<CloneId>,
    pub app: Option<AppId>,
    /// `retired` ids this undo restores, for the seat-resurrection helper.
    pub set: BTreeSet<AnyId>,
    /// Active seats with no clone to bring back get one new clone `(seat, clone)`.
    pub new_clones: Vec<(SeatId, CloneId)>,
    pub warnings: Vec<String>,
    pub repair: Option<String>,
}

impl Restore {
    /// Clones that get a runtime (a pane): those of seats that come back active.
    pub fn runtime_clones(&self, tree: &dyn TreeRead) -> Result<Vec<CloneId>, PlanError> {
        let mut out = Vec::new();
        for (seat, active) in &self.seats {
            if !*active {
                continue;
            }
            let f = read_seat(tree, seat)?;
            for (_, c) in &f.clones {
                if self.clones.contains(&c.id) {
                    out.push(c.id.clone());
                }
            }
            out.extend(self.new_clones.iter().filter(|(s, _)| s == seat).map(|(_, c)| c.clone()));
        }
        for c in &self.clones {
            if out.contains(c) {
                continue;
            }
            if let Some(rec) = read_clone(tree, c)?
                && !self.seats.iter().any(|(s, _)| s == &rec.seat)
                && read_seat(tree, &rec.seat)?.rec.lifecycle == Lifecycle::Active
            {
                out.push(c.clone());
            }
        }
        Ok(out)
    }
}

pub fn compute_restore(tree: &dyn TreeRead, act: &ActionRecord, reserved: &mut Reserved) -> Result<Restore, PlanError> {
    let mut r = Restore::default();
    let mut standalone: Vec<CloneId> = Vec::new();
    for id in &act.retired {
        match id.kind() {
            IdKind::Teamspace => {
                let ts_id = TeamspaceId::parse(id.as_str()).map_err(|e| invalid(e.to_string()))?;
                match read_ts(tree, &ts_id) {
                    Ok(ts) if ts.rec.lifecycle == Lifecycle::Retired && retired_by_act(&ts.rec.retired, act) => {
                        r.teamspaces.push((ts_id, before_state(act, id).as_deref() == Some("active")));
                        r.set.insert(id.clone());
                    }
                    _ => r.warnings.push(format!("teamspace {id} is no longer retired by this action; left alone")),
                }
            }
            IdKind::Seat => {
                let seat_id = SeatId::parse(id.as_str()).map_err(|e| invalid(e.to_string()))?;
                let Ok(f) = read_seat(tree, &seat_id) else {
                    r.warnings.push(format!("seat {id} no longer exists; left alone"));
                    continue;
                };
                if f.rec.lifecycle != Lifecycle::Retired || !retired_by_act(&f.rec.retired, act) {
                    r.warnings.push(format!("seat {id} is no longer retired by this action; left alone"));
                    continue;
                }
                r.seats.push((seat_id, before_state(act, id).as_deref() == Some("active")));
                r.set.insert(id.clone());
                for (_, c) in &f.clones {
                    if c.lifecycle == CloneLifecycle::Retired
                        && retired_by_act(&c.retired, act)
                        && act.retired.contains(&c.id.to_any())
                    {
                        r.clones.push(c.id.clone());
                        r.set.insert(c.id.to_any());
                    }
                }
            }
            IdKind::Clone => {
                if let Ok(c) = CloneId::parse(id.as_str()) {
                    standalone.push(c);
                }
            }
            IdKind::Application => {
                let app_id = AppId::parse(id.as_str()).map_err(|e| invalid(e.to_string()))?;
                match read_application(tree, &app_id)? {
                    Some(a) if a.lifecycle == AppLifecycle::Retired && retired_by_act(&a.retired, act) => {
                        r.app = Some(app_id);
                        r.set.insert(id.clone());
                    }
                    _ => r.warnings.push(format!("application {id} is no longer retired by this action; left alone")),
                }
            }
            _ => {}
        }
    }
    for c in standalone {
        if r.clones.contains(&c) {
            continue;
        }
        let Some(rec) = read_clone(tree, &c)? else {
            r.warnings.push(format!("clone {c} no longer exists; left alone"));
            continue;
        };
        if rec.lifecycle != CloneLifecycle::Retired || !retired_by_act(&rec.retired, act) {
            r.warnings.push(format!("clone {c} is no longer retired by this action; left alone"));
            continue;
        }
        let seat = read_seat(tree, &rec.seat)?;
        if seat.rec.lifecycle == Lifecycle::Retired {
            r.warnings.push(format!("clone {c} belongs to retired seat {}; left alone", rec.seat));
            continue;
        }
        r.set.insert(c.to_any());
        r.clones.push(c);
    }
    // Seats whose teamspace stays retired cannot come back.
    for (seat, _) in &r.seats {
        let f = read_seat(tree, seat)?;
        if f.ts.rec.lifecycle == Lifecycle::Retired && !r.teamspaces.iter().any(|(t, _)| t == &f.ts.rec.id) {
            r.repair.get_or_insert_with(|| {
                format!(
                    "seat {} ({}) belongs to teamspace {} ({}) which is retired; undo the action that retired the \
                     teamspace first",
                    f.rec.name, f.rec.id, f.ts.rec.name, f.ts.rec.id
                )
            });
        }
    }
    // An active seat that gets no clone back still needs one.
    for (seat, active) in r.seats.clone() {
        if !active {
            continue;
        }
        let f = read_seat(tree, &seat)?;
        let has_clone = f.clones.iter().any(|(_, c)| r.clones.contains(&c.id));
        if !has_clone {
            let id: CloneId = reserved.get_or_mint(&format!("clone:{seat}"));
            r.new_clones.push((seat, id));
        }
    }
    if r.set.is_empty() && r.repair.is_none() {
        return Err(invalid(format!(
            "nothing to restore: everything action {} retired has been restored or retired again since",
            act.id
        )));
    }
    Ok(r)
}


/// The warning for an undo that closes the caller's own pane: the pane is bound to a clone (or its seat's tab)
/// that `effects` retire or close.
pub fn closes_caller(tree: &dyn TreeRead, caller_pane: &str, effects: &[PlanEffect]) -> Result<Option<String>, PlanError> {
    for (_, c) in layout::all_clones(tree)? {
        let bound_here = c.runtime.bound.as_ref().and_then(|b| b.pane_id.as_ref()).is_some_and(|p| p.0 == caller_pane);
        if c.lifecycle != CloneLifecycle::Active || !bound_here {
            continue;
        }
        let (clone, seat) = (c.id.to_any(), c.seat.to_any());
        let closes = effects.iter().any(|e| {
            (matches!(e.kind.as_str(), "clone.retire" | "runtime.close_pane") && e.object == clone)
                || (matches!(e.kind.as_str(), "seat.retire" | "runtime.close_tab") && e.object == seat)
        });
        if closes {
            return Ok(Some(CLOSES_PANE_WARNING.to_owned()));
        }
    }
    Ok(None)
}

/// Preview of undoing a closure cascade, a retirement or an application retirement.
pub fn restore_preview(
    cx: &PlanCx<'_>,
    act: &ActionRecord,
    caller_pane: Option<&str>,
    reserved: &mut Reserved,
) -> Result<PlanBody, PlanError> {
    let tree = cx.tree;
    let r = compute_restore(tree, act, reserved)?;
    let mut effects = Vec::new();
    let mut relied_on = Vec::new();
    let mut warnings = r.warnings.clone();
    let runtime = r.runtime_clones(tree)?;
    let adopt: Option<Adoption> = match caller_pane {
        Some(p) => adoption(tree, p, &runtime)?,
        None => None,
    };
    let mut repair = r.repair.clone();

    for (ts, active) in &r.teamspaces {
        let t = read_ts(tree, ts)?;
        relied_on.push(rev_of(ts.clone(), t.rec.rev));
        effects.push(PlanEffect::new(
            "teamspace.resurrect",
            ts.clone(),
            json!({ "name": t.rec.name, "to": if *active { "active" } else { "dormant" } }),
        ));
        if *active {
            effects.push(PlanEffect::new("runtime.open_workspace", ts.clone(), json!({ "name": t.rec.name })));
        }
    }
    let mut activated = BTreeSet::new();
    for (seat, active) in &r.seats {
        let f = read_seat(tree, seat)?;
        relied_on.push(rev_of(seat.clone(), f.rec.rev));
        effects.push(PlanEffect::new(
            "seat.resurrect",
            seat.clone(),
            json!({ "name": f.rec.name, "to": if *active { "active" } else { "dormant" } }),
        ));
        for (_, c) in &f.clones {
            if r.clones.contains(&c.id) {
                effects.push(PlanEffect::new("clone.resurrect", c.id.clone(), json!({ "seat": seat, "name": c.name })));
            }
        }
        for (_, c) in r.new_clones.iter().filter(|(s, _)| s == seat) {
            effects.push(PlanEffect::new("clone.add", c.clone(), json!({ "seat": seat, "name": f.rec.name })));
        }
        if *active {
            if f.ts.rec.lifecycle == Lifecycle::Dormant
                && !r.teamspaces.iter().any(|(t, _)| t == &f.ts.rec.id)
                && activated.insert(f.ts.rec.id.clone())
            {
                effects.push(
                    PlanEffect::new("teamspace.activate", f.ts.rec.id.clone(), json!({ "name": f.ts.rec.name })).induced(),
                );
            }
            effects.push(PlanEffect::new("runtime.open_tab", seat.clone(), json!({ "name": f.rec.name })));
            let own: Vec<CloneId> = f
                .clones
                .iter()
                .filter(|(_, c)| r.clones.contains(&c.id))
                .map(|(_, c)| c.id.clone())
                .chain(r.new_clones.iter().filter(|(s, _)| s == seat).map(|(_, c)| c.clone()))
                .collect();
            runtime_effects(tree, &f.rec, &own, adopt.as_ref(), &mut effects)?;
        }
    }
    for c in &r.clones {
        let Some(rec) = read_clone(tree, c)? else { continue };
        if r.seats.iter().any(|(s, _)| s == &rec.seat) {
            continue;
        }
        relied_on.push(rev_of(c.clone(), rec.rev));
        effects.push(PlanEffect::new("clone.resurrect", c.clone(), json!({ "seat": rec.seat, "name": rec.name })));
        let f = read_seat(tree, &rec.seat)?;
        if f.rec.lifecycle == Lifecycle::Active {
            runtime_effects(tree, &f.rec, std::slice::from_ref(c), adopt.as_ref(), &mut effects)?;
        }
    }
    if let Some(app) = &r.app {
        let a = read_application(tree, app)?.ok_or_else(|| invalid(format!("no application {app}")))?;
        relied_on.push(rev_of(app.clone(), a.rev));
        effects.push(PlanEffect::new("application.resurrect", app.clone(), json!({ "name": a.name, "to": "active" })));
    }

    if let Some(a) = &adopt
        && let Some((x, _)) = &a.displaced
    {
        let xr = read_clone(tree, x)?.ok_or_else(|| invalid(format!("no clone {x}")))?;
        let xs = read_seat(tree, &xr.seat)?;
        relied_on.push(rev_of(x.clone(), xr.rev));
        effects.push(PlanEffect::new(
            "clone.retire",
            x.clone(),
            json!({
                "seat": xr.seat, "name": xr.name, "mechanism": "undo", "displaced_by_undo": true,
                "binding": a.displaced_binding,
            }),
        ));
        if xs.rec.lifecycle == Lifecycle::Active && xs.active_clones().all(|(_, c)| &c.id == x) {
            warnings.push(format!(
                "clone {} is the last active clone of seat {}; the seat is left with no active clone",
                xr.name, xs.rec.name
            ));
        }
        if let Some(msg) = a.conflict(&format!("{} of seat {}", xr.name, xs.rec.name))
            && repair.is_none()
        {
            repair = Some(msg);
        }
    }
    if let Some(pane) = caller_pane
        && let Some(w) = closes_caller(tree, pane, &effects)?
    {
        warnings.push(w);
    }
    Ok(PlanBody { effects, relied_on, warnings, repair_required: repair, summary: format!("undo {}", act.id) })
}

/// Pane effects for the clones `own` of one restored active seat: the caller's pane replaces the adopted
/// clone's `open_pane`.
fn runtime_effects(
    tree: &dyn TreeRead,
    seat: &SeatRecord,
    own: &[CloneId],
    adopt: Option<&Adoption>,
    effects: &mut Vec<PlanEffect>,
) -> Result<(), PlanError> {
    let mut as_active = seat.clone();
    as_active.lifecycle = Lifecycle::Active;
    let cfg = crate::model::effective::resolve_in(tree, &as_active)?;
    for c in own {
        match adopt {
            Some(a) if &a.clone == c => effects.push(PlanEffect::new(
                "undo.adopt_pane",
                c.clone(),
                json!({
                    "pane": a.pane, "seat": seat.id,
                    "displaced_clone": a.displaced.as_ref().map(|(x, _)| x),
                    "displaced_binding": a.displaced_binding,
                }),
            )),
            _ => effects.extend(open_clone_effects(c, &seat.id, &cfg)),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// template edits: inverse patch of the changed fields
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
pub enum Inverse {
    /// The edit document that puts the changed fields back, on top of the template as it is now.
    Ready { template: TemplateId, document: Box<TemplateDocument> },
    /// A later edit or a seat override touches a changed field.
    Conflict(String),
}

fn comp_str<'a>(act: &'a ActionRecord, key: &str) -> Option<&'a str> {
    act.compensation.get(key).and_then(|v| v.as_str())
}

fn comp_doc(act: &ActionRecord, key: &str) -> Result<TemplateDocument, PlanError> {
    act.compensation
        .get(key)
        .cloned()
        .ok_or_else(|| invalid(format!("action {} carries no `{key}` document", act.id)))?
        .try_into::<TemplateDocument>()
        .map_err(|e| invalid(format!("action {} `{key}` document: {e}", act.id)))
}

/// `a` and `b` name the same field, or one contains the other (`members.x` and `members.x.name`).
fn related(a: &str, b: &str) -> bool {
    a == b || a.strip_prefix(b).is_some_and(|r| r.starts_with('.')) || b.strip_prefix(a).is_some_and(|r| r.starts_with('.'))
}

fn patch_json<T: Serialize + DeserializeOwned>(t: &mut T, key: &str, v: Option<&Value>) -> Result<(), PlanError> {
    let mut j = serde_json::to_value(&*t).map_err(|e| invalid(e.to_string()))?;
    let obj = j.as_object_mut().ok_or_else(|| invalid("expected an object"))?;
    match v {
        Some(v) => obj.insert(key.to_owned(), v.clone()),
        None => obj.remove(key),
    };
    *t = serde_json::from_value(j).map_err(|e| invalid(format!("cannot restore {key}: {e}")))?;
    Ok(())
}

fn member_key(m: &DocumentMember) -> String {
    m.id.as_ref().map_or_else(|| m.name.clone(), |i| i.to_string())
}

fn member_mut<'a>(cur: &'a mut TemplateDocument, k: &str) -> Result<&'a mut DocumentMember, PlanError> {
    cur.members.iter_mut().find(|m| member_key(m) == k).ok_or_else(|| invalid(format!("member {k} no longer exists")))
}

/// Put one changed field back to its `before` value in `cur`.
fn restore_field(
    cur: &mut TemplateDocument,
    before_doc: &TemplateDocument,
    path: &str,
    before: Option<&Value>,
    after: Option<&Value>,
) -> Result<(), PlanError> {
    let parts: Vec<&str> = path.split('.').collect();
    match parts.as_slice() {
        ["name"] => {}
        ["defaults", f] => patch_json(&mut cur.defaults, f, before)?,
        ["relationships"] => {
            cur.relationships = match before {
                Some(v) => serde_json::from_value(v.clone()).map_err(|e| invalid(e.to_string()))?,
                None => vec![],
            }
        }
        ["members", k] => {
            if before.is_none() {
                cur.members.retain(|m| member_key(m) != *k);
            } else if after.is_none() {
                let old = before_doc
                    .members
                    .iter()
                    .find(|m| member_key(m) == *k)
                    .ok_or_else(|| invalid(format!("the edit's record has no member {k}")))?;
                cur.members.push(old.clone());
            }
        }
        ["members", k, "defaults", f] => patch_json(&mut member_mut(cur, k)?.defaults, f, before)?,
        // An edit document cannot delete the file: only a previous text is put back.
        ["members", k, "agents_md"] => {
            if before.is_some() {
                patch_json(member_mut(cur, k)?, "agents_md", before)?;
            }
        }
        ["members", k, f] => patch_json(member_mut(cur, k)?, f, before)?,
        _ => return Err(invalid(format!("cannot undo a change to {path}"))),
    }
    Ok(())
}

/// The seat override (`harness`, `model`, `args`, `summaries`) a changed template path would shadow:
/// `(member id when the path is member-specific, field)`.
fn override_of(path: &str) -> Option<(Option<&str>, &str)> {
    let parts: Vec<&str> = path.split('.').collect();
    let ok = |f: &str| matches!(f, "harness" | "model" | "args" | "summaries");
    match parts.as_slice() {
        ["defaults", f] if ok(f) => Some((None, f)),
        ["members", k, "defaults", f] if ok(f) => Some((Some(k), f)),
        _ => None,
    }
}

fn override_set(seat: &SeatRecord, field: &str) -> bool {
    match field {
        "harness" => seat.overrides.harness.is_some(),
        "model" => seat.overrides.model.is_some(),
        "args" => seat.overrides.args.is_some(),
        "summaries" => seat.overrides.summaries.is_some(),
        _ => false,
    }
}

/// The inverse patch of a template edit: each changed field goes back to its recorded before-value. A later
/// edit touching the same field, or a seat override now setting it, makes the undo `repair_required`.
pub fn template_inverse(tree: &dyn TreeRead, act: &ActionRecord) -> Result<Inverse, PlanError> {
    let tpl: TemplateId = comp_str(act, "template")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| invalid(format!("action {} names no template", act.id)))?;
    let before = comp_doc(act, "before")?;
    let after = comp_doc(act, "after")?;
    let Some((_, rec)) = read_template(tree, &tpl)? else {
        return Ok(Inverse::Conflict(format!("template {tpl} no longer exists")));
    };
    let cur = TemplateDocument::from_record(&rec);
    let changes = TemplateDocument::diff(&before, &after);
    let drift: Vec<String> = TemplateDocument::diff(&after, &cur).into_iter().map(|c| c.path).collect();
    let later: Vec<ActionRecord> = layout::list_actions(tree)?
        .into_iter()
        .map(|(_, a)| a)
        .filter(|a| {
            a.kind == ActionKind::TemplateEdit
                && a.id != act.id
                && (a.at, &a.id) > (act.at, &act.id)
                && comp_str(a, "template") == Some(tpl.as_str())
        })
        .collect();
    for c in &changes {
        if !drift.iter().any(|d| related(&c.path, d)) {
            continue;
        }
        let by = later.iter().find(|a| {
            a.compensation
                .get("changed_fields")
                .and_then(|v| v.as_array())
                .is_some_and(|f| f.iter().filter_map(|x| x.as_str()).any(|p| related(p, &c.path)))
        });
        return Ok(Inverse::Conflict(match by {
            Some(a) => {
                format!("template field {} was changed again by a later edit ({}); undo that edit first", c.path, a.id)
            }
            None => format!("template field {} has changed since this edit", c.path),
        }));
    }
    for c in &changes {
        let Some((member, field)) = override_of(&c.path) else { continue };
        for (_, seat) in layout::all_seats(tree)? {
            let Some(tr) = &seat.template_ref else { continue };
            if seat.lifecycle == Lifecycle::Retired
                || tr.template != tpl
                || member.is_some_and(|m| m != tr.member.as_str())
                || !override_set(&seat, field)
            {
                continue;
            }
            return Ok(Inverse::Conflict(format!(
                "seat {} ({}) overrides {field}, which this edit changed ({}); the override would make the \
                 restored value ambiguous",
                seat.name, seat.id, c.path
            )));
        }
    }
    let mut doc = cur;
    for c in &changes {
        restore_field(&mut doc, &before, &c.path, c.before.as_ref(), c.after.as_ref())?;
    }
    Ok(Inverse::Ready { template: tpl, document: Box::new(doc) })
}

/// Arguments of the template edit that applies the inverse patch.
pub fn template_edit_args(template: &TemplateId, doc: &TemplateDocument) -> Value {
    json!({ "template": template.to_string(), "document": doc })
}
