use super::document::{DocumentMember, TemplateDocument};
use super::kinds::register_kinds;
use super::structure::{SeatSource, effective_structure, read_seat_rec};
use super::withdrawal::withdraw_plan;
use crate::daemon::registry::CallerInfo;
use crate::journal::{Journal, OpRow};
use crate::model::action::{ActionKind, ActionRecord};
use crate::model::application::ApplicationRecord;
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::common::{AppLifecycle, CloneLifecycle, Lifecycle, Occupant, RetireMechanism};
use crate::model::harness::Harness;
use crate::model::operation::OpState;
use crate::model::seat::{InstructionSection, SeatRecord};
use crate::model::template::{MemberSelector, Startup, TemplateRecord};
use crate::model::{AnyId, MemberId, NsId, PlanId, SeatId, TemplateId};
use crate::plan::commands::{PlanDeps, admit_apply, create_plan};
use crate::plan::core_kinds::register_core_kinds;
use crate::plan::kind::KindRegistry;
use crate::plan::store::PlanStore;
use crate::plan::types::StoredPlan;
use crate::ports::clock::ManualClock;
use crate::ports::store::Store;
use crate::store::GitStore;
use crate::store::init::init_instance;
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::{CommitView, TreeRead};
use crate::writer::{
    Applied, Mutation, MutationCx, MutationError, MutationRegistry, WriterConfig, WriterCore,
};
use chrono::TimeZone;
use serde_json::json;
use std::sync::Arc;

// ---------------------------------------------------------------------------------------------
// fixture
// ---------------------------------------------------------------------------------------------

fn t0() -> crate::model::Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

/// Test-only seat patch: `seat_wide`, one instruction `section`, and `occupy` (every active clone gets an
/// occupant). Stands in for the participation/override kinds other tasks own.
struct Patch;
impl Mutation for Patch {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a = &cx.request.args;
        let seat = SeatId::parse(a["seat"].as_str().unwrap())
            .map_err(|e| MutationError::Bug(e.to_string()))?;
        let (loc, mut rec) =
            read_seat_rec(&cx.tree, &seat)?.ok_or_else(|| MutationError::Bug("no seat".into()))?;
        if let Some(v) = a.get("overrides") {
            rec.overrides = serde_json::from_value(v.clone()).unwrap();
        }
        if let Some(v) = a.get("seat_wide") {
            rec.participation.seat_wide = serde_json::from_value(v.clone()).unwrap();
        }
        if let Some(body) = a.get("section").and_then(|v| v.as_str()) {
            rec.overrides
                .instructions_sections
                .push(InstructionSection {
                    name: "deps".into(),
                    body: body.into(),
                });
        }
        cx.tree.put_record(loc.record_path.clone(), &mut rec)?;
        if a.get("occupy").is_some() {
            for (cloc, mut c) in layout::list_clones(&cx.tree, &loc.folder)? {
                if c.lifecycle == CloneLifecycle::Active {
                    c.occupant = Some(Occupant {
                        native_session: NsId::new(),
                        harness: Harness::Claude,
                        since: cx.now,
                    });
                    cx.tree.put_record(cloc.record_path, &mut c)?;
                }
            }
        }
        Ok(Applied {
            summary: "patch".into(),
            action: None,
        })
    }
}

struct Fx {
    _tmp: tempfile::TempDir,
    docs: std::path::PathBuf,
    deps: PlanDeps,
    w: Arc<WriterCore>,
    store: Arc<GitStore>,
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
    register_kinds(&mut kinds);
    crate::undo::register_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    let plans = Arc::new(PlanStore::new(root.join(".graph-local/plans")));
    let mut reg = MutationRegistry::default();
    reg.register("bookkeeping.patch", Arc::new(Patch));
    kinds.register_mutations(&mut reg, plans.clone());
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let w = WriterCore::new(
        store.clone(),
        journal,
        Arc::new(reg),
        clock.clone(),
        WriterConfig::default(),
    );
    let deps = PlanDeps {
        kinds,
        plans,
        store: store.clone(),
        writer: w.clone(),
        clock,
        instance: root,
    };
    Fx {
        _tmp: tmp,
        docs,
        deps,
        w,
        store,
        n: std::cell::Cell::new(0),
    }
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn plan(fx: &Fx, change: &str) -> StoredPlan {
    let v = create_plan(&fx.deps, &CallerInfo::default(), words(change))
        .unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    fx.deps.plans.get(&id).unwrap().unwrap()
}

fn plan_err(fx: &Fx, change: &str) -> String {
    match create_plan(&fx.deps, &CallerInfo::default(), words(change)) {
        Ok(_) => panic!("{change}: expected a planning error"),
        Err(e) => e.message,
    }
}

fn apply_plan(fx: &Fx, sp: &StoredPlan) -> OpRow {
    let op = admit_apply(
        &fx.deps,
        &CallerInfo::default(),
        sp.plan.id.as_str(),
        Some(&sp.hash),
        "relay",
    )
    .unwrap_or_else(|e| panic!("admit: {}", e.message));
    fx.w.drain().unwrap();
    fx.w.journal().get(&op).unwrap().unwrap()
}

fn commit(fx: &Fx, change: &str) -> OpRow {
    let sp = plan(fx, change);
    let row = apply_plan(fx, &sp);
    assert_eq!(
        row.state,
        OpState::Committed,
        "{change}: {:?}",
        row.rejection
    );
    row
}

fn view(fx: &Fx) -> CommitView<'_> {
    CommitView {
        store: &*fx.store,
        at: fx.store.head().unwrap(),
    }
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
    let loc = layout::locate(&v, &act.to_any())
        .unwrap()
        .expect("action record committed");
    read_toml(&v, &loc.record_path).unwrap().unwrap()
}

fn tpl(fx: &Fx, name: &str) -> TemplateRecord {
    let mut found: Vec<_> = layout::list_templates(&view(fx))
        .unwrap()
        .into_iter()
        .filter(|(_, t)| t.name == name)
        .collect();
    assert_eq!(found.len(), 1, "template {name}");
    found.remove(0).1
}

fn app(fx: &Fx, name: &str) -> ApplicationRecord {
    let mut found: Vec<_> = layout::list_applications(&view(fx))
        .unwrap()
        .into_iter()
        .filter(|(_, a)| a.name == name)
        .collect();
    assert_eq!(found.len(), 1, "application {name}");
    found.remove(0).1
}

fn seat_by_id(fx: &Fx, id: &SeatId) -> SeatRecord {
    read_seat_rec(&view(fx), id)
        .unwrap()
        .unwrap_or_else(|| panic!("no seat {id}"))
        .1
}

fn member_seat(fx: &Fx, app_name: &str, mem: &MemberId) -> SeatId {
    app(fx, app_name)
        .member_map
        .get(mem)
        .cloned()
        .unwrap_or_else(|| panic!("{app_name} maps no {mem}"))
}

fn seat_dir_of(fx: &Fx, id: &SeatId) -> crate::ports::store::RepoPath {
    layout::locate(&view(fx), &id.to_any())
        .unwrap()
        .unwrap()
        .folder
}

fn patch(fx: &Fx, args: serde_json::Value) {
    let mut args = args;
    args["sub"] = json!("patch");
    let op = crate::ports::writer::Writer::admit(
        &*fx.w,
        ChangeRequest {
            kind: RequestKind::Bookkeeping,
            args,
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
            confirmed: None,
        },
    )
    .unwrap();
    fx.w.drain().unwrap();
    assert_eq!(
        fx.w.journal().get(&op).unwrap().unwrap().state,
        OpState::Committed
    );
}

/// One member line of a template document.
struct M {
    id: Option<MemberId>,
    name: &'static str,
    startup: &'static str,
    model: Option<&'static str>,
    agents: Option<&'static str>,
}

fn m(id: &MemberId, name: &'static str, startup: &'static str) -> M {
    M {
        id: Some(id.clone()),
        name,
        startup,
        model: None,
        agents: None,
    }
}

fn doc_text(name: &str, members: &[M], threads: &[(&str, Option<&[&MemberId]>)]) -> String {
    let mut s = format!("name = \"{name}\"\n");
    for m in members {
        s.push_str("\n[[members]]\n");
        if let Some(id) = &m.id {
            s.push_str(&format!("id = \"{id}\"\n"));
        }
        s.push_str(&format!(
            "name = \"{}\"\nstartup = \"{}\"\n",
            m.name, m.startup
        ));
        if let Some(a) = m.agents {
            s.push_str(&format!("agents_md = \"{a}\"\n"));
        }
        if let Some(model) = m.model {
            s.push_str(&format!("[members.defaults]\nmodel = \"{model}\"\n"));
        }
    }
    for (thread, who) in threads {
        s.push_str(&format!(
            "\n[[relationships]]\nkind = \"thread_participation\"\nthread = \"{thread}\"\n"
        ));
        match who {
            None => s.push_str("members = \"all\"\n"),
            Some(ids) => {
                let ids: Vec<String> = ids.iter().map(|i| format!("\"{i}\"")).collect();
                s.push_str(&format!("members = {{ ids = [{}] }}\n", ids.join(", ")));
            }
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

fn create_template(fx: &Fx, name: &str, members: &[M], threads: &[(&str, Option<&[&MemberId]>)]) {
    let path = write_doc(fx, &doc_text(name, members, threads));
    commit(fx, &format!("template create {name} --from {path}"));
}

fn edit_plan(
    fx: &Fx,
    name: &str,
    members: &[M],
    threads: &[(&str, Option<&[&MemberId]>)],
) -> StoredPlan {
    let path = write_doc(fx, &doc_text(name, members, threads));
    plan(fx, &format!("template edit {name} --from {path}"))
}

fn edit(fx: &Fx, name: &str, members: &[M], threads: &[(&str, Option<&[&MemberId]>)]) -> OpRow {
    let sp = edit_plan(fx, name, members, threads);
    let row = apply_plan(fx, &sp);
    assert_eq!(
        row.state,
        OpState::Committed,
        "edit {name}: {:?}",
        row.rejection
    );
    row
}

fn alpha(fx: &Fx) {
    commit(fx, "teamspace create alpha");
}

fn apply_app(fx: &Fx, template: &str, name: &str) -> OpRow {
    commit(
        fx,
        &format!("application apply {template} --teamspace alpha --name {name}"),
    )
}

// ---------------------------------------------------------------------------------------------
// acceptance: Auth/Billing composition withdrawal (DESIGN-NOTES)
// ---------------------------------------------------------------------------------------------

#[test]
fn auth_billing_composition_withdrawal() {
    let fx = fx();
    alpha(&fx);
    let (e_auth, e_billing) = (MemberId::new(), MemberId::new());
    create_template(
        &fx,
        "auth-tpl",
        &[m(&e_auth, "engineer", "deferred")],
        &[("auth-thread", None)],
    );
    create_template(
        &fx,
        "billing-tpl",
        &[m(&e_billing, "engineer", "deferred")],
        &[("billing-thread", None)],
    );
    apply_app(&fx, "auth-tpl", "auth");
    let engineer = member_seat(&fx, "auth", &e_auth);
    commit(
        &fx,
        &format!(
            "application apply billing-tpl --teamspace alpha --name billing --reuse {e_billing}={engineer}"
        ),
    );
    assert_eq!(
        app(&fx, "billing").member_map.get(&e_billing),
        Some(&engineer),
        "billing maps its engineer member to the shared seat"
    );
    assert_eq!(
        layout::all_seats(&view(&fx)).unwrap().len(),
        1,
        "billing created no seat of its own"
    );
    // Reuse is recorded in each participating application.
    let auth_id = app(&fx, "auth").id;
    assert_eq!(
        app(&fx, "billing")
            .reused
            .iter()
            .map(|r| (r.seat.clone(), r.from.clone()))
            .collect::<Vec<_>>(),
        vec![(engineer.clone(), Some(auth_id.clone()))]
    );
    assert!(
        app(&fx, "auth").reused.iter().any(|r| r.seat == engineer),
        "the providing application records the reuse too"
    );
    assert_eq!(seat_by_id(&fx, &engineer).applications.len(), 2);

    // A later live-template edit gives Auth an Auth-only reviewer.
    let reviewer_mem = MemberId::new();
    let before = layout::all_seats(&view(&fx)).unwrap().len();
    edit(
        &fx,
        "auth-tpl",
        &[
            m(&e_auth, "engineer", "deferred"),
            m(&reviewer_mem, "reviewer", "deferred"),
        ],
        &[("auth-thread", None)],
    );
    assert_eq!(
        layout::all_seats(&view(&fx)).unwrap().len(),
        before + 1,
        "only Auth gets the reviewer"
    );
    let reviewer = member_seat(&fx, "auth", &reviewer_mem);
    assert!(
        !app(&fx, "billing")
            .member_map
            .values()
            .any(|s| s == &reviewer)
    );

    // Undo Auth: engineer preserved, reviewer retired, Auth's contribution withdrawn, Billing's kept.
    let sp = plan(&fx, "application retire auth");
    assert_eq!(count(&sp, "application.retire"), 1);
    assert_eq!(count(&sp, "seat.retire"), 1);
    let retire = sp
        .plan
        .effects
        .iter()
        .find(|e| e.kind == "seat.retire")
        .unwrap();
    assert_eq!(retire.object, reviewer.to_any());
    assert_eq!(retire.detail["mechanism"], json!("application_withdrawal"));
    let keep = sp
        .plan
        .effects
        .iter()
        .find(|e| e.kind == "seat.keep")
        .expect("preview lists the kept seat");
    assert_eq!(keep.object, engineer.to_any());
    assert_eq!(keep.detail["reason"], json!("reused by billing"));
    assert_eq!(count(&sp, "participation.withdraw"), 1);
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);

    let r = seat_by_id(&fx, &reviewer);
    assert_eq!(r.lifecycle, Lifecycle::Retired);
    assert_eq!(
        r.retired.unwrap().mechanism,
        RetireMechanism::ApplicationWithdrawal
    );
    let e = seat_by_id(&fx, &engineer);
    assert_ne!(e.lifecycle, Lifecycle::Retired, "shared engineer survives");
    assert_eq!(
        e.applications,
        vec![app(&fx, "billing").id],
        "the engineer no longer belongs to Auth"
    );
    let auth = app(&fx, "auth");
    assert_eq!(auth.lifecycle, AppLifecycle::Retired);
    assert!(
        auth.contributions.relationships.is_empty(),
        "Auth's live contribution is withdrawn"
    );
    let billing = app(&fx, "billing");
    assert_eq!(billing.lifecycle, AppLifecycle::Active);
    assert_eq!(billing.contributions.relationships.len(), 1);
    assert_eq!(
        billing.contributions.relationships[0].thread,
        "billing-thread"
    );

    let act = action_of(&fx, &row);
    assert_eq!(act.kind, ActionKind::ApplicationRetire);
    assert!(act.retired.contains(&reviewer.to_any()) && act.retired.contains(&auth.id.to_any()));
    assert!(!act.retired.contains(&engineer.to_any()));
    let kept = act.compensation["kept"].as_array().unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0]["seat"].as_str(), Some(engineer.as_str()));
    assert_eq!(kept[0]["reason"].as_str(), Some("reused by billing"));
}

fn reuse_pair(fx: &Fx) -> SeatId {
    let (e_a, e_b) = (MemberId::new(), MemberId::new());
    create_template(fx, "a-tpl", &[m(&e_a, "engineer", "deferred")], &[]);
    create_template(fx, "b-tpl", &[m(&e_b, "engineer", "deferred")], &[]);
    apply_app(fx, "a-tpl", "a");
    let engineer = member_seat(fx, "a", &e_a);
    commit(
        fx,
        &format!("application apply b-tpl --teamspace alpha --name b --reuse {e_b}={engineer}"),
    );
    engineer
}

#[test]
fn reused_seat_survives_after_its_creator_retires_first() {
    let fx = fx();
    alpha(&fx);
    let engineer = reuse_pair(&fx);

    // A retires: the engineer is kept (reused by b).
    let sp = plan(&fx, "application retire a");
    assert_eq!(count(&sp, "seat.retire"), 0);
    assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
    assert_ne!(seat_by_id(&fx, &engineer).lifecycle, Lifecycle::Retired);

    // B retires: the engineer pre-existed b, so it is still kept.
    let sp = plan(&fx, "application retire b");
    assert_eq!(count(&sp, "seat.retire"), 0, "{:?}", kinds_of(&sp));
    let keep = sp
        .plan
        .effects
        .iter()
        .find(|e| e.kind == "seat.keep")
        .expect("the kept seat is listed");
    assert_eq!(keep.object, engineer.to_any());
    assert!(
        keep.detail["reason"]
            .as_str()
            .unwrap()
            .contains("pre-existing"),
        "{}",
        keep.detail["reason"]
    );
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_ne!(
        seat_by_id(&fx, &engineer).lifecycle,
        Lifecycle::Retired,
        "a reused seat is never withdrawn by its borrower"
    );
}

#[test]
fn creator_still_retires_its_lent_seat_once_the_borrower_is_gone() {
    let fx = fx();
    alpha(&fx);
    let engineer = reuse_pair(&fx);

    // B retires first: the engineer pre-existed b, so it is kept.
    let sp = plan(&fx, "application retire b");
    assert_eq!(count(&sp, "seat.retire"), 0, "{:?}", kinds_of(&sp));
    assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
    assert_ne!(seat_by_id(&fx, &engineer).lifecycle, Lifecycle::Retired);

    // A created it and nobody else uses it any more: it is retired.
    let sp = plan(&fx, "application retire a");
    assert_eq!(count(&sp, "seat.retire"), 1, "{:?}", kinds_of(&sp));
    let retire = sp
        .plan
        .effects
        .iter()
        .find(|e| e.kind == "seat.retire")
        .unwrap();
    assert_eq!(retire.object, engineer.to_any());
    assert_eq!(retire.detail["mechanism"], json!("application_withdrawal"));
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let seat = seat_by_id(&fx, &engineer);
    assert_eq!(seat.lifecycle, Lifecycle::Retired);
    assert_eq!(
        seat.retired.unwrap().mechanism,
        RetireMechanism::ApplicationWithdrawal
    );
}

#[test]
fn withdraw_plan_keeps_borrowed_seat_when_provider_is_retired() {
    let fx = fx();
    alpha(&fx);
    let engineer = reuse_pair(&fx);
    let sp = plan(&fx, "application retire a");
    assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);

    let preview = withdraw_plan(&view(&fx), &app(&fx, "b")).unwrap();
    assert!(preview.retire.is_empty(), "{:?}", preview.retire);
    assert_eq!(preview.keep.len(), 1);
    assert_eq!(preview.keep[0].0, engineer);
    assert!(
        preview.keep[0].1.contains("pre-existing"),
        "{}",
        preview.keep[0].1
    );
}

#[test]
fn repeated_application_maps_distinct_seats() {
    let fx = fx();
    alpha(&fx);
    let (a, b) = (MemberId::new(), MemberId::new());
    create_template(
        &fx,
        "pair",
        &[m(&a, "lead", "deferred"), m(&b, "dev", "deferred")],
        &[],
    );
    apply_app(&fx, "pair", "one");
    apply_app(&fx, "pair", "two");
    let (one, two) = (app(&fx, "one"), app(&fx, "two"));
    let one_seats: Vec<_> = one.member_map.values().cloned().collect();
    let two_seats: Vec<_> = two.member_map.values().cloned().collect();
    assert_eq!(one_seats.len(), 2);
    assert!(
        one_seats.iter().all(|s| !two_seats.contains(s)),
        "disjoint seats: {one_seats:?} vs {two_seats:?}"
    );
    for mem in [&a, &b] {
        let (s1, s2) = (
            seat_by_id(&fx, &one.member_map[mem]),
            seat_by_id(&fx, &two.member_map[mem]),
        );
        assert_eq!(
            s1.template_ref, s2.template_ref,
            "both are the same template member"
        );
        assert_eq!(s1.template_ref.as_ref().unwrap().member, *mem);
        assert_eq!(s1.applications, vec![one.id.clone()]);
        assert_eq!(s2.applications, vec![two.id.clone()]);
    }
    assert_eq!(layout::all_seats(&view(&fx)).unwrap().len(), 4);
}

#[test]
fn exclusion_scoped_per_application() {
    let fx = fx();
    alpha(&fx);
    let (a, b) = (MemberId::new(), MemberId::new());
    create_template(
        &fx,
        "pair",
        &[m(&a, "lead", "deferred"), m(&b, "dev", "deferred")],
        &[],
    );
    apply_app(&fx, "pair", "one");
    apply_app(&fx, "pair", "two");
    let one_lead = member_seat(&fx, "one", &a);
    let two_lead = member_seat(&fx, "two", &a);
    commit(&fx, &format!("seat retire {one_lead}"));
    assert_eq!(
        app(&fx, "one").exclusions,
        vec![a.clone()],
        "retiring a template-member seat records the exclusion"
    );
    assert!(
        app(&fx, "two").exclusions.is_empty(),
        "exclusion is scoped to the application"
    );

    // A later template edit does not recreate the excluded member for `one`; `two` still has its seat.
    let c = MemberId::new();
    let members = [
        m(&a, "lead", "deferred"),
        m(&b, "dev", "deferred"),
        m(&c, "qa", "deferred"),
    ];
    let sp = edit_plan(&fx, "pair", &members, &[]);
    assert_eq!(
        count(&sp, "seat.create"),
        2,
        "qa for each application: {:?}",
        kinds_of(&sp)
    );
    assert!(
        sp.plan
            .effects
            .iter()
            .all(|e| e.detail.get("member") != Some(&json!(a)) || e.kind != "seat.create")
    );
    assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
    let leads: Vec<_> = layout::all_seats(&view(&fx))
        .unwrap()
        .into_iter()
        .map(|(_, s)| s)
        .filter(|s| s.template_ref.as_ref().is_some_and(|r| r.member == a))
        .collect();
    assert_eq!(
        leads.len(),
        2,
        "no new lead seat was created for the excluding application"
    );
    assert_eq!(seat_by_id(&fx, &one_lead).lifecycle, Lifecycle::Retired);
    assert_ne!(seat_by_id(&fx, &two_lead).lifecycle, Lifecycle::Retired);
    assert!(
        app(&fx, "one").member_map.contains_key(&c) && app(&fx, "two").member_map.contains_key(&c)
    );
}

#[test]
fn member_removal_retires_exclusive_keeps_shared() {
    let fx = fx();
    alpha(&fx);
    let (e_auth, r_auth, e_billing) = (MemberId::new(), MemberId::new(), MemberId::new());
    create_template(
        &fx,
        "auth-tpl",
        &[
            m(&e_auth, "engineer", "deferred"),
            m(&r_auth, "reviewer", "deferred"),
        ],
        &[],
    );
    create_template(
        &fx,
        "billing-tpl",
        &[m(&e_billing, "engineer", "deferred")],
        &[],
    );
    apply_app(&fx, "auth-tpl", "auth");
    let engineer = member_seat(&fx, "auth", &e_auth);
    let reviewer = member_seat(&fx, "auth", &r_auth);
    commit(
        &fx,
        &format!(
            "application apply billing-tpl --teamspace alpha --name billing --reuse {e_billing}={engineer}"
        ),
    );

    let sp = edit_plan(&fx, "auth-tpl", &[], &[]);
    let unmaps: Vec<_> = sp
        .plan
        .effects
        .iter()
        .filter(|e| e.kind == "application.unmap")
        .collect();
    assert_eq!(unmaps.len(), 2);
    let outcome = |seat: &SeatId| {
        unmaps
            .iter()
            .find(|e| e.detail["seat"] == json!(seat))
            .map(|e| e.detail["outcome"].as_str().unwrap().to_owned())
    };
    assert_eq!(outcome(&engineer).as_deref(), Some("kept"));
    assert_eq!(outcome(&reviewer).as_deref(), Some("retired"));
    assert_eq!(count(&sp, "seat.retire"), 1);
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);

    assert_eq!(
        seat_by_id(&fx, &reviewer).retired.unwrap().mechanism,
        RetireMechanism::ApplicationWithdrawal
    );
    assert_ne!(seat_by_id(&fx, &engineer).lifecycle, Lifecycle::Retired);
    let auth = app(&fx, "auth");
    assert!(auth.member_map.is_empty(), "both members were unmapped");
    assert!(
        auth.exclusions.is_empty(),
        "withdrawal records no exclusion"
    );
    assert_eq!(
        app(&fx, "billing").member_map.get(&e_billing),
        Some(&engineer),
        "billing keeps its mapping"
    );
    assert_eq!(
        seat_by_id(&fx, &engineer).applications,
        vec![app(&fx, "billing").id]
    );
    let act = action_of(&fx, &row);
    assert_eq!(act.kind, ActionKind::TemplateEdit);
    assert!(act.retired.contains(&reviewer.to_any()) && !act.retired.contains(&engineer.to_any()));
}

#[test]
fn readd_same_member_keeps_seat_id() {
    let fx = fx();
    alpha(&fx);
    let (e, r) = (MemberId::new(), MemberId::new());
    let both = [m(&e, "engineer", "deferred"), m(&r, "reviewer", "deferred")];
    create_template(&fx, "pair", &both, &[]);
    apply_app(&fx, "pair", "one");
    let original = member_seat(&fx, "one", &r);
    let live_dir = seat_dir_of(&fx, &original);

    edit(&fx, "pair", &[m(&e, "engineer", "deferred")], &[]);
    let gone = seat_by_id(&fx, &original);
    assert_eq!(gone.lifecycle, Lifecycle::Retired);
    assert_eq!(
        gone.retired.as_ref().unwrap().mechanism,
        RetireMechanism::ApplicationWithdrawal
    );
    assert!(!app(&fx, "one").member_map.contains_key(&r));
    assert_ne!(
        seat_dir_of(&fx, &original),
        live_dir,
        "the retired seat folder is archived"
    );

    let sp = edit_plan(&fx, "pair", &both, &[]);
    assert_eq!(count(&sp, "seat.resurrect"), 1, "{:?}", kinds_of(&sp));
    assert_eq!(
        count(&sp, "seat.create"),
        0,
        "the withdrawn seat is reused, not duplicated"
    );
    assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
    assert_eq!(
        member_seat(&fx, "one", &r),
        original,
        "re-adding the same member id remaps the same seat id"
    );
    let back = seat_by_id(&fx, &original);
    assert_eq!(back.lifecycle, Lifecycle::Dormant);
    assert!(back.retired.is_none());
    assert_eq!(back.applications, vec![app(&fx, "one").id]);
    assert_eq!(
        seat_dir_of(&fx, &original),
        live_dir,
        "the seat folder is back at its live path"
    );
    assert_eq!(layout::all_seats(&view(&fx)).unwrap().len(), 2);
}

#[test]
fn readd_respects_local_exclusion() {
    let fx = fx();
    alpha(&fx);
    let (e, r) = (MemberId::new(), MemberId::new());
    let both = [m(&e, "engineer", "deferred"), m(&r, "reviewer", "deferred")];
    create_template(&fx, "pair", &both, &[]);
    apply_app(&fx, "pair", "one");
    let reviewer = member_seat(&fx, "one", &r);
    commit(&fx, &format!("seat retire {reviewer}"));
    assert_eq!(app(&fx, "one").exclusions, vec![r.clone()]);

    edit(&fx, "pair", &[m(&e, "engineer", "deferred")], &[]);
    let sp = edit_plan(&fx, "pair", &both, &[]);
    assert_eq!(
        count(&sp, "seat.resurrect") + count(&sp, "seat.create"),
        0,
        "{:?}",
        kinds_of(&sp)
    );
    assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
    let one = app(&fx, "one");
    assert!(
        one.exclusions.contains(&r),
        "the local exclusion survives the member leaving and returning"
    );
    assert!(!one.member_map.contains_key(&r));
    let s = seat_by_id(&fx, &reviewer);
    assert_eq!(s.lifecycle, Lifecycle::Retired);
    let mech = s.retired.unwrap().mechanism;
    assert!(
        matches!(
            mech,
            RetireMechanism::UserCli | RetireMechanism::AgentRequest
        ),
        "a requested retirement is never undone by re-adding: {mech:?}"
    );
    assert_eq!(layout::all_seats(&view(&fx)).unwrap().len(), 2);
}

#[test]
fn copy_preserves_member_ids() {
    let fx = fx();
    let (a, b) = (MemberId::new(), MemberId::new());
    let members = [
        M {
            agents: Some("be careful"),
            ..m(&a, "lead", "active")
        },
        m(&b, "dev", "deferred"),
    ];
    create_template(&fx, "orig", &members, &[("t", Some(&[&a]))]);
    commit(&fx, "template copy orig clone-tpl");
    let (orig, copy) = (tpl(&fx, "orig"), tpl(&fx, "clone-tpl"));
    assert_ne!(orig.id, copy.id);
    assert_eq!(
        copy.members
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>(),
        orig.members
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>(),
        "member ids are preserved"
    );
    assert_eq!(copy.members, orig.members);
    assert_eq!(copy.relationships, orig.relationships);
    let from = copy.copied_from.expect("copied_from is set");
    assert_eq!(from.template, orig.id);
    assert_eq!(from.at, t0());
    assert!(orig.copied_from.is_none());
    let v = view(&fx);
    let dir = layout::locate(&v, &copy.id.to_any())
        .unwrap()
        .unwrap()
        .folder;
    let agents = v
        .read_file(&layout::member_agents_md(&dir, "lead"))
        .unwrap()
        .expect("AGENTS.md copied");
    assert_eq!(String::from_utf8(agents).unwrap(), "be careful");
    assert!(
        v.read_file(&layout::member_agents_md(&dir, "dev"))
            .unwrap()
            .is_none()
    );
    // Copies are independent: applying the copy maps the same member ids.
    alpha(&fx);
    apply_app(&fx, "clone-tpl", "from-copy");
    assert!(app(&fx, "from-copy").member_map.contains_key(&a));
}

#[test]
fn template_edit_plan_lists_every_affected_application_and_session_replacement() {
    let fx = fx();
    alpha(&fx);
    let a = MemberId::new();
    let old = [M {
        model: Some("old-model"),
        ..m(&a, "lead", "active")
    }];
    create_template(&fx, "solo", &old, &[]);
    apply_app(&fx, "solo", "one");
    apply_app(&fx, "solo", "two");
    for name in ["one", "two"] {
        patch(
            &fx,
            json!({ "seat": member_seat(&fx, name, &a), "occupy": true }),
        );
    }
    let new = [M {
        model: Some("new-model"),
        ..m(&a, "lead", "active")
    }];
    let sp = edit_plan(&fx, "solo", &new, &[]);

    let updates: Vec<_> = sp
        .plan
        .effects
        .iter()
        .filter(|e| e.kind == "application.update")
        .map(|e| e.object.clone())
        .collect();
    let expected: Vec<AnyId> = ["one", "two"]
        .iter()
        .map(|n| app(&fx, n).id.to_any())
        .collect();
    assert_eq!(updates.len(), 2);
    assert!(
        expected.iter().all(|id| updates.contains(id)),
        "every application of the template is listed"
    );
    let edit_fx = sp
        .plan
        .effects
        .iter()
        .find(|e| e.kind == "template.edit")
        .unwrap();
    assert_eq!(
        edit_fx.detail["changed_fields"],
        json!([format!("members.{a}.defaults.model")])
    );

    let replaces: Vec<_> = sp
        .plan
        .effects
        .iter()
        .filter(|e| e.kind == "session.replace")
        .collect();
    assert_eq!(
        replaces.len(),
        2,
        "one per occupied clone: {:?}",
        kinds_of(&sp)
    );
    for e in &replaces {
        assert_eq!(e.detail["from"]["model"], json!("old-model"));
        assert_eq!(e.detail["to"]["model"], json!("new-model"));
    }
    let clones: Vec<_> = replaces.iter().map(|e| e.object.clone()).collect();
    assert_ne!(clones[0], clones[1]);
    for ae in sp
        .plan
        .effects
        .iter()
        .filter(|e| e.kind == "application.update")
    {
        assert_eq!(
            ae.detail["replaced_seats"].as_array().unwrap().len(),
            1,
            "each application names its replaced seat"
        );
    }

    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    let act = action_of(&fx, &row);
    assert_eq!(
        act.compensation["changed_fields"].as_array().unwrap().len(),
        1
    );
    let before = act.compensation["before"].as_table().unwrap();
    let after = act.compensation["after"].as_table().unwrap();
    assert_eq!(
        before["members"][0]["defaults"]["model"].as_str(),
        Some("old-model")
    );
    assert_eq!(
        after["members"][0]["defaults"]["model"].as_str(),
        Some("new-model")
    );
    assert_eq!(
        tpl(&fx, "solo").members[0].defaults.model.as_deref(),
        Some("new-model")
    );
}

#[test]
fn hydrate_creates_dormant_and_active_per_startup() {
    let fx = fx();
    alpha(&fx);
    let (a, b) = (MemberId::new(), MemberId::new());
    let members = [
        M {
            agents: Some("lead notes"),
            ..m(&a, "lead", "active")
        },
        m(&b, "dev", "deferred"),
    ];
    create_template(&fx, "pair", &members, &[("pair-thread", None)]);
    let sp = plan(&fx, "application apply pair --teamspace alpha --name one");
    assert_eq!(count(&sp, "application.create"), 1);
    assert_eq!(count(&sp, "seat.create"), 2);
    assert_eq!(
        count(&sp, "runtime.open_tab"),
        1,
        "only the active member opens a tab"
    );
    assert_eq!(count(&sp, "participation.contribute"), 1);
    let row = apply_plan(&fx, &sp);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);

    let one = app(&fx, "one");
    let (lead, dev) = (
        seat_by_id(&fx, &one.member_map[&a]),
        seat_by_id(&fx, &one.member_map[&b]),
    );
    assert_eq!(lead.lifecycle, Lifecycle::Active);
    assert_eq!(dev.lifecycle, Lifecycle::Dormant);
    let lead_clones = layout::list_clones(&view(&fx), &seat_dir_of(&fx, &lead.id)).unwrap();
    assert_eq!(lead_clones.len(), 1);
    assert_eq!(lead_clones[0].1.lifecycle, CloneLifecycle::Active);
    assert!(
        layout::list_clones(&view(&fx), &seat_dir_of(&fx, &dev.id))
            .unwrap()
            .is_empty(),
        "dormant seats start with no clone"
    );
    assert_eq!(
        lead.template_ref.as_ref().unwrap().template,
        tpl(&fx, "pair").id
    );
    assert_eq!(lead.applications, vec![one.id.clone()]);
    assert_eq!(one.contributions.relationships.len(), 1);
    assert_eq!(one.created_by.op, row.op);
    let v = view(&fx);
    let md = v
        .read_file(&seat_dir_of(&fx, &lead.id).join("AGENTS.md").unwrap())
        .unwrap()
        .expect("member AGENTS.md copied into the seat");
    assert_eq!(String::from_utf8(md).unwrap(), "lead notes");
    assert!(
        v.read_file(&seat_dir_of(&fx, &dev.id).join("AGENTS.md").unwrap())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        crate::model::common::Lifecycle::Active,
        crate::store::record::read_toml::<crate::model::teamspace::TeamspaceRecord>(
            &v,
            &layout::locate(&v, &one.teamspace.to_any())
                .unwrap()
                .unwrap()
                .record_path
        )
        .unwrap()
        .unwrap()
        .lifecycle,
        "an active member activates the teamspace"
    );

    let act = action_of(&fx, &row);
    assert_eq!(act.kind, ActionKind::Hydrate);
    let created: Vec<&str> = act.compensation["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for id in [
        one.id.as_str(),
        lead.id.as_str(),
        dev.id.as_str(),
        lead_clones[0].1.id.as_str(),
    ] {
        assert!(created.contains(&id), "{id} listed as created: {created:?}");
    }
    assert!(act.compensation["reused"].as_array().unwrap().is_empty());
    assert_eq!(
        act.compensation["relationships_added"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(act.affected.iter().any(|a| a.object == one.id.to_any()));
}

#[test]
fn application_retire_includes_seats_created_by_later_edits() {
    let fx = fx();
    alpha(&fx);
    let (a, b) = (MemberId::new(), MemberId::new());
    create_template(&fx, "grow", &[m(&a, "lead", "deferred")], &[]);
    apply_app(&fx, "grow", "one");
    edit(
        &fx,
        "grow",
        &[m(&a, "lead", "deferred"), m(&b, "dev", "active")],
        &[],
    );
    let later = member_seat(&fx, "one", &b);

    let one = app(&fx, "one");
    let preview = withdraw_plan(&view(&fx), &one).unwrap();
    assert_eq!(preview.retire.len(), 2, "{preview:?}");
    assert!(preview.retire.contains(&later));
    assert!(preview.keep.is_empty() && preview.repair_required.is_none());

    let sp = plan(&fx, "application retire one");
    assert_eq!(count(&sp, "seat.retire"), 2);
    assert_eq!(
        count(&sp, "clone.retire"),
        1,
        "the later, active member has a clone"
    );
    assert_eq!(count(&sp, "runtime.close_tab"), 1);
    assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
    for seat in one.member_map.values().chain([&later]) {
        let s = seat_by_id(&fx, seat);
        assert_eq!(s.lifecycle, Lifecycle::Retired);
        assert_eq!(
            s.retired.unwrap().mechanism,
            RetireMechanism::ApplicationWithdrawal
        );
    }
    assert!(
        plan_err(&fx, "application retire one").contains("no application named"),
        "a retired application no longer resolves by name"
    );
    let by_id = plan_err(&fx, &format!("application retire {}", one.id));
    assert!(by_id.contains("already retired"), "{by_id}");
}

#[test]
fn ambiguous_dependency_is_repair_required() {
    let fx = fx();
    alpha(&fx);
    let (x, y) = (MemberId::new(), MemberId::new());
    create_template(&fx, "tpl-x", &[m(&x, "xseat", "deferred")], &[]);
    create_template(&fx, "tpl-y", &[m(&y, "yseat", "deferred")], &[]);
    apply_app(&fx, "tpl-x", "app-x");
    apply_app(&fx, "tpl-y", "app-y");
    let sp = plan(&fx, "application retire app-x");
    assert!(
        sp.plan.repair_required.is_none(),
        "unambiguous before the dependency exists"
    );

    let xseat = member_seat(&fx, "app-x", &x);
    patch(
        &fx,
        json!({ "seat": xseat, "section": format!("coordinates with {}", app(&fx, "app-y").id) }),
    );
    let sp = plan(&fx, "application retire app-x");
    let why = sp
        .plan
        .repair_required
        .clone()
        .expect("a seat naming another application is ambiguous");
    assert!(why.contains("app-y") && why.contains("xseat"), "{why}");
    let err = admit_apply(
        &fx.deps,
        &CallerInfo::default(),
        sp.plan.id.as_str(),
        Some(&sp.hash),
        "relay",
    )
    .unwrap_err();
    assert!(err.message.contains("repair"), "{}", err.message);
    assert_eq!(
        seat_by_id(&fx, &xseat).lifecycle,
        Lifecycle::Dormant,
        "nothing was retired"
    );

    // The same ambiguity blocks a template edit that would withdraw the seat.
    let sp = edit_plan(&fx, "tpl-x", &[], &[]);
    assert!(
        sp.plan.repair_required.is_some(),
        "{:?}",
        sp.plan.repair_required
    );
    let preview = withdraw_plan(&view(&fx), &app(&fx, "app-x")).unwrap();
    assert_eq!(preview.retire, vec![xseat]);
    assert!(preview.repair_required.is_some());
}

#[test]
fn ambiguous_occupied_seat_wide_thread_is_repair_required() {
    let fx = fx();
    alpha(&fx);
    let (x, y) = (MemberId::new(), MemberId::new());
    create_template(&fx, "tpl-x", &[m(&x, "xseat", "active")], &[]);
    create_template(
        &fx,
        "tpl-y",
        &[m(&y, "yseat", "deferred")],
        &[("shared-thread", None)],
    );
    apply_app(&fx, "tpl-x", "app-x");
    apply_app(&fx, "tpl-y", "app-y");
    let xseat = member_seat(&fx, "app-x", &x);
    patch(
        &fx,
        json!({ "seat": xseat, "seat_wide": ["shared-thread"] }),
    );
    assert!(
        plan(&fx, "application retire app-x")
            .plan
            .repair_required
            .is_none(),
        "not ambiguous while nobody occupies the clone"
    );
    patch(&fx, json!({ "seat": xseat, "occupy": true }));
    let why = plan(&fx, "application retire app-x")
        .plan
        .repair_required
        .expect("occupied + seat-wide in another application's thread");
    assert!(
        why.contains("shared-thread") && why.contains("app-y"),
        "{why}"
    );
}

#[test]
fn effective_structure_reflects_exclusions_mapping_and_pending_members() {
    let fx = fx();
    alpha(&fx);
    let (a, b) = (MemberId::new(), MemberId::new());
    create_template(
        &fx,
        "pair",
        &[m(&a, "lead", "deferred"), m(&b, "dev", "active")],
        &[],
    );
    apply_app(&fx, "pair", "one");
    let s = effective_structure(&view(&fx), &app(&fx, "one")).unwrap();
    assert_eq!(s.seats.len(), 2);
    assert!(
        s.seats
            .iter()
            .all(|d| d.source == SeatSource::Mapped && d.seat.is_some())
    );
    assert_eq!(s.to_create().count(), 0);

    commit(&fx, &format!("seat retire {}", member_seat(&fx, "one", &a)));
    let s = effective_structure(&view(&fx), &app(&fx, "one")).unwrap();
    assert_eq!(s.excluded, vec![a.clone()]);
    assert_eq!(s.seats.len(), 1, "the excluded member is not desired");
    assert_eq!(s.seats[0].member.as_ref(), Some(&b));

    // A member added to the template but not yet propagated is a seat to create.
    let c = MemberId::new();
    let mut t = tpl(&fx, "pair");
    t.members.push(crate::model::template::TemplateMember {
        id: c.clone(),
        name: "qa".into(),
        seat_template: None,
        responsibility: None,
        system_duty: None,
        startup: Startup::Deferred,
        defaults: Default::default(),
    });
    let s = super::structure::effective_structure_with(&view(&fx), &t, &app(&fx, "one")).unwrap();
    let pending: Vec<_> = s.to_create().collect();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].member.as_ref(), Some(&c));
    assert_eq!(pending[0].name, "qa");
}

// ---------------------------------------------------------------------------------------------
// document and CLI words
// ---------------------------------------------------------------------------------------------

#[test]
fn document_roundtrip_and_diff_paths() {
    // `diff` orders members by id, and ULIDs minted in one millisecond are not ordered: sort them so a < b < c.
    let mut ids = [MemberId::new(), MemberId::new(), MemberId::new()];
    ids.sort();
    let [a, b, c] = ids;
    let text = format!(
        r#"
name = "tpl"

[defaults]
model = "m1"
summaries = false

[[members]]
id = "{a}"
name = "lead"
startup = "active"
role_ref = "dispatcher"
agents_md = "hi"

[members.defaults]
harness = "claude"
args = ["--x"]

[[members]]
id = "{b}"
name = "dev"
startup = "deferred"

[[relationships]]
kind = "thread_participation"
thread = "t"
members = "all"

[[relationships]]
kind = "thread_participation"
thread = "u"
members = {{ ids = ["{a}"] }}
"#
    );
    let doc = TemplateDocument::parse(&text).unwrap();
    assert_eq!(doc.members[0].agents_md.as_deref(), Some("hi"));
    assert_eq!(doc.relationships[0].members, MemberSelector::All);
    assert_eq!(
        doc.relationships[1].members,
        MemberSelector::Ids(vec![a.clone()])
    );

    let tid = TemplateId::new();
    let mut reserved = crate::plan::types::Reserved::default();
    let rec = doc.to_record(tid.clone(), 3, &Default::default(), &mut reserved);
    assert_eq!((rec.id.clone(), rec.rev), (tid, 3));
    assert_eq!(rec.members[0].id, a);
    assert!(reserved.0.is_empty(), "explicit ids reserve nothing");
    let back = TemplateDocument::from_record(&rec);
    let mut expected = doc.clone();
    expected.members[0].agents_md = None;
    assert_eq!(
        back, expected,
        "record round-trip loses only agents_md (a file, not a record field)"
    );
    assert_eq!(
        TemplateDocument::parse(&toml::to_string(&doc).unwrap()).unwrap(),
        doc,
        "document TOML round-trip"
    );
    assert_eq!(doc.to_table()["name"].as_str(), Some("tpl"));

    // Members without ids get reserved ids in slot `member:<name>`, stable across recomputation.
    let mut no_ids = doc.clone();
    no_ids.members[1].id = None;
    let r1 = no_ids.to_record(TemplateId::new(), 0, &Default::default(), &mut reserved);
    assert_eq!(reserved.0.keys().collect::<Vec<_>>(), vec!["member:dev"]);
    let r2 = no_ids.to_record(TemplateId::new(), 0, &Default::default(), &mut reserved);
    assert_eq!(r1.members[1].id, r2.members[1].id);

    let mut new = doc.clone();
    new.defaults.model = Some("m2".into());
    new.members[0].defaults.harness = Some(Harness::Codex);
    new.members[0].agents_md = Some("changed".into());
    new.members.remove(1);
    new.members.push(DocumentMember {
        id: Some(c.clone()),
        name: "qa".into(),
        seat_template: None,
        responsibility: None,
        system_duty: None,
        startup: Startup::Deferred,
        defaults: Default::default(),
        agents_md: None,
    });
    new.relationships.pop();
    let paths: Vec<String> = TemplateDocument::diff(&doc, &new)
        .into_iter()
        .map(|c| c.path)
        .collect();
    assert_eq!(
        paths,
        vec![
            "defaults.model".to_owned(),
            format!("members.{a}.defaults.harness"),
            format!("members.{a}.agents_md"),
            format!("members.{b}"),
            format!("members.{c}"),
            "relationships".to_owned(),
        ]
    );
    let ch = TemplateDocument::diff(&doc, &new);
    assert_eq!(
        (ch[0].before.clone(), ch[0].after.clone()),
        (Some(json!("m1")), Some(json!("m2")))
    );
    assert_eq!(
        (ch[3].before.clone(), ch[3].after.clone()),
        (Some(json!("dev")), None),
        "removed member"
    );
    assert!(TemplateDocument::diff(&doc, &doc).is_empty());
}

#[test]
fn document_validation_rejects_duplicate_names_and_ids() {
    assert!(TemplateDocument::parse("name = \"t\"\n[[members]]\nname = \"a\"\nstartup = \"active\"\n[[members]]\nname = \"a\"\nstartup = \"active\"\n")
        .unwrap_err()
        .contains("duplicate member name"));
    let id = MemberId::new();
    let dup = format!(
        "name = \"t\"\n[[members]]\nid = \"{id}\"\nname = \"a\"\nstartup = \"active\"\n[[members]]\nid = \"{id}\"\nname = \"b\"\nstartup = \"active\"\n"
    );
    assert!(
        TemplateDocument::parse(&dup)
            .unwrap_err()
            .contains("duplicate member id")
    );
    assert!(
        TemplateDocument::parse(
            "name = \"t\"\n[[members]]\nname = \"a\"\nstartup = \"sometimes\"\n"
        )
        .is_err()
    );
}

#[test]
fn apply_words_parse_repeated_reuse_and_reject_malformed() {
    let fx = fx();
    let (m1, m2) = (MemberId::new(), MemberId::new());
    let (s1, s2) = (SeatId::new(), SeatId::new());
    let w = words(&format!(
        "application apply tpl --teamspace alpha --name app --reuse {m1}={s1} --reuse={m2}={s2}"
    ));
    let (kind, rest) = fx.deps.kinds.resolve(&w).unwrap();
    assert_eq!(kind.kind(), RequestKind::ApplicationApply);
    let args = kind.parse(&rest, &CallerInfo::default()).unwrap();
    assert_eq!(args["template"], json!("tpl"));
    assert_eq!(
        args["reuse"],
        json!({ m1.to_string(): s1.to_string(), m2.to_string(): s2.to_string() })
    );
    let bad = words("application apply tpl --teamspace alpha --name app --reuse nomember");
    let (kind, rest) = fx.deps.kinds.resolve(&bad).unwrap();
    assert!(kind.parse(&rest, &CallerInfo::default()).is_err());
    assert!(plan_err(&fx, "application apply tpl --teamspace alpha").contains("--name <app-name>"));
}

#[test]
fn edit_rejects_unchanged_document_and_rename() {
    let fx = fx();
    let a = MemberId::new();
    let members = [m(&a, "lead", "deferred")];
    create_template(&fx, "solo", &members, &[]);
    let same = write_doc(&fx, &doc_text("solo", &members, &[]));
    assert!(plan_err(&fx, &format!("template edit solo --from {same}")).contains("unchanged"));
    let renamed = write_doc(&fx, &doc_text("other", &members, &[]));
    assert!(plan_err(&fx, &format!("template edit solo --from {renamed}")).contains("rename"));
    let dup = write_doc(&fx, &doc_text("solo", &members, &[]));
    assert!(
        plan_err(&fx, &format!("template create solo --from {dup}")).contains("already exists")
    );
}

#[test]
fn edit_matches_idless_members_by_name_and_mints_ids_for_new_ones() {
    let fx = fx();
    alpha(&fx);
    let a = MemberId::new();
    create_template(&fx, "solo", &[m(&a, "lead", "deferred")], &[]);
    apply_app(&fx, "solo", "one");
    let kept = member_seat(&fx, "one", &a);
    let members = [
        M {
            id: None,
            name: "lead",
            startup: "deferred",
            model: Some("x"),
            agents: None,
        },
        M {
            id: None,
            name: "dev",
            startup: "deferred",
            model: None,
            agents: None,
        },
    ];
    edit(&fx, "solo", &members, &[]);
    let t = tpl(&fx, "solo");
    assert_eq!(
        t.members[0].id, a,
        "an id-less member keeps the id of the same-named member"
    );
    assert_ne!(t.members[1].id, a);
    assert_eq!(member_seat(&fx, "one", &a), kept);
    let dev = seat_by_id(&fx, &member_seat(&fx, "one", &t.members[1].id));
    assert_eq!(dev.name, "dev");
    assert_ne!(dev.id, kept, "the new member gets its own seat");
}

#[test]
fn stale_hydrate_plan_is_rejected_when_membership_changes() {
    let fx = fx();
    alpha(&fx);
    let (a, b) = (MemberId::new(), MemberId::new());
    create_template(&fx, "grow", &[m(&a, "lead", "deferred")], &[]);
    let pending = plan(&fx, "application apply grow --teamspace alpha --name one");
    edit(
        &fx,
        "grow",
        &[m(&a, "lead", "deferred"), m(&b, "dev", "deferred")],
        &[],
    );
    let row = apply_plan(&fx, &pending);
    assert_eq!(row.state, OpState::Rejected);
    assert_eq!(row.rejection.unwrap().reason, "stale_plan");
    assert!(
        layout::list_applications(&view(&fx)).unwrap().is_empty(),
        "nothing was hydrated"
    );
}

// Reusable seat definitions are live references, never shared instantiated identities.
fn create_doc(fx: &Fx, name: &str, text: &str) {
    let path = write_doc(fx, text);
    commit(fx, &format!("template create {name} --from {path}"));
}

#[test]
fn seat_template_document_kind_and_legacy_duty_compatibility() {
    let doc = TemplateDocument::parse(
        "name = 'engineer'\nkind = 'seat'\nagents_md = 'Reusable instructions'\n",
    )
    .unwrap();
    let json = serde_json::to_value(&doc).unwrap();
    assert_eq!(json["kind"], "seat");
    assert_eq!(json["agents_md"], "Reusable instructions");
    let legacy = TemplateDocument::parse("name = 'legacy'\n[[members]]\nname = 'worker'\nstartup = 'deferred'\nrole = 'dispatcher'\nrole_ref = 'unused'\n").unwrap();
    assert_eq!(legacy.members.len(), 1);
    let json = serde_json::to_value(legacy).unwrap();
    assert_eq!(json["kind"], "team");
    assert_eq!(json["members"][0]["system_duty"], "dispatcher");
    assert!(json["members"][0].get("role_ref").is_none());
    assert!(
        TemplateDocument::parse(
            "name = 'bad'\nkind = 'seat'\n[[members]]\nname = 'nested'\nstartup = 'deferred'\n"
        )
        .is_err()
    );
}

#[test]
fn seat_template_shared_definition_creates_independent_live_seats() {
    let fx = fx();
    alpha(&fx);
    create_doc(
        &fx,
        "engineer",
        "name = 'engineer'\nkind = 'seat'\nagents_md = 'Reusable engineering instructions'\n[defaults]\nmodel = 'seat-model'\n",
    );
    let shared = tpl(&fx, "engineer");
    for name in ["auth", "billing"] {
        create_doc(
            &fx,
            name,
            &format!(
                "name = '{name}'\n[defaults]\nmodel = 'team-model'\n[[members]]\nname = 'dev'\nstartup = 'active'\nseat_template = '{}'\nresponsibility = 'Deliver {name}'\nagents_md = '{name} specialization'\n",
                shared.id
            ),
        );
        apply_app(&fx, name, name);
    }
    let a = app(&fx, "auth").member_map.values().next().unwrap().clone();
    let b = app(&fx, "billing")
        .member_map
        .values()
        .next()
        .unwrap()
        .clone();
    assert_ne!(a, b);
    for id in [&a, &b] {
        assert_eq!(
            crate::model::effective::resolve_in(&view(&fx), &seat_by_id(&fx, id))
                .unwrap()
                .model
                .as_deref(),
            Some("seat-model")
        );
        patch(&fx, json!({"seat": id, "occupy": true}));
    }
    let text = "name = 'engineer'\nkind = 'seat'\nagents_md = 'Updated reusable instructions'\n[defaults]\nmodel = 'new-seat-model'\n";
    let path = write_doc(&fx, text);
    let p = plan(&fx, &format!("template edit engineer --from {path}"));
    assert_eq!(count(&p, "session.replace"), 2);
    assert_eq!(count(&p, "application.update"), 2);
    for e in p
        .plan
        .effects
        .iter()
        .filter(|e| e.kind == "session.replace")
    {
        assert_eq!(e.detail["from"]["model"], "seat-model");
        assert_eq!(e.detail["to"]["model"], "new-seat-model");
    }
    assert_eq!(apply_plan(&fx, &p).state, OpState::Committed);
    assert_eq!(app(&fx, "auth").member_map.values().next(), Some(&a));
    assert_eq!(app(&fx, "billing").member_map.values().next(), Some(&b));
    for id in [&a, &b] {
        assert_eq!(
            crate::model::effective::resolve_in(&view(&fx), &seat_by_id(&fx, id))
                .unwrap()
                .model
                .as_deref(),
            Some("new-seat-model")
        );
    }
}

#[test]
fn seat_template_reference_validation_and_dependency_revisions() {
    let fx = fx();
    alpha(&fx);
    let missing = TemplateId::new();
    let text = |id: &TemplateId| {
        format!(
            "name = 'team'\n[[members]]\nname = 'dev'\nstartup = 'active'\nseat_template = '{id}'\n"
        )
    };
    let path = write_doc(&fx, &text(&missing));
    assert!(plan_err(&fx, &format!("template create team --from {path}")).contains("missing"));
    create_doc(&fx, "wrong", "name = 'wrong'\n");
    let path = write_doc(&fx, &text(&tpl(&fx, "wrong").id));
    assert!(
        plan_err(&fx, &format!("template create team --from {path}")).contains("seat template")
    );
    create_doc(
        &fx,
        "engineer",
        "name = 'engineer'\nkind = 'seat'\n[defaults]\nmodel = 'old'\n",
    );
    let shared = tpl(&fx, "engineer");
    create_doc(&fx, "team", &text(&shared.id));
    let pending = plan(&fx, "application apply team --teamspace alpha --name one");
    let path = write_doc(
        &fx,
        "name = 'engineer'\nkind = 'seat'\n[defaults]\nmodel = 'new'\n",
    );
    commit(&fx, &format!("template edit engineer --from {path}"));
    assert_eq!(apply_plan(&fx, &pending).state, OpState::Rejected);
    assert!(
        plan_err(
            &fx,
            "application apply engineer --teamspace alpha --name bad"
        )
        .contains("team template")
    );
}

#[test]
fn seat_template_copy_and_undo_restore_instructions_and_runtime() {
    let fx = fx();
    alpha(&fx);
    create_doc(
        &fx,
        "engineer",
        "name = 'engineer'\nkind = 'seat'\nagents_md = 'original reusable instructions'\n[defaults]\nmodel = 'old'\n",
    );
    commit(&fx, "template copy engineer engineer-copy");
    let copied = tpl(&fx, "engineer-copy");
    let v = view(&fx);
    let loc = layout::locate(&v, &copied.id.to_any()).unwrap().unwrap();
    assert_eq!(
        String::from_utf8(
            v.read_file(&loc.folder.join("AGENTS.md").unwrap())
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        "original reusable instructions"
    );
    assert_eq!(serde_json::to_value(&copied).unwrap()["kind"], "seat");
    let shared = tpl(&fx, "engineer");
    create_doc(
        &fx,
        "team",
        &format!(
            "name = 'team'\n[[members]]\nname = 'dev'\nstartup = 'active'\nseat_template = '{}'\n",
            shared.id
        ),
    );
    apply_app(&fx, "team", "one");
    let id = app(&fx, "one").member_map.values().next().unwrap().clone();
    patch(&fx, json!({"seat": id, "occupy": true}));
    let path = write_doc(
        &fx,
        "name = 'engineer'\nkind = 'seat'\nagents_md = 'updated instructions'\n[defaults]\nmodel = 'new'\n",
    );
    let row = commit(&fx, &format!("template edit engineer --from {path}"));
    let act = action_of(&fx, &row);
    let undo = plan(&fx, &format!("undo {}", act.id));
    assert!(
        undo.plan.repair_required.is_none(),
        "{:?}",
        undo.plan.repair_required
    );
    assert_eq!(count(&undo, "session.replace"), 1);
    assert_eq!(apply_plan(&fx, &undo).state, OpState::Committed);
    let v = view(&fx);
    let loc = layout::locate(&v, &shared.id.to_any()).unwrap().unwrap();
    assert_eq!(
        String::from_utf8(
            v.read_file(&loc.folder.join("AGENTS.md").unwrap())
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        "original reusable instructions"
    );
    assert_eq!(
        crate::model::effective::resolve_in(&v, &seat_by_id(&fx, &id))
            .unwrap()
            .model
            .as_deref(),
        Some("old")
    );
}

#[test]
fn seat_template_runtime_precedence_at_every_level() {
    use crate::model::effective::resolve_with_seat_template;
    use crate::model::graph::GraphDefaults;
    use crate::model::template::MemberDefaults;
    let graph: GraphDefaults =
        toml::from_str("harness = 'shell'\nmodel = 'graph'\nargs = ['graph']\nsummaries = false\n")
            .unwrap();
    let doc =
        TemplateDocument::parse("name = 'team'\n[[members]]\nname = 'dev'\nstartup = 'deferred'\n")
            .unwrap();
    let mut team = doc.to_record(
        TemplateId::new(),
        1,
        &Default::default(),
        &mut Default::default(),
    );
    let mut member = team.members[0].clone();
    let mut shared = team.clone();
    shared.kind = crate::model::template::TemplateKind::Seat;
    shared.members.clear();
    let mut seat = super::structure::member_seat_record(
        SeatId::new(),
        &team.id,
        &member,
        &crate::model::TeamspaceId::new(),
        &crate::model::AppId::new(),
    );
    let defaults = |model: &str, harness, summaries| MemberDefaults {
        model: Some(model.into()),
        harness: Some(harness),
        args: Some(vec![model.into()]),
        summaries: Some(summaries),
    };
    for level in 0..6 {
        let (g, expected, harness, summaries) = match level {
            0 => (GraphDefaults::default(), None, Harness::Claude, true),
            1 => (graph.clone(), Some("graph"), Harness::Shell, false),
            2 => {
                team.defaults = defaults("team", Harness::Codex, true);
                (graph.clone(), Some("team"), Harness::Codex, true)
            }
            3 => {
                shared.defaults = defaults("shared", Harness::Claude, false);
                (graph.clone(), Some("shared"), Harness::Claude, false)
            }
            4 => {
                member.defaults = defaults("member", Harness::Shell, true);
                (graph.clone(), Some("member"), Harness::Shell, true)
            }
            _ => {
                seat.overrides.model = Some("instance".into());
                seat.overrides.harness = Some(Harness::Codex);
                seat.overrides.args = Some(vec!["instance".into()]);
                seat.overrides.summaries = Some(false);
                (graph.clone(), Some("instance"), Harness::Codex, false)
            }
        };
        let c = resolve_with_seat_template(&g, Some(&team), Some(&member), Some(&shared), &seat);
        assert_eq!(
            (c.model.as_deref(), c.harness, c.summaries),
            (expected, harness, summaries),
            "level {level}"
        );
        assert_eq!(
            c.args,
            expected.into_iter().map(str::to_owned).collect::<Vec<_>>(),
            "level {level}"
        );
    }
    seat.overrides.args = Some(vec![]);
    assert!(
        resolve_with_seat_template(&graph, Some(&team), Some(&member), Some(&shared), &seat)
            .args
            .is_empty()
    );
}

#[test]
fn seat_template_live_update_follows_explicit_reuse_after_origin_withdrawal() {
    let fx = fx();
    alpha(&fx);
    create_doc(
        &fx,
        "engineer",
        "name = 'engineer'\nkind = 'seat'\n[defaults]\nmodel = 'old'\n",
    );
    let shared = tpl(&fx, "engineer");
    create_doc(
        &fx,
        "auth",
        &format!(
            "name = 'auth'\n[[members]]\nname = 'dev'\nstartup = 'active'\nseat_template = '{}'\n",
            shared.id
        ),
    );
    apply_app(&fx, "auth", "origin");
    let seat = app(&fx, "origin")
        .member_map
        .values()
        .next()
        .unwrap()
        .clone();
    let mem = MemberId::new();
    create_template(&fx, "billing", &[m(&mem, "borrowed", "active")], &[]);
    commit(
        &fx,
        &format!(
            "application apply billing --teamspace alpha --name consumer --reuse {mem}={seat}"
        ),
    );
    patch(&fx, json!({"seat": seat, "occupy": true}));
    let structure = effective_structure(&view(&fx), &app(&fx, "consumer")).unwrap();
    assert_eq!(
        structure.seats[0].config.model.as_deref(),
        Some("old"),
        "reused seats retain their own configuration owner"
    );
    commit(&fx, "application retire origin");
    let path = write_doc(
        &fx,
        "name = 'engineer'\nkind = 'seat'\n[defaults]\nmodel = 'new'\n",
    );
    let p = plan(&fx, &format!("template edit engineer --from {path}"));
    assert_eq!(count(&p, "session.replace"), 1);
    assert_eq!(count(&p, "application.update"), 1);
    assert_eq!(apply_plan(&fx, &p).state, OpState::Committed);
    assert_eq!(member_seat(&fx, "consumer", &mem), seat);
    assert_eq!(
        crate::model::effective::resolve_in(&view(&fx), &seat_by_id(&fx, &seat))
            .unwrap()
            .model
            .as_deref(),
        Some("new")
    );
    commit(&fx, "application retire consumer");
    assert_eq!(seat_by_id(&fx, &seat).lifecycle, Lifecycle::Active);
    let path = write_doc(
        &fx,
        "name = 'engineer'\nkind = 'seat'\n[defaults]\nmodel = 'independent'\n",
    );
    let p = plan(&fx, &format!("template edit engineer --from {path}"));
    assert_eq!(
        count(&p, "session.replace"),
        1,
        "a retained independent seat still has its live definition"
    );
    assert_eq!(count(&p, "application.update"), 0);
    assert_eq!(apply_plan(&fx, &p).state, OpState::Committed);
}

#[test]
fn seat_template_instruction_undo_detects_later_clear_and_removes_new_file() {
    let fx = fx();
    create_doc(&fx, "engineer", "name = 'engineer'\nkind = 'seat'\n");
    let path = write_doc(
        &fx,
        "name = 'engineer'\nkind = 'seat'\nagents_md = 'first'\n",
    );
    let row = commit(&fx, &format!("template edit engineer --from {path}"));
    let first = action_of(&fx, &row);
    let path = write_doc(&fx, "name = 'engineer'\nkind = 'seat'\nagents_md = ''\n");
    let row = commit(&fx, &format!("template edit engineer --from {path}"));
    let clear = action_of(&fx, &row);
    let p = plan(&fx, &format!("undo {}", first.id));
    assert!(
        p.plan.repair_required.is_some(),
        "later removal conflicts with the earlier text edit"
    );
    commit(&fx, &format!("undo {}", clear.id));
    commit(&fx, &format!("undo {}", first.id));
    let v = view(&fx);
    let loc = layout::locate(&v, &tpl(&fx, "engineer").id.to_any())
        .unwrap()
        .unwrap();
    assert!(
        v.read_file(&loc.folder.join("AGENTS.md").unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn seat_template_edits_preserve_exclusions_overrides_and_copied_member_correspondence() {
    let fx = fx();
    alpha(&fx);
    create_doc(
        &fx,
        "engineer",
        "name = 'engineer'\nkind = 'seat'\n[defaults]\nmodel = 'old'\n",
    );
    let shared = tpl(&fx, "engineer");
    create_doc(
        &fx,
        "team",
        &format!(
            "name = 'team'\n[[members]]\nname = 'dev'\nstartup = 'active'\nseat_template = '{}'\n",
            shared.id
        ),
    );
    commit(&fx, "template copy team team-copy");
    let member = tpl(&fx, "team").members[0].id.clone();
    assert_eq!(tpl(&fx, "team-copy").members[0].id, member);
    for (template, name) in [
        ("team", "excluded"),
        ("team", "overridden"),
        ("team-copy", "live"),
    ] {
        apply_app(&fx, template, name);
        patch(
            &fx,
            json!({"seat": member_seat(&fx, name, &member), "occupy": true}),
        );
    }
    let excluded = member_seat(&fx, "excluded", &member);
    let overridden = member_seat(&fx, "overridden", &member);
    let live = member_seat(&fx, "live", &member);
    commit(&fx, &format!("seat retire {excluded}"));
    patch(
        &fx,
        json!({"seat": overridden, "overrides": {"model": "local"}}),
    );
    let pending = plan(
        &fx,
        "application apply team --teamspace alpha --name pending",
    );
    let path = write_doc(
        &fx,
        "name = 'engineer'\nkind = 'seat'\nagents_md = 'live instructions'\n[defaults]\nmodel = 'old'\n",
    );
    commit(&fx, &format!("template edit engineer --from {path}"));
    assert_eq!(
        apply_plan(&fx, &pending).state,
        OpState::Rejected,
        "instruction-only changes invalidate dependent plans"
    );
    let path = write_doc(
        &fx,
        "name = 'engineer'\nkind = 'seat'\n[defaults]\nmodel = 'new'\n",
    );
    let p = plan(&fx, &format!("template edit engineer --from {path}"));
    assert_eq!(count(&p, "session.replace"), 1);
    assert_eq!(apply_plan(&fx, &p).state, OpState::Committed);
    assert_eq!(app(&fx, "excluded").exclusions, vec![member.clone()]);
    assert_eq!(seat_by_id(&fx, &excluded).lifecycle, Lifecycle::Retired);
    assert_eq!(member_seat(&fx, "overridden", &member), overridden);
    assert_eq!(member_seat(&fx, "live", &member), live);
    assert_eq!(
        crate::model::effective::resolve_in(&view(&fx), &seat_by_id(&fx, &overridden))
            .unwrap()
            .model
            .as_deref(),
        Some("local")
    );
    assert_eq!(
        crate::model::effective::resolve_in(&view(&fx), &seat_by_id(&fx, &live))
            .unwrap()
            .model
            .as_deref(),
        Some("new")
    );
}

#[test]
fn review_retained_seat_reference_switch_and_undo_preview_replacements() {
    for retire_borrower in [false, true] {
        let fx = fx();
        alpha(&fx);
        for (name, model) in [("definition-a", "a"), ("definition-b", "b")] {
            create_doc(
                &fx,
                name,
                &format!("name = '{name}'\nkind = 'seat'\n[defaults]\nmodel = '{model}'\n"),
            );
        }
        let a = tpl(&fx, "definition-a").id;
        let b = tpl(&fx, "definition-b").id;
        let member = MemberId::new();
        let doc = |definition: &TemplateId| {
            format!(
                "name = 'owner'\n[[members]]\nid = '{member}'\nname = 'dev'\nstartup = 'active'\nseat_template = '{definition}'\n"
            )
        };
        create_doc(&fx, "owner", &doc(&a));
        apply_app(&fx, "owner", "origin");
        let seat = member_seat(&fx, "origin", &member);
        let borrowed_member = MemberId::new();
        create_template(
            &fx,
            "borrower",
            &[m(&borrowed_member, "peer", "active")],
            &[],
        );
        commit(
            &fx,
            &format!(
                "application apply borrower --teamspace alpha --name consumer --reuse {borrowed_member}={seat}"
            ),
        );
        patch(&fx, json!({"seat": seat, "occupy": true}));
        commit(&fx, "application retire origin");
        if retire_borrower {
            commit(&fx, "application retire consumer");
        }
        let path = write_doc(&fx, &doc(&b));
        let p = plan(&fx, &format!("template edit owner --from {path}"));
        assert_eq!(
            count(&p, "session.replace"),
            1,
            "retire_borrower={retire_borrower}"
        );
        assert_eq!(
            count(&p, "application.update"),
            usize::from(!retire_borrower)
        );
        let replacement = p
            .plan
            .effects
            .iter()
            .find(|e| e.kind == "session.replace")
            .unwrap();
        assert_eq!(replacement.detail["from"]["model"], "a");
        assert_eq!(replacement.detail["to"]["model"], "b");
        let row = apply_plan(&fx, &p);
        assert_eq!(row.state, OpState::Committed);
        let action = action_of(&fx, &row);
        let undo = plan(&fx, &format!("undo {}", action.id));
        assert!(
            undo.plan.repair_required.is_none(),
            "{:?}",
            undo.plan.repair_required
        );
        assert_eq!(count(&undo, "session.replace"), 1);
        let replacement = undo
            .plan
            .effects
            .iter()
            .find(|e| e.kind == "session.replace")
            .unwrap();
        assert_eq!(replacement.detail["from"]["model"], "b");
        assert_eq!(replacement.detail["to"]["model"], "a");
        assert_eq!(apply_plan(&fx, &undo).state, OpState::Committed);
        assert_eq!(
            crate::model::effective::resolve_in(&view(&fx), &seat_by_id(&fx, &seat))
                .unwrap()
                .model
                .as_deref(),
            Some("a")
        );
    }
}

#[test]
fn review_added_reusable_member_without_instructions_undoes_immediately() {
    let fx = fx();
    alpha(&fx);
    create_doc(&fx, "engineer", "name = 'engineer'\nkind = 'seat'\n");
    create_doc(&fx, "team", "name = 'team'\n");
    apply_app(&fx, "team", "one");
    let shared = tpl(&fx, "engineer").id;
    let path = write_doc(
        &fx,
        &format!(
            "name = 'team'\n[[members]]\nname = 'dev'\nstartup = 'active'\nseat_template = '{shared}'\n"
        ),
    );
    let row = commit(&fx, &format!("template edit team --from {path}"));
    let action = action_of(&fx, &row);
    let seat = app(&fx, "one").member_map.values().next().unwrap().clone();
    patch(&fx, json!({"seat": seat, "occupy": true}));
    let undo = plan(&fx, &format!("undo {}", action.id));
    assert!(
        undo.plan.repair_required.is_none(),
        "{:?}",
        undo.plan.repair_required
    );
    assert_eq!(count(&undo, "seat.retire"), 1);
    assert_eq!(
        count(&undo, "session.replace"),
        0,
        "a withdrawn seat is retired, not replaced"
    );
    assert_eq!(apply_plan(&fx, &undo).state, OpState::Committed);
    assert!(tpl(&fx, "team").members.is_empty());
    assert_eq!(seat_by_id(&fx, &seat).lifecycle, Lifecycle::Retired);
}

#[test]
fn member_rename_swap_preserves_each_identity_and_undo() {
    let fx = fx();
    alpha(&fx);
    let first = MemberId::new();
    let second = MemberId::new();
    let mut members = [m(&first, "dev", "active"), m(&second, "reviewer", "active")];
    members[0].agents = Some("developer specialization");
    members[1].agents = Some("reviewer specialization");
    create_template(&fx, "team", &members, &[]);
    apply_app(&fx, "team", "one");
    let mapping = app(&fx, "one").member_map;
    let dir = layout::locate(&view(&fx), &tpl(&fx, "team").id.to_any())
        .unwrap()
        .unwrap()
        .folder;
    let bytes = |name| {
        view(&fx)
            .read_file(&layout::member_agents_md(&dir, name))
            .unwrap()
    };
    let path = write_doc(
        &fx,
        &doc_text(
            "team",
            &[m(&first, "reviewer", "active"), m(&second, "dev", "active")],
            &[],
        ),
    );
    let row = commit(&fx, &format!("template edit team --from {path}"));
    assert_eq!(
        bytes("dev").as_deref(),
        Some(b"reviewer specialization".as_slice())
    );
    assert_eq!(
        bytes("reviewer").as_deref(),
        Some(b"developer specialization".as_slice())
    );
    assert_eq!(app(&fx, "one").member_map, mapping);
    commit(&fx, &format!("undo {}", row.action.unwrap()));
    assert_eq!(
        bytes("dev").as_deref(),
        Some(b"developer specialization".as_slice())
    );
    assert_eq!(
        bytes("reviewer").as_deref(),
        Some(b"reviewer specialization".as_slice())
    );
    assert_eq!(app(&fx, "one").member_map, mapping);
}

#[test]
fn member_rename_into_departing_path_does_not_inherit_other_identity_text() {
    for replacement in [false, true] {
        let fx = fx();
        let first = MemberId::new();
        let second = MemberId::new();
        let mut members = [
            m(&first, "dev", "deferred"),
            m(&second, "reviewer", "deferred"),
        ];
        members[1].agents = Some("reviewer specialization");
        create_template(&fx, "team", &members, &[]);
        let id = if replacement {
            MemberId::new()
        } else {
            first.clone()
        };
        let path = write_doc(
            &fx,
            &doc_text("team", &[m(&id, "reviewer", "deferred")], &[]),
        );
        let row = commit(&fx, &format!("template edit team --from {path}"));
        let dir = layout::locate(&view(&fx), &tpl(&fx, "team").id.to_any())
            .unwrap()
            .unwrap()
            .folder;
        assert_eq!(
            view(&fx)
                .read_file(&layout::member_agents_md(&dir, "reviewer"))
                .unwrap(),
            None,
            "an omitted instruction belongs to the stable member, never the destination's previous owner"
        );
        commit(&fx, &format!("undo {}", row.action.unwrap()));
        assert_eq!(
            view(&fx)
                .read_file(&layout::member_agents_md(&dir, "reviewer"))
                .unwrap()
                .as_deref(),
            Some(b"reviewer specialization".as_slice())
        );
        assert_eq!(
            view(&fx)
                .read_file(&layout::member_agents_md(&dir, "dev"))
                .unwrap(),
            None
        );
    }
}

#[test]
fn member_rename_rejects_colliding_instruction_paths_before_writing() {
    let fx = fx();
    let first = MemberId::new();
    let second = MemberId::new();
    create_template(
        &fx,
        "team",
        &[
            m(&first, "dev", "deferred"),
            m(&second, "reviewer", "deferred"),
        ],
        &[],
    );
    let head = fx.store.head().unwrap();
    for (left, right) in [("Dev Lead", "dev-lead"), ("DEV", "dev")] {
        let path = write_doc(
            &fx,
            &doc_text(
                "team",
                &[m(&first, left, "deferred"), m(&second, right, "deferred")],
                &[],
            ),
        );
        let err = plan_err(&fx, &format!("template edit team --from {path}"));
        assert!(
            err.contains("instruction path") && err.contains("rename"),
            "{err}"
        );
        assert_eq!(fx.store.head().unwrap(), head);
    }
    let path = write_doc(
        &fx,
        &doc_text(
            "new",
            &[
                m(&first, "Dev Lead", "deferred"),
                m(&second, "dev-lead", "deferred"),
            ],
            &[],
        ),
    );
    let err = plan_err(&fx, &format!("template create new --from {path}"));
    assert!(err.contains("instruction path"), "{err}");
    assert_eq!(fx.store.head().unwrap(), head);
}
