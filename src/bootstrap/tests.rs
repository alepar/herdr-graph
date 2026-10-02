use super::content::{self, b64_decode, b64_encode};
use super::resolve::{Resolution, proposal_words, resolve_caller};
use super::setup_claude::{self, HookChange};
use super::*;
use crate::daemon::registry::CallerInfo;
use crate::journal::{Journal, OpRow};
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::clone::{Invitation, InvitationState, InviteConstraint, ThreadsLink};
use crate::model::common::{Availability, Binding, Lifecycle, Occupant, Role};
use crate::model::effective::resolve_in;
use crate::model::harness::Harness;
use crate::model::native_session::NativeSession;
use crate::model::operation::OpState;
use crate::model::seat::SeatRecord;
use crate::model::template::Startup;
use crate::model::{AnyId, HerdrPaneId, NsId, SeatId, TeamspaceId};
use crate::plan::commands::{PlanDeps, admit_apply};
use crate::plan::core_kinds::register_core_kinds;
use crate::plan::kind::KindRegistry;
use crate::plan::kinds_extra::register_kinds as register_extra_kinds;
use crate::plan::store::PlanStore;
use crate::plan::types::StoredPlan;
use crate::ports::clock::ManualClock;
use crate::ports::herdr::{AgentInfo, AgentSession, AgentStatus, PaneInfo};
use crate::ports::store::Store;
use crate::store::GitStore;
use crate::store::init::init_instance;
use crate::templates::kinds::register_kinds as register_template_kinds;
use crate::writer::{Applied, Mutation, MutationCx, MutationError, WriterConfig, WriterCore};
use chrono::TimeZone;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------------------------
// fixture
// ---------------------------------------------------------------------------------------------

fn t0() -> crate::model::Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

/// Test-only clone patch (`bookkeeping.test_clone`): pane binding, availability, session/occupant,
/// reload flag and a pending required invitation. Stands in for the observer and threads tasks.
struct TestClone;
impl Mutation for TestClone {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a = &cx.request.args;
        let id: CloneId = a["clone"].as_str().unwrap().parse().unwrap();
        let loc = layout::locate(&cx.tree, &id.to_any())?.unwrap();
        let mut rec: CloneRecord = read_toml(&cx.tree, &loc.record_path)?.unwrap();
        if let Some(p) = a["pane"].as_str() {
            rec.runtime.bound = Some(Binding { pane_id: Some(HerdrPaneId(p.into())), ..Default::default() });
        }
        if a["unbind"].as_bool() == Some(true) {
            rec.runtime.bound = None;
        }
        match a["availability"].as_str() {
            Some("unknown") => rec.runtime.availability = Availability::Unknown,
            Some(_) => rec.runtime.availability = Availability::Present,
            None => {}
        }
        if let Some(native) = a["session"].as_str() {
            let ns = NsId::new();
            rec.sessions.push(NativeSession {
                id: ns.clone(),
                harness: Harness::Claude,
                native_session_id: native.to_owned(),
                transcript_path: None,
                transcript: None,
                cwd: "/tmp".into(),
                started: cx.now,
                ended: None,
                end_reason: None,
            });
            rec.occupant = Some(Occupant { native_session: ns, harness: Harness::Claude, since: cx.now });
        }
        if let Some(r) = a["reload_required"].as_bool() {
            rec.reload_required = r;
        }
        if a["invite"].as_bool() == Some(true) {
            rec.invitations.push(Invitation {
                thread: "team".into(),
                constraint: InviteConstraint::Required,
                state: InvitationState::Pending,
                link: Some(ThreadsLink {
                    seat: "ts-seat".into(),
                    occupant: None,
                    invitation: Some("inv1".into()),
                    requirement: Some("req1".into()),
                    revision: Some(2),
                }),
            });
        }
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: "test clone".into(), action: None })
    }
}

struct Fx {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
    deps: BootstrapDeps,
    w: Arc<WriterCore>,
    store: Arc<GitStore>,
}

fn fx() -> Fx {
    fx_with(|_| {})
}

/// A fixture over a freshly initialised instance; `prepare` runs before the writer and store open it.
fn fx_with(prepare: impl FnOnce(&Path)) -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("inst");
    init_instance(&root).unwrap();
    prepare(&root);
    let mut kinds = KindRegistry::default();
    register_core_kinds(&mut kinds);
    register_extra_kinds(&mut kinds);
    register_template_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    let plans = Arc::new(PlanStore::new(root.join(".graph-local/plans")));
    let mut reg = crate::writer::MutationRegistry::default();
    kinds.register_mutations(&mut reg, plans.clone());
    register_mutations(&mut reg);
    reg.register("bookkeeping.test_clone", Arc::new(TestClone));
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let w = WriterCore::new(store.clone(), journal.clone(), Arc::new(reg), clock.clone(), WriterConfig::default());
    let plan = Arc::new(PlanDeps { kinds, plans, store: store.clone(), writer: w.clone(), clock, instance: root.clone() });
    Fx { _tmp: tmp, root, deps: BootstrapDeps { plan, journal, herdr: None }, w, store }
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn plan(fx: &Fx, change: &str) -> StoredPlan {
    let v = create_plan(&fx.deps.plan, &CallerInfo::default(), words(change))
        .unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    fx.deps.plan.plans.get(&id).unwrap().unwrap()
}

fn commit(fx: &Fx, change: &str) -> OpRow {
    let sp = plan(fx, change);
    let op = admit_apply(&fx.deps.plan, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay")
        .unwrap_or_else(|e| panic!("admit {change}: {}", e.message));
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(row.state, OpState::Committed, "{change}: {:?}", row.rejection);
    row
}

fn view(fx: &Fx) -> CommitView<'_> {
    CommitView { store: &*fx.store, at: fx.store.head().unwrap() }
}

fn teamspace(fx: &Fx, name: &str) -> TeamspaceId {
    let found: Vec<_> = layout::list_teamspaces(&view(fx)).unwrap().into_iter().filter(|(_, t)| t.name == name).collect();
    assert_eq!(found.len(), 1, "teamspace {name}");
    found[0].1.id.clone()
}

fn seat_rec(fx: &Fx, name: &str) -> SeatRecord {
    let mut found: Vec<_> = layout::all_seats(&view(fx)).unwrap().into_iter().filter(|(_, s)| s.name == name).collect();
    assert_eq!(found.len(), 1, "seat {name}");
    found.remove(0).1
}

fn clone_of(fx: &Fx, seat: &SeatId) -> CloneRecord {
    let mut found: Vec<_> =
        layout::all_clones(&view(fx)).unwrap().into_iter().map(|(_, c)| c).filter(|c| &c.seat == seat).collect();
    assert_eq!(found.len(), 1, "one clone of {seat}");
    found.remove(0)
}

fn patch_clone(fx: &Fx, clone: &CloneId, patch: serde_json::Value) {
    use crate::ports::writer::Writer;
    let mut args = patch;
    args["sub"] = json!("test_clone");
    args["clone"] = json!(clone);
    let req = ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args,
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
    };
    let op = fx.w.admit(req).unwrap();
    fx.w.drain().unwrap();
    assert_eq!(fx.w.journal().get(&op).unwrap().unwrap().state, OpState::Committed);
}

/// alpha / one (active, one clone) with the clone bound to `pane` and present.
fn bound_seat(fx: &Fx, pane: &str) -> (SeatRecord, CloneRecord) {
    commit(fx, "teamspace create alpha --active");
    commit(fx, "seat create one --teamspace alpha --active");
    let seat = seat_rec(fx, "one");
    let clone = clone_of(fx, &seat.id);
    patch_clone(fx, &clone.id, json!({ "pane": pane, "availability": "present" }));
    let clone = clone_of(fx, &seat.id);
    (seat, clone)
}

fn caller(pane: &str) -> CallerInfo {
    CallerInfo { pane_id: Some(pane.into()), ..CallerInfo::default() }
}

fn write(fx: &Fx, object: &str, rel: &str, bytes: &[u8], expect: Option<&str>) -> OpRow {
    let args = json!({ "object": object, "rel": rel, "bytes_b64": b64_encode(bytes), "expect": expect });
    let op = content::admit_write(&fx.deps.plan, &CallerInfo::default(), &args)
        .unwrap_or_else(|e| panic!("admit write {rel}: {}", e.message));
    fx.w.drain().unwrap();
    fx.w.journal().get(&op).unwrap().unwrap()
}

fn file_in(fx: &Fx, object: &str, rel: &str) -> Option<Vec<u8>> {
    let head = fx.store.head().unwrap();
    let loc = fx.store.locate(&head, &AnyId::parse(object).unwrap()).unwrap()?;
    fx.store.read_file(&head, &loc.folder.join(rel).unwrap()).unwrap()
}

fn pane_with_session(session: &str) -> PaneInfo {
    PaneInfo {
        id: HerdrPaneId("p9".into()),
        terminal_id: None,
        label: None,
        cwd: None,
        metadata: BTreeMap::new(),
        agent: Some(AgentInfo {
            kind: "claude".into(),
            status: AgentStatus::Idle,
            session: Some(AgentSession::Id(session.into())),
        }),
    }
}

// ---------------------------------------------------------------------------------------------
// resolution
// ---------------------------------------------------------------------------------------------

#[test]
fn resolve_bound_by_pane_binding() {
    let fx = fx();
    let (seat, clone) = bound_seat(&fx, "p1");
    let r = resolve_caller(&view(&fx), &caller("p1"), None);
    match r {
        Resolution::Bound { teamspace, seat: s, clone: c, via, proposal, native_session } => {
            assert_eq!((teamspace, s, c), (seat.teamspace.clone(), seat.id.clone(), clone.id.clone()));
            assert_eq!(via, "binding");
            assert!(proposal.is_none() && native_session.is_none());
        }
        other => panic!("expected bound, got {other:?}"),
    }
    // A pane nobody is bound to does not resolve by binding.
    assert!(matches!(resolve_caller(&view(&fx), &caller("p2"), None), Resolution::Unbound { .. }));
}

#[test]
fn resolve_bound_by_env_when_binding_stale_proposes_rebind() {
    let fx = fx();
    let (_, clone) = bound_seat(&fx, "p_old");
    let c = CallerInfo { pane_id: Some("p_new".into()), graph_clone: Some(clone.id.to_string()), ..Default::default() };
    let r = resolve_caller(&view(&fx), &c, None);
    assert!(matches!(&r, Resolution::Bound { via, clone: id, .. } if via == "env" && *id == clone.id), "{r:?}");
    let words = proposal_words(&view(&fx), &c, None, &r).expect("a rebind proposal");
    assert_eq!(words, ["clone", "rebind", clone.id.as_str(), "--pane", "p_new"]);
    // The proposed words are a valid plan.
    assert!(create_plan(&fx.deps.plan, &c, words).is_ok());
    // A fresh binding needs no proposal.
    let fresh = CallerInfo { pane_id: Some("p_old".into()), graph_clone: Some(clone.id.to_string()), ..Default::default() };
    let r = resolve_caller(&view(&fx), &fresh, None);
    assert!(matches!(&r, Resolution::Bound { via, .. } if via == "binding"), "{r:?}");
    assert_eq!(proposal_words(&view(&fx), &fresh, None, &r), None);
}

#[test]
fn resolve_by_agent_session() {
    let fx = fx();
    let (_, clone) = bound_seat(&fx, "p_old");
    patch_clone(&fx, &clone.id, json!({ "session": "native-77" }));
    let clone = clone_of(&fx, &clone.seat);
    let pane = pane_with_session("native-77");
    let r = resolve_caller(&view(&fx), &caller("p9"), Some(&pane));
    match &r {
        Resolution::Bound { clone: id, via, native_session, .. } => {
            assert_eq!(id, &clone.id);
            assert_eq!(via, "agent_session");
            assert_eq!(native_session.as_ref(), clone.occupant.as_ref().map(|o| &o.native_session));
        }
        other => panic!("expected bound by session, got {other:?}"),
    }
    let other_pane = pane_with_session("native-other");
    assert!(matches!(resolve_caller(&view(&fx), &caller("p9"), Some(&other_pane)), Resolution::Unbound { .. }));
}

#[test]
fn resolve_unbound_proposes_create() {
    let fx = fx();
    let (seat, _) = bound_seat(&fx, "p1");
    // The env names a seat whose only clone is confirmed in another pane: nothing to rebind.
    let c = CallerInfo {
        pane_id: Some("p2".into()),
        graph_seat: Some(seat.id.to_string()),
        ..Default::default()
    };
    let pane = PaneInfo {
        id: HerdrPaneId("p2".into()),
        terminal_id: None,
        label: Some("worker ·abc123".into()),
        cwd: None,
        metadata: BTreeMap::new(),
        agent: None,
    };
    let r = resolve_caller(&view(&fx), &c, Some(&pane));
    assert!(matches!(&r, Resolution::Unbound { candidates, .. } if candidates.is_empty()), "{r:?}");
    let words = proposal_words(&view(&fx), &c, Some(&pane), &r).expect("a create proposal");
    assert_eq!(words, ["seat", "create", "worker", "--teamspace", seat.teamspace.as_str()]);
    assert!(create_plan(&fx.deps.plan, &c, words).is_ok(), "the proposal is a plannable change");
    // Without anything identifying a teamspace there is nothing to propose.
    assert_eq!(proposal_words(&view(&fx), &caller("p2"), Some(&pane), &r), None);
}

#[test]
fn resolve_ambiguous_lists_candidates() {
    let fx = fx();
    let (_, one) = bound_seat(&fx, "p1");
    commit(&fx, "seat create two --teamspace alpha --active");
    let two = clone_of(&fx, &seat_rec(&fx, "two").id);
    patch_clone(&fx, &two.id, json!({ "pane": "p1" }));
    let r = resolve_caller(&view(&fx), &caller("p1"), None);
    match &r {
        Resolution::Ambiguous { candidates, proposal } => {
            let mut ids: Vec<_> = candidates.iter().map(|c| c.clone.clone()).collect();
            ids.sort();
            let mut want = vec![one.id.clone(), two.id.clone()];
            want.sort();
            assert_eq!(ids, want);
            assert!(proposal.is_none());
        }
        other => panic!("expected ambiguous, got {other:?}"),
    }
    assert_eq!(proposal_words(&view(&fx), &caller("p1"), None, &r), None, "the user decides which clone this is");
}

#[test]
fn resolve_moved_pane_reload_required_candidate() {
    let fx = fx();
    let (seat, clone) = bound_seat(&fx, "p_old");
    patch_clone(&fx, &clone.id, json!({ "reload_required": true }));
    let c = CallerInfo { pane_id: Some("p_new".into()), graph_seat: Some(seat.id.to_string()), ..Default::default() };
    let r = resolve_caller(&view(&fx), &c, None);
    match &r {
        Resolution::Unbound { candidates, .. } => {
            assert_eq!(candidates.len(), 1, "{candidates:?}");
            assert_eq!(candidates[0].clone, clone.id);
            assert!(candidates[0].reason.contains("reload_required"), "{}", candidates[0].reason);
        }
        other => panic!("expected unbound, got {other:?}"),
    }
    let words = proposal_words(&view(&fx), &c, None, &r).unwrap();
    assert_eq!(words, ["clone", "rebind", clone.id.as_str(), "--pane", "p_new"]);
}

#[test]
fn resolve_retired_seat_proposes_resurrect() {
    let fx = fx();
    let (seat, _) = bound_seat(&fx, "p1");
    commit(&fx, "seat retire one");
    let c = CallerInfo { pane_id: Some("p2".into()), graph_seat: Some(seat.id.to_string()), ..Default::default() };
    let r = resolve_caller(&view(&fx), &c, None);
    assert!(matches!(&r, Resolution::Unbound { candidates, .. } if candidates.is_empty()), "{r:?}");
    assert_eq!(proposal_words(&view(&fx), &c, None, &r).unwrap(), ["seat", "resurrect", seat.id.as_str()]);
}

// ---------------------------------------------------------------------------------------------
// /seat output
// ---------------------------------------------------------------------------------------------

#[test]
fn seat_json_includes_paths_pending_ops_invitations_beads_view_rev() {
    let fx = fx();
    let (seat, clone) = bound_seat(&fx, "p1");
    let seat_id = seat.id.to_string();
    let ts_id = seat.teamspace.to_string();

    // Seat and team files the agent must read, written through the real content path.
    assert_eq!(write(&fx, &seat_id, "AGENTS.md", b"v1", None).state, OpState::Committed);
    assert_eq!(write(&fx, &ts_id, "rules/style.md", b"be brief", None).state, OpState::Committed);
    assert_eq!(write(&fx, &clone.id.to_string(), "notes/n.md", b"n", None).state, OpState::Committed);
    // A local edit the graph must not overwrite, then a newer committed version of the same file.
    let agents = fx.root.join(layout::locate(&view(&fx), &seat.id.to_any()).unwrap().unwrap().folder.as_str()).join("AGENTS.md");
    std::fs::write(&agents, "local edit").unwrap();
    assert_eq!(write(&fx, &seat_id, "AGENTS.md", b"v2", None).state, OpState::Committed);

    // A rejected op the seat asked for, a pending required invitation and a reload flag.
    let op = {
        use crate::ports::writer::Writer;
        let op = fx
            .w
            .admit(ChangeRequest {
                kind: RequestKind::SeatRetire,
                args: json!({ "seat": "nobody" }),
                relied_on: vec![],
                requester: Requester { seat: Some(seat.id.clone()), ..Default::default() },
                supersedes: None,
            })
            .unwrap();
        fx.w.drain().unwrap();
        assert_eq!(fx.w.journal().get(&op).unwrap().unwrap().state, OpState::Rejected);
        op
    };
    patch_clone(&fx, &clone.id, json!({ "invite": true, "reload_required": true, "session": "native-1" }));

    let reply = seat_reply(&fx.deps, &caller("p1"), None).unwrap();
    assert_eq!(reply["resolution"]["status"], "bound");
    assert_eq!(reply["resolution"]["seat"], seat_id.as_str());
    let head = fx.store.head().unwrap().0;
    assert_eq!(reply["head"], head.as_str());
    assert_eq!(reply["view_rev"], head.as_str(), "the working tree mirrors the head");
    assert_eq!(reply["instance"], fx.root.display().to_string().as_str());

    let seat_folder = fx.root.join(layout::locate(&view(&fx), &seat.id.to_any()).unwrap().unwrap().folder.as_str());
    let p = &reply["paths"];
    assert_eq!(p["seat_agents_md"], seat_folder.join("AGENTS.md").display().to_string().as_str());
    assert_eq!(p["seat_folder"], seat_folder.display().to_string().as_str());
    let team = p["rules"]["team"].as_array().unwrap();
    assert_eq!(team.len(), 1);
    assert!(team[0].as_str().unwrap().ends_with("teamspaces/alpha/rules/style.md"), "{team:?}");
    assert_eq!(p["rules"]["global"], json!([]));
    assert_eq!(p["rules"]["seat"], json!([]));
    let clone_files: Vec<&str> = p["clone_files"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
    assert!(clone_files.iter().any(|f| f.ends_with("notes/n.md")), "{clone_files:?}");
    assert!(clone_files.iter().any(|f| f.ends_with("clone.toml")), "{clone_files:?}");
    assert!(std::path::Path::new(p["seat_agents_md"].as_str().unwrap()).exists(), "paths are real working-tree paths");

    let dirty: Vec<&str> = reply["worktree_dirty"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
    assert!(dirty.iter().any(|d| d.ends_with("AGENTS.md")), "{dirty:?}");

    let ops = reply["pending_ops"].as_array().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["op"], op.to_string().as_str());
    assert_eq!(ops[0]["state"], "rejected");

    let inv = reply["pending_invitations"].as_array().unwrap();
    assert_eq!(inv.len(), 1);
    assert_eq!(inv[0]["thread"], "team");
    assert_eq!(
        inv[0]["accept_command"],
        "herdr-threads accept-required team --invitation inv1 --requirement req1 --revision 2"
    );
    assert_eq!(reply["reload_required"], true);
    assert_eq!(reply["beads_query"], format!("bd list --label hg-seat:{seat_id}").as_str());
    let labels = &reply["beads_labels"];
    assert_eq!(labels["ts"], format!("hg-ts:{ts_id}").as_str());
    assert_eq!(labels["seat"], format!("hg-seat:{seat_id}").as_str());
    assert_eq!(labels["clone"], format!("hg-clone:{}", clone.id).as_str());
    assert!(labels["ns"].as_str().unwrap().starts_with("hg-ns:ns_"), "{labels}");

    // The human rendering names the essentials.
    let text = render_seat(&reply);
    assert!(text.contains("accept-required team"), "{text}");
    assert!(text.contains(&format!("bd list --label hg-seat:{seat_id}")), "{text}");
}

#[test]
fn seat_reply_for_a_stale_binding_carries_a_stored_unapplied_rebind_plan() {
    let fx = fx();
    let (_, clone) = bound_seat(&fx, "p_old");
    let c = CallerInfo { pane_id: Some("p_new".into()), graph_clone: Some(clone.id.to_string()), ..Default::default() };
    let before = fx.w.journal().list(&[], 100).unwrap().len();
    let reply = seat_reply(&fx.deps, &c, None).unwrap();
    let proposal = &reply["resolution"]["proposal"];
    assert_eq!(proposal["words"], json!(["clone", "rebind", clone.id.as_str(), "--pane", "p_new"]));
    let id: PlanId = proposal["plan_id"].as_str().unwrap().parse().unwrap();
    let stored = fx.deps.plan.plans.get(&id).unwrap().expect("the proposal is a stored plan");
    assert_eq!(stored.hash, proposal["hash"].as_str().unwrap());
    assert_eq!(fx.w.journal().list(&[], 100).unwrap().len(), before, "/seat never applies a proposal");
    assert_eq!(clone_of(&fx, &clone.seat).runtime.bound.unwrap().pane_id.unwrap().0, "p_old");
}

#[test]
fn seat_reply_for_an_unknown_pane_is_unbound_with_no_paths() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    let reply = seat_reply(&fx.deps, &caller("nope"), None).unwrap();
    assert_eq!(reply["resolution"]["status"], "unbound");
    assert_eq!(reply["beads_query"], serde_json::Value::Null);
    assert_eq!(reply["pending_ops"], json!([]));
}

// ---------------------------------------------------------------------------------------------
// content write
// ---------------------------------------------------------------------------------------------

#[test]
fn content_write_resolves_current_folder_after_rename() {
    let fx = fx();
    commit(&fx, "teamspace create alpha --active");
    commit(&fx, "seat create one --teamspace alpha");
    let seat = seat_rec(&fx, "one");
    let id = seat.id.to_string();
    assert_eq!(write(&fx, &id, "notes/a.md", b"first", None).state, OpState::Committed);
    let old_folder = layout::locate(&view(&fx), &seat.id.to_any()).unwrap().unwrap().folder;

    commit(&fx, "seat rename one two");
    let new_folder = layout::locate(&view(&fx), &seat.id.to_any()).unwrap().unwrap().folder;
    assert_ne!(old_folder, new_folder);

    assert_eq!(write(&fx, &id, "notes/b.md", b"second", None).state, OpState::Committed);
    assert_eq!(file_in(&fx, &id, "notes/a.md").unwrap(), b"first", "earlier file moved with the folder");
    assert_eq!(file_in(&fx, &id, "notes/b.md").unwrap(), b"second");
    let head = fx.store.head().unwrap();
    let stale = old_folder.join("notes/b.md").unwrap();
    assert_eq!(fx.store.read_file(&head, &stale).unwrap(), None, "nothing is written under the old name");
}

#[test]
fn content_write_rejects_unknown_object_and_record_files_and_dotdot() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(&fx, "seat create one --teamspace alpha");
    let id = seat_rec(&fx, "one").id.to_string();
    let call = |rel: &str| {
        let args = json!({ "object": id, "rel": rel, "bytes_b64": b64_encode(b"x") });
        content::admit_write(&fx.deps.plan, &CallerInfo::default(), &args).map(|_| ()).unwrap_err().message
    };
    for rel in ["../escape.md", "a/../../b.md", "/abs.md", "", "a//b.md", "./a.md", "a\\b.md", ".git/config"] {
        assert!(!call(rel).is_empty(), "{rel:?} must be refused at admission");
    }
    for rel in ["seat.toml", "notes/clone.toml", "teamspace.toml", "template.toml", "deep/er/seat.toml"] {
        assert!(call(rel).contains("record file"), "{rel}: {}", call(rel));
    }

    // The writer validates again: a request that skipped admission-time checks is rejected, not applied.
    let raw = |object: &str, rel: &str| {
        use crate::ports::writer::Writer;
        let op = fx
            .w
            .admit(ChangeRequest {
                kind: RequestKind::ContentWrite,
                args: json!({ "object": object, "rel": rel, "bytes_b64": b64_encode(b"x"), "path": rel }),
                relied_on: vec![],
                requester: Requester::default(),
                supersedes: None,
            })
            .unwrap();
        fx.w.drain().unwrap();
        fx.w.journal().get(&op).unwrap().unwrap()
    };
    let before = fx.store.head().unwrap();
    for rel in ["../escape.md", "seat.toml"] {
        let row = raw(&id, rel);
        assert_eq!((row.state, row.rejection.unwrap().reason), (OpState::Rejected, "invalid_path".to_owned()), "{rel}");
    }
    let ghost = SeatId::new();
    let row = raw(&ghost.to_string(), "notes/a.md");
    assert_eq!(row.state, OpState::Rejected);
    assert_eq!(row.rejection.unwrap().reason, "unknown_object");
    let row = raw(&crate::model::OpId::new().to_string(), "notes/a.md");
    assert_eq!(row.rejection.unwrap().reason, "unknown_object");
    assert_eq!(fx.store.head().unwrap(), before, "rejected writes commit nothing");

    // A file cannot become a directory nor the reverse.
    assert_eq!(write(&fx, &id, "notes", b"file", None).state, OpState::Committed);
    assert_eq!(raw(&id, "notes/inside.md").rejection.unwrap().reason, "invalid_path");
    assert_eq!(write(&fx, &id, "dir/x.md", b"x", None).state, OpState::Committed);
    assert_eq!(raw(&id, "dir").rejection.unwrap().reason, "invalid_path");
}

#[test]
fn content_write_retired_object_only_summaries() {
    let fx = fx();
    commit(&fx, "teamspace create alpha --active");
    commit(&fx, "seat create one --teamspace alpha --active");
    let id = seat_rec(&fx, "one").id.to_string();
    assert_eq!(write(&fx, &id, "notes/live.md", b"live", None).state, OpState::Committed);
    commit(&fx, "seat retire one");

    let row = write(&fx, &id, "notes/late.md", b"late", None);
    assert_eq!(row.state, OpState::Rejected);
    assert_eq!(row.rejection.unwrap().reason, "retired_object");
    assert_eq!(file_in(&fx, &id, "notes/late.md"), None);

    let row = write(&fx, &id, "summaries/tr_x-0-10.md", b"summary", None);
    assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    assert_eq!(file_in(&fx, &id, "summaries/tr_x-0-10.md").unwrap(), b"summary");
    let loc = layout::locate(&view(&fx), &AnyId::parse(&id).unwrap()).unwrap().unwrap();
    assert!(loc.folder.as_str().contains("archive/"), "written into the archived folder: {}", loc.folder.as_str());
    assert_eq!(file_in(&fx, &id, "notes/live.md").unwrap(), b"live", "earlier files moved to the archive");
}

#[test]
fn content_write_expect_blob_precondition() {
    let fx = fx();
    commit(&fx, "teamspace create alpha");
    commit(&fx, "seat create one --teamspace alpha");
    let id = seat_rec(&fx, "one").id.to_string();
    let blob = |fx: &Fx| {
        let head = fx.store.head().unwrap();
        let loc = fx.store.locate(&head, &AnyId::parse(&id).unwrap()).unwrap().unwrap();
        fx.store.blob_hash(&head, &loc.folder.join("notes/p.md").unwrap()).unwrap().unwrap().0
    };
    assert_eq!(write(&fx, &id, "notes/p.md", b"v1", None).state, OpState::Committed);
    let v1 = blob(&fx);
    assert_eq!(write(&fx, &id, "notes/p.md", b"v2", Some(&v1)).state, OpState::Committed);
    assert_eq!(file_in(&fx, &id, "notes/p.md").unwrap(), b"v2");

    let stale = write(&fx, &id, "notes/p.md", b"v3", Some(&v1));
    assert_eq!(stale.state, OpState::Rejected);
    assert_eq!(stale.rejection.unwrap().reason, "precondition_failed");
    assert_eq!(file_in(&fx, &id, "notes/p.md").unwrap(), b"v2", "a refused write changes nothing");
}

#[test]
fn base64_round_trips_every_tail_length_and_binary_bytes() {
    assert_eq!(b64_encode(b"f"), "Zg==");
    assert_eq!(b64_encode(b"fo"), "Zm8=");
    assert_eq!(b64_encode(b"foo"), "Zm9v");
    for n in 0..40usize {
        let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
        assert_eq!(b64_decode(&b64_encode(&bytes)).unwrap(), bytes, "length {n}");
    }
    assert!(b64_decode("a$b=").is_err());
}

// ---------------------------------------------------------------------------------------------
// setup claude
// ---------------------------------------------------------------------------------------------

const BIN: &str = "/opt/herdr graph/bin/herdr-graph";

fn owned_groups(settings: &serde_json::Value) -> Vec<serde_json::Value> {
    settings["hooks"]["SessionStart"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|g| g["hooks"][0]["command"].as_str().is_some_and(|c| c.contains(setup_claude::OWNER_MARKER)))
        .collect()
}

#[test]
fn setup_claude_installs_hook_and_skills_in_temp_config_dir() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("claude");
    let report = setup_claude::install(&config, Path::new(BIN)).unwrap();
    assert_eq!(report.hook, HookChange::Installed);
    assert_eq!(report.skills_written, ["seat", "graph"]);

    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(config.join("settings.json")).unwrap()).unwrap();
    let groups = owned_groups(&settings);
    assert_eq!(groups.len(), 1, "{settings}");
    let hook = &groups[0]["hooks"][0];
    assert_eq!(groups[0]["matcher"], "");
    assert_eq!(hook["type"], "command");
    let command = hook["command"].as_str().unwrap();
    let quoted = "'/opt/herdr graph/bin/herdr-graph'";
    assert!(command.starts_with(&format!("{quoted} seat --hook-prompt; ")), "{command}");
    assert!(command.contains(&format!("{quoted} session-report --from-hook claude")), "{command}");
    assert!(
        command.find("seat --hook-prompt").unwrap() < command.find("session-report --from-hook claude").unwrap(),
        "the /seat prompt comes first: {command}"
    );
    assert!(command.ends_with(setup_claude::OWNER_MARKER), "{command}");

    let seat = std::fs::read_to_string(config.join("skills/seat/SKILL.md")).unwrap();
    let graph = std::fs::read_to_string(config.join("skills/graph/SKILL.md")).unwrap();
    assert_eq!(seat, include_str!("../../skills/seat/SKILL.md"));
    assert_eq!(graph, include_str!("../../skills/graph/SKILL.md"));
    assert!(seat.starts_with("---\nname: seat\n"), "{seat}");
    assert!(graph.starts_with("---\nname: graph\n"), "{graph}");
    for needle in ["hg-ts:<ts>", "hg-seat:<st>", "hg-clone:<cl>", "hg-ns:<ns>", "--confirmed-by user-relay", "check-instruction", "content write"] {
        assert!(graph.contains(needle), "graph skill mentions {needle}");
    }
    assert!(seat.contains("herdr-graph seat --json") && seat.contains("bd list --label hg-seat:"));
}

#[test]
fn setup_claude_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("claude");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("settings.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo mine"}]}]},"model":"opus"}"#,
    )
    .unwrap();
    setup_claude::install(&config, Path::new(BIN)).unwrap();
    let after_first = std::fs::read(config.join("settings.json")).unwrap();
    let skill_first = std::fs::read(config.join("skills/seat/SKILL.md")).unwrap();

    let again = setup_claude::install(&config, Path::new(BIN)).unwrap();
    assert_eq!(again.hook, HookChange::Unchanged);
    assert!(again.skills_written.is_empty(), "{:?}", again.skills_written);
    assert_eq!(std::fs::read(config.join("settings.json")).unwrap(), after_first);
    assert_eq!(std::fs::read(config.join("skills/seat/SKILL.md")).unwrap(), skill_first);

    let settings: serde_json::Value = serde_json::from_slice(&after_first).unwrap();
    assert_eq!(owned_groups(&settings).len(), 1);
    assert_eq!(settings["hooks"]["SessionStart"].as_array().unwrap().len(), 2, "the user's own group stays");
    assert_eq!(settings["model"], "opus");

    // A moved binary replaces the owned group in place instead of adding a second one.
    let moved = setup_claude::install(&config, Path::new("/new/place/herdr-graph")).unwrap();
    assert_eq!(moved.hook, HookChange::Updated);
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(config.join("settings.json")).unwrap()).unwrap();
    let groups = owned_groups(&settings);
    assert_eq!(groups.len(), 1);
    assert!(groups[0]["hooks"][0]["command"].as_str().unwrap().starts_with("/new/place/herdr-graph seat --hook-prompt"));
}

#[test]
fn setup_claude_uninstall_restores_settings() {
    // Hand-formatted settings with unrelated hooks: odd indentation and key order must survive exactly.
    let original = "{\n    \"permissions\": {\"allow\": [\"Bash(ls)\"]},\n    \"hooks\": {\n        \"PreToolUse\": [{\"matcher\": \"Bash\", \"hooks\": [{\"type\": \"command\", \"command\": \"audit\"}]}],\n        \"SessionStart\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"echo mine\"}]}]\n    },\n    \"model\": \"opus\"\n}\n";
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("claude");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("settings.json"), original).unwrap();

    setup_claude::install(&config, Path::new(BIN)).unwrap();
    assert_ne!(std::fs::read_to_string(config.join("settings.json")).unwrap(), original);
    let report = setup_claude::uninstall(&config).unwrap();
    assert!(report.hook_removed);
    assert_eq!(report.skills_removed, ["seat", "graph"]);
    assert_eq!(std::fs::read_to_string(config.join("settings.json")).unwrap(), original, "byte-for-byte");
    assert!(!config.join("skills/seat").exists() && !config.join("skills/graph").exists());
    assert!(!config.join("herdr-graph-setup.json").exists(), "the manifest goes with the install");

    // Uninstalling again is a no-op.
    let again = setup_claude::uninstall(&config).unwrap();
    assert!(!again.hook_removed && again.skills_removed.is_empty());
    assert_eq!(std::fs::read_to_string(config.join("settings.json")).unwrap(), original);
}

#[test]
fn setup_claude_uninstall_after_manual_edit_removes_only_the_owned_group() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("claude");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("settings.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo mine"}]}]}}"#,
    )
    .unwrap();
    setup_claude::install(&config, Path::new(BIN)).unwrap();
    // The user edits the file after setup.
    let mut doc: serde_json::Value = serde_json::from_slice(&std::fs::read(config.join("settings.json")).unwrap()).unwrap();
    doc["model"] = json!("sonnet");
    std::fs::write(config.join("settings.json"), serde_json::to_string_pretty(&doc).unwrap()).unwrap();

    assert!(setup_claude::uninstall(&config).unwrap().hook_removed);
    let after: serde_json::Value = serde_json::from_slice(&std::fs::read(config.join("settings.json")).unwrap()).unwrap();
    assert_eq!(
        after,
        json!({"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo mine"}]}]},"model":"sonnet"}),
        "the user's edit and hook remain"
    );
}

#[test]
fn setup_claude_uninstall_removes_files_it_created() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("claude");
    setup_claude::install(&config, Path::new(BIN)).unwrap();
    assert!(config.join("settings.json").exists() && config.join("skills").is_dir());
    setup_claude::uninstall(&config).unwrap();
    assert!(!config.join("settings.json").exists(), "an installer-created settings file goes away");
    assert!(!config.join("skills").exists(), "an installer-created skills directory goes away");
    // Garbage settings are refused, never overwritten.
    std::fs::write(config.join("settings.json"), "not json").unwrap();
    assert!(setup_claude::install(&config, Path::new(BIN)).is_err());
    assert_eq!(std::fs::read_to_string(config.join("settings.json")).unwrap(), "not json");
}

#[test]
fn hook_prompt_only_when_graph_env_or_bound_pane() {
    let fx = fx();
    bound_seat(&fx, "p1");
    let tree = view(&fx);
    assert!(wants_seat_prompt(Some("1"), None, None), "graph-launched pane");
    assert!(!wants_seat_prompt(None, None, None), "not a graph pane");
    assert!(!wants_seat_prompt(Some("0"), Some(&tree), Some("p2")), "HERDR_GRAPH=0 and an unbound pane");
    assert!(wants_seat_prompt(None, Some(&tree), Some("p1")), "a pane bound to a clone");
    assert!(!wants_seat_prompt(None, Some(&tree), Some("p2")), "an unbound pane");
    assert!(!wants_seat_prompt(None, Some(&tree), None), "no pane id");
}

// ---------------------------------------------------------------------------------------------
// examples
// ---------------------------------------------------------------------------------------------

#[test]
fn init_with_examples_templates_hydrate() {
    let fx = fx_with(|root| {
        let c = examples::install_examples(root).unwrap();
        assert_eq!(c.0.len(), 40);
    });
    let tpls: Vec<String> = layout::list_templates(&view(&fx)).unwrap().into_iter().map(|(_, t)| t.name).collect();
    assert_eq!(tpls, ["feature-team", "project-team", "system-summarizer"]);

    commit(&fx, "teamspace create alpha");
    commit(&fx, "application apply system-summarizer --teamspace alpha --name sums");
    commit(&fx, "application apply project-team --teamspace alpha --name pt");

    let summarizer = seat_rec(&fx, "summarizer");
    let cfg = resolve_in(&view(&fx), &summarizer).unwrap();
    assert!(!cfg.summaries, "summaries = false for the summarizer");
    assert_eq!(cfg.role, Some(Role::Summarizer));
    assert_eq!(cfg.harness, Harness::Claude);
    assert_eq!(summarizer.lifecycle, Lifecycle::Active, "startup = active");

    let foreman = seat_rec(&fx, "foreman");
    let researcher = seat_rec(&fx, "researcher");
    assert_eq!(foreman.lifecycle, Lifecycle::Active);
    assert_eq!(researcher.lifecycle, Lifecycle::Dormant, "deferred members start dormant");
    assert!(resolve_in(&view(&fx), &foreman).unwrap().summaries, "ordinary seats keep summaries on");
    assert_eq!(clone_of(&fx, &foreman.id).seat, foreman.id);

    // The member instructions arrive in the seat folder; the summarizer's carry the protocol.
    let md = |name: &str| String::from_utf8(file_in(&fx, seat_rec(&fx, name).id.as_str(), "AGENTS.md").unwrap()).unwrap();
    let s = md("summarizer");
    for needle in [
        "herdr-graph request ack <rq>",
        "content write --object <source seat id>",
        "summaries/<tr>-<start>-<end>.md",
        "herdr-graph request complete <rq> --output",
        "/loop 1h",
        "request list --pending --undispatched",
        "request list --pending --unresolved",
        "ONE subagent per request",
    ] {
        assert!(s.contains(needle), "summarizer AGENTS.md mentions {needle}");
    }
    assert!(md("foreman").contains("foreman"));

    // The project team's relationship reaches both members; the template round-trips through show.
    let tpl = layout::list_templates(&view(&fx)).unwrap().into_iter().find(|(_, t)| t.name == "project-team").unwrap().1;
    assert_eq!(tpl.members.iter().filter(|m| m.startup == Startup::Active).count(), 1);
    assert_eq!(tpl.relationships.len(), 1);
    assert_eq!(tpl.relationships[0].thread, "team");
}

#[test]
fn shipped_templates_are_self_consistent() {
    // Every embedded member file belongs to a member of its template, ids are unique across templates.
    let fx = fx_with(|root| {
        examples::install_examples(root).unwrap();
    });
    let mut ids = std::collections::BTreeSet::new();
    for (loc, t) in layout::list_templates(&view(&fx)).unwrap() {
        assert!(ids.insert(t.id.to_string()), "duplicate id {}", t.id);
        assert_eq!(t.rev, 1);
        for m in &t.members {
            assert!(ids.insert(m.id.to_string()), "duplicate id {}", m.id);
            let md = layout::member_agents_md(&loc.folder, &crate::store::slug::slugify(&m.name));
            assert!(fx.store.read_file(&fx.store.head().unwrap(), &md).unwrap().is_some(), "{} has instructions", m.name);
        }
        assert_eq!(loc.folder.as_str(), format!("templates/{}", crate::store::slug::slugify(&t.name)));
    }
    assert_eq!(ids.len(), 3 + 5);
    // A second install would overwrite the instance's own files, so it is refused.
    let err = examples::install_examples(&fx.root).unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
}

// ---------------------------------------------------------------------------------------------
// show / list / path
// ---------------------------------------------------------------------------------------------

#[test]
fn show_list_path_read_committed_state() {
    let fx = fx_with(|root| {
        examples::install_examples(root).unwrap();
    });
    commit(&fx, "teamspace create alpha --active");
    commit(&fx, "seat create one --teamspace alpha --active");
    commit(&fx, "seat create two --teamspace alpha");
    let seat = seat_rec(&fx, "one");
    let ts = teamspace(&fx, "alpha");
    let v = view(&fx);

    // list: default teamspaces + seats, `id  name  lifecycle  path`.
    let rows = show::list(&v, &fx.root, None).unwrap();
    let rendered: Vec<String> = rows.iter().map(show::Row::render).collect();
    assert_eq!(rows.len(), 3, "{rendered:?}");
    assert_eq!(rows[0].id, ts.to_string());
    assert_eq!((rows[0].name.as_str(), rows[0].lifecycle.as_str()), ("alpha", "active"));
    assert_eq!(rows[0].path, fx.root.join("teamspaces/alpha"));
    let one = rows.iter().find(|r| r.name == "one").unwrap();
    assert_eq!(one.id, seat.id.to_string());
    assert_eq!(one.lifecycle, "active");
    assert_eq!(one.path, fx.root.join("teamspaces/alpha/seats/one"));
    assert_eq!(rows.iter().find(|r| r.name == "two").unwrap().lifecycle, "dormant");
    assert_eq!(rendered[0], format!("{ts}  alpha  active  {}", fx.root.join("teamspaces/alpha").display()));

    let clones = show::list(&v, &fx.root, Some("clones")).unwrap();
    assert_eq!(clones.len(), 1);
    assert!(clones[0].path.ends_with("teamspaces/alpha/seats/one/clones/one"), "{:?}", clones[0].path);
    let templates = show::list(&v, &fx.root, Some("templates")).unwrap();
    assert_eq!(templates.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["feature-team", "project-team", "system-summarizer"]);
    assert!(show::list(&v, &fx.root, Some("widgets")).is_err());

    // path: the object's current folder, which follows a rename; it exists in the working tree.
    let p = show::path(&v, &fx.root, seat.id.as_str()).unwrap();
    assert_eq!(p, fx.root.join("teamspaces/alpha/seats/one"));
    assert!(p.is_dir(), "the writer keeps the working tree on the committed revision");
    commit(&fx, "seat rename one uno");
    let v = view(&fx);
    assert_eq!(show::path(&v, &fx.root, seat.id.as_str()).unwrap(), fx.root.join("teamspaces/alpha/seats/uno"));
    assert_eq!(show::path(&v, &fx.root, "uno").unwrap(), fx.root.join("teamspaces/alpha/seats/uno"), "names resolve too");

    // show: record TOML by id, a rendered template, a committed file and a directory.
    let toml = show::show(&v, &fx.root, seat.id.as_str()).unwrap();
    assert!(toml.contains(&format!("id = \"{}\"", seat.id)) && toml.contains("name = \"uno\""), "{toml}");
    let tpl = templates.iter().find(|r| r.name == "project-team").unwrap();
    let rendered = show::show(&v, &fx.root, &tpl.id).unwrap();
    assert!(rendered.starts_with("template project-team (tpl_"), "{rendered}");
    assert!(rendered.contains("foreman") && rendered.contains("researcher (deferred)"), "{rendered}");
    let file = show::show(&v, &fx.root, "templates/project-team/members/foreman/AGENTS.md").unwrap();
    assert!(file.starts_with("# foreman"), "{file}");
    let dir = show::show(&v, &fx.root, "templates/project-team").unwrap();
    assert!(dir.contains("template.toml") && dir.contains("members/"), "{dir}");
    assert!(show::show(&v, &fx.root, "st_01ARZ3NDEKTSV4RRFFQ69G5FAV").is_err());
    assert!(show::path(&v, &fx.root, "no-such-thing").is_err());
}

#[test]
fn list_and_show_reflect_retirement_without_a_daemon() {
    let fx = fx();
    commit(&fx, "teamspace create alpha --active");
    commit(&fx, "seat create one --teamspace alpha --active");
    let seat = seat_rec(&fx, "one");
    commit(&fx, "seat retire one");
    let rows = show::list(&view(&fx), &fx.root, Some("seats")).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].lifecycle, "retired");
    assert!(rows[0].path.to_string_lossy().contains("archive/seats/one-"), "{:?}", rows[0].path);
    assert_eq!(show::path(&view(&fx), &fx.root, seat.id.as_str()).unwrap(), rows[0].path, "ids keep working after archiving");
}
