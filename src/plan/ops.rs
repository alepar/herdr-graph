//! Operations queries and interventions: `ops`, `op`, `cancel`, `reassign`, `check-instruction`, the pending-ops
//! query used by `/seat`, and supersession validation (spec §3.7, §10). Owned by hg-zmi.17.
use super::grammar::{self, Scope};
use crate::daemon::registry::{CommandCtx, CommandError, Registry};
use crate::journal::{CancelOutcome, Journal, JournalError, OpRow, ReassignOutcome};
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::common::Lifecycle;
use crate::model::operation::{OpState, OperationRecord, Rejection};
use crate::model::seat::SeatRecord;
use crate::model::{AnyId, CloneId, OpId, SeatId, Timestamp};
use crate::ports::clock::Clock;
use crate::ports::store::{Store, StoreError, read_record};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::CommitView;
use crate::writer::{Applied, Mutation, MutationCx, MutationError, MutationRegistry};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;

/// Meta key set when an op's rejection reminders are stopped without a state change (cancel of a rejected op).
pub fn reminders_stopped_key(op: &OpId) -> String {
    format!("reminders_stopped:{op}")
}

/// Journal meta key holding how many reminders (initial one included) were sent for `op`.
pub fn reminder_count_key(op: &OpId) -> String {
    format!("reminder:{op}")
}

#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpSummary {
    pub op: OpId,
    pub kind: RequestKind,
    pub state: OpState,
    pub requester: Requester,
    pub summary: String,
    pub rejection: Option<Rejection>,
    pub superseded_by: Option<OpId>,
    pub admitted_at: Timestamp,
}

/// One-line description from the request: the kind name and its scalar, non-envelope arguments.
fn summarize(req: &ChangeRequest) -> String {
    let mut s = crate::writer::kind_name(req.kind);
    if let Value::Object(m) = &req.args {
        for (k, v) in m.iter().filter(|(k, _)| !k.starts_with('_') && *k != "sub") {
            match v {
                Value::String(x) => s.push_str(&format!(" {k}={x}")),
                Value::Number(_) | Value::Bool(_) => s.push_str(&format!(" {k}={v}")),
                _ => {}
            }
        }
    }
    s
}

fn summary_of(row: &OpRow) -> OpSummary {
    OpSummary {
        op: row.op.clone(),
        kind: row.request.kind,
        state: row.state,
        requester: row.request.requester.clone(),
        summary: summarize(&row.request),
        rejection: row.rejection.clone(),
        superseded_by: row.superseded_by.clone(),
        admitted_at: row.admitted_at,
    }
}

fn reminders_stopped(journal: &Journal, op: &OpId) -> Result<bool, JournalError> {
    Ok(journal.meta_get(&reminders_stopped_key(op))?.is_some())
}

/// Unresolved = rejected|failed and not superseded and not reminders-stopped; plus admitted|applying.
/// (A superseded or cancelled op has left those states.)
fn is_unresolved(journal: &Journal, row: &OpRow) -> Result<bool, JournalError> {
    Ok(match row.state {
        OpState::Admitted | OpState::Applying => true,
        OpState::Rejected | OpState::Failed => !reminders_stopped(journal, &row.op)?,
        OpState::Committed | OpState::Cancelled | OpState::Superseded => false,
    })
}

const ALL: usize = usize::MAX >> 1;

/// Newest first. Ops of retired requesters stay listed (spec §3.7).
pub fn list_ops(journal: &Journal, unresolved: bool, limit: usize) -> Result<Vec<OpSummary>, JournalError> {
    let mut out = Vec::new();
    for row in journal.list(&[], ALL)? {
        if out.len() >= limit {
            break;
        }
        if !unresolved || is_unresolved(journal, &row)? {
            out.push(summary_of(&row));
        }
    }
    Ok(out)
}

/// Unresolved ops requested by `seat` and/or `clone` (a `None` filter matches anything); used by `/seat`.
pub fn pending_for(
    journal: &Journal,
    seat: Option<&SeatId>,
    clone: Option<&CloneId>,
) -> Result<Vec<OpSummary>, JournalError> {
    let mut out = Vec::new();
    for row in journal.list(&[], ALL)? {
        let r = &row.request.requester;
        let matches = seat.is_none_or(|s| r.seat.as_ref() == Some(s)) && clone.is_none_or(|c| r.clone.as_ref() == Some(c));
        if matches && is_unresolved(journal, &row)? {
            out.push(summary_of(&row));
        }
    }
    Ok(out)
}

/// `ops.get` payload: the summary plus journal detail and the reminder state.
pub fn op_detail(journal: &Journal, op: &OpId) -> Result<Value, OpsError> {
    let row = journal.get(op)?.ok_or_else(|| OpsError::Invalid(format!("unknown operation {op}")))?;
    let sent = journal.meta_get(&reminder_count_key(op))?.and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    Ok(json!({
        "op": summary_of(&row),
        "request": { "kind": row.request.kind, "args": row.request.args, "supersedes": row.request.supersedes },
        "attempts": row.attempts,
        "commit": row.commit,
        "action": row.action,
        "updated_at": row.updated_at,
        "reminders": { "sent": sent, "stopped": reminders_stopped(journal, op)? },
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelResult {
    /// admitted|failed → cancelled.
    Cancelled,
    /// rejected: state unchanged, reminders stopped.
    RemindersStopped,
    /// Nothing changed; `message` says why.
    Refused,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CancelReply {
    pub op: OpId,
    pub result: CancelResult,
    pub state: Option<OpState>,
    pub message: String,
}

fn refusal_for(state: OpState) -> String {
    match state {
        OpState::Committed | OpState::Applying => format!(
            "op is {}: post-commit cancellation requires a superseding request, e.g. `herdr-graph plan seat deactivate <seat>` with supersedes",
            serde_json::to_value(state).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
        ),
        other => format!(
            "op is already {}; nothing to cancel",
            serde_json::to_value(other).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
        ),
    }
}

/// `cancel <op>` (spec §3.7): admitted|failed → cancelled; rejected → reminders stopped; anything else refused.
pub fn cancel(journal: &Journal, op: &OpId, now: Timestamp) -> Result<CancelReply, JournalError> {
    let reply = |result, state, message: &str| CancelReply { op: op.clone(), result, state, message: message.to_owned() };
    let Some(row) = journal.get(op)? else {
        return Ok(reply(CancelResult::Refused, None, &format!("unknown operation {op}")));
    };
    if row.state == OpState::Rejected {
        journal.meta_set(&reminders_stopped_key(op), &now.to_rfc3339())?;
        return Ok(reply(CancelResult::RemindersStopped, Some(OpState::Rejected), "op already rejected; reminders stopped"));
    }
    match journal.cancel(op, now)? {
        CancelOutcome::Cancelled => Ok(reply(CancelResult::Cancelled, Some(OpState::Cancelled), "op cancelled")),
        CancelOutcome::NotCancellable(state) => Ok(reply(CancelResult::Refused, Some(state), &refusal_for(state))),
        CancelOutcome::Unknown => Ok(reply(CancelResult::Refused, None, &format!("unknown operation {op}"))),
    }
}

fn live_seat_record(store: &dyn Store, seat: &SeatId) -> Result<SeatRecord, OpsError> {
    let head = store.head()?;
    let view = CommitView { store, at: head };
    let loc = layout::locate(&view, &seat.to_any())?.ok_or_else(|| OpsError::Invalid(format!("no seat {seat}")))?;
    let rec = read_toml::<SeatRecord>(&view, &loc.record_path)?.ok_or_else(|| OpsError::Invalid(format!("no seat {seat}")))?;
    if rec.lifecycle == Lifecycle::Retired {
        return Err(OpsError::Invalid(format!("seat {seat} is retired; reassign to a live seat")));
    }
    Ok(rec)
}

/// `reassign <op> --to <seat>`: the requester becomes `to` (a live seat) and its teamspace. The op's rejection
/// reminders start over for the new requester. For a committed op the operation record is updated too, by a
/// `bookkeeping.reassign` op admitted straight into the journal (the writer picks it up on its next pass).
pub fn reassign(journal: &Journal, store: &dyn Store, op: &OpId, to: &SeatId, now: Timestamp) -> Result<(), OpsError> {
    let row = journal.get(op)?.ok_or_else(|| OpsError::Invalid(format!("unknown operation {op}")))?;
    let seat = live_seat_record(store, to)?;
    let old = &row.request.requester;
    let requester = Requester { teamspace: Some(seat.teamspace.clone()), seat: Some(seat.id.clone()), clone: None, native_session: None, human: old.human };
    // The writer builds the git operation record from the request it read before the reassign, so the follow-up
    // is admitted for admitted/applying ops too; it runs after the op (higher seq). The state check, the
    // requester update and the follow-up admission are one journal transaction.
    let follow_up = ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args: json!({ "sub": "reassign", "op": op, "requester": requester }),
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    };
    match journal.reassign_requester(op, &requester, &follow_up, now)? {
        ReassignOutcome::Reassigned(_) => {}
        ReassignOutcome::NotReassignable(s) => {
            return Err(OpsError::Invalid(format!("op {op} is already resolved ({s:?}); nothing to reassign")));
        }
        ReassignOutcome::Unknown => return Err(OpsError::Invalid(format!("unknown operation {op}"))),
    }
    journal.meta_delete(&reminder_count_key(op))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionStatus {
    Current,
    Obsolete,
}

impl InstructionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            InstructionStatus::Current => "current",
            InstructionStatus::Obsolete => "obsolete",
        }
    }
}

/// `check-instruction <op> <object> <rev>`: `Current` iff the object's committed `rev` equals `rev` and the op is
/// neither superseded nor cancelled. An object that does not exist at head is obsolete.
pub fn check_instruction(
    journal: &Journal,
    store: &dyn Store,
    op: &OpId,
    object: &AnyId,
    rev: u64,
) -> Result<InstructionStatus, OpsError> {
    let row = journal.get(op)?.ok_or_else(|| OpsError::Invalid(format!("unknown operation {op}")))?;
    if matches!(row.state, OpState::Superseded | OpState::Cancelled) {
        return Ok(InstructionStatus::Obsolete);
    }
    let head = store.head()?;
    let Some(loc) = store.locate(&head, object)? else { return Ok(InstructionStatus::Obsolete) };
    let table: Option<toml::Table> = read_record(store, &head, &loc.record_path)?;
    let committed = table.and_then(|t| t.get("rev").and_then(|v| v.as_integer())).map(|r| r as u64);
    Ok(if committed == Some(rev) { InstructionStatus::Current } else { InstructionStatus::Obsolete })
}

/// A replacement request may supersede an op only while the original exists and is committed, rejected, failed or
/// admitted. (The writer marks it `superseded` when the replacement commits; reminders stop and the reconciler's
/// fencing obsoletes the original's remaining effects, latest intent winning.)
pub fn validate_supersedes(journal: &Journal, op: &OpId) -> Result<(), OpsError> {
    let row = journal.get(op)?.ok_or_else(|| OpsError::Invalid(format!("cannot supersede unknown operation {op}")))?;
    match row.state {
        OpState::Committed | OpState::Rejected | OpState::Failed | OpState::Admitted => Ok(()),
        other => Err(OpsError::Invalid(format!("cannot supersede op {op}: it is {other:?}"))),
    }
}

// ---------------------------------------------------------------------------------------------
// writer mutation: bookkeeping.reassign
// ---------------------------------------------------------------------------------------------

struct ReassignRecord;
impl Mutation for ReassignRecord {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let args = &cx.request.args;
        let op: OpId = args
            .get("op")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| MutationError::Bug("bookkeeping.reassign needs a valid op".into()))?;
        let requester: Requester = args
            .get("requester")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| MutationError::Bug("bookkeeping.reassign needs a requester".into()))?;
        // An op that ended rejected or failed has no operation record: nothing to update.
        let no_record = || Applied { summary: format!("no operation record for {op}; nothing to update"), action: None };
        let Some(loc) = layout::locate(&cx.tree, &op.to_any())? else { return Ok(no_record()) };
        let Some(mut rec) = read_toml::<OperationRecord>(&cx.tree, &loc.record_path)? else { return Ok(no_record()) };
        rec.requester = requester;
        cx.tree.put_record(loc.record_path, &mut rec)?;
        Ok(Applied { summary: format!("reassign {op}"), action: None })
    }
}

/// Registers `bookkeeping.reassign`.
pub fn register_mutations(reg: &mut MutationRegistry) {
    reg.register("bookkeeping.reassign", Arc::new(ReassignRecord));
}

// ---------------------------------------------------------------------------------------------
// IPC commands
// ---------------------------------------------------------------------------------------------

pub struct OpsDeps {
    pub journal: Arc<Journal>,
    pub store: Arc<dyn Store>,
    pub clock: Arc<dyn Clock>,
}

fn err(e: OpsError) -> CommandError {
    match e {
        OpsError::Invalid(m) => CommandError::bad_request(m),
        other => CommandError::internal(other.to_string()),
    }
}

fn jerr(e: JournalError) -> CommandError {
    CommandError::internal(e.to_string())
}

fn id_arg<T: std::str::FromStr<Err = crate::model::IdError>>(args: &Value, key: &str) -> Result<T, CommandError> {
    let s = args.get(key).and_then(|v| v.as_str()).ok_or_else(|| CommandError::bad_request(format!("missing {key}")))?;
    s.parse().map_err(|e| CommandError::bad_request(format!("{key}: {e}")))
}

/// `ops.list` payload, shared by the daemon handler and the CLI's daemon-less read.
pub fn list_json(journal: &Journal, unresolved: bool, limit: usize) -> Result<Value, JournalError> {
    Ok(json!({ "ops": list_ops(journal, unresolved, limit)? }))
}

/// Registers `ops.list`, `ops.get`, `ops.cancel`, `ops.reassign`, `ops.check_instruction`.
pub fn register_commands(reg: &mut Registry, deps: OpsDeps) {
    let deps = Arc::new(deps);
    let d = deps.clone();
    reg.command("ops.list", move |_cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let unresolved = args.get("unresolved").and_then(|v| v.as_bool()).unwrap_or(false);
            let limit = args.get("limit").and_then(|v| v.as_u64()).map_or(200, |n| n as usize);
            list_json(&d.journal, unresolved, limit).map_err(jerr)
        }
    });
    let d = deps.clone();
    reg.command("ops.get", move |_cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move { op_detail(&d.journal, &id_arg::<OpId>(&args, "op")?).map_err(err) }
    });
    let d = deps.clone();
    reg.command("ops.cancel", move |_cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let reply = cancel(&d.journal, &id_arg::<OpId>(&args, "op")?, d.clock.now()).map_err(jerr)?;
            if reply.result == CancelResult::Refused {
                return Err(CommandError::rejected(reply.message));
            }
            Ok(json!(reply))
        }
    });
    let d = deps.clone();
    reg.command("ops.reassign", move |_cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let op: OpId = id_arg(&args, "op")?;
            let to = args.get("to").and_then(|v| v.as_str()).ok_or_else(|| CommandError::bad_request("missing to"))?.to_owned();
            tokio::task::spawn_blocking(move || {
                let head = d.store.head().map_err(|e| CommandError::internal(e.to_string()))?;
                let view = CommitView { store: &*d.store, at: head };
                let seat = grammar::resolve_seat(&view, &to, Scope::Live).map_err(|e| CommandError::bad_request(e.to_string()))?;
                reassign(&d.journal, &*d.store, &op, &seat, d.clock.now()).map_err(err)?;
                Ok(json!({ "op": op, "requester_seat": seat }))
            })
            .await
            .map_err(|e| CommandError::internal(e.to_string()))?
        }
    });
    let d = deps;
    reg.command("ops.check_instruction", move |_cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let op: OpId = id_arg(&args, "op")?;
            let object: AnyId = id_arg(&args, "object")?;
            let rev = args.get("rev").and_then(|v| v.as_u64()).ok_or_else(|| CommandError::bad_request("missing rev"))?;
            let status = tokio::task::spawn_blocking(move || check_instruction(&d.journal, &*d.store, &op, &object, rev))
                .await
                .map_err(|e| CommandError::internal(e.to_string()))?
                .map_err(err)?;
            Ok(json!({ "status": status }))
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::kinds_extra::testkit::*;
    use super::super::reminders::ReminderScheduler;
    use super::super::reminders::stub::RecordingThreads;
    use super::*;
    use crate::daemon::registry::CallerInfo;
    use crate::model::operation::OperationRecord;
    use crate::ports::writer::Writer;

    fn admitted_op(fx: &Fx) -> OpId {
        let req = ChangeRequest {
            kind: RequestKind::SeatRetire,
            args: json!({ "seat": "anyone" }),
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
            confirmed: None,
        };
        fx.w.admit(req).unwrap()
    }

    fn two_seats(fx: &Fx) {
        commit(fx, "teamspace create alpha");
        commit(fx, "seat create one --teamspace alpha --active");
        commit(fx, "seat create two --teamspace alpha --active");
    }

    fn j(fx: &Fx) -> &Arc<Journal> {
        fx.w.journal()
    }

    #[test]
    fn cancel_admitted_only() {
        let fx = fx();
        two_seats(&fx);
        let op = admitted_op(&fx);
        let r = cancel(j(&fx), &op, t0()).unwrap();
        assert_eq!((r.result, r.state), (CancelResult::Cancelled, Some(OpState::Cancelled)));
        assert_eq!(j(&fx).get(&op).unwrap().unwrap().state, OpState::Cancelled);
        fx.w.drain().unwrap();
        assert_eq!(j(&fx).get(&op).unwrap().unwrap().state, OpState::Cancelled, "the writer never applies a cancelled op");

        let committed = commit(&fx, "seat rename one uno");
        let r = cancel(j(&fx), &committed.op, t0()).unwrap();
        assert_eq!((r.result, r.state), (CancelResult::Refused, Some(OpState::Committed)));
        assert!(r.message.contains("supersed") && r.message.contains("committed"), "{}", r.message);
        assert_eq!(j(&fx).get(&committed.op).unwrap().unwrap().state, OpState::Committed);

        let unknown = cancel(j(&fx), &OpId::new(), t0()).unwrap();
        assert_eq!(unknown.result, CancelResult::Refused);
    }

    #[test]
    fn cancel_rejected_stops_reminders() {
        let fx = fx();
        two_seats(&fx);
        let one = seat(&fx, "one");
        let rejected = rejected_op(&fx, requester_of(&one));
        assert_eq!(list_ops(j(&fx), true, 50).unwrap().len(), 1);
        let r = cancel(j(&fx), &rejected.op, t0()).unwrap();
        assert_eq!((r.result, r.state), (CancelResult::RemindersStopped, Some(OpState::Rejected)));
        assert_eq!(r.message, "op already rejected; reminders stopped");
        assert_eq!(j(&fx).get(&rejected.op).unwrap().unwrap().state, OpState::Rejected, "state unchanged");
        assert!(list_ops(j(&fx), true, 50).unwrap().is_empty(), "no longer unresolved");
        assert_eq!(list_ops(j(&fx), false, 50).unwrap().iter().filter(|o| o.op == rejected.op).count(), 1, "still in history");
    }

    #[test]
    fn cancel_failed_allowed() {
        let fx = fx();
        let req = ChangeRequest {
            kind: RequestKind::SeatRetire,
            args: json!({}),
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
            confirmed: None,
        };
        let op = j(&fx).admit(&req, t0()).unwrap();
        j(&fx).begin_applying(&op, t0()).unwrap().unwrap();
        j(&fx).finish_failed(&op, "boom", t0()).unwrap();
        assert_eq!(list_ops(j(&fx), true, 50).unwrap().len(), 1, "failed is unresolved");
        let r = cancel(j(&fx), &op, t0()).unwrap();
        assert_eq!(r.result, CancelResult::Cancelled);
        assert!(list_ops(j(&fx), true, 50).unwrap().is_empty());
    }

    #[test]
    fn reassign_changes_requester_and_keeps_unresolved_discoverable() {
        let fx = fx();
        two_seats(&fx);
        let (one, two) = (seat(&fx, "one"), seat(&fx, "two"));
        let rejected = rejected_op(&fx, requester_of(&one));
        assert_eq!(pending_for(j(&fx), Some(&one.id), None).unwrap().len(), 1);
        reassign(j(&fx), &*fx.store, &rejected.op, &two.id, t0()).unwrap();
        let row = j(&fx).get(&rejected.op).unwrap().unwrap();
        assert_eq!(row.request.requester.seat, Some(two.id.clone()));
        assert_eq!(row.request.requester.teamspace, Some(two.teamspace.clone()));
        assert_eq!(row.request.requester.clone, None);
        assert_eq!(row.state, OpState::Rejected, "reassign does not resolve the op");
        assert_eq!(list_ops(j(&fx), true, 50).unwrap().len(), 1, "still discoverable");
        assert!(pending_for(j(&fx), Some(&one.id), None).unwrap().is_empty());
        assert_eq!(pending_for(j(&fx), Some(&two.id), None).unwrap().len(), 1);

        // a retired seat cannot take the op
        commit(&fx, "seat retire one");
        let err = reassign(j(&fx), &*fx.store, &rejected.op, &one.id, t0()).unwrap_err();
        assert!(err.to_string().contains("retired"), "{err}");
        let err = reassign(j(&fx), &*fx.store, &OpId::new(), &two.id, t0()).unwrap_err();
        assert!(err.to_string().contains("unknown operation"), "{err}");
    }

    #[test]
    fn reassign_committed_op_updates_operation_record() {
        let fx = fx();
        two_seats(&fx);
        let two = seat(&fx, "two");
        let done = commit(&fx, "seat rename one uno");
        reassign(j(&fx), &*fx.store, &done.op, &two.id, t0()).unwrap();
        assert_eq!(fx.w.drain().unwrap().len(), 1, "the bookkeeping op was admitted and applies");
        let v = view(&fx);
        let loc = layout::locate(&v, &done.op.to_any()).unwrap().unwrap();
        let rec: OperationRecord = read_toml(&v, &loc.record_path).unwrap().unwrap();
        assert_eq!(rec.requester.seat, Some(two.id));
        let row = j(&fx).get(&done.op).unwrap().unwrap();
        assert_eq!(row.request.requester.seat, rec.requester.seat);
    }

    #[test]
    fn reassign_admitted_op_updates_record_after_commit() {
        let fx = fx();
        two_seats(&fx);
        let two = seat(&fx, "two");
        let sp = plan(&fx, "seat rename one uno");
        let op = crate::plan::commands::admit_apply(&fx.deps, &CallerInfo::default(), sp.plan.id.as_str(), Some(&sp.hash), "relay").unwrap();
        assert_eq!(j(&fx).get(&op).unwrap().unwrap().state, OpState::Admitted);
        reassign(j(&fx), &*fx.store, &op, &two.id, t0()).unwrap();
        assert_eq!(fx.w.drain().unwrap().len(), 2, "the op and the follow-up bookkeeping op");
        let row = j(&fx).get(&op).unwrap().unwrap();
        assert_eq!(row.state, OpState::Committed);
        let v = view(&fx);
        let loc = layout::locate(&v, &op.to_any()).unwrap().unwrap();
        let rec: OperationRecord = read_toml(&v, &loc.record_path).unwrap().unwrap();
        assert_eq!(rec.requester.seat, Some(two.id.clone()), "the committed record names the new requester");
        assert_eq!(row.request.requester.seat, Some(two.id));
    }

    #[test]
    fn reassign_follow_up_for_rejected_op_is_noop() {
        let fx = fx();
        two_seats(&fx);
        let two = seat(&fx, "two");
        let req = ChangeRequest {
            kind: RequestKind::SeatRetire,
            args: json!({ "seat": "nobody" }),
            relied_on: vec![],
            requester: Requester::default(),
            supersedes: None,
            confirmed: None,
        };
        let op = fx.w.admit(req).unwrap();
        reassign(j(&fx), &*fx.store, &op, &two.id, t0()).unwrap();
        assert_eq!(fx.w.drain().unwrap().len(), 2);
        assert_eq!(j(&fx).get(&op).unwrap().unwrap().state, OpState::Rejected);
        let follow = j(&fx)
            .list(&[], 50)
            .unwrap()
            .into_iter()
            .find(|r| r.request.args["sub"] == "reassign")
            .expect("follow-up admitted");
        assert_eq!(follow.state, OpState::Committed, "the follow-up commits as a no-op, not a rejection");
        assert!(follow.rejection.is_none());
    }

    #[test]
    fn check_instruction_current_vs_obsolete() {
        let fx = fx();
        two_seats(&fx);
        commit(&fx, "participation join th_a --scope seat --seat one");
        let one = seat(&fx, "one");
        let leave = commit(&fx, "participation leave th_a --scope seat --seat one");
        let rev = head_rev_of(&fx, &one.id.to_any());
        let obj = one.id.to_any();
        let st = |op: &OpId, rev| check_instruction(j(&fx), &*fx.store, op, &obj, rev).unwrap();
        assert_eq!(st(&leave.op, rev), InstructionStatus::Current);
        assert_eq!(st(&leave.op, rev - 1), InstructionStatus::Obsolete, "stale rev");
        // seat moves on: the instruction becomes obsolete
        commit(&fx, "participation join th_a --scope seat --seat one");
        assert_eq!(st(&leave.op, rev), InstructionStatus::Obsolete, "rev advanced");
        // superseded op: obsolete even at the right rev
        let rev_now = head_rev_of(&fx, &obj);
        assert_eq!(st(&leave.op, rev_now), InstructionStatus::Current);
        j(&fx).supersede(&leave.op, &OpId::new(), t0()).unwrap();
        assert_eq!(st(&leave.op, rev_now), InstructionStatus::Obsolete, "superseded op");
        // cancelled op and missing object
        let cancelled = admitted_op(&fx);
        cancel(j(&fx), &cancelled, t0()).unwrap();
        assert_eq!(st(&cancelled, rev_now), InstructionStatus::Obsolete);
        let ghost = SeatId::new().to_any();
        assert_eq!(check_instruction(j(&fx), &*fx.store, &cancelled, &ghost, 1).unwrap(), InstructionStatus::Obsolete);
        assert!(check_instruction(j(&fx), &*fx.store, &OpId::new(), &obj, 1).is_err());
        assert_eq!(InstructionStatus::Current.as_str(), "current");
        assert_eq!(InstructionStatus::Obsolete.as_str(), "obsolete");
    }

    #[tokio::test]
    async fn supersession_marks_original_and_stops_reminders() {
        let fx = fx();
        two_seats(&fx);
        let one = seat(&fx, "one");
        set_channel(&fx, &one.id, "th_one");
        let one = seat(&fx, "one");
        let rejected = rejected_op(&fx, requester_of(&one));
        validate_supersedes(j(&fx), &rejected.op).unwrap();
        let threads = Arc::new(RecordingThreads::default());
        let sched = ReminderScheduler { journal: j(&fx).clone(), store: fx.store.clone(), threads: threads.clone(), clock: fx.clock.clone() };
        assert_eq!(sched.tick().await.len(), 1, "initial notify at rejection");

        // the replacement: same intent, plan carries supersedes
        let caller = CallerInfo { graph_seat: Some(one.id.to_string()), ..Default::default() };
        let sp = plan_as(&fx, &caller, &format!("seat rename two zwei --supersedes {}", rejected.op));
        assert_eq!(sp.supersedes, Some(rejected.op.clone()));
        let done = apply_plan(&fx, &sp);
        assert_eq!(done.state, OpState::Committed, "{:?}", done.rejection);
        let orig = j(&fx).get(&rejected.op).unwrap().unwrap();
        assert_eq!((orig.state, orig.superseded_by), (OpState::Superseded, Some(done.op.clone())));
        assert!(list_ops(j(&fx), true, 50).unwrap().is_empty(), "superseded is resolved");
        assert!(validate_supersedes(j(&fx), &rejected.op).is_err(), "cannot supersede twice");

        fx.clock.advance(chrono::Duration::hours(30));
        assert!(sched.tick().await.is_empty(), "no more reminders for a superseded op");
        assert_eq!(threads.sent().len(), 1);
    }

    #[test]
    fn validate_supersedes_checks_state() {
        let fx = fx();
        two_seats(&fx);
        assert!(validate_supersedes(j(&fx), &OpId::new()).is_err());
        let done = commit(&fx, "seat rename one uno");
        validate_supersedes(j(&fx), &done.op).unwrap();
        let admitted = admitted_op(&fx);
        validate_supersedes(j(&fx), &admitted).unwrap();
        cancel(j(&fx), &admitted, t0()).unwrap();
        let e = validate_supersedes(j(&fx), &admitted).unwrap_err();
        assert!(e.to_string().contains("Cancelled"), "{e}");
    }

    #[test]
    fn pending_for_seat_lists_unresolved_ops() {
        let fx = fx();
        two_seats(&fx);
        let (one, two) = (seat(&fx, "one"), seat(&fx, "two"));
        let a = rejected_op(&fx, requester_of(&one));
        let b = rejected_op(&fx, requester_of(&two));
        let queued = {
            let req = ChangeRequest {
                kind: RequestKind::SeatRetire,
                args: json!({ "seat": "x" }),
                relied_on: vec![],
                requester: requester_of(&one),
                supersedes: None,
                confirmed: None,
            };
            fx.w.admit(req).unwrap()
        };
        let ids = |v: Vec<OpSummary>| v.into_iter().map(|s| s.op).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids(pending_for(j(&fx), Some(&one.id), None).unwrap()), [a.op.clone(), queued.clone()].into());
        assert_eq!(ids(pending_for(j(&fx), Some(&two.id), None).unwrap()), [b.op.clone()].into());
        assert_eq!(pending_for(j(&fx), None, None).unwrap().len(), 3);
        let someone_else = SeatId::new();
        assert!(pending_for(j(&fx), Some(&someone_else), None).unwrap().is_empty());
        // resolved ops drop out: committed ones were never listed
        cancel(j(&fx), &a.op, t0()).unwrap();
        assert_eq!(ids(pending_for(j(&fx), Some(&one.id), None).unwrap()), [queued].into());
        let s = &pending_for(j(&fx), Some(&two.id), None).unwrap()[0];
        assert_eq!(s.rejection.as_ref().unwrap().reason, "unknown_plan");
        assert!(s.summary.starts_with("seat_retire"), "{}", s.summary);
    }

    #[test]
    fn ops_of_retired_requester_still_listed() {
        let fx = fx();
        two_seats(&fx);
        let one = seat(&fx, "one");
        let rejected = rejected_op(&fx, requester_of(&one));
        commit(&fx, "seat retire one");
        let listed = list_ops(j(&fx), true, 50).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].op, rejected.op);
        assert_eq!(listed[0].requester.seat, Some(one.id.clone()));
        assert_eq!(pending_for(j(&fx), Some(&one.id), None).unwrap().len(), 1);
        // the limit applies after filtering
        assert_eq!(list_ops(j(&fx), false, 1).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ipc_commands_roundtrip() {
        let fx = fx();
        two_seats(&fx);
        let one = seat(&fx, "one");
        let rejected = rejected_op(&fx, requester_of(&one));
        let mut reg = Registry::default();
        register_commands(&mut reg, OpsDeps { journal: j(&fx).clone(), store: fx.store.clone(), clock: fx.clock.clone() });
        let call = |kind: &'static str, args: Value| {
            let h = reg.handler(kind).unwrap();
            async move { h.call(CommandCtx { request_id: "r".into(), caller: CallerInfo::default() }, args).await }
        };
        let v = call("ops.list", json!({ "unresolved": true })).await.unwrap();
        assert_eq!(v["ops"].as_array().unwrap().len(), 1);
        let v = call("ops.get", json!({ "op": rejected.op })).await.unwrap();
        assert_eq!(v["op"]["state"], "rejected");
        assert_eq!(v["reminders"]["sent"], 0);
        call("ops.reassign", json!({ "op": rejected.op, "to": "two" })).await.unwrap();
        let two = seat(&fx, "two");
        assert_eq!(j(&fx).get(&rejected.op).unwrap().unwrap().request.requester.seat, Some(two.id.clone()));
        let v = call("ops.check_instruction", json!({ "op": rejected.op, "object": two.id, "rev": two.rev })).await.unwrap();
        assert_eq!(v["status"], "current");
        let done = commit(&fx, "seat rename one uno");
        let e = call("ops.cancel", json!({ "op": done.op })).await.unwrap_err();
        assert!(e.message.contains("supersed"), "{}", e.message);
        let v = call("ops.cancel", json!({ "op": rejected.op })).await.unwrap();
        assert_eq!(v["result"], "reminders_stopped");
        let e = call("ops.get", json!({ "op": "nonsense" })).await.unwrap_err();
        assert!(e.message.contains("op"), "{}", e.message);
    }
}
