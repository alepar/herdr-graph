super-roast verdict: Should-fix (6 confirmed) [low coverage] [review readiness incomplete]
mode: PR        iteration: 2 of 3
profile (assumed): herdr-graph 0.3.0 is a local, single-operator Herdr plugin: a Rust CLI plus a long-running per-user daemon over a Git-backed instance repository, reached over a local socket. It has no network-exposed surface and no multi-tenant users. The data at stake is the operator's own graph repository, which Git history keeps recoverable, and the code under review is read-only inspection views with a new, unreleased `--json` surface. Blast radius is low, so resilience, observability, cost and contract-consistency findings are rated Nit unless they contradict the change's own stated spec contract.
inputs: super-auto/graph-state-inspection@86bc78b vs main@bfeb8f3 (bounded: product paths only)
delta vs prior: 3 new confirmed (0 Blocking) · 0 carried (0 Blocking) · 9 resolved · 3 regressed (0 Blocking) · 1 punch-listed (open)
coverage: scouts 14/14 (correctness, security, premortem, simplicity-design, hot-path-perf, concurrency-async, regression, api-contract, observability, testing, test-causality-lifecycle, test-isolation-environment, test-suite-cost, hygiene-docs) · raw 17 → deduped 8 → panel 6 · spot 2 · promoted 0 · judge completion 100% · remainder-capped: 0 · review readiness: coverage bounded, provenance unavailable — basis: "review input selection: provenance unavailable; captured source available (run.md inv-64d86974 incomplete claimed collection: final-review cluster 13 never written); inspected: all 44 product paths outside docs/superpowers/runs/ at bfeb8f3104dd3783d67a46570c9f272e935012b5..86bc78b001901290a405ee0d2a9864849a06e785, every hunk; unreviewed: 299 run-record paths under docs/superpowers/runs/ (listed in /var/home/alepar/AleCode/herdr-graph/.worktrees/super-auto/graph-state-inspection/.superpowers/sdd/hg-blw-plan/assembled/roast-code-r2-bounded/unreviewed-paths.txt)"
independence: same-family (Claude) — seat-differentiated panel · rung: Workflow
seat-agreement: panels 6 · rr 0.83 · rg 0.83 · fg 0.67 · unanimous 0.67 · ground-loo 0.80 (n=5) · reproduce 5/1/0 · refute 4/2/0 · ground 6/0/0
lane-yield (found/confirmed/unique/refuted): correctness 2/2/0/0 · security 0/0/0/0 · premortem 2/2/0/0 · simplicity-design 1/0/0/1 · hot-path-perf 0/0/0/0 · concurrency-async 1/1/1/0 · regression 3/2/0/1 · api-contract 2/2/0/0 · observability 2/2/0/0 · testing 2/1/1/0 · test-causality-lifecycle 0/0/0/0 · test-isolation-environment 0/0/0/0 · test-suite-cost 0/0/0/0 · hygiene-docs 2/2/0/0

## Confirmed findings            ← consumed by super-design, one task per finding
- [Should-fix] src/inspect/roster.rs:587; src/inspect/roster.rs:588 — The round-1 fix makes the roster's `fill_last_changes` silently skip any commit whose tree has an unparseable record, for every seat at once, so a seat can report an older commit (or null) as `last_change` with `last_change_scanned: true` and no degraded field, warning or documentation. (regressed: fixed incompletely, see iteration 1 `src/inspect/log.rs:68; src/inspect/roster.rs:580`) [lanes: correctness, premortem, regression, api-contract, observability, hygiene-docs] [fix-regression]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/inspect/roster.rs:587-597 is `if let Ok(paths) = Scope::seat_paths_at(&store.at(&c.commit), &ids) { ... q.matches_with(store, c, paths).unwrap_or(false) ... }` with no else branch and no warning. The doc comment at roster.rs:556-558 says such a commit "counts as 'no match' for every seat".
  evidence: roster.rs:609 sets `last_change_scanned = Some(seat.last_change.is_some() || !report.stopped_early)`, so skipped commits still count as scanned.
  evidence: `seat_paths_at` goes through `layout::all_seats(tree)?`, so one bad record in an unrelated teamspace drops the commit for every seat.
  evidence: the test `roster_last_change_degrades_on_unparseable_historical_tree` (roster.rs:1157-1180) commits "beta while broken" and then asserts `sb.last_change.summary == "init"` and `last_change_scanned == Some(true)`.
  evidence: design line 163: "Every fact, notice or degraded state the text rendering shows has a named, typed JSON field." Design line 358 and README.md:96 define `last_change` as the newest in-scope commit, or `null` with `last_change_scanned`.
  evidence: the sibling `log` fix in the same commit (98af6df) keeps the commit as a row marked `Malformed::Tree` and warns, so the two views disagree on the same history.
  evidence: the round-1 abort is gone. The iteration 1 hint asked for `last_change: null` plus a scan or warning signal; the landed fix reports an older commit instead.
  evidence: this is not the iteration 1 rejected entry `src/inspect/roster.rs:94`. That rejection rested on `at` and `op` staying correct and stands. Here `at` and `op` name an older commit.
  severity: seats rated Should-fix / Nit / Should-fix. Kept at Should-fix despite the low-blast-radius profile because the value is wrong, not absent, and it contradicts quoted spec text. No seat ran the test.
  fix-shape hint: when a commit is skipped for a seat that has no match yet, report that seat as `last_change: null` with `last_change_scanned: false`, or add a typed skipped-commits field plus a warning; change the test to assert the signal and record the behaviour in the README and spec.
- [Should-fix] src/daemon/roster_live.rs:76; src/daemon/roster_live.rs:79; src/daemon/roster_live.rs:78; src/daemon/roster_live.rs:80 — `roster --live` reads `SeatReadSnapshot`, which is written only inside `LazyMap::inspect`; `observe_seats` returns early without calling it when no clone is seated, so a healthy instance with no seated clones always reports threads unavailable ("no threads seat read yet"), and once the seated set empties the last read's error or refusals stay frozen and are reported as current. (regressed: fixed incompletely, see iteration 1 `src/daemon/roster_live.rs:64`) [lanes: correctness, premortem, regression, observability] [fix-regression]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/observe/step.rs:605-619 `if seats.is_empty() { refused_since.clear(); seat_lost.clear(); return Default::default(); }` returns before `self.seat_view.inspect(&seats)` and never touches `seat_read`.
  evidence: src/daemon/compose.rs:1138-1157 writes `seat_read.last` and `seat_read.refusals` only inside `LazyMap::inspect`. A git grep finds no other writer.
  evidence: src/daemon/roster_live.rs:76-85 maps `last == None` to `Unavailable("no threads seat read yet")`, `Some(Err(msg))` to `Unavailable(msg)`, and `Some(Ok)` to the stored refusals plus the live `lost`.
  evidence: design §2.2 line 205 reserves `live_unavailable.threads` for "threads seat view not running"; line 206 says `live_unavailable` is null "when everything was read".
  evidence: before 2a80c88 the never-read case returned `Ok` with empty lists. The transport-error case from iteration 1 is fixed. The stale-state fault remains on the empty-seated-set path, and the fix added a false outage line.
  evidence: tests/inspect_live.rs:203-213 asserts only `live_unavailable.threads.is_string()`, which passes for both never-read and failed-read. The roster_live unit tests set `last` by hand.
  severity: all three seats rated Should-fix. Kept because it contradicts quoted spec text and puts a permanent false outage on the one field meant to report a real one.
  fix-shape hint: in the empty-seats branch of `observe_seats`, reset the snapshot to an Ok read with empty refusals, or have `roster.live` treat an empty seated set as Ok with empty lists; add a test for the seated-to-empty transition.
- [Should-fix] src/daemon/compose.rs:1143 — No test exercises the success branch of the new `roster --live` threads snapshot. Nothing shows that a successful `LazyMap::inspect` sets `seat_read.last = Some(Ok(()))` and copies that read's refusals. The diff also replaced the only end-to-end assertions of a healthy threads read with assertions that threads is degraded. A regression that makes `roster --live` always report threads unavailable, or always report empty refusals, would pass the whole suite. (new) [lanes: testing]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/daemon/compose.rs:1142-1151, the `Ok(_)` arm, sets `snap.last = Some(Ok(()))` and copies `seat_refusals` into `snap.refusals`.
  evidence: the two LazyMap tests (`seat_view_rejection_is_visible_in_threads_status_and_records_nothing`, `transient_seat_read_failure_is_recorded_for_roster_but_not_for_status`) assert only `last.is_none()` and `Some(Err(..))`.
  evidence: src/daemon/roster_live.rs:131 and :237 write `last = Some(Ok(()))` by hand, so they never reach the code that fills the snapshot.
  evidence: tests/inspect_live.rs:203-213 and tests/inspect_sweep.rs:448-453 now assert `live_unavailable.threads.is_string()` and `seat_refusals == null`.
  evidence: the mutation spike (delete the Ok-arm assignments and run the suite) was not run by any seat. The gap follows from reading the code and tests.
  severity: seats rated Should-fix / Nit / Should-fix. Kept at Should-fix: the uncovered path is the round-1 fix itself, and the finding above shows this code already misreports the healthy case.
  fix-shape hint: add a LazyMap test that drives one successful `inspect` against an accepting seat service and asserts `last == Some(Ok(()))` and the copied refusals; cover the healthy `roster --live` threads reply through the composed daemon.
- [Nit] src/inspect/log.rs:136; src/inspect/log.rs:118 — After the round-1 fix, scoped `log` keeps every scanned commit whose tree fails to resolve as a row marked with the new, undocumented value `malformed: ["tree"]`, whether or not it touches the scope; these rows count toward `--limit`, so `log <scope> --limit 1` can return an unrelated commit, and neither the spec nor the README mentions the value. (new) [lanes: api-contract, hygiene-docs]
  verdict: confirmed (reproduce ✓ / refute REJECT-outvoted / ground ✓)
  evidence: src/inspect/log.rs:133-140 `Err(_) => { let mut c = c.clone(); c.malformed.push(Malformed::Tree); kept.push(c); }`, then `if kept.len() >= self.limit { Break }`.
  evidence: src/store/history.rs:47-53 adds `Malformed::Tree` under `#[serde(rename_all = "snake_case")]`, so it serializes as "tree".
  evidence: design lines 168 and 519 list the values as `["operation"|"action"]`; line 352 gives the row shape. A grep of the design file and README finds no `tree` value.
  evidence: the stderr warning reuses "commits with unreadable records (listed with malformed)" and does not say a `tree` row may be out of scope.
  evidence: the refute seat rejected because design lines 168 and 519 already say an unreadable record never hides a commit, the row is flagged, and a warning is printed. Both confirming seats accept that the keep is deliberate and confirm on the undocumented schema-1 value and the limit count. The refute seat itself states the spec and README should name `tree`.
  demoted: seats rated Should-fix / Nit; the profile's `--json` surface is new and unreleased with no existing consumers, and the row is visibly marked, so a contract-documentation gap is a Nit here.
  fix-shape hint: add `tree` to design lines 168, 352 and 519 and to the README; state that `--limit` counts these rows, or give them a distinct warning line.
- [Nit] src/inspect/sessions.rs:59; src/inspect/stack.rs:376 — The "serde snake_case word of an enum" conversion is still reimplemented in sessions.rs (`fn word`) and inlined twice in stack.rs (:376, :392), although `roster::serde_word` now exists and log.rs uses it. (regressed: fixed incompletely, see iteration 1 `src/inspect/roster.rs:588`) [lanes: simplicity-design]
  verdict: carried from iteration 1, where the panel confirmed it (reproduce ✓ / refute ✗-survived / ground ✓). No scout re-raised it and no panel judged it this round.
  evidence: commit da52326 fixed the one live effect: src/inspect/log.rs:297-298 `kind_word` now calls `crate::inspect::roster::serde_word`, and the test at log.rs:1017-1019 asserts "native_session".
  evidence: at 86bc78b, src/inspect/sessions.rs:59-64 still defines its own `word`, and src/inspect/stack.rs:376 and :392 still inline `serde_json::to_value`. Read by the final gate with git grep; this is the duplication half of the iteration 1 claim, unchanged.
  fix-shape hint: move `serde_word` to a shared inspect module and call it from sessions.rs and stack.rs.
- [FYI] src/daemon/compose.rs:1137 — The new `SeatReadSnapshot` that `roster --live` reports is overwritten by every caller of the shared `LazyMap::inspect`, including the reconciler's one-seat finalize check (src/threads/effects.rs:1614). After such a check, `roster --live` shows `threads_live.seat_refusals` holding only the tombstoned seat's entry, which drops the real per-seat refusals. If that one-seat read fails, the whole threads overlay is marked `live_unavailable` until the observe loop reads again. Because the snapshot is written after the endpoint call guard is released, two concurrent reads can also store their outcomes in the opposite order to their reads. (new) [lanes: concurrency-async]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: compose.rs:275-299 passes the same `seat_view` to `threads::register_with` and `RuntimeLoop::new`. effects.rs `finalize_seat` calls `self.seat_view.inspect(std::slice::from_ref(&seat_ref))` on a Rejected `finalize_retirement`.
  evidence: mapping.rs ends `inspect` with `*self.refusals.lock() = refused`; `LazyMap::inspect` copies that set into `seat_refusals`, then into `snap.refusals`.
  evidence: the snapshot lock at compose.rs:1139 is taken after the inner async block that drops the call guard.
  evidence: the bad state needs a Rejected finalize and lasts until the next observe read. All three seats rated Nit.
  pre-existing on base: bfeb8f3:src/daemon/compose.rs:1088 already overwrites the `seat_refusals` status slot from the one-seat read. This change adds `roster.live` as a second reader. Final severity FYI.
  fix-shape hint: write the snapshot only from the observe loop's full read, and write it while the call guard is still held.

## Not verified (beyond panel cap)   ← severe candidates the panel cap left unverified — listed, never dropped
- none

## Not verified (dedupe failed or judge lost)   ← findings a pipeline failure left unjudged — listed, never dropped
- none

## Beyond remainder cap (count only)   ← low-severity candidates the dedupe remainder cap dropped; the count survives, the claims do not
- none

## Rejected (with reason)        ← so re-roasts don't re-litigate
- src/inspect/log.rs:131 — The log --since fix turns --since from a scan bound into a filter, so on a history longer than max_scan every `log --since` reports stopped_early true. [lanes: regression]
  reason: rejected 2/1. Design §2.4 "Cost" says "A scoped query scans until it finds `limit` matches, or until `--max-scan`"; the spec lists `--since` only as a filter (line 523: "It also applies `--since` and `--limit`"). With non-monotonic commit times, `stopped_early: true` at the cap is accurate. The cost is bounded by `--max-scan`. The change is the fix for the iteration 1 finding `src/inspect/log.rs:98`, documented in the code comment. The dissenting ground seat (Nit) cites design line 354 and concedes the spec's cost sentence "describes the new behavior".
- src/bootstrap/setup_claude.rs:211; tests/inspect_loaded.rs:487 — unbounded `p=$(cat)` in the Claude SessionStart hook command. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/cli/ops.rs:141; src/cli/ops.rs:201 — `ops.list` reply and argument changes with no IPC_VERSION bump. reason: rejected 2/1 in iteration 1; not re-raised; rejection stands.
- src/daemon/roster_live.rs:48; src/store/history.rs:128 — `roster.live` history walks contend with the writer on the repo Mutex. reason: rejected 2/1 in iteration 1; not re-raised; rejection stands.
- src/daemon/roster_live.rs:47 — `roster.live` build runs in spawn_blocking with no deadline or concurrency limit. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/inspect/scope.rs:228 — teamspace scope leaves out its seats' transcripts. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/inspect/log.rs:53 — `log --by <seat>` drops commits whose OperationRecord failed to parse. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/daemon/roster_live.rs:64 — fixed 10 s Herdr timeout after an unbounded build can exceed the client's CALL_TIMEOUT. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/plan/ops.rs:479 — `ops.list` calls `store.head()` synchronously on a tokio worker. reason: rejected 2/1 in iteration 1; not re-raised; rejection stands.
- src/cli/roster.rs:73 — `roster --live` against an old daemon exits 1, not 5. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/daemon/doctor.rs:268; src/daemon/doctor.rs:269; src/daemon/doctor.rs:266 — permanent WARN from the `harness_hook codex` doctor check. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- tests/inspect_sweep.rs:597 — every_component_reports_the_one_pinned_head cannot force a commit inside one view invocation. reason: rejected 2/1 in iteration 1; not re-raised; rejection stands.
- tests/inspect_sweep.rs:154 — `Rig::settle` uses a fixed 1.5 s quiet window. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/inspect/loaded.rs:156 — a failed loaded-record write is discarded silently. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/inspect/roster.rs:94 — `LastChange` has no `malformed` field for a commit with an unreadable record. reason: rejected 2/1 in iteration 1; rejection stands. The confirmed finding at src/inspect/roster.rs:587 is a different claim: there `at` and `op` name the wrong commit.

## Unverified nits (spot-checked)
- [Nit] src/inspect/roster.rs:1157 — The new test `roster_last_change_degrades_on_unparseable_historical_tree` asserts that a seat whose newest change sits in an unreadable tree reports the older commit `init` with `last_change_scanned: Some(true)`, and checks for no degraded signal. spot: CONFIRM (this is the test side of the confirmed finding at src/inspect/roster.rs:587; a correct fix there breaks this test, so change both together).
- [FYI] src/inspect/loaded.rs:308 — The round-1 batching fix (hg-blw.23) adds a public two-pass protocol, `loaded::HistoryPaths`, that roster.rs and stack.rs each copy by hand; keeping each stale part's path and filling `path_last_touched` afterwards would be simpler. spot: REJECT (the protocol is correct and tested, both callers do the second pass, the read race degrades to a null commit column, and the alternative changes a public output type; a code-shape preference, not a defect).

## Escalations (need human)      ← UNVERIFIED externals, incomplete panels, unsettled panels, material dissent
- none

## Prior-report tracking (iteration 1 confirmed findings; outside the recognized sections)
- [Should-fix] src/inspect/log.rs:68; src/inspect/roster.rs:580 — regressed (fixed incompletely). The abort is gone. The roster half now reports a silently wrong value (confirmed above at src/inspect/roster.rs:587). The log half marks the row but the marker is undocumented (new Nit above at src/inspect/log.rs:136).
- [Should-fix] src/daemon/roster_live.rs:64 — regressed (fixed incompletely). Transport errors are now recorded. The empty-seated-set path still shows frozen state and adds a false outage (confirmed above at src/daemon/roster_live.rs:76).
- [Should-fix] src/inspect/loaded.rs:296 — resolved. 98af6df batches the stale-part lookups into one `last_touches` call on the caller's walker and cap (`HistoryPaths`, test `stale_part_walk_honours_the_callers_cap`).
- [Should-fix] tests/inspect_sweep.rs:568 — resolved. tests/inspect_sweep.rs:576-632 now asserts `ops --seat` and `ops --teamspace` rows by requester id and asserts empty results for non-matching filters.
- [Nit] src/inspect/roster.rs:554 — resolved. `fill_last_changes` resolves every open seat's paths once per commit tree through `Scope::seat_paths_at`.
- [Nit] src/store/history.rs:130 — resolved. No `set_sorting` call remains in src/store/history.rs; `simplify_first_parent` is kept (line 134).
- [Nit] src/inspect/log.rs:98 — resolved. Commits older than `since` are skipped, not a stop; test `since_with_non_monotonic_times`. The regression claim against this fix was rejected 2/1 above.
- [Nit] src/daemon/roster_live.rs:52 — resolved. 307db36 routes `InstructionError::Store` through `CommandError::from_store` (roster_live.rs:37, test `instruction_store_errors_route_through_from_store`).
- [Nit] src/inspect/stack.rs:199 — resolved. The stack's history lookup goes through `HistoryPaths::resolve`, where a failed lookup is an empty map (test `one_history_pass_and_a_failed_lookup_degrades`).
- [Nit] tests/inspect_live.rs:184 — resolved. tests/inspect_live.rs:198-199 calls `rig.settle()` before sampling `commit_count()`.
- [Nit] src/cli/ops.rs:120 — resolved. 10ea84d adds a dedicated ops row view that reuses `log`'s `RequesterView`.
- [Nit] src/inspect/roster.rs:588 — regressed (fixed incompletely). The log spelling is fixed; the duplicate helpers in sessions.rs and stack.rs remain (carried Nit above).
- [Nit] CURRENT-DESIGN.md:96 — punch-listed (open). The caller's scope filter left it unfixed and no scout re-raised it.
- The resolved marks for src/store/history.rs:130, tests/inspect_sweep.rs:568, tests/inspect_live.rs:184, src/cli/ops.rs:120 and src/daemon/roster_live.rs:52 rest on a direct read of 86bc78b by the final gate, not on packet evidence. No test was run for any mark.
- Iteration 1's 48 unverified nits and 14 remainder-capped candidates are not restated here; see the iteration 1 report.
- Review readiness is incomplete (coverage bounded, provenance unavailable), so the absence of a finding in this round is not evidence that the 299 unreviewed run-record paths are clean.
