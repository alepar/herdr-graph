//! Transcript and request tests: coverage arithmetic (proptest), then the service over the real writer,
//! journal and git store with FakeThreads, FakeHerdr and the reconciler.
use super::*;
use crate::daemon::registry::{CallerInfo, Registry};
use crate::daemon::server::dispatch_registered;
use crate::herdr::fake::{FakeCall, FakeHerdr};
use crate::ipc::IpcResult;
use crate::model::Timestamp;
use crate::model::change::ChangeRequest;
use crate::model::clone::CloneRecord;
use crate::model::common::Lifecycle;
use crate::model::effect::{EffectKind, EffectRecord, EffectStatus};
use crate::model::harness::Harness;
use crate::model::native_session::SessionEndReason;
use crate::model::operation::OpState;
use crate::model::request::ProcessingRequest;
use crate::model::seat::SeatRecord;
use crate::model::transcript::TranscriptRecord;
use crate::observe::mutations::occupancy_request;
use crate::observe::{SessionCapture, SessionEnded};
use crate::plan::commands::{PlanDeps, admit_apply, create_plan};
use crate::plan::core_kinds::register_core_kinds;
use crate::plan::kind::KindRegistry;
use crate::plan::store::PlanStore;
use crate::ports::clock::ManualClock;
use crate::ports::threads::*;
use crate::reconcile::{Reconciler, ReconcilerConfig, RequesterNotifier, StepReport};
use crate::store::GitStore;
use crate::store::init::init_instance;
use crate::threads::{FakePaneSeatMap, FakeThreads};
use crate::writer::{
    Applied, Mutation, MutationCx, MutationError, MutationRegistry, WriterConfig, WriterCore,
};
use chrono::TimeZone;
use proptest::prelude::*;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------------------------------
// coverage
// ---------------------------------------------------------------------------------------------

fn br(start: u64, end: u64) -> ByteRange {
    ByteRange { start, end }
}

/// Every byte covered by `ranges`, for brute-force comparison.
fn bytes_of(ranges: &[ByteRange]) -> std::collections::BTreeSet<u64> {
    ranges.iter().flat_map(|r| r.start..r.end).collect()
}

fn range_strategy() -> impl Strategy<Value = ByteRange> {
    (0u64..200, 0u64..60).prop_map(|(s, len)| br(s, s + len))
}

fn fold(order: &[ByteRange]) -> (Vec<ByteRange>, Vec<Vec<ByteRange>>) {
    let mut cov = Vec::new();
    let mut steps = Vec::new();
    for r in order {
        cov = merge_coverage(&cov, *r);
        steps.push(cov.clone());
    }
    (cov, steps)
}

proptest! {
    #[test]
    fn coverage_merge_is_order_independent(
        (a, b) in proptest::collection::vec(range_strategy(), 0..12)
            .prop_flat_map(|v| (Just(v.clone()).prop_shuffle(), Just(v).prop_shuffle()))
    ) {
        let (cov_a, steps_a) = fold(&a);
        let (cov_b, _) = fold(&b);
        prop_assert_eq!(&cov_a, &cov_b, "the same results in two orders must give identical coverage");
        // Coverage never shrinks after any step, and is exactly the union of what was merged so far.
        let mut prev: std::collections::BTreeSet<u64> = Default::default();
        for (i, step) in steps_a.iter().enumerate() {
            let now = bytes_of(step);
            prop_assert!(prev.is_subset(&now), "step {} lost bytes", i);
            prop_assert_eq!(&now, &bytes_of(&a[..=i]));
            prev = now;
        }
        // Canonical form: sorted, non-empty, neither overlapping nor adjacent.
        for w in cov_a.windows(2) {
            prop_assert!(w[0].end < w[1].start);
        }
        prop_assert!(cov_a.iter().all(|r| !r.is_empty()));
    }

    #[test]
    fn gaps_are_requested_minus_covered(
        requested in proptest::collection::vec(range_strategy(), 0..6),
        covered in proptest::collection::vec(range_strategy(), 0..6),
    ) {
        let g = gaps(&requested, &covered);
        let want: std::collections::BTreeSet<u64> = bytes_of(&requested).difference(&bytes_of(&covered)).copied().collect();
        prop_assert_eq!(bytes_of(&g), want);
        for w in g.windows(2) {
            prop_assert!(w[0].end < w[1].start, "gaps are coalesced and sorted");
        }
    }
}

#[test]
fn align_end_to_newline() {
    let bytes = b"ab\ncd\nef";
    assert_eq!(
        align_end(bytes, 8),
        6,
        "a trailing partial line is not covered"
    );
    assert_eq!(align_end(bytes, 5), 3);
    assert_eq!(align_end(bytes, 6), 6, "an end right after a newline stays");
    assert_eq!(align_end(bytes, 2), 0, "no newline yet");
    assert_eq!(
        align_end(bytes, 100),
        6,
        "an end past the file clamps to its length"
    );
    assert_eq!(align_end(b"", 0), 0);
}

#[test]
fn aligned_len_reads_the_tail_of_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("t.jsonl");
    // Longer than one read chunk, with a partial last line.
    let mut body = String::new();
    for i in 0..2000 {
        body.push_str(&format!("{{\"n\":{i}}}\n"));
    }
    let full = body.len() as u64;
    body.push_str("{\"partial\":");
    std::fs::write(&p, &body).unwrap();
    assert_eq!(requests::stat_file(&p).unwrap().aligned, full);
    std::fs::write(&p, "no newline at all").unwrap();
    assert_eq!(requests::stat_file(&p).unwrap().aligned, 0);
}

#[test]
fn merge_adjacent_and_overlapping_ranges_coalesce() {
    assert_eq!(merge_coverage(&[br(0, 10)], br(10, 20)), vec![br(0, 20)]);
    assert_eq!(
        merge_coverage(&[br(0, 10), br(30, 40)], br(5, 35)),
        vec![br(0, 40)]
    );
    assert_eq!(
        merge_coverage(&[br(0, 10)], br(20, 30)),
        vec![br(0, 10), br(20, 30)]
    );
    assert_eq!(
        gaps(&[br(0, 100)], &[br(10, 20), br(50, 60)]),
        vec![br(0, 10), br(20, 50), br(60, 100)]
    );
}

// ---------------------------------------------------------------------------------------------
// fixture
// ---------------------------------------------------------------------------------------------

fn t0() -> Timestamp {
    chrono::Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap()
}

struct Quiet;
impl RequesterNotifier for Quiet {
    fn notify(&self, _op: &crate::model::OpId, _severity: Severity, _text: &str) {}
}

/// Admission that commits before it returns: the services under test wait for commits, and the test
/// thread is the only one that can run the (synchronous) writer.
struct DrainingWriter(Arc<WriterCore>);
impl Writer for DrainingWriter {
    fn admit(
        &self,
        request: ChangeRequest,
    ) -> Result<crate::model::OpId, crate::ports::writer::WriterError> {
        let op = self.0.admit(request)?;
        self.0.drain()?;
        Ok(op)
    }
    fn status(
        &self,
        op: &crate::model::OpId,
    ) -> Result<Option<OpState>, crate::ports::writer::WriterError> {
        self.0.status(op)
    }
}
use crate::ports::writer::Writer;

/// Test-only: point `graph.toml summarizer_seat` at a seat.
struct SetSummarizerSeat;
impl Mutation for SetSummarizerSeat {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let seat: SeatId = cx.request.args["seat"].as_str().unwrap().parse().unwrap();
        let mut g = crate::store::layout::read_graph(&cx.tree)?;
        g.summarizer_seat = Some(seat);
        cx.tree.put_file(
            crate::store::layout::graph_toml(),
            toml::to_string(&g).unwrap().into_bytes(),
        );
        Ok(Applied {
            summary: "set summarizer seat".into(),
            action: None,
        })
    }
}

struct Fx {
    _tmp: tempfile::TempDir,
    dir: std::path::PathBuf,
    deps: PlanDeps,
    w: Arc<WriterCore>,
    dw: Arc<DrainingWriter>,
    store: Arc<GitStore>,
    journal: Arc<crate::journal::Journal>,
    clock: Arc<ManualClock>,
    herdr: Arc<FakeHerdr>,
    rec: Arc<Reconciler>,
    threads: Arc<FakeThreads>,
    map: Arc<FakePaneSeatMap>,
    tr: Arc<Transcripts>,
}

fn fx() -> Fx {
    fx_with(|t| t as Arc<dyn ThreadsPort>, |_| {})
}

fn fx_with(
    wrap: impl FnOnce(Arc<FakeThreads>) -> Arc<dyn ThreadsPort>,
    setup: impl FnOnce(&Fx),
) -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("inst");
    init_instance(&root).unwrap();
    let dir = tmp.path().join("claude");
    std::fs::create_dir_all(&dir).unwrap();
    let mut kinds = KindRegistry::default();
    register_core_kinds(&mut kinds);
    crate::plan::kinds_extra::register_kinds(&mut kinds);
    let kinds = Arc::new(kinds);
    let plans = Arc::new(PlanStore::new(root.join(".graph-local/plans")));
    let mut reg = MutationRegistry::default();
    kinds.register_mutations(&mut reg, plans.clone());
    crate::reconcile::register_mutations(&mut reg);
    crate::threads::register_mutations(&mut reg);
    crate::observe::register_mutations(&mut reg);
    register_mutations(&mut reg);
    reg.register(
        "bookkeeping.test_set_summarizer",
        Arc::new(SetSummarizerSeat),
    );
    let store = Arc::new(GitStore::open(&root).unwrap());
    let journal =
        Arc::new(crate::journal::Journal::open(&crate::journal::Journal::path_in(&root)).unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let w = WriterCore::new(
        store.clone(),
        journal.clone(),
        Arc::new(reg),
        clock.clone(),
        WriterConfig::default(),
    );
    let dw = Arc::new(DrainingWriter(w.clone()));
    let deps = PlanDeps {
        kinds,
        plans,
        store: store.clone(),
        writer: w.clone(),
        clock: clock.clone(),
        instance: root.clone(),
    };
    let herdr = FakeHerdr::new();
    let cfg = ReconcilerConfig::new(root.clone());
    let rec = Reconciler::new(
        store.clone(),
        journal.clone(),
        dw.clone(),
        herdr.clone(),
        clock.clone(),
        Arc::new(Quiet),
        cfg,
    );
    let threads = Arc::new(FakeThreads::new());
    let map = Arc::new(FakePaneSeatMap::new());
    crate::threads::register_with(&rec, threads.clone(), map.clone());
    let tr = Transcripts::new(
        store.clone(),
        dw.clone(),
        rec.clone(),
        wrap(threads.clone()),
        map.clone(),
        clock.clone(),
        None,
    );
    tr.register_with(&rec);
    let fx = Fx {
        _tmp: tmp,
        dir,
        deps,
        w,
        dw,
        store,
        journal,
        clock,
        herdr,
        rec,
        threads,
        map,
        tr,
    };
    setup(&fx);
    fx
}

fn plan_and_apply(fx: &Fx, change: &str) {
    let words = change.split_whitespace().map(str::to_owned).collect();
    let v = create_plan(&fx.deps, &CallerInfo::default(), words)
        .unwrap_or_else(|e| panic!("{change}: {}", e.message));
    let id: crate::model::PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
    let sp = fx.deps.plans.get(&id).unwrap().unwrap();
    let op = admit_apply(
        &fx.deps,
        &CallerInfo::default(),
        sp.plan.id.as_str(),
        Some(&sp.hash),
        "relay",
    )
    .unwrap_or_else(|e| panic!("admit {change}: {}", e.message));
    fx.w.drain().unwrap();
    let row = fx.w.journal().get(&op).unwrap().unwrap();
    assert_eq!(
        row.state,
        OpState::Committed,
        "{change}: {:?}",
        row.rejection
    );
}

fn admit(fx: &Fx, request: ChangeRequest) -> crate::model::OpId {
    let op = fx.dw.admit(request).unwrap();
    assert_eq!(
        fx.w.status(&op).unwrap(),
        Some(OpState::Committed),
        "{:?}",
        fx.w.journal().get(&op).unwrap().unwrap().rejection
    );
    op
}

fn view(fx: &Fx) -> crate::store::tree::CommitView<'_> {
    crate::store::tree::CommitView {
        store: &*fx.store,
        at: fx.store.head().unwrap(),
    }
}

fn seat(fx: &Fx, name: &str) -> SeatRecord {
    let mut found: Vec<_> = layout::all_seats(&view(fx))
        .unwrap()
        .into_iter()
        .filter(|(_, s)| s.name == name)
        .collect();
    assert_eq!(found.len(), 1, "seat {name}");
    found.remove(0).1
}

fn teamspace(fx: &Fx, name: &str) -> crate::model::teamspace::TeamspaceRecord {
    layout::list_teamspaces(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, t)| t)
        .find(|t| t.name == name)
        .unwrap()
}

fn clone_of(fx: &Fx, seat_name: &str) -> CloneRecord {
    let s = seat(fx, seat_name);
    let mut cs: Vec<_> = layout::all_clones(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, c)| c)
        .filter(|c| c.seat == s.id)
        .collect();
    assert_eq!(cs.len(), 1, "clone of {seat_name}");
    cs.remove(0)
}

fn clone_rec(fx: &Fx, id: &CloneId) -> CloneRecord {
    layout::all_clones(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, c)| c)
        .find(|c| &c.id == id)
        .unwrap()
}

fn all_requests(fx: &Fx) -> Vec<ProcessingRequest> {
    let mut v: Vec<_> = layout::list_requests(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, r)| r)
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

fn live_requests(fx: &Fx) -> Vec<ProcessingRequest> {
    all_requests(fx)
        .into_iter()
        .filter(|r| !is_merged(r))
        .collect()
}

fn the_request(fx: &Fx) -> ProcessingRequest {
    let mut v = live_requests(fx);
    assert_eq!(v.len(), 1, "{v:#?}");
    v.remove(0)
}

fn request(fx: &Fx, id: &RequestId) -> ProcessingRequest {
    all_requests(fx).into_iter().find(|r| &r.id == id).unwrap()
}

fn all_transcripts(fx: &Fx) -> Vec<TranscriptRecord> {
    layout::list_transcripts(&view(fx))
        .unwrap()
        .into_iter()
        .map(|(_, t)| t)
        .collect()
}

fn the_transcript(fx: &Fx) -> TranscriptRecord {
    let mut v = all_transcripts(fx);
    assert_eq!(v.len(), 1, "{v:#?}");
    v.remove(0)
}

async fn step(fx: &Fx) -> StepReport {
    fx.rec.step_fresh().await
}

async fn steps(fx: &Fx, n: usize) {
    for _ in 0..n {
        step(fx).await;
    }
}

fn rows(fx: &Fx, kind: EffectKind) -> Vec<EffectRecord> {
    let all = [
        EffectStatus::Pending,
        EffectStatus::Done,
        EffectStatus::Obsolete,
        EffectStatus::Failed,
        EffectStatus::Unknown,
        EffectStatus::NeedsRevision,
        EffectStatus::BlockedNeedsHuman,
    ];
    fx.journal
        .effects_with_status(&all)
        .unwrap()
        .into_iter()
        .filter(|r| r.kind == kind)
        .collect()
}

fn seat_thread(fx: &Fx, name: &str) -> ThreadRef {
    ThreadRef(
        seat(fx, name)
            .channel
            .thread_id
            .expect("seat channel ensured"),
    )
}

fn notifications_to(fx: &Fx, thread: &ThreadRef) -> Vec<crate::threads::fake::FakeNotification> {
    fx.threads
        .notifications()
        .into_iter()
        .filter(|n| &n.thread == thread)
        .collect()
}

const LINE: &str = "0123456789012345678\n";

/// A transcript of `lines` 20-byte lines in the fixture's Claude root; returns its path.
fn write_transcript(fx: &Fx, name: &str, lines: usize) -> std::path::PathBuf {
    let p = fx.dir.join(format!("{name}.jsonl"));
    std::fs::write(&p, LINE.repeat(lines)).unwrap();
    p
}

fn append_lines(path: &std::path::Path, lines: usize) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(LINE.repeat(lines).as_bytes()).unwrap();
}

/// Start a native session on the clone (the real `observed.occupancy` mutation).
fn start_session(
    fx: &Fx,
    clone: &CloneId,
    native: &str,
    path: Option<&std::path::Path>,
) -> crate::model::NsId {
    let cap = SessionCapture {
        harness: Harness::Shell,
        native_session_id: native.into(),
        transcript_path: path.map(Into::into),
        cwd: "/work".into(),
    };
    admit(
        fx,
        occupancy_request(clone, None, Some(&cap), fx.clock.now()),
    );
    let rec = clone_rec(fx, clone);
    rec.occupant.expect("session started").native_session
}

/// End the clone's current session (the real mutation) and return the event the observer would broadcast.
fn end_session(fx: &Fx, clone: &CloneId, reason: SessionEndReason) -> SessionEnded {
    let before = clone_rec(fx, clone);
    let ns = before
        .sessions
        .iter()
        .find(|s| Some(&s.id) == before.occupant.as_ref().map(|o| &o.native_session))
        .unwrap()
        .clone();
    let op = admit(
        fx,
        occupancy_request(clone, Some(reason), None, fx.clock.now()),
    );
    SessionEnded {
        clone: clone.clone(),
        seat: before.seat.clone(),
        ns: ns.id,
        harness: ns.harness,
        transcript_path: ns.transcript_path,
        reason,
        op,
    }
}

/// A teamspace `alpha` with the active shell seat `worker`, panes and channels created.
async fn base(fx: &Fx) -> CloneRecord {
    plan_and_apply(fx, "teamspace create alpha");
    plan_and_apply(
        fx,
        "seat create worker --teamspace alpha --active --harness shell",
    );
    steps(fx, 3).await;
    clone_of(fx, "worker")
}

/// A shell summarizer seat in `alpha` whose clone has an occupant.
async fn staffed_summarizer(fx: &Fx, name: &str) -> CloneRecord {
    plan_and_apply(
        fx,
        &format!("seat create {name} --teamspace alpha --active --harness shell --role summarizer"),
    );
    steps(fx, 3).await;
    let c = clone_of(fx, name);
    start_session(fx, &c.id, &format!("native-{name}"), None);
    c
}

/// Run one source session to its end and let the service create the request.
async fn finish_session(
    fx: &Fx,
    worker: &CloneRecord,
    native: &str,
    path: &std::path::Path,
) -> SessionEnded {
    start_session(fx, &worker.id, native, Some(path));
    let ev = end_session(fx, &worker.id, SessionEndReason::AgentExited);
    fx.tr.on_session_ended(ev.clone()).await;
    ev
}

/// Run one command through the exact path every IPC command takes: `_caller` on the wire becomes `CommandCtx`.
async fn dispatch(
    fx: &Fx,
    kind: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut reg = Registry::default();
    fx.tr.register_commands(&mut reg);
    match dispatch_registered(&reg, "r".into(), kind, args).await {
        IpcResult::Ok { value } => Ok(value),
        IpcResult::Error { code, message } => Err(format!("{code:?}: {message}")),
    }
}

async fn complete(fx: &Fx, args: serde_json::Value) -> Result<serde_json::Value, String> {
    dispatch(fx, "request.complete", args).await
}

async fn report_session(fx: &Fx, args: serde_json::Value) -> Result<serde_json::Value, String> {
    dispatch(fx, "session.report", args).await
}

fn complete_args(rq: &RequestId, covered: ByteRange, clone: Option<&CloneId>) -> serde_json::Value {
    let caller = CallerInfo {
        graph_clone: clone.map(|c| c.to_string()),
        ..Default::default()
    };
    json!({ "request": rq, "output": "summaries/out.md", "covered": covered, "_caller": caller })
}

// ---------------------------------------------------------------------------------------------
// request creation
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn session_end_creates_request_for_uncovered_range() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 5);
    finish_session(&fx, &worker, "native-1", &path).await;

    let tr = the_transcript(&fx);
    assert_eq!(
        (
            tr.transcript_path.clone(),
            tr.seat.clone(),
            tr.clone.clone()
        ),
        (path.clone(), worker.seat.clone(), worker.id.clone())
    );
    assert!(tr.source_seat_summaries_enabled_at_capture);
    assert!(tr.coverage.is_empty());
    assert_eq!(
        clone_rec(&fx, &worker.id).sessions[0].transcript,
        Some(tr.id.clone()),
        "the ns record links its transcript"
    );
    let rq = the_request(&fx);
    assert_eq!(
        (rq.status, rq.range, rq.transcript.clone()),
        (RequestStatus::Pending, br(0, 100), tr.id.clone())
    );

    // The same boundary again creates nothing: nothing new lies beyond the covered end.
    fx.tr
        .on_session_ended(end_session_event_clone(&fx, &worker, &path))
        .await;
    assert_eq!(live_requests(&fx).len(), 1);
}

/// A second `SessionEnded` for the session the first one described (a duplicate broadcast).
fn end_session_event_clone(fx: &Fx, worker: &CloneRecord, path: &std::path::Path) -> SessionEnded {
    let ns = clone_rec(fx, &worker.id).sessions[0].clone();
    SessionEnded {
        clone: worker.id.clone(),
        seat: worker.seat.clone(),
        ns: ns.id,
        harness: ns.harness,
        transcript_path: Some(path.to_path_buf()),
        reason: SessionEndReason::AgentExited,
        op: crate::model::OpId::new(),
    }
}

#[tokio::test]
async fn summaries_false_seat_creates_no_request() {
    let fx = fx();
    plan_and_apply(&fx, "teamspace create alpha");
    plan_and_apply(
        &fx,
        "seat create cron --teamspace alpha --active --harness shell --role system",
    );
    steps(&fx, 3).await;
    let c = clone_of(&fx, "cron");
    let path = write_transcript(&fx, "s1", 5);
    finish_session(&fx, &c, "native-1", &path).await;
    assert!(
        all_requests(&fx).is_empty(),
        "a role seat defaults to summaries = false"
    );
    assert!(all_transcripts(&fx).is_empty());
}

/// The crash window: the occupancy end is committed, the `SessionEnded` event never reached the service.
async fn crash_window(fx: &Fx, lines: usize) -> (CloneRecord, std::path::PathBuf, SessionEnded) {
    let worker = base(fx).await;
    let path = write_transcript(fx, "w", lines);
    start_session(fx, &worker.id, "s-1", Some(&path));
    let ev = end_session(fx, &worker.id, SessionEndReason::AgentExited);
    assert!(
        live_requests(fx).is_empty(),
        "the event was lost: no request yet"
    );
    (worker, path, ev)
}

#[tokio::test]
async fn lost_session_end_request_is_recovered_exactly_once() {
    let fx = fx();
    let (_, _, ev) = crash_window(&fx, 5).await;
    assert_eq!(fx.tr.recover_session_requests().await.unwrap(), 1);
    let live = live_requests(&fx);
    assert_eq!(live.len(), 1, "{live:#?}");
    assert_eq!(live[0].range, br(0, 100));
    assert_eq!(
        fx.tr.recover_session_requests().await.unwrap(),
        0,
        "steady state commits nothing"
    );
    assert_eq!(live_requests(&fx).len(), 1);
    fx.tr.on_session_ended(ev).await;
    assert_eq!(
        live_requests(&fx).len(),
        1,
        "the late original event dedups"
    );
}

#[tokio::test]
async fn recovery_skips_summaries_false_seat() {
    let fx = fx();
    plan_and_apply(&fx, "teamspace create alpha");
    plan_and_apply(
        &fx,
        "seat create cron --teamspace alpha --active --harness shell --role system",
    );
    steps(&fx, 3).await;
    let c = clone_of(&fx, "cron");
    let path = write_transcript(&fx, "s1", 5);
    start_session(&fx, &c.id, "native-1", Some(&path));
    end_session(&fx, &c.id, SessionEndReason::AgentExited);
    assert_eq!(fx.tr.recover_session_requests().await.unwrap(), 0);
    assert!(all_requests(&fx).is_empty());
}

#[tokio::test]
async fn recovery_requests_only_the_uncovered_tail() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 5);
    finish_session(&fx, &worker, "native-1", &path).await;
    let first = the_request(&fx);
    complete(&fx, complete_args(&first.id, first.range, Some(&worker.id)))
        .await
        .unwrap();
    assert_eq!(
        fx.tr.recover_session_requests().await.unwrap(),
        0,
        "fully covered"
    );
    append_lines(&path, 3);
    assert_eq!(
        fx.tr.recover_session_requests().await.unwrap(),
        0,
        "first observation: not settled yet"
    );
    assert_eq!(fx.tr.recover_session_requests().await.unwrap(), 1);
    let live = live_requests(&fx);
    assert_eq!(live.len(), 2, "{live:#?}");
    assert!(
        live.iter()
            .any(|r| r.id != first.id && r.range == br(100, 160))
    );
    assert_eq!(fx.tr.recover_session_requests().await.unwrap(), 0);
    assert_eq!(live_requests(&fx).len(), 2);
}

#[tokio::test]
async fn recovery_leaves_a_live_resumed_session_to_the_watcher() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 5);
    finish_session(&fx, &worker, "native-1", &path).await;
    let first = the_request(&fx);
    complete(&fx, complete_args(&first.id, first.range, Some(&worker.id)))
        .await
        .unwrap();
    // resumed on the same file and still appending
    start_session(&fx, &worker.id, "native-1", Some(&path));
    append_lines(&path, 3);
    assert_eq!(
        fx.tr.recover_session_requests().await.unwrap(),
        0,
        "the live occupant owns the file"
    );
    assert_eq!(fx.tr.recover_session_requests().await.unwrap(), 0);
    assert_eq!(live_requests(&fx).len(), 1);
    // the resumed session ends; its SessionEnded event is lost
    end_session(&fx, &worker.id, SessionEndReason::AgentExited);
    assert_eq!(
        fx.tr.recover_session_requests().await.unwrap(),
        0,
        "first observation: not settled yet"
    );
    assert_eq!(
        fx.tr.recover_session_requests().await.unwrap(),
        1,
        "settled: the tail is requested"
    );
    let live = live_requests(&fx);
    assert_eq!(live.len(), 2, "{live:#?}");
    assert!(
        live.iter()
            .any(|r| r.id != first.id && r.range == br(100, 160))
    );
    assert_eq!(
        fx.tr.recover_session_requests().await.unwrap(),
        0,
        "steady state"
    );
}

#[tokio::test]
async fn liveness_scan_runs_recovery() {
    let fx = fx();
    crash_window(&fx, 5).await;
    fx.tr.liveness_scan().await.unwrap();
    assert_eq!(live_requests(&fx).len(), 1);
}

#[tokio::test]
async fn disabling_summaries_keeps_pending_request() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 5);
    finish_session(&fx, &worker, "native-1", &path).await;
    let rq = the_request(&fx);

    plan_and_apply(&fx, "seat override worker --summaries false");
    fx.tr.liveness_scan().await.unwrap();
    assert_eq!(
        request(&fx, &rq.id).status,
        RequestStatus::Pending,
        "disabling affects new captures only"
    );

    // A later session end on a source seat with summaries disabled creates nothing new.
    append_lines(&path, 3);
    finish_session(&fx, &worker, "native-2", &path).await;
    assert_eq!(all_requests(&fx).len(), 1);
}

#[tokio::test]
async fn missing_transcript_is_unresolved_missing_input() {
    let fx = fx();
    let worker = base(&fx).await;
    let summ = staffed_summarizer(&fx, "sum").await;
    let gone = fx.dir.join("never-written.jsonl");
    finish_session(&fx, &worker, "native-1", &gone).await;
    let rq = the_request(&fx);
    assert_eq!(
        (rq.status, rq.unresolved.as_deref()),
        (RequestStatus::Unresolved, Some("missing_input"))
    );
    assert!(rq.range.is_empty());

    // Visible to the scan, never delivered, never done.
    let listed = requests::list_view(
        &view(&fx),
        ListFilter {
            unresolved: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        listed.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
        vec![rq.id.to_string()]
    );
    steps(&fx, 2).await;
    fx.tr.liveness_scan().await.unwrap();
    assert!(notifications_to(&fx, &seat_thread(&fx, "sum")).is_empty());
    assert_eq!(request(&fx, &rq.id).status, RequestStatus::Unresolved);
    let _ = summ;
}

#[tokio::test]
async fn codex_without_a_path_leaves_the_transcript_unresolved() {
    let fx = fx();
    let worker = base(&fx).await;
    finish_session(&fx, &worker, "native-1", &fx.dir.join("x")).await;
    // Same shape, but the session carries no path at all.
    let c2 = clone_rec(&fx, &worker.id);
    let ns = start_session(&fx, &c2.id, "native-2", None);
    let ev = end_session(&fx, &c2.id, SessionEndReason::AgentExited);
    assert_eq!(ev.ns, ns);
    fx.tr.on_session_ended(ev).await;
    let unresolved: Vec<_> = all_transcripts(&fx)
        .into_iter()
        .filter(|t| t.native_session == ns)
        .collect();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(
        unresolved[0].unresolved.as_deref(),
        Some("transcript_unresolved")
    );
    assert_eq!(
        live_requests(&fx)
            .iter()
            .filter(|r| r.transcript == unresolved[0].id)
            .count(),
        0,
        "no range to request"
    );
}

#[tokio::test]
async fn dedup_same_range_and_merge_overlapping() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 20);
    finish_session(&fx, &worker, "native-1", &path).await;
    let tr = the_transcript(&fx);
    let first = the_request(&fx);
    assert_eq!(first.range, br(0, 400));

    // Identical (tr, range): not duplicated.
    fx.tr
        .create_request_for_range(&tr.id, br(0, 400))
        .await
        .unwrap();
    assert_eq!(live_requests(&fx).len(), 1);

    // An overlapping pending request is merged into the oldest, which keeps its id and gets the union.
    fx.tr
        .create_request_for_range(&tr.id, br(300, 600))
        .await
        .unwrap();
    let live = live_requests(&fx);
    assert_eq!(live.len(), 1, "{live:#?}");
    assert_eq!(
        (live[0].id.clone(), live[0].range),
        (first.id.clone(), br(0, 600))
    );

    // Two disjoint pending requests bridged by a third: the oldest survives, the other points at it.
    let tr2_path = write_transcript(&fx, "s2", 30);
    let c = clone_rec(&fx, &worker.id);
    finish_session(&fx, &c, "native-2", &tr2_path).await;
    let tr2 = all_transcripts(&fx)
        .into_iter()
        .find(|t| t.transcript_path == tr2_path)
        .unwrap();
    let a = live_requests(&fx)
        .into_iter()
        .find(|r| r.transcript == tr2.id)
        .unwrap();
    assert_eq!(a.range, br(0, 600));
    fx.tr
        .create_request_for_range(&tr2.id, br(700, 800))
        .await
        .unwrap();
    assert_eq!(
        live_requests(&fx)
            .iter()
            .filter(|r| r.transcript == tr2.id)
            .count(),
        2
    );
    fx.tr
        .create_request_for_range(&tr2.id, br(500, 750))
        .await
        .unwrap();
    let mine: Vec<_> = all_requests(&fx)
        .into_iter()
        .filter(|r| r.transcript == tr2.id)
        .collect();
    let keeper = mine.iter().find(|r| r.id == a.id).unwrap();
    assert_eq!(
        (keeper.status, keeper.range),
        (RequestStatus::Pending, br(0, 800))
    );
    let merged: Vec<_> = mine.iter().filter(|r| is_merged(r)).collect();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].unresolved, Some(format!("merged_into:{}", a.id)));
    // Merged rows stay out of the scans.
    let listed = requests::list_view(&view(&fx), ListFilter::default()).unwrap();
    assert!(listed.iter().all(|r| r.id != merged[0].id.to_string()));
}

#[tokio::test]
async fn resume_append_creates_request_for_new_bytes_only() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 5);
    finish_session(&fx, &worker, "native-1", &path).await;
    let tr = the_transcript(&fx);
    let first = the_request(&fx);
    complete(&fx, complete_args(&first.id, first.range, Some(&worker.id)))
        .await
        .unwrap();
    assert_eq!(the_transcript(&fx).coverage, vec![br(0, 100)]);

    // The session is resumed on the same file (a new native session record), which grows.
    start_session(&fx, &worker.id, "native-1", Some(&path));
    fx.tr.watch_once().await.unwrap();
    assert_eq!(live_requests(&fx).len(), 1, "nothing new yet");
    append_lines(&path, 3);
    fx.tr.watch_once().await.unwrap();
    assert_eq!(
        live_requests(&fx).len(),
        1,
        "still being written: no boundary yet"
    );
    fx.tr.watch_once().await.unwrap();
    let live = live_requests(&fx);
    assert_eq!(live.len(), 2, "{live:#?}");
    let second = live.iter().find(|r| r.id != first.id).unwrap();
    assert_eq!(
        (second.range, second.transcript.clone(), second.status),
        (br(100, 160), tr.id.clone(), RequestStatus::Pending)
    );
    assert_eq!(
        all_transcripts(&fx).len(),
        1,
        "appends go to the existing transcript record"
    );
    fx.tr.watch_once().await.unwrap();
    assert_eq!(
        live_requests(&fx).len(),
        2,
        "the same bytes are not requested twice"
    );
}

#[tokio::test]
async fn replaced_transcript_file_starts_a_new_record() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 5);
    finish_session(&fx, &worker, "native-1", &path).await;
    start_session(&fx, &worker.id, "native-1", Some(&path));
    fx.tr.watch_once().await.unwrap();
    // The file is replaced by a different one at the same path (new inode).
    let other = fx.dir.join("other.jsonl");
    std::fs::write(&other, LINE.repeat(2)).unwrap();
    std::fs::rename(&other, &path).unwrap();
    fx.tr.watch_once().await.unwrap();
    let trs = all_transcripts(&fx);
    assert_eq!(trs.len(), 2, "{trs:#?}");
    let fresh = trs.iter().max_by(|a, b| a.id.cmp(&b.id)).unwrap();
    assert!(fresh.coverage.is_empty());
    assert!(
        live_requests(&fx)
            .iter()
            .any(|r| r.transcript == fresh.id && r.range == br(0, 40))
    );
}

#[tokio::test]
async fn clear_session_change_ends_previous_ns_and_requests() {
    let fx = fx();
    let worker = base(&fx).await;
    let first = write_transcript(&fx, "first", 4);
    let second = write_transcript(&fx, "second", 2);
    let report = |id: &str, path: &std::path::Path| {
        json!({
            "clone": worker.id,
            "capture": { "harness": "claude", "native_session_id": id, "transcript_path": path, "cwd": "/work" },
            "source": "startup",
        })
    };
    let r = report_session(&fx, report("sess-1", &first)).await.unwrap();
    assert_eq!(r["resolved"], true);
    assert!(
        all_requests(&fx).is_empty(),
        "a session that has not ended has no request"
    );
    let c = clone_rec(&fx, &worker.id);
    assert_eq!(c.sessions.len(), 1);
    assert_eq!(c.sessions[0].native_session_id, "sess-1");

    // Reporting the same session again changes nothing.
    let head = fx.store.head().unwrap();
    let same = report_session(&fx, report("sess-1", &first)).await.unwrap();
    assert_eq!(same["changed"], false);
    assert_eq!(fx.store.head().unwrap(), head);

    // /clear: a new session id on the same clone ends the previous ns and requests its transcript.
    let r = report_session(&fx, report("sess-2", &second))
        .await
        .unwrap();
    assert_eq!(r["changed"], true);
    let c = clone_rec(&fx, &worker.id);
    assert_eq!(c.sessions.len(), 2);
    assert_eq!(
        (c.sessions[0].end_reason, c.sessions[0].ended.is_some()),
        (Some(SessionEndReason::SessionChanged), true)
    );
    assert_eq!(
        c.occupant.as_ref().map(|o| o.native_session.clone()),
        Some(c.sessions[1].id.clone())
    );
    assert_eq!(
        c.sessions[1].transcript_path.as_deref(),
        Some(second.as_path())
    );
    let rq = the_request(&fx);
    assert_eq!(rq.range, br(0, 80));
    assert_eq!(the_transcript(&fx).transcript_path, first);
}

#[tokio::test]
async fn session_report_resolves_clone_by_pane_binding_when_env_absent() {
    let fx = fx();
    let worker = base(&fx).await;
    let pane = clone_rec(&fx, &worker.id)
        .runtime
        .bound
        .unwrap()
        .pane_id
        .unwrap();
    let path = write_transcript(&fx, "s1", 1);
    let capture = json!({ "harness": "claude", "native_session_id": "sess-9", "transcript_path": path, "cwd": "/work" });
    // The CLI passed neither clone: HERDR_PANE_ID reaches the daemon as the caller's pane.
    let caller = CallerInfo {
        pane_id: Some(pane.0.clone()),
        ..Default::default()
    };
    let r = report_session(&fx, json!({ "capture": capture, "_caller": caller }))
        .await
        .unwrap();
    assert_eq!(r["resolved"], true);
    let c = clone_rec(&fx, &worker.id);
    assert_eq!(
        c.sessions
            .iter()
            .map(|s| s.native_session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["sess-9"]
    );
    // The explicit pane argument resolves the same way.
    let other = json!({ "pane": pane.0, "capture": { "harness": "claude", "native_session_id": "sess-9", "cwd": "/work" } });
    assert_eq!(report_session(&fx, other).await.unwrap()["changed"], false);
}

#[tokio::test]
async fn session_report_noop_when_unresolved() {
    let fx = fx();
    base(&fx).await;
    let head = fx.store.head().unwrap();
    let capture = json!({ "harness": "claude", "native_session_id": "sess-x", "cwd": "/work" });
    let r = report_session(
        &fx,
        json!({ "pane": "no-such-pane", "capture": capture.clone() }),
    )
    .await
    .unwrap();
    assert_eq!(r["resolved"], false);
    let r = report_session(
        &fx,
        json!({ "clone": "cl_01ARZ3NDEKTSV4RRFFQ69G5FAV", "capture": capture.clone() }),
    )
    .await
    .unwrap();
    assert_eq!(r["resolved"], false, "an unknown clone id and no pane");
    let r = report_session(&fx, json!({ "capture": capture }))
        .await
        .unwrap();
    assert_eq!(r["resolved"], false);
    assert_eq!(
        fx.store.head().unwrap(),
        head,
        "an unresolved report writes nothing"
    );
}

fn spool_session(fx: &Fx, clone: &CloneId, native: &str) {
    let hook = capture::parse_claude_hook(&format!(
        r#"{{"session_id":"{native}","source":"startup"}}"#
    ))
    .unwrap();
    let args = capture::report_args(&hook, Some(&clone.to_string()), None, "/work".into()).unwrap();
    let r = capture::SpooledReport {
        version: 1,
        caller: CallerInfo::default(),
        args,
        spooled_at: fx.clock.now(),
    };
    capture::spool_report(fx.tr.reconciler.instance(), &r).unwrap();
}

fn spool_files(fx: &Fx) -> Vec<std::path::PathBuf> {
    let dir = capture::spool_dir(fx.tr.reconciler.instance());
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn occupant_native(fx: &Fx, clone: &CloneId) -> Option<String> {
    let c = clone_rec(fx, clone);
    let occ = c.occupant.as_ref()?;
    c.sessions
        .iter()
        .find(|s| s.id == occ.native_session)
        .map(|s| s.native_session_id.clone())
}

#[tokio::test]
async fn spooled_report_is_ingested_and_removed() {
    let fx = fx();
    let worker = base(&fx).await;
    spool_session(&fx, &worker.id, "s-spooled");
    assert_eq!(spool_files(&fx).len(), 1);
    assert_eq!(fx.tr.ingest_spool().await, 1);
    assert_eq!(
        occupant_native(&fx, &worker.id).as_deref(),
        Some("s-spooled")
    );
    assert!(spool_files(&fx).is_empty());
    assert_eq!(fx.tr.ingest_spool().await, 0, "nothing left to apply");
}

#[tokio::test]
async fn spooled_duplicate_of_processed_report_is_dropped() {
    let fx = fx();
    let worker = base(&fx).await;
    let live = |id: &str| json!({ "clone": worker.id, "capture": { "harness": "claude", "native_session_id": id, "cwd": "/work" } });
    fx.tr
        .session_report(CallerInfo::default(), live("s1"))
        .await
        .unwrap();
    spool_session(&fx, &worker.id, "s1");
    fx.tr
        .session_report(CallerInfo::default(), live("s2"))
        .await
        .unwrap();
    assert_eq!(occupant_native(&fx, &worker.id).as_deref(), Some("s2"));
    assert_eq!(fx.tr.ingest_spool().await, 0);
    assert!(spool_files(&fx).is_empty(), "the stale entry is deleted");
    assert_eq!(
        occupant_native(&fx, &worker.id).as_deref(),
        Some("s2"),
        "the stale entry did not end s2"
    );
}

#[tokio::test]
async fn live_report_ingests_older_spool_first() {
    let fx = fx();
    let worker = base(&fx).await;
    spool_session(&fx, &worker.id, "s1");
    let live = json!({ "clone": worker.id, "capture": { "harness": "claude", "native_session_id": "s2", "cwd": "/work" } });
    let r = fx
        .tr
        .session_report_live(CallerInfo::default(), live)
        .await
        .unwrap();
    assert_eq!(r["changed"], true);
    let c = clone_rec(&fx, &worker.id);
    let s1 = c
        .sessions
        .iter()
        .find(|s| s.native_session_id == "s1")
        .expect("s1 was ingested");
    assert_eq!(s1.end_reason, Some(SessionEndReason::SessionChanged));
    assert_eq!(occupant_native(&fx, &worker.id).as_deref(), Some("s2"));
    assert!(spool_files(&fx).is_empty());
}

#[tokio::test]
async fn malformed_spool_entry_is_quarantined_and_later_ones_still_apply() {
    let fx = fx();
    let worker = base(&fx).await;
    let dir = capture::spool_dir(fx.tr.reconciler.instance());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("00000000000000000000000000.json"), "not json").unwrap();
    spool_session(&fx, &worker.id, "s-good");
    assert_eq!(fx.tr.ingest_spool().await, 1);
    assert!(
        dir.join("rejected/00000000000000000000000000.json")
            .exists()
    );
    assert_eq!(occupant_native(&fx, &worker.id).as_deref(), Some("s-good"));
}

fn shell_capture(native: &str) -> SessionCapture {
    SessionCapture {
        harness: Harness::Shell,
        native_session_id: native.into(),
        transcript_path: None,
        cwd: "/work".into(),
    }
}

#[tokio::test]
async fn duplicate_start_report_is_a_noop() {
    let fx = fx();
    let worker = base(&fx).await;
    start_session(&fx, &worker.id, "s1", None);
    let at = fx.clock.now();
    admit(
        &fx,
        occupancy_request(
            &worker.id,
            Some(SessionEndReason::SessionChanged),
            Some(&shell_capture("s1")),
            at,
        ),
    );
    let c = clone_rec(&fx, &worker.id);
    assert_eq!(
        c.sessions.len(),
        1,
        "no phantom session for the same native id: {:?}",
        c.sessions
    );
    assert_eq!(occupant_native(&fx, &worker.id).as_deref(), Some("s1"));
    assert!(
        c.sessions[0].ended.is_none(),
        "the duplicate did not end the session it started"
    );
}

#[tokio::test]
async fn older_start_report_after_newer_occupant_is_dropped() {
    let fx = fx();
    let worker = base(&fx).await;
    let older = fx.clock.now();
    fx.clock.advance(chrono::Duration::seconds(10));
    start_session(&fx, &worker.id, "s2", None);
    admit(
        &fx,
        occupancy_request(
            &worker.id,
            Some(SessionEndReason::SessionChanged),
            Some(&shell_capture("s1")),
            older,
        ),
    );
    let c = clone_rec(&fx, &worker.id);
    assert_eq!(occupant_native(&fx, &worker.id).as_deref(), Some("s2"));
    assert!(
        c.sessions.iter().all(|s| s.native_session_id != "s1"),
        "{:?}",
        c.sessions
    );
    assert!(c.sessions[0].ended.is_none());
    assert!(all_requests(&fx).is_empty());
}

#[tokio::test]
async fn resume_of_an_ended_session_still_starts_a_new_session() {
    let fx = fx();
    let worker = base(&fx).await;
    start_session(&fx, &worker.id, "n1", None);
    end_session(&fx, &worker.id, SessionEndReason::AgentExited);
    start_session(&fx, &worker.id, "n1", None);
    let c = clone_rec(&fx, &worker.id);
    assert_eq!(
        c.sessions
            .iter()
            .filter(|s| s.native_session_id == "n1")
            .count(),
        2
    );
    let occ = c.occupant.as_ref().unwrap();
    assert_eq!(
        occ.native_session, c.sessions[1].id,
        "the occupant is the second session"
    );
}

/// Journal ops that start session `native` through `observed.occupancy`.
fn occupancy_ops_starting(fx: &Fx, native: &str) -> usize {
    fx.journal
        .list(&[], 1000)
        .unwrap()
        .into_iter()
        .filter(|r| {
            r.request.args["sub"] == "occupancy"
                && r.request.args["start"]["native_session_id"] == native
        })
        .count()
}

#[tokio::test]
async fn halted_writer_and_repeated_spool_passes_admit_one_occupancy_op() {
    let fx = fx();
    let worker = base(&fx).await;
    let tr = Transcripts::new(
        fx.store.clone(),
        fx.w.clone(),
        fx.rec.clone(),
        fx.threads.clone(),
        fx.map.clone(),
        fx.clock.clone(),
        None,
    );
    spool_session(&fx, &worker.id, "s1");
    fx.journal
        .meta_set(crate::writer::WRITER_HALTED, "test")
        .unwrap();
    assert_eq!(tr.ingest_spool().await, 0);
    assert_eq!(tr.ingest_spool().await, 0);
    assert_eq!(
        occupancy_ops_starting(&fx, "s1"),
        0,
        "a halted writer admits nothing"
    );

    fx.journal
        .meta_delete(crate::writer::WRITER_HALTED)
        .unwrap();
    for _ in 0..2 {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(200);
        assert_eq!(
            crate::daemon::budget::within(deadline, tr.ingest_spool()).await,
            0
        );
    }
    assert_eq!(
        occupancy_ops_starting(&fx, "s1"),
        1,
        "the second pass did not re-admit the report"
    );
    assert_eq!(spool_files(&fx).len(), 1, "uncommitted: the file stays");

    fx.w.drain().unwrap();
    assert_eq!(tr.ingest_spool().await, 1);
    assert!(spool_files(&fx).is_empty());
    assert_eq!(occupancy_ops_starting(&fx, "s1"), 1);
    let c = clone_rec(&fx, &worker.id);
    assert_eq!(
        c.sessions
            .iter()
            .filter(|s| s.native_session_id == "s1")
            .count(),
        1
    );
}

#[tokio::test]
async fn stale_live_report_after_newer_spooled_report_leaves_occupant() {
    let fx = fx();
    let worker = base(&fx).await;
    let hook = capture::parse_claude_hook(r#"{"session_id":"s2","source":"startup"}"#).unwrap();
    let args =
        capture::report_args(&hook, Some(&worker.id.to_string()), None, "/work".into()).unwrap();
    let later = fx.clock.now() + chrono::Duration::seconds(10);
    let r = capture::SpooledReport {
        version: 1,
        caller: CallerInfo::default(),
        args,
        spooled_at: later,
    };
    capture::spool_report(fx.tr.reconciler.instance(), &r).unwrap();
    assert_eq!(fx.tr.ingest_spool().await, 1);
    assert_eq!(occupant_native(&fx, &worker.id).as_deref(), Some("s2"));

    let live = json!({ "clone": worker.id, "capture": { "harness": "claude", "native_session_id": "s1", "cwd": "/work" } });
    let r = fx
        .tr
        .session_report(CallerInfo::default(), live)
        .await
        .unwrap();
    assert_eq!(r["changed"], false);
    let c = clone_rec(&fx, &worker.id);
    assert_eq!(occupant_native(&fx, &worker.id).as_deref(), Some("s2"));
    assert!(
        c.sessions.iter().all(|s| s.native_session_id != "s1"),
        "{:?}",
        c.sessions
    );
    assert!(c.sessions[0].ended.is_none());
    assert!(all_requests(&fx).is_empty());
}

#[test]
fn session_report_cli_is_a_noop_without_clone_or_pane() {
    let hook = capture::parse_claude_hook(
        r#"{"session_id":"abc","transcript_path":"/t/abc.jsonl","source":"clear","cwd":"/w","hook_event_name":"SessionStart"}"#,
    )
    .unwrap();
    assert_eq!(hook.session_id, "abc");
    assert!(capture::report_args(&hook, None, None, "/fallback".into()).is_none());
    let args = capture::report_args(&hook, None, Some("p1"), "/fallback".into()).unwrap();
    assert_eq!(args["pane"], "p1");
    assert_eq!(args["capture"]["transcript_path"], "/t/abc.jsonl");
    assert_eq!(args["capture"]["cwd"], "/w");
    assert_eq!(args["source"], "clear");
    let args = capture::report_args(
        &capture::parse_claude_hook(r#"{"session_id":"abc"}"#).unwrap(),
        Some("cl_x"),
        None,
        "/fallback".into(),
    )
    .unwrap();
    assert_eq!(args["capture"]["cwd"], "/fallback");
    assert!(capture::parse_claude_hook("not json").is_err());
}

// ---------------------------------------------------------------------------------------------
// destination
// ---------------------------------------------------------------------------------------------

fn destination(fx: &Fx, source: &str) -> Destination {
    let v = view(fx);
    let g = Graph::load(&v).unwrap();
    resolve_destination(&g, &v, &seat(fx, source).id).unwrap()
}

fn dest_seat(d: &Destination) -> Option<SeatId> {
    match d {
        Destination::Ready { seat, .. } | Destination::Relaunch { seat, .. } => Some(seat.clone()),
        Destination::Undeliverable { seat, .. } => seat.clone(),
    }
}

#[tokio::test]
async fn destination_oldest_summarizer_in_teamspace_else_graph_default() {
    let fx = fx();
    base(&fx).await;
    plan_and_apply(
        &fx,
        "seat create sum-a --teamspace alpha --active --harness shell --role summarizer",
    );
    plan_and_apply(
        &fx,
        "seat create sum-b --teamspace alpha --active --harness shell --role summarizer",
    );
    plan_and_apply(&fx, "teamspace create beta");
    plan_and_apply(
        &fx,
        "seat create other --teamspace beta --active --harness shell",
    );
    steps(&fx, 3).await;
    let (a, b) = (seat(&fx, "sum-a").id, seat(&fx, "sum-b").id);
    let oldest = a.clone().min(b.clone());
    assert_eq!(
        dest_seat(&destination(&fx, "worker")),
        Some(oldest.clone()),
        "oldest by id among the teamspace's summarizers"
    );

    // beta has no summarizer and graph.toml names none: nowhere to deliver.
    assert_eq!(
        destination(&fx, "other"),
        Destination::Undeliverable {
            seat: None,
            reason: "no_destination"
        }
    );
    // With the graph default set, beta uses it (a seat in another teamspace).
    admit(
        &fx,
        crate::transcripts::mutations::bookkeeping_request(
            "test_set_summarizer",
            json!({ "seat": b }),
        ),
    );
    assert_eq!(dest_seat(&destination(&fx, "other")), Some(b.clone()));
    // A teamspace summarizer still wins over the graph default.
    assert_eq!(dest_seat(&destination(&fx, "worker")), Some(oldest));
}

#[tokio::test]
async fn retired_summarizer_not_activated_request_pending() {
    let fx = fx();
    let worker = base(&fx).await;
    plan_and_apply(
        &fx,
        "seat create sum --teamspace alpha --active --harness shell --role summarizer",
    );
    steps(&fx, 3).await;
    let sum = seat(&fx, "sum").id;
    plan_and_apply(&fx, "seat retire sum");
    steps(&fx, 2).await;
    admit(
        &fx,
        crate::transcripts::mutations::bookkeeping_request(
            "test_set_summarizer",
            json!({ "seat": sum }),
        ),
    );
    assert_eq!(
        destination(&fx, "worker"),
        Destination::Undeliverable {
            seat: Some(sum.clone()),
            reason: "summarizer_retired"
        }
    );

    let path = write_transcript(&fx, "s1", 3);
    fx.herdr.clear_calls();
    finish_session(&fx, &worker, "native-1", &path).await;
    steps(&fx, 3).await;
    let rq = the_request(&fx);
    assert_eq!(
        (rq.status, rq.undeliverable.as_deref()),
        (RequestStatus::Pending, Some("summarizer_retired"))
    );
    assert_eq!(
        seat(&fx, "sum").lifecycle,
        Lifecycle::Retired,
        "retirement precedence: never reactivated"
    );
    assert!(rows(&fx, EffectKind::RelaunchOccupant).is_empty());
    assert!(rows(&fx, EffectKind::DeliverRequest).is_empty());
    assert!(
        !fx.herdr
            .calls()
            .iter()
            .any(|c| matches!(c, FakeCall::StartAgent(_))),
        "{:?}",
        fx.herdr.calls()
    );
}

#[tokio::test]
async fn absent_occupant_relaunched_via_reconciler_hook() {
    let fx = fx();
    let worker = base(&fx).await;
    plan_and_apply(
        &fx,
        "seat create sum --teamspace alpha --active --harness claude --role summarizer",
    );
    steps(&fx, 3).await;
    let sum_clone = clone_of(&fx, "sum");
    let pane = clone_rec(&fx, &sum_clone.id)
        .runtime
        .bound
        .unwrap()
        .pane_id
        .unwrap();
    fx.herdr.set_agent(&pane, None);
    assert!(clone_rec(&fx, &sum_clone.id).occupant.is_none());

    let path = write_transcript(&fx, "s1", 3);
    fx.herdr.clear_calls();
    finish_session(&fx, &worker, "native-1", &path).await;
    let relaunch = rows(&fx, EffectKind::RelaunchOccupant);
    assert_eq!(
        relaunch.len(),
        1,
        "the reconciler hook enqueued the relaunch"
    );
    assert_eq!(relaunch[0].object, sum_clone.id.to_any());
    assert_eq!(
        Some(relaunch[0].op.clone()),
        seat(&fx, "sum").activation.last_op,
        "authority: the op that made the seat active"
    );
    let rq = the_request(&fx);
    assert_eq!(
        (rq.status, rq.undeliverable.clone()),
        (RequestStatus::Pending, None),
        "waiting for the occupant, not undeliverable"
    );

    step(&fx).await;
    let starts = fx
        .herdr
        .calls()
        .into_iter()
        .filter(|c| matches!(c, FakeCall::StartAgent(s) if s.pane == pane))
        .count();
    assert_eq!(starts, 1, "{:?}", fx.herdr.calls());
    assert_eq!(
        rows(&fx, EffectKind::RelaunchOccupant)[0].status,
        EffectStatus::Done
    );
}

#[tokio::test]
async fn no_destination_is_undeliverable_and_notified_once() {
    let fx = fx();
    let worker = base(&fx).await;
    let ts_thread = ThreadRef(
        teamspace(&fx, "alpha")
            .channel
            .thread_id
            .expect("teamspace channel"),
    );
    let path = write_transcript(&fx, "s1", 3);
    finish_session(&fx, &worker, "native-1", &path).await;
    let rq = the_request(&fx);
    assert_eq!(
        (rq.status, rq.undeliverable.as_deref()),
        (RequestStatus::Pending, Some("no_destination"))
    );
    let told = notifications_to(&fx, &ts_thread);
    assert_eq!(told.len(), 1, "{told:#?}");
    assert!(told[0].body.contains(rq.id.as_str()) && told[0].severity == Severity::Warn);

    fx.tr.liveness_scan().await.unwrap();
    fx.tr.process_pending().await;
    steps(&fx, 2).await;
    assert_eq!(
        notifications_to(&fx, &ts_thread).len(),
        1,
        "told once, not on every pass"
    );
    assert!(request(&fx, &rq.id).delivery.attempts.is_empty());

    // The doctor-facing status names it.
    let status = fx.tr.status_json();
    assert_eq!(status["undeliverable"][0]["reason"], "no_destination");

    // A summarizer appears: the flag clears and the request is delivered.
    let sum = staffed_summarizer(&fx, "sum").await;
    steps(&fx, 3).await;
    fx.tr.process_pending().await;
    let rq = request(&fx, &rq.id);
    assert_eq!(rq.undeliverable, None);
    assert_eq!(rq.status, RequestStatus::Delivered, "{rq:#?}");
    let _ = sum;
}

// ---------------------------------------------------------------------------------------------
// delivery, ACK, completion
// ---------------------------------------------------------------------------------------------

/// Worker session finished, staffed summarizer present, request delivered through the reconciler.
async fn delivered(fx: &Fx, lines: usize) -> (CloneRecord, ProcessingRequest, std::path::PathBuf) {
    let worker = base(fx).await;
    staffed_summarizer(fx, "sum").await;
    steps(fx, 2).await;
    let path = write_transcript(fx, "s1", lines);
    finish_session(fx, &worker, "native-1", &path).await;
    steps(fx, 2).await;
    let rq = the_request(fx);
    (worker, rq, path)
}

#[tokio::test]
async fn fallback_delivery_notifies_summarizer_channel_with_rq_id() {
    let fx = fx();
    let (_, rq, path) = delivered(&fx, 5).await;
    assert_eq!(rq.status, RequestStatus::Delivered, "{rq:#?}");
    let sent = notifications_to(&fx, &seat_thread(&fx, "sum"));
    assert_eq!(sent.len(), 1, "{sent:#?}");
    assert_eq!(sent[0].severity, Severity::Warn);
    for needle in [
        rq.id.as_str(),
        path.to_str().unwrap(),
        "bytes 0-100",
        "from seat worker",
        &format!("herdr-graph request ack {}", rq.id),
        &format!("herdr-graph request complete {}", rq.id),
    ] {
        assert!(
            sent[0].body.contains(needle),
            "{needle:?} missing from {:?}",
            sent[0].body
        );
    }
    assert_eq!(sent[0].op_key, OpKey(format!("deliver:{}:0", rq.id)));
    assert_eq!(
        rq.delivery.message_id, None,
        "the fallback has no message id"
    );
    assert_eq!(rq.delivery.attempts.len(), 1);
    assert!(!rq.delivery.attempts[0].retry);
    assert_eq!(rows(&fx, EffectKind::DeliverRequest).len(), 1);
    steps(&fx, 3).await;
    assert_eq!(
        notifications_to(&fx, &seat_thread(&fx, "sum")).len(),
        1,
        "a delivered request is not sent again"
    );
}

#[tokio::test]
async fn ack_records_dispatched_not_completed() {
    let fx = fx();
    let (worker, rq, _) = delivered(&fx, 5).await;
    fx.clock.advance(chrono::Duration::minutes(1));
    fx.tr.cmd_ack(json!({ "request": rq.id })).await.unwrap();
    let acked = request(&fx, &rq.id);
    assert_eq!(acked.status, RequestStatus::Dispatched);
    assert_eq!(acked.delivery.dispatched_at, Some(fx.clock.now()));
    assert!(acked.result.is_none(), "an ACK is not a result");
    assert!(
        the_transcript(&fx).coverage.is_empty(),
        "an ACK covers nothing"
    );

    // Acknowledging twice keeps the first time; completion is a separate act.
    fx.clock.advance(chrono::Duration::minutes(5));
    fx.tr.cmd_ack(json!({ "request": rq.id })).await.unwrap();
    assert_eq!(
        request(&fx, &rq.id).delivery.dispatched_at,
        acked.delivery.dispatched_at
    );
    complete(&fx, complete_args(&rq.id, rq.range, Some(&worker.id)))
        .await
        .unwrap();
    assert_eq!(request(&fx, &rq.id).status, RequestStatus::Completed);
    // A late ACK never regresses a completed request.
    fx.tr.cmd_ack(json!({ "request": rq.id })).await.unwrap();
    assert_eq!(request(&fx, &rq.id).status, RequestStatus::Completed);
}

/// A writer that admits but never reports progress.
struct StuckWriter(Arc<dyn Writer>);

impl Writer for StuckWriter {
    fn admit(
        &self,
        request: ChangeRequest,
    ) -> Result<crate::model::OpId, crate::ports::writer::WriterError> {
        self.0.admit(request)
    }
    fn status(
        &self,
        _: &crate::model::OpId,
    ) -> Result<Option<OpState>, crate::ports::writer::WriterError> {
        Ok(Some(OpState::Admitted))
    }
}

#[tokio::test]
async fn commit_answers_still_running_at_the_request_deadline() {
    let fx = fx();
    let (_worker, rq, _) = delivered(&fx, 5).await;
    let stuck = Transcripts::new(
        fx.store.clone(),
        Arc::new(StuckWriter(fx.dw.clone())),
        fx.rec.clone(),
        fx.threads.clone(),
        fx.map.clone(),
        fx.clock.clone(),
        None,
    );
    let began = std::time::Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(200);
    let err = crate::daemon::budget::within(deadline, stuck.cmd_ack(json!({ "request": rq.id })))
        .await
        .unwrap_err();
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "{:?}",
        began.elapsed()
    );
    assert_eq!(
        err.code,
        crate::ipc::IpcErrorCode::StillRunning,
        "{}",
        err.message
    );
    assert!(
        err.message.contains("op_"),
        "the message names the op: {}",
        err.message
    );
}

#[tokio::test]
async fn complete_records_result_and_merges_coverage() {
    let fx = fx();
    let (_, rq, _) = delivered(&fx, 5).await;
    let sum_clone = clone_of(&fx, "sum");
    fx.clock.advance(chrono::Duration::hours(1));
    complete(&fx, complete_args(&rq.id, br(0, 60), Some(&sum_clone.id)))
        .await
        .unwrap();
    let done = request(&fx, &rq.id);
    let result = done.result.clone().unwrap();
    assert_eq!(done.status, RequestStatus::Completed);
    assert_eq!(result.output_ref, "summaries/out.md");
    assert_eq!(result.covered_range, br(0, 60));
    assert_eq!(result.reported_by, sum_clone.id.to_any());
    assert_eq!(result.at, fx.clock.now());
    let tr = the_transcript(&fx);
    assert_eq!(tr.coverage, vec![br(0, 60)]);
    assert_eq!(
        tr.gaps,
        vec![br(60, 100)],
        "the requested bytes the result did not cover stay visible"
    );

    // Completing the rest through a later result merges: no gap remains.
    complete(&fx, complete_args(&rq.id, br(60, 100), Some(&sum_clone.id)))
        .await
        .unwrap();
    let tr = the_transcript(&fx);
    assert_eq!((tr.coverage, tr.gaps), (vec![br(0, 100)], vec![]));

    // Reversed ranges are a usage error, unknown requests are rejected.
    let bad = json!({ "request": rq.id, "output": "o", "covered": { "start": 9, "end": 3 } });
    assert!(complete(&fx, bad).await.is_err());
    let unknown = complete_args(&RequestId::new(), br(0, 1), None);
    assert!(complete(&fx, unknown).await.is_err());
}

#[tokio::test]
async fn completion_by_summarizer_is_attributed_to_its_clone() {
    let fx = fx();
    let (worker, rq, _) = delivered(&fx, 2).await;
    let summarizer = clone_of(&fx, "sum");
    complete(&fx, complete_args(&rq.id, rq.range, Some(&summarizer.id)))
        .await
        .unwrap();
    let by = request(&fx, &rq.id).result.unwrap().reported_by;
    assert_eq!(
        by,
        summarizer.id.to_any(),
        "the summarizer clone reported it"
    );
    assert_ne!(by, worker.seat.to_any(), "not the source seat");
}

#[tokio::test]
async fn session_report_via_dispatch_resolves_caller_clone() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 1);
    let caller = CallerInfo {
        graph_clone: Some(worker.id.to_string()),
        ..Default::default()
    };
    let capture = json!({ "harness": "claude", "native_session_id": "sess-c", "transcript_path": path, "cwd": "/work" });
    let r = report_session(&fx, json!({ "capture": capture, "_caller": caller }))
        .await
        .unwrap();
    assert_eq!(
        (r["resolved"].clone(), r["clone"].clone()),
        (json!(true), json!(worker.id))
    );
    let sessions = clone_rec(&fx, &worker.id).sessions;
    assert_eq!(
        sessions
            .iter()
            .map(|s| s.native_session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["sess-c"]
    );
}

#[tokio::test]
async fn complete_without_caller_identity_attributes_to_the_source_seat() {
    let fx = fx();
    let (worker, rq, _) = delivered(&fx, 2).await;
    complete(&fx, complete_args(&rq.id, rq.range, None))
        .await
        .unwrap();
    assert_eq!(
        request(&fx, &rq.id).result.unwrap().reported_by,
        worker.seat.to_any()
    );
}

#[tokio::test]
async fn late_older_result_never_shrinks_coverage() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 10);
    finish_session(&fx, &worker, "native-1", &path).await;
    let tr = the_transcript(&fx);
    let a = the_request(&fx);
    assert_eq!(a.range, br(0, 200));
    // A second, later range of the same transcript (the session resumed and grew).
    fx.tr
        .create_request_for_range(&tr.id, br(200, 300))
        .await
        .unwrap();
    let b = live_requests(&fx)
        .into_iter()
        .find(|r| r.id != a.id)
        .unwrap();

    complete(&fx, complete_args(&b.id, br(200, 300), Some(&worker.id)))
        .await
        .unwrap();
    assert_eq!(the_transcript(&fx).coverage, vec![br(200, 300)]);
    complete(&fx, complete_args(&a.id, br(0, 200), Some(&worker.id)))
        .await
        .unwrap();
    assert_eq!(
        the_transcript(&fx).coverage,
        vec![br(0, 300)],
        "order does not matter"
    );
    // A late, narrower report about the older range cannot take anything back.
    complete(&fx, complete_args(&a.id, br(0, 50), Some(&worker.id)))
        .await
        .unwrap();
    let tr = the_transcript(&fx);
    assert_eq!(tr.coverage, vec![br(0, 300)]);
    assert_eq!(tr.gaps, vec![], "{:?}", tr.gaps);
}

#[tokio::test]
async fn unresolved_requests_cannot_be_acked_or_completed() {
    let fx = fx();
    let worker = base(&fx).await;
    finish_session(&fx, &worker, "native-1", &fx.dir.join("missing.jsonl")).await;
    let rq = the_request(&fx);
    let err = fx
        .tr
        .cmd_ack(json!({ "request": rq.id }))
        .await
        .unwrap_err();
    assert!(
        err.message.contains("request_unresolved"),
        "{}",
        err.message
    );
    assert!(
        complete(&fx, complete_args(&rq.id, br(0, 1), None))
            .await
            .is_err()
    );
    assert_eq!(request(&fx, &rq.id).status, RequestStatus::Unresolved);
}

#[tokio::test]
async fn request_list_filters() {
    let fx = fx();
    let (worker, rq, path) = delivered(&fx, 5).await;
    let list = |f: ListFilter| {
        requests::list_view(&view(&fx), f)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()
    };
    let id = rq.id.to_string();
    assert_eq!(
        list(ListFilter {
            pending: true,
            ..Default::default()
        }),
        vec![id.clone()]
    );
    assert_eq!(
        list(ListFilter {
            undispatched: true,
            ..Default::default()
        }),
        vec![id.clone()],
        "delivered, not ACKed"
    );
    assert_eq!(
        list(ListFilter {
            unresolved: true,
            ..Default::default()
        }),
        Vec::<String>::new()
    );
    let row = &requests::list_view(&view(&fx), ListFilter::default()).unwrap()[0];
    assert_eq!(row.destination, "sum");
    assert!(
        row.render().starts_with(&format!("{id}  delivered  tr_")),
        "{}",
        row.render()
    );
    assert!(row.render().contains("0-100  sum"));

    fx.tr.cmd_ack(json!({ "request": rq.id })).await.unwrap();
    assert_eq!(
        list(ListFilter {
            pending: true,
            ..Default::default()
        }),
        vec![id.clone()]
    );
    assert_eq!(
        list(ListFilter {
            pending: true,
            undispatched: true,
            unresolved: false
        }),
        Vec::<String>::new()
    );
    // pending ∪ unresolved
    finish_session(&fx, &worker, "native-2", &fx.dir.join("missing.jsonl")).await;
    let both = list(ListFilter {
        pending: true,
        unresolved: true,
        undispatched: false,
    });
    assert_eq!(both.len(), 2, "{both:?}");
    complete(&fx, complete_args(&rq.id, rq.range, Some(&worker.id)))
        .await
        .unwrap();
    assert_eq!(
        list(ListFilter {
            pending: true,
            ..Default::default()
        }),
        Vec::<String>::new()
    );
    let _ = path;
}

// ---------------------------------------------------------------------------------------------
// liveness
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn liveness_redelivers_undelivered() {
    let fx = fx();
    let worker = base(&fx).await;
    staffed_summarizer(&fx, "sum").await;
    steps(&fx, 2).await;
    fx.threads.disconnect();
    let path = write_transcript(&fx, "s1", 5);
    finish_session(&fx, &worker, "native-1", &path).await;
    steps(&fx, 2).await;
    let rq = the_request(&fx);
    assert_eq!(
        rq.status,
        RequestStatus::Pending,
        "threads is down: nothing was delivered"
    );
    fx.tr.liveness_scan().await.unwrap();
    assert_eq!(the_request(&fx).status, RequestStatus::Pending);

    fx.threads.reconnect();
    fx.clock.advance(chrono::Duration::minutes(10));
    fx.tr.liveness_scan().await.unwrap();
    let rq = the_request(&fx);
    assert_eq!(rq.status, RequestStatus::Delivered);
    assert_eq!(notifications_to(&fx, &seat_thread(&fx, "sum")).len(), 1);
    // Scanning again does not deliver twice.
    fx.clock.advance(chrono::Duration::minutes(10));
    fx.tr.liveness_scan().await.unwrap();
    assert_eq!(notifications_to(&fx, &seat_thread(&fx, "sum")).len(), 1);
    assert_eq!(the_request(&fx).delivery.attempts.len(), 1);
}

#[tokio::test]
async fn liveness_reminds_unacked_after_30min_without_resend() {
    let fx = fx();
    let (_, rq, _) = delivered(&fx, 5).await;
    let thread = seat_thread(&fx, "sum");
    fx.clock.advance(chrono::Duration::minutes(29));
    fx.tr.liveness_scan().await.unwrap();
    assert_eq!(
        notifications_to(&fx, &thread).len(),
        1,
        "too early for a reminder"
    );

    fx.clock.advance(chrono::Duration::minutes(2));
    fx.tr.liveness_scan().await.unwrap();
    let sent = notifications_to(&fx, &thread);
    assert_eq!(sent.len(), 2, "{sent:#?}");
    assert!(
        sent[1].body.contains("reminder") && sent[1].body.contains(rq.id.as_str()),
        "{}",
        sent[1].body
    );
    assert_eq!(sent[1].severity, Severity::Warn);
    assert_ne!(
        sent[1].op_key, sent[0].op_key,
        "the reminder is its own notification, not a re-send"
    );
    let after = request(&fx, &rq.id);
    assert_eq!(
        after.delivery.attempts.len(),
        1,
        "the original delivery is never repeated"
    );
    assert_eq!(after.delivery.reminded_at, Some(fx.clock.now()));
    assert_eq!(after.status, RequestStatus::Delivered);

    fx.clock.advance(chrono::Duration::minutes(10));
    fx.tr.liveness_scan().await.unwrap();
    assert_eq!(
        notifications_to(&fx, &thread).len(),
        2,
        "one reminder per delivery"
    );
}

#[tokio::test]
async fn liveness_retries_dispatched_after_6h_with_fresh_key() {
    let fx = fx();
    let (_, rq, _) = delivered(&fx, 5).await;
    let thread = seat_thread(&fx, "sum");
    fx.tr.cmd_ack(json!({ "request": rq.id })).await.unwrap();
    let acked_at = fx.clock.now();

    fx.clock
        .advance(chrono::Duration::hours(5) + chrono::Duration::minutes(59));
    fx.tr.liveness_scan().await.unwrap();
    assert_eq!(
        notifications_to(&fx, &thread).len(),
        1,
        "acknowledged and not yet 6 h old: left alone"
    );
    assert_eq!(request(&fx, &rq.id).status, RequestStatus::Dispatched);

    fx.clock.advance(chrono::Duration::minutes(2));
    fx.tr.liveness_scan().await.unwrap();
    let after = request(&fx, &rq.id);
    assert_eq!(after.delivery.attempts.len(), 2);
    let retry = &after.delivery.attempts[1];
    assert!(retry.retry);
    assert_eq!(
        retry.op_key,
        format!("deliver:{}:1", rq.id),
        "a fresh op key per attempt"
    );
    assert_ne!(retry.op_key, after.delivery.attempts[0].op_key);
    assert_eq!(
        after.status,
        RequestStatus::Delivered,
        "a retry needs a fresh ACK"
    );
    assert_eq!(after.delivery.dispatched_at, None);
    let sent = notifications_to(&fx, &thread);
    assert_eq!(sent.len(), 2);
    assert!(sent[1].body.starts_with("RETRY "), "{}", sent[1].body);
    assert!(fx.clock.now() - acked_at > chrono::Duration::hours(6));
}

#[tokio::test]
async fn watcher_and_liveness_never_touch_completed_or_unresolved() {
    let fx = fx();
    let (worker, rq, _) = delivered(&fx, 5).await;
    complete(&fx, complete_args(&rq.id, rq.range, Some(&worker.id)))
        .await
        .unwrap();
    finish_session(&fx, &worker, "native-2", &fx.dir.join("gone.jsonl")).await;
    let before = fx.threads.notifications().len();
    fx.clock.advance(chrono::Duration::hours(24));
    fx.tr.liveness_scan().await.unwrap();
    fx.tr.watch_once().await.unwrap();
    assert_eq!(fx.threads.notifications().len(), before);
    let statuses: Vec<_> = live_requests(&fx).iter().map(|r| r.status).collect();
    assert!(
        statuses.contains(&RequestStatus::Completed)
            && statuses.contains(&RequestStatus::Unresolved),
        "{statuses:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// daemon wiring
// ---------------------------------------------------------------------------------------------

#[test]
fn mutations_register_once_each() {
    let mut reg = MutationRegistry::default();
    register_mutations(&mut reg);
    assert_eq!(
        reg.keys(),
        vec![
            "bookkeeping.request_ack",
            "bookkeeping.request_complete",
            "bookkeeping.request_create",
            "bookkeeping.request_delivery",
            "bookkeeping.request_unresolved",
            "bookkeeping.transcript",
        ]
    );
}

#[tokio::test]
async fn commands_and_loops_register_without_clashing() {
    let fx = fx();
    let mut reg = Registry::default();
    fx.tr.register_commands(&mut reg);
    fx.tr.register_loops(&mut reg);
    assert_eq!(
        reg.command_kinds(),
        vec![
            "request.ack",
            "request.complete",
            "request.list",
            "session.report"
        ]
    );
    assert_eq!(reg.take_loops().len(), 4);
    assert!(reg.status_components().contains_key("transcripts"));
    // The list command answers from the committed tree.
    let h = reg.handler("request.list").unwrap();
    let cx = CommandCtx {
        request_id: "r".into(),
        caller: CallerInfo::default(),
    };
    let v = h.call(cx, json!({ "pending": true })).await.unwrap();
    assert_eq!(v["requests"], json!([]));
}

#[tokio::test]
async fn session_ended_loop_consumes_observer_events() {
    let tmp_fx = fx();
    let worker = base(&tmp_fx).await;
    // A second service wired to a broadcast channel, as the daemon does with the observer's.
    let (tx, rx) = broadcast::channel(8);
    let tr = Transcripts::new(
        tmp_fx.store.clone(),
        tmp_fx.dw.clone(),
        tmp_fx.rec.clone(),
        tmp_fx.threads.clone(),
        tmp_fx.map.clone(),
        tmp_fx.clock.clone(),
        Some(rx),
    );
    let path = write_transcript(&tmp_fx, "s1", 3);
    start_session(&tmp_fx, &worker.id, "native-1", Some(&path));
    let ev = end_session(&tmp_fx, &worker.id, SessionEndReason::PaneClosed);
    let (stop, shutdown) = crate::daemon::registry::shutdown_channel();
    let handle = tokio::spawn(tr.clone().run_session_ended(shutdown));
    tx.send(ev).unwrap();
    // The loop runs on the same thread: give it a few turns.
    for _ in 0..200 {
        if !all_requests(&tmp_fx).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(the_request(&tmp_fx).range, br(0, 60));
    stop.send(true).unwrap();
    handle.await.unwrap().unwrap();
}

// ---------------------------------------------------------------------------------------------
// service-ack delivery
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "threads-service-ack")]
mod service_ack {
    use super::*;
    use std::sync::Mutex;

    /// FakeThreads plus the ServiceAck capability: `send_request` records its recipients and returns a
    /// message id; receipts stay pending until the test acknowledges.
    type Sent = (ThreadRef, Vec<ThreadsSeatRef>, String, OpKey);

    pub struct AckThreads {
        pub inner: Arc<FakeThreads>,
        pub sent: Mutex<Vec<Sent>>,
        pub acked: Mutex<Vec<(String, ThreadsSeatRef, Timestamp)>>,
    }

    #[async_trait::async_trait]
    impl ThreadsPort for AckThreads {
        async fn ensure_thread(
            &self,
            s: ChannelScope,
            t: &str,
            k: &OpKey,
        ) -> Result<ThreadRef, ThreadsError> {
            self.inner.ensure_thread(s, t, k).await
        }
        async fn invite(
            &self,
            t: &ThreadRef,
            s: &ThreadsSeatRef,
            c: crate::model::clone::InviteConstraint,
            k: &OpKey,
        ) -> Result<(), ThreadsError> {
            self.inner.invite(t, s, c, k).await
        }
        async fn membership(
            &self,
            t: &ThreadRef,
            s: &ThreadsSeatRef,
        ) -> Result<Option<crate::model::clone::InvitationState>, ThreadsError> {
            self.inner.membership(t, s).await
        }
        async fn notify(
            &self,
            t: &ThreadRef,
            sev: Severity,
            b: &str,
            k: &OpKey,
        ) -> Result<(), ThreadsError> {
            self.inner.notify(t, sev, b, k).await
        }
        async fn set_topic(
            &self,
            t: &ThreadRef,
            topic: &str,
            k: &OpKey,
        ) -> Result<(), ThreadsError> {
            self.inner.set_topic(t, topic, k).await
        }
        async fn release_requirement(
            &self,
            t: &ThreadRef,
            s: &ThreadsSeatRef,
            k: &OpKey,
        ) -> Result<(), ThreadsError> {
            self.inner.release_requirement(t, s, k).await
        }
        async fn send_request(
            &self,
            thread: &ThreadRef,
            recipients: &[ThreadsSeatRef],
            body: &str,
            op_key: &OpKey,
        ) -> Result<MessageRef, ThreadsError> {
            let mut sent = self.sent.lock().unwrap();
            sent.push((
                thread.clone(),
                recipients.to_vec(),
                body.to_owned(),
                op_key.clone(),
            ));
            Ok(MessageRef(format!("msg-{}", sent.len())))
        }
        async fn receipt_state(
            &self,
            messages: &[MessageRef],
        ) -> Result<Vec<MessageReceipts>, ThreadsError> {
            let acked = self.acked.lock().unwrap();
            Ok(messages
                .iter()
                .map(|m| MessageReceipts {
                    message: m.clone(),
                    recipients: acked
                        .iter()
                        .filter(|(id, _, _)| id == &m.0)
                        .map(|(_, seat, at)| RecipientReceipt {
                            seat: seat.clone(),
                            state: ReceiptState::Acknowledged { at: *at },
                        })
                        .chain(std::iter::once(RecipientReceipt {
                            seat: ThreadsSeatRef("other".into()),
                            state: ReceiptState::Pending,
                        }))
                        .collect(),
                })
                .collect())
        }
        async fn delivery_capability(&self) -> Result<DeliveryCapability, ThreadsError> {
            Ok(DeliveryCapability::ServiceAck)
        }
    }

    #[tokio::test]
    async fn service_ack_delivery_records_message_id_and_receipt() {
        let ack: Arc<std::sync::OnceLock<Arc<AckThreads>>> = Default::default();
        let slot = ack.clone();
        let fx = fx_with(
            move |fake| {
                let t = Arc::new(AckThreads {
                    inner: fake,
                    sent: Default::default(),
                    acked: Default::default(),
                });
                slot.set(t.clone()).ok();
                t
            },
            |_| {},
        );
        let ack = ack.get().unwrap().clone();
        let worker = base(&fx).await;
        let sum = staffed_summarizer(&fx, "sum").await;
        // The summarizer's pane is a threads seat.
        let pane = clone_rec(&fx, &sum.id)
            .runtime
            .bound
            .unwrap()
            .pane_id
            .unwrap();
        fx.map.set(&pane, "seat-S");
        steps(&fx, 2).await;
        let path = write_transcript(&fx, "s1", 5);
        finish_session(&fx, &worker, "native-1", &path).await;
        steps(&fx, 2).await;

        let rq = the_request(&fx);
        assert_eq!(rq.status, RequestStatus::Delivered, "{rq:#?}");
        let sent = ack.sent.lock().unwrap().clone();
        assert_eq!(sent.len(), 1, "{sent:#?}");
        assert_eq!(sent[0].0, seat_thread(&fx, "sum"));
        assert_eq!(
            sent[0].1,
            vec![ThreadsSeatRef("seat-S".into())],
            "the summarizer clones' threads seats"
        );
        assert!(sent[0].2.contains(rq.id.as_str()));
        assert_eq!(sent[0].3, OpKey(format!("deliver:{}:0", rq.id)));
        assert_eq!(rq.delivery.message_id.as_deref(), Some("msg-1"));
        assert_eq!(rq.delivery.attempts[0].message_id.as_deref(), Some("msg-1"));
        assert!(
            notifications_to(&fx, &seat_thread(&fx, "sum")).is_empty(),
            "no Notify fallback when the ACK path works"
        );

        // No receipt yet: still delivered.
        fx.tr.liveness_scan().await.unwrap();
        assert_eq!(the_request(&fx).status, RequestStatus::Delivered);
        // The summarizer's ACK arrives as a receipt: dispatched, still not success.
        let at = fx.clock.now() + chrono::Duration::minutes(3);
        ack.acked
            .lock()
            .unwrap()
            .push(("msg-1".into(), ThreadsSeatRef("seat-S".into()), at));
        fx.tr.liveness_scan().await.unwrap();
        let rq = the_request(&fx);
        assert_eq!(
            (rq.status, rq.delivery.dispatched_at),
            (RequestStatus::Dispatched, Some(at))
        );
        assert!(rq.result.is_none());
        assert!(the_transcript(&fx).coverage.is_empty());

        // A 6 h retry sends again under a fresh key and records the new message id.
        fx.clock.advance(chrono::Duration::hours(7));
        fx.tr.liveness_scan().await.unwrap();
        let rq = the_request(&fx);
        assert_eq!(rq.delivery.message_id.as_deref(), Some("msg-2"));
        assert_eq!(
            ack.sent.lock().unwrap()[1].3,
            OpKey(format!("deliver:{}:1", rq.id))
        );
        assert!(rq.delivery.attempts[1].retry);
    }
}

fn replace_file(fx: &Fx, path: &std::path::Path, lines: usize) {
    let other = fx.dir.join("replacement.jsonl");
    std::fs::write(&other, LINE.repeat(lines)).unwrap();
    std::fs::remove_file(path).unwrap();
    std::fs::rename(&other, path).unwrap();
}

#[tokio::test]
async fn concurrent_reports_for_replaced_file_create_one_transcript() {
    let fx = fx();
    let worker = base(&fx).await;
    let path = write_transcript(&fx, "s1", 5);
    let ev = finish_session(&fx, &worker, "native-1", &path).await;
    assert_eq!(all_transcripts(&fx).len(), 1);
    replace_file(&fx, &path, 2);
    let report = || {
        fx.tr
            .request_for(&ev.clone, &ev.ns, Some(path.clone()), None, false)
    };
    let (a, b) = tokio::join!(report(), report());
    a.unwrap();
    b.unwrap();
    let trs = all_transcripts(&fx);
    assert_eq!(
        trs.iter().filter(|t| t.transcript_path == path).count(),
        2,
        "one old plus exactly one new: {trs:#?}"
    );
}

#[test]
fn note_identity_reports_a_replaced_file_to_exactly_one_caller() {
    let fx = fx();
    let path = write_transcript(&fx, "s1", 3);
    let state = requests::stat_file(&path).unwrap();
    assert!(
        !fx.tr.note_identity(&path, &state),
        "first sighting is not a change"
    );
    assert!(
        !fx.tr.note_identity(&path, &state),
        "same file is not a change"
    );
    replace_file(&fx, &path, 3);
    let state = requests::stat_file(&path).unwrap();
    let barrier = std::sync::Barrier::new(8);
    let changed = std::thread::scope(|s| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                s.spawn(|| {
                    barrier.wait();
                    fx.tr.note_identity(&path, &state)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|c| *c)
            .count()
    });
    assert_eq!(changed, 1);
}
