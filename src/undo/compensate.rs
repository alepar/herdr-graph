//! The `undo` organizational kind (spec §6): previews and applies compensating operations. An undo is its
//! own action (`kind = undo`, `undoes = <act_>`); the original action's `undone_by` records the op.
use super::adopt::{ADOPT_BINDING_KEY, adoption};
use super::candidates::{is_undoable, read_action};
use super::preview::{
    Inverse, closes_caller, compute_restore, read_application, read_clone, restore_preview, template_edit_args,
    template_inverse,
};
use crate::daemon::registry::CallerInfo;
use crate::model::action::{ActionKind, ActionRecord};
use crate::model::application::ApplicationRecord;
use crate::model::change::RequestKind;
use crate::model::clone::CloneRecord;
use crate::model::common::{AppLifecycle, Binding, CloneLifecycle, Lifecycle, RetireMechanism, Retirement, Runtime};
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::template::Relationship;
use crate::model::{ActionId, AnyId, AppId, CloneId, IdKind};
use crate::plan::core_kinds::{
    Acc, absent, clone_slug_for, mm, new_clone_record, parse_args, read_seat, read_ts, rev_of, write_action,
};
use crate::plan::grammar::positional;
use crate::plan::kind::{OrgKind, PlanBody, PlanCx, PlanError};
use crate::plan::types::{Plan, Reserved};
use crate::store::layout;
use crate::store::slug::unique_slug;
use crate::writer::{Applied, MutationCx, MutationError, Reject};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

/// The undo kind. Hydration undo and template-edit undo run through the kinds that own those rules
/// (`application retire`, `template edit`), so they stay in step with them.
pub struct UndoKind {
    pub(crate) app_retire: Arc<dyn OrgKind>,
    pub(crate) template_edit: Arc<dyn OrgKind>,
}

#[derive(Deserialize)]
struct UndoArgs {
    act: String,
    #[serde(default)]
    caller_pane: Option<String>,
}

fn invalid(m: impl Into<String>) -> PlanError {
    PlanError::Invalid(m.into())
}

fn reject(reason: &str, explanation: String) -> MutationError {
    MutationError::Reject(Reject { reason: reason.into(), explanation, current_revs: vec![] })
}

fn act_id(a: &UndoArgs) -> Result<ActionId, PlanError> {
    a.act.parse().map_err(|e| PlanError::Usage(format!("undo <act>: {e}")))
}

/// The action to undo, checked to be undoable and not yet undone.
fn target(tree: &dyn crate::store::tree::TreeRead, id: &ActionId) -> Result<ActionRecord, PlanError> {
    let rec = read_action(tree, id)?.ok_or_else(|| invalid(format!("no action {id}")))?;
    if !is_undoable(rec.kind) {
        return Err(invalid(format!("action {id} is a {:?} and cannot be undone", rec.kind)));
    }
    if let Some(op) = rec.undone_by.first() {
        return Err(invalid(format!("action {id} was already undone by {op}")));
    }
    Ok(rec)
}

fn comp_app(rec: &ActionRecord) -> Result<AppId, PlanError> {
    rec.compensation
        .get("application")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| invalid(format!("action {} names no application", rec.id)))
}

impl OrgKind for UndoKind {
    fn kind(&self) -> RequestKind {
        RequestKind::Undo
    }

    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("undo", "")]
    }

    fn parse(&self, words: &[String], caller: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let act = positional(words, 0).ok_or_else(|| PlanError::Usage("undo <act>".into()))?;
        act.parse::<ActionId>().map_err(|e| PlanError::Usage(format!("undo <act>: {e}")))?;
        Ok(json!({ "act": act, "caller_pane": caller.pane_id }))
    }

    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: UndoArgs = parse_args(args)?;
        let id = act_id(&a)?;
        let _act: ActionId = reserved.get_or_mint("act");
        let orig = target(cx.tree, &id)?;
        let pane = a.caller_pane.as_deref();
        let mut body = match orig.kind {
            ActionKind::ClosureCascade | ActionKind::Retire | ActionKind::ApplicationRetire => {
                restore_preview(cx, &orig, pane, reserved)?
            }
            ActionKind::Hydrate => {
                let app = comp_app(&orig)?;
                let rec = read_application(cx.tree, &app)?.ok_or_else(|| invalid(format!("no application {app}")))?;
                if rec.lifecycle == AppLifecycle::Retired {
                    return Err(invalid(format!("application {} ({app}) is already retired", rec.name)));
                }
                self.app_retire.plan(cx, &json!({ "application": app.to_string() }), reserved)?
            }
            ActionKind::TemplateEdit => match template_inverse(cx.tree, &orig)? {
                Inverse::Conflict(why) => PlanBody {
                    effects: vec![],
                    relied_on: vec![],
                    warnings: vec![],
                    repair_required: Some(why),
                    summary: String::new(),
                },
                Inverse::Ready { template, document } => {
                    self.template_edit.plan(cx, &template_edit_args(&template, &document), reserved)?
                }
            },
            ActionKind::Resurrect | ActionKind::Undo => unreachable!("{:?} is not undoable", orig.kind),
        };
        if matches!(orig.kind, ActionKind::Hydrate | ActionKind::TemplateEdit)
            && let Some(p) = pane
            && let Some(w) = closes_caller(cx.tree, p, &body.effects)?
        {
            body.warnings.push(w);
        }
        body.relied_on.push(rev_of(orig.id.clone(), orig.rev));
        body.summary = format!("undo {} action {}", format!("{:?}", orig.kind).to_lowercase(), orig.id);
        Ok(body)
    }

    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: UndoArgs = parse_args(args).map_err(mm)?;
        let id = act_id(&a).map_err(mm)?;
        let orig = target(&cx.tree, &id).map_err(mm)?;
        let act: ActionId =
            plan.reserved.get("act").ok_or_else(|| MutationError::Bug("plan reserved no act id".into()))?;
        match orig.kind {
            ActionKind::ClosureCascade | ActionKind::Retire | ActionKind::ApplicationRetire => {
                restore_mutate(cx, &orig, &act, a.caller_pane.as_deref(), plan)?;
            }
            ActionKind::Hydrate => {
                let app = comp_app(&orig).map_err(mm)?;
                self.app_retire.mutate(cx, &json!({ "application": app.to_string() }), plan)?;
            }
            ActionKind::TemplateEdit => match template_inverse(&cx.tree, &orig).map_err(mm)? {
                Inverse::Conflict(why) => return Err(reject("repair_required", why)),
                Inverse::Ready { template, document } => {
                    self.template_edit.mutate(cx, &template_edit_args(&template, &document), plan)?;
                }
            },
            ActionKind::Resurrect | ActionKind::Undo => {
                return Err(MutationError::Bug(format!("{:?} is not undoable", orig.kind)));
            }
        }
        finalize(cx, &orig, &act)?;
        Ok(Applied { summary: format!("undo action {}", orig.id), action: Some(act) })
    }
}

/// `act`'s record becomes the undo action; the original action records the op in `undone_by`; every object
/// the undo retired carries mechanism `undo`.
fn finalize(cx: &mut MutationCx<'_>, orig: &ActionRecord, act: &ActionId) -> Result<(), MutationError> {
    let path = layout::action_record(cx.now, act);
    let mut rec: ActionRecord =
        cx.tree.read_record(&path)?.ok_or_else(|| MutationError::Bug(format!("undo wrote no action record {act}")))?;
    rec.kind = ActionKind::Undo;
    rec.undoes = Some(orig.id.clone());
    cx.tree.put_record(path, &mut rec)?;
    for id in &rec.retired {
        restamp(cx, id, act)?;
    }
    let loc = cx
        .tree
        .locate(&orig.id.to_any())?
        .ok_or_else(|| MutationError::Bug(format!("action {} vanished", orig.id)))?;
    let mut o: ActionRecord = cx
        .tree
        .read_record(&loc.record_path)?
        .ok_or_else(|| MutationError::Bug(format!("action {} vanished", orig.id)))?;
    o.undone_by.push(cx.op.clone());
    cx.tree.put_record(loc.record_path, &mut o)?;
    Ok(())
}

/// Retirements an undo performs are `undo` retirements, whichever shared path did the retiring.
fn restamp(cx: &mut MutationCx<'_>, id: &AnyId, act: &ActionId) -> Result<(), MutationError> {
    macro_rules! stamp {
        ($ty:ty) => {{
            if let Some(loc) = cx.tree.locate(id)?
                && let Some(mut rec) = cx.tree.read_record::<$ty>(&loc.record_path)?
                && let Some(r) = rec.retired.as_mut()
                && r.action.as_ref() == Some(act)
                && r.mechanism != RetireMechanism::Undo
            {
                r.mechanism = RetireMechanism::Undo;
                cx.tree.put_record(loc.record_path, &mut rec)?;
            }
        }};
    }
    match id.kind() {
        IdKind::Teamspace => stamp!(TeamspaceRecord),
        IdKind::Seat => stamp!(SeatRecord),
        IdKind::Clone => stamp!(CloneRecord),
        IdKind::Application => stamp!(ApplicationRecord),
        _ => {}
    }
    Ok(())
}

/// Resurrect exactly what the action retired; adopt the caller's pane as one restored clone.
fn restore_mutate(
    cx: &mut MutationCx<'_>,
    orig: &ActionRecord,
    act: &ActionId,
    caller_pane: Option<&str>,
    plan: &Plan,
) -> Result<(), MutationError> {
    // The caller's pane as Herdr showed it when `undo.apply` admitted the op: an observed input carried in the
    // request's typed `confirmed.observed`, not a recomputed effect.
    let observed: Option<Binding> =
        cx.request.confirmed.as_ref().and_then(|c| c.observed.get(ADOPT_BINDING_KEY)).and_then(|v| serde_json::from_value(v.clone()).ok());
    let mut reserved = plan.reserved.clone();
    let r = compute_restore(&cx.tree, orig, &mut reserved).map_err(mm)?;
    let runtime = r.runtime_clones(&cx.tree).map_err(mm)?;
    let adopt = match caller_pane {
        Some(p) => adoption(&cx.tree, p, &runtime)?,
        None => None,
    };
    let mut acc = Acc::default();

    for (ts_id, active) in &r.teamspaces {
        let ts = read_ts(&cx.tree, ts_id).map_err(mm)?;
        let mut rec = ts.rec.clone();
        rec.lifecycle = if *active { Lifecycle::Active } else { Lifecycle::Dormant };
        rec.retired = None;
        rec.runtime = absent();
        cx.tree.put_record(ts.loc.record_path.clone(), &mut rec)?;
        acc.changed(ts_id.to_any(), "retired", if *active { "active" } else { "dormant" });
        let taken = layout::taken_slugs(&cx.tree, &layout::teamspaces_root())?;
        let live = layout::teamspace_dir(&unique_slug(&rec.name, ts_id.suffix6(), &taken));
        cx.tree.move_dir(&ts.loc.folder, &live)?;
    }

    let set = Some(r.set.clone());
    for (seat, active) in &r.seats {
        crate::plan::core_kinds::do_resurrect_seat(cx, seat, *active, &set, &mut acc)?;
        if !*active {
            continue;
        }
        let f = read_seat(&cx.tree, seat).map_err(mm)?;
        if f.ts.rec.lifecycle == Lifecycle::Dormant {
            let mut t = f.ts.rec.clone();
            t.lifecycle = Lifecycle::Active;
            cx.tree.put_record(f.ts.loc.record_path.clone(), &mut t)?;
        }
        if let Some((_, cid)) = r.new_clones.iter().find(|(s, _)| s == seat) {
            let slug = clone_slug_for(&cx.tree, &f.loc.folder, &f.rec.name, cid).map_err(mm)?;
            let mut clone = new_clone_record(seat, &f.rec.name, cid.clone());
            cx.tree.put_record(layout::clone_record(&layout::clone_dir(&f.loc.folder, &slug)), &mut clone)?;
        }
    }

    for c in &r.clones {
        let Some(rec) = read_clone(&cx.tree, c).map_err(mm)? else { continue };
        if r.seats.iter().any(|(s, _)| s == &rec.seat) {
            continue;
        }
        let loc = cx.tree.locate(&c.to_any())?.ok_or_else(|| MutationError::Bug(format!("clone {c} vanished")))?;
        let mut rec = rec;
        rec.lifecycle = CloneLifecycle::Active;
        rec.retired = None;
        rec.occupant = None;
        rec.runtime = absent();
        cx.tree.put_record(loc.record_path, &mut rec)?;
        acc.changed(c.to_any(), "retired", "active");
        let f = read_seat(&cx.tree, &rec.seat).map_err(mm)?;
        if f.rec.lifecycle == Lifecycle::Active {
            let mut seat = f.rec.clone();
            seat.activation.last_op = Some(cx.op.clone());
            cx.tree.put_record(f.loc.record_path.clone(), &mut seat)?;
        }
    }

    if let Some(app) = &r.app {
        let loc = cx.tree.locate(&app.to_any())?.ok_or_else(|| MutationError::Bug(format!("application {app} vanished")))?;
        let mut rec = read_application(&cx.tree, app).map_err(mm)?.ok_or_else(|| MutationError::Bug(format!("application {app} vanished")))?;
        rec.lifecycle = AppLifecycle::Active;
        rec.retired = None;
        rec.contributions.relationships = orig
            .compensation
            .get("relationships_removed")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.clone().try_into::<Relationship>().ok()).collect())
            .unwrap_or_default();
        cx.tree.put_record(loc.record_path, &mut rec)?;
        acc.changed(app.to_any(), "retired", "active");
    }

    let mut comp = toml::Table::new();
    comp.insert("undoes_kind".into(), toml::Value::String(format!("{:?}", orig.kind).to_lowercase()));
    if let Some(a) = &adopt {
        if let Some((x, _)) = &a.displaced {
            displace(cx, x, act, &mut acc)?;
        }
        let loc = cx
            .tree
            .locate(&a.clone.to_any())?
            .ok_or_else(|| MutationError::Bug(format!("adopted clone {} vanished", a.clone)))?;
        let mut rec: CloneRecord = cx
            .tree
            .read_record(&loc.record_path)?
            .ok_or_else(|| MutationError::Bug(format!("adopted clone {} vanished", a.clone)))?;
        // The adopted clone commits with the caller's pane bound and `present`: the commit that wakes the
        // reconciler already holds the binding, so there is no restored clone without a pane to create one for.
        let (availability, binding) = a.committed_runtime(observed.as_ref());
        rec.runtime = Runtime { availability, bound: Some(binding), observed_at: Some(cx.now) };
        let mut seat = read_seat(&cx.tree, &rec.seat).map_err(mm)?;
        seat.rec.activation.last_op = Some(cx.op.clone());
        cx.tree.put_record(seat.loc.record_path.clone(), &mut seat.rec)?;
        cx.tree.put_record(loc.record_path, &mut rec)?;
        let mut t = toml::Table::new();
        t.insert("clone".into(), toml::Value::String(a.clone.to_string()));
        t.insert("pane".into(), toml::Value::String(a.pane.clone()));
        if let Some(rt) = &rec.runtime.bound
            && let Ok(b) = toml::Value::try_from(rt)
        {
            t.insert("binding".into(), b);
        }
        if let Some((x, _)) = &a.displaced {
            t.insert("displaced".into(), toml::Value::String(x.to_string()));
        }
        comp.insert("adopt".into(), toml::Value::Table(t));
    }
    write_action(cx, act, ActionKind::Undo, acc, comp)?;
    Ok(())
}

/// Retire the clone the caller's pane was bound to (mechanism `undo`); its pane now belongs to the adopted
/// clone, so its runtime binding is dropped first and the reconciler has nothing to close.
fn displace(cx: &mut MutationCx<'_>, x: &CloneId, act: &ActionId, acc: &mut Acc) -> Result<(), MutationError> {
    let loc = cx.tree.locate(&x.to_any())?.ok_or_else(|| MutationError::Bug(format!("clone {x} vanished")))?;
    let mut rec = read_clone(&cx.tree, x).map_err(mm)?.ok_or_else(|| MutationError::Bug(format!("clone {x} vanished")))?;
    // Backstop: OrgMutation recomputes the plan against this revision first and rejects an occupied displaced clone (repair_required); this guard only matters if a kind ever skips that recompute.
    if rec.occupant.is_some() {
        return Err(reject(
            "repair_required",
            format!("clone {x} gained an active session since the plan; plan the undo again"),
        ));
    }
    rec.lifecycle = CloneLifecycle::Retired;
    rec.retired = Some(Retirement { op: cx.op.clone(), action: Some(act.clone()), at: cx.now, mechanism: RetireMechanism::Undo });
    rec.runtime = absent();
    cx.tree.put_record(loc.record_path, &mut rec)?;
    acc.retired.push(x.to_any());
    acc.changed(x.to_any(), "active", "retired");
    Ok(())
}
