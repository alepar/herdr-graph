# Final review (bounded) — epic hg-blw, branch super-auto/graph-state-inspection

Invocation inv-faf81e1f-a04c-4dbd-b66e-76dfcc5eac5e · base bfeb8f3104dd3783d67a46570c9f272e935012b5 · head 527a81752a4fc649d3c92f62bc778dfcc7d7c51f

## Review route and provenance

- Route: bounded captured-source review. review-inputs.json reports status unavailable, sourceAvailable true, wholeRangeReady false, 0 omitted, 364 changed paths, 6 provenance gaps (run.md:119 inv-8323a5ec-1d63-45c5-88e1-e4ca9246fbb1: incomplete claimed collection, incomplete claimed final report, invalid failed assembly archive missing final-review-inv-8323a5ec-1d63-45c5-88e1-e4ca9246fbb1.md, incomplete assembled bundle digest.md; run.md:113 inv-64d86974-dab5-4fe4-a772-e304978dabd1: incomplete claimed collection, incomplete split bundle).
- Both SHAs verified with `git rev-parse --verify`; all reads used `git diff base..head` and `git show head:<path>`. No worktree was created, nothing was built or run, and the caller checkout was not switched.
- Inventory: 364 changed paths. 319 are under docs/superpowers/runs/ (run artifacts, ~71k added lines). 45 are product/test paths (~13.5k lines): README.md, evidence/hg-blw.24/final-run.log, 34 src/ files, 11 tests/ files.
- Inspected: the CLI wiring, the hook writer and setup/doctor changes, the instruction-manifest refactor, every new src/inspect module's production code, the history walker, the daemon roster.live handler and its compose/observe wiring, the ops filter changes, the instruction_source pick rule and the writer recovery refactor (exact hunks in bounded-coverage.json). For the new whole-file hunks (src/inspect/*.rs, src/store/history.rs, src/daemon/roster_live.rs, src/cli/roster.rs, src/cli/log.rs) the production code was read in full; their in-file `mod tests` sections were read only in part.
- Not inspected: src/cli/sessions.rs, src/inspect/output.rs, src/inspect/fixture.rs, src/bootstrap/tests.rs, README.md, evidence/hg-blw.24/final-run.log, all 11 tests/*.rs integration files, the test-only hunks src/plan/ops.rs `@@ -951,13 +1025,180 @@`, src/cli/instructions.rs `@@ -273,6 +372,67 @@`, src/cli/ops.rs `@@ -189,6 +360,127 @@`, and all 319 docs/superpowers/runs/ paths, the design doc's later sections included (only its baseline and §2–§5 were read).

## Requirement baseline trace

The design doc (docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md) has a `## Requirement baseline` section with rows B1–B8 and scope change S1. Each row was traced from a production entry point at head:

- B1 "which seats are active": `herdr-graph roster` → cli/roster.rs `execute` → inspect::roster::build (lifecycle, runtime availability and pane, occupant with native id and transcript, threads seat state, manager, reload_required). Delivered.
- B2 "which files are loaded into context per each": production writer is the SessionStart hook. `setup claude` now installs `p=$(cat); printf '%s' "$p" | <bin> seat --hook-prompt --stdin-payload; printf '%s' "$p" | <bin> session-report --from-hook claude`, and `setup codex` installs `seat --hook-prompt --codex --stdin-payload`. cli/seat.rs `hook_prompt` prints first, then reads the payload's `session_id` and calls loaded::record for a bound caller only. roster (`instructions.parts`/`omitted`/status) and `instructions stack` (manifest, hook-cap fit, per-clone status, harness-native files) read it back through loaded::clone_view, against the occupant's `native_session_id` from Inventory. `seat --instructions` and `seat.resolve` do not write. Delivered. Existing installs need `setup` rerun; doctor's new `harness_hook` check flags an outdated owned hook.
- B3 "state activity, latest modifications: global, space, seat": `herdr-graph log [global|teamspace <t>|seat <s>]` (daemon-free HistoryWalker, first-parent, scope resolved at each commit's own tree), roster `last_change`, and `ops --seat/--teamspace/--limit/--json` through the daemon. Delivered.
- B4 "list all sessions, by space/seat/clone, with timestamps and most recent first": `herdr-graph sessions [teamspace|seat|clone <x>]` → Inventory (live and archive/) → inspect::sessions::build, sorted by `started` descending with ns_ tiebreak, `--since`/`--limit`/`total`/`tombstones`. Delivered. The CLI file src/cli/sessions.rs itself was not inspected; parse coverage is in src/cli/mod.rs tests.
- B5 "Staleness is shown, not notified": no new notice path in the inspected diff; status shows only in roster and stack. Delivered.
- B6 "`log` hides `bookkeeping` by default": LogQuery::passes_filters drops Category::Bookkeeping unless `--all`. Delivered.
- B7 "Harness-native files: presence, size and mtime … never reads their content": inspect/harness_files.rs uses only `fs::metadata` and `read_dir`; no file is opened. Delivered.
- B8 "`roster --live` is in scope now": `roster --live` → `roster.live` daemon command, registered in daemon/compose.rs `compose_with` with the production Herdr client, the observe loop's SeatReadState and SeatLost. One snapshot per call. A failed snapshot or threads read degrades to `live_unavailable`, exit 0. Delivered.
- S1 (affects B8) "not managed by graph - just enumerates them, no additional info": `--unmanaged` (requires `--live`) lists only workspace/tab/pane ids and labels absent from every same-server non-retired binding. Delivered as scoped.

## Integration seam analysis

- Hook writer ↔ readers: the writer's root comes from `locate_instance`; the readers use `GitStore::root()`. Both resolve to the instance root, and `.graph-local/` is git-ignored at init, so inspection adds no commits.
- Recorded delivery ↔ HEAD comparison: both sides use the same `delivered(r, fit_hook(r, HOOK_RENDER_CAP))`. The cut-part digest covers only the delivered prefix. The header-only and partial-header budgets now omit the last part rather than recording a zero-byte cut (hg-blw.32 tests at head), which closes the hg-blw.29 deferred edge.
- Roster ↔ log: the roster's `last_change` reuses `LogQuery::matches_with` with `Scope::seat_paths_at` per commit. A seat absent from an older tree yields empty prefixes rather than an error, so pre-creation commits do not mark seats unreadable.
- Daemon roster.live ↔ CLI: the daemon builds the same Roster at its own pinned HEAD and returns `{head, roster}`. The CLI deserializes it and wraps it in the envelope with `live: true`.
- writer/recovery ↔ history: `trailers_since` now calls the shared `parse_trailers`, which matches the removed inline parser line for line.
- ops: `pending_for` now routes through `list_ops` with an unresolved filter and ALL limit. That is the same behavior. The `ops.list` reply gains `head`, which costs one `store.head()` read per call.

## Ledger triage

- Recurring minor (process-class missing-evidence, ×7 across hg-blw.7, .8, .5, .12, .11, .29, .31): this is process evidence, not a product defect. Not a landing gate. It reinforces that no full-suite measurement exists.
- Task 4 (hg-blw.5) [preexisting-failure] cli::tests::inspect_stubs_not_implemented failed at base: it names a failing test, so it was triaged as must-fix. At head the test no longer exists (no match in src/ or tests/), and every stubbed command is implemented. Resolved. Only the new `herdr_spikes`/`packaging` ignores remain, and both predate the branch.
- Task 19 (hg-blw.29) [untested-edge-case] fit_hook zero-byte cut: resolved at head by hg-blw.32 (partial_header_budget_omits_the_last_part, header_only_budget_omits_the_last_part).
- Task 13 (hg-blw.23) [silent-error-drop] in fill_last_changes: resolved at head by hg-blw.31 (`last_change_unreadable` plus a roster warning line).
- Task 11 (hg-blw.11) parked: only the `sessions --limit 1` flag-reach assertion in tests/inspect_sweep.rs was left unfixed, because the rig has no session rows. The pinned-head part of that finding is addressed at head (stack `rev` is compared with the envelope head at tests/inspect_sweep.rs:736-742). Acceptable to land. `--limit` truncation is a pure function in inspect/sessions.rs::build.
- Task 11 (hg-blw.11) [unrecorded-flake] daemon_composed::session_end_flows_to_transcripts timed out once and passed on rerun. This is a possible flaky test, so it should be watched in the caller's full-suite run rather than waived.
- The remaining minors (dangling-parent check over the whole instance, manager integrity error shown as null, retired-seat render noise under `--all`, duplicate serde-word helpers, fixed stack scan cap, the codex_bare 19-char timestamp assumption, formatting noise, weak or synthetic test setups) are deferrable as recorded.
- BLOCKED-AUTH: none in the ledger.
- Process sweep at ledger tail: 48 unknown processes, kept worktrees hg-blw.30/.31/.32 and /tmp/sp-f11883d5 ("process cleanup incomplete, exit 4"). This is an operational cleanup item for the caller (AGENTS.md leak rule), not a code defect.

## Concerns

- src/daemon/doctor.rs harness_hook_checks: a current Codex hook always produces a `warn` ("Codex runs a changed hook only after you trust it in /hooks; graph cannot check Codex trust"), even long after the user re-trusted it. That is a permanent doctor warning for every Codex install. warn_check keeps `ok: true`, so no exit code changes, but the noise may train users to ignore doctor warnings.
- src/cli/log.rs execute: walker warnings (malformed records, unreadable trees) go to stderr in text mode too. Design §2.1 says text-mode notices are part of the text rendering. This is minor drift: the information is not lost, but a text consumer reading only stdout misses it.
- src/inspect/log.rs LogQuery::run: `--since` skips older commits instead of stopping, by design because of clock skew. So `log --since 1d` with fewer than `--limit` matches always scans up to `--max-scan` (5000) commits, and then prints "older history not searched" even though `--since` excluded that history. The cost is bounded and the output stays honest, but it can mislead.
- Roster cost: one instruction render per listed seat, one loaded-record read per clone, and one history walk that enumerates `all_seats` per scanned commit (up to 5000). No measurement exists on a large instance (compare deferred hg-blw.5 perf-risk).

## Verdict
not ready (bounded review only: provenance gaps leave the whole-range review outstanding and no full-suite run exists; no blocking defect found in inspected hunks)

## Must fix before landing
- none

## Untested scope
- Whole-range final review outstanding: review-input provenance gaps at run.md:119 (inv-8323a5ec-1d63-45c5-88e1-e4ca9246fbb1: incomplete claimed collection, incomplete claimed final report, invalid failed assembly archive, incomplete assembled bundle digest.md) and run.md:113 (inv-64d86974-dab5-4fe4-a772-e304978dabd1: incomplete claimed collection, incomplete split bundle)
- No full-suite measurement of this branch exists; the sweep is deferred to the caller and nothing was built or run in this review
- Unreviewed product paths: src/cli/sessions.rs, src/inspect/output.rs, src/inspect/fixture.rs, src/bootstrap/tests.rs, README.md, evidence/hg-blw.24/final-run.log
- Unreviewed integration tests: tests/daemon_composed.rs, tests/daemon_free_reads.rs, tests/inspect_live.rs, tests/inspect_loaded.rs, tests/inspect_log.rs, tests/inspect_roster.rs, tests/inspect_sessions.rs, tests/inspect_stack.rs, tests/inspect_sweep.rs (only lines near 656-742 spot-checked)
- Unreviewed test-only hunks: src/plan/ops.rs @@ -951,13 +1025,180 @@, src/cli/instructions.rs @@ -273,6 +372,67 @@, src/cli/ops.rs @@ -189,6 +360,127 @@, and the in-file mod tests sections of the new src/inspect/*.rs, src/store/history.rs, src/daemon/roster_live.rs, src/cli/roster.rs and src/cli/log.rs files (read only in part)
- All 319 docs/superpowers/runs/ paths unreviewed (run artifacts and design doc beyond its baseline and sections 2-5)
- Real Codex SessionStart payload form and Codex hook trust are not verifiable from the repo; Codex loaded status relies on the documented session_id field and degrades to unknown
- Possible flaky test daemon_composed::session_end_flows_to_transcripts (timed out once, passed on rerun, no location recorded)
- Process sweep incomplete: 48 unknown processes and kept worktrees hg-blw.30, hg-blw.31, hg-blw.32 and /tmp/sp-f11883d5 need a leak check and cleanup per AGENTS.md

## Deferred OK
- Recurring missing-evidence process minor (x7 across hg-blw.7, .8, .5, .12, .11, .29, .31): process hygiene, no product effect
- hg-blw.11 parked sessions --limit 1 flag-reach assertion: rig has no session rows; truncation is a pure function covered at unit level
- doctor harness_hook codex: permanent warn on a current Codex hook because trust cannot be read (ok stays true)
- log text mode sends walker warnings to stderr rather than into the text rendering (section 2.1 drift)
- log --since scans to --max-scan and may print "older history not searched" when --since already excluded it
- Roster and log per-commit scope resolution cost unmeasured on large instances (hg-blw.5 perf-risk)
- hg-blw.7 dangling-parent integrity check runs instance-wide so a corrupt unrelated record fails a scoped view (exit 6, documented)
- hg-blw.1 manager-chain integrity error shown as manager null; retired-seat render error rows under --all; retired(reversible) text form absent
- hg-blw.8 ops --seat/--teamspace resolve before the daemon call, so exit codes differ from 5 when the daemon is down
- hg-blw.23 stack uses DEFAULT_MAX_SCAN with no caller cap; hg-blw.24 codex_bare fixed 19-char rollout timestamp width
- hg-blw.27 duplicate serde-word helpers in sessions.rs and stack.rs; formatting-noise and doc-comment-length minors
