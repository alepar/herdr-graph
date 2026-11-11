# Final review, cluster 9 (store), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298 (both verified with `git rev-parse --verify`; review source bound to the ledger before any write). Caller checkout observed at b9caa7f with a clean status before and after the review; it was not switched or modified.

Files: `src/store/history.rs` (new, HistoryWalker), `src/store/instruction_source.rs` (`PickMode`, `resolve_scope_in`), `src/store/mod.rs` (module registration). Slices 000078 to 000080.

## Spec conformance

- HistoryWalker (design Key abstractions, HistoryWalker; §2.4). It walks from the `CommitId` it is given, never from HEAD, so the one-HEAD-per-invocation rule (§2.1) holds. It walks newest first with first-parent simplification. Per commit it reads the trailers, the operation and action records from that commit's own tree, and the diff-tree paths against the first parent (every file for the root commit). It stops at `max_scan`. `ScanReport.stopped_early` is true only when the cap was reached with older commits left, so a full walk whose length equals the cap reports false (tested). An early `Break` from the callback reports false, which is the right answer for `--limit` satisfied.
- Malformed records. A trailer whose record is missing from the commit's changed paths, unreadable or unparseable gives `malformed: [operation|action]`, the record is absent and the walk continues (tested for both kinds). The stderr warning belongs to the view and is outside this cluster.
- Undo state at HEAD. `action_at_head` reads the record at the walker's `from` (the pinned HEAD) and maps a missing or corrupt record to `None`. That gives `undone: null`, as §2.4 asks (tested).
- Last-touch-per-path. `last_touches` makes one capped pass for the whole path set. A path matches itself or any file under it, and deleted files count under their old name.
- Shared trailer parser. `writer::recovery::trailers_since` now calls `parse_trailers`. The removed inline loop matched it line for line (same prefixes, same `trim().parse().ok()`, last occurrence wins), so recovery behavior is unchanged. This meets the design's "extracted rather than duplicated" relationship.
- Scope's retired rule. `pick` gains `PickMode`. `LiveOnly` keeps exactly the old mapping: no matches gives UnknownTarget, retired-only gives Retired, several live gives Ambiguous. So `instructions show` is unchanged (resolve_scope delegates with `LiveOnly`). `RetiredAllowed` gives the design's rule: one live match resolves; with none live, one retired match resolves and several are ambiguous, listing every id. Under `RetiredAllowed` an id resolves at any lifecycle. The id and name lookups go through `layout::list_teamspaces` / `all_seats`, which cover `archive/teamspaces` and `archive/seats`. Error wording is shared through the extracted `ambiguous` helper.

## Requirement baseline trace

- B3 (latest modifications, global/space/seat) and B2's stale-part `path last touched` column depend on this cluster's walker. Production callers exist at head. `herdr-graph log` goes through `cli/mod.rs:94` to `cli/log.rs:113`, which builds `HistoryWalker::new(store, &head, a.max_scan)` and runs `LogQuery`. Roster `last_change` goes through `inspect/roster.rs:554`, and `instructions stack` / stale parts through `inspect/stack.rs:199` and `inspect/loaded.rs:296`. Scope filtering, `--by`, the bookkeeping filter and rendering after the walker are LogQuery's and the views', which are other clusters and the seam reviewer's to trace end to end. Within this cluster the walker supplies every per-commit fact those consumers need.
- The `RetiredAllowed` resolution behind `log`/`sessions` coverage of retired objects (B3, B4) is reached from production through `inspect/scope.rs:127` (`Scope::resolve`). The view wiring past that point is the seam reviewer's.
- B1, B5 to B8 do not touch this cluster.

## Verification run

- A review-owned detached worktree was made at b9caa7f, with its build dir reflink-seeded. It ran `toolbox run -c hg-build cargo test --lib -- store::history store::instruction_source inspect::scope writer::recovery`: 22 passed, 0 failed. A second run, `cargo test --lib -- instruction recovery`, gave 88 passed, 0 failed. After each run the leaked-rig check printed 0. The worktree and its target dir were then removed.

## Observations below the bar

- `last_touches` returns only the map and no `ScanReport`. A part whose last touch is older than the cap (stack uses `DEFAULT_MAX_SCAN` = 5000) is absent, which is indistinguishable from a part never touched. The spec only requires the `scan` object on `log` and the roster (§2.1 list), so this is not a conformance gap. A consumer showing `-` for both cases is slightly less honest than the log view.
- A `Graph-Op:` value that is not a valid OpId is treated as an unattributed commit, not as `malformed`. This matches recovery's old behavior. The spec's malformed rule covers records, not trailer values, and the writer always formats a valid id.
- `read_record` finds the record among the commit's changed paths, so a trailer whose record was written in an earlier commit would read as malformed. The writer writes the operation record (and the mutation's action record) in the same tree as the trailered commit (`writer/mod.rs:356-366`), so this cannot happen on writer-produced history.
- The walker takes the repo lock once per commit rather than holding it for the whole walk. This is deliberate (the doc comment says so) and fine for a 5000-commit cap.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- `last_touches` has no test for a deleted file or directory path, which takes the `Delta::Deleted` old-name branch of `changed_paths`; only additions and modifications are exercised.
- No test in this cluster drives `walk` with `max_scan = 0`, or with a `from` id that is valid hex but absent from the repo (the `StoreError` path).

## Deferred OK
- `HistoryWalker::last_touches` could return its `ScanReport`, so that `stack`'s `path last touched` can say "older than scan" rather than showing the same empty value as never-touched; the spec does not require it today.
