# Task 2 review — 7a87863..71a85ba

Spec verdict: **compliant for the requested layout, attribution, and mixed-layout mechanics**. No missing path or attribution requirement was found. The quality issue below must be addressed before integration; it affects compatibility and availability beyond the happy-path layout checks.

Quality verdict: **needs fixes** — one Important finding. No Critical or Minor findings.

## Important — targeted ID lookup now makes every write depend on all operation history

**Location:** `src/store/layout.rs:300-303`, especially the Operation arm at line 303; full record parsing occurs at lines 157-167.

Replacing filename-based lookup with `find_by_id(list_operations(tr), id)` affects more than history readers. Every successful mutation writes a fresh `OperationRecord` through `WriterCore::write_operation_record` (`src/writer/mod.rs:535-536`). `Overlay::put_record` calls `base_info` (`src/store/tree.rs:129-131`), which calls `Store::locate` for that brand-new ID (`src/store/tree.rs:102-106`). Lookup now reads and deserializes every operation record in both legacy and new roots before it can report that the new ID is absent.

Two concrete consequences follow:

1. A malformed, unrelated historical operation blocks **every otherwise valid new mutation**, including changes that do not read or alter that historical record. The writer turns this into a terminal failed operation (`src/writer/mod.rs:259-262`). Previously lookup checked candidate filenames for the new ID and did not deserialize unrelated operations. This is a new instance-wide failure dependency, not merely the explicit failure of a history-list command on corrupt data.
2. Even entirely valid instances deserialize their entire accumulated operation history on each new operation. The cache is keyed by commit and queried ID (`src/store/git.rs:176-199`); both change between fresh operations, so it does not eliminate this work. The added record-parsing work grows linearly per write and quadratically across a growing history. This affects ordinary bookkeeping as well as user changes. No wall-clock threshold or production slowdown is claimed without a benchmark.

**Focused confirmation:** built a standalone Rust harness against the existing validated `libherdr_graph-682553607b505cb0.rlib`, with an in-memory `Store` implementation delegating lookup to production `layout::locate`, and invoked production `Overlay::put_record`. It touched no instance, Git ref, repository source, external service, or daemon.

- Fixture A had one unrelated legacy `operations/2026-03/<other-op>.toml` containing `schema = 1\nbroken = [`. Inserting a distinct fresh operation returned `StoreError::Corrupt` naming that old file. The assertion passed.
- Fixture B had 100 valid, unrelated legacy operations. Inserting a distinct fresh operation performed exactly **100 record blob reads**. The assertion passed.
- Harness executable: `/tmp/herdr-graph-task2-review-lookup`; execution exited zero. The first direct `rustc` link attempt lacked the native libgit2 search path; rerunning with `/opt/homebrew/opt/libgit2/lib` resolved that harness setup issue.

**Requested fix:** keep dual-layout discovery and duplicate-ID rejection, but separate targeted lookup/new-record collision checks from full history deserialization. Inspect all supported candidate locations for the requested ID and reject ambiguous matches; retain full parsing/validation in enumeration or another explicit integrity check. An indexed alternative is also acceptable if it avoids reparsing complete history per mutation and does not let an unrelated malformed operation block unrelated writes. Preserve explicit errors for a corrupt targeted record and duplicate targeted IDs.

**Regression expectations:** a fresh unrelated operation can be written with an unrelated malformed historical operation present; accessing that malformed record and full enumeration still report corruption; a fresh insertion does not read every unrelated valid operation blob; duplicate IDs in supported old/new paths still fail lookup and enumeration. The same targeted-lookup distinction should be considered for request, transcript, and action arms changed alongside operations.

The implementation report candidly mentions full enumeration and corrupt-record failure. It does not account for the fact that `put_record` uses lookup on every new operation, expanding this limitation to the complete write path. Existing green suites do not exercise that case.

## Spec and integration assessment

- New requests, actions, and operations use the exact requested `mutations/` roots, retaining the existing path-helper signatures.
- New transcript registration captures the initial team ID and team/seat display names and uses safe slugs with full stable IDs in the two-level grouping. Shared slugification bounds names and handles unsafe or empty input.
- Optional capture attribution preserves legacy deserialization. Existing transcript refresh and processing keep absence unknown; delivery labels explicitly identify unknown legacy capture names rather than using renamed current seats.
- Enumeration covers both supported layouts. Duplicate guards reject ambiguous record IDs with conflicting path information. New transcript IDs and old/new operational IDs are covered by tests.
- Existing request ACK, delivery, unresolved, completion, merge-keeper and merged-away updates resolve the existing path. Transcript eligibility/coverage/gap updates do likewise. Searches found no remaining production registration caller of the legacy transcript helper and no missed direct legacy request write. The `doctor` request helper hit is in its test fixture.
- New action/operation writers use the new helpers. Existing action updates and operation reassignment use location lookup, preserving original paths; undo reads old action records and keeps compensation data. The focused compatibility tests check both retirement undo and template-edit inverse compensation.
- Registration names and folder paths remain fixed through rename, observed move, retirement, undo, and late completion. Native transcript content is left untouched. There is no eager migration or rewriting of historical operation files.
- SQLite remains the operation lifecycle authority: writer status still reads the local journal, and this patch does not reconstruct live/rejected/failed/cancelled state from Git summaries. The Important finding concerns the additional write-time dependency on Git history, not a replacement of SQLite authority.
- Read-only request views and undo consumers use the shared readers. The real-agent fixture now resolves transcript location through the store. Task 3 owns consolidated documentation/skills/examples and herdr-threads integration; their pending changes are not treated as Task 2 omissions.

## Strengths and evidence

The production diff is compact and puts compatibility behavior in shared layout helpers. Location-preserving updates avoid invalidating old compensation paths. Captured attribution is additive and explicitly optional, with a useful legacy-label regression test. The new tests cover mixed layouts, duplicate IDs, old request processing, immutable attribution, unchanged native transcript bytes, and old undo records; sensitivity checks documented in the implementation report strengthen those assertions.

Read the supplied diff once in ordered chunks, plus the full Task 2 brief/report and `AGREED-FIXES-HANDOFF.md`. Follow-up source inspection was limited to named caller, compensation, lookup, and corruption risks. Existing logs independently confirm **614 library tests** and **34 integration tests** passing (20 daemon-composed, 8 integration-sweep, 3 writer-concurrency, 3 writer-crash). No suites were rerun. The report records successful formatting/diff checks and compilation of the opt-in real-agent test without launching it.

## Limits

No live instance or external service was accessed, and no real agent was launched. There was no source, index, HEAD, or branch mutation. The sole repository write for this review is this artifact; the pre-existing `Cargo.lock` change was left untouched. The focused harness demonstrates lookup behavior and read counts, not end-to-end timing or a full daemon failure run; the global mutation effect follows the inspected writer call chain. Large-history latency and unsupported arbitrary nesting remain unbenchmarked/unverified. Older binaries cannot discover new-layout records; the implementation correctly reports this release-documentation boundary for Task 3.
