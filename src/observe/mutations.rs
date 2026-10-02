//! Observed mutation kinds (spec §4.3, §2.2): facts the observer saw. They carry no `relied_on`
//! preconditions; if one is rejected the next diff re-derives it. Keys: `observed.cascade`,
//! `observed.rename`, `observed.occupancy`, `observed.move`, `observed.availability`.
use super::classify::CascadeRule;
use super::sessions::SessionCapture;
use crate::model::action::ActionKind;
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::clone::CloneRecord;
use crate::model::common::{
    Availability, Binding, CloneLifecycle, Lifecycle, NameChange, NameSource, Occupant, RetireMechanism, Retirement,
};
use crate::model::native_session::{NativeSession, SessionEndReason};
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{ActionId, AnyId, CloneId, IdKind, NsId, SeatId, Timestamp};
use crate::plan::core_kinds::{Acc, absent, basename, do_retire_seat, mm, read_seat, read_ts, seat_slug_for, write_action};
use crate::ports::store::RepoPath;
use crate::store::slug::unique_slug;
use crate::store::{Record, layout};
use crate::writer::{Applied, Mutation, MutationCx, MutationError, MutationRegistry, Reject};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::sync::Arc;

pub const CASCADE_KEY: &str = "observed.cascade";
pub const RENAME_KEY: &str = "observed.rename";
pub const OCCUPANCY_KEY: &str = "observed.occupancy";
pub const MOVE_KEY: &str = "observed.move";
pub const AVAILABILITY_KEY: &str = "observed.availability";

pub fn register_mutations(reg: &mut MutationRegistry) {
    reg.register(CASCADE_KEY, Arc::new(CascadeMutation));
    reg.register(RENAME_KEY, Arc::new(RenameMutation));
    reg.register(OCCUPANCY_KEY, Arc::new(OccupancyMutation));
    reg.register(MOVE_KEY, Arc::new(MoveMutation));
    reg.register(AVAILABILITY_KEY, Arc::new(AvailabilityMutation));
}

// ---------------------------------------------------------------------------------------------
// request builders (also used by hg-zmi.13's `session-report` for `observed.occupancy`)
// ---------------------------------------------------------------------------------------------

fn observed(args: serde_json::Value) -> ChangeRequest {
    ChangeRequest { kind: RequestKind::Observed, args, relied_on: vec![], requester: Requester::default(), supersedes: None, confirmed: None }
}

pub fn cascade_request(rule: CascadeRule, retire: &[AnyId], at: Timestamp) -> ChangeRequest {
    observed(json!({ "sub": "cascade", "rule": rule.as_str(), "retire": retire, "at": at }))
}

pub fn rename_request(object: &AnyId, new: &str, observed_at: Timestamp, event_at: Option<Timestamp>) -> ChangeRequest {
    observed(json!({ "sub": "rename", "object": object, "new": new, "observed_at": observed_at, "event_at": event_at }))
}

/// End the clone's current occupancy and/or start a new one from `start`.
pub fn occupancy_request(
    clone: &CloneId,
    end: Option<SessionEndReason>,
    start: Option<&SessionCapture>,
    at: Timestamp,
) -> ChangeRequest {
    observed(json!({
        "sub": "occupancy", "clone": clone, "at": at,
        "end": end.map(|reason| json!({ "reason": reason, "at": at })),
        "start": start,
    }))
}

pub fn move_request(clone: Option<&CloneId>, binding: Option<&Binding>, seat_moved_out: Option<&SeatId>) -> ChangeRequest {
    observed(json!({ "sub": "move", "clone": clone, "binding": binding, "seat_moved_out": seat_moved_out }))
}

pub fn availability_request(object: &AnyId, availability: Availability) -> ChangeRequest {
    observed(json!({ "sub": "availability", "object": object, "availability": availability }))
}

// ---------------------------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------------------------

fn parse<T: DeserializeOwned>(cx: &MutationCx<'_>) -> Result<T, MutationError> {
    serde_json::from_value(cx.request.args.clone()).map_err(|e| MutationError::Bug(e.to_string()))
}

fn gone(object: &AnyId) -> MutationError {
    MutationError::Reject(Reject {
        reason: "object_missing".into(),
        explanation: format!("{object} does not exist at the committed head"),
        current_revs: vec![],
    })
}

fn edit<R: Record>(
    cx: &mut MutationCx<'_>,
    object: &AnyId,
    f: impl FnOnce(&mut R, &MutationCx<'_>) -> Result<(), MutationError>,
) -> Result<(), MutationError> {
    let loc = cx.tree.locate(object)?.ok_or_else(|| gone(object))?;
    let mut rec: R = cx.tree.read_record(&loc.record_path)?.ok_or_else(|| gone(object))?;
    f(&mut rec, cx)?;
    cx.tree.put_record(loc.record_path, &mut rec)?;
    Ok(())
}

/// Close the clone's current occupancy; false when it had none.
fn end_occupant(rec: &mut CloneRecord, reason: SessionEndReason, at: Timestamp) -> bool {
    let Some(occ) = rec.occupant.take() else { return false };
    if let Some(ns) = rec.sessions.iter_mut().find(|s| s.id == occ.native_session)
        && ns.ended.is_none()
    {
        ns.ended = Some(at);
        ns.end_reason = Some(reason);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// observed.cascade
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct CascadeArgs {
    rule: String,
    retire: Vec<AnyId>,
    #[serde(default)]
    at: Option<Timestamp>,
}

struct CascadeMutation;

impl CascadeMutation {
    fn retire_seat(
        cx: &mut MutationCx<'_>,
        seat: &SeatId,
        act: &ActionId,
        mech: RetireMechanism,
        at: Timestamp,
        exclusions: bool,
        acc: &mut Acc,
    ) -> Result<(), MutationError> {
        let f = read_seat(&cx.tree, seat).map_err(mm)?;
        if f.rec.lifecycle == Lifecycle::Retired {
            acc.already.push(seat.to_any());
            return Ok(());
        }
        // The panes are gone: occupancy ends and the runtime is absent, before the shared retire path runs.
        for (loc, c) in &f.clones {
            if c.lifecycle == CloneLifecycle::Retired {
                continue;
            }
            let mut c = c.clone();
            end_occupant(&mut c, SessionEndReason::PaneClosed, at);
            c.runtime = absent();
            cx.tree.put_record(loc.record_path.clone(), &mut c)?;
        }
        let mut rec = f.rec.clone();
        rec.runtime = absent();
        cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
        do_retire_seat(cx, seat, act, mech, acc, exclusions)
    }

    fn retire_teamspace(
        cx: &mut MutationCx<'_>,
        id: &crate::model::TeamspaceId,
        act: &ActionId,
        mech: RetireMechanism,
        at: Timestamp,
        acc: &mut Acc,
    ) -> Result<(), MutationError> {
        let ts = read_ts(&cx.tree, id).map_err(mm)?;
        if ts.rec.lifecycle == Lifecycle::Retired {
            acc.already.push(id.to_any());
            return Ok(());
        }
        for (_, seat) in layout::list_seats(&cx.tree, &ts.loc.folder)? {
            Self::retire_seat(cx, &seat.id, act, mech, at, false, acc)?;
        }
        let before = if ts.rec.lifecycle == Lifecycle::Active { "active" } else { "dormant" };
        let mut rec = ts.rec.clone();
        rec.lifecycle = Lifecycle::Retired;
        rec.retired = Some(Retirement { op: cx.op.clone(), action: Some(act.clone()), at, mechanism: mech });
        rec.runtime = absent();
        cx.tree.put_record(ts.loc.record_path.clone(), &mut rec)?;
        acc.retired.insert(0, id.to_any());
        acc.changed(id.to_any(), before, "retired");
        cx.tree.move_dir(&ts.loc.folder, &layout::archived_teamspace_dir(basename(&ts.loc.folder), id))?;
        Ok(())
    }

    fn retire_clone(
        cx: &mut MutationCx<'_>,
        id: &CloneId,
        act: &ActionId,
        mech: RetireMechanism,
        at: Timestamp,
        acc: &mut Acc,
    ) -> Result<(), MutationError> {
        let any = id.to_any();
        let loc = cx.tree.locate(&any)?.ok_or_else(|| gone(&any))?;
        let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.ok_or_else(|| gone(&any))?;
        if rec.lifecycle == CloneLifecycle::Retired {
            acc.already.push(any);
            return Ok(());
        }
        end_occupant(&mut rec, SessionEndReason::PaneClosed, at);
        rec.runtime = absent();
        rec.lifecycle = CloneLifecycle::Retired;
        rec.retired = Some(Retirement { op: cx.op.clone(), action: Some(act.clone()), at, mechanism: mech });
        cx.tree.put_record(loc.record_path, &mut rec)?;
        acc.retired.push(any.clone());
        acc.changed(any, "active", "retired");
        Ok(())
    }
}

impl Mutation for CascadeMutation {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: CascadeArgs = parse(cx)?;
        let at = a.at.unwrap_or(cx.now);
        let (mech, exclusions) = match a.rule.as_str() {
            "pane" => (RetireMechanism::ObservedPaneClose, true),
            "tab" => (RetireMechanism::ObservedTabClose, true),
            "workspace" => (RetireMechanism::ObservedWorkspaceClose, false),
            other => return Err(MutationError::Bug(format!("unknown cascade rule {other:?}"))),
        };
        let act = ActionId::new();
        let mut acc = Acc::default();
        let mut teamspaces = Vec::new();
        let mut seats = Vec::new();
        let mut clones = Vec::new();
        for id in &a.retire {
            match id.kind() {
                IdKind::Teamspace => teamspaces.extend(crate::model::TeamspaceId::parse(id.as_str())),
                IdKind::Seat => seats.extend(SeatId::parse(id.as_str())),
                IdKind::Clone => clones.extend(CloneId::parse(id.as_str())),
                other => return Err(MutationError::Bug(format!("cannot retire a {other:?}"))),
            }
        }
        let handled = |acc: &Acc, id: &AnyId| acc.retired.contains(id);
        for ts in &teamspaces {
            Self::retire_teamspace(cx, ts, &act, mech, at, &mut acc)?;
        }
        for seat in &seats {
            if !handled(&acc, &seat.to_any()) {
                Self::retire_seat(cx, seat, &act, mech, at, exclusions, &mut acc)?;
            }
        }
        for clone in &clones {
            if !handled(&acc, &clone.to_any()) {
                Self::retire_clone(cx, clone, &act, mech, at, &mut acc)?;
            }
        }
        let (n, already) = (acc.retired.len(), acc.already.len());
        let mut comp = toml::Table::new();
        comp.insert("rule".into(), toml::Value::String(a.rule.clone()));
        write_action(cx, &act, ActionKind::ClosureCascade, acc, comp)?;
        Ok(Applied {
            summary: format!("observed {} closure retired {n} object(s), {already} already retired", a.rule),
            action: Some(act),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// observed.rename
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct RenameArgs {
    object: AnyId,
    new: String,
    observed_at: Timestamp,
    #[serde(default)]
    event_at: Option<Timestamp>,
}

struct RenameMutation;

fn name_change(old: &str, a: &RenameArgs) -> NameChange {
    NameChange {
        old: old.to_owned(),
        new: a.new.clone(),
        observed_at: a.observed_at,
        event_at: a.event_at,
        source: NameSource::Observed,
    }
}

impl Mutation for RenameMutation {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: RenameArgs = parse(cx)?;
        let noop = |what: &str| Applied { summary: format!("rename {what}: nothing to record"), action: None };
        match a.object.kind() {
            IdKind::Teamspace => {
                let id = crate::model::TeamspaceId::parse(a.object.as_str()).map_err(|e| MutationError::Bug(e.to_string()))?;
                let ts = read_ts(&cx.tree, &id).map_err(mm)?;
                if ts.rec.lifecycle == Lifecycle::Retired || ts.rec.name == a.new {
                    return Ok(noop(a.object.as_str()));
                }
                let mut taken = layout::taken_slugs(&cx.tree, &layout::teamspaces_root())?;
                taken.remove(basename(&ts.loc.folder));
                let to = layout::teamspace_dir(&unique_slug(&a.new, id.suffix6(), &taken));
                let mut rec: TeamspaceRecord = ts.rec.clone();
                rec.name_history.push(name_change(&ts.rec.name, &a));
                rec.name = a.new.clone();
                cx.tree.put_record(ts.loc.record_path.clone(), &mut rec)?;
                if ts.loc.folder != to {
                    cx.tree.move_dir(&ts.loc.folder, &to)?;
                }
                Ok(Applied { summary: format!("observed rename of teamspace {} to {}", ts.rec.name, a.new), action: None })
            }
            IdKind::Seat => {
                let id = SeatId::parse(a.object.as_str()).map_err(|e| MutationError::Bug(e.to_string()))?;
                let f = read_seat(&cx.tree, &id).map_err(mm)?;
                if f.rec.lifecycle == Lifecycle::Retired || f.rec.name == a.new {
                    return Ok(noop(a.object.as_str()));
                }
                let own = basename(&f.loc.folder).to_owned();
                let slug = seat_slug_for(&cx.tree, &f.ts.loc.folder, &a.new, &id, Some(&own)).map_err(mm)?;
                let to = layout::seat_dir(&f.ts.loc.folder, &slug);
                let mut rec: SeatRecord = f.rec.clone();
                rec.name_history.push(name_change(&f.rec.name, &a));
                rec.name = a.new.clone();
                cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
                if f.loc.folder != to {
                    cx.tree.move_dir(&f.loc.folder, &to)?;
                }
                Ok(Applied { summary: format!("observed rename of seat {} to {}", f.rec.name, a.new), action: None })
            }
            IdKind::Clone => {
                let loc = cx.tree.locate(&a.object)?.ok_or_else(|| gone(&a.object))?;
                let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.ok_or_else(|| gone(&a.object))?;
                if rec.lifecycle == CloneLifecycle::Retired || rec.name == a.new {
                    return Ok(noop(a.object.as_str()));
                }
                let old = rec.name.clone();
                rec.name_history.push(name_change(&old, &a));
                rec.name = a.new.clone();
                cx.tree.put_record(loc.record_path.clone(), &mut rec)?;
                // clones/<slug> lives under <seat dir>/clones.
                if let Some((seat_dir, own)) = loc.folder.as_str().rsplit_once("/clones/") {
                    let seat_dir = RepoPath::new(seat_dir)?;
                    let mut taken = layout::taken_slugs(&cx.tree, &seat_dir.join("clones")?)?;
                    taken.remove(own);
                    let to = layout::clone_dir(&seat_dir, &unique_slug(&a.new, rec.id.suffix6(), &taken));
                    if loc.folder != to {
                        cx.tree.move_dir(&loc.folder, &to)?;
                    }
                }
                Ok(Applied { summary: format!("observed rename of clone {old} to {}", a.new), action: None })
            }
            other => Err(MutationError::Bug(format!("cannot rename a {other:?}"))),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// observed.occupancy
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct EndArg {
    reason: SessionEndReason,
    #[serde(default)]
    at: Option<Timestamp>,
}

#[derive(Deserialize)]
struct OccupancyArgs {
    clone: CloneId,
    #[serde(default)]
    end: Option<EndArg>,
    #[serde(default)]
    start: Option<SessionCapture>,
    #[serde(default)]
    at: Option<Timestamp>,
}

struct OccupancyMutation;

/// A start report that changes nothing: a copy of the report whose session is the occupant, one older than the
/// occupant, or one older than a recorded session of the same native id. A resume (the same native id, at or
/// after the end of its earlier session) is none of these.
fn is_duplicate_or_stale_report(rec: &CloneRecord, cap: &SessionCapture, at: Timestamp) -> bool {
    let occupant = rec.occupant.as_ref();
    let is_occupant = occupant
        .and_then(|o| rec.sessions.iter().find(|s| s.id == o.native_session))
        .is_some_and(|s| s.native_session_id == cap.native_session_id);
    let older_than_occupant = occupant.is_some_and(|o| at < o.since);
    let predates_recorded = rec.sessions.iter().any(|s| {
        s.native_session_id == cap.native_session_id && (at < s.started || s.ended.is_some_and(|e| at < e))
    });
    is_occupant || older_than_occupant || predates_recorded
}

impl Mutation for OccupancyMutation {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: OccupancyArgs = parse(cx)?;
        let at = a.at.unwrap_or(cx.now);
        let mut summary = format!("occupancy of {}", a.clone);
        edit::<CloneRecord>(cx, &a.clone.to_any(), |rec, _| {
            if let Some(cap) = &a.start
                && rec.lifecycle == CloneLifecycle::Active
                && is_duplicate_or_stale_report(rec, cap, at)
            {
                summary = format!("duplicate or stale session report on {} ignored", a.clone);
                return Ok(());
            }
            if let Some(e) = &a.end
                && end_occupant(rec, e.reason, e.at.unwrap_or(at))
            {
                summary = format!("session ended on {} ({:?})", a.clone, e.reason);
            }
            if let Some(cap) = &a.start
                && rec.lifecycle == CloneLifecycle::Active
            {
                let current = rec
                    .occupant
                    .as_ref()
                    .and_then(|o| rec.sessions.iter().find(|s| s.id == o.native_session))
                    .map(|s| s.native_session_id.clone());
                if current.as_deref() != Some(cap.native_session_id.as_str()) {
                    end_occupant(rec, SessionEndReason::SessionChanged, at);
                    let ns = NativeSession {
                        id: NsId::new(),
                        harness: cap.harness,
                        native_session_id: cap.native_session_id.clone(),
                        transcript_path: cap.transcript_path.clone(),
                        transcript: None,
                        cwd: cap.cwd.clone(),
                        started: at,
                        ended: None,
                        end_reason: None,
                    };
                    rec.occupant = Some(Occupant { native_session: ns.id.clone(), harness: cap.harness, since: at });
                    rec.sessions.push(ns);
                    summary = format!("session started on {}", a.clone);
                }
            }
            Ok(())
        })?;
        Ok(Applied { summary, action: None })
    }
}

// ---------------------------------------------------------------------------------------------
// observed.move
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct MoveArgs {
    #[serde(default)]
    clone: Option<CloneId>,
    #[serde(default)]
    binding: Option<Binding>,
    #[serde(default)]
    seat_moved_out: Option<SeatId>,
}

struct MoveMutation;

impl Mutation for MoveMutation {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: MoveArgs = parse(cx)?;
        let mut parts = Vec::new();
        if let Some(c) = &a.clone {
            edit::<CloneRecord>(cx, &c.to_any(), |rec, cx| {
                rec.reload_required = true;
                match &a.binding {
                    Some(b) => {
                        rec.runtime.bound = Some(b.clone());
                        rec.runtime.availability = Availability::Present;
                    }
                    None => {
                        rec.runtime.bound = None;
                        rec.runtime.availability = Availability::Unknown;
                    }
                }
                rec.runtime.observed_at = Some(cx.now);
                Ok(())
            })?;
            parts.push(format!("clone {c} moved"));
        }
        if let Some(s) = &a.seat_moved_out {
            edit::<SeatRecord>(cx, &s.to_any(), |rec, cx| {
                rec.moved_out = true;
                rec.runtime.availability = Availability::Absent;
                rec.runtime.bound = None;
                rec.runtime.observed_at = Some(cx.now);
                Ok(())
            })?;
            parts.push(format!("seat {s} moved out"));
        }
        Ok(Applied { summary: parts.join(", "), action: None })
    }
}

// ---------------------------------------------------------------------------------------------
// observed.availability
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct AvailabilityArgs {
    object: AnyId,
    availability: Availability,
}

struct AvailabilityMutation;

impl Mutation for AvailabilityMutation {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: AvailabilityArgs = parse(cx)?;
        let set = |rt: &mut crate::model::common::Runtime, cx: &MutationCx<'_>| {
            rt.availability = a.availability;
            if a.availability == Availability::Absent {
                rt.bound = None;
            }
            rt.observed_at = Some(cx.now);
        };
        match a.object.kind() {
            IdKind::Teamspace => edit::<TeamspaceRecord>(cx, &a.object, |r, cx| {
                set(&mut r.runtime, cx);
                Ok(())
            })?,
            IdKind::Seat => edit::<SeatRecord>(cx, &a.object, |r, cx| {
                set(&mut r.runtime, cx);
                Ok(())
            })?,
            IdKind::Clone => edit::<CloneRecord>(cx, &a.object, |r, cx| {
                set(&mut r.runtime, cx);
                Ok(())
            })?,
            other => return Err(MutationError::Bug(format!("{other:?} objects have no runtime"))),
        }
        Ok(Applied { summary: format!("{} is {:?}", a.object, a.availability), action: None })
    }
}
