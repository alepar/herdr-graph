//! Template and application mutation kinds (spec §5): template create/edit/copy, application apply (hydrate)
//! and retire. Live propagation of a template edit, exclusivity withdrawal and re-addition share one
//! computation (`compute_edit`) between the plan and the writer-side mutation.
use crate::cli::template as cli;
use crate::daemon::registry::CallerInfo;
use crate::model::action::{ActionKind, AffectedObject};
use crate::model::application::{ApplicationRecord, CreatedBy, Reuse};
use crate::model::change::RequestKind;
use crate::model::clone::CloneRecord;
use crate::model::common::{AppLifecycle, CloneLifecycle, Lifecycle, RetireMechanism, Retirement};
use crate::model::effective::{
    EffectiveSeatConfig, referenced_seat_template, resolve_member_in, resolve_with_seat_template,
    session_replacements,
};
use crate::model::seat::{SeatRecord, TemplateRef};
use crate::model::template::{
    CopiedFrom, Relationship, Startup, TemplateKind, TemplateMember, TemplateRecord,
};
use crate::model::{
    ActionId, AnyId, AppId, CloneId, MemberId, SCHEMA_VERSION, SeatId, TeamspaceId, TemplateId,
};
use crate::plan::core_kinds::{
    Acc, SeatFacts, TsInfo, basename, clone_slug_for, do_resurrect_seat, do_retire_seat,
    ensure_ts_live, mechanism, mm, new_clone_record, open_clone_effects, parse_args, read_seat,
    read_ts, retired_by, retired_with, rev_of, seat_slug_for, write_action,
};
use crate::plan::grammar::{self, Scope, flag, positional};
use crate::plan::kind::{KindRegistry, OrgKind, PlanBody, PlanCx, PlanError};
use crate::plan::types::{Plan, PlanEffect, Reserved, ReservedId};
use crate::ports::store::{ObjectLocation, RepoPath, StoreError};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::slug::{slugify, unique_slug};
use crate::store::tree::TreeRead;
use crate::templates::document::TemplateDocument;
use crate::templates::structure::{
    effective_structure_with, member_seat_record, read_seat_rec, read_template,
};
use crate::templates::withdrawal::{
    Verdict, ambiguity, classify, live_others, membership, withdraw_plan,
};
use crate::writer::{Applied, MutationCx, MutationError};
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

/// Registers the five template/application kinds.
pub fn register_kinds(reg: &mut KindRegistry) {
    reg.register(Arc::new(TemplateCreate));
    reg.register(Arc::new(TemplateEdit));
    reg.register(Arc::new(TemplateCopy));
    reg.register(Arc::new(ApplicationApply));
    reg.register(Arc::new(ApplicationRetire));
}

// ---------------------------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------------------------

/// Plan-time ids come from `Reserved` (minted while planning); the writer reads the same slots back.
enum Ids<'a> {
    Mint(&'a mut Reserved),
    Fixed(&'a Reserved),
}

impl Ids<'_> {
    fn get<I: ReservedId>(&mut self, slot: &str) -> Result<I, PlanError> {
        match self {
            Ids::Mint(r) => Ok(r.get_or_mint(slot)),
            Ids::Fixed(r) => r
                .get(slot)
                .ok_or_else(|| PlanError::Invalid(format!("plan reserved no id for slot {slot}"))),
        }
    }
}

fn reserved_id<I: ReservedId>(plan: &Plan, slot: &str) -> Result<I, MutationError> {
    plan.reserved
        .get(slot)
        .ok_or_else(|| MutationError::Bug(format!("plan reserved no id for slot {slot}")))
}

fn templates_root() -> RepoPath {
    RepoPath::new("templates").expect("static path")
}

fn resolve_template(
    tree: &dyn TreeRead,
    s: &str,
) -> Result<(ObjectLocation, TemplateRecord), PlanError> {
    if let Ok(id) = TemplateId::parse(s) {
        return read_template(tree, &id)?
            .ok_or_else(|| PlanError::Invalid(format!("no template {id}")));
    }
    let mut found: Vec<_> = layout::list_templates(tree)?
        .into_iter()
        .filter(|(_, t)| t.name == s)
        .collect();
    match found.len() {
        0 => Err(PlanError::Invalid(format!("no template named {s:?}"))),
        1 => Ok(found.remove(0)),
        _ => Err(PlanError::Invalid(format!(
            "template name {s:?} is ambiguous; use an id"
        ))),
    }
}

fn read_app(
    tree: &dyn TreeRead,
    id: &AppId,
) -> Result<Option<(ObjectLocation, ApplicationRecord)>, StoreError> {
    let Some(loc) = layout::locate(tree, &id.to_any())? else {
        return Ok(None);
    };
    Ok(read_toml::<ApplicationRecord>(tree, &loc.record_path)?.map(|r| (loc, r)))
}

fn resolve_application(
    tree: &dyn TreeRead,
    s: &str,
) -> Result<(ObjectLocation, ApplicationRecord), PlanError> {
    let (loc, rec) = if let Ok(id) = AppId::parse(s) {
        read_app(tree, &id)?.ok_or_else(|| PlanError::Invalid(format!("no application {id}")))?
    } else {
        let mut found: Vec<_> = layout::list_applications(tree)?
            .into_iter()
            .filter(|(_, a)| a.name == s && a.lifecycle == AppLifecycle::Active)
            .collect();
        match found.len() {
            0 => return Err(PlanError::Invalid(format!("no application named {s:?}"))),
            1 => found.remove(0),
            _ => {
                return Err(PlanError::Invalid(format!(
                    "application name {s:?} is ambiguous; use an id"
                )));
            }
        }
    };
    if rec.lifecycle == AppLifecycle::Retired {
        return Err(PlanError::Invalid(format!(
            "application {} is already retired",
            rec.id
        )));
    }
    Ok((loc, rec))
}

fn agents_md(
    tree: &dyn TreeRead,
    tpl_dir: &RepoPath,
    member_name: &str,
) -> Result<Option<Vec<u8>>, StoreError> {
    tree.read_file(&layout::member_agents_md(tpl_dir, &slugify(member_name)))
}

/// `doc` with `agents_md` filled from the template folder (so a diff against an edit is meaningful).
pub(crate) fn doc_with_agents(
    tree: &dyn TreeRead,
    tpl_dir: &RepoPath,
    rec: &TemplateRecord,
) -> Result<TemplateDocument, StoreError> {
    let mut doc = TemplateDocument::from_record(rec);
    doc.agents_md = Some(
        tree.read_file(&tpl_dir.join("AGENTS.md")?)?
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default(),
    );
    for m in &mut doc.members {
        m.agents_md = Some(
            agents_md(tree, tpl_dir, &m.name)?
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default(),
        );
    }
    Ok(doc)
}

fn write_agents(
    cx: &mut MutationCx<'_>,
    tpl_dir: &RepoPath,
    doc: &TemplateDocument,
) -> Result<(), MutationError> {
    if let Some(text) = &doc.agents_md {
        let path = tpl_dir.join("AGENTS.md")?;
        if text.is_empty() {
            cx.tree.delete_file(&path);
        } else {
            cx.tree.put_file(path, text.as_bytes().to_vec());
        }
    }
    for m in &doc.members {
        if let Some(text) = &m.agents_md {
            let path = layout::member_agents_md(tpl_dir, &slugify(&m.name));
            if text.is_empty() {
                cx.tree.delete_file(&path);
            } else {
                cx.tree.put_file(path, text.as_bytes().to_vec());
            }
        }
    }
    Ok(())
}

fn str_array<'a>(items: impl IntoIterator<Item = &'a str>) -> toml::Value {
    toml::Value::Array(
        items
            .into_iter()
            .map(|s| toml::Value::String(s.to_owned()))
            .collect(),
    )
}

fn state_value(lifecycle: &str) -> toml::Value {
    let mut t = toml::Table::new();
    t.insert("lifecycle".into(), toml::Value::String(lifecycle.into()));
    toml::Value::Table(t)
}

fn created_object(acc: &mut Acc, id: AnyId, lifecycle: &str) {
    acc.affected.push(AffectedObject {
        object: id,
        before: None,
        after: Some(state_value(lifecycle)),
    });
}

fn startup_lifecycle(s: Startup) -> &'static str {
    if s == Startup::Active {
        "active"
    } else {
        "dormant"
    }
}

fn read_json_arg(
    words: &[String],
    caller: &CallerInfo,
    usage: &str,
) -> Result<serde_json::Value, PlanError> {
    let from = flag(words, "--from").ok_or_else(|| PlanError::Usage(usage.into()))?;
    let mut path = PathBuf::from(from);
    if path.is_relative()
        && let Some(cwd) = &caller.cwd
    {
        path = cwd.join(path);
    }
    let doc = cli::read_document(&path).map_err(|e| PlanError::Invalid(e.to_string()))?;
    Ok(serde_json::to_value(doc).expect("a template document serializes"))
}

/// Every value of a repeatable `--name <v>` flag.
fn flag_values(words: &[String], name: &str) -> Vec<String> {
    let eq = format!("{name}=");
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if words[i] == name {
            if let Some(v) = words.get(i + 1).filter(|v| !v.starts_with("--")) {
                out.push(v.clone());
            }
            i += 1;
        } else if let Some(v) = words[i].strip_prefix(&eq) {
            out.push(v.to_owned());
        }
        i += 1;
    }
    out
}

/// The live application that provides `seat` (its first live application), if any.
fn seat_owner(
    tree: &dyn TreeRead,
    seat: &SeatRecord,
) -> Result<Option<ApplicationRecord>, StoreError> {
    for id in &seat.applications {
        if let Some((_, a)) = read_app(tree, id)?
            && a.lifecycle == AppLifecycle::Active
        {
            return Ok(Some(a));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// effects shared by hydrate and live propagation
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn create_effects(
    tpl: &TemplateId,
    member: &TemplateMember,
    app: &AppId,
    ts: &TsInfo,
    seat: &SeatId,
    clone: Option<&CloneId>,
    cfg: &EffectiveSeatConfig,
    activated: &mut BTreeSet<TeamspaceId>,
) -> Vec<PlanEffect> {
    let mut v = vec![PlanEffect::new(
        "seat.create",
        seat.clone(),
        json!({
            "name": member.name, "teamspace": ts.rec.id, "lifecycle": startup_lifecycle(member.startup),
            "member": member.id, "template": tpl, "application": app,
            "seat_template": member.seat_template, "responsibility": member.responsibility,
        }),
    )];
    if let Some(clone) = clone {
        v.push(PlanEffect::new(
            "clone.add",
            clone.clone(),
            json!({ "seat": seat, "name": member.name }),
        ));
        if ts.rec.lifecycle == Lifecycle::Dormant && activated.insert(ts.rec.id.clone()) {
            v.push(
                PlanEffect::new(
                    "teamspace.activate",
                    ts.rec.id.clone(),
                    json!({ "name": ts.rec.name }),
                )
                .induced(),
            );
        }
        v.push(PlanEffect::new(
            "runtime.open_tab",
            seat.clone(),
            json!({ "name": member.name }),
        ));
        v.extend(open_clone_effects(clone, seat, cfg));
    }
    v
}

/// Retire effects for a seat leaving `app` by withdrawal.
fn withdrawal_effects(
    tree: &dyn TreeRead,
    seat: &SeatId,
    app: &AppId,
) -> Result<Vec<PlanEffect>, PlanError> {
    let f = read_seat(tree, seat)?;
    let archive = layout::archived_seat_dir(&f.ts.loc.folder, basename(&f.loc.folder), &f.rec.id);
    let mut v = vec![PlanEffect::new(
        "seat.retire",
        f.rec.id.clone(),
        json!({
            "name": f.rec.name, "mechanism": "application_withdrawal", "application": app,
            "path_from": f.loc.folder.as_str(), "path_to": archive.as_str(),
        }),
    )];
    for (_, c) in f
        .clones
        .iter()
        .filter(|(_, c)| c.lifecycle == CloneLifecycle::Active)
    {
        v.push(PlanEffect::new(
            "clone.retire",
            c.id.clone(),
            json!({ "seat": f.rec.id, "name": c.name }),
        ));
    }
    if f.rec.lifecycle == Lifecycle::Active {
        v.push(PlanEffect::new(
            "runtime.close_tab",
            f.rec.id.clone(),
            json!({ "name": f.rec.name }),
        ));
    }
    Ok(v)
}

/// Seat + clone records and the member's AGENTS.md for one template member (hydration and live propagation).
#[allow(clippy::too_many_arguments)]
fn create_member_seat(
    cx: &mut MutationCx<'_>,
    tpl: &TemplateRecord,
    tpl_dir: &RepoPath,
    member: &TemplateMember,
    app: &AppId,
    ts_id: &TeamspaceId,
    seat_id: &SeatId,
    clone_id: Option<&CloneId>,
    acc: &mut Acc,
) -> Result<(), MutationError> {
    let ts = read_ts(&cx.tree, ts_id).map_err(mm)?;
    ensure_ts_live(&ts).map_err(mm)?;
    let slug = seat_slug_for(&cx.tree, &ts.loc.folder, &member.name, seat_id, None).map_err(mm)?;
    let seat_dir = layout::seat_dir(&ts.loc.folder, &slug);
    let mut rec = member_seat_record(seat_id.clone(), &tpl.id, member, ts_id, app);
    let active = member.startup == Startup::Active;
    if active {
        rec.activation.last_op = Some(cx.op.clone());
        let clone_id = clone_id
            .ok_or_else(|| MutationError::Bug("active member without a clone id".into()))?;
        let cslug = clone_slug_for(&cx.tree, &seat_dir, &member.name, clone_id).map_err(mm)?;
        let mut clone = new_clone_record(seat_id, &member.name, clone_id.clone());
        cx.tree.put_record(
            layout::clone_record(&layout::clone_dir(&seat_dir, &cslug)),
            &mut clone,
        )?;
        created_object(acc, clone_id.to_any(), "active");
        if ts.rec.lifecycle == Lifecycle::Dormant {
            let mut t = ts.rec.clone();
            t.lifecycle = Lifecycle::Active;
            cx.tree.put_record(ts.loc.record_path.clone(), &mut t)?;
        }
    }
    cx.tree
        .put_record(layout::seat_record(&seat_dir), &mut rec)?;
    created_object(acc, seat_id.to_any(), startup_lifecycle(member.startup));
    // Legacy inline templates seed an instance file. Reusable definitions retain live member
    // references so future specialization edits cannot conflict with a stale copied context.
    if member.seat_template.is_none()
        && let Some(bytes) = agents_md(&cx.tree, tpl_dir, &member.name)?
    {
        cx.tree.put_file(seat_dir.join("AGENTS.md")?, bytes);
    }
    Ok(())
}

fn seat_ids_of(acc: &Acc, prefix_seat: bool) -> Vec<AnyId> {
    acc.affected
        .iter()
        .filter(|a| {
            a.before.is_none()
                && a.object
                    .as_str()
                    .starts_with(if prefix_seat { "st_" } else { "cl_" })
        })
        .map(|a| a.object.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// template create
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateArgs {
    name: String,
    document: TemplateDocument,
}

fn invalid(e: String) -> PlanError {
    PlanError::Invalid(e)
}

struct TemplateCreate;
impl OrgKind for TemplateCreate {
    fn kind(&self) -> RequestKind {
        RequestKind::TemplateCreate
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("template", "create")]
    }
    fn parse(&self, words: &[String], caller: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = "template create <name> --from <file.toml>";
        let name = positional(words, 0).ok_or_else(|| PlanError::Usage(usage.into()))?;
        let document = read_json_arg(words, caller, usage)?;
        Ok(json!({ "name": name, "document": document }))
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: CreateArgs = parse_args(args)?;
        let mut doc = a.document;
        doc.name = a.name.clone();
        doc.validate().map_err(invalid)?;
        if layout::list_templates(cx.tree)?
            .iter()
            .any(|(_, t)| t.name == a.name)
        {
            return Err(PlanError::Invalid(format!(
                "a template named {:?} already exists",
                a.name
            )));
        }
        let id: TemplateId = reserved.get_or_mint("template");
        let rec = doc.to_record(id.clone(), 0, &BTreeMap::new(), reserved);
        let dependencies = template_dependencies(cx.tree, &rec)?;
        let slug = unique_slug(
            &a.name,
            id.suffix6(),
            &layout::taken_slugs(cx.tree, &templates_root())?,
        );
        Ok(PlanBody {
            effects: vec![PlanEffect::new(
                "template.create",
                id,
                json!({
                    "name": a.name, "kind": rec.kind, "path": layout::template_dir(&slug).as_str(),
                    "seat_templates": dependencies,
                    "members": rec.members.iter().map(|m| json!({ "id": m.id, "name": m.name })).collect::<Vec<_>>(),
                    "relationships": rec.relationships.len(),
                }),
            )],
            relied_on: dependencies,
            warnings: vec![],
            repair_required: None,
            summary: format!("create template {}", a.name),
        })
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: CreateArgs = parse_args(args).map_err(mm)?;
        let mut doc = a.document;
        doc.name = a.name.clone();
        let id: TemplateId = reserved_id(plan, "template")?;
        let mut reserved = plan.reserved.clone();
        let mut rec = doc.to_record(id.clone(), 0, &BTreeMap::new(), &mut reserved);
        let slug = unique_slug(
            &a.name,
            id.suffix6(),
            &layout::taken_slugs(&cx.tree, &templates_root())?,
        );
        let dir = layout::template_dir(&slug);
        cx.tree
            .put_record(layout::template_record(&dir), &mut rec)?;
        write_agents(cx, &dir, &doc)?;
        Ok(Applied {
            summary: format!("create template {}", a.name),
            action: None,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// template copy
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct CopyArgs {
    template: String,
    name: String,
}

struct TemplateCopy;
impl OrgKind for TemplateCopy {
    fn kind(&self) -> RequestKind {
        RequestKind::TemplateCopy
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("template", "copy")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = || PlanError::Usage("template copy <template> <new-name>".into());
        Ok(
            json!({ "template": positional(words, 0).ok_or_else(usage)?, "name": positional(words, 1).ok_or_else(usage)? }),
        )
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: CopyArgs = parse_args(args)?;
        let (_, src) = resolve_template(cx.tree, &a.template)?;
        if layout::list_templates(cx.tree)?
            .iter()
            .any(|(_, t)| t.name == a.name)
        {
            return Err(PlanError::Invalid(format!(
                "a template named {:?} already exists",
                a.name
            )));
        }
        let id: TemplateId = reserved.get_or_mint("template");
        Ok(PlanBody {
            effects: vec![PlanEffect::new(
                "template.copy",
                id,
                json!({
                    "name": a.name, "from": src.id, "kind": src.kind,
                    "seat_templates": template_dependencies(cx.tree, &src)?,
                    "members": src.members.iter().map(|m| json!({ "id": m.id, "name": m.name })).collect::<Vec<_>>(),
                }),
            )],
            relied_on: vec![rev_of(src.id.clone(), src.rev)],
            warnings: vec![],
            repair_required: None,
            summary: format!("copy template {} to {}", src.name, a.name),
        })
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: CopyArgs = parse_args(args).map_err(mm)?;
        let (src_loc, src) = resolve_template(&cx.tree, &a.template).map_err(mm)?;
        let id: TemplateId = reserved_id(plan, "template")?;
        let mut rec = src.clone();
        rec.id = id.clone();
        rec.name = a.name.clone();
        rec.name_history = vec![];
        rec.copied_from = Some(CopiedFrom {
            template: src.id.clone(),
            at: cx.now,
        });
        let slug = unique_slug(
            &a.name,
            id.suffix6(),
            &layout::taken_slugs(&cx.tree, &templates_root())?,
        );
        let dir = layout::template_dir(&slug);
        cx.tree
            .put_record(layout::template_record(&dir), &mut rec)?;
        if let Some(bytes) = cx.tree.read_file(&src_loc.folder.join("AGENTS.md")?)? {
            cx.tree.put_file(dir.join("AGENTS.md")?, bytes);
        }
        for m in &src.members {
            if let Some(bytes) = agents_md(&cx.tree, &src_loc.folder, &m.name)? {
                cx.tree
                    .put_file(layout::member_agents_md(&dir, &slugify(&m.name)), bytes);
            }
        }
        Ok(Applied {
            summary: format!("copy template {} to {}", src.name, a.name),
            action: None,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// template edit: live propagation, withdrawal, re-addition
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct EditArgs {
    template: String,
    document: TemplateDocument,
}

enum Outcome {
    /// The mapped seat is already retired or gone: only the mapping is dropped.
    Gone,
    Retire,
    Keep(String),
}

struct WithdrawOp {
    member: MemberId,
    seat: SeatId,
    outcome: Outcome,
}

enum AddOp {
    Create {
        member: TemplateMember,
        seat: SeatId,
        clone: Option<CloneId>,
    },
    Resurrect {
        member: TemplateMember,
        seat: SeatId,
        new_clone: Option<CloneId>,
    },
}

struct AppEdit {
    app: ApplicationRecord,
    ts: TsInfo,
    withdraw: Vec<WithdrawOp>,
    adds: Vec<AddOp>,
    rel_added: Vec<Relationship>,
    rel_removed: Vec<Relationship>,
    /// Seats of this application whose running sessions the edit replaces.
    replaced_seats: Vec<SeatId>,
}

struct EditPlan {
    apps: Vec<AppEdit>,
    /// `session.replace` effects, once per clone.
    replacements: Vec<PlanEffect>,
    dependencies: Vec<crate::model::change::ReliedOn>,
    warnings: Vec<String>,
    repair_required: Option<String>,
}

/// The seat an earlier edit retired by withdrawal for `member` of `app` (the latest one), if any.
fn find_withdrawn(
    tree: &dyn TreeRead,
    tpl: &TemplateId,
    member: &MemberId,
    app: &ApplicationRecord,
) -> Result<Option<SeatRecord>, StoreError> {
    let want = TemplateRef {
        template: tpl.clone(),
        member: member.clone(),
    };
    Ok(layout::all_seats(tree)?
        .into_iter()
        .map(|(_, s)| s)
        .filter(|s| {
            s.lifecycle == Lifecycle::Retired
                && s.template_ref.as_ref() == Some(&want)
                && s.teamspace == app.teamspace
                && s.applications.contains(&app.id)
                && s.retired
                    .as_ref()
                    .is_some_and(|r| r.mechanism == RetireMechanism::ApplicationWithdrawal)
        })
        .max_by_key(|s| s.retired.as_ref().map(|r| r.at)))
}

fn compute_edit(
    tree: &dyn TreeRead,
    old: &TemplateRecord,
    new: &TemplateRecord,
    ids: &mut Ids<'_>,
) -> Result<EditPlan, PlanError> {
    if old.kind == TemplateKind::Seat {
        return compute_seat_edit(tree, old, new);
    }
    let mut apps = Vec::new();
    let mut warnings = Vec::new();
    let mut repair_required = None;
    let templated: Vec<ApplicationRecord> = layout::list_applications(tree)?
        .into_iter()
        .map(|(_, a)| a)
        .filter(|a| a.template == old.id && a.lifecycle == AppLifecycle::Active)
        .collect();
    for app in templated {
        let ts = read_ts(tree, &app.teamspace)?;
        if ts.rec.lifecycle == Lifecycle::Retired {
            warnings.push(format!(
                "application {} skipped: its teamspace {} is retired",
                app.name, ts.rec.name
            ));
            continue;
        }
        let others = live_others(tree, &app)?;
        let mut withdraw = Vec::new();
        for (mem, seat_id) in &app.member_map {
            if new.members.iter().any(|m| &m.id == mem) {
                continue;
            }
            let outcome = match read_seat_rec(tree, seat_id)? {
                None => Outcome::Gone,
                Some((_, s)) if s.lifecycle == Lifecycle::Retired => Outcome::Gone,
                Some((loc, s)) => match classify(&app, &others, seat_id) {
                    Verdict::Keep(why) => Outcome::Keep(why),
                    Verdict::Retire => {
                        if repair_required.is_none() {
                            let clones: Vec<CloneRecord> = layout::list_clones(tree, &loc.folder)?
                                .into_iter()
                                .map(|(_, c)| c)
                                .collect();
                            repair_required = ambiguity(tree, &app, &s, &clones, &others)?;
                        }
                        Outcome::Retire
                    }
                },
            };
            withdraw.push(WithdrawOp {
                member: mem.clone(),
                seat: seat_id.clone(),
                outcome,
            });
        }
        let mut adds = Vec::new();
        for d in effective_structure_with(tree, new, &app)?.to_create() {
            let Some(mem) = d.member.clone() else {
                continue;
            };
            let member = new
                .members
                .iter()
                .find(|m| m.id == mem)
                .expect("structure lists template members")
                .clone();
            if let Some(seat) = find_withdrawn(tree, &new.id, &mem, &app)? {
                let action = seat.retired.as_ref().and_then(|r| r.action.clone());
                let set = retired_by(tree, action.as_ref())?;
                let (loc, _) =
                    read_seat_rec(tree, &seat.id)?.expect("the withdrawn seat was just read");
                let revived = layout::list_clones(tree, &loc.folder)?
                    .into_iter()
                    .any(|(_, c)| {
                        c.lifecycle == CloneLifecycle::Retired
                            && retired_with(
                                &set,
                                action.as_ref(),
                                &c.id.to_any(),
                                c.retired.as_ref(),
                            )
                    });
                let new_clone = if member.startup == Startup::Active && !revived {
                    Some(ids.get::<CloneId>(&format!("clone:{}:{}", app.id, mem))?)
                } else {
                    None
                };
                adds.push(AddOp::Resurrect {
                    member,
                    seat: seat.id,
                    new_clone,
                });
            } else {
                let seat = ids.get::<SeatId>(&format!("seat:{}:{}", app.id, mem))?;
                let clone = if member.startup == Startup::Active {
                    Some(ids.get::<CloneId>(&format!("clone:{}:{}", app.id, mem))?)
                } else {
                    None
                };
                adds.push(AddOp::Create {
                    member,
                    seat,
                    clone,
                });
            }
        }
        let rel_added = new
            .relationships
            .iter()
            .filter(|r| !old.relationships.contains(r))
            .cloned()
            .collect::<Vec<_>>();
        let rel_removed = old
            .relationships
            .iter()
            .filter(|r| !new.relationships.contains(r))
            .cloned()
            .collect::<Vec<_>>();
        apps.push(AppEdit {
            app,
            ts,
            withdraw,
            adds,
            rel_added,
            rel_removed,
            replaced_seats: vec![],
        });
    }

    let (replacements, dependencies) = compute_team_replacements(tree, old, new, &mut apps)?;
    Ok(EditPlan {
        apps,
        replacements,
        dependencies,
        warnings,
        repair_required,
    })
}

fn build_new_record(
    old: &TemplateRecord,
    doc: &TemplateDocument,
    reserved: &mut Reserved,
) -> Result<TemplateRecord, PlanError> {
    doc.validate().map_err(invalid)?;
    if doc.kind != old.kind {
        return Err(PlanError::Invalid(
            "template edit cannot change kind".into(),
        ));
    }
    if doc.name != old.name {
        return Err(PlanError::Invalid(format!(
            "template edit cannot rename {:?} to {:?}; the document must keep the template's name",
            old.name, doc.name
        )));
    }
    // A member written without an id that matches an existing member by name keeps that member's id.
    let by_name: BTreeMap<String, MemberId> = old
        .members
        .iter()
        .map(|m| (m.name.clone(), m.id.clone()))
        .collect();
    let mut rec = doc.to_record(old.id.clone(), old.rev, &by_name, reserved);
    rec.name_history = old.name_history.clone();
    rec.copied_from = old.copied_from.clone();
    Ok(rec)
}

struct TemplateEdit;
impl OrgKind for TemplateEdit {
    fn kind(&self) -> RequestKind {
        RequestKind::TemplateEdit
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("template", "edit")]
    }
    fn parse(&self, words: &[String], caller: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = "template edit <template> --from <file.toml>";
        let tpl = positional(words, 0).ok_or_else(|| PlanError::Usage(usage.into()))?;
        let document = read_json_arg(words, caller, usage)?;
        Ok(json!({ "template": tpl, "document": document }))
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: EditArgs = parse_args(args)?;
        let (loc, old) = resolve_template(cx.tree, &a.template)?;
        let _act: ActionId = reserved.get_or_mint("act");
        let new = build_new_record(&old, &a.document, reserved)?;
        let before = doc_with_agents(cx.tree, &loc.folder, &old)?;
        let after = merged_after(&new, &a.document);
        let changes = TemplateDocument::diff(&before, &after);
        if changes.is_empty() {
            return Err(PlanError::Invalid(format!(
                "template {} is unchanged by this document",
                old.name
            )));
        }
        let edit = compute_edit(cx.tree, &old, &new, &mut Ids::Mint(reserved))?;

        let mut effects = vec![PlanEffect::new(
            "template.edit",
            old.id.clone(),
            json!({ "name": old.name, "changed_fields": changes.iter().map(|c| c.path.clone()).collect::<Vec<_>>(),
                "changes": changes.iter().map(|c| json!({ "path": c.path, "before": c.before, "after": c.after })).collect::<Vec<_>>() }),
        )];
        let mut relied_on = vec![rev_of(old.id.clone(), old.rev)];
        relied_on.extend(template_dependencies(cx.tree, &new)?);
        relied_on.extend(edit.dependencies.clone());
        let graph = layout::read_graph(cx.tree)?;
        let mut activated = BTreeSet::new();
        for ae in &edit.apps {
            relied_on.push(rev_of(ae.app.id.clone(), ae.app.rev));
            if let Some((_, tpl)) = read_template(cx.tree, &ae.app.template)? {
                relied_on.push(rev_of(tpl.id.clone(), tpl.rev));
                relied_on.extend(template_dependencies(cx.tree, &tpl)?);
            }
            for id in membership(&ae.app) {
                if let Some((loc, seat)) = read_seat_rec(cx.tree, &id)? {
                    relied_on.push(rev_of(seat.id.clone(), seat.rev));
                    if let Some(reference) = &seat.template_ref
                        && let Some((_, owner)) = read_template(cx.tree, &reference.template)?
                    {
                        relied_on.push(rev_of(owner.id.clone(), owner.rev));
                        relied_on.extend(template_dependencies(cx.tree, &owner)?);
                    }
                    for (_, clone) in layout::list_clones(cx.tree, &loc.folder)? {
                        relied_on.push(rev_of(clone.id.clone(), clone.rev));
                    }
                }
            }
            effects.push(PlanEffect::new(
                "application.update",
                ae.app.id.clone(),
                json!({
                    "name": ae.app.name, "withdrawn": ae.withdraw.len(), "added": ae.adds.len(),
                    "replaced_seats": ae.replaced_seats,
                }),
            ));
            for w in &ae.withdraw {
                let (outcome, reason) = match &w.outcome {
                    Outcome::Gone => ("gone", None),
                    Outcome::Retire => ("retired", None),
                    Outcome::Keep(why) => ("kept", Some(why.clone())),
                };
                effects.push(PlanEffect::new(
                    "application.unmap",
                    ae.app.id.clone(),
                    json!({ "member": w.member, "seat": w.seat, "outcome": outcome, "reason": reason }),
                ));
                if matches!(w.outcome, Outcome::Retire) {
                    if let Some((_, s)) = read_seat_rec(cx.tree, &w.seat)? {
                        relied_on.push(rev_of(s.id.clone(), s.rev));
                    }
                    effects.extend(withdrawal_effects(cx.tree, &w.seat, &ae.app.id)?);
                }
            }
            for add in &ae.adds {
                match add {
                    AddOp::Create {
                        member,
                        seat,
                        clone,
                    } => {
                        let stand_in = member_seat_record(
                            seat.clone(),
                            &new.id,
                            member,
                            &ae.app.teamspace,
                            &ae.app.id,
                        );
                        let cfg = resolve_member_in(
                            cx.tree,
                            &graph.defaults,
                            Some(&new),
                            Some(member),
                            &stand_in,
                        )?;
                        effects.extend(create_effects(
                            &new.id,
                            member,
                            &ae.app.id,
                            &ae.ts,
                            seat,
                            clone.as_ref(),
                            &cfg,
                            &mut activated,
                        ));
                        effects.push(PlanEffect::new(
                            "application.map",
                            ae.app.id.clone(),
                            json!({ "member": member.id, "seat": seat }),
                        ));
                    }
                    AddOp::Resurrect {
                        member,
                        seat,
                        new_clone,
                    } => {
                        effects.extend(resurrect_effects(
                            cx.tree,
                            &new,
                            &graph,
                            member,
                            seat,
                            new_clone.as_ref(),
                            &ae.app,
                            &ae.ts,
                            &mut activated,
                        )?);
                        effects.push(PlanEffect::new(
                            "application.map",
                            ae.app.id.clone(),
                            json!({ "member": member.id, "seat": seat, "resurrected": true }),
                        ));
                    }
                }
            }
            for r in &ae.rel_added {
                effects.push(PlanEffect::new(
                    "participation.contribute",
                    ae.app.id.clone(),
                    json!({ "thread": r.thread, "kind": "thread_participation" }),
                ));
            }
            for r in &ae.rel_removed {
                effects.push(PlanEffect::new(
                    "participation.withdraw",
                    ae.app.id.clone(),
                    json!({ "thread": r.thread, "kind": "thread_participation" }),
                ));
            }
        }
        effects[0].detail["template_dependencies"] = json!(
            relied_on
                .iter()
                .filter(|r| r.object.kind() == crate::model::IdKind::Template)
                .collect::<Vec<_>>()
        );
        effects.extend(edit.replacements.clone());
        Ok(PlanBody {
            effects,
            relied_on,
            warnings: edit.warnings,
            repair_required: edit.repair_required,
            summary: format!("edit template {}", old.name),
        })
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: EditArgs = parse_args(args).map_err(mm)?;
        let (loc, old) = resolve_template(&cx.tree, &a.template).map_err(mm)?;
        let act: ActionId = reserved_id(plan, "act")?;
        let mut reserved = plan.reserved.clone();
        let mut new = build_new_record(&old, &a.document, &mut reserved).map_err(mm)?;
        let before = doc_with_agents(&cx.tree, &loc.folder, &old)?;
        let after = merged_after(&new, &a.document);
        let changes = TemplateDocument::diff(&before, &after);
        let edit =
            compute_edit(&cx.tree, &old, &new, &mut Ids::Fixed(&plan.reserved)).map_err(mm)?;

        // The template itself (the applications below resolve their config against the new record).
        cx.tree.put_record(loc.record_path.clone(), &mut new)?;
        write_agents(cx, &loc.folder, &a.document)?;

        let mut acc = Acc::default();
        let mut created: Vec<AnyId> = Vec::new();
        let mut resurrected: Vec<AnyId> = Vec::new();
        for ae in edit.apps {
            let mut app = ae.app.clone();
            for w in &ae.withdraw {
                match &w.outcome {
                    Outcome::Retire => do_retire_seat(
                        cx,
                        &w.seat,
                        &act,
                        RetireMechanism::ApplicationWithdrawal,
                        &mut acc,
                        false,
                    )?,
                    Outcome::Keep(_) => detach_seat(cx, &w.seat, &app.id)?,
                    Outcome::Gone => {}
                }
                app.member_map.remove(&w.member);
                app.reused.retain(|r| r.seat != w.seat);
            }
            for add in &ae.adds {
                match add {
                    AddOp::Create {
                        member,
                        seat,
                        clone,
                    } => {
                        create_member_seat(
                            cx,
                            &new,
                            &loc.folder,
                            member,
                            &app.id,
                            &app.teamspace,
                            seat,
                            clone.as_ref(),
                            &mut acc,
                        )?;
                        app.member_map.insert(member.id.clone(), seat.clone());
                        created.push(seat.to_any());
                    }
                    AddOp::Resurrect {
                        member,
                        seat,
                        new_clone,
                    } => {
                        resurrect_member_seat(
                            cx,
                            member,
                            seat,
                            new_clone.as_ref(),
                            &app.id,
                            &mut acc,
                        )?;
                        app.member_map.insert(member.id.clone(), seat.clone());
                        resurrected.push(seat.to_any());
                    }
                }
            }
            app.contributions
                .relationships
                .retain(|r| !ae.rel_removed.contains(r));
            for r in &ae.rel_added {
                if !app.contributions.relationships.contains(r) {
                    app.contributions.relationships.push(r.clone());
                }
            }
            let app_loc = layout::locate(&cx.tree, &app.id.to_any())?
                .ok_or_else(|| MutationError::Bug(format!("application {} vanished", app.id)))?;
            cx.tree.put_record(app_loc.record_path, &mut app)?;
        }

        let mut comp = toml::Table::new();
        comp.insert("template".into(), toml::Value::String(old.id.to_string()));
        comp.insert("before".into(), toml::Value::Table(before.to_table()));
        comp.insert("after".into(), toml::Value::Table(after.to_table()));
        comp.insert(
            "changed_fields".into(),
            str_array(changes.iter().map(|c| c.path.as_str())),
        );
        comp.insert(
            "created".into(),
            str_array(created.iter().map(|i| i.as_str())),
        );
        comp.insert(
            "resurrected".into(),
            str_array(resurrected.iter().map(|i| i.as_str())),
        );
        acc.affected.insert(
            0,
            AffectedObject {
                object: old.id.to_any(),
                before: Some(toml::Value::Table(before.to_table())),
                after: Some(toml::Value::Table(after.to_table())),
            },
        );
        write_action(cx, &act, ActionKind::TemplateEdit, acc, comp)?;
        Ok(Applied {
            summary: format!("edit template {}", old.name),
            action: Some(act),
        })
    }
}

/// The edited document as stored: ids resolved, `agents_md` kept only where the edit sets it.
fn merged_after(new: &TemplateRecord, doc: &TemplateDocument) -> TemplateDocument {
    let mut after = TemplateDocument::from_record(new);
    after.agents_md = doc.agents_md.clone();
    for m in &mut after.members {
        m.agents_md = doc
            .members
            .iter()
            .find(|d| d.name == m.name)
            .and_then(|d| d.agents_md.clone());
    }
    after
}

/// A kept seat stops being part of `app`.
fn detach_seat(cx: &mut MutationCx<'_>, seat: &SeatId, app: &AppId) -> Result<(), MutationError> {
    if let Some((loc, mut rec)) = read_seat_rec(&cx.tree, seat)?
        && rec.applications.contains(app)
    {
        rec.applications.retain(|a| a != app);
        cx.tree.put_record(loc.record_path, &mut rec)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resurrect_effects(
    tree: &dyn TreeRead,
    tpl: &TemplateRecord,
    graph: &crate::model::graph::GraphRecord,
    member: &TemplateMember,
    seat: &SeatId,
    new_clone: Option<&CloneId>,
    app: &ApplicationRecord,
    ts: &TsInfo,
    activated: &mut BTreeSet<TeamspaceId>,
) -> Result<Vec<PlanEffect>, PlanError> {
    let f = read_seat(tree, seat)?;
    let active = member.startup == Startup::Active;
    let action = f.rec.retired.as_ref().and_then(|r| r.action.clone());
    let set = retired_by(tree, action.as_ref())?;
    let mut v = vec![PlanEffect::new(
        "seat.resurrect",
        seat.clone(),
        json!({
            "name": f.rec.name, "to": if active { "active" } else { "dormant" },
            "member": member.id, "application": app.id,
        }),
    )];
    let mut revived = Vec::new();
    for (_, c) in &f.clones {
        if c.lifecycle == CloneLifecycle::Retired
            && retired_with(&set, action.as_ref(), &c.id.to_any(), c.retired.as_ref())
        {
            v.push(PlanEffect::new(
                "clone.resurrect",
                c.id.clone(),
                json!({ "seat": seat, "name": c.name }),
            ));
            revived.push(c.id.clone());
        }
    }
    if active {
        let mut cfg_seat = f.rec.clone();
        cfg_seat.lifecycle = Lifecycle::Active;
        let cfg = resolve_member_in(tree, &graph.defaults, Some(tpl), Some(member), &cfg_seat)?;
        if ts.rec.lifecycle == Lifecycle::Dormant && activated.insert(ts.rec.id.clone()) {
            v.push(
                PlanEffect::new(
                    "teamspace.activate",
                    ts.rec.id.clone(),
                    json!({ "name": ts.rec.name }),
                )
                .induced(),
            );
        }
        if let Some(c) = new_clone {
            v.push(PlanEffect::new(
                "clone.add",
                c.clone(),
                json!({ "seat": seat, "name": f.rec.name }),
            ));
        }
        v.push(PlanEffect::new(
            "runtime.open_tab",
            seat.clone(),
            json!({ "name": f.rec.name }),
        ));
        for c in revived.iter().chain(new_clone) {
            v.extend(open_clone_effects(c, seat, &cfg));
        }
    }
    Ok(v)
}

fn resurrect_member_seat(
    cx: &mut MutationCx<'_>,
    member: &TemplateMember,
    seat: &SeatId,
    new_clone: Option<&CloneId>,
    app: &AppId,
    acc: &mut Acc,
) -> Result<(), MutationError> {
    let f = read_seat(&cx.tree, seat).map_err(mm)?;
    ensure_ts_live(&f.ts).map_err(mm)?;
    let active = member.startup == Startup::Active;
    let action = f.rec.retired.as_ref().and_then(|r| r.action.clone());
    let set = retired_by(&cx.tree, action.as_ref()).map_err(mm)?;
    do_resurrect_seat(cx, seat, active, &set, acc)?;
    let (loc, mut rec) = read_seat_rec(&cx.tree, seat)?
        .ok_or_else(|| MutationError::Bug("resurrected seat vanished".into()))?;
    if !rec.applications.contains(app) {
        rec.applications.push(app.clone());
        cx.tree.put_record(loc.record_path, &mut rec)?;
    }
    if active {
        if f.ts.rec.lifecycle == Lifecycle::Dormant {
            let mut t = f.ts.rec.clone();
            t.lifecycle = Lifecycle::Active;
            cx.tree.put_record(f.ts.loc.record_path.clone(), &mut t)?;
        }
        if let Some(cid) = new_clone {
            let live = layout::locate(&cx.tree, &seat.to_any())?
                .ok_or_else(|| MutationError::Bug("resurrected seat vanished".into()))?
                .folder;
            let slug = clone_slug_for(&cx.tree, &live, &f.rec.name, cid).map_err(mm)?;
            let mut clone = new_clone_record(seat, &f.rec.name, cid.clone());
            cx.tree.put_record(
                layout::clone_record(&layout::clone_dir(&live, &slug)),
                &mut clone,
            )?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// application apply (hydrate)
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct ApplyArgs {
    template: String,
    teamspace: String,
    name: String,
    #[serde(default)]
    reuse: BTreeMap<String, String>,
}

struct HydratePlan {
    loc: ObjectLocation,
    tpl: TemplateRecord,
    ts: TsInfo,
    /// Members mapped to an existing seat.
    reused: BTreeMap<MemberId, SeatFacts>,
}

fn hydrate_facts(tree: &dyn TreeRead, a: &ApplyArgs) -> Result<HydratePlan, PlanError> {
    let (loc, tpl) = resolve_template(tree, &a.template)?;
    if tpl.kind != TemplateKind::Team {
        return Err(PlanError::Invalid(
            "application apply requires a team template".into(),
        ));
    }
    template_dependencies(tree, &tpl)?;
    let ts_id = grammar::resolve_teamspace(tree, &a.teamspace, Scope::Live)?;
    let ts = read_ts(tree, &ts_id)?;
    ensure_ts_live(&ts)?;
    if a.name.trim().is_empty() {
        return Err(PlanError::Usage(
            "application apply needs a non-empty --name".into(),
        ));
    }
    if layout::list_applications(tree)?
        .iter()
        .any(|(_, x)| x.name == a.name && x.lifecycle == AppLifecycle::Active)
    {
        return Err(PlanError::Invalid(format!(
            "an application named {:?} is already active",
            a.name
        )));
    }
    let mut reused = BTreeMap::new();
    for (mem, seat_ref) in &a.reuse {
        let mem =
            MemberId::parse(mem).map_err(|e| PlanError::Invalid(format!("--reuse {mem}: {e}")))?;
        if !tpl.members.iter().any(|m| m.id == mem) {
            return Err(PlanError::Invalid(format!(
                "--reuse: {mem} is not a member of template {}",
                tpl.name
            )));
        }
        let seat_id = grammar::resolve_seat(tree, seat_ref, Scope::Live)?;
        let f = read_seat(tree, &seat_id)?;
        if f.rec.lifecycle == Lifecycle::Retired {
            return Err(PlanError::Invalid(format!(
                "--reuse: seat {seat_id} is retired"
            )));
        }
        if reused.values().any(|x: &SeatFacts| x.rec.id == seat_id) {
            return Err(PlanError::Invalid(format!(
                "--reuse: seat {seat_id} is mapped to two members"
            )));
        }
        reused.insert(mem, f);
    }
    Ok(HydratePlan {
        loc,
        tpl,
        ts,
        reused,
    })
}

struct ApplicationApply;
impl OrgKind for ApplicationApply {
    fn kind(&self) -> RequestKind {
        RequestKind::ApplicationApply
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("application", "apply")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = || {
            PlanError::Usage("application apply <template> --teamspace <ts> --name <app-name> [--reuse <mem>=<seat>]…".into())
        };
        let tpl = positional(words, 0).ok_or_else(usage)?;
        let ts = flag(words, "--teamspace").ok_or_else(usage)?;
        let name = flag(words, "--name").ok_or_else(usage)?;
        let mut reuse = BTreeMap::new();
        for pair in flag_values(words, "--reuse") {
            let (m, s) = pair.split_once('=').ok_or_else(|| {
                PlanError::Usage(format!("--reuse expects <mem>=<seat>, got {pair:?}"))
            })?;
            if reuse.insert(m.to_owned(), s.to_owned()).is_some() {
                return Err(PlanError::Usage(format!("--reuse names member {m} twice")));
            }
        }
        Ok(json!({ "template": tpl, "teamspace": ts, "name": name, "reuse": reuse }))
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: ApplyArgs = parse_args(args)?;
        let h = hydrate_facts(cx.tree, &a)?;
        let graph = layout::read_graph(cx.tree)?;
        let app: AppId = reserved.get_or_mint("app");
        let _act: ActionId = reserved.get_or_mint("act");
        let mut effects = vec![PlanEffect::new(
            "application.create",
            app.clone(),
            json!({ "name": a.name, "template": h.tpl.id, "teamspace": h.ts.rec.id,
                "seat_templates": template_dependencies(cx.tree, &h.tpl)? }),
        )];
        let mut relied_on = vec![
            rev_of(h.tpl.id.clone(), h.tpl.rev),
            rev_of(h.ts.rec.id.clone(), h.ts.rec.rev),
        ];
        relied_on.extend(template_dependencies(cx.tree, &h.tpl)?);
        let mut activated = BTreeSet::new();
        for m in &h.tpl.members {
            if let Some(f) = h.reused.get(&m.id) {
                let from = seat_owner(cx.tree, &f.rec)?.map(|o| o.id);
                effects.push(PlanEffect::new(
                    "application.reuse",
                    app.clone(),
                    json!({ "member": m.id, "seat": f.rec.id, "from": from }),
                ));
                relied_on.push(rev_of(f.rec.id.clone(), f.rec.rev));
                continue;
            }
            let seat: SeatId = reserved.get_or_mint(&format!("seat:{}", m.id));
            let clone: Option<CloneId> = (m.startup == Startup::Active)
                .then(|| reserved.get_or_mint(&format!("clone:{}", m.id)));
            let stand_in = member_seat_record(seat.clone(), &h.tpl.id, m, &h.ts.rec.id, &app);
            let cfg =
                resolve_member_in(cx.tree, &graph.defaults, Some(&h.tpl), Some(m), &stand_in)?;
            effects.extend(create_effects(
                &h.tpl.id,
                m,
                &app,
                &h.ts,
                &seat,
                clone.as_ref(),
                &cfg,
                &mut activated,
            ));
        }
        for r in &h.tpl.relationships {
            effects.push(PlanEffect::new(
                "participation.contribute",
                app.clone(),
                json!({ "thread": r.thread, "kind": "thread_participation" }),
            ));
        }
        Ok(PlanBody {
            effects,
            relied_on,
            warnings: vec![],
            repair_required: None,
            summary: format!("apply template {} as {}", h.tpl.name, a.name),
        })
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: ApplyArgs = parse_args(args).map_err(mm)?;
        let h = hydrate_facts(&cx.tree, &a).map_err(mm)?;
        let app_id: AppId = reserved_id(plan, "app")?;
        let act: ActionId = reserved_id(plan, "act")?;
        let mut acc = Acc::default();
        let mut member_map = BTreeMap::new();
        let mut reused = Vec::new();
        let mut reused_ids = Vec::new();
        for m in &h.tpl.members {
            if let Some(f) = h.reused.get(&m.id) {
                // Reuse is recorded on both sides: here with the providing application, and on the provider.
                let owner = seat_owner(&cx.tree, &f.rec).map_err(MutationError::Store)?;
                reused.push(Reuse {
                    seat: f.rec.id.clone(),
                    from: owner.as_ref().map(|o| o.id.clone()),
                });
                if let Some(o) = owner {
                    let (oloc, mut orec) = read_app(&cx.tree, &o.id)?.ok_or_else(|| {
                        MutationError::Bug(format!("application {} vanished", o.id))
                    })?;
                    if !orec.reused.iter().any(|r| r.seat == f.rec.id) {
                        orec.reused.push(Reuse {
                            seat: f.rec.id.clone(),
                            from: Some(o.id.clone()),
                        });
                        cx.tree.put_record(oloc.record_path, &mut orec)?;
                    }
                }
                let (sloc, mut srec) = read_seat_rec(&cx.tree, &f.rec.id)?
                    .ok_or_else(|| MutationError::Bug(format!("seat {} vanished", f.rec.id)))?;
                if !srec.applications.contains(&app_id) {
                    srec.applications.push(app_id.clone());
                    cx.tree.put_record(sloc.record_path, &mut srec)?;
                }
                member_map.insert(m.id.clone(), f.rec.id.clone());
                reused_ids.push(f.rec.id.to_any());
                continue;
            }
            let seat: SeatId = reserved_id(plan, &format!("seat:{}", m.id))?;
            let clone: Option<CloneId> = if m.startup == Startup::Active {
                Some(reserved_id(plan, &format!("clone:{}", m.id))?)
            } else {
                None
            };
            create_member_seat(
                cx,
                &h.tpl,
                &h.loc.folder,
                m,
                &app_id,
                &h.ts.rec.id,
                &seat,
                clone.as_ref(),
                &mut acc,
            )?;
            member_map.insert(m.id.clone(), seat);
        }
        let mut app = ApplicationRecord {
            schema: SCHEMA_VERSION,
            id: app_id.clone(),
            rev: 0,
            name: a.name.clone(),
            template: h.tpl.id.clone(),
            teamspace: h.ts.rec.id.clone(),
            lifecycle: AppLifecycle::Active,
            retired: None,
            member_map,
            additions: vec![],
            exclusions: vec![],
            reused,
            contributions: crate::model::application::Contributions {
                relationships: h.tpl.relationships.clone(),
            },
            created_by: CreatedBy {
                op: cx.op.clone(),
                action: act.clone(),
            },
        };
        cx.tree
            .put_record(layout::application_record(&app_id), &mut app)?;
        created_object(&mut acc, app_id.to_any(), "active");

        let created: Vec<AnyId> = std::iter::once(app_id.to_any())
            .chain(seat_ids_of(&acc, true))
            .chain(seat_ids_of(&acc, false))
            .collect();
        let mut comp = toml::Table::new();
        comp.insert(
            "application".into(),
            toml::Value::String(app_id.to_string()),
        );
        comp.insert(
            "created".into(),
            str_array(created.iter().map(|i| i.as_str())),
        );
        comp.insert(
            "reused".into(),
            str_array(reused_ids.iter().map(|i| i.as_str())),
        );
        let rels: Vec<toml::Value> = h
            .tpl
            .relationships
            .iter()
            .filter_map(|r| toml::Value::try_from(r).ok())
            .collect();
        comp.insert("relationships_added".into(), toml::Value::Array(rels));
        write_action(cx, &act, ActionKind::Hydrate, acc, comp)?;
        Ok(Applied {
            summary: format!("apply template {} as {}", h.tpl.name, a.name),
            action: Some(act),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// application retire
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct RetireArgs {
    application: String,
}

struct ApplicationRetire;
impl OrgKind for ApplicationRetire {
    fn kind(&self) -> RequestKind {
        RequestKind::ApplicationRetire
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("application", "retire")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let app = positional(words, 0)
            .ok_or_else(|| PlanError::Usage("application retire <application>".into()))?;
        Ok(json!({ "application": app }))
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: RetireArgs = parse_args(args)?;
        let (_, app) = resolve_application(cx.tree, &a.application)?;
        let _act: ActionId = reserved.get_or_mint("act");
        let preview = withdraw_plan(cx.tree, &app)?;
        let mut effects = vec![PlanEffect::new(
            "application.retire",
            app.id.clone(),
            json!({ "name": app.name, "template": app.template }),
        )];
        let mut relied_on = vec![rev_of(app.id.clone(), app.rev)];
        for seat in &preview.retire {
            effects.extend(withdrawal_effects(cx.tree, seat, &app.id)?);
            if let Some((_, s)) = read_seat_rec(cx.tree, seat)? {
                relied_on.push(rev_of(s.id.clone(), s.rev));
            }
        }
        for (seat, why) in &preview.keep {
            let name = read_seat_rec(cx.tree, seat)?
                .map(|(_, s)| s.name)
                .unwrap_or_default();
            effects.push(PlanEffect::new(
                "seat.keep",
                seat.clone(),
                json!({ "name": name, "reason": why }),
            ));
        }
        for r in &preview.relationships_removed {
            effects.push(PlanEffect::new(
                "participation.withdraw",
                app.id.clone(),
                json!({ "thread": r.thread, "kind": "thread_participation" }),
            ));
        }
        Ok(PlanBody {
            effects,
            relied_on,
            warnings: vec![],
            repair_required: preview.repair_required,
            summary: format!("retire application {}", app.name),
        })
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: RetireArgs = parse_args(args).map_err(mm)?;
        let (loc, mut app) = resolve_application(&cx.tree, &a.application).map_err(mm)?;
        let act: ActionId = reserved_id(plan, "act")?;
        let preview = withdraw_plan(&cx.tree, &app)?;
        let mut acc = Acc::default();
        for seat in &preview.retire {
            do_retire_seat(
                cx,
                seat,
                &act,
                RetireMechanism::ApplicationWithdrawal,
                &mut acc,
                false,
            )?;
        }
        for (seat, _) in &preview.keep {
            detach_seat(cx, seat, &app.id)?;
        }
        let removed: Vec<toml::Value> = preview
            .relationships_removed
            .iter()
            .filter_map(|r| toml::Value::try_from(r).ok())
            .collect();
        let kept: Vec<toml::Value> = preview
            .keep
            .iter()
            .map(|(s, why)| {
                let mut t = toml::Table::new();
                t.insert("seat".into(), toml::Value::String(s.to_string()));
                t.insert("reason".into(), toml::Value::String(why.clone()));
                toml::Value::Table(t)
            })
            .collect();
        app.lifecycle = AppLifecycle::Retired;
        app.retired = Some(Retirement {
            op: cx.op.clone(),
            action: Some(act.clone()),
            at: cx.now,
            mechanism: mechanism(cx),
        });
        app.contributions.relationships.clear();
        cx.tree.put_record(loc.record_path, &mut app)?;
        acc.retired.insert(0, app.id.to_any());
        acc.affected.insert(
            0,
            AffectedObject {
                object: app.id.to_any(),
                before: Some(state_value("active")),
                after: Some(state_value("retired")),
            },
        );
        let mut comp = toml::Table::new();
        comp.insert(
            "application".into(),
            toml::Value::String(app.id.to_string()),
        );
        comp.insert(
            "retired".into(),
            str_array(preview.retire.iter().map(|s| s.as_str())),
        );
        comp.insert("kept".into(), toml::Value::Array(kept));
        comp.insert("relationships_removed".into(), toml::Value::Array(removed));
        write_action(cx, &act, ActionKind::ApplicationRetire, acc, comp)?;
        Ok(Applied {
            summary: format!("retire application {}", app.name),
            action: Some(act),
        })
    }
}

#[path = "seat_definitions.rs"]
mod seat_definitions;
use seat_definitions::{compute_seat_edit, compute_team_replacements, template_dependencies};
