# Final review: cluster 9 (src-writer), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 6eb6cec786cde8ca070fbf9aed46bb0609ff63ac (both verified with `git rev-parse --verify`). Caller checkout observed at head 6eb6cec, working tree clean. Cluster input: slice 000042.diff, `src/writer/recovery.rs` (+3/-12).

## What changed

`writer::recovery::trailers_since` drops its inline `Graph-Op:` / `Graph-Action:` line loop and calls `crate::store::history::parse_trailers`, then pushes `(t.op, commit, t.action)` when `t.op` is `Some`.

## Equivalence analysis

- `parse_trailers` at head (`src/store/history.rs:23-33`) is a verbatim lift of the removed loop: same `msg.lines()` iteration, same `strip_prefix` on the exact prefixes, same `trim().parse::<OpId>().ok()` / `parse::<ActionId>().ok()`, same `else if` ordering, same last-occurrence-wins overwrite (including a later unparsable value resetting an earlier parsed one to `None`). Output for every message is identical to the old code.
- The commit-selection rule (emit only when an op trailer parsed) and the tuple shape returned to the sole caller (`src/writer/mod.rs:658`, recovery in `Writer::recover`) are unchanged.
- Imports: `OpId` and `ActionId` are still used by `RecoveryReport` and the return type, so no unused-import warning results.
- Revwalk, checkpoint hiding and the missing-checkpoint fallback are untouched.

## Spec conformance

- Design §HistoryWalker (design doc line 517): "`writer::recovery::trailers_since` and the walker share one trailer-parsing function, extracted rather than duplicated." Delivered: one parser, both callers (`history.rs:278` and `recovery.rs`).
- Design line 561-562 ("callers never use revwalk or parse trailers directly") governs HistoryWalker consumers (log, roster, stack). Recovery keeping its own checkpoint-bounded revwalk matches line 517, which asks only for a shared parser, and the epic's "No new Git writes" constraint is unaffected.
- Requirement baseline rows B1-B8 and S1 are inspection-view behaviors. None is delivered in this cluster; this file contributes only the shared-parser refactor that the walker uses. Tracing those rows through `store::history` and the views is the src-store, src-inspect and seam reviewers' job.

## Test evidence

- `parse_trailers_forms` (`history.rs:344-366`) covers both trailers, op-only, no trailers and unparsable values.
- Existing writer tests `recover_marks_committed_from_trailer` and `recover_requeues_applying_op_without_trailer_and_counts_attempt` (`src/writer/tests.rs:845`, `:893`) exercise `trailers_since` through `Writer::recover` with and without a `Graph-Op` trailer. Tests were not rerun by this review (read-only; no rig started).

## Findings

- none

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- Recovery with a `Graph-Action` trailer present (the action id passed into `finish_committed_superseding`) has no end-to-end recovery test; the parser half is unit-tested and the gap predates this diff.

## Deferred OK
- none
