# Final review (bounded) — epic hg-blw, super-auto/graph-state-inspection

Captured source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 20f1f16270073d4eb29646a65722ee2254cdfcb3 (both verified with `git rev-parse --verify`). Caller checkout observed at head 20f1f16, working tree clean; it was not switched or modified.

## Scope of this pass

- Route: bounded. The review-inputs sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, 351 changed paths selected, 0 omitted, 2 retained provenance gaps (run.md:108, inv-64d86974: incomplete claimed collection; incomplete split bundle). No whole-range package was built or loaded.
- Inventory read in full: 351 changed paths; 306 are run archives under docs/superpowers/runs (an established repo convention: 1088 such files at base), 1 is evidence/hg-blw.24/final-run.log, README.md, and 43 Rust source/test paths (about 15.6k added lines).
- Inspected from captured Git (exact hunks in bounded-coverage.json): CLI wiring (src/cli/mod.rs), the hook writer path (src/cli/seat.rs, src/bootstrap/setup_claude.rs, src/cli/setup.rs, src/bootstrap/instructions.rs fit_hook/digest), the daemon registration and roster.live handler (src/daemon/compose.rs, src/daemon/roster_live.rs, src/observe/step.rs seat-read slot), doctor hook checks, ops filters (src/plan/ops.rs, partial), instruction_source pick modes, the recovery trailer refactor, and the roster/log/sessions/instructions-stack CLI entry points.
- Read beyond the coverage list, without claiming hunk coverage: production half of src/inspect/loaded.rs (lines 1-420 of 855), src/inspect/roster.rs lines 236-660 (build_inner, overlay, unmanaged_of, fill_last_changes), src/inspect/log.rs lines 40-180 (LogQuery filters and run), the harness_files.rs metadata-only access, sessions.rs sort/limit lines, store/history.rs parse_trailers, and transcripts/capture.rs report_args (to trace the session-id seam).

## Requirement trace

- B1 (which seats are active): `roster` is wired in src/cli/mod.rs to cli::roster::run, which pins HEAD and calls inspect::roster::build; `--live` goes through daemon `roster.live`, registered in production compose_with with the real Herdr client. Delivered from a production entry point.
- B2 (which files are loaded per seat): the only writer is `seat --hook-prompt`, which records after printing via loaded::record; the installed hook commands (setup_claude.rs hook_command / codex_hook_command) pass `--stdin-payload`. For Claude, session-report commits `native_session_id = payload.session_id` (transcripts/capture.rs:97), the same value the hook records, so the identity join is byte-equal by construction. Status is surfaced in roster and `instructions stack`. Delivered; the end-to-end test simulates the occupant commit rather than driving session.report (ledger hg-blw.12), so the join is source-traced, not exercised end to end.
- B3 (activity / latest modifications by global, space, seat): `log` (daemon-free history walk with scope, `--by` AND filter, bookkeeping filter) and roster `last_change`; `ops --seat/--teamspace/--json` for the journal. Delivered at entry-point level; scope.rs path membership internals were not read in this pass.
- B4 (sessions by space/seat/clone, timestamps, newest first): `sessions` with `clone <c>` scope, sorted started-descending with ns tie-break, `--since`/`--limit`/`total`. Delivered.
- B5 (staleness shown, not notified): no notification path added; status only in views. Delivered.
- B6 (log hides bookkeeping by default): LogQuery::passes_filters drops Category::Bookkeeping unless `--all`. Delivered.
- B7 (harness files: presence, size, mtime, never content): harness_files.rs uses std::fs::metadata only; stack JSON carries imports_followed false and the presence note. Delivered.
- B8 + S1 (`roster --live`, `--unmanaged` enumerates only): overlay fills agent/status, incarnation-aware live_pane_matches, threads refusals/lost, live_unavailable; unmanaged_of lists ids and labels only, with same-server binding filtering. Delivered.

## Seam analysis

- Hook output vs record: hook_prompt prints and flushes before reading stdin and writing the record; record errors are swallowed; stdin read is capped (64 KiB) and bounded (2 s) on a helper thread. The Claude command reads the payload once and pipes it to both commands. No ordering defect found.
- fit_hook vs delivered(): fit omits a prefix of render order and cuts only the last part; delivered() relies on exactly that, and the header-only/partial-header budgets now omit the last part (cut None), so recorded parts and printed text agree.
- roster.live vs observe loop: the threads slot is written once per full seat read under the step lock; the handler reads `lost` from a separate slot while holding the snapshot lock, so seat_lost can lag the refusals by one step (ledger hg-blw.22 lock-scope minor; benign).
- Pinned HEAD: every daemon-free view pins HEAD once and passes it to scope, inventory, history and envelope; the sweep test now asserts stack rev equals the envelope head.
- Writer refactor: recovery.rs now uses store::history::parse_trailers, whose parsing is byte-identical to the removed inline loop.

## Ledger triage

- Parked (hg-blw.11, sweep flag-reach and pinned-head assertions): the later sweep at head asserts `log --by` finds exactly the seat-requested change, `--all` differs from the default, and stack rev equals the envelope head; the remaining `sessions --limit 1` check is weak in the sweep but `sessions --limit 2` is asserted at binary level in tests/inspect_sessions.rs:271. Acceptable to land.
- Recurring minor, missing-evidence (x7 across 7 tasks): process-class reporting gap, not a product defect; it does mean per-task test claims are prose-only, which is one more reason the deferred full-suite run is load-bearing.
- hg-blw.5 preexisting-failure (cli::tests::inspect_stubs_not_implemented): would be must-fix as a known failing test, but the test no longer exists at head (git grep finds no match), so it is resolved.
- hg-blw.29 fit_hook partial-header Some(0) edge: resolved by hg-blw.32 at head (partial_header_budget_omits_the_last_part test, cut None).
- Process sweep line: 48 unknown processes, 4 named pids whose cwd could not be inspected, 8 task worktrees and /tmp/sp-f11883d5 kept. The AGENTS.md no-leak rule (`/tmp/hg-*` cwd count 0) is therefore unconfirmed for this run.
- Other deferred minors (dangling-parent scope check, swallowed manager errors, noisy error rows under --all, codex_bare fixed-width assumption, hardcoded stack scan cap, duplicate serde-word helper, doc drift on last_change_unreadable in design 2.2): none is a data-loss path, a skipped test or a delivery gap; fine as follow-ups.

## Verdict
not ready (bounded review only: whole-range review, provenance gaps and the full-suite run are outstanding)

## Must fix before landing
- none

## Untested scope
- Full-suite sweep of this branch has not run (deferred to the caller); no full-suite measurement exists, so the branch is not established as tested.
- Whole-range review outstanding: this bounded pass did not inspect src/inspect/{scope,inventory,stack,harness_files,sessions,output,mod,fixture}.rs, src/inspect/roster.rs and src/inspect/log.rs beyond the noted line ranges, src/inspect/loaded.rs test module, src/store/history.rs beyond parse_trailers, src/cli/ops.rs, src/bootstrap/tests.rs, src/plan/ops.rs hunk @@ -951,13 +1025,180 @@, src/observe/step.rs hunks @@ -657,13 +726,7 @@ and @@ -672,10 +735,7 @@, README.md, evidence/hg-blw.24/final-run.log, all tests/*.rs changes, and the 306 docs/superpowers/runs archive paths.
- Provenance gap retained: 20f1f16:docs/superpowers/runs/2026-11-07-graph-state-inspection/run.md:108 inv-64d86974-dab5-4fe4-a772-e304978dabd1: incomplete claimed collection.
- Provenance gap retained: same location: incomplete split bundle: missing cluster report or slices.
- Claude hook-to-occupant identity join (B2 current status) is source-traced only; tests/inspect_loaded.rs commits the occupant directly instead of via daemon session.report.
- Run process hygiene unconfirmed: ledger process sweep reports 48 unknown processes and kept task worktrees plus /tmp/sp-f11883d5; the AGENTS.md `/tmp/hg-*` leak check needs a human or caller run.

## Deferred OK
- roster.live reads seat_lost from a separate slot from the refusal snapshot, so it can lag by one observe step (src/daemon/roster_live.rs handle).
- roster --json emits no stderr warning for last_change_unreadable (typed field is present; text mode prints the warning); spec 2.1 asks for a warning.
- fill_last_changes resolves seat paths before the bookkeeping filter, so an unreadable tree on a bookkeeping commit can mark seats last_change_unreadable that the filter would have skipped (conservative, typed).
- log --since skips rather than stops on old commits, so every --since query scans to max_scan (documented clock-skew trade-off; cost only).
- ops.list reply head is read from the store separately from the journal listing (src/plan/ops.rs list_json); informational only.
- Ledger deferred minors listed under Ledger triage (dangling-parent scope check, swallowed manager errors, noisy --all error rows, codex_bare width, stack scan cap, duplicate serde-word helper, design 2.2 doc drift, recurring missing-evidence).
