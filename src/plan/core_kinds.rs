//! Core organizational kinds: teamspace create/retire/resurrect, seat create/activate/deactivate/rename/
//! retire/resurrect, clone add/retire (spec §2.2, §3.3, §6). Args shapes are normative for later kinds.
use super::grammar::{self, Scope, flag, has, positional};
use super::kind::{KindRegistry, OrgKind, PlanBody, PlanCx, PlanError};
use super::types::{Plan, PlanEffect, Reserved};
use crate::daemon::registry::CallerInfo;
use crate::model::action::{ActionKind, ActionRecord, AffectedObject};
use crate::model::application::ApplicationRecord;
use crate::model::change::{ReliedOn, RequestKind, Version};
use crate::model::clone::CloneRecord;
use crate::model::common::{
    Availability, CloneLifecycle, Lifecycle, NameChange, NameSource, RetireMechanism, Retirement, Role, Runtime,
};
use crate::model::effective::{EffectiveSeatConfig, resolve_in};
use crate::model::harness::Harness;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{
    ActionId, AnyId, CloneId, MemberId, SCHEMA_VERSION, SeatId, TeamspaceId,
};
use crate::ports::store::{ObjectLocation, RepoPath};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::slug::unique_slug;
use crate::store::tree::TreeRead;
use crate::writer::{Applied, MutationCx, MutationError, Reject};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

/// Registers every core kind.
pub fn register_core_kinds(reg: &mut KindRegistry) {
    reg.register(Arc::new(TeamspaceCreate));
    reg.register(Arc::new(TeamspaceRetire));
    reg.register(Arc::new(TeamspaceResurrect));
    reg.register(Arc::new(SeatCreate));
    reg.register(Arc::new(SeatActivate));
    reg.register(Arc::new(SeatDeactivate));
    reg.register(Arc::new(SeatRename));
    reg.register(Arc::new(SeatRetire));
    reg.register(Arc::new(SeatResurrect));
    reg.register(Arc::new(CloneAdd));
    reg.register(Arc::new(CloneRetire));
}

// ---------------------------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------------------------

fn parse_args<T: DeserializeOwned>(v: &serde_json::Value) -> Result<T, PlanError> {
    serde_json::from_value(v.clone()).map_err(|e| PlanError::Invalid(format!("malformed arguments: {e}")))
}

/// A state mismatch seen while applying: the op is rejected, not retried.
fn mm(e: PlanError) -> MutationError {
    match e {
        PlanError::Store(s) => MutationError::Store(s),
        other => MutationError::Reject(Reject {
            reason: "invalid_request".into(),
            explanation: other.to_string(),
            current_revs: vec![],
        }),
    }
}

fn rev_of(id: impl Into<AnyId>, rev: u64) -> ReliedOn {
    ReliedOn { object: id.into(), version: Version::Rev(rev) }
}

fn lc_value(name: &str) -> toml::Value {
    let mut t = toml::Table::new();
    t.insert("lifecycle".into(), toml::Value::String(name.into()));
    toml::Value::Table(t)
}

fn absent() -> Runtime {
    Runtime { availability: Availability::Absent, bound: None, observed_at: None }
}

fn basename(p: &RepoPath) -> &str {
    p.as_str().rsplit('/').next().unwrap_or_default()
}

fn mechanism(cx: &MutationCx<'_>) -> RetireMechanism {
    if cx.request.requester.human { RetireMechanism::UserCli } else { RetireMechanism::AgentRequest }
}

struct TsInfo {
    loc: ObjectLocation,
    rec: TeamspaceRecord,
}

fn read_ts(tree: &dyn TreeRead, id: &TeamspaceId) -> Result<TsInfo, PlanError> {
    let loc = layout::locate(tree, &id.to_any())?.ok_or_else(|| PlanError::Invalid(format!("no teamspace {id}")))?;
    let rec = read_toml::<TeamspaceRecord>(tree, &loc.record_path)?
        .ok_or_else(|| PlanError::Invalid(format!("no teamspace {id}")))?;
    Ok(TsInfo { loc, rec })
}

struct SeatFacts {
    loc: ObjectLocation,
    rec: SeatRecord,
    ts: TsInfo,
    clones: Vec<(ObjectLocation, CloneRecord)>,
}

impl SeatFacts {
    fn active_clones(&self) -> impl Iterator<Item = &(ObjectLocation, CloneRecord)> {
        self.clones.iter().filter(|(_, c)| c.lifecycle == CloneLifecycle::Active)
    }
}

fn read_seat(tree: &dyn TreeRead, id: &SeatId) -> Result<SeatFacts, PlanError> {
    let loc = layout::locate(tree, &id.to_any())?.ok_or_else(|| PlanError::Invalid(format!("no seat {id}")))?;
    let rec = read_toml::<SeatRecord>(tree, &loc.record_path)?
        .ok_or_else(|| PlanError::Invalid(format!("no seat {id}")))?;
    let ts = read_ts(tree, &rec.teamspace)?;
    let clones = layout::list_clones(tree, &loc.folder)?;
    Ok(SeatFacts { loc, rec, ts, clones })
}

fn live_seat(tree: &dyn TreeRead, seat_ref: &str) -> Result<SeatFacts, PlanError> {
    let id = grammar::resolve_seat(tree, seat_ref, Scope::Live)?;
    let f = read_seat(tree, &id)?;
    if f.rec.lifecycle == Lifecycle::Retired {
        return Err(PlanError::Invalid(format!("seat {} is retired; use `seat resurrect`", f.rec.id)));
    }
    Ok(f)
}

fn seat_slug_for(tree: &dyn TreeRead, ts_dir: &RepoPath, name: &str, id: &SeatId, own: Option<&str>) -> Result<String, PlanError> {
    let parent = ts_dir.join("seats")?;
    let mut taken = layout::taken_slugs(tree, &parent)?;
    if let Some(own) = own {
        taken.remove(own);
    }
    Ok(unique_slug(name, id.suffix6(), &taken))
}

fn clone_slug_for(tree: &dyn TreeRead, seat_dir: &RepoPath, name: &str, id: &CloneId) -> Result<String, PlanError> {
    let taken = layout::taken_slugs(tree, &seat_dir.join("clones")?)?;
    Ok(unique_slug(name, id.suffix6(), &taken))
}

fn ensure_ts_live(ts: &TsInfo) -> Result<(), PlanError> {
    if ts.rec.lifecycle == Lifecycle::Retired {
        return Err(PlanError::Invalid(format!(
            "teamspace {} is retired; resurrect it first",
            ts.rec.id
        )));
    }
    Ok(())
}

fn harness_is_shell(c: &EffectiveSeatConfig) -> bool {
    c.harness == Harness::Shell
}

fn run_detail(c: &EffectiveSeatConfig) -> serde_json::Value {
    json!({ "harness": c.harness, "model": c.model, "args": c.args })
}

/// open_pane (+ start_agent unless shell) for one clone of an active seat.
fn open_clone_effects(clone: &CloneId, seat: &SeatId, cfg: &EffectiveSeatConfig) -> Vec<PlanEffect> {
    let mut v = vec![PlanEffect::new("runtime.open_pane", clone.clone(), json!({ "seat": seat }))];
    if !harness_is_shell(cfg) {
        v.push(PlanEffect::new("runtime.start_agent", clone.clone(), run_detail(cfg)));
    }
    v
}

fn new_clone_record(seat: &SeatId, name: &str, id: CloneId) -> CloneRecord {
    CloneRecord {
        schema: SCHEMA_VERSION,
        id,
        rev: 0,
        seat: seat.clone(),
        name: name.to_owned(),
        name_history: vec![],
        lifecycle: CloneLifecycle::Active,
        retired: None,
        runtime: absent(),
        occupant: None,
        sessions: vec![],
        opt_outs: vec![],
        invitations: vec![],
        reload_required: false,
    }
}

fn new_seat_record(id: SeatId, name: &str, ts: &TeamspaceId, active: bool, a: &SeatCreateArgs) -> SeatRecord {
    let mut rec = SeatRecord {
        schema: SCHEMA_VERSION,
        id,
        rev: 0,
        name: name.to_owned(),
        name_history: vec![],
        teamspace: ts.clone(),
        lifecycle: if active { Lifecycle::Active } else { Lifecycle::Dormant },
        retired: None,
        role: a.role,
        template_ref: None,
        applications: vec![],
        overrides: Default::default(),
        participation: Default::default(),
        activation: Default::default(),
        runtime: absent(),
        channel: Default::default(),
        reload_required: false,
        moved_out: false,
    };
    rec.overrides.harness = a.harness;
    rec.overrides.model = a.model.clone();
    rec
}

/// Application exclusions a seat retirement adds (decision 8): live applications that map this seat's member.
fn app_exclusions(
    tree: &dyn TreeRead,
    seat: &SeatRecord,
) -> Result<Vec<(ObjectLocation, ApplicationRecord, MemberId)>, PlanError> {
    let Some(r) = &seat.template_ref else { return Ok(vec![]) };
    let mut out = Vec::new();
    for app in &seat.applications {
        let Some(loc) = layout::locate(tree, &app.to_any())? else { continue };
        let Some(rec) = read_toml::<ApplicationRecord>(tree, &loc.record_path)? else { continue };
        if rec.lifecycle == crate::model::common::AppLifecycle::Active && !rec.exclusions.contains(&r.member) {
            out.push((loc, rec, r.member.clone()));
        }
    }
    Ok(out)
}

/// Accumulator for one retire/resurrect action record.
#[derive(Default)]
struct Acc {
    retired: Vec<AnyId>,
    already: Vec<AnyId>,
    affected: Vec<AffectedObject>,
}

impl Acc {
    fn changed(&mut self, id: AnyId, before: &str, after: &str) {
        self.affected.push(AffectedObject { object: id, before: Some(lc_value(before)), after: Some(lc_value(after)) });
    }
}

fn write_action(
    cx: &mut MutationCx<'_>,
    act: &ActionId,
    kind: ActionKind,
    acc: Acc,
    compensation: toml::Table,
) -> Result<(), MutationError> {
    let mut rec = ActionRecord {
        schema: SCHEMA_VERSION,
        id: act.clone(),
        rev: 0,
        kind,
        at: cx.now,
        ops: vec![cx.op.clone()],
        affected: acc.affected,
        retired: acc.retired,
        already_retired: acc.already,
        compensation,
        undoes: None,
        undone_by: vec![],
    };
    cx.tree.put_record(layout::action_record(cx.now, act), &mut rec)?;
    Ok(())
}

/// Ids retired by `action` (its record's `retired` list); when the record is missing, an empty set.
fn retired_by(tree: &dyn TreeRead, action: Option<&ActionId>) -> Result<Option<BTreeSet<AnyId>>, PlanError> {
    let Some(action) = action else { return Ok(None) };
    let Some(loc) = layout::locate(tree, &action.to_any())? else { return Ok(None) };
    Ok(read_toml::<ActionRecord>(tree, &loc.record_path)?.map(|a| a.retired.into_iter().collect()))
}

fn retired_with(
    set: &Option<BTreeSet<AnyId>>,
    action: Option<&ActionId>,
    id: &AnyId,
    own: Option<&Retirement>,
) -> bool {
    match set {
        Some(s) => s.contains(id),
        None => action.is_some() && own.and_then(|r| r.action.as_ref()) == action,
    }
}

/// Retire `seat` and its active clones in the overlay; archive its folder. Appends to `acc`.
#[allow(clippy::too_many_arguments)]
fn do_retire_seat(
    cx: &mut MutationCx<'_>,
    seat: &SeatId,
    act: &ActionId,
    mech: RetireMechanism,
    acc: &mut Acc,
    exclusions: bool,
) -> Result<(), MutationError> {
    let f = read_seat(&cx.tree, seat).map_err(mm)?;
    if f.rec.lifecycle == Lifecycle::Retired {
        acc.already.push(f.rec.id.to_any());
        return Ok(());
    }
    let retirement = |cx: &MutationCx<'_>| Retirement { op: cx.op.clone(), action: Some(act.clone()), at: cx.now, mechanism: mech };
    for (loc, c) in &f.clones {
        if c.lifecycle == CloneLifecycle::Retired {
            acc.already.push(c.id.to_any());
            continue;
        }
        let mut c = c.clone();
        c.lifecycle = CloneLifecycle::Retired;
        c.retired = Some(retirement(cx));
        cx.tree.put_record(loc.record_path.clone(), &mut c)?;
        acc.retired.push(c.id.to_any());
        acc.changed(c.id.to_any(), "active", "retired");
    }
    let before = match f.rec.lifecycle {
        Lifecycle::Active => "active",
        _ => "dormant",
    };
    let mut rec = f.rec.clone();
    rec.lifecycle = Lifecycle::Retired;
    rec.retired = Some(retirement(cx));
    rec.activation.last_op = Some(cx.op.clone());
    cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
    acc.retired.push(rec.id.to_any());
    acc.changed(rec.id.to_any(), before, "retired");
    let archive = layout::archived_seat_dir(&f.ts.loc.folder, basename(&f.loc.folder), &rec.id);
    cx.tree.move_dir(&f.loc.folder, &archive)?;
    if exclusions {
        for (loc, mut app, member) in app_exclusions(&cx.tree, &f.rec).map_err(mm)? {
            app.exclusions.push(member);
            cx.tree.put_record(loc.record_path, &mut app)?;
        }
    }
    Ok(())
}

/// Resurrect a retired seat (and the clones in `with_clones`), moving its folder back to a live path.
fn do_resurrect_seat(
    cx: &mut MutationCx<'_>,
    seat: &SeatId,
    active: bool,
    set: &Option<BTreeSet<AnyId>>,
    acc: &mut Acc,
) -> Result<(), MutationError> {
    let f = read_seat(&cx.tree, seat).map_err(mm)?;
    let action = f.rec.retired.as_ref().and_then(|r| r.action.clone());
    let mut clone_ids = Vec::new();
    for (loc, c) in &f.clones {
        if c.lifecycle != CloneLifecycle::Retired
            || !retired_with(set, action.as_ref(), &c.id.to_any(), c.retired.as_ref())
        {
            continue;
        }
        let mut c = c.clone();
        c.lifecycle = CloneLifecycle::Active;
        c.retired = None;
        c.occupant = None;
        c.runtime = absent();
        cx.tree.put_record(loc.record_path.clone(), &mut c)?;
        acc.changed(c.id.to_any(), "retired", "active");
        clone_ids.push(c.id.clone());
    }
    let mut rec = f.rec.clone();
    rec.lifecycle = if active { Lifecycle::Active } else { Lifecycle::Dormant };
    rec.retired = None;
    rec.runtime = absent();
    if active {
        rec.activation.last_op = Some(cx.op.clone());
    }
    cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
    acc.changed(rec.id.to_any(), "retired", if active { "active" } else { "dormant" });
    let slug = seat_slug_for(&cx.tree, &f.ts.loc.folder, &rec.name, &rec.id, None).map_err(mm)?;
    let live = layout::seat_dir(&f.ts.loc.folder, &slug);
    cx.tree.move_dir(&f.loc.folder, &live)?;
    Ok(())
}

fn push_toml_str(t: &mut toml::Table, k: &str, v: &str) {
    t.insert(k.into(), toml::Value::String(v.into()));
}

// ---------------------------------------------------------------------------------------------
// teamspace create | retire | resurrect
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct TeamspaceCreateArgs {
    name: String,
    #[serde(default)]
    project_repo: Option<String>,
    #[serde(default)]
    active: bool,
}

#[derive(Deserialize)]
struct TeamspaceRef {
    teamspace: String,
}

struct TeamspaceCreate;
impl OrgKind for TeamspaceCreate {
    fn kind(&self) -> RequestKind {
        RequestKind::TeamspaceCreate
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("teamspace", "create")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let name = positional(words, 0)
            .ok_or_else(|| PlanError::Usage("teamspace create <name> [--project-repo <path>] [--active]".into()))?;
        Ok(json!({ "name": name, "project_repo": flag(words, "--project-repo"), "active": has(words, "--active") }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: TeamspaceCreateArgs = parse_args(args)?;
        let id: TeamspaceId = reserved.get_or_mint("teamspace");
        let taken = layout::taken_slugs(cx.tree, &layout::teamspaces_root())?;
        let slug = unique_slug(&a.name, id.suffix6(), &taken);
        let path = layout::teamspace_dir(&slug);
        let lifecycle = if a.active { "active" } else { "dormant" };
        let mut effects = vec![PlanEffect::new(
            "teamspace.create",
            id.clone(),
            json!({ "name": a.name, "path": path.as_str(), "lifecycle": lifecycle, "project_repo": a.project_repo }),
        )];
        if a.active {
            effects.push(PlanEffect::new("runtime.open_workspace", id, json!({ "name": a.name })));
        }
        Ok(PlanBody {
            effects,
            relied_on: vec![],
            warnings: vec![],
            repair_required: None,
            summary: format!("create teamspace {}", a.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: TeamspaceCreateArgs = parse_args(args).map_err(mm)?;
        let id: TeamspaceId = plan
            .reserved
            .get("teamspace")
            .ok_or_else(|| MutationError::Bug("plan reserved no teamspace id".into()))?;
        let taken = layout::taken_slugs(&cx.tree, &layout::teamspaces_root())?;
        let dir = layout::teamspace_dir(&unique_slug(&a.name, id.suffix6(), &taken));
        let mut rec = TeamspaceRecord {
            schema: SCHEMA_VERSION,
            id,
            rev: 0,
            name: a.name.clone(),
            name_history: vec![],
            lifecycle: if a.active { Lifecycle::Active } else { Lifecycle::Dormant },
            retired: None,
            runtime: absent(),
            project_repo: a.project_repo.map(PathBuf::from),
            channel: Default::default(),
        };
        cx.tree.put_record(layout::teamspace_record(&dir), &mut rec)?;
        Ok(Applied { summary: format!("create teamspace {}", a.name), action: None })
    }
}

struct TeamspaceRetire;
impl OrgKind for TeamspaceRetire {
    fn kind(&self) -> RequestKind {
        RequestKind::TeamspaceRetire
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("teamspace", "retire")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let ts = positional(words, 0).ok_or_else(|| PlanError::Usage("teamspace retire <teamspace>".into()))?;
        Ok(json!({ "teamspace": ts }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: TeamspaceRef = parse_args(args)?;
        let id = grammar::resolve_teamspace(cx.tree, &a.teamspace, Scope::Live)?;
        let ts = read_ts(cx.tree, &id)?;
        if ts.rec.lifecycle == Lifecycle::Retired {
            return Err(PlanError::Invalid(format!("teamspace {id} is already retired")));
        }
        let _act: ActionId = reserved.get_or_mint("act");
        let archive = layout::archived_teamspace_dir(basename(&ts.loc.folder), &id);
        let mut effects = vec![PlanEffect::new(
            "teamspace.retire",
            id.clone(),
            json!({ "name": ts.rec.name, "path_from": ts.loc.folder.as_str(), "path_to": archive.as_str() }),
        )];
        let mut relied_on = vec![rev_of(id.clone(), ts.rec.rev)];
        for (loc, seat) in layout::list_seats(cx.tree, &ts.loc.folder)? {
            if seat.lifecycle == Lifecycle::Retired {
                continue;
            }
            effects.push(PlanEffect::new("seat.retire", seat.id.clone(), json!({ "name": seat.name })));
            relied_on.push(rev_of(seat.id.clone(), seat.rev));
            for (_, c) in layout::list_clones(cx.tree, &loc.folder)? {
                if c.lifecycle == CloneLifecycle::Active {
                    effects.push(PlanEffect::new("clone.retire", c.id.clone(), json!({ "seat": seat.id, "name": c.name })));
                }
            }
        }
        if ts.rec.lifecycle == Lifecycle::Active {
            effects.push(PlanEffect::new("runtime.close_workspace", id, json!({ "name": ts.rec.name })));
        }
        Ok(PlanBody {
            effects,
            relied_on,
            warnings: vec![],
            repair_required: None,
            summary: format!("retire teamspace {}", ts.rec.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: TeamspaceRef = parse_args(args).map_err(mm)?;
        let id = grammar::resolve_teamspace(&cx.tree, &a.teamspace, Scope::Live).map_err(mm)?;
        let act: ActionId = plan.reserved.get("act").ok_or_else(|| MutationError::Bug("plan reserved no act id".into()))?;
        let ts = read_ts(&cx.tree, &id).map_err(mm)?;
        let mech = mechanism(cx);
        let mut acc = Acc::default();
        for (_, seat) in layout::list_seats(&cx.tree, &ts.loc.folder)? {
            do_retire_seat(cx, &seat.id, &act, mech, &mut acc, false)?;
        }
        let mut rec = ts.rec.clone();
        let before = if rec.lifecycle == Lifecycle::Active { "active" } else { "dormant" };
        rec.lifecycle = Lifecycle::Retired;
        rec.retired = Some(Retirement { op: cx.op.clone(), action: Some(act.clone()), at: cx.now, mechanism: mech });
        cx.tree.put_record(ts.loc.record_path.clone(), &mut rec)?;
        acc.retired.insert(0, id.to_any());
        acc.changed(id.to_any(), before, "retired");
        cx.tree.move_dir(&ts.loc.folder, &layout::archived_teamspace_dir(basename(&ts.loc.folder), &id))?;
        let summary = format!("retire teamspace {}", ts.rec.name);
        write_action(cx, &act, ActionKind::Retire, acc, toml::Table::new())?;
        Ok(Applied { summary, action: Some(act) })
    }
}

struct TeamspaceResurrect;
impl OrgKind for TeamspaceResurrect {
    fn kind(&self) -> RequestKind {
        RequestKind::TeamspaceResurrect
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("teamspace", "resurrect")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let ts = positional(words, 0).ok_or_else(|| PlanError::Usage("teamspace resurrect <teamspace>".into()))?;
        Ok(json!({ "teamspace": ts }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: TeamspaceRef = parse_args(args)?;
        let id = grammar::resolve_teamspace(cx.tree, &a.teamspace, Scope::Retired)?;
        let ts = read_ts(cx.tree, &id)?;
        if ts.rec.lifecycle != Lifecycle::Retired {
            return Err(PlanError::Invalid(format!("teamspace {id} is not retired")));
        }
        let _act: ActionId = reserved.get_or_mint("act");
        let action = ts.rec.retired.as_ref().and_then(|r| r.action.clone());
        let set = retired_by(cx.tree, action.as_ref())?;
        let taken = layout::taken_slugs(cx.tree, &layout::teamspaces_root())?;
        let live = layout::teamspace_dir(&unique_slug(&ts.rec.name, id.suffix6(), &taken));
        let mut effects = vec![PlanEffect::new(
            "teamspace.resurrect",
            id.clone(),
            json!({ "name": ts.rec.name, "to": "dormant", "path_from": ts.loc.folder.as_str(), "path_to": live.as_str() }),
        )];
        for (loc, seat) in layout::list_seats(cx.tree, &ts.loc.folder)? {
            if seat.lifecycle != Lifecycle::Retired
                || !retired_with(&set, action.as_ref(), &seat.id.to_any(), seat.retired.as_ref())
            {
                continue;
            }
            effects.push(PlanEffect::new("seat.resurrect", seat.id.clone(), json!({ "name": seat.name, "to": "dormant" })));
            for (_, c) in layout::list_clones(cx.tree, &loc.folder)? {
                if c.lifecycle == CloneLifecycle::Retired
                    && retired_with(&set, action.as_ref(), &c.id.to_any(), c.retired.as_ref())
                {
                    effects.push(PlanEffect::new("clone.resurrect", c.id.clone(), json!({ "seat": seat.id, "name": c.name })));
                }
            }
        }
        Ok(PlanBody {
            effects,
            relied_on: vec![rev_of(id, ts.rec.rev)],
            warnings: vec![],
            repair_required: None,
            summary: format!("resurrect teamspace {}", ts.rec.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: TeamspaceRef = parse_args(args).map_err(mm)?;
        let id = grammar::resolve_teamspace(&cx.tree, &a.teamspace, Scope::Retired).map_err(mm)?;
        let act: ActionId = plan.reserved.get("act").ok_or_else(|| MutationError::Bug("plan reserved no act id".into()))?;
        let ts = read_ts(&cx.tree, &id).map_err(mm)?;
        let action = ts.rec.retired.as_ref().and_then(|r| r.action.clone());
        let set = retired_by(&cx.tree, action.as_ref()).map_err(mm)?;
        let seat_ids: Vec<SeatId> = layout::list_seats(&cx.tree, &ts.loc.folder)?
            .into_iter()
            .filter(|(_, s)| {
                s.lifecycle == Lifecycle::Retired && retired_with(&set, action.as_ref(), &s.id.to_any(), s.retired.as_ref())
            })
            .map(|(_, s)| s.id)
            .collect();
        let mut acc = Acc::default();
        let mut rec = ts.rec.clone();
        rec.lifecycle = Lifecycle::Dormant;
        rec.retired = None;
        rec.runtime = absent();
        cx.tree.put_record(ts.loc.record_path.clone(), &mut rec)?;
        acc.changed(id.to_any(), "retired", "dormant");
        let taken = layout::taken_slugs(&cx.tree, &layout::teamspaces_root())?;
        let live = layout::teamspace_dir(&unique_slug(&rec.name, id.suffix6(), &taken));
        cx.tree.move_dir(&ts.loc.folder, &live)?;
        for seat in seat_ids {
            do_resurrect_seat(cx, &seat, false, &set, &mut acc)?;
        }
        let mut comp = toml::Table::new();
        if let Some(a) = &action {
            push_toml_str(&mut comp, "resurrects", a.as_str());
        }
        write_action(cx, &act, ActionKind::Resurrect, acc, comp)?;
        Ok(Applied { summary: format!("resurrect teamspace {}", rec.name), action: Some(act) })
    }
}

// ---------------------------------------------------------------------------------------------
// seat kinds
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct SeatCreateArgs {
    name: String,
    teamspace: String,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    harness: Option<Harness>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    role: Option<Role>,
}

#[derive(Deserialize)]
struct SeatRef {
    seat: String,
}

#[derive(Deserialize)]
struct SeatRenameArgs {
    seat: String,
    name: String,
}

#[derive(Deserialize)]
struct SeatResurrectArgs {
    seat: String,
    #[serde(default)]
    active: bool,
}

fn parse_enum<T: DeserializeOwned>(what: &str, s: &str) -> Result<T, PlanError> {
    serde_json::from_value(json!(s)).map_err(|_| PlanError::Usage(format!("unknown {what} {s:?}")))
}

struct SeatCreate;
impl OrgKind for SeatCreate {
    fn kind(&self) -> RequestKind {
        RequestKind::SeatCreate
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("seat", "create")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = || {
            PlanError::Usage(
                "seat create <name> --teamspace <ts> [--active] [--harness h] [--model m] [--role r]".into(),
            )
        };
        let name = positional(words, 0).ok_or_else(usage)?;
        let ts = flag(words, "--teamspace").ok_or_else(usage)?;
        let harness: Option<Harness> = flag(words, "--harness").map(|h| parse_enum("harness", &h)).transpose()?;
        let role: Option<Role> = flag(words, "--role").map(|r| parse_enum("role", &r)).transpose()?;
        Ok(json!({
            "name": name, "teamspace": ts, "active": has(words, "--active"),
            "harness": harness, "model": flag(words, "--model"), "role": role,
        }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: SeatCreateArgs = parse_args(args)?;
        let ts_id = grammar::resolve_teamspace(cx.tree, &a.teamspace, Scope::Live)?;
        let ts = read_ts(cx.tree, &ts_id)?;
        ensure_ts_live(&ts)?;
        let seat_id: SeatId = reserved.get_or_mint("seat");
        let clone_id: CloneId = reserved.get_or_mint("clone:0");
        let slug = seat_slug_for(cx.tree, &ts.loc.folder, &a.name, &seat_id, None)?;
        let seat_dir = layout::seat_dir(&ts.loc.folder, &slug);
        let rec = new_seat_record(seat_id.clone(), &a.name, &ts_id, a.active, &a);
        let cfg = resolve_in(cx.tree, &rec)?;
        let mut effects = vec![PlanEffect::new(
            "seat.create",
            seat_id.clone(),
            json!({
                "name": a.name, "teamspace": ts_id, "path": seat_dir.as_str(),
                "lifecycle": if a.active { "active" } else { "dormant" },
                "harness": a.harness, "model": a.model, "role": a.role,
            }),
        )];
        if a.active {
            effects.push(PlanEffect::new(
                "clone.add",
                clone_id.clone(),
                json!({ "seat": seat_id, "name": a.name, "path": layout::clone_dir(&seat_dir, &clone_slug_for(cx.tree, &seat_dir, &a.name, &clone_id)?).as_str() }),
            ));
            if ts.rec.lifecycle == Lifecycle::Dormant {
                effects.push(PlanEffect::new("teamspace.activate", ts_id.clone(), json!({ "name": ts.rec.name })).induced());
            }
            effects.push(PlanEffect::new("runtime.open_tab", seat_id.clone(), json!({ "name": a.name })));
            effects.extend(open_clone_effects(&clone_id, &seat_id, &cfg));
        }
        Ok(PlanBody {
            effects,
            relied_on: vec![rev_of(ts_id, ts.rec.rev)],
            warnings: vec![],
            repair_required: None,
            summary: format!("create seat {}", a.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: SeatCreateArgs = parse_args(args).map_err(mm)?;
        let ts_id = grammar::resolve_teamspace(&cx.tree, &a.teamspace, Scope::Live).map_err(mm)?;
        let ts = read_ts(&cx.tree, &ts_id).map_err(mm)?;
        ensure_ts_live(&ts).map_err(mm)?;
        let seat_id: SeatId = plan.reserved.get("seat").ok_or_else(|| MutationError::Bug("plan reserved no seat id".into()))?;
        let slug = seat_slug_for(&cx.tree, &ts.loc.folder, &a.name, &seat_id, None).map_err(mm)?;
        let seat_dir = layout::seat_dir(&ts.loc.folder, &slug);
        let mut rec = new_seat_record(seat_id.clone(), &a.name, &ts_id, a.active, &a);
        if a.active {
            rec.activation.last_op = Some(cx.op.clone());
            let clone_id: CloneId =
                plan.reserved.get("clone:0").ok_or_else(|| MutationError::Bug("plan reserved no clone id".into()))?;
            let cslug = clone_slug_for(&cx.tree, &seat_dir, &a.name, &clone_id).map_err(mm)?;
            let cdir = layout::clone_dir(&seat_dir, &cslug);
            let mut clone = new_clone_record(&seat_id, &a.name, clone_id);
            cx.tree.put_record(layout::clone_record(&cdir), &mut clone)?;
            if ts.rec.lifecycle == Lifecycle::Dormant {
                let mut t = ts.rec.clone();
                t.lifecycle = Lifecycle::Active;
                cx.tree.put_record(ts.loc.record_path.clone(), &mut t)?;
            }
        }
        cx.tree.put_record(layout::seat_record(&seat_dir), &mut rec)?;
        Ok(Applied { summary: format!("create seat {}", a.name), action: None })
    }
}

/// open_tab + clone panes for activating `f` (clones that will be active), with induced teamspace activation.
fn activation_effects(
    f: &SeatFacts,
    cfg: &EffectiveSeatConfig,
    new_clone: Option<&CloneId>,
    clone_name_path: Option<(&str, String)>,
) -> Vec<PlanEffect> {
    let mut effects = Vec::new();
    if let (Some(id), Some((name, path))) = (new_clone, clone_name_path) {
        effects.push(PlanEffect::new("clone.add", id.clone(), json!({ "seat": f.rec.id, "name": name, "path": path })));
    }
    if f.ts.rec.lifecycle == Lifecycle::Dormant {
        effects.push(PlanEffect::new("teamspace.activate", f.ts.rec.id.clone(), json!({ "name": f.ts.rec.name })).induced());
    }
    effects.push(PlanEffect::new("runtime.open_tab", f.rec.id.clone(), json!({ "name": f.rec.name })));
    for (_, c) in f.active_clones() {
        effects.extend(open_clone_effects(&c.id, &f.rec.id, cfg));
    }
    if let Some(id) = new_clone {
        effects.extend(open_clone_effects(id, &f.rec.id, cfg));
    }
    effects
}

struct SeatActivate;
impl OrgKind for SeatActivate {
    fn kind(&self) -> RequestKind {
        RequestKind::SeatActivate
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("seat", "activate")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let s = positional(words, 0).ok_or_else(|| PlanError::Usage("seat activate <seat>".into()))?;
        Ok(json!({ "seat": s }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: SeatRef = parse_args(args)?;
        let f = live_seat(cx.tree, &a.seat)?;
        if f.rec.lifecycle == Lifecycle::Active {
            return Err(PlanError::Invalid(format!("seat {} is already active", f.rec.id)));
        }
        ensure_ts_live(&f.ts)?;
        let cfg = resolve_in(cx.tree, &f.rec)?;
        let (new_clone, path) = if f.active_clones().next().is_none() {
            let id: CloneId = reserved.get_or_mint("clone:0");
            let slug = clone_slug_for(cx.tree, &f.loc.folder, &f.rec.name, &id)?;
            (Some(id), Some(layout::clone_dir(&f.loc.folder, &slug).as_str().to_owned()))
        } else {
            (None, None)
        };
        let mut effects = vec![PlanEffect::new("seat.activate", f.rec.id.clone(), json!({ "name": f.rec.name }))];
        effects.extend(activation_effects(&f, &cfg, new_clone.as_ref(), path.map(|p| (f.rec.name.as_str(), p))));
        Ok(PlanBody {
            effects,
            relied_on: vec![rev_of(f.rec.id.clone(), f.rec.rev)],
            warnings: vec![],
            repair_required: None,
            summary: format!("activate seat {}", f.rec.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: SeatRef = parse_args(args).map_err(mm)?;
        let f = live_seat(&cx.tree, &a.seat).map_err(mm)?;
        ensure_ts_live(&f.ts).map_err(mm)?;
        if f.active_clones().next().is_none() {
            let id: CloneId = plan.reserved.get("clone:0").ok_or_else(|| MutationError::Bug("plan reserved no clone id".into()))?;
            let slug = clone_slug_for(&cx.tree, &f.loc.folder, &f.rec.name, &id).map_err(mm)?;
            let mut clone = new_clone_record(&f.rec.id, &f.rec.name, id);
            cx.tree.put_record(layout::clone_record(&layout::clone_dir(&f.loc.folder, &slug)), &mut clone)?;
        }
        if f.ts.rec.lifecycle == Lifecycle::Dormant {
            let mut t = f.ts.rec.clone();
            t.lifecycle = Lifecycle::Active;
            cx.tree.put_record(f.ts.loc.record_path.clone(), &mut t)?;
        }
        let mut rec = f.rec.clone();
        rec.lifecycle = Lifecycle::Active;
        rec.activation.last_op = Some(cx.op.clone());
        cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
        Ok(Applied { summary: format!("activate seat {}", rec.name), action: None })
    }
}

struct SeatDeactivate;
impl OrgKind for SeatDeactivate {
    fn kind(&self) -> RequestKind {
        RequestKind::SeatDeactivate
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("seat", "deactivate")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let s = positional(words, 0).ok_or_else(|| PlanError::Usage("seat deactivate <seat>".into()))?;
        Ok(json!({ "seat": s }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, _: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: SeatRef = parse_args(args)?;
        let f = live_seat(cx.tree, &a.seat)?;
        if f.rec.lifecycle != Lifecycle::Active {
            return Err(PlanError::Invalid(format!("seat {} is not active", f.rec.id)));
        }
        Ok(PlanBody {
            effects: vec![
                PlanEffect::new("seat.deactivate", f.rec.id.clone(), json!({ "name": f.rec.name })),
                PlanEffect::new("runtime.close_tab", f.rec.id.clone(), json!({ "name": f.rec.name })),
            ],
            relied_on: vec![rev_of(f.rec.id.clone(), f.rec.rev)],
            warnings: vec![],
            repair_required: None,
            summary: format!("deactivate seat {}", f.rec.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, _: &Plan) -> Result<Applied, MutationError> {
        let a: SeatRef = parse_args(args).map_err(mm)?;
        let f = live_seat(&cx.tree, &a.seat).map_err(mm)?;
        if f.rec.lifecycle != Lifecycle::Active {
            return Err(mm(PlanError::Invalid(format!("seat {} is not active", f.rec.id))));
        }
        let mut rec = f.rec.clone();
        rec.lifecycle = Lifecycle::Dormant;
        rec.activation.last_op = Some(cx.op.clone());
        cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
        Ok(Applied { summary: format!("deactivate seat {}", rec.name), action: None })
    }
}

struct SeatRename;
impl SeatRename {
    fn paths(tree: &dyn TreeRead, f: &SeatFacts, new_name: &str) -> Result<(RepoPath, RepoPath), PlanError> {
        let own = basename(&f.loc.folder).to_owned();
        let slug = seat_slug_for(tree, &f.ts.loc.folder, new_name, &f.rec.id, Some(&own))?;
        Ok((f.loc.folder.clone(), layout::seat_dir(&f.ts.loc.folder, &slug)))
    }
}
impl OrgKind for SeatRename {
    fn kind(&self) -> RequestKind {
        RequestKind::SeatRename
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("seat", "rename")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = || PlanError::Usage("seat rename <seat> <new-name>".into());
        let seat = positional(words, 0).ok_or_else(usage)?;
        let name = positional(words, 1).ok_or_else(usage)?;
        Ok(json!({ "seat": seat, "name": name }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, _: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: SeatRenameArgs = parse_args(args)?;
        let f = live_seat(cx.tree, &a.seat)?;
        if f.rec.name == a.name {
            return Err(PlanError::Invalid(format!("seat {} is already named {:?}", f.rec.id, a.name)));
        }
        let (from, to) = Self::paths(cx.tree, &f, &a.name)?;
        let mut effects = vec![PlanEffect::new(
            "seat.rename",
            f.rec.id.clone(),
            json!({ "from": f.rec.name, "to": a.name, "path_from": from.as_str(), "path_to": to.as_str() }),
        )];
        if f.rec.lifecycle == Lifecycle::Active {
            effects.push(PlanEffect::new("runtime.rename_tab", f.rec.id.clone(), json!({ "name": a.name })));
        }
        Ok(PlanBody {
            effects,
            relied_on: vec![rev_of(f.rec.id.clone(), f.rec.rev)],
            warnings: vec![],
            repair_required: None,
            summary: format!("rename seat {} to {}", f.rec.name, a.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, _: &Plan) -> Result<Applied, MutationError> {
        let a: SeatRenameArgs = parse_args(args).map_err(mm)?;
        let f = live_seat(&cx.tree, &a.seat).map_err(mm)?;
        let (from, to) = Self::paths(&cx.tree, &f, &a.name).map_err(mm)?;
        let mut rec = f.rec.clone();
        rec.name_history.push(NameChange {
            old: rec.name.clone(),
            new: a.name.clone(),
            observed_at: cx.now,
            event_at: None,
            source: NameSource::Request,
        });
        rec.name = a.name.clone();
        if rec.lifecycle == Lifecycle::Active {
            rec.activation.last_op = Some(cx.op.clone());
        }
        cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
        if from != to {
            cx.tree.move_dir(&from, &to)?;
        }
        Ok(Applied { summary: format!("rename seat {} to {}", f.rec.name, a.name), action: None })
    }
}

struct SeatRetire;
impl OrgKind for SeatRetire {
    fn kind(&self) -> RequestKind {
        RequestKind::SeatRetire
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("seat", "retire")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let s = positional(words, 0).ok_or_else(|| PlanError::Usage("seat retire <seat>".into()))?;
        Ok(json!({ "seat": s }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: SeatRef = parse_args(args)?;
        let f = live_seat(cx.tree, &a.seat)?;
        let _act: ActionId = reserved.get_or_mint("act");
        let mut effects = seat_retire_effects(cx.tree, &f, false)?;
        // Explicit clone listing, in listing order, right after the seat itself.
        let clones: Vec<PlanEffect> = f
            .active_clones()
            .map(|(_, c)| PlanEffect::new("clone.retire", c.id.clone(), json!({ "seat": f.rec.id, "name": c.name })))
            .collect();
        effects.splice(1..1, clones);
        Ok(PlanBody {
            effects,
            relied_on: vec![rev_of(f.rec.id.clone(), f.rec.rev)],
            warnings: vec![],
            repair_required: None,
            summary: format!("retire seat {}", f.rec.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: SeatRef = parse_args(args).map_err(mm)?;
        let f = live_seat(&cx.tree, &a.seat).map_err(mm)?;
        let act: ActionId = plan.reserved.get("act").ok_or_else(|| MutationError::Bug("plan reserved no act id".into()))?;
        let mech = mechanism(cx);
        let mut acc = Acc::default();
        do_retire_seat(cx, &f.rec.id, &act, mech, &mut acc, true)?;
        write_action(cx, &act, ActionKind::Retire, acc, toml::Table::new())?;
        Ok(Applied { summary: format!("retire seat {}", f.rec.name), action: Some(act) })
    }
}

/// `seat.retire`, `runtime.close_tab` (when active) and `exclusion.add` per application; `induced` marks all of them.
fn seat_retire_effects(tree: &dyn TreeRead, f: &SeatFacts, induced: bool) -> Result<Vec<PlanEffect>, PlanError> {
    let archive = layout::archived_seat_dir(&f.ts.loc.folder, basename(&f.loc.folder), &f.rec.id);
    let mark = |e: PlanEffect| if induced { e.induced() } else { e };
    let mut effects = vec![mark(PlanEffect::new(
        "seat.retire",
        f.rec.id.clone(),
        json!({ "name": f.rec.name, "path_from": f.loc.folder.as_str(), "path_to": archive.as_str() }),
    ))];
    if f.rec.lifecycle == Lifecycle::Active {
        effects.push(mark(PlanEffect::new("runtime.close_tab", f.rec.id.clone(), json!({ "name": f.rec.name }))));
    }
    for (_, app, member) in app_exclusions(tree, &f.rec)? {
        effects.push(mark(PlanEffect::new("exclusion.add", app.id.clone(), json!({ "member": member, "seat": f.rec.id }))));
    }
    Ok(effects)
}

struct SeatResurrect;
impl OrgKind for SeatResurrect {
    fn kind(&self) -> RequestKind {
        RequestKind::SeatResurrect
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("seat", "resurrect")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let s = positional(words, 0).ok_or_else(|| PlanError::Usage("seat resurrect <seat> [--active]".into()))?;
        Ok(json!({ "seat": s, "active": has(words, "--active") }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: SeatResurrectArgs = parse_args(args)?;
        let id = grammar::resolve_seat(cx.tree, &a.seat, Scope::Retired)?;
        let f = read_seat(cx.tree, &id)?;
        if f.rec.lifecycle != Lifecycle::Retired {
            return Err(PlanError::Invalid(format!("seat {id} is not retired")));
        }
        ensure_ts_live(&f.ts)?;
        let _act: ActionId = reserved.get_or_mint("act");
        let action = f.rec.retired.as_ref().and_then(|r| r.action.clone());
        let set = retired_by(cx.tree, action.as_ref())?;
        let slug = seat_slug_for(cx.tree, &f.ts.loc.folder, &f.rec.name, &f.rec.id, None)?;
        let live = layout::seat_dir(&f.ts.loc.folder, &slug);
        let mut effects = vec![PlanEffect::new(
            "seat.resurrect",
            f.rec.id.clone(),
            json!({
                "name": f.rec.name, "to": if a.active { "active" } else { "dormant" },
                "path_from": f.loc.folder.as_str(), "path_to": live.as_str(),
            }),
        )];
        let mut revived: Vec<&CloneRecord> = Vec::new();
        for (_, c) in &f.clones {
            if c.lifecycle == CloneLifecycle::Retired
                && retired_with(&set, action.as_ref(), &c.id.to_any(), c.retired.as_ref())
            {
                effects.push(PlanEffect::new("clone.resurrect", c.id.clone(), json!({ "seat": f.rec.id, "name": c.name })));
                revived.push(c);
            }
        }
        if a.active {
            let mut cfg_seat = f.rec.clone();
            cfg_seat.lifecycle = Lifecycle::Active;
            let cfg = resolve_in(cx.tree, &cfg_seat)?;
            if f.ts.rec.lifecycle == Lifecycle::Dormant {
                effects.push(PlanEffect::new("teamspace.activate", f.ts.rec.id.clone(), json!({ "name": f.ts.rec.name })).induced());
            }
            let new_clone = if revived.is_empty() {
                let id: CloneId = reserved.get_or_mint("clone:0");
                effects.push(PlanEffect::new(
                    "clone.add",
                    id.clone(),
                    json!({ "seat": f.rec.id, "name": f.rec.name, "path": layout::clone_dir(&live, &unique_slug(&f.rec.name, id.suffix6(), &BTreeSet::new())).as_str() }),
                ));
                Some(id)
            } else {
                None
            };
            effects.push(PlanEffect::new("runtime.open_tab", f.rec.id.clone(), json!({ "name": f.rec.name })));
            for c in revived {
                effects.extend(open_clone_effects(&c.id, &f.rec.id, &cfg));
            }
            if let Some(id) = new_clone {
                effects.extend(open_clone_effects(&id, &f.rec.id, &cfg));
            }
        }
        Ok(PlanBody {
            effects,
            relied_on: vec![rev_of(f.rec.id.clone(), f.rec.rev)],
            warnings: vec![],
            repair_required: None,
            summary: format!("resurrect seat {}", f.rec.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: SeatResurrectArgs = parse_args(args).map_err(mm)?;
        let id = grammar::resolve_seat(&cx.tree, &a.seat, Scope::Retired).map_err(mm)?;
        let f = read_seat(&cx.tree, &id).map_err(mm)?;
        ensure_ts_live(&f.ts).map_err(mm)?;
        let act: ActionId = plan.reserved.get("act").ok_or_else(|| MutationError::Bug("plan reserved no act id".into()))?;
        let action = f.rec.retired.as_ref().and_then(|r| r.action.clone());
        let set = retired_by(&cx.tree, action.as_ref()).map_err(mm)?;
        let revives_clone = f.clones.iter().any(|(_, c)| {
            c.lifecycle == CloneLifecycle::Retired
                && retired_with(&set, action.as_ref(), &c.id.to_any(), c.retired.as_ref())
        });
        let mut acc = Acc::default();
        do_resurrect_seat(cx, &id, a.active, &set, &mut acc)?;
        if a.active {
            if f.ts.rec.lifecycle == Lifecycle::Dormant {
                let mut t = f.ts.rec.clone();
                t.lifecycle = Lifecycle::Active;
                cx.tree.put_record(f.ts.loc.record_path.clone(), &mut t)?;
            }
            if !revives_clone {
                let cid: CloneId = plan.reserved.get("clone:0").ok_or_else(|| MutationError::Bug("plan reserved no clone id".into()))?;
                let live = layout::locate(&cx.tree, &id.to_any())?
                    .ok_or_else(|| MutationError::Bug("resurrected seat vanished".into()))?
                    .folder;
                let slug = clone_slug_for(&cx.tree, &live, &f.rec.name, &cid).map_err(mm)?;
                let mut clone = new_clone_record(&id, &f.rec.name, cid);
                cx.tree.put_record(layout::clone_record(&layout::clone_dir(&live, &slug)), &mut clone)?;
            }
        }
        let mut comp = toml::Table::new();
        if let Some(a) = &action {
            push_toml_str(&mut comp, "resurrects", a.as_str());
        }
        write_action(cx, &act, ActionKind::Resurrect, acc, comp)?;
        Ok(Applied { summary: format!("resurrect seat {}", f.rec.name), action: Some(act) })
    }
}

// ---------------------------------------------------------------------------------------------
// clone add | retire
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct CloneAddArgs {
    seat: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct CloneRef {
    clone: String,
}

struct CloneAdd;
impl OrgKind for CloneAdd {
    fn kind(&self) -> RequestKind {
        RequestKind::CloneAdd
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("clone", "add")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let s = positional(words, 0).ok_or_else(|| PlanError::Usage("clone add <seat> [--name n]".into()))?;
        Ok(json!({ "seat": s, "name": flag(words, "--name") }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: CloneAddArgs = parse_args(args)?;
        let f = live_seat(cx.tree, &a.seat)?;
        let id: CloneId = reserved.get_or_mint("clone:0");
        let name = a.name.unwrap_or_else(|| f.rec.name.clone());
        let slug = clone_slug_for(cx.tree, &f.loc.folder, &name, &id)?;
        let mut effects = vec![PlanEffect::new(
            "clone.add",
            id.clone(),
            json!({ "seat": f.rec.id, "name": name, "path": layout::clone_dir(&f.loc.folder, &slug).as_str() }),
        )];
        if f.rec.lifecycle == Lifecycle::Active {
            let cfg = resolve_in(cx.tree, &f.rec)?;
            effects.extend(open_clone_effects(&id, &f.rec.id, &cfg));
        }
        Ok(PlanBody {
            effects,
            relied_on: vec![rev_of(f.rec.id.clone(), f.rec.rev)],
            warnings: vec![],
            repair_required: None,
            summary: format!("add clone {name} to seat {}", f.rec.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: CloneAddArgs = parse_args(args).map_err(mm)?;
        let f = live_seat(&cx.tree, &a.seat).map_err(mm)?;
        let id: CloneId = plan.reserved.get("clone:0").ok_or_else(|| MutationError::Bug("plan reserved no clone id".into()))?;
        let name = a.name.unwrap_or_else(|| f.rec.name.clone());
        let slug = clone_slug_for(&cx.tree, &f.loc.folder, &name, &id).map_err(mm)?;
        let mut clone = new_clone_record(&f.rec.id, &name, id);
        cx.tree.put_record(layout::clone_record(&layout::clone_dir(&f.loc.folder, &slug)), &mut clone)?;
        if f.rec.lifecycle == Lifecycle::Active {
            let mut seat = f.rec.clone();
            seat.activation.last_op = Some(cx.op.clone());
            cx.tree.put_record(f.loc.record_path.clone(), &mut seat)?;
        }
        Ok(Applied { summary: format!("add clone {name} to seat {}", f.rec.name), action: None })
    }
}

struct CloneRetire;
struct CloneFacts {
    seat: SeatFacts,
    clone: CloneRecord,
    clone_loc: ObjectLocation,
    last_of_active_seat: bool,
}
impl CloneRetire {
    fn facts(tree: &dyn TreeRead, clone_ref: &str) -> Result<CloneFacts, PlanError> {
        let id = grammar::resolve_clone(tree, clone_ref, Scope::Live)?;
        let loc = layout::locate(tree, &id.to_any())?.ok_or_else(|| PlanError::Invalid(format!("no clone {id}")))?;
        let clone = read_toml::<CloneRecord>(tree, &loc.record_path)?
            .ok_or_else(|| PlanError::Invalid(format!("no clone {id}")))?;
        if clone.lifecycle == CloneLifecycle::Retired {
            return Err(PlanError::Invalid(format!("clone {id} is already retired")));
        }
        let seat = read_seat(tree, &clone.seat)?;
        if seat.rec.lifecycle == Lifecycle::Retired {
            return Err(PlanError::Invalid(format!("seat {} is retired", seat.rec.id)));
        }
        let last_of_active_seat = seat.rec.lifecycle == Lifecycle::Active
            && seat.active_clones().all(|(_, c)| c.id == clone.id);
        Ok(CloneFacts { seat, clone, clone_loc: loc, last_of_active_seat })
    }
}
impl OrgKind for CloneRetire {
    fn kind(&self) -> RequestKind {
        RequestKind::CloneRetire
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("clone", "retire")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let c = positional(words, 0).ok_or_else(|| PlanError::Usage("clone retire <clone>".into()))?;
        Ok(json!({ "clone": c }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, reserved: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: CloneRef = parse_args(args)?;
        let f = Self::facts(cx.tree, &a.clone)?;
        let _act: ActionId = reserved.get_or_mint("act");
        let seat = &f.seat.rec;
        let mut effects = vec![PlanEffect::new("clone.retire", f.clone.id.clone(), json!({ "seat": seat.id, "name": f.clone.name }))];
        let mut warnings = Vec::new();
        let mut relied_on = vec![rev_of(f.clone.id.clone(), f.clone.rev)];
        if seat.lifecycle == Lifecycle::Active {
            effects.push(PlanEffect::new("runtime.close_pane", f.clone.id.clone(), json!({ "seat": seat.id })));
        }
        if f.last_of_active_seat {
            relied_on.push(rev_of(seat.id.clone(), seat.rev));
            effects.extend(seat_retire_effects(cx.tree, &f.seat, true)?);
            warnings.push(format!(
                "retiring the last clone closes the tab and retires seat {}; consider 'seat deactivate {}'",
                seat.name, seat.name
            ));
        }
        Ok(PlanBody {
            effects,
            relied_on,
            warnings,
            repair_required: None,
            summary: format!("retire clone {}", f.clone.name),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, plan: &Plan) -> Result<Applied, MutationError> {
        let a: CloneRef = parse_args(args).map_err(mm)?;
        let f = Self::facts(&cx.tree, &a.clone).map_err(mm)?;
        let act: ActionId = plan.reserved.get("act").ok_or_else(|| MutationError::Bug("plan reserved no act id".into()))?;
        let mech = mechanism(cx);
        let mut acc = Acc::default();
        if f.last_of_active_seat {
            // The induced seat retirement retires every active clone, this one included.
            do_retire_seat(cx, &f.seat.rec.id, &act, mech, &mut acc, true)?;
        } else {
            let mut c = f.clone.clone();
            c.lifecycle = CloneLifecycle::Retired;
            c.retired = Some(Retirement { op: cx.op.clone(), action: Some(act.clone()), at: cx.now, mechanism: mech });
            cx.tree.put_record(f.clone_loc.record_path.clone(), &mut c)?;
            acc.retired.push(c.id.to_any());
            acc.changed(c.id.to_any(), "active", "retired");
            if f.seat.rec.lifecycle == Lifecycle::Active {
                let mut seat = f.seat.rec.clone();
                seat.activation.last_op = Some(cx.op.clone());
                cx.tree.put_record(f.seat.loc.record_path.clone(), &mut seat)?;
            }
        }
        write_action(cx, &act, ActionKind::Retire, acc, toml::Table::new())?;
        Ok(Applied { summary: format!("retire clone {}", f.clone.name), action: Some(act) })
    }
}
