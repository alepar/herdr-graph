//! Bookkeeping mutation kinds for `tr_` / `rq_` records (spec §8.1-8.2). They record facts (a transcript was
//! captured, a request was delivered, a result arrived); none of them confirms anything for a human.
//! Keys: `bookkeeping.transcript`, `bookkeeping.request_create`, `bookkeeping.request_delivery`,
//! `bookkeeping.request_ack`, `bookkeeping.request_complete`, `bookkeeping.request_unresolved`.
use super::coverage::{gaps, merge_coverage};
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::clone::CloneRecord;
use crate::model::request::{DeliveryAttempt, ProcessingRequest, RequestResult, RequestStatus};
use crate::model::transcript::TranscriptRecord;
use crate::model::{
    AnyId, ByteRange, CloneId, NsId, RequestId, SCHEMA_VERSION, SeatId, Timestamp, TranscriptId,
};
use crate::ports::store::StoreError;
use crate::store::layout;
use crate::store::tree::{Overlay, TreeRead};
use crate::writer::{Applied, Mutation, MutationCx, MutationError, MutationRegistry, Reject};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

pub const TRANSCRIPT_KEY: &str = "bookkeeping.transcript";
pub const REQUEST_CREATE_KEY: &str = "bookkeeping.request_create";
pub const REQUEST_DELIVERY_KEY: &str = "bookkeeping.request_delivery";
pub const REQUEST_ACK_KEY: &str = "bookkeeping.request_ack";
pub const REQUEST_COMPLETE_KEY: &str = "bookkeeping.request_complete";
pub const REQUEST_UNRESOLVED_KEY: &str = "bookkeeping.request_unresolved";

/// `unresolved` reason of a transcript whose path the harness profile cannot supply.
pub const TRANSCRIPT_UNRESOLVED: &str = "transcript_unresolved";
/// `unresolved` reason of a request whose transcript file is missing or unreadable.
pub const MISSING_INPUT: &str = "missing_input";
/// Prefix of the `unresolved` reason of a request merged into an older overlapping one.
pub const MERGED_PREFIX: &str = "merged_into:";

pub fn register_mutations(reg: &mut MutationRegistry) {
    reg.register(TRANSCRIPT_KEY, Arc::new(TranscriptMutation));
    reg.register(REQUEST_CREATE_KEY, Arc::new(RequestCreate));
    reg.register(REQUEST_DELIVERY_KEY, Arc::new(RequestDelivery));
    reg.register(REQUEST_ACK_KEY, Arc::new(RequestAck));
    reg.register(REQUEST_COMPLETE_KEY, Arc::new(RequestComplete));
    reg.register(REQUEST_UNRESOLVED_KEY, Arc::new(RequestUnresolved));
}

/// A bookkeeping request of sub-kind `sub` (`args["sub"]` selects the mutation).
pub fn bookkeeping_request(sub: &str, mut args: serde_json::Value) -> ChangeRequest {
    args["sub"] = json!(sub);
    ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args,
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    }
}

/// Whether the request was merged into an older overlapping one (such rows are skipped by scans).
pub fn is_merged(r: &ProcessingRequest) -> bool {
    r.status == RequestStatus::Unresolved
        && r.unresolved
            .as_deref()
            .is_some_and(|u| u.starts_with(MERGED_PREFIX))
}

fn parse<T: DeserializeOwned>(cx: &MutationCx<'_>) -> Result<T, MutationError> {
    serde_json::from_value(cx.request.args.clone()).map_err(|e| MutationError::Bug(e.to_string()))
}

fn reject(reason: &str, explanation: String) -> MutationError {
    MutationError::Reject(Reject {
        reason: reason.into(),
        explanation,
        current_revs: vec![],
    })
}

fn noop(summary: impl Into<String>) -> Applied {
    Applied {
        summary: summary.into(),
        action: None,
    }
}

fn read_request(
    tree: &Overlay<'_>,
    rq: &RequestId,
) -> Result<(crate::ports::store::RepoPath, ProcessingRequest), MutationError> {
    let path = layout::request_record(rq);
    match tree.read_record::<ProcessingRequest>(&path)? {
        Some(r) => Ok((path, r)),
        None => Err(reject(
            "request_missing",
            format!("{rq} does not exist at the committed head"),
        )),
    }
}

/// Every request of one transcript.
fn requests_of(
    tree: &dyn TreeRead,
    tr: &TranscriptId,
) -> Result<Vec<ProcessingRequest>, StoreError> {
    Ok(layout::list_requests(tree)?
        .into_iter()
        .map(|(_, r)| r)
        .filter(|r| &r.transcript == tr)
        .collect())
}

/// The newest transcript record of a native session and path.
fn find_transcript(
    tree: &dyn TreeRead,
    ns: &NsId,
    path: &std::path::Path,
) -> Result<Option<TranscriptRecord>, StoreError> {
    Ok(layout::list_transcripts(tree)?
        .into_iter()
        .map(|(_, t)| t)
        .filter(|t| &t.native_session == ns && t.transcript_path == path)
        .max_by(|a, b| a.id.cmp(&b.id)))
}

fn find_transcript_by_id(
    tree: &dyn TreeRead,
    id: &TranscriptId,
) -> Result<Option<TranscriptRecord>, StoreError> {
    Ok(layout::list_transcripts(tree)?
        .into_iter()
        .map(|(_, t)| t)
        .find(|t| &t.id == id))
}

struct TranscriptSpec<'a> {
    seat: &'a SeatId,
    clone: &'a CloneId,
    ns: &'a NsId,
    path: Option<&'a std::path::Path>,
    summaries: bool,
    unresolved: Option<&'a str>,
    force_new: bool,
}

/// Find the transcript record for (ns, path) or create it; links the native session to it.
fn ensure_transcript(
    cx: &mut MutationCx<'_>,
    s: &TranscriptSpec<'_>,
) -> Result<TranscriptRecord, MutationError> {
    let path = s.path.map(std::path::Path::to_path_buf).unwrap_or_default();
    if !s.force_new
        && let Some(mut found) = find_transcript(&cx.tree, s.ns, &path)?
    {
        if found.source_seat_summaries_enabled_at_capture != s.summaries {
            found.source_seat_summaries_enabled_at_capture = s.summaries;
            let p = layout::transcript_record(&found.seat, &found.id);
            cx.tree.put_record(p, &mut found)?;
        }
        return Ok(found);
    }
    let mut rec = TranscriptRecord {
        schema: SCHEMA_VERSION,
        id: TranscriptId::new(),
        rev: 0,
        transcript_path: path,
        native_session: s.ns.clone(),
        seat: s.seat.clone(),
        clone: s.clone.clone(),
        source_seat_summaries_enabled_at_capture: s.summaries,
        coverage: vec![],
        gaps: vec![],
        unresolved: s.unresolved.map(str::to_owned),
    };
    cx.tree
        .put_record(layout::transcript_record(s.seat, &rec.id), &mut rec)?;
    link_session(cx, s.clone, s.ns, &rec.id)?;
    Ok(rec)
}

/// `clone.sessions[ns].transcript = tr` (the newest record of that session wins).
fn link_session(
    cx: &mut MutationCx<'_>,
    clone: &CloneId,
    ns: &NsId,
    tr: &TranscriptId,
) -> Result<(), MutationError> {
    let any = clone.to_any();
    let Some(loc) = cx.tree.locate(&any)? else {
        return Ok(());
    };
    let Some(mut rec) = cx.tree.read_record::<CloneRecord>(&loc.record_path)? else {
        return Ok(());
    };
    let Some(session) = rec.sessions.iter_mut().find(|s| &s.id == ns) else {
        return Ok(());
    };
    if session.transcript.as_ref() == Some(tr) {
        return Ok(());
    }
    session.transcript = Some(tr.clone());
    cx.tree.put_record(loc.record_path, &mut rec)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// bookkeeping.transcript
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct TranscriptArgs {
    clone: CloneId,
    ns: NsId,
    #[serde(default)]
    path: Option<PathBuf>,
    summaries: bool,
    #[serde(default)]
    unresolved: Option<String>,
    #[serde(default)]
    force_new: bool,
}

fn seat_of(cx: &MutationCx<'_>, clone: &CloneId) -> Result<SeatId, MutationError> {
    let any = clone.to_any();
    let loc = cx
        .tree
        .locate(&any)?
        .ok_or_else(|| reject("object_missing", format!("{any} does not exist")))?;
    let rec: CloneRecord = cx
        .tree
        .read_record(&loc.record_path)?
        .ok_or_else(|| reject("object_missing", format!("{any} does not exist")))?;
    Ok(rec.seat)
}

/// `bookkeeping.transcript`: create or update the `tr_` of a native session (path, seat, clone, the seat's
/// `summaries` flag at capture, unresolved reason).
struct TranscriptMutation;
impl Mutation for TranscriptMutation {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: TranscriptArgs = parse(cx)?;
        let seat = seat_of(cx, &a.clone)?;
        let unresolved = a
            .unresolved
            .clone()
            .or_else(|| a.path.is_none().then(|| TRANSCRIPT_UNRESOLVED.to_owned()));
        let tr = ensure_transcript(
            cx,
            &TranscriptSpec {
                seat: &seat,
                clone: &a.clone,
                ns: &a.ns,
                path: a.path.as_deref(),
                summaries: a.summaries,
                unresolved: unresolved.as_deref(),
                force_new: a.force_new,
            },
        )?;
        Ok(noop(format!("transcript {} of {}", tr.id, a.ns)))
    }
}

// ---------------------------------------------------------------------------------------------
// bookkeeping.request_create
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateArgs {
    clone: CloneId,
    ns: NsId,
    #[serde(default)]
    path: Option<PathBuf>,
    /// Newline-aligned size of the transcript file now; `None` when it is missing or unreadable.
    #[serde(default)]
    size: Option<u64>,
    summaries: bool,
    #[serde(default)]
    unresolved: Option<String>,
    /// Explicit range (a gap re-request); default `[covered_end, size)`.
    #[serde(default)]
    range: Option<ByteRange>,
    /// The file was replaced: start a fresh `tr_` with empty coverage.
    #[serde(default)]
    force_new: bool,
    /// Request against this existing transcript instead of looking one up.
    #[serde(default)]
    tr: Option<TranscriptId>,
}

/// The highest byte any coverage or live (non-unresolved) request of the transcript reaches.
pub(crate) fn covered_end(tr: &TranscriptRecord, requests: &[ProcessingRequest]) -> u64 {
    let cov = tr.coverage.iter().map(|r| r.end);
    let reqs = requests
        .iter()
        .filter(|r| r.status != RequestStatus::Unresolved)
        .map(|r| r.range.end);
    cov.chain(reqs).max().unwrap_or(0)
}

fn overlaps(a: &ByteRange, b: &ByteRange) -> bool {
    a.start < b.end && b.start < a.end
}

struct RequestCreate;
impl Mutation for RequestCreate {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: CreateArgs = parse(cx)?;
        if !a.summaries {
            return Ok(noop("source seat has summaries disabled: no request"));
        }
        let seat = seat_of(cx, &a.clone)?;
        let tr = match &a.tr {
            Some(id) => find_transcript_by_id(&cx.tree, id)?
                .ok_or_else(|| reject("object_missing", format!("{id} does not exist")))?,
            None => ensure_transcript(
                cx,
                &TranscriptSpec {
                    seat: &seat,
                    clone: &a.clone,
                    ns: &a.ns,
                    path: a.path.as_deref(),
                    summaries: a.summaries,
                    unresolved: a
                        .unresolved
                        .as_deref()
                        .or_else(|| a.path.is_none().then_some(TRANSCRIPT_UNRESOLVED)),
                    force_new: a.force_new,
                },
            )?,
        };
        if tr.unresolved.is_some() && a.size.is_none() && a.path.is_none() {
            return Ok(noop(format!(
                "{} is unresolved ({}): nothing to request",
                tr.id,
                tr.unresolved.as_deref().unwrap_or("?")
            )));
        }
        let existing = requests_of(&cx.tree, &tr.id)?;
        let start = covered_end(&tr, &existing);

        let (range, unresolved) = match (a.range, a.size) {
            (Some(r), _) => (r, None),
            (None, Some(size)) => (ByteRange { start, end: size }, None),
            // Missing or unreadable input: visible to the scan, never "done".
            (None, None) => (ByteRange { start, end: start }, Some(MISSING_INPUT)),
        };
        if let Some(reason) = unresolved {
            let dup = existing.iter().any(|r| {
                r.status == RequestStatus::Unresolved && r.unresolved.as_deref() == Some(reason)
            });
            if dup {
                return Ok(noop(format!("{}: {reason} already recorded", tr.id)));
            }
            let rq = new_request(
                cx,
                &tr,
                range,
                RequestStatus::Unresolved,
                Some(reason.to_owned()),
            )?;
            return Ok(noop(format!("request {rq} unresolved: {reason}")));
        }
        if range.is_empty() || range.start > range.end {
            return Ok(noop(format!("{}: nothing new beyond byte {start}", tr.id)));
        }
        // Dedup key (tr, range).
        if let Some(dup) = existing
            .iter()
            .find(|r| r.range == range && r.status != RequestStatus::Unresolved)
        {
            return Ok(noop(format!(
                "request {} already covers {}-{}",
                dup.id, range.start, range.end
            )));
        }
        // Overlapping pending (undelivered) requests merge before delivery: the oldest keeps its id.
        let mut group: Vec<&ProcessingRequest> = existing
            .iter()
            .filter(|r| r.status == RequestStatus::Pending && overlaps(&r.range, &range))
            .collect();
        if group.is_empty() {
            let rq = new_request(cx, &tr, range, RequestStatus::Pending, None)?;
            return Ok(noop(format!(
                "request {rq} for {} bytes {}-{}",
                tr.id, range.start, range.end
            )));
        }
        group.sort_by(|x, y| x.id.cmp(&y.id));
        let mut keeper = group[0].clone();
        let (mut lo, mut hi) = (
            range.start.min(keeper.range.start),
            range.end.max(keeper.range.end),
        );
        for other in &group[1..] {
            lo = lo.min(other.range.start);
            hi = hi.max(other.range.end);
            let mut o = (*other).clone();
            o.status = RequestStatus::Unresolved;
            o.unresolved = Some(format!("{MERGED_PREFIX}{}", keeper.id));
            cx.tree.put_record(layout::request_record(&o.id), &mut o)?;
        }
        keeper.range = ByteRange { start: lo, end: hi };
        let id = keeper.id.clone();
        cx.tree
            .put_record(layout::request_record(&id), &mut keeper)?;
        Ok(noop(format!("request {id} extended to bytes {lo}-{hi}")))
    }
}

fn new_request(
    cx: &mut MutationCx<'_>,
    tr: &TranscriptRecord,
    range: ByteRange,
    status: RequestStatus,
    unresolved: Option<String>,
) -> Result<RequestId, MutationError> {
    let mut rq = ProcessingRequest {
        schema: SCHEMA_VERSION,
        id: RequestId::new(),
        rev: 0,
        transcript: tr.id.clone(),
        range,
        status,
        unresolved,
        undeliverable: None,
        created_by_op: cx.op.clone(),
        delivery: Default::default(),
        result: None,
    };
    cx.tree
        .put_record(layout::request_record(&rq.id), &mut rq)?;
    Ok(rq.id)
}

// ---------------------------------------------------------------------------------------------
// bookkeeping.request_delivery
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct DeliveryArgs {
    rq: RequestId,
    /// Apply only while the request still has exactly this many delivery attempts (a duplicate or late
    /// write about an attempt that is already recorded is a no-op).
    #[serde(default)]
    expect_attempts: Option<usize>,
    #[serde(default)]
    attempt: Option<DeliveryAttempt>,
    #[serde(default)]
    undeliverable: Option<String>,
    #[serde(default)]
    clear_undeliverable: bool,
    #[serde(default)]
    reminded_at: Option<Timestamp>,
}

/// `bookkeeping.request_delivery`: a delivery attempt (message id, op key, retry), the `undeliverable`
/// flag, the reminder time. A delivered request waits for its ACK; nothing here completes it.
struct RequestDelivery;
impl Mutation for RequestDelivery {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: DeliveryArgs = parse(cx)?;
        let (path, mut rq) = read_request(&cx.tree, &a.rq)?;
        if matches!(
            rq.status,
            RequestStatus::Completed | RequestStatus::Unresolved
        ) {
            return Ok(noop(format!(
                "{} is {:?}: delivery facts ignored",
                a.rq, rq.status
            )));
        }
        if a.expect_attempts
            .is_some_and(|n| n != rq.delivery.attempts.len())
        {
            return Ok(noop(format!("{}: attempt already recorded", a.rq)));
        }
        let mut what = Vec::new();
        if let Some(att) = &a.attempt {
            if att.message_id.is_some() {
                rq.delivery.message_id = att.message_id.clone();
            }
            if att.retry {
                // A fresh delivery needs a fresh ACK.
                rq.delivery.dispatched_at = None;
            }
            rq.delivery.reminded_at = None;
            rq.delivery.attempts.push(att.clone());
            rq.status = RequestStatus::Delivered;
            rq.undeliverable = None;
            what.push("attempt");
        }
        if let Some(reason) = &a.undeliverable
            && rq.status == RequestStatus::Pending
        {
            rq.undeliverable = Some(reason.clone());
            what.push("undeliverable");
        }
        if a.clear_undeliverable && rq.undeliverable.take().is_some() {
            what.push("undeliverable cleared");
        }
        if let Some(at) = a.reminded_at {
            rq.delivery.reminded_at = Some(at);
            what.push("reminded");
        }
        if what.is_empty() {
            return Ok(noop(format!("{}: nothing to record", a.rq)));
        }
        cx.tree.put_record(path, &mut rq)?;
        Ok(noop(format!("{} delivery: {}", a.rq, what.join(", "))))
    }
}

// ---------------------------------------------------------------------------------------------
// bookkeeping.request_ack
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct AckArgs {
    rq: RequestId,
    #[serde(default)]
    at: Option<Timestamp>,
}

/// `bookkeeping.request_ack`: the summarizer (or the service receipt) acknowledged dispatch. Records
/// `dispatched_at` and status `dispatched`; an ACK is never success and never completes a request.
struct RequestAck;
impl Mutation for RequestAck {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: AckArgs = parse(cx)?;
        let (path, mut rq) = read_request(&cx.tree, &a.rq)?;
        match rq.status {
            RequestStatus::Completed => return Ok(noop(format!("{} already completed", a.rq))),
            RequestStatus::Unresolved => {
                return Err(reject(
                    "request_unresolved",
                    format!(
                        "{} is unresolved ({}); it cannot be acknowledged",
                        a.rq,
                        rq.unresolved.as_deref().unwrap_or("?")
                    ),
                ));
            }
            _ => {}
        }
        if rq.delivery.dispatched_at.is_none() {
            rq.delivery.dispatched_at = Some(a.at.unwrap_or(cx.now));
        }
        rq.status = RequestStatus::Dispatched;
        cx.tree.put_record(path, &mut rq)?;
        Ok(noop(format!("{} dispatched", a.rq)))
    }
}

// ---------------------------------------------------------------------------------------------
// bookkeeping.request_complete
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct CompleteArgs {
    rq: RequestId,
    output_ref: String,
    covered: ByteRange,
    reported_by: AnyId,
    #[serde(default)]
    at: Option<Timestamp>,
}

/// `bookkeeping.request_complete`: the result and its covered range; the transcript's coverage becomes
/// the union (it never shrinks, whatever order results arrive in) and its gaps are recomputed.
struct RequestComplete;
impl Mutation for RequestComplete {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: CompleteArgs = parse(cx)?;
        let (path, mut rq) = read_request(&cx.tree, &a.rq)?;
        if rq.status == RequestStatus::Unresolved {
            return Err(reject(
                "request_unresolved",
                format!(
                    "{} is unresolved ({}); it cannot be completed",
                    a.rq,
                    rq.unresolved.as_deref().unwrap_or("?")
                ),
            ));
        }
        rq.status = RequestStatus::Completed;
        rq.result = Some(RequestResult {
            output_ref: a.output_ref.clone(),
            covered_range: a.covered,
            reported_by: a.reported_by.clone(),
            at: a.at.unwrap_or(cx.now),
        });
        let tr_id = rq.transcript.clone();
        cx.tree.put_record(path, &mut rq)?;

        let Some(mut tr) = find_transcript_by_id(&cx.tree, &tr_id)? else {
            return Err(reject("object_missing", format!("{tr_id} does not exist")));
        };
        tr.coverage = merge_coverage(&tr.coverage, a.covered);
        let requested: Vec<ByteRange> = requests_of(&cx.tree, &tr_id)?
            .into_iter()
            .filter(|r| r.status == RequestStatus::Completed)
            .map(|r| r.range)
            .collect();
        tr.gaps = gaps(&requested, &tr.coverage);
        let tr_path = layout::transcript_record(&tr.seat, &tr.id);
        cx.tree.put_record(tr_path, &mut tr)?;
        Ok(noop(format!(
            "{} completed covering {}-{}",
            a.rq, a.covered.start, a.covered.end
        )))
    }
}

// ---------------------------------------------------------------------------------------------
// bookkeeping.request_unresolved
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct UnresolvedArgs {
    rq: RequestId,
    reason: String,
}

/// `bookkeeping.request_unresolved`: mark a request unresolved with a reason (never "done").
struct RequestUnresolved;
impl Mutation for RequestUnresolved {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: UnresolvedArgs = parse(cx)?;
        let (path, mut rq) = read_request(&cx.tree, &a.rq)?;
        if rq.status == RequestStatus::Completed {
            return Ok(noop(format!("{} already completed", a.rq)));
        }
        rq.status = RequestStatus::Unresolved;
        rq.unresolved = Some(a.reason.clone());
        cx.tree.put_record(path, &mut rq)?;
        Ok(noop(format!("{} unresolved: {}", a.rq, a.reason)))
    }
}
