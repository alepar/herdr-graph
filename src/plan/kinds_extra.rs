//! Remaining organizational kinds (override, participation, teamspace rename, clone rebind). Owned by hg-zmi.17.
//! Supersession and reminders live in `ops` / `reminders`. Args shapes are normative for hg-zmi.12 (participation
//! intent) and hg-zmi.14 (`/seat` proposes a rebind).
use super::grammar::{self, Scope, flag, positional};
use super::kind::{KindRegistry, OrgKind, PlanBody, PlanCx, PlanError};
use super::types::{Plan, PlanEffect, Reserved};
use crate::daemon::registry::CallerInfo;
use crate::model::change::{ReliedOn, RequestKind, Version};
use crate::model::clone::CloneRecord;
use crate::model::common::{Availability, Binding, CloneLifecycle, Lifecycle, NameChange, NameSource};
use crate::model::effective::{resolve_in, session_replacements};
use crate::model::harness::Harness;
use crate::model::launch::graph_token;
use crate::model::seat::{InstructionSection, SeatRecord};
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{AnyId, HerdrPaneId, TeamspaceId};
use crate::ports::store::{ObjectLocation, RepoPath};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::slug::unique_slug;
use crate::store::tree::TreeRead;
use crate::writer::{Applied, MutationCx, MutationError, Reject};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

/// Registers teamspace rename, seat override, participation join/leave and clone rebind.
pub fn register_kinds(reg: &mut KindRegistry) {
    reg.register(Arc::new(TeamspaceRename));
    reg.register(Arc::new(SeatOverride));
    reg.register(Arc::new(ParticipationJoin));
    reg.register(Arc::new(ParticipationLeave));
    reg.register(Arc::new(CloneRebind));
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

fn basename(p: &RepoPath) -> &str {
    p.as_str().rsplit('/').next().unwrap_or_default()
}

fn body(effects: Vec<PlanEffect>, relied_on: Vec<ReliedOn>, summary: String) -> PlanBody {
    PlanBody { effects, relied_on, warnings: vec![], repair_required: None, summary }
}

/// Every value of a repeatable `--name v` / `--name=v` flag, in order. Unlike `grammar::flag`, a value that
/// starts with `--` is accepted (harness args often do).
fn flags(words: &[String], name: &str) -> Vec<String> {
    let eq = format!("{name}=");
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if words[i] == name {
            if let Some(v) = words.get(i + 1) {
                out.push(v.clone());
            }
            i += 2;
            continue;
        }
        if let Some(v) = words[i].strip_prefix(&eq) {
            out.push(v.to_owned());
        }
        i += 1;
    }
    out
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
    clones: Vec<(ObjectLocation, CloneRecord)>,
}

fn live_seat(tree: &dyn TreeRead, seat_ref: &str) -> Result<SeatFacts, PlanError> {
    let id = grammar::resolve_seat(tree, seat_ref, Scope::Live)?;
    let loc = layout::locate(tree, &id.to_any())?.ok_or_else(|| PlanError::Invalid(format!("no seat {id}")))?;
    let rec = read_toml::<SeatRecord>(tree, &loc.record_path)?.ok_or_else(|| PlanError::Invalid(format!("no seat {id}")))?;
    if rec.lifecycle == Lifecycle::Retired {
        return Err(PlanError::Invalid(format!("seat {id} is retired; use `seat resurrect`")));
    }
    let clones = layout::list_clones(tree, &loc.folder)?;
    Ok(SeatFacts { loc, rec, clones })
}

struct CloneFacts {
    loc: ObjectLocation,
    rec: CloneRecord,
}

fn live_clone(tree: &dyn TreeRead, clone_ref: &str) -> Result<CloneFacts, PlanError> {
    let id = grammar::resolve_clone(tree, clone_ref, Scope::Live)?;
    let loc = layout::locate(tree, &id.to_any())?.ok_or_else(|| PlanError::Invalid(format!("no clone {id}")))?;
    let rec = read_toml::<CloneRecord>(tree, &loc.record_path)?.ok_or_else(|| PlanError::Invalid(format!("no clone {id}")))?;
    if rec.lifecycle == CloneLifecycle::Retired {
        return Err(PlanError::Invalid(format!("clone {id} is retired")));
    }
    live_seat(tree, rec.seat.as_str())?; // the owning seat must be live too
    Ok(CloneFacts { loc, rec })
}

/// Seat edits made by an active-seat op carry the op as the seat's last attributed op (same rule as the core kinds).
fn attribute(cx: &MutationCx<'_>, seat: &mut SeatRecord) {
    if seat.lifecycle == Lifecycle::Active {
        seat.activation.last_op = Some(cx.op.clone());
    }
}

// ---------------------------------------------------------------------------------------------
// teamspace rename
// ---------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct TeamspaceRenameArgs {
    teamspace: String,
    name: String,
}

struct TeamspaceRename;
impl TeamspaceRename {
    fn paths(tree: &dyn TreeRead, ts: &TsInfo, new_name: &str) -> Result<(RepoPath, RepoPath), PlanError> {
        let mut taken = layout::taken_slugs(tree, &layout::teamspaces_root())?;
        taken.remove(basename(&ts.loc.folder));
        let slug = unique_slug(new_name, ts.rec.id.suffix6(), &taken);
        Ok((ts.loc.folder.clone(), layout::teamspace_dir(&slug)))
    }

    fn facts(tree: &dyn TreeRead, a: &TeamspaceRenameArgs) -> Result<TsInfo, PlanError> {
        if a.name.trim().is_empty() {
            return Err(PlanError::Invalid("teamspace name must not be empty".into()));
        }
        let id = grammar::resolve_teamspace(tree, &a.teamspace, Scope::Live)?;
        let ts = read_ts(tree, &id)?;
        if ts.rec.lifecycle == Lifecycle::Retired {
            return Err(PlanError::Invalid(format!("teamspace {id} is retired; resurrect it first")));
        }
        if ts.rec.name == a.name {
            return Err(PlanError::Invalid(format!("teamspace {id} is already named {:?}", a.name)));
        }
        Ok(ts)
    }
}
impl OrgKind for TeamspaceRename {
    fn kind(&self) -> RequestKind {
        RequestKind::TeamspaceRename
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("teamspace", "rename")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = || PlanError::Usage("teamspace rename <teamspace> <new-name>".into());
        let ts = positional(words, 0).ok_or_else(usage)?;
        let name = positional(words, 1).ok_or_else(usage)?;
        Ok(json!({ "teamspace": ts, "name": name }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, _: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: TeamspaceRenameArgs = parse_args(args)?;
        let ts = Self::facts(cx.tree, &a)?;
        let (from, to) = Self::paths(cx.tree, &ts, &a.name)?;
        let mut effects = vec![PlanEffect::new(
            "teamspace.rename",
            ts.rec.id.clone(),
            json!({ "from": ts.rec.name, "to": a.name, "path_from": from.as_str(), "path_to": to.as_str() }),
        )];
        if ts.rec.lifecycle == Lifecycle::Active {
            effects.push(PlanEffect::new("runtime.rename_workspace", ts.rec.id.clone(), json!({ "name": a.name })));
        }
        for (_, seat) in layout::list_seats(cx.tree, &ts.loc.folder)? {
            if seat.lifecycle != Lifecycle::Retired {
                effects.push(PlanEffect::new(
                    "threads.notify_rename",
                    seat.id.clone(),
                    json!({ "teamspace": ts.rec.id, "from": ts.rec.name, "to": a.name }),
                ));
            }
        }
        let summary = format!("rename teamspace {} to {}", ts.rec.name, a.name);
        Ok(body(effects, vec![rev_of(ts.rec.id.clone(), ts.rec.rev)], summary))
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, _: &Plan) -> Result<Applied, MutationError> {
        let a: TeamspaceRenameArgs = parse_args(args).map_err(mm)?;
        let ts = Self::facts(&cx.tree, &a).map_err(mm)?;
        let (from, to) = Self::paths(&cx.tree, &ts, &a.name).map_err(mm)?;
        let mut rec = ts.rec.clone();
        rec.name_history.push(NameChange {
            old: rec.name.clone(),
            new: a.name.clone(),
            observed_at: cx.now,
            event_at: None,
            source: NameSource::Request,
        });
        rec.name = a.name.clone();
        cx.tree.put_record(ts.loc.record_path.clone(), &mut rec)?;
        if from != to {
            cx.tree.move_dir(&from, &to)?;
        }
        Ok(Applied { summary: format!("rename teamspace {} to {}", ts.rec.name, a.name), action: None })
    }
}

// ---------------------------------------------------------------------------------------------
// seat override
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
struct OverrideSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    harness: Option<Harness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summaries: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    instructions_sections: Option<Vec<InstructionSection>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SeatOverrideArgs {
    seat: String,
    #[serde(default)]
    set: OverrideSet,
    #[serde(default)]
    clear: Vec<String>,
}

const CLEARABLE: &[&str] = &["harness", "model", "args", "summaries", "instructions_sections"];

fn canonical_field(f: &str) -> Option<&'static str> {
    let f = if f == "sections" { "instructions_sections" } else { f };
    CLEARABLE.iter().copied().find(|c| *c == f)
}

/// Apply `clear` then `set` to the seat's overrides. Sections merge by name (a same-named one is replaced).
fn apply_override(rec: &mut SeatRecord, a: &SeatOverrideArgs) -> Result<(), PlanError> {
    let o = &mut rec.overrides;
    for f in &a.clear {
        match canonical_field(f) {
            Some("harness") => o.harness = None,
            Some("model") => o.model = None,
            Some("args") => o.args = None,
            Some("summaries") => o.summaries = None,
            Some("instructions_sections") => o.instructions_sections.clear(),
            _ => return Err(PlanError::Invalid(format!("cannot clear {f:?}; one of {}", CLEARABLE.join(", ")))),
        }
    }
    let s = &a.set;
    if let Some(h) = s.harness {
        o.harness = Some(h);
    }
    if let Some(m) = &s.model {
        o.model = Some(m.clone());
    }
    if let Some(args) = &s.args {
        o.args = Some(args.clone());
    }
    if let Some(b) = s.summaries {
        o.summaries = Some(b);
    }
    for sec in s.instructions_sections.iter().flatten() {
        match o.instructions_sections.iter_mut().find(|x| x.name == sec.name) {
            Some(existing) => existing.body = sec.body.clone(),
            None => o.instructions_sections.push(sec.clone()),
        }
    }
    Ok(())
}

struct SeatOverride;
impl SeatOverride {
    /// The seat after the override, or an error when the override changes nothing.
    fn edited(f: &SeatFacts, a: &SeatOverrideArgs) -> Result<SeatRecord, PlanError> {
        let mut rec = f.rec.clone();
        apply_override(&mut rec, a)?;
        if rec.overrides == f.rec.overrides {
            return Err(PlanError::Invalid(format!("seat {} already has these overrides; nothing to change", f.rec.id)));
        }
        Ok(rec)
    }
}
impl OrgKind for SeatOverride {
    fn kind(&self) -> RequestKind {
        RequestKind::SeatOverride
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("seat", "override")]
    }
    fn parse(&self, words: &[String], caller: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = || {
            PlanError::Usage(
                "seat override <seat> [--harness h] [--model m] [--args \"<a b>\"] [--summaries true|false] \
                 [--section <name>=<file>]… [--clear <field>]…"
                    .into(),
            )
        };
        let seat = positional(words, 0).ok_or_else(usage)?;
        let mut set = OverrideSet::default();
        if let Some(h) = flag(words, "--harness") {
            set.harness =
                Some(serde_json::from_value(json!(h)).map_err(|_| PlanError::Usage(format!("unknown harness {h:?}")))?);
        }
        set.model = flag(words, "--model");
        if let Some(a) = flags(words, "--args").pop() {
            set.args = Some(a.split_whitespace().map(str::to_owned).collect());
        }
        if let Some(s) = flag(words, "--summaries") {
            set.summaries = Some(match s.as_str() {
                "true" => true,
                "false" => false,
                other => return Err(PlanError::Usage(format!("--summaries takes true or false, not {other:?}"))),
            });
        }
        let mut sections = Vec::new();
        for spec in flags(words, "--section") {
            let (name, file) = spec
                .split_once('=')
                .filter(|(n, f)| !n.is_empty() && !f.is_empty())
                .ok_or_else(|| PlanError::Usage(format!("--section takes <name>=<file>, not {spec:?}")))?;
            let path = match &caller.cwd {
                Some(cwd) => cwd.join(file),
                None => std::path::PathBuf::from(file),
            };
            let text = std::fs::read_to_string(&path)
                .map_err(|e| PlanError::Invalid(format!("cannot read section file {}: {e}", path.display())))?;
            sections.push(InstructionSection { name: name.to_owned(), body: text });
        }
        if !sections.is_empty() {
            set.instructions_sections = Some(sections);
        }
        let mut clear = Vec::new();
        for f in flags(words, "--clear") {
            let c = canonical_field(&f)
                .ok_or_else(|| PlanError::Usage(format!("cannot clear {f:?}; one of {}", CLEARABLE.join(", "))))?;
            if !clear.contains(&c.to_owned()) {
                clear.push(c.to_owned());
            }
        }
        let touched = |c: &str| match c {
            "harness" => set.harness.is_some(),
            "model" => set.model.is_some(),
            "args" => set.args.is_some(),
            "summaries" => set.summaries.is_some(),
            _ => set.instructions_sections.is_some(),
        };
        if let Some(c) = clear.iter().find(|c| touched(c)) {
            return Err(PlanError::Usage(format!("{c} is both set and cleared")));
        }
        if set == OverrideSet::default() && clear.is_empty() {
            return Err(usage());
        }
        Ok(serde_json::to_value(SeatOverrideArgs { seat, set, clear }).expect("override args serialize"))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, _: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: SeatOverrideArgs = parse_args(args)?;
        let f = live_seat(cx.tree, &a.seat)?;
        let edited = Self::edited(&f, &a)?;
        let old = resolve_in(cx.tree, &f.rec)?;
        let new = resolve_in(cx.tree, &edited)?;
        let mut effects =
            vec![PlanEffect::new("seat.override", f.rec.id.clone(), json!({ "set": a.set, "clear": a.clear }))];
        if f.rec.lifecycle == Lifecycle::Active {
            let clones: Vec<CloneRecord> = f.clones.iter().map(|(_, c)| c.clone()).collect();
            effects.extend(session_replacements(&f.rec, &clones, &old, &new));
        }
        let summary = format!("override seat {} config", f.rec.name);
        Ok(body(effects, vec![rev_of(f.rec.id.clone(), f.rec.rev)], summary))
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, _: &Plan) -> Result<Applied, MutationError> {
        let a: SeatOverrideArgs = parse_args(args).map_err(mm)?;
        let f = live_seat(&cx.tree, &a.seat).map_err(mm)?;
        let mut rec = Self::edited(&f, &a).map_err(mm)?;
        attribute(cx, &mut rec);
        cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
        Ok(Applied { summary: format!("override seat {} config", f.rec.name), action: None })
    }
}

// ---------------------------------------------------------------------------------------------
// participation join | leave (intent only; threads effects are hg-zmi.11/12)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PScope {
    Seat,
    Clone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParticipationArgs {
    thread: String,
    scope: PScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clone: Option<String>,
}

fn parse_participation(verb: &str, words: &[String], caller: &CallerInfo) -> Result<serde_json::Value, PlanError> {
    let usage = || PlanError::Usage(format!("participation {verb} <thread> --scope seat|clone [--seat s] [--clone c]"));
    let thread = positional(words, 0).ok_or_else(usage)?;
    let scope = match flag(words, "--scope").as_deref() {
        Some("seat") => PScope::Seat,
        Some("clone") => PScope::Clone,
        _ => return Err(usage()),
    };
    let (seat, clone) = (flag(words, "--seat"), flag(words, "--clone"));
    let args = match scope {
        PScope::Seat => {
            if clone.is_some() {
                return Err(PlanError::Usage("--clone only applies to --scope clone".into()));
            }
            let seat = seat.or_else(|| caller.graph_seat.clone()).ok_or_else(usage)?;
            ParticipationArgs { thread, scope, seat: Some(seat), clone: None }
        }
        PScope::Clone => {
            if seat.is_some() {
                return Err(PlanError::Usage("--seat only applies to --scope seat".into()));
            }
            let clone = clone.or_else(|| caller.graph_clone.clone()).ok_or_else(usage)?;
            ParticipationArgs { thread, scope, seat: None, clone: Some(clone) }
        }
    };
    Ok(serde_json::to_value(args).expect("participation args serialize"))
}

fn need<'a>(v: &'a Option<String>, what: &str) -> Result<&'a str, PlanError> {
    v.as_deref().ok_or_else(|| PlanError::Invalid(format!("malformed arguments: missing {what}")))
}

struct ParticipationJoin;
struct ParticipationLeave;

/// What a participation change does, shared by both verbs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Join,
    Leave,
}

fn participation_plan(cx: &PlanCx<'_>, args: &serde_json::Value, dir: Dir) -> Result<PlanBody, PlanError> {
    let a: ParticipationArgs = parse_args(args)?;
    let word = if dir == Dir::Join { "join" } else { "leave" };
    match a.scope {
        PScope::Seat => {
            let f = live_seat(cx.tree, need(&a.seat, "seat")?)?;
            let present = f.rec.participation.seat_wide.contains(&a.thread);
            match (dir, present) {
                (Dir::Join, true) => {
                    return Err(PlanError::Invalid(format!("seat {} already participates in {}", f.rec.id, a.thread)));
                }
                (Dir::Leave, false) => {
                    return Err(PlanError::Invalid(format!("seat {} does not participate in {}", f.rec.id, a.thread)));
                }
                _ => {}
            }
            let detail = json!({ "thread": a.thread, "scope": "seat", "seat": f.rec.id });
            let mut effects = vec![PlanEffect::new(&format!("participation.{word}"), f.rec.id.clone(), detail)];
            if dir == Dir::Leave {
                // The rev the seat has once this op commits: the instruction is `current` until it moves again.
                effects.push(PlanEffect::new(
                    "participation.leave_instruction",
                    f.rec.id.clone(),
                    json!({ "seat": f.rec.id, "rev": f.rec.rev + 1, "thread": a.thread }),
                ));
            }
            let summary = format!("seat {} {word} {}", f.rec.name, a.thread);
            Ok(body(effects, vec![rev_of(f.rec.id.clone(), f.rec.rev)], summary))
        }
        PScope::Clone => {
            let f = live_clone(cx.tree, need(&a.clone, "clone")?)?;
            let opted_out = f.rec.opt_outs.contains(&a.thread);
            match (dir, opted_out) {
                (Dir::Join, false) => {
                    return Err(PlanError::Invalid(format!("clone {} has no opt-out for {}; nothing to join", f.rec.id, a.thread)));
                }
                (Dir::Leave, true) => {
                    return Err(PlanError::Invalid(format!("clone {} already opted out of {}", f.rec.id, a.thread)));
                }
                _ => {}
            }
            let detail = json!({ "thread": a.thread, "scope": "clone", "clone": f.rec.id, "seat": f.rec.seat });
            let effects = vec![PlanEffect::new(&format!("participation.{word}"), f.rec.id.clone(), detail)];
            let summary = format!("clone {} {word} {}", f.rec.name, a.thread);
            Ok(body(effects, vec![rev_of(f.rec.id.clone(), f.rec.rev)], summary))
        }
    }
}

fn participation_mutate(cx: &mut MutationCx<'_>, args: &serde_json::Value, dir: Dir) -> Result<Applied, MutationError> {
    let a: ParticipationArgs = parse_args(args).map_err(mm)?;
    let word = if dir == Dir::Join { "join" } else { "leave" };
    // Re-run the planning checks against the overlay so a raced state change is rejected, not applied twice.
    let at = cx.tree.base().clone();
    let caller = CallerInfo::default();
    let pcx = PlanCx { tree: &cx.tree, at, caller: &caller, now: cx.now, instance: std::path::Path::new("") };
    participation_plan(&pcx, args, dir).map_err(mm)?;
    match a.scope {
        PScope::Seat => {
            let f = live_seat(&cx.tree, need(&a.seat, "seat").map_err(mm)?).map_err(mm)?;
            let mut rec = f.rec.clone();
            match dir {
                Dir::Join => rec.participation.seat_wide.push(a.thread.clone()),
                Dir::Leave => rec.participation.seat_wide.retain(|t| t != &a.thread),
            }
            attribute(cx, &mut rec);
            cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
            Ok(Applied { summary: format!("seat {} {word} {}", f.rec.name, a.thread), action: None })
        }
        PScope::Clone => {
            let f = live_clone(&cx.tree, need(&a.clone, "clone").map_err(mm)?).map_err(mm)?;
            let mut rec = f.rec.clone();
            match dir {
                Dir::Join => rec.opt_outs.retain(|t| t != &a.thread),
                Dir::Leave => rec.opt_outs.push(a.thread.clone()),
            }
            cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
            Ok(Applied { summary: format!("clone {} {word} {}", f.rec.name, a.thread), action: None })
        }
    }
}

impl OrgKind for ParticipationJoin {
    fn kind(&self) -> RequestKind {
        RequestKind::ParticipationJoin
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("participation", "join")]
    }
    fn parse(&self, words: &[String], caller: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        parse_participation("join", words, caller)
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, _: &mut Reserved) -> Result<PlanBody, PlanError> {
        participation_plan(cx, args, Dir::Join)
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, _: &Plan) -> Result<Applied, MutationError> {
        participation_mutate(cx, args, Dir::Join)
    }
}

impl OrgKind for ParticipationLeave {
    fn kind(&self) -> RequestKind {
        RequestKind::ParticipationLeave
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("participation", "leave")]
    }
    fn parse(&self, words: &[String], caller: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        parse_participation("leave", words, caller)
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, _: &mut Reserved) -> Result<PlanBody, PlanError> {
        participation_plan(cx, args, Dir::Leave)
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, _: &Plan) -> Result<Applied, MutationError> {
        participation_mutate(cx, args, Dir::Leave)
    }
}

// ---------------------------------------------------------------------------------------------
// clone rebind
// ---------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct CloneRebindArgs {
    clone: String,
    pane: String,
}

struct CloneRebind;
impl OrgKind for CloneRebind {
    fn kind(&self) -> RequestKind {
        RequestKind::CloneRebind
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("clone", "rebind")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = || PlanError::Usage("clone rebind <clone> --pane <pane>".into());
        let clone = positional(words, 0).ok_or_else(usage)?;
        let pane = flag(words, "--pane").filter(|p| !p.is_empty()).ok_or_else(usage)?;
        Ok(json!({ "clone": clone, "pane": pane }))
    }
    fn plan(&self, cx: &PlanCx<'_>, args: &serde_json::Value, _: &mut Reserved) -> Result<PlanBody, PlanError> {
        let a: CloneRebindArgs = parse_args(args)?;
        let f = live_clone(cx.tree, &a.clone)?;
        let from = f.rec.runtime.bound.as_ref().and_then(|b| b.pane_id.clone());
        let mut warnings = Vec::new();
        let mut repair_required = None;
        for (_, other) in layout::all_clones(cx.tree)? {
            let bound_here = other.runtime.bound.as_ref().and_then(|b| b.pane_id.as_ref()).is_some_and(|p| p.0 == a.pane);
            if other.id != f.rec.id && other.lifecycle == CloneLifecycle::Active && bound_here {
                let why = format!("pane {} is already bound to clone {} ({}); rebind that clone first", a.pane, other.id, other.name);
                warnings.push(why.clone());
                repair_required = Some(why);
            }
        }
        if f.rec.reload_required {
            warnings.push(format!("clone {} was flagged reload_required; rebinding clears the flag", f.rec.id));
        }
        let effects = vec![
            PlanEffect::new(
                "clone.rebind",
                f.rec.id.clone(),
                json!({ "pane": a.pane, "from": from.map(|p| p.0), "seat": f.rec.seat }),
            ),
            PlanEffect::new("runtime.stamp_token", f.rec.id.clone(), json!({ "clone": f.rec.id, "pane": a.pane })),
        ];
        Ok(PlanBody {
            effects,
            relied_on: vec![rev_of(f.rec.id.clone(), f.rec.rev)],
            warnings,
            repair_required,
            summary: format!("rebind clone {} to pane {}", f.rec.name, a.pane),
        })
    }
    fn mutate(&self, cx: &mut MutationCx<'_>, args: &serde_json::Value, _: &Plan) -> Result<Applied, MutationError> {
        let a: CloneRebindArgs = parse_args(args).map_err(mm)?;
        let f = live_clone(&cx.tree, &a.clone).map_err(mm)?;
        let mut rec = f.rec.clone();
        let pane = HerdrPaneId(a.pane.clone());
        let old = rec.runtime.bound.clone().unwrap_or_default();
        let same_pane = old.pane_id.as_ref() == Some(&pane);
        rec.runtime.bound = Some(Binding {
            token: Some(graph_token(&rec.id.to_any())),
            workspace_id: if same_pane { old.workspace_id.clone() } else { None },
            tab_id: if same_pane { old.tab_id.clone() } else { None },
            pane_id: Some(pane),
            // The terminal id the clone had may belong to a server that restarted since; the observer fills in
            // the pane's own once it matches the adoption.
            terminal_id: None,
            incarnation: old.incarnation,
        });
        rec.runtime.availability = Availability::Present;
        rec.runtime.observed_at = Some(cx.now);
        rec.reload_required = false;
        cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
        // The re-stamp effect is attributed to this op, so its id differs from the original stamp's (hg-zmi.50).
        let seat = live_seat(&cx.tree, rec.seat.as_str()).map_err(mm)?;
        let mut seat_rec = seat.rec;
        attribute(cx, &mut seat_rec);
        cx.tree.put_record(seat.loc.record_path.clone(), &mut seat_rec)?;
        Ok(Applied { summary: format!("rebind clone {} to pane {}", f.rec.name, a.pane), action: None })
    }
}

/// Test fixture shared with `ops` and `reminders` tests: a temp instance with writer, all kinds and ops mutations.
#[cfg(test)]
pub(crate) mod testkit {
    use super::super::commands::{PlanDeps, admit_apply, create_plan};
    use super::super::core_kinds::register_core_kinds;
    use super::super::store::PlanStore;
    use super::super::types::StoredPlan;
    use super::*;
    use crate::journal::{Journal, OpRow};
    use crate::model::{CloneId, PlanId, SeatId};
    use crate::model::change::{ChangeRequest, Requester};
    use crate::model::common::Occupant;
    use crate::model::operation::OpState;
    use crate::ports::clock::ManualClock;
    use crate::ports::store::Store;
    use crate::store::GitStore;
    use crate::store::init::init_instance;
    use crate::store::tree::CommitView;
    use crate::writer::{Mutation, MutationRegistry, WriterConfig, WriterCore};
    use chrono::TimeZone;

    pub fn t0() -> crate::model::Timestamp {
        chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
    }

    pub struct Fx {
        pub _tmp: tempfile::TempDir,
        pub deps: PlanDeps,
        pub w: Arc<WriterCore>,
        pub store: Arc<GitStore>,
        pub clock: Arc<ManualClock>,
    }

    /// Test-only writer mutation: gives a clone an occupant and/or a pane binding (`bookkeeping.test_occupy`).
    struct TestOccupy;
    impl Mutation for TestOccupy {
        fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
            let id: CloneId = cx.request.args["clone"].as_str().unwrap().parse().unwrap();
            let loc = layout::locate(&cx.tree, &id.to_any())?.unwrap();
            let mut rec: CloneRecord = read_toml(&cx.tree, &loc.record_path)?.unwrap();
            if cx.request.args["occupy"].as_bool().unwrap_or(false) {
                rec.occupant = Some(Occupant {
                    native_session: crate::model::NsId::new(),
                    harness: Harness::Claude,
                    since: cx.now,
                });
            }
            if let Some(p) = cx.request.args["pane"].as_str() {
                rec.runtime.bound = Some(Binding { pane_id: Some(HerdrPaneId(p.into())), ..Default::default() });
                rec.runtime.availability = Availability::Present;
            }
            if cx.request.args["reload_required"].as_bool().unwrap_or(false) {
                rec.reload_required = true;
            }
            cx.tree.put_record(loc.record_path, &mut rec)?;
            Ok(Applied { summary: "test occupy".into(), action: None })
        }
    }

    /// Test-only writer mutation: gives a seat a managed channel (`bookkeeping.test_channel`).
    struct TestChannel;
    impl Mutation for TestChannel {
        fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
            let id: SeatId = cx.request.args["seat"].as_str().unwrap().parse().unwrap();
            let loc = layout::locate(&cx.tree, &id.to_any())?.unwrap();
            let mut rec: SeatRecord = read_toml(&cx.tree, &loc.record_path)?.unwrap();
            rec.channel.thread_id = Some(cx.request.args["thread"].as_str().unwrap().to_owned());
            cx.tree.put_record(loc.record_path, &mut rec)?;
            Ok(Applied { summary: "test channel".into(), action: None })
        }
    }

    pub fn set_channel(fx: &Fx, seat: &SeatId, thread: &str) {
        use crate::ports::writer::Writer;
        let req = ChangeRequest {
            kind: RequestKind::Bookkeeping,
            args: json!({ "sub": "test_channel", "seat": seat, "thread": thread }),
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
            confirmed: None,
        };
        let op = fx.w.admit(req).unwrap();
        fx.w.drain().unwrap();
        assert_eq!(fx.w.journal().get(&op).unwrap().unwrap().state, OpState::Committed);
    }

    pub fn fx() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("inst");
        init_instance(&root).unwrap();
        let mut kinds = KindRegistry::default();
        register_core_kinds(&mut kinds);
        register_kinds(&mut kinds);
        let kinds = Arc::new(kinds);
        let plans = Arc::new(PlanStore::new(root.join(".graph-local/plans")));
        let mut reg = MutationRegistry::default();
        kinds.register_mutations(&mut reg, plans.clone());
        super::super::ops::register_mutations(&mut reg);
        reg.register("bookkeeping.test_occupy", Arc::new(TestOccupy));
        reg.register("bookkeeping.test_channel", Arc::new(TestChannel));
        let store = Arc::new(GitStore::open(&root).unwrap());
        let journal = Arc::new(Journal::open(&Journal::path_in(&root)).unwrap());
        let clock = Arc::new(ManualClock::new(t0()));
        let w = WriterCore::new(store.clone(), journal, Arc::new(reg), clock.clone(), WriterConfig::default());
        let deps =
            PlanDeps { kinds, plans, store: store.clone(), writer: w.clone(), clock: clock.clone(), instance: root.clone() };
        Fx { _tmp: tmp, deps, w, store, clock }
    }

    impl Fx {
        pub fn clock_now(&self) -> crate::model::Timestamp {
            use crate::ports::clock::Clock;
            self.clock.now()
        }
    }

    pub fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    pub fn plan_as(fx: &Fx, caller: &CallerInfo, change: &str) -> StoredPlan {
        let v = create_plan(&fx.deps, caller, words(change)).unwrap_or_else(|e| panic!("{change}: {}", e.message));
        let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
        fx.deps.plans.get(&id).unwrap().unwrap()
    }

    pub fn plan(fx: &Fx, change: &str) -> StoredPlan {
        plan_as(fx, &CallerInfo::default(), change)
    }

    pub fn apply_plan(fx: &Fx, sp: &StoredPlan) -> OpRow {
        let op = admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay")
            .unwrap_or_else(|e| panic!("admit: {}", e.message));
        fx.w.drain().unwrap();
        fx.w.journal().get(&op).unwrap().unwrap()
    }

    /// Plan and apply one change; it must commit.
    pub fn commit(fx: &Fx, change: &str) -> OpRow {
        let row = apply_plan(fx, &plan(fx, change));
        assert_eq!(row.state, OpState::Committed, "{change}: {:?}", row.rejection);
        row
    }

    /// An op the writer rejects (`unknown_plan`): requested by `requester`, in state `rejected`.
    pub fn rejected_op(fx: &Fx, requester: Requester) -> OpRow {
        let req = ChangeRequest {
            kind: RequestKind::SeatRetire,
            args: json!({ "seat": "nobody" }),
            relied_on: vec![],
            requester,
            supersedes: None,
            confirmed: None,
        };
        use crate::ports::writer::Writer;
        let op = fx.w.admit(req).unwrap();
        fx.w.drain().unwrap();
        let row = fx.w.journal().get(&op).unwrap().unwrap();
        assert_eq!(row.state, OpState::Rejected);
        row
    }

    pub fn occupy(fx: &Fx, clone: &CloneId, occupy: bool, pane: Option<&str>) {
        use crate::ports::writer::Writer;
        let req = ChangeRequest {
            kind: RequestKind::Bookkeeping,
            args: json!({ "sub": "test_occupy", "clone": clone, "occupy": occupy, "pane": pane,
                          "reload_required": pane.is_some() && !occupy }),
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
            confirmed: None,
        };
        let op = fx.w.admit(req).unwrap();
        fx.w.drain().unwrap();
        assert_eq!(fx.w.journal().get(&op).unwrap().unwrap().state, OpState::Committed);
    }

    pub fn view(fx: &Fx) -> CommitView<'_> {
        CommitView { store: &*fx.store, at: fx.store.head().unwrap() }
    }

    pub fn seat(fx: &Fx, name: &str) -> SeatRecord {
        let mut found: Vec<_> =
            layout::all_seats(&view(fx)).unwrap().into_iter().filter(|(_, s)| s.name == name).collect();
        assert_eq!(found.len(), 1, "seat {name}");
        found.remove(0).1
    }

    pub fn clones_of(fx: &Fx, seat_id: &SeatId) -> Vec<CloneRecord> {
        layout::all_clones(&view(fx)).unwrap().into_iter().map(|(_, c)| c).filter(|c| &c.seat == seat_id).collect()
    }

    pub fn requester_of(seat: &SeatRecord) -> Requester {
        Requester { teamspace: Some(seat.teamspace.clone()), seat: Some(seat.id.clone()), ..Default::default() }
    }

    pub fn head_rev_of(fx: &Fx, id: &AnyId) -> u64 {
        let head = fx.store.head().unwrap();
        let loc = fx.store.locate(&head, id).unwrap().unwrap();
        let t: toml::Table = crate::ports::store::read_record(&*fx.store, &head, &loc.record_path).unwrap().unwrap();
        t["rev"].as_integer().unwrap() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::model::operation::OpState;

    fn kinds_of(sp: &super::super::types::StoredPlan) -> Vec<String> {
        sp.plan.effects.iter().map(|e| e.kind.clone()).collect()
    }

    fn ts_named(fx: &Fx, name: &str) -> TeamspaceRecord {
        let mut v: Vec<_> =
            layout::list_teamspaces(&view(fx)).unwrap().into_iter().filter(|(_, t)| t.name == name).collect();
        assert_eq!(v.len(), 1, "teamspace {name}");
        v.remove(0).1
    }

    #[test]
    fn teamspace_rename_moves_folder_and_records_history() {
        let fx = fx();
        commit(&fx, "teamspace create alpha --active");
        commit(&fx, "seat create one --teamspace alpha --active");
        let before = ts_named(&fx, "alpha");
        let loc_before = layout::locate(&view(&fx), &before.id.to_any()).unwrap().unwrap();
        let sp = plan(&fx, "teamspace rename alpha beta");
        let ks = kinds_of(&sp);
        assert!(ks.contains(&"teamspace.rename".to_owned()), "{ks:?}");
        assert!(ks.contains(&"runtime.rename_workspace".to_owned()), "active teamspace renames its workspace: {ks:?}");
        assert_eq!(ks.iter().filter(|k| *k == "threads.notify_rename").count(), 1, "one per seat: {ks:?}");
        assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
        let after = ts_named(&fx, "beta");
        assert_eq!(after.id, before.id);
        assert_eq!(after.name_history.len(), 1);
        let h = &after.name_history[0];
        assert_eq!((h.old.as_str(), h.new.as_str(), h.source), ("alpha", "beta", NameSource::Request));
        let loc_after = layout::locate(&view(&fx), &after.id.to_any()).unwrap().unwrap();
        assert_ne!(loc_before.folder, loc_after.folder, "folder moved");
        assert!(loc_after.folder.as_str().contains("beta"), "{}", loc_after.folder.as_str());
        assert_eq!(seat(&fx, "one").teamspace, after.id, "seat travels with its teamspace folder");
    }

    #[test]
    fn teamspace_rename_to_same_name_is_refused() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        let err = create_plan_err(&fx, "teamspace rename alpha alpha");
        assert!(err.contains("already named"), "{err}");
    }

    fn create_plan_err(fx: &Fx, change: &str) -> String {
        super::super::commands::create_plan(&fx.deps, &CallerInfo::default(), words(change)).unwrap_err().message
    }

    #[test]
    fn seat_override_sets_and_clears_fields() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("rules.md"), "be brief").unwrap();
        let caller = CallerInfo { cwd: Some(dir.path().into()), ..Default::default() };
        let sp = plan_as(
            &fx,
            &caller,
            "seat override one --harness codex --model m1 --args=--fast --summaries false --section style=rules.md",
        );
        assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
        let o = seat(&fx, "one").overrides;
        assert_eq!(o.harness, Some(Harness::Codex));
        assert_eq!(o.model.as_deref(), Some("m1"));
        assert_eq!(o.args, Some(vec!["--fast".to_owned()]));
        assert_eq!(o.summaries, Some(false));
        assert_eq!(o.instructions_sections, vec![InstructionSection { name: "style".into(), body: "be brief".into() }]);

        commit(&fx, "seat override one --clear model --clear args --clear sections --clear summaries");
        let o = seat(&fx, "one").overrides;
        assert_eq!((o.model, o.args, o.summaries), (None, None, None));
        assert!(o.instructions_sections.is_empty());
        assert_eq!(o.harness, Some(Harness::Codex), "untouched field stays");
    }

    #[test]
    fn seat_override_rejects_set_and_clear_of_one_field_and_no_ops() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha");
        let e = create_plan_err(&fx, "seat override one --model a --clear model");
        assert!(e.contains("both set and cleared"), "{e}");
        let e = create_plan_err(&fx, "seat override one");
        assert!(e.contains("seat override <seat>"), "{e}");
        commit(&fx, "seat override one --model a");
        let e = create_plan_err(&fx, "seat override one --model a");
        assert!(e.contains("nothing to change"), "{e}");
    }

    #[test]
    fn seat_override_model_change_plans_session_replacement() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha --active");
        let s = seat(&fx, "one");
        let clone = clones_of(&fx, &s.id).remove(0);
        // No occupant yet: nothing to replace.
        assert_eq!(kinds_of(&plan(&fx, "seat override one --model fancy")).iter().filter(|k| *k == "session.replace").count(), 0);
        occupy(&fx, &clone.id, true, None);
        let sp = plan(&fx, "seat override one --model fancy");
        let repl: Vec<_> = sp.plan.effects.iter().filter(|e| e.kind == "session.replace").collect();
        assert_eq!(repl.len(), 1, "{:?}", kinds_of(&sp));
        assert_eq!(repl[0].object, clone.id.to_any());
        assert_eq!(repl[0].detail["to"]["model"], "fancy");
        // A summaries-only override does not change the run shape.
        let sp = plan(&fx, "seat override one --summaries false");
        assert_eq!(kinds_of(&sp), vec!["seat.override"]);
    }

    #[test]
    fn seat_override_summaries_false() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha");
        assert!(crate::model::effective::resolve_in(&view(&fx), &seat(&fx, "one")).unwrap().summaries);
        commit(&fx, "seat override one --summaries false");
        let s = seat(&fx, "one");
        assert!(!crate::model::effective::resolve_in(&view(&fx), &s).unwrap().summaries);
        commit(&fx, "seat override one --clear summaries");
        assert!(crate::model::effective::resolve_in(&view(&fx), &seat(&fx, "one")).unwrap().summaries);
    }

    #[test]
    fn participation_join_seat_scope_updates_intent() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha");
        let sp = plan(&fx, "participation join th_general --scope seat --seat one");
        assert_eq!(kinds_of(&sp), vec!["participation.join"]);
        assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
        assert_eq!(seat(&fx, "one").participation.seat_wide, vec!["th_general".to_owned()]);
        let e = create_plan_err(&fx, "participation join th_general --scope seat --seat one");
        assert!(e.contains("already participates"), "{e}");
    }

    #[test]
    fn participation_leave_seat_scope_plans_leave_instruction() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha --active");
        commit(&fx, "participation join th_general --scope seat --seat one");
        let s = seat(&fx, "one");
        let sp = plan(&fx, "participation leave th_general --scope seat --seat one");
        let instr = sp.plan.effects.iter().find(|e| e.kind == "participation.leave_instruction").expect("instruction effect");
        assert_eq!(instr.detail["seat"], json!(s.id));
        assert_eq!(instr.detail["rev"], json!(s.rev + 1), "rev the seat has after the op commits");
        assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
        let after = seat(&fx, "one");
        assert!(after.participation.seat_wide.is_empty());
        assert_eq!(after.rev, s.rev + 1, "the planned rev is the committed rev");
        let e = create_plan_err(&fx, "participation leave th_general --scope seat --seat one");
        assert!(e.contains("does not participate"), "{e}");
    }

    #[test]
    fn participation_clone_scope_updates_opt_outs() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha --active");
        let clone = clones_of(&fx, &seat(&fx, "one").id).remove(0);
        let cid = clone.id.to_string();
        let sp = plan(&fx, &format!("participation leave th_x --scope clone --clone {cid}"));
        assert_eq!(kinds_of(&sp), vec!["participation.leave"], "clone scope has no leave instruction");
        assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
        assert_eq!(clones_of(&fx, &clone.seat)[0].opt_outs, vec!["th_x".to_owned()]);
        commit(&fx, &format!("participation join th_x --scope clone --clone {cid}"));
        assert!(clones_of(&fx, &clone.seat)[0].opt_outs.is_empty());
        let e = create_plan_err(&fx, &format!("participation join th_x --scope clone --clone {cid}"));
        assert!(e.contains("no opt-out"), "{e}");
    }

    #[test]
    fn participation_defaults_to_caller_seat_and_validates_scope() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha");
        let s = seat(&fx, "one");
        let caller = CallerInfo { graph_seat: Some(s.id.to_string()), ..Default::default() };
        let sp = plan_as(&fx, &caller, "participation join th_a --scope seat");
        assert_eq!(apply_plan(&fx, &sp).state, OpState::Committed);
        assert_eq!(seat(&fx, "one").participation.seat_wide, vec!["th_a".to_owned()]);
        assert!(create_plan_err(&fx, "participation join th_a --scope seat").contains("--scope"));
        assert!(create_plan_err(&fx, "participation join th_a --scope team").contains("--scope"));
        assert!(create_plan_err(&fx, "participation join th_a --scope seat --seat one --clone c").contains("only applies"));
    }

    #[test]
    fn rebind_binds_clone_to_pane_and_clears_reload_required() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha --active");
        let clone = clones_of(&fx, &seat(&fx, "one").id).remove(0);
        // moved clone: bound to an old pane, flagged reload_required
        occupy(&fx, &clone.id, false, Some("p_old"));
        assert!(clones_of(&fx, &clone.seat)[0].reload_required);
        let sp = plan(&fx, &format!("clone rebind {} --pane p_new", clone.id));
        assert_eq!(kinds_of(&sp), vec!["clone.rebind", "runtime.stamp_token"]);
        assert_eq!(sp.plan.effects[0].detail["from"], "p_old");
        assert_eq!(sp.plan.effects[1].detail["pane"], "p_new");
        assert!(sp.plan.repair_required.is_none());
        let row = apply_plan(&fx, &sp);
        assert_eq!(row.state, OpState::Committed);
        let c = clones_of(&fx, &clone.seat).remove(0);
        let b = c.runtime.bound.expect("bound");
        assert_eq!(b.pane_id, Some(HerdrPaneId("p_new".into())));
        assert_eq!(b.token, Some(graph_token(&c.id.to_any())));
        assert!(!c.reload_required);
        assert_eq!(c.runtime.availability, Availability::Present);
        assert_eq!(seat(&fx, "one").activation.last_op, Some(row.op.clone()), "re-stamp is attributed to the rebind op");
    }

    #[test]
    fn rebind_conflicting_bound_pane_is_repair_required() {
        let fx = fx();
        commit(&fx, "teamspace create alpha");
        commit(&fx, "seat create one --teamspace alpha --active");
        commit(&fx, "seat create two --teamspace alpha --active");
        let c1 = clones_of(&fx, &seat(&fx, "one").id).remove(0);
        let c2 = clones_of(&fx, &seat(&fx, "two").id).remove(0);
        occupy(&fx, &c2.id, false, Some("p_taken"));
        let sp = plan(&fx, &format!("clone rebind {} --pane p_taken", c1.id));
        let why = sp.plan.repair_required.clone().expect("conflict refuses");
        assert!(why.contains(c2.id.as_str()), "plan names the clone holding the pane: {why}");
        assert!(sp.plan.warnings.iter().any(|w| w.contains(c2.id.as_str())));
        // apply is refused: the admission gate refuses a plan needing repair
        let err = super::super::commands::admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay")
            .unwrap_err();
        assert!(err.message.contains("repair"), "{}", err.message);
        assert!(clones_of(&fx, &c1.seat)[0].runtime.bound.is_none(), "nothing changed");
    }
}
