//! Undo tests: a real writer, git store, journal, plan engine, core + template + undo kinds and the observer's
//! cascade mutation, so cascades are produced by committing `observed.cascade` requests directly.
use super::adopt::admit_adopted_binding;
use super::candidates::{list_candidates, render_list};
use super::preview::CLOSES_PANE_WARNING;
use super::register_kinds;
use crate::daemon::registry::CallerInfo;
use crate::journal::{Journal, OpRow};
use crate::model::action::{ActionKind, ActionRecord};
use crate::model::application::ApplicationRecord;
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::clone::CloneRecord;
use crate::model::common::{
    AppLifecycle, Availability, Binding, CloneLifecycle, Lifecycle, Occupant, RetireMechanism, Runtime,
};
use crate::model::harness::Harness;
use crate::model::operation::OpState;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::template::TemplateRecord;
use crate::model::{
    ActionId, AnyId, CloneId, HerdrPaneId, HerdrTabId, HerdrWorkspaceId, MemberId, NsId, PlanId, SeatId,
};
use crate::observe::classify::CascadeRule;
use crate::observe::mutations::cascade_request;
use crate::plan::commands::{PlanDeps, admit_apply, create_plan};
use crate::plan::core_kinds::register_core_kinds;
use crate::plan::kind::KindRegistry;
use crate::plan::store::PlanStore;
use crate::plan::types::StoredPlan;
use crate::ports::clock::{Clock, ManualClock};
use crate::ports::store::Store;
use crate::ports::writer::Writer;
use crate::store::GitStore;
use crate::store::init::init_instance;
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::CommitView;
use crate::templates::register_kinds as register_template_kinds;
use crate::writer::{Applied, Mutation, MutationCx, MutationError, MutationRegistry, WriterConfig, WriterCore};
use chrono::TimeZone;
use serde_json::json;
use std::sync::Arc;

// ---------------------------------------------------------------------------------------------
// fixture
// ---------------------------------------------------------------------------------------------

fn t0() -> crate::model::Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

/// Test-only clone patch: `pane` binds the clone to that pane (present), `occupy` gives it a running session.
/// Stands in for what the reconciler and the observer write.
struct Patch;
impl Mutation for Patch {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a = &cx.request.args;
        let clone = CloneId::parse(a["clone"].as_str().unwrap()).map_err(|e| MutationError::Bug(e.to_string()))?;
        let loc = cx.tree.locate(&clone.to_any())?.ok_or_else(|| MutationError::Bug("no clone".into()))?;
        let mut rec: CloneRecord =
            cx.tree.read_record(&loc.record_path)?.ok_or_else(|| MutationError::Bug("no clone".into()))?;
        if let Some(p) = a.get("pane").and_then(|v| v.as_str()) {
            rec.runtime = Runtime {
                availability: Availability::Present,
                bound: Some(Binding {
                    token: Some("hg=test".into()),
                    workspace_id: Some(HerdrWorkspaceId("w1".into())),
                    tab_id: Some(HerdrTabId("t1".into())),
                    pane_id: Some(HerdrPaneId(p.into())),
                    terminal_id: None,
                    incarnation: Default::default(),
                }),
                observed_at: Some(cx.now),
            };
        }
        if a.get("occupy").is_some() {
            rec.occupant = Some(Occupant { native_session: NsId::new(), harness: Harness::Claude, since: cx.now });
        }
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "patch".into(), action: None })
    }
}

struct Fx {
    _tmp: tempfile::TempDir,
    docs: std::path::PathBuf,
    deps: PlanDeps,
    w: Arc<WriterCore>,
    store: Arc<GitStore>,
    clock: Arc<ManualClock>,
    n: std::cell::Cell<u32>,
}

fn fx() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("inst");
    let docs = tmp.path().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    init_instance(&root).unwrap();
    let mut kinds = KindRegistry::default();
    register_core_kinds(&mut kinds);
    register_template_kinds(&mut kinds);
    register_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    let plans = Arc::new(PlanStore::new(root.join(".graph-local/plans")));
    let mut reg = MutationRegistry::default();
    reg.register("bookkeeping.patch", Arc::new(Patch));
    kinds.register_mutations(&mut reg, plans.clone());
    crate::observe::register_mutations(&mut reg);
    crate::reconcile::register_mutations(&mut reg);
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let w = WriterCore::new(store.clone(), journal, Arc::new(reg), clock.clone(), WriterConfig::default());
    let deps = PlanDeps { kinds, plans, store: store.clone(), writer: w.clone(), clock: clock.clone(), instance: root };
    Fx { _tmp: tmp, docs, deps, w, store, clock, n: std::cell::Cell::new(0) }
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn caller(pane: Option<&str>) -> CallerInfo {
    CallerInfo { pane_id: pane.map(str::to_owned), ..CallerInfo::default() }
}

fn plan_as(fx: &Fx, change: &str, pane: Option<&str>) -> StoredPlan {
    let v = create_plan(&fx.deps, &caller(pane), words(change)).unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    fx.deps.plans.get(&id).unwrap().unwrap()
}

fn plan(fx: &Fx, change: &str) -> StoredPlan {
    plan_as(fx, change, None)
}

fn plan_err(fx: &Fx, change: &str) -> String {
    match create_plan(&fx.deps, &caller(None), words(change)) {
        Ok(_) => panic!("{change}: expected a planning error"),
        Err(e) => e.message,
    }
}

fn apply_plan(fx: &Fx, sp: &StoredPlan) -> OpRow {
    let op = admit_apply(&fx.deps, &caller(None), sp.plan.id.as_str(), Some(&sp.hash), "relay")
        .unwrap_or_else(|e| panic!("admit: {}", e.message));
    fx.w.drain().unwrap();
    fx.w.journal().get(&op).unwrap().unwrap()
}

fn commit(fx: &Fx, change: &str) -> OpRow {
    let row = apply_plan(fx, &plan(fx, change));
    assert_eq!(row.state, OpState::Committed, "{change}: {:?}", row.rejection);
    row
}

fn view(fx: &Fx) -> CommitView<'_> {
    CommitView { store: &*fx.store, at: fx.store.head().unwrap() }
}

fn tick(fx: &Fx) {
    fx.clock.advance(chrono::Duration::minutes(1));
}

fn kinds_of(sp: &StoredPlan, kind: &str) -> Vec<AnyId> {
    sp.plan.effects.iter().filter(|e| e.kind == kind).map(|e| e.object.clone()).collect()
}

fn action(fx: &Fx, act: &ActionId) -> ActionRecord {
    let v = view(fx);
    let loc = layout::locate(&v, &act.to_any()).unwrap().expect("action record committed");
    read_toml(&v, &loc.record_path).unwrap().unwrap()
}

fn action_of(fx: &Fx, row: &OpRow) -> ActionRecord {
    action(fx, row.action.as_ref().expect("op carries an action"))
}

fn seat_named(fx: &Fx, name: &str) -> SeatRecord {
    let mut found: Vec<_> = layout::all_seats(&view(fx)).unwrap().into_iter().filter(|(_, s)| s.name == name).collect();
    assert_eq!(found.len(), 1, "seat {name}");
    found.remove(0).1
}

fn seat_by_id(fx: &Fx, id: &SeatId) -> SeatRecord {
    layout::all_seats(&view(fx)).unwrap().into_iter().map(|(_, s)| s).find(|s| &s.id == id).expect("seat")
}

fn clones_of(fx: &Fx, seat: &SeatId) -> Vec<CloneRecord> {
    layout::all_clones(&view(fx)).unwrap().into_iter().map(|(_, c)| c).filter(|c| &c.seat == seat).collect()
}

fn clone_by_id(fx: &Fx, id: &CloneId) -> CloneRecord {
    layout::all_clones(&view(fx)).unwrap().into_iter().map(|(_, c)| c).find(|c| &c.id == id).expect("clone")
}

fn ts_named(fx: &Fx, name: &str) -> TeamspaceRecord {
    layout::list_teamspaces(&view(fx)).unwrap().into_iter().map(|(_, t)| t).find(|t| t.name == name).expect("teamspace")
}

fn app(fx: &Fx, name: &str) -> ApplicationRecord {
    layout::list_applications(&view(fx)).unwrap().into_iter().map(|(_, a)| a).find(|a| a.name == name).expect("application")
}

fn tpl(fx: &Fx, name: &str) -> TemplateRecord {
    layout::list_templates(&view(fx)).unwrap().into_iter().map(|(_, t)| t).find(|t| t.name == name).expect("template")
}

/// An observed closure cascade, committed directly (what the observer does when the user closes a tab).
fn cascade(fx: &Fx, rule: CascadeRule, ids: &[AnyId]) -> ActionRecord {
    let op = fx.w.admit(cascade_request(rule, ids, fx.clock.now())).unwrap();
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(row.state, OpState::Committed, "cascade: {:?}", row.rejection);
    action_of(fx, &row)
}

fn patch(fx: &Fx, args: serde_json::Value) {
    let mut args = args;
    args["sub"] = json!("patch");
    let op = fx
        .w
        .admit(ChangeRequest {
            kind: RequestKind::Bookkeeping,
            args,
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
        })
        .unwrap();
    fx.w.drain().unwrap();
    assert_eq!(fx.w.journal().get(&op).unwrap().unwrap().state, OpState::Committed);
}

fn alpha(fx: &Fx) {
    commit(fx, "teamspace create alpha");
}

/// One active seat with `n` clones.
fn active_seat(fx: &Fx, name: &str) -> SeatRecord {
    commit(fx, &format!("seat create {name} --teamspace alpha --active"));
    seat_named(fx, name)
}

// template documents -----------------------------------------------------------------------------

struct M {
    id: MemberId,
    name: &'static str,
    startup: &'static str,
    model: Option<&'static str>,
}

fn m(id: &MemberId, name: &'static str, startup: &'static str) -> M {
    M { id: id.clone(), name, startup, model: None }
}

fn doc_text(name: &str, members: &[M]) -> String {
    let mut s = format!("name = \"{name}\"\n");
    for m in members {
        s.push_str(&format!("\n[[members]]\nid = \"{}\"\nname = \"{}\"\nstartup = \"{}\"\n", m.id, m.name, m.startup));
        if let Some(model) = m.model {
            s.push_str(&format!("[members.defaults]\nmodel = \"{model}\"\n"));
        }
    }
    s
}

fn write_doc(fx: &Fx, text: &str) -> String {
    fx.n.set(fx.n.get() + 1);
    let p = fx.docs.join(format!("doc{}.toml", fx.n.get()));
    std::fs::write(&p, text).unwrap();
    p.to_str().unwrap().to_owned()
}

fn create_template(fx: &Fx, name: &str, members: &[M]) {
    let path = write_doc(fx, &doc_text(name, members));
    commit(fx, &format!("template create {name} --from {path}"));
}

fn edit_template(fx: &Fx, name: &str, members: &[M]) -> OpRow {
    let path = write_doc(fx, &doc_text(name, members));
    commit(fx, &format!("template edit {name} --from {path}"))
}

fn apply_app(fx: &Fx, template: &str, name: &str) -> OpRow {
    commit(fx, &format!("application apply {template} --teamspace alpha --name {name}"))
}

// ---------------------------------------------------------------------------------------------
// closure cascades and retirements
// ---------------------------------------------------------------------------------------------

#[test]
fn undo_tab_close_cascade_restores_seat_and_clones_excluding_already_retired() {
    let fx = fx();
    alpha(&fx);
    let seat = active_seat(&fx, "foreman");
    commit(&fx, "clone add foreman");
    let clones = clones_of(&fx, &seat.id);
    assert_eq!(clones.len(), 2);
    let (c1, c2) = (clones[0].clone(), clones[1].clone());
    commit(&fx, &format!("clone retire {}", c2.id));
    let act = cascade(&fx, CascadeRule::Tab, &[seat.id.to_any(), c1.id.to_any(), c2.id.to_any()]);
    assert_eq!(act.kind, ActionKind::ClosureCascade);
    assert!(act.already_retired.contains(&c2.id.to_any()), "c2 was retired before the cascade");
    assert!(act.retired.contains(&seat.id.to_any()) && act.retired.contains(&c1.id.to_any()));
    assert_eq!(seat_by_id(&fx, &seat.id).lifecycle, Lifecycle::Retired);

    let sp = plan(&fx, &format!("undo {}", act.id));
    assert_eq!(kinds_of(&sp, "seat.resurrect"), vec![seat.id.to_any()]);
    assert_eq!(kinds_of(&sp, "clone.resurrect"), vec![c1.id.to_any()], "exactly the retired clone, not the already-retired one");
    assert_eq!(kinds_of(&sp, "runtime.open_tab"), vec![seat.id.to_any()]);
    assert!(kinds_of(&sp, "runtime.open_pane").contains(&c1.id.to_any()), "the restored runtime gets a new pane");
    assert!(sp.plan.repair_required.is_none());

    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(seat_by_id(&fx, &seat.id).lifecycle, Lifecycle::Active);
    let c1_after = clone_by_id(&fx, &c1.id);
    assert_eq!(c1_after.lifecycle, CloneLifecycle::Active);
    assert_eq!(c1_after.runtime.availability, Availability::Absent, "the restored clone has no pane until the reconciler opens one");
    assert_eq!(clone_by_id(&fx, &c2.id).lifecycle, CloneLifecycle::Retired, "the already-retired clone stays retired");
}

#[test]
fn undo_keeps_previously_dormant_seats_dormant() {
    let fx = fx();
    alpha(&fx);
    let active = active_seat(&fx, "worker");
    commit(&fx, "seat create sleeper --teamspace alpha");
    let sleeper = seat_named(&fx, "sleeper");
    assert_eq!(sleeper.lifecycle, Lifecycle::Dormant);
    let ts = ts_named(&fx, "alpha");
    assert_eq!(ts.lifecycle, Lifecycle::Active);
    let act = cascade(&fx, CascadeRule::Workspace, &[ts.id.to_any()]);
    assert!(act.retired.contains(&sleeper.id.to_any()));

    let sp = plan(&fx, &format!("undo {}", act.id));
    let sleeper_effect = sp.plan.effects.iter().find(|e| e.kind == "seat.resurrect" && e.object == sleeper.id.to_any()).unwrap();
    assert_eq!(sleeper_effect.detail["to"], json!("dormant"));
    assert_eq!(kinds_of(&sp, "runtime.open_tab"), vec![active.id.to_any()], "only the previously active seat gets a tab");
    assert_eq!(kinds_of(&sp, "runtime.open_workspace"), vec![ts.id.to_any()]);

    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(seat_by_id(&fx, &sleeper.id).lifecycle, Lifecycle::Dormant);
    assert_eq!(seat_by_id(&fx, &active.id).lifecycle, Lifecycle::Active);
    assert_eq!(ts_named(&fx, "alpha").lifecycle, Lifecycle::Active);
}

#[test]
fn undo_plan_applied_seat_retirement_previews_and_applies_resurrection() {
    let fx = fx();
    alpha(&fx);
    let seat = active_seat(&fx, "foreman");
    let retire = commit(&fx, "seat retire foreman");
    let act = action_of(&fx, &retire);
    assert_eq!(act.kind, ActionKind::Retire);
    assert_eq!(seat_by_id(&fx, &seat.id).lifecycle, Lifecycle::Retired);

    let sp = plan(&fx, &format!("undo {}", act.id));
    assert_eq!(kinds_of(&sp, "seat.resurrect"), vec![seat.id.to_any()]);
    assert_eq!(kinds_of(&sp, "clone.resurrect").len(), 1);
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let back = seat_by_id(&fx, &seat.id);
    assert_eq!(back.lifecycle, Lifecycle::Active);
    assert!(back.retired.is_none());
    assert!(clones_of(&fx, &seat.id).iter().all(|c| c.lifecycle == CloneLifecycle::Active));
}

// ---------------------------------------------------------------------------------------------
// caller-pane adoption
// ---------------------------------------------------------------------------------------------

/// `foreman` (retired by a tab cascade) and `other`, whose only clone is bound to `pane`.
fn adoption_fixture(fx: &Fx, pane: &str) -> (SeatRecord, CloneRecord, CloneRecord, ActionRecord) {
    alpha(fx);
    let foreman = active_seat(fx, "foreman");
    let other = active_seat(fx, "other");
    let x = clones_of(fx, &other.id).remove(0);
    patch(fx, json!({ "clone": x.id, "pane": pane }));
    let f_clone = clones_of(fx, &foreman.id).remove(0);
    let act = cascade(fx, CascadeRule::Tab, &[foreman.id.to_any(), f_clone.id.to_any()]);
    (foreman, f_clone, clone_by_id(fx, &x.id), act)
}

#[test]
fn adoption_preview_with_bound_caller_pane() {
    let fx = fx();
    let (foreman, f_clone, x, act) = adoption_fixture(&fx, "p-caller");
    let sp = plan_as(&fx, &format!("undo {}", act.id), Some("p-caller"));
    assert!(sp.plan.repair_required.is_none(), "{:?}", sp.plan.repair_required);

    let adopt = sp.plan.effects.iter().find(|e| e.kind == "undo.adopt_pane").expect("the preview names the adoption");
    assert_eq!(adopt.object, f_clone.id.to_any(), "the caller pane is adopted as the restored clone");
    assert_eq!(adopt.detail["pane"], json!("p-caller"));
    assert_eq!(adopt.detail["displaced_clone"], json!(x.id));
    assert_eq!(adopt.detail["displaced_binding"]["pane_id"], json!("p-caller"), "the existing binding is shown");
    assert!(
        !kinds_of(&sp, "runtime.open_pane").contains(&f_clone.id.to_any()),
        "no second pane is opened for the adopted clone"
    );
    let retire = sp.plan.effects.iter().find(|e| e.kind == "clone.retire").expect("the displaced clone is retired");
    assert_eq!(retire.object, x.id.to_any());
    assert_eq!(retire.detail["mechanism"], json!("undo"));
    assert_eq!(retire.detail["displaced_by_undo"], json!(true));
    assert!(sp.plan.warnings.iter().any(|w| w.contains("last active clone")), "{:?}", sp.plan.warnings);

    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let displaced = clone_by_id(&fx, &x.id);
    assert_eq!(displaced.lifecycle, CloneLifecycle::Retired);
    assert_eq!(displaced.retired.unwrap().mechanism, RetireMechanism::Undo);
    assert!(displaced.runtime.bound.is_none(), "the pane is no longer the displaced clone's");
    let adopted = clone_by_id(&fx, &f_clone.id);
    assert_eq!(adopted.lifecycle, CloneLifecycle::Active);
    let bound = adopted.runtime.bound.clone().expect("the adopted clone holds the caller's pane");
    assert_eq!(bound.pane_id, Some(HerdrPaneId("p-caller".into())));
    assert_eq!(bound.token, None, "the reconciler stamps the new clone's token on its next step");
    assert_eq!(adopted.runtime.availability, Availability::Unknown, "no second pane before the binding is confirmed");
    assert_eq!(seat_by_id(&fx, &foreman.id).lifecycle, Lifecycle::Active);

    // After the op commits the binding write is admitted.
    let undo_act = action_of(&fx, &row);
    assert!(admit_adopted_binding(&*fx.store, &*fx.w, &undo_act.id).unwrap());
    fx.w.drain().unwrap();
    let adopted = clone_by_id(&fx, &f_clone.id);
    assert_eq!(adopted.runtime.availability, Availability::Present);
    assert_eq!(adopted.runtime.bound.unwrap().pane_id, Some(HerdrPaneId("p-caller".into())));
}

#[test]
fn adoption_conflict_with_active_occupant_is_repair_required() {
    let fx = fx();
    let (_, f_clone, x, act) = adoption_fixture(&fx, "p-busy");
    patch(&fx, json!({ "clone": x.id, "occupy": true }));
    let sp = plan_as(&fx, &format!("undo {}", act.id), Some("p-busy"));
    let why = sp.plan.repair_required.clone().expect("an occupied displaced clone blocks the undo");
    assert!(why.contains(x.id.as_str()) && why.contains("active"), "{why}");
    let err = admit_apply(&fx.deps, &caller(None), sp.plan.id.as_str(), Some(&sp.hash), "relay").unwrap_err();
    assert!(err.message.contains("repair"), "{}", err.message);
    assert_eq!(clone_by_id(&fx, &f_clone.id).lifecycle, CloneLifecycle::Retired, "nothing was restored");
    assert_eq!(clone_by_id(&fx, &x.id).lifecycle, CloneLifecycle::Active, "the occupied clone stays");
}

#[test]
fn undo_closing_caller_pane_warns() {
    let fx = fx();
    alpha(&fx);
    let e = MemberId::new();
    create_template(&fx, "solo", &[m(&e, "engineer", "active")]);
    let hydrate = apply_app(&fx, "solo", "one");
    let h = action_of(&fx, &hydrate);
    let seat = seat_named(&fx, "engineer");
    let clone = clones_of(&fx, &seat.id).remove(0);
    patch(&fx, json!({ "clone": clone.id, "pane": "p-me" }));

    let sp = plan_as(&fx, &format!("undo {}", h.id), Some("p-me"));
    assert!(sp.plan.warnings.iter().any(|w| w == CLOSES_PANE_WARNING), "{:?}", sp.plan.warnings);
    assert!(kinds_of(&sp, "seat.retire").contains(&seat.id.to_any()));
    let elsewhere = plan_as(&fx, &format!("undo {}", h.id), Some("p-other"));
    assert!(!elsewhere.plan.warnings.iter().any(|w| w == CLOSES_PANE_WARNING), "a different pane is not closed");
}

// ---------------------------------------------------------------------------------------------
// hydration and template-edit undo
// ---------------------------------------------------------------------------------------------

#[test]
fn hydration_undo_after_later_reuse_keeps_reused_seat() {
    let fx = fx();
    alpha(&fx);
    let (e_auth, reviewer_mem, e_billing) = (MemberId::new(), MemberId::new(), MemberId::new());
    create_template(&fx, "auth-tpl", &[m(&e_auth, "engineer", "deferred"), m(&reviewer_mem, "reviewer", "deferred")]);
    create_template(&fx, "billing-tpl", &[m(&e_billing, "billing-engineer", "deferred")]);
    let hydrate = apply_app(&fx, "auth-tpl", "auth");
    let h = action_of(&fx, &hydrate);
    assert_eq!(h.kind, ActionKind::Hydrate);
    let auth = app(&fx, "auth");
    let engineer = auth.member_map[&e_auth].clone();
    let reviewer = auth.member_map[&reviewer_mem].clone();
    commit(&fx, &format!("application apply billing-tpl --teamspace alpha --name billing --reuse {e_billing}={engineer}"));

    let sp = plan(&fx, &format!("undo {}", h.id));
    assert_eq!(kinds_of(&sp, "seat.retire"), vec![reviewer.to_any()], "only the seat nothing else uses is withdrawn");
    assert_eq!(kinds_of(&sp, "seat.keep"), vec![engineer.to_any()], "the seat billing reused later stays");
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);

    let r = seat_by_id(&fx, &reviewer);
    assert_eq!(r.lifecycle, Lifecycle::Retired);
    assert_eq!(r.retired.unwrap().mechanism, RetireMechanism::Undo, "retirements performed by undo use mechanism undo");
    assert_ne!(seat_by_id(&fx, &engineer).lifecycle, Lifecycle::Retired);
    assert_eq!(app(&fx, "auth").lifecycle, AppLifecycle::Retired);
    assert_eq!(app(&fx, "billing").lifecycle, AppLifecycle::Active);
    let undo = action_of(&fx, &row);
    assert_eq!(undo.kind, ActionKind::Undo);
    assert_eq!(undo.undoes, Some(h.id.clone()));
}

#[test]
fn template_edit_undo_inverse_patch() {
    let fx = fx();
    let e = MemberId::new();
    create_template(&fx, "solo", &[m(&e, "engineer", "deferred")]);
    let before = tpl(&fx, "solo");
    assert_eq!(before.members[0].defaults.model, None);
    let edit = edit_template(&fx, "solo", &[M { model: Some("opus"), ..m(&e, "engineer", "deferred") }]);
    let act = action_of(&fx, &edit);
    assert_eq!(act.kind, ActionKind::TemplateEdit);
    assert_eq!(tpl(&fx, "solo").members[0].defaults.model.as_deref(), Some("opus"));

    let sp = plan(&fx, &format!("undo {}", act.id));
    assert_eq!(kinds_of(&sp, "template.edit").len(), 1, "the inverse patch is a template edit");
    assert!(sp.plan.repair_required.is_none());
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(tpl(&fx, "solo").members[0].defaults.model, None, "the changed field is back to its before-value");
    let undo = action_of(&fx, &row);
    assert_eq!((undo.kind, undo.undoes), (ActionKind::Undo, Some(act.id.clone())));
}

#[test]
fn template_edit_undo_with_conflicting_later_edit_is_repair_required() {
    let fx = fx();
    let e = MemberId::new();
    create_template(&fx, "solo", &[m(&e, "engineer", "deferred")]);
    let first = action_of(&fx, &edit_template(&fx, "solo", &[M { model: Some("opus"), ..m(&e, "engineer", "deferred") }]));
    let second = action_of(&fx, &edit_template(&fx, "solo", &[M { model: Some("sonnet"), ..m(&e, "engineer", "deferred") }]));

    let sp = plan(&fx, &format!("undo {}", first.id));
    let why = sp.plan.repair_required.clone().expect("a later edit of the same field blocks the undo");
    assert!(why.contains(second.id.as_str()), "the conflicting action is named: {why}");
    let err = admit_apply(&fx.deps, &caller(None), sp.plan.id.as_str(), Some(&sp.hash), "relay").unwrap_err();
    assert!(err.message.contains("repair"), "{}", err.message);
    assert_eq!(tpl(&fx, "solo").members[0].defaults.model.as_deref(), Some("sonnet"), "the later edit is untouched");
}

// ---------------------------------------------------------------------------------------------
// candidates and the compensating record
// ---------------------------------------------------------------------------------------------

#[test]
fn candidates_newest_first_with_undone_marked() {
    let fx = fx();
    alpha(&fx);
    let mut acts = Vec::new();
    for name in ["a", "b", "c"] {
        active_seat(&fx, name);
        tick(&fx);
        acts.push(action_of(&fx, &commit(&fx, &format!("seat retire {name}"))));
        tick(&fx);
    }
    let undone = commit(&fx, &format!("undo {}", acts[1].id));
    assert_eq!(action_of(&fx, &undone).kind, ActionKind::Undo);

    let list = list_candidates(&view(&fx), 20).unwrap();
    let ids: Vec<&ActionId> = list.iter().map(|c| &c.act).collect();
    assert_eq!(ids, vec![&acts[2].id, &acts[1].id, &acts[0].id], "newest first; the undo itself is not a candidate");
    assert_eq!(list.iter().map(|c| c.undone).collect::<Vec<_>>(), vec![false, true, false]);
    assert_eq!(list.iter().map(|c| c.index).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert_eq!(list_candidates(&view(&fx), 2).unwrap().len(), 2, "the list is limited");
    let text = render_list(&list);
    assert_eq!(text.matches("[undone]").count(), 1, "{text}");
    assert!(text.lines().nth(1).unwrap().contains("[undone]"), "the undone entry is the second line: {text}");
    assert!(text.contains("a") && text.contains(acts[0].id.as_str()));
}

#[test]
fn undo_writes_compensating_action_and_marks_original_undone() {
    let fx = fx();
    alpha(&fx);
    let seat = active_seat(&fx, "foreman");
    let act = action_of(&fx, &commit(&fx, "seat retire foreman"));
    let rev_before = act.rev;

    let row = commit(&fx, &format!("undo {}", act.id));
    let undo = action_of(&fx, &row);
    assert_eq!(undo.kind, ActionKind::Undo);
    assert_eq!(undo.undoes, Some(act.id.clone()), "kind undo, undoes the original act_");
    assert_eq!(undo.ops, vec![row.op.clone()]);
    assert!(undo.affected.iter().any(|a| a.object == seat.id.to_any()), "the resurrected seat is recorded");
    let orig = action(&fx, &act.id);
    assert_eq!(orig.undone_by, vec![row.op.clone()], "the original action names the undo op");
    assert!(orig.rev > rev_before, "the original record's rev is bumped");

    let again = plan_err(&fx, &format!("undo {}", act.id));
    assert!(again.contains("already undone"), "{again}");
    assert!(plan_err(&fx, &format!("undo {}", undo.id)).contains("cannot be undone"), "an undo is history, not a candidate");
}
