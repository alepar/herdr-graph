# Task 2 re-review, round 1 — 71a85ba..0cf38e8

Prior Important finding: **ADDRESSED**.

Spec verdict: **compliant** within the Task 2 scope previously reviewed.

Quality verdict: **approved**. No new Critical or Important regressions found in the fix diff. No additional findings.

## Resolution

`find_record_file<R>` now probes only `<requested-id>.toml` in every supported directory. The request, transcript, action, and operation lookup arms use the correct typed record parser and retain the existing dual-layout directory helpers. This removes the full-history deserialization from new-record collision checks through `Overlay::put_record`.

- Existing candidate payloads are deserialized as the expected record type. Malformed targeted records fail explicitly.
- A valid payload whose ID differs from the requested filename ID produces a corruption error naming both IDs and the candidate path, rather than being mistaken for an absent record.
- Finding a valid candidate does not stop traversal. A second candidate in another supported old/new directory rejects the lookup with both conflicting paths; neither layout silently wins.
- Missing candidates continue without parsing unrelated records. A fresh operation therefore no longer inherits unrelated historical operation corruption or one blob read per historical operation.
- Enumeration remains unchanged: it fully parses supported records and rejects duplicate serialized IDs. Targeted lookup intentionally checks ID-named candidates; arbitrary misnamed payloads are an enumeration integrity concern, not a reason to deserialize all history during a new-ID collision check.

The fix is limited to the shared layout lookup, focused store tests, and the report. It does not change historical record destinations, attribution capture, existing-path updates, compensation, journal lifecycle authority, or native transcript handling. The prior Task 2 compatibility assessment continues to apply.

## Evidence assessed

Read the complete 341-line fix package and updated Task 2 report. The new tests exercise production `Overlay::put_record` and lookup against an isolated real Git store; the counting adapter routes lookup through production `layout::locate` and counts successful blob reads rather than inventing a separate lookup implementation.

1. With 100 valid historical operations, a fresh insertion reads **zero existing record blobs**, discovers the two operation directory roots, and obtains revision 1.
2. With unrelated malformed legacy records, fresh operation insertion succeeds. Targeting each malformed operation/request/transcript/action still errors at its path; absent IDs return `None`; full enumeration of each kind still errors.
3. A requested-ID filename containing another valid operation ID fails with a precise corruption error.

The existing duplicate-layout tests for all four kinds remain in the passing store suite. They exercise both supported locations with the same requested ID; the new shared loop preserves their rejection behavior.

Inspected the recorded test results: the three focused regressions failed before the fix and passed after it; the blob-count RED was 100 versus expected zero. The store suite passed **33**, compatibility tests passed **4**, and final validation passed **617 library tests plus 34 integration tests** (20 + 8 + 3 + 3). These logs provide adequate evidence for the original concern; no suites or independent harness were rerun in this re-review.

## Limits

This was a scoped review of the correction, not a fresh review of unchanged Task 2 code or Task 3. Directory discovery and absent filename probes still scale with supported month/transcript directories; no large-history wall-clock benchmark was performed or claimed. No live instance, daemon, external service, or real agent was accessed. No source, index, HEAD, or branch was modified; this review artifact is the only repository write.
