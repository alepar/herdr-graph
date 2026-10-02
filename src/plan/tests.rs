use super::commands::{PlanDeps, admit_apply, create_plan, op_result, register_commands, show_plan};
use super::core_kinds::register_core_kinds;
use super::hash::{canonical_json, plan_hash, same_effects};
use super::kind::{KindRegistry, OrgKind, PlanBody, PlanCx, PlanError, make_plan};
use super::store::PlanStore;
use super::types::{Plan, PlanEffect, PlanRequest, Reserved, StoredPlan};
use crate::daemon::registry::{CallerInfo, CommandCtx, Registry};
use crate::ipc::IpcErrorCode;
use crate::journal::{Journal, OpRow};
use crate::model::action::ActionRecord;
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::clone::CloneRecord;
use crate::model::common::{CloneLifecycle, Lifecycle, RetireMechanism};
use crate::model::operation::OpState;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{AnyId, CommitId, OpId, PlanId, SeatId};
use crate::ports::clock::ManualClock;
use crate::ports::store::{ObjectLocation, Store};
use crate::store::init::init_instance;
use crate::store::layout;
use crate::store::tree::CommitView;
use crate::store::GitStore;
use crate::writer::{MutationCx, MutationError, MutationRegistry, WriterConfig, WriterCore};
use chrono::TimeZone;
use serde_json::json;
use std::sync::Arc;

fn t0() -> crate::model::Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

struct Fx {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
    deps: PlanDeps,
    w: Arc<WriterCore>,
    store: Arc<GitStore>,
}

fn fx() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("inst");
    init_instance(&root).unwrap();
    let mut kinds = KindRegistry::default();
    register_core_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    let plans = Arc::new(PlanStore::new(root.join(".graph-local/plans")));
    let mut reg = MutationRegistry::default();
    kinds.register_mutations(&mut reg, plans.clone());
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let w = WriterCore::new(store.clone(), journal, Arc::new(reg), clock.clone(), WriterConfig::default());
    let deps = PlanDeps {
        kinds,
        plans,
        store: store.clone(),
        writer: w.clone(),
        clock,
        instance: root.clone(),
    };
    Fx { _tmp: tmp, root, deps, w, store }
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn plan(fx: &Fx, change: &str) -> StoredPlan {
    let v = create_plan(&fx.deps, &CallerInfo::default(), words(change)).unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    fx.deps.plans.get(&id).unwrap().unwrap()
}

fn apply_plan(fx: &Fx, sp: &StoredPlan) -> OpRow {
    let op = admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay")
        .unwrap_or_else(|e| panic!("admit: {}", e.message));
    fx.w.drain().unwrap();
    fx.w.journal().get(&op).unwrap().unwrap()
}

/// Plan and apply one change; it must commit.
fn commit(fx: &Fx, change: &str) -> OpRow {
    let sp = plan(fx, change);
    let row = apply_plan(fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{change}: {:?}", row.rejection);
    row
}

fn view(fx: &Fx) -> CommitView<'_> {
    CommitView { store: &*fx.store, at: fx.store.head().unwrap() }
}

fn seat(fx: &Fx, name: &str, lifecycle: Option<Lifecycle>) -> (ObjectLocation, SeatRecord) {
    let mut found: Vec<_> = layout::all_seats(&view(fx))
        .unwrap()
        .into_iter()
        .filter(|(_, s)| s.name == name && lifecycle.is_none_or(|l| s.lifecycle == l))
        .collect();
    assert_eq!(found.len(), 1, "seat {name} {lifecycle:?}: {} matches", found.len());
    found.remove(0)
}

fn teamspace(fx: &Fx, name: &str) -> (ObjectLocation, TeamspaceRecord) {
    let mut found: Vec<_> =
        layout::list_teamspaces(&view(fx)).unwrap().into_iter().filter(|(_, t)| t.name == name).collect();
    assert_eq!(found.len(), 1, "teamspace {name}");
    found.remove(0)
}

fn clones_of(fx: &Fx, seat_id: &SeatId) -> Vec<(ObjectLocation, CloneRecord)> {
    layout::all_clones(&view(fx)).unwrap().into_iter().filter(|(_, c)| &c.seat == seat_id).collect()
}

fn kinds_of(sp: &StoredPlan) -> Vec<String> {
    sp.plan.effects.iter().map(|e| e.kind.clone()).collect()
}

fn count(sp: &StoredPlan, kind: &str) -> usize {
    sp.plan.effects.iter().filter(|e| e.kind == kind).count()
}

fn action_of(fx: &Fx, row: &OpRow) -> ActionRecord {
    let act = row.action.clone().expect("op carries an action");
    let v = view(fx);
    let loc = layout::locate(&v, &act.to_any()).unwrap().expect("action record committed");
    crate::store::record::read_toml(&v, &loc.record_path).unwrap().unwrap()
}

/// teamspace `alpha` (dormant) with a dormant seat `one` ready for use.
fn alpha(fx: &Fx) {
    commit(fx, "teamspace create alpha");
}

// ---------------------------------------------------------------------------------------------
// Step 1: types and hashing
// ---------------------------------------------------------------------------------------------

#[test]
fn canonical_json_sorts_nested_keys() {
    let v: serde_json::Value = serde_json::from_str(r#"{"b":{"z":1,"a":[{"y":2,"x":3}]},"a":null}"#).unwrap();
    assert_eq!(canonical_json(&v), r#"{"a":null,"b":{"a":[{"x":3,"y":2}],"z":1}}"#);
}

fn sample_plan() -> Plan {
    let seat = SeatId::new();
    Plan {
        id: PlanId::new(),
        request: PlanRequest { kind: RequestKind::SeatRetire, args: json!({"seat": "x", "n": 1}) },
        committed_rev: CommitId("a".repeat(40)),
        relied_on: vec![],
        effects: vec![PlanEffect::new("seat.retire", seat, json!({"name": "x", "b": 1, "a": 2}))],
        warnings: vec!["w".into()],
        repair_required: None,
        reserved: Reserved::default(),
    }
}

#[test]
fn plan_hash_stable_across_runs_and_key_order() {
    let p = sample_plan();
    let h1 = plan_hash(&p);
    assert_eq!(h1, plan_hash(&p.clone()));
    assert_eq!(h1.len(), 64);
    let mut q = p.clone();
    q.request.args = serde_json::from_str(r#"{"n":1,"seat":"x"}"#).unwrap();
    q.effects[0].detail = serde_json::from_str(r#"{"a":2,"name":"x","b":1}"#).unwrap();
    assert_eq!(h1, plan_hash(&q), "key order must not matter");
}

#[test]
fn plan_hash_ignores_commit_and_plan_id() {
    let p = sample_plan();
    let mut q = p.clone();
    q.id = PlanId::new();
    q.committed_rev = CommitId("b".repeat(40));
    q.warnings.push("another warning".into());
    q.reserved.0.insert("act".into(), "x".into());
    assert_eq!(plan_hash(&p), plan_hash(&q));
}

#[test]
fn plan_hash_changes_with_effects() {
    let p = sample_plan();
    let mut q = p.clone();
    q.effects[0].detail = json!({"name": "y"});
    assert_ne!(plan_hash(&p), plan_hash(&q));
    let mut r = p.clone();
    r.effects[0].induced = true;
    assert_ne!(plan_hash(&p), plan_hash(&r));
    let mut s = p.clone();
    s.request.args = json!({"seat": "other"});
    assert_ne!(plan_hash(&p), plan_hash(&s));
}

#[test]
fn same_effects_ignores_order() {
    let a = PlanEffect::new("seat.retire", SeatId::new(), json!({}));
    let b = PlanEffect::new("clone.retire", crate::model::CloneId::new(), json!({"n": 1}));
    assert!(same_effects(&[a.clone(), b.clone()], &[b.clone(), a.clone()]));
    assert!(!same_effects(&[a.clone(), b.clone()], std::slice::from_ref(&a)));
    assert!(!same_effects(&[a.clone(), a.clone()], &[a.clone(), b.clone()]), "multiset, not set");
}

#[test]
fn reserved_ids_mint_once_and_survive_roundtrip() {
    let mut r = Reserved::default();
    let a: SeatId = r.get_or_mint("seat");
    assert_eq!(r.get_or_mint::<SeatId>("seat"), a, "second mint must return the same id");
    assert_eq!(r.get::<SeatId>("seat"), Some(a.clone()));
    assert_eq!(r.get::<SeatId>("other"), None);
    let back: Reserved = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back.get::<SeatId>("seat"), Some(a));
}

#[test]
fn effect_describe_marks_induced() {
    let e = PlanEffect::new("seat.retire", SeatId::new(), json!({"name": "foreman"}));
    assert!(e.describe().starts_with("seat.retire st_"));
    assert!(e.describe().contains("name=foreman"));
    assert!(!e.describe().contains("(induced)"));
    assert!(e.induced().describe().ends_with("(induced)"));
}

// ---------------------------------------------------------------------------------------------
// Step 2: store
// ---------------------------------------------------------------------------------------------

#[test]
fn plan_store_roundtrip_and_prune() {
    let tmp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(tmp.path().join("plans"));
    let mk = |at| {
        let p = sample_plan();
        StoredPlan {
            hash: plan_hash(&p),
            plan: p,
            created_at: at,
            caller: CallerInfo::default(),
            requester: Requester::default(),
            supersedes: Some(OpId::new()),
        }
    };
    let old = mk(t0());
    let new = mk(t0() + chrono::Duration::hours(2));
    store.put(&old).unwrap();
    store.put(&new).unwrap();
    assert_eq!(store.get(&old.plan.id).unwrap().unwrap(), old);
    assert_eq!(store.get(&PlanId::new()).unwrap(), None);
    let removed = store.prune_older_than(t0() + chrono::Duration::hours(1)).unwrap();
    assert_eq!(removed, 1);
    assert_eq!(store.get(&old.plan.id).unwrap(), None);
    assert!(store.get(&new.plan.id).unwrap().is_some());
    let leftovers: Vec<_> =
        std::fs::read_dir(store.dir()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    assert_eq!(leftovers, vec![format!("{}.json", new.plan.id)], "no temp files may linger");
}

// ---------------------------------------------------------------------------------------------
// Grammar and registry
// ---------------------------------------------------------------------------------------------

#[test]
fn grammar_flag_positional_and_has() {
    use super::grammar::{flag, has, positional, positionals};
    let w = words("foreman --teamspace alpha --active --model=opus extra");
    assert_eq!(flag(&w, "--teamspace").as_deref(), Some("alpha"));
    assert_eq!(flag(&w, "--model").as_deref(), Some("opus"));
    assert_eq!(flag(&w, "--missing"), None);
    assert!(has(&w, "--active"));
    assert!(!has(&w, "--dormant"));
    assert_eq!(positionals(&w), vec!["foreman", "extra"], "value of --teamspace and boolean --active are not positional");
    assert_eq!(positional(&w, 1).as_deref(), Some("extra"));
    assert_eq!(positional(&w, 2), None);
}

#[test]
fn registry_resolves_noun_verb_and_rejects_unknown() {
    let mut reg = KindRegistry::default();
    register_core_kinds(&mut reg);
    let (k, rest) = reg.resolve(&words("seat create foreman --teamspace a")).unwrap();
    assert_eq!(k.kind(), RequestKind::SeatCreate);
    assert_eq!(rest, words("foreman --teamspace a"));
    let err = reg.resolve(&words("seat explode x")).err().unwrap();
    assert!(err.to_string().contains("seat retire"), "usage lists the known changes: {err}");
    assert!(reg.resolve(&[]).is_err());
}

struct Observer;
impl OrgKind for Observer {
    fn kind(&self) -> RequestKind {
        RequestKind::Observed
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("observed", "x")]
    }
    fn parse(&self, _: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        Ok(json!({}))
    }
    fn plan(&self, _: &PlanCx<'_>, _: &serde_json::Value, _: &mut Reserved) -> Result<PlanBody, PlanError> {
        unreachable!()
    }
    fn mutate(&self, _: &mut MutationCx<'_>, _: &serde_json::Value, _: &Plan) -> Result<crate::writer::Applied, MutationError> {
        unreachable!()
    }
}

#[test]
#[should_panic(expected = "not an organizational kind")]
fn non_organizational_kind_cannot_register() {
    KindRegistry::default().register(Arc::new(Observer));
}

#[test]
#[should_panic(expected = "registered twice")]
fn duplicate_kind_cannot_register() {
    let mut reg = KindRegistry::default();
    register_core_kinds(&mut reg);
    register_core_kinds(&mut reg);
}

#[test]
fn register_mutations_registers_one_key_per_kind() {
    let mut kinds = KindRegistry::default();
    register_core_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    let mut reg = MutationRegistry::default();
    kinds.register_mutations(&mut reg, Arc::new(PlanStore::new("/nonexistent/.graph-local/plans".into())));
    let keys = reg.keys();
    assert_eq!(keys.len(), kinds.kinds().len());
    for k in ["seat_create", "seat_retire", "teamspace_resurrect", "clone_retire"] {
        assert!(keys.contains(&k.to_owned()), "{k} missing from {keys:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// Step 8: engine behavior
// ---------------------------------------------------------------------------------------------

#[test]
fn plan_hash_stable() {
    let fx = fx();
    alpha(&fx);
    let first = plan(&fx, "seat create foreman --teamspace alpha --active --harness shell");
    let (kind, rest) = fx.deps.kinds.resolve(&words("seat create foreman --teamspace alpha --active --harness shell")).unwrap();
    let args = kind.parse(&rest, &CallerInfo::default()).unwrap();
    let head = fx.store.head().unwrap();
    let v = view(&fx);
    let caller = CallerInfo::default();
    let pcx = PlanCx { tree: &v, at: head, caller: &caller, now: t0(), instance: &fx.root };
    let again = make_plan(&*kind, &pcx, args, first.plan.reserved.clone()).unwrap();
    assert_ne!(again.id, first.plan.id, "a recompute is a new plan");
    assert_eq!(plan_hash(&again), first.hash, "same change, same revision, same reserved ids");
    // A fresh plan mints different ids, so its hash differs.
    let other = plan(&fx, "seat create foreman --teamspace alpha --active --harness shell");
    assert_ne!(other.hash, first.hash);
}

#[test]
fn unrelated_commit_does_not_stale() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create one --teamspace alpha");
    commit(&fx, "seat create two --teamspace alpha");
    let pending = plan(&fx, "seat rename one uno");
    commit(&fx, "seat rename two dos");
    let row = apply_plan(&fx, &pending);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(seat(&fx, "uno", None).1.name_history.len(), 1);
    assert_eq!(seat(&fx, "dos", None).1.name, "dos");
}

#[test]
fn same_object_revision_bump_without_effect_change_does_not_stale() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create one --teamspace alpha");
    let pending = plan(&fx, "seat rename one uno");
    let rev_before = seat(&fx, "one", None).1.rev;
    // Two commits on the very same seat that leave it exactly as planned against: rename away and back.
    commit(&fx, "seat rename one tmp");
    commit(&fx, "seat rename tmp one");
    assert!(seat(&fx, "one", None).1.rev > rev_before, "the relied-on object really was revised");
    let row = apply_plan(&fx, &pending);
    assert_eq!(row.state, OpState::Committed, "identical effects apply despite the revision bump: {:?}", row.rejection);
    assert_eq!(seat(&fx, "uno", None).1.name, "uno");
}

#[test]
fn same_object_change_stales() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create one --teamspace alpha");
    let one = seat(&fx, "one", None).1.id;
    let stale = plan(&fx, &format!("seat rename {one} uno"));
    commit(&fx, &format!("seat rename {one} eins"));
    let row = apply_plan(&fx, &stale);
    assert_eq!(row.state, OpState::Rejected);
    let rej = row.rejection.unwrap();
    assert_eq!(rej.reason, "stale_plan");
    let new_id = rej.explanation.split("new plan ").nth(1).unwrap().split_whitespace().next().unwrap();
    let new_plan = fx.deps.plans.get(&new_id.parse().unwrap()).unwrap().expect("replacement plan stored");
    assert_ne!(new_plan.plan.id, stale.plan.id);
    assert_ne!(new_plan.hash, stale.hash);
    assert!(rej.explanation.contains(&new_plan.hash), "{}", rej.explanation);
    assert_eq!(seat(&fx, "eins", None).1.name, "eins", "the stale apply must not have renamed anything");
    // The replacement plan is itself applicable.
    let row = apply_plan(&fx, &new_plan);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(seat(&fx, "uno", None).1.name, "uno");
}

#[test]
fn plan_whose_name_no_longer_resolves_is_stale_not_applied() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create one --teamspace alpha");
    let pending = plan(&fx, "seat retire one");
    commit(&fx, "seat rename one eins");
    let row = apply_plan(&fx, &pending);
    assert_eq!(row.state, OpState::Rejected);
    let rej = row.rejection.unwrap();
    assert_eq!(rej.reason, "stale_plan");
    assert!(rej.explanation.contains("no seat named"), "{}", rej.explanation);
    assert_eq!(seat(&fx, "eins", None).1.lifecycle, Lifecycle::Dormant, "nothing was retired");
}

#[test]
fn effects_change_without_relied_on_change_stales() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create one --teamspace alpha");
    let pending = plan(&fx, "teamspace retire alpha");
    let ts_rev = teamspace(&fx, "alpha").1.rev;
    commit(&fx, "seat create two --teamspace alpha");
    assert_eq!(teamspace(&fx, "alpha").1.rev, ts_rev, "the teamspace record itself is untouched");
    let row = apply_plan(&fx, &pending);
    assert_eq!(row.state, OpState::Rejected);
    let rej = row.rejection.unwrap();
    assert_eq!(rej.reason, "stale_plan");
    assert!(rej.explanation.contains("seat.retire"), "diff names the new seat effect: {}", rej.explanation);
    assert_eq!(teamspace(&fx, "alpha").1.lifecycle, Lifecycle::Dormant);
}

#[test]
fn create_plan_reuses_reserved_ids() {
    let fx = fx();
    alpha(&fx);
    let pending = plan(&fx, "seat create foreman --teamspace alpha --active --harness shell");
    let planned_seat: SeatId = pending.plan.reserved.get("seat").unwrap();
    let planned_clone: crate::model::CloneId = pending.plan.reserved.get("clone:0").unwrap();
    commit(&fx, "seat create unrelated --teamspace alpha");
    let row = apply_plan(&fx, &pending);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let (_, rec) = seat(&fx, "foreman", None);
    assert_eq!(rec.id, planned_seat);
    assert_eq!(clones_of(&fx, &rec.id).into_iter().map(|(_, c)| c.id).collect::<Vec<_>>(), vec![planned_clone]);
}

fn active_seat_with_two_clones(fx: &Fx) {
    alpha(fx);
    commit(fx, "seat create foreman --teamspace alpha --active --harness shell");
    commit(fx, "clone add foreman --name second");
}

#[test]
fn retire_seat_plan_lists_clones_and_closes_tab() {
    let fx = fx();
    active_seat_with_two_clones(&fx);
    let (_, rec) = seat(&fx, "foreman", None);
    let sp = plan(&fx, "seat retire foreman");
    assert_eq!(count(&sp, "seat.retire"), 1);
    assert_eq!(count(&sp, "clone.retire"), 2, "{:?}", kinds_of(&sp));
    assert_eq!(count(&sp, "runtime.close_tab"), 1);
    let listed: Vec<AnyId> = sp.plan.effects.iter().filter(|e| e.kind == "clone.retire").map(|e| e.object.clone()).collect();
    for (_, c) in clones_of(&fx, &rec.id) {
        assert!(listed.contains(&c.id.to_any()), "clone {} missing from plan", c.id);
    }
    assert!(sp.plan.effects.iter().all(|e| !e.induced), "an explicit retire has no induced effects");
}

#[test]
fn retire_seat_writes_action_record_with_retired_and_already_retired() {
    let fx = fx();
    active_seat_with_two_clones(&fx);
    let (_, rec) = seat(&fx, "foreman", None);
    let first = clones_of(&fx, &rec.id).into_iter().find(|(_, c)| c.name == "foreman").unwrap().1;
    let second = clones_of(&fx, &rec.id).into_iter().find(|(_, c)| c.name == "second").unwrap().1;
    commit(&fx, &format!("clone retire {}", first.id));
    let row = commit(&fx, "seat retire foreman");
    let act = action_of(&fx, &row);
    assert_eq!(act.kind, crate::model::action::ActionKind::Retire);
    assert_eq!(act.ops, vec![row.op.clone()]);
    let mut retired = act.retired.clone();
    retired.sort();
    let mut want = vec![rec.id.to_any(), second.id.to_any()];
    want.sort();
    assert_eq!(retired, want);
    assert_eq!(act.already_retired, vec![first.id.to_any()]);
    let (_, after) = seat(&fx, "foreman", Some(Lifecycle::Retired));
    let r = after.retired.unwrap();
    assert_eq!(r.action, Some(act.id.clone()));
    assert_eq!(r.op, row.op);
    assert_eq!(r.mechanism, RetireMechanism::AgentRequest, "plan came from a non-human caller");
    assert_eq!(row.action, Some(act.id));
}

#[test]
fn retire_seat_moves_folder_to_archive() {
    let fx = fx();
    active_seat_with_two_clones(&fx);
    let (loc, rec) = seat(&fx, "foreman", None);
    assert!(loc.folder.as_str().ends_with("/seats/foreman"), "{}", loc.folder.as_str());
    commit(&fx, "seat retire foreman");
    let (after, _) = seat(&fx, "foreman", Some(Lifecycle::Retired));
    let want = format!("teamspaces/alpha/archive/seats/foreman-{}", rec.id.suffix6().to_ascii_lowercase());
    assert_eq!(after.folder.as_str(), want);
    let v = view(&fx);
    assert!(v.store.list_dir(&v.at, &crate::ports::store::RepoPath::new("teamspaces/alpha/seats/foreman").unwrap()).unwrap().is_empty());
    for (cloc, c) in clones_of(&fx, &rec.id) {
        assert!(cloc.record_path.as_str().starts_with(&want), "clone {} did not move with its seat", c.id);
    }
    // The name is free for reuse.
    commit(&fx, "seat create foreman --teamspace alpha");
    assert_eq!(seat(&fx, "foreman", Some(Lifecycle::Dormant)).0.folder.as_str(), "teamspaces/alpha/seats/foreman");
}

#[test]
fn resurrect_restores_ids() {
    let fx = fx();
    active_seat_with_two_clones(&fx);
    let (_, rec) = seat(&fx, "foreman", None);
    let all: Vec<_> = clones_of(&fx, &rec.id).into_iter().map(|(_, c)| c).collect();
    let (early, late) = (all.iter().find(|c| c.name == "foreman").unwrap(), all.iter().find(|c| c.name == "second").unwrap());
    commit(&fx, &format!("clone retire {}", early.id));
    let retire = commit(&fx, "seat retire foreman");

    let sp = plan(&fx, &format!("seat resurrect {}", rec.id));
    assert_eq!(count(&sp, "clone.resurrect"), 1, "only the clone retired by the same act: {:?}", kinds_of(&sp));
    let res = apply_plan(&fx, &sp);
    assert_eq!(res.state, OpState::Committed, "{:?}", res.rejection);

    let (loc, back) = seat(&fx, "foreman", None);
    assert_eq!(back.id, rec.id);
    assert_eq!(back.lifecycle, Lifecycle::Dormant);
    assert!(back.retired.is_none());
    assert_eq!(loc.folder.as_str(), "teamspaces/alpha/seats/foreman", "folder back at a live path");
    let clones = clones_of(&fx, &rec.id);
    let by_id = |id: &crate::model::CloneId| clones.iter().find(|(_, c)| &c.id == id).map(|(_, c)| c.clone()).unwrap();
    assert_eq!(by_id(&late.id).lifecycle, CloneLifecycle::Active);
    assert!(by_id(&late.id).retired.is_none());
    assert_eq!(by_id(&early.id).lifecycle, CloneLifecycle::Retired, "an already-retired clone stays retired");
    let act = action_of(&fx, &res);
    assert_eq!(act.kind, crate::model::action::ActionKind::Resurrect);
    assert_eq!(act.compensation.get("resurrects").and_then(|v| v.as_str()), retire.action.as_ref().map(|a| a.as_str()));
}

#[test]
fn resurrect_active_opens_runtime() {
    let fx = fx();
    active_seat_with_two_clones(&fx);
    commit(&fx, "seat retire foreman");
    let (_, rec) = seat(&fx, "foreman", None);
    let sp = plan(&fx, &format!("seat resurrect {} --active", rec.id));
    assert_eq!(count(&sp, "runtime.open_tab"), 1);
    assert_eq!(count(&sp, "clone.resurrect"), 2);
    assert_eq!(count(&sp, "runtime.open_pane"), 2);
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let (_, back) = seat(&fx, "foreman", None);
    assert_eq!(back.lifecycle, Lifecycle::Active);
    assert_eq!(back.activation.last_op, Some(row.op));
}

#[test]
fn retire_last_clone_includes_induced_seat_retirement_and_warning() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create foreman --teamspace alpha --active --harness shell");
    let (_, rec) = seat(&fx, "foreman", None);
    let only = clones_of(&fx, &rec.id).remove(0).1;
    let sp = plan(&fx, &format!("clone retire {}", only.id));
    let induced: Vec<&PlanEffect> = sp.plan.effects.iter().filter(|e| e.induced).collect();
    assert!(induced.iter().any(|e| e.kind == "seat.retire" && e.object == rec.id.to_any()), "{:?}", kinds_of(&sp));
    assert!(induced.iter().any(|e| e.kind == "runtime.close_tab"));
    assert!(sp.plan.effects.iter().any(|e| e.kind == "clone.retire" && !e.induced));
    assert_eq!(sp.plan.warnings.len(), 1);
    assert!(sp.plan.warnings[0].contains("seat deactivate"), "{:?}", sp.plan.warnings);

    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(seat(&fx, "foreman", Some(Lifecycle::Retired)).1.id, rec.id);
    let act = action_of(&fx, &row);
    assert!(act.retired.contains(&rec.id.to_any()) && act.retired.contains(&only.id.to_any()), "one act covers both");
}

#[test]
fn retire_non_last_clone_leaves_seat_alone() {
    let fx = fx();
    active_seat_with_two_clones(&fx);
    let (_, rec) = seat(&fx, "foreman", None);
    let second = clones_of(&fx, &rec.id).into_iter().find(|(_, c)| c.name == "second").unwrap().1;
    let sp = plan(&fx, &format!("clone retire {}", second.id));
    assert!(sp.plan.warnings.is_empty());
    assert_eq!(kinds_of(&sp), vec!["clone.retire", "runtime.close_pane"]);
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(seat(&fx, "foreman", None).1.lifecycle, Lifecycle::Active);
    let act = action_of(&fx, &row);
    assert_eq!(act.retired, vec![second.id.to_any()]);
}

#[test]
fn deactivate_keeps_clones_active_runtime_absent() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create foreman --teamspace alpha --active --harness shell");
    let (_, rec) = seat(&fx, "foreman", None);
    let before = clones_of(&fx, &rec.id).remove(0).1;
    let sp = plan(&fx, "seat deactivate foreman");
    assert_eq!(kinds_of(&sp), vec!["seat.deactivate", "runtime.close_tab"], "no clone effects");
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let (_, after) = seat(&fx, "foreman", None);
    assert_eq!(after.lifecycle, Lifecycle::Dormant);
    assert_eq!(after.activation.last_op, Some(row.op));
    let clone = clones_of(&fx, &rec.id).remove(0).1;
    assert_eq!(clone.lifecycle, CloneLifecycle::Active);
    assert_eq!(clone.runtime.availability, crate::model::common::Availability::Absent);
    assert_eq!(clone.rev, before.rev, "the clone record is untouched");
}

#[test]
fn rename_writes_name_history_and_moves_folder() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create foreman --teamspace alpha --active --harness shell");
    let sp = plan(&fx, "seat rename foreman chief");
    assert_eq!(kinds_of(&sp), vec!["seat.rename", "runtime.rename_tab"]);
    let d = &sp.plan.effects[0].detail;
    assert_eq!((d["from"].as_str(), d["to"].as_str()), (Some("foreman"), Some("chief")));
    assert_eq!(d["path_to"], "teamspaces/alpha/seats/chief");
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let (loc, rec) = seat(&fx, "chief", None);
    assert_eq!(loc.folder.as_str(), "teamspaces/alpha/seats/chief");
    assert_eq!(rec.name_history.len(), 1);
    let h = &rec.name_history[0];
    assert_eq!((h.old.as_str(), h.new.as_str()), ("foreman", "chief"));
    assert_eq!(h.source, crate::model::common::NameSource::Request);
    assert_eq!(h.observed_at, t0());
    assert!(h.event_at.is_none(), "event time is never invented");
    assert_eq!(clones_of(&fx, &rec.id).len(), 1);
    assert!(clones_of(&fx, &rec.id)[0].0.record_path.as_str().starts_with("teamspaces/alpha/seats/chief/clones/"));
}

#[test]
fn rename_to_same_name_is_rejected_at_plan_time() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create foreman --teamspace alpha");
    let err = create_plan(&fx.deps, &CallerInfo::default(), words("seat rename foreman foreman")).unwrap_err();
    assert_eq!(err.code, IpcErrorCode::BadRequest);
}

#[test]
fn teamspace_retire_includes_dormant_seats() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create dormant --teamspace alpha");
    commit(&fx, "seat create live --teamspace alpha --active --harness shell");
    let sp = plan(&fx, "teamspace retire alpha");
    assert_eq!(count(&sp, "teamspace.retire"), 1);
    assert_eq!(count(&sp, "seat.retire"), 2, "dormant seats are retired too: {:?}", kinds_of(&sp));
    assert_eq!(count(&sp, "clone.retire"), 1);
    assert_eq!(count(&sp, "runtime.close_workspace"), 1, "the active seat made the teamspace active");
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let (loc, ts) = teamspace(&fx, "alpha");
    assert_eq!(ts.lifecycle, Lifecycle::Retired);
    assert!(loc.folder.as_str().starts_with("archive/teamspaces/alpha-"), "{}", loc.folder.as_str());
    let act = action_of(&fx, &row);
    assert_eq!(act.retired.len(), 4, "teamspace + two seats + one clone: {:?}", act.retired);
    for name in ["dormant", "live"] {
        assert_eq!(seat(&fx, name, None).1.lifecycle, Lifecycle::Retired);
    }

    // And back: only what that act retired.
    let sp = plan(&fx, &format!("teamspace resurrect {}", ts.id));
    assert_eq!(count(&sp, "seat.resurrect"), 2);
    assert_eq!(count(&sp, "clone.resurrect"), 1);
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let (loc, back) = teamspace(&fx, "alpha");
    assert_eq!((back.id.clone(), back.lifecycle), (ts.id, Lifecycle::Dormant));
    assert_eq!(loc.folder.as_str(), "teamspaces/alpha");
    for name in ["dormant", "live"] {
        let (sloc, s) = seat(&fx, name, None);
        assert_eq!(s.lifecycle, Lifecycle::Dormant, "resurrected seats come back dormant");
        assert!(sloc.folder.as_str().starts_with("teamspaces/alpha/seats/"), "{}", sloc.folder.as_str());
    }
}

#[test]
fn teamspace_resurrect_leaves_earlier_retired_seats_retired() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create early --teamspace alpha");
    commit(&fx, "seat create late --teamspace alpha");
    commit(&fx, "seat retire early");
    commit(&fx, "teamspace retire alpha");
    let (_, ts) = teamspace(&fx, "alpha");
    let sp = plan(&fx, &format!("teamspace resurrect {}", ts.id));
    assert_eq!(count(&sp, "seat.resurrect"), 1);
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(seat(&fx, "late", None).1.lifecycle, Lifecycle::Dormant);
    assert_eq!(seat(&fx, "early", None).1.lifecycle, Lifecycle::Retired);
}

#[test]
fn activate_seat_in_dormant_teamspace_induces_teamspace_activation() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create foreman --teamspace alpha --harness shell");
    let (_, ts) = teamspace(&fx, "alpha");
    assert_eq!(ts.lifecycle, Lifecycle::Dormant);
    let sp = plan(&fx, "seat activate foreman");
    let induced: Vec<_> = sp.plan.effects.iter().filter(|e| e.induced).map(|e| (e.kind.as_str(), e.object.clone())).collect();
    assert_eq!(induced, vec![("teamspace.activate", ts.id.to_any())]);
    assert_eq!(count(&sp, "clone.add"), 1, "a seat with no clone gets one");
    assert_eq!(count(&sp, "runtime.open_tab"), 1);
    assert_eq!(count(&sp, "runtime.open_pane"), 1);
    assert_eq!(count(&sp, "runtime.start_agent"), 0, "shell harness starts no agent");
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(teamspace(&fx, "alpha").1.lifecycle, Lifecycle::Active);
    let (_, s) = seat(&fx, "foreman", None);
    assert_eq!((s.lifecycle, s.activation.last_op.clone()), (Lifecycle::Active, Some(row.op)));
    assert_eq!(clones_of(&fx, &s.id).len(), 1);
}

#[test]
fn seat_create_active_starts_agent_for_agent_harness() {
    let fx = fx();
    commit(&fx, "teamspace create alpha --active");
    let sp = plan(&fx, "seat create foreman --teamspace alpha --active --harness codex --model gpt-x");
    assert_eq!(count(&sp, "runtime.start_agent"), 1);
    let start = sp.plan.effects.iter().find(|e| e.kind == "runtime.start_agent").unwrap();
    assert_eq!(start.detail["harness"], "codex");
    assert_eq!(start.detail["model"], "gpt-x");
    assert_eq!(count(&sp, "teamspace.activate"), 0, "the teamspace is already active");
}

#[test]
fn teamspace_create_active_opens_workspace() {
    let fx = fx();
    let sp = plan(&fx, "teamspace create alpha --active --project-repo /tmp/repo");
    assert_eq!(kinds_of(&sp), vec!["teamspace.create", "runtime.open_workspace"]);
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let (loc, ts) = teamspace(&fx, "alpha");
    assert_eq!((loc.folder.as_str(), ts.lifecycle), ("teamspaces/alpha", Lifecycle::Active));
    assert_eq!(ts.project_repo, Some(std::path::PathBuf::from("/tmp/repo")));
}

#[test]
fn seat_name_ambiguity_is_reported_not_guessed() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(&fx, "teamspace create beta");
    commit(&fx, "seat create twin --teamspace alpha");
    commit(&fx, "seat create twin --teamspace beta");
    let err = create_plan(&fx.deps, &CallerInfo::default(), words("seat rename twin other")).unwrap_err();
    assert_eq!(err.code, IpcErrorCode::BadRequest);
    assert!(err.message.contains("ambiguous") && err.message.contains("st_"), "{}", err.message);
}

#[test]
fn seat_create_unknown_teamspace_and_harness_are_rejected_at_plan_time() {
    let fx = fx();
    alpha(&fx);
    assert!(create_plan(&fx.deps, &CallerInfo::default(), words("seat create x --teamspace nowhere")).is_err());
    assert!(create_plan(&fx.deps, &CallerInfo::default(), words("seat create x --teamspace alpha --harness vim")).is_err());
    assert!(create_plan(&fx.deps, &CallerInfo::default(), words("seat create x")).is_err());
}

#[test]
fn confirmation_mismatch_rejected() {
    let fx = fx();
    alpha(&fx);
    let sp = plan(&fx, "seat create foreman --teamspace alpha");
    // Through the command layer a wrong hash never reaches the writer.
    let err = admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some("deadbeef"), "relay").unwrap_err();
    assert_eq!(err.code, IpcErrorCode::BadRequest);
    // A request that bypasses the command layer is still refused by the writer.
    let mut args = sp.plan.request.args.as_object().unwrap().clone();
    args.insert("_plan".into(), json!(sp.plan.id));
    args.insert(
        "_confirmation".into(),
        json!({"mode": "relay", "plan_hash": "deadbeef", "at": "2026-10-02T12:00:00Z"}),
    );
    let op = fx
        .deps
        .writer
        .admit(ChangeRequest {
            kind: sp.plan.request.kind,
            args: args.into(),
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
        })
        .unwrap();
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(row.state, OpState::Rejected);
    assert_eq!(row.rejection.unwrap().reason, "confirmation_mismatch");
    assert!(layout::all_seats(&view(&fx)).unwrap().is_empty(), "nothing was created");
}

#[test]
fn writer_rejects_unknown_plan_and_tampered_args() {
    let fx = fx();
    alpha(&fx);
    let sp = plan(&fx, "seat create foreman --teamspace alpha");
    let admit = |args: serde_json::Value| {
        let op = fx
            .deps
            .writer
            .admit(ChangeRequest {
                kind: RequestKind::SeatCreate,
                args,
                relied_on: vec![],
                requester: Requester::default(),
                supersedes: None,
            })
            .unwrap();
        fx.w.drain().unwrap();
        fx.w.journal().get(&op).unwrap().unwrap().rejection.unwrap().reason
    };
    let confirmation = json!({"mode": "relay", "plan_hash": sp.hash, "at": "2026-10-02T12:00:00Z"});
    assert_eq!(admit(json!({"name": "x", "teamspace": "alpha"})), "unknown_plan");
    assert_eq!(
        admit(json!({"_plan": PlanId::new(), "_confirmation": confirmation, "name": "x", "teamspace": "alpha"})),
        "unknown_plan"
    );
    let mut tampered = sp.plan.request.args.clone();
    tampered["name"] = json!("evil");
    tampered["_plan"] = json!(sp.plan.id);
    tampered["_confirmation"] = confirmation;
    assert_eq!(admit(tampered), "confirmation_mismatch", "confirmed plan, different request");
}

#[tokio::test]
async fn apply_without_confirmation_is_bad_request() {
    let fx = fx();
    alpha(&fx);
    let sp = plan(&fx, "seat create foreman --teamspace alpha");
    let err = admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), None, "relay").unwrap_err();
    assert_eq!((err.code, err.message.contains("confirmation required")), (IpcErrorCode::BadRequest, true));
    // tty confirmation cannot be claimed from a non-terminal caller.
    let err = admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "tty").unwrap_err();
    assert_eq!(err.code, IpcErrorCode::BadRequest);
    // Same refusal through the registered IPC handler.
    let mut reg = Registry::default();
    register_commands(&mut reg, fx.deps_clone());
    let h = reg.handler("plan.apply").unwrap();
    let cx = CommandCtx { request_id: "r".into(), caller: CallerInfo::default() };
    let err = h.call(cx, json!({"plan": sp.plan.id})).await.unwrap_err();
    assert_eq!((err.code, err.message.contains("confirmation required")), (IpcErrorCode::BadRequest, true));
    assert!(layout::all_seats(&view(&fx)).unwrap().is_empty());
}

impl Fx {
    fn deps_clone(&self) -> PlanDeps {
        PlanDeps {
            kinds: self.deps.kinds.clone(),
            plans: self.deps.plans.clone(),
            store: self.deps.store.clone(),
            writer: self.deps.writer.clone(),
            clock: self.deps.clock.clone(),
            instance: self.deps.instance.clone(),
        }
    }
}

#[tokio::test]
async fn ipc_handlers_create_show_and_apply_end_to_end() {
    let fx = fx();
    alpha(&fx);
    let mut reg = Registry::default();
    register_commands(&mut reg, fx.deps_clone());
    let cx = || CommandCtx { request_id: "r".into(), caller: CallerInfo::default() };
    let created = reg
        .handler("plan.create")
        .unwrap()
        .call(cx(), json!({"words": ["seat", "create", "foreman", "--teamspace", "alpha"]}))
        .await
        .unwrap();
    let (id, hash) = (created["plan_id"].as_str().unwrap().to_owned(), created["hash"].as_str().unwrap().to_owned());
    assert!(created["rendered"].as_str().unwrap().contains("seat.create"));
    let shown = show_plan(&fx.deps, &id).unwrap();
    assert_eq!(shown["hash"], created["hash"]);

    // The daemon's writer loop is the thing that drains; run it for the wait inside plan.apply.
    let (tx, rx) = tokio::sync::watch::channel(false);
    let runner = tokio::spawn(fx.w.clone().run(rx));
    let done = reg
        .handler("plan.apply")
        .unwrap()
        .call(cx(), json!({"plan": id, "confirm": hash, "mode": "relay"}))
        .await
        .unwrap();
    let _ = tx.send(true);
    let _ = runner.await;
    assert_eq!(done["state"], "committed", "{done}");
    assert!(done["commit"].is_string());
    assert_eq!(seat(&fx, "foreman", None).1.name, "foreman");
    let op: OpId = done["op"].as_str().unwrap().parse().unwrap();
    assert_eq!(op_result(&fx.deps, &op).unwrap()["state"], "committed");
}

#[test]
fn rejected_apply_reports_rejection_through_op_result() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create one --teamspace alpha");
    let one = seat(&fx, "one", None).1.id;
    let stale = plan(&fx, &format!("seat rename {one} uno"));
    commit(&fx, &format!("seat rename {one} eins"));
    let op = admit_apply(&fx.deps, &CallerInfo::default(), stale.plan.id.as_str(), Some(&stale.hash), "relay").unwrap();
    fx.w.drain().unwrap();
    let v = op_result(&fx.deps, &op).unwrap();
    assert_eq!(v["state"], "rejected");
    assert_eq!(v["rejection"]["reason"], "stale_plan");
    assert!(v["rejection"]["explanation"].as_str().unwrap().contains("new plan pl_"));
}

#[test]
fn supersedes_flag_carried_into_change_request() {
    let fx = fx();
    alpha(&fx);
    let earlier = OpId::new();
    let v = create_plan(
        &fx.deps,
        &CallerInfo::default(),
        words(&format!("seat create foreman --supersedes {earlier} --teamspace alpha")),
    )
    .unwrap();
    let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    let sp = fx.deps.plans.get(&id).unwrap().unwrap();
    assert_eq!(sp.supersedes, Some(earlier.clone()));
    assert!(!sp.plan.request.args.to_string().contains("supersedes"), "the flag is removed before kind parsing");
    let op = admit_apply(&fx.deps, &CallerInfo::default(), id.as_str(), Some(&sp.hash), "relay").unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(row.request.supersedes, Some(earlier));
    assert_eq!(row.request.kind, RequestKind::SeatCreate);
    assert!(row.request.relied_on.is_empty());
    assert_eq!(row.request.args["_confirmation"]["plan_hash"], json!(sp.hash));
}

#[test]
fn retire_mechanism_follows_requester() {
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create foreman --teamspace alpha");
    let sp = plan(&fx, "seat retire foreman");
    let op = admit_apply(&fx.deps, &CallerInfo { tty: true, ..Default::default() }, sp.plan.id.as_str(), Some(&sp.hash), "tty").unwrap();
    fx.w.drain().unwrap();
    assert_eq!(fx.w.journal().get(&op).unwrap().unwrap().state, OpState::Committed);
    let r = seat(&fx, "foreman", Some(Lifecycle::Retired)).1.retired.unwrap();
    assert_eq!(r.mechanism, RetireMechanism::UserCli);
    let committed_op = fx.w.journal().get(&op).unwrap().unwrap();
    assert!(committed_op.request.requester.human);
}

#[test]
fn retire_template_member_seat_adds_exclusion_to_each_application() {
    use crate::model::application::ApplicationRecord;
    use crate::model::common::AppLifecycle;
    use crate::model::seat::TemplateRef;
    use crate::model::{AppId, MemberId, TemplateId};
    use std::collections::BTreeMap;
    let fx = fx();
    alpha(&fx);
    commit(&fx, "seat create member --teamspace alpha");
    let (loc, rec) = seat(&fx, "member", None);
    let (app_id, member) = (AppId::new(), MemberId::new());
    // Task 9 owns application writes; seed the records directly in one commit via a bookkeeping mutation.
    struct Seed(SeatRecord, std::path::PathBuf, ApplicationRecord, crate::ports::store::RepoPath);
    impl crate::writer::Mutation for Seed {
        fn apply(&self, cx: &mut MutationCx<'_>) -> Result<crate::writer::Applied, MutationError> {
            let mut s = self.0.clone();
            cx.tree.put_record(self.3.clone(), &mut s)?;
            let mut a = self.2.clone();
            cx.tree.put_record(layout::application_record(&a.id), &mut a)?;
            let _ = &self.1;
            Ok(crate::writer::Applied { summary: "seed".into(), action: None })
        }
    }
    let mut seeded = rec.clone();
    seeded.template_ref = Some(TemplateRef { template: TemplateId::new(), member: member.clone() });
    seeded.applications = vec![app_id.clone()];
    let app = ApplicationRecord {
        schema: 1,
        id: app_id.clone(),
        rev: 0,
        name: "app".into(),
        template: TemplateId::new(),
        teamspace: rec.teamspace.clone(),
        lifecycle: AppLifecycle::Active,
        retired: None,
        member_map: BTreeMap::from([(member.clone(), rec.id.clone())]),
        additions: vec![],
        exclusions: vec![],
        reused: vec![],
        contributions: Default::default(),
        created_by: crate::model::application::CreatedBy { op: OpId::new(), action: crate::model::ActionId::new() },
    };
    let mut reg = MutationRegistry::default();
    reg.register("bookkeeping.seed", Arc::new(Seed(seeded, fx.root.clone(), app, loc.record_path.clone())));
    let mut kinds = KindRegistry::default();
    register_core_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    kinds.register_mutations(&mut reg, fx.deps.plans.clone());
    let w = WriterCore::new(
        fx.store.clone(),
        Arc::new(Journal::open(&Journal::path_in(&fx.root)).unwrap()),
        Arc::new(reg),
        Arc::new(ManualClock::new(t0() + chrono::Duration::minutes(5))),
        WriterConfig::default(),
    );
    let seed_op = crate::ports::writer::Writer::admit(
        &*w,
        ChangeRequest {
            kind: RequestKind::Bookkeeping,
            args: json!({"sub": "seed"}),
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
        },
    )
    .unwrap();
    w.drain().unwrap();
    assert_eq!(w.journal().get(&seed_op).unwrap().unwrap().state, OpState::Committed);

    let sp = plan(&fx, "seat retire member");
    let ex: Vec<_> = sp.plan.effects.iter().filter(|e| e.kind == "exclusion.add").collect();
    assert_eq!(ex.len(), 1, "{:?}", kinds_of(&sp));
    assert_eq!(ex[0].object, app_id.to_any());
    assert_eq!(ex[0].detail["member"], json!(member));
    let op = admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay").unwrap();
    w.drain().unwrap();
    assert_eq!(w.journal().get(&op).unwrap().unwrap().state, OpState::Committed);
    let v = view(&fx);
    let loc = layout::locate(&v, &app_id.to_any()).unwrap().unwrap();
    let after: ApplicationRecord = crate::store::record::read_toml(&v, &loc.record_path).unwrap().unwrap();
    assert_eq!(after.exclusions, vec![member]);
}

#[test]
fn plan_cli_non_tty_never_applies() {
    use crate::cli::plan::{Decision, decide};
    assert_eq!(decide(false, false), Decision::PrintOnly);
    assert_eq!(decide(false, true), Decision::Json);
    assert_eq!(decide(true, true), Decision::Json, "--json never applies, even on a terminal");
    assert_eq!(decide(true, false), Decision::Prompt);
}

#[test]
fn render_lists_effects_warnings_and_repair() {
    let mut p = sample_plan();
    p.warnings = vec!["careful".into()];
    p.repair_required = Some("narrow the request".into());
    p.effects.push(PlanEffect::new("runtime.close_tab", SeatId::new(), json!({})).induced());
    let text = super::commands::render(&p, "abc123");
    assert!(text.contains(p.id.as_str()) && text.contains("abc123"));
    assert!(text.contains("1. seat.retire") && text.contains("2. runtime.close_tab") && text.contains("(induced)"));
    assert!(text.contains("careful") && text.contains("narrow the request"));
}

// ---------------------------------------------------------------------------------------------
// Step 4: effective config
// ---------------------------------------------------------------------------------------------

mod effective {
    use crate::model::clone::CloneRecord;
    use crate::model::common::{Availability, CloneLifecycle, Occupant, Role, Runtime};
    use crate::model::effective::{EffectiveSeatConfig, resolve, session_replacements};
    use crate::model::graph::GraphDefaults;
    use crate::model::harness::Harness;
    use crate::model::seat::{InstructionSection, SeatRecord};
    use crate::model::template::{MemberDefaults, Startup, TemplateMember, TemplateRecord};
    use crate::model::{CloneId, MemberId, NsId, SCHEMA_VERSION, SeatId, TeamspaceId, TemplateId};
    use super::t0;

    fn seat() -> SeatRecord {
        SeatRecord {
            schema: SCHEMA_VERSION,
            id: SeatId::new(),
            rev: 1,
            name: "s".into(),
            name_history: vec![],
            teamspace: TeamspaceId::new(),
            lifecycle: crate::model::common::Lifecycle::Active,
            retired: None,
            role: None,
            template_ref: None,
            applications: vec![],
            overrides: Default::default(),
            participation: Default::default(),
            activation: Default::default(),
            runtime: Default::default(),
            channel: Default::default(),
            reload_required: false,
            moved_out: false,
        }
    }

    fn template(defaults: MemberDefaults) -> TemplateRecord {
        TemplateRecord {
            schema: SCHEMA_VERSION,
            id: TemplateId::new(),
            rev: 1,
            name: "t".into(),
            name_history: vec![],
            defaults,
            members: vec![],
            relationships: vec![],
            copied_from: None,
        }
    }

    fn member(defaults: MemberDefaults, role: Option<Role>) -> TemplateMember {
        TemplateMember {
            id: MemberId::new(),
            name: "m".into(),
            role_ref: None,
            role,
            startup: Startup::Active,
            defaults,
        }
    }

    fn defaults(h: Option<Harness>, model: Option<&str>, args: Option<&[&str]>, summaries: Option<bool>) -> MemberDefaults {
        MemberDefaults {
            harness: h,
            model: model.map(str::to_owned),
            args: args.map(|a| a.iter().map(|s| s.to_string()).collect()),
            summaries,
        }
    }

    #[test]
    fn precedence_seat_over_member_over_template_over_graph() {
        let graph = GraphDefaults { harness: Some(Harness::Shell), model: Some("graph-model".into()) };
        let tpl = template(defaults(Some(Harness::Codex), Some("tpl-model"), Some(&["--tpl"]), Some(false)));
        let mem = member(defaults(None, Some("mem-model"), None, None), None);
        let mut s = seat();

        // graph < template: the template harness wins; member beats template for model; args come from the template.
        let c = resolve(&graph, Some(&tpl), Some(&mem), &s);
        assert_eq!(c.harness, Harness::Codex);
        assert_eq!(c.model.as_deref(), Some("mem-model"));
        assert_eq!(c.args, vec!["--tpl"]);
        assert!(!c.summaries, "template default summaries=false applies");

        // member harness beats template harness.
        let mem2 = member(defaults(Some(Harness::Claude), None, Some(&["--mem"]), None), None);
        let c = resolve(&graph, Some(&tpl), Some(&mem2), &s);
        assert_eq!((c.harness, c.model.as_deref(), c.args), (Harness::Claude, Some("tpl-model"), vec!["--mem".to_string()]));

        // seat overrides beat everything.
        s.overrides.harness = Some(Harness::Shell);
        s.overrides.model = Some("seat-model".into());
        s.overrides.args = Some(vec!["--seat".into()]);
        s.overrides.summaries = Some(true);
        let c = resolve(&graph, Some(&tpl), Some(&mem2), &s);
        assert_eq!(c.harness, Harness::Shell);
        assert_eq!(c.model.as_deref(), Some("seat-model"));
        assert_eq!(c.args, vec!["--seat"]);
        assert!(c.summaries);

        // an explicitly empty seat args list overrides non-empty defaults.
        s.overrides.args = Some(vec![]);
        assert!(resolve(&graph, Some(&tpl), Some(&mem2), &s).args.is_empty());
    }

    #[test]
    fn graph_defaults_apply_without_template() {
        let graph = GraphDefaults { harness: Some(Harness::Codex), model: Some("gm".into()) };
        let c = resolve(&graph, None, None, &seat());
        assert_eq!((c.harness, c.model.as_deref()), (Harness::Codex, Some("gm")));
    }

    #[test]
    fn builtin_defaults() {
        let c = resolve(&GraphDefaults::default(), None, None, &seat());
        assert_eq!(
            c,
            EffectiveSeatConfig {
                harness: Harness::Claude,
                model: None,
                args: vec![],
                summaries: true,
                role: None,
                cwd: None,
                instructions_sections: vec![],
            }
        );
    }

    #[test]
    fn summaries_default_by_role() {
        let g = GraphDefaults::default();
        assert!(resolve(&g, None, None, &seat()).summaries, "no role: true");
        let summarizer = member(MemberDefaults::default(), Some(Role::Summarizer));
        let c = resolve(&g, None, Some(&summarizer), &seat());
        assert_eq!((c.role, c.summaries), (Some(Role::Summarizer), false), "member role summarizer: false");
        let mut s = seat();
        s.role = Some(Role::Cron);
        assert!(!resolve(&g, None, Some(&summarizer), &s).summaries);
        assert_eq!(resolve(&g, None, Some(&summarizer), &s).role, Some(Role::Cron), "seat role beats member role");
        s.overrides.summaries = Some(true);
        assert!(resolve(&g, None, Some(&summarizer), &s).summaries, "seat override wins");
    }

    #[test]
    fn instructions_sections_appended() {
        let mut s = seat();
        s.overrides.instructions_sections = vec![
            InstructionSection { name: "b".into(), body: "two".into() },
            InstructionSection { name: "a".into(), body: "one".into() },
        ];
        let c = resolve(&GraphDefaults::default(), None, None, &s);
        let names: Vec<_> = c.instructions_sections.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a"], "named sections keep their order and are never merged");
    }

    fn clone_of(seat: &SeatRecord, lifecycle: CloneLifecycle, occupied: Option<Harness>) -> CloneRecord {
        CloneRecord {
            schema: SCHEMA_VERSION,
            id: CloneId::new(),
            rev: 1,
            seat: seat.id.clone(),
            name: "c".into(),
            name_history: vec![],
            lifecycle,
            retired: None,
            runtime: Runtime { availability: Availability::Present, bound: None, observed_at: None },
            occupant: occupied.map(|harness| Occupant { native_session: NsId::new(), harness, since: t0() }),
            sessions: vec![],
            opt_outs: vec![],
            invitations: vec![],
            reload_required: false,
        }
    }

    fn cfg(h: Harness, model: Option<&str>, args: &[&str]) -> EffectiveSeatConfig {
        EffectiveSeatConfig {
            harness: h,
            model: model.map(str::to_owned),
            args: args.iter().map(|s| s.to_string()).collect(),
            summaries: true,
            role: None,
            cwd: None,
            instructions_sections: vec![],
        }
    }

    #[test]
    fn replace_session_only_for_occupied_active_clones_and_changed_fields() {
        let s = seat();
        let occupied = clone_of(&s, CloneLifecycle::Active, Some(Harness::Claude));
        let empty = clone_of(&s, CloneLifecycle::Active, None);
        let retired = clone_of(&s, CloneLifecycle::Retired, Some(Harness::Claude));
        let foreign = clone_of(&seat(), CloneLifecycle::Active, Some(Harness::Claude));
        let clones = [occupied.clone(), empty, retired, foreign];
        let old = cfg(Harness::Claude, Some("a"), &[]);

        let fx = session_replacements(&s, &clones, &old, &cfg(Harness::Claude, Some("b"), &[]));
        assert_eq!(fx.len(), 1, "{fx:?}");
        assert_eq!((fx[0].kind.as_str(), fx[0].object.clone()), ("session.replace", occupied.id.to_any()));
        assert_eq!(fx[0].detail["from"]["model"], "a");
        assert_eq!(fx[0].detail["to"]["model"], "b");

        assert_eq!(session_replacements(&s, &clones, &old, &cfg(Harness::Claude, Some("a"), &["--x"])).len(), 1, "args change");
        assert_eq!(session_replacements(&s, &clones, &old, &cfg(Harness::Codex, Some("a"), &[])).len(), 1, "harness change");
        assert!(session_replacements(&s, &clones, &old, &old.clone()).is_empty(), "nothing changed");
        let mut only_summaries = old.clone();
        only_summaries.summaries = false;
        assert!(session_replacements(&s, &clones, &old, &only_summaries).is_empty(), "summaries is not a session property");
    }

    #[test]
    fn resume_flag_follows_profile() {
        let s = seat();
        let c = [clone_of(&s, CloneLifecycle::Active, Some(Harness::Claude))];
        let resume = |from: Harness, to: Harness| {
            let fx = session_replacements(&s, &c, &cfg(from, Some("a"), &[]), &cfg(to, Some("b"), &[]));
            fx[0].detail["resume"].as_bool().unwrap()
        };
        assert!(resume(Harness::Claude, Harness::Claude), "same harness, resume supported");
        assert!(resume(Harness::Codex, Harness::Codex));
        assert!(!resume(Harness::Claude, Harness::Codex), "different harness cannot resume the old session");
        assert!(!resume(Harness::Shell, Harness::Shell), "shell has no resume");
    }
}
