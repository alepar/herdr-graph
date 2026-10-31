# Final review: cluster 13 (src-transcripts), epic hg-45e

Invocation: inv-p3b-20261031T222244. Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify <sha>^{commit}`). Review source bound to the ledger before this write (bind-review-source exit 0).

Caller checkout observed: HEAD bce6589afead4c031ac9ecfa3a094c9a5c4be161, `git status --short` empty at start and at review time.

Inputs: slices/000075.diff (src/transcripts/mod.rs), slices/000076.diff (src/transcripts/mutations.rs), slices/000077.diff (src/transcripts/tests.rs), digest.md, and spec docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md §1 (Summary API) and acceptance criterion 2. src/cli/request.rs and src/model/b64.rs were opened only to check the transport and decode paths. All three cluster files come from a single commit (64bcec1); `git diff ce06b29 bce6589 -- src/transcripts/` is empty, so the code is exactly what the hg-45e.8 task review saw.

## Spec compliance

- §1.1 command shape: `cmd_complete` takes exactly one of `output` / `summary_b64` (bad_request otherwise), and the `--output` path builds the same bookkeeping args as before (`output_ref`). Met.
- §1.2 path derivation: `write_summary` runs inside the `bookkeeping.request_complete` mutation at apply time and resolves request -> `find_transcript_by_id` -> `tr.seat` -> `cx.tree.locate`. It writes `<seat folder>/summaries/<tr>-<start>-<end>.md` and sets `output_ref` to the seat-relative `summaries/...` path. The caller cannot influence the path: `tr.id` is a typed id, the range is two integers, and `join` validates. Live and archived seats are both tested. Met.
- §1.3 idempotency/conflicts: if the request is already Completed with the same range and the file holds identical bytes, the mutation returns before any record write. A Completed request with a different range is rejected `summary_conflict`. Writing over an existing file with different bytes is rejected `summary_conflict`, and the file is never overwritten. `request_unresolved` is checked before any summary handling. Met.
- §1.4 transport/category: the summary travels as `summary_b64` through the shared `crate::model::b64` module. The bytes are decoded once at admission (bad_request on malformed base64) and again in the mutation. Non-UTF-8 is rejected `invalid_summary` at apply time. The request stays bookkeeping, and `reported_by` attribution is unchanged. Met.
- Acceptance criterion 2 (write into the live or archived seat, one commit, identical retry is a no-op, different bytes rejected): covered by `summary_from_writes_into_the_live_seat_folder_and_completes_in_one_commit`, which checks that neither the file nor the completion exists at the parent commit, and by `summary_for_an_archived_seat_lands_in_its_archive_folder` and `summary_retry_identical_is_noop_and_conflicts_reject`. The golden-sweep half of the criterion is in tests/integration_sweep.rs, which belongs to another cluster.

## Findings

No blocking defects found. Minor observations:

- Adoption/misleading message: suppose the request is not yet Completed and the derived file already exists with identical bytes. This can happen to an in-flight legacy run where `content write` committed the file but the separate `request complete --output` never ran. `write_summary` then rejects `summary_conflict` with the text "already exists ... with different content", which is false. The spec only defines conflict for different bytes. Recovery is possible with `--output summaries/...`, so this does not block landing. Adopting the identical file, or at least correcting the message, would be cleaner.
- Re-completing a request that was completed via `--output` (no file at the derived path) with `--summary-from` for the same range writes the file and replaces `result` (new `output_ref`, new `at`). The spec is silent on this, and the hg-45e.8 task review already noted and deferred it. It is the same latitude `--output` re-completion already has.
- Style: `output_ref: output_ref` in `RequestResult` construction is redundant field-init (clippy `redundant_field_names`). This is not part of the required verification commands.

## Test evidence

Per task-3-report.md (hg-45e.8): `cargo test --locked --offline transcripts::tests` -> 66 passed, including the 6 new summary_* tests. The test diff is additions only, with no loosened assertions. Not rerun by this reviewer: static review only, and the code is unchanged since the task review.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- Request not yet Completed with the derived summary file already present at identical bytes (legacy in-flight content-write case): currently rejected summary_conflict with an inaccurate "different content" message; no test pins either behavior
- Request completed via --output then re-completed via --summary-from for the same range (no file at derived path): writes and re-stamps the result; no test
- Malformed summary_b64 at admission (bad_request "summary_b64: ...") and a request carrying neither or both output and summary_b64 at the mutation layer (MutationError::Bug path) have no direct tests

## Deferred OK
- Adopt an identical pre-existing summary file for a not-yet-completed request (or correct the "different content" wording) to smooth the legacy content-write migration
- Decide and document whether --summary-from may re-complete a request previously completed via --output for the same range
- Replace `output_ref: output_ref` with field-init shorthand in RequestComplete
