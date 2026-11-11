# Final review: cluster 8 (src-store), epic hg-blw

Captured source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 6eb6cec786cde8ca070fbf9aed46bb0609ff63ac (both verified with `git rev-parse --verify`). Caller checkout observed at HEAD 6eb6cec, clean status, before and after the review. Inputs: slices 000039 (src/store/history.rs), 000040 (src/store/instruction_source.rs), 000041 (src/store/mod.rs), digest.md, the approved design doc docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md (Key abstractions: HistoryWalker, Scope, `instruction_source`; §2.1, §2.4).

## Scope of this cluster

- `store::history` (new): `parse_trailers`, `HistoryWalker` (`walk`, `last_touches`, `action_at_head`), `Change`, `ScanReport`, `Malformed`, `Touch`.
- `store::instruction_source`: `pick` made public with a `PickMode` (`LiveOnly` | `RetiredAllowed`), and `resolve_scope_in` as the mode-taking form of `resolve_scope`.
- `store::mod`: registers `pub mod history`.

## Analysis against the spec

HistoryWalker contract (Key abstractions):
- Walks from the `CommitId` it is given, never resolving HEAD itself; `action_at_head` reads at `self.from` (the pinned HEAD). Matches §2.1 "One HEAD per invocation".
- First-parent walk (`simplify_first_parent`), newest first, capped at `max_scan` (default 5000, matching §2.4). `stopped_early` is true only when the cap was hit with older commits left; a callback `Break` and a cap equal to the history length both report false. This is exactly what roster `last_change_scanned` (`last_change.is_some() || !stopped_early`) and `log`'s `scan` need. Covered by `walk_newest_first_and_cap`.
- Trailer parser is one function shared with `writer::recovery::trailers_since`. I compared the removed inline parser in recovery.rs with `parse_trailers`: identical semantics (prefix match per line, trim, last occurrence wins, unparsable value leaves `None`). No recovery behavior change.
- Per-commit record reads: the record is located among the commit's changed paths under `mutations/graph-changes/` or `mutations/undoable-actions/` by file name `<id>.toml`, and read from that commit's tree, so it is "the record as that commit wrote it". Checked against the writer: `src/writer/mod.rs:361-366` writes `Graph-Op` always and `Graph-Action` only when `applied.action` is set, and `layout::operation_record`/`action_record` place records at `<dir>/<YYYY-MM>/<id>.toml`; the undo path (`src/undo/compensate.rs`) writes a new action record for the undo action and modifies the original's `undone_by`, so the trailer-named record is present in that commit's diff. A present trailer whose record is missing or unparsable is flagged in `malformed` and the walk continues (§2.1). Missing counted as malformed is a reasonable reading of "does not parse".
- Changed paths: tree-to-tree diff against the first parent (all files for the root commit), deleted paths under their old name; no rename detection, so a move shows both old and new paths, which is what Scope path membership at the commit's tree needs.
- `action_at_head` maps a corrupt record to `None` and a missing one to `None` (via `read_action`), giving `undone: null` per §2.4.
- `last_touches` (last commit touching each path, one pass, within the cap) is implemented and unit-tested, but at the reviewed head it has no production caller: its consumer is `instructions stack` stale-part commits (hg-blw.12, reported BLOCKED in the digest). Whether B2/B5's stack path is delivered is outside this cluster; that trace belongs to the seam reviewer.

`instruction_source` change:
- `LiveOnly` reproduces the old `pick` exactly: one live → ok; several live → Ambiguous; only retired → Retired; none → UnknownTarget. `resolve_scope` delegates with `LiveOnly`, and the id path still returns Retired for a retired id in that mode, so `instructions show` is unchanged (spec "The default mode, and so `instructions show`, is unchanged").
- `RetiredAllowed` implements the Scope rule: no live match → the one retired match, or Ambiguous listing every id; an id resolves whatever its lifecycle. `layout::list_teamspaces` and `all_seats` enumerate `archive/` folders too, so archived objects resolve by id and name as the doc comment claims. `Dormant` counts as live in both the name and id paths, consistently.
- Error wording is shared through one `ambiguous` helper, so Scope's errors match `instructions show` (Key abstractions, Scope, Errors).
- Tests for both modes live in `src/inspect/scope.rs` (`pick_live_only_keeps_retired_error`, `pick_retired_allowed_rule`, `instructions_show_still_rejects_retired_name`, `resolve_live_and_retired`, `errors_match_instructions_show`).

Requirement baseline trace for this cluster:
- B3 (latest modifications: global, space, seat): the walker is reached from production at `src/cli/log.rs:112` (`log`) and `src/inspect/roster.rs:545` (roster `last_change`). The CLI-to-walker composition is the seam reviewer's to trace; within the cluster the walker supplies every per-commit fact §2.4 lists (trailers, operation record, action record as committed, changed paths, undo state at HEAD, scan report).
- B6 (`log` hides bookkeeping): filtering is LogQuery's, not the walker's (the walker correctly does not filter, per its Must not); outside this cluster.
- B1, B4 (scope resolution of retired targets for `sessions`/`log`): the `RetiredAllowed` mode delivers the resolver rule Scope consumes; the Scope wiring is in the inspect cluster.
- B2, B5, B7, B8: not implemented in this cluster.

Verification note: I could not run the tests. `toolbox run -c hg-build true` fails with "Error: unable to find user alepar: no matching entries in passwd file" (rc 255), the same wedged-container symptom tasks 6 and 9 reported. The review is static: code reading plus cross-checks against the writer, layout and undo sources at head.

## Concerns

- src/store/history.rs `commits()` sets `Sort::TOPOLOGICAL`. In libgit2 a topological sort is a limited walk that prepares the whole reachable history before yielding the first commit, so the revwalk cost is O(total history) whatever `max_scan` is (plausible, not measured). On a first-parent walk of a linear `main` the order is the same without it. Per-commit reads and tree diffs are still bounded by the cap, so the output is correct; this only weakens the §2.4 cost bound on very long histories.
- src/store/history.rs `last_touches` stops early only when `found.len() == paths.len()`, so duplicate input paths make it scan to the cap. The result is still correct; this is a performance edge only, and the function has no production caller yet.

## Must fix before landing

- none

## Untested scope

- Tests in this cluster were not run by this review: the hg-build toolbox fails ("unable to find user alepar: no matching entries in passwd file"), so `store::history` and `scope::pick_*` pass/fail at head is unverified here.
- No HistoryWalker test with a merge commit on `main` to pin the first-parent behavior.
- No test of a `Graph-Op`/`Graph-Action` trailer whose record is absent from that commit's diff (missing, not unparsable); only the unparsable cases are covered by `malformed_records_flagged`.
- `HistoryWalker::last_touches` has no production caller at head (its consumer, `instructions stack` stale-part commits in hg-blw.12, is blocked), so it is exercised only by `last_touches_one_pass`.

## Deferred OK

- Topological revwalk sort in `HistoryWalker::commits` makes revwalk preparation O(full history) despite the scan cap; dropping `Sort::TOPOLOGICAL` (first-parent walk of linear main) or measuring it is a follow-up (perf only, output correct).
- `last_touches` early exit counts distinct found paths against the input length, so duplicate input paths disable the early exit; dedupe the input when the stack consumer lands.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- `store::history` and `scope::pick_*` tests not run by this review: hg-build toolbox fails with "unable to find user alepar: no matching entries in passwd file".
- No HistoryWalker test with a merge commit on main to pin first-parent behavior.
- No test of a trailer whose record is missing from the commit's diff (only unparsable records are covered).
- `HistoryWalker::last_touches` has no production caller at head (consumer hg-blw.12 blocked); only its unit test exercises it.

## Deferred OK
- `Sort::TOPOLOGICAL` in `HistoryWalker::commits` likely makes revwalk preparation O(full history) despite the scan cap; perf only, output correct.
- `last_touches` early exit compares distinct found paths to input length, so duplicate input paths scan to the cap; perf only.
