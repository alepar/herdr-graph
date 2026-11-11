# Final review (bounded) — epic hg-blw, super-auto/graph-state-inspection

Invocation inv-42406b71-557f-4a20-820e-53b3ac19347c · captured base bfeb8f3104dd3783d67a46570c9f272e935012b5 · captured head 85d7810f30207cdd956a50915228ec528341189d (both verified with `git rev-parse --verify`). The caller checkout was at 85d7810 with a clean status when the review started and when the draft was written.

## Review route and source

The review-inputs sidecar reads `status: unavailable`, `sourceAvailable: true`, `wholeRangeReady: false`, 337 selected paths, 0 omitted, and two retained gaps for inv-64d86974 (run.md:99: an incomplete claimed collection, and a split bundle with no cluster report or slices). So this is a bounded captured-source review. No whole-range package was built or loaded. I read the full changed-path inventory with `git diff --stat` and `--numstat`: 44 product, test and evidence paths, plus about 293 run and review-archive documents under docs/superpowers/runs/. Then I read the named hunks listed in bounded-coverage.json from captured Git. For new files listed there, I read the production code in full; the test module at the end of each was only skimmed. src/inspect/roster.rs (lines 236-780 read) and src/inspect/log.rs (lines 1-330 read) were read in part and are left out of the coverage. Paths not in the coverage file were not reviewed.

## Baseline trace from production entry points

- B1 (which seats are active): `herdr-graph roster` goes through cli/roster.rs `execute` → `pin_head` → `Scope::resolve` → `inspect::roster::build`, which yields seats, clones, lifecycle, runtime, occupant and threads. Delivered.
- B2 (which files are loaded per seat): the installed SessionStart hook (setup_claude.rs `hook_command` / `codex_hook_command` with `--stdin-payload`) → cli/seat.rs `hook_prompt` → `loaded::record` writes `.graph-local/loaded/<clone>.json`. `roster` and `instructions stack` read it through `loaded::clone_view`, and roster JSON carries `instructions.parts`/`omitted`. Delivered for Claude. For Codex it rests on an unverified external payload form and degrades to `unknown` with a note (parked P1 in run.md).
- B3 (activity and latest modifications: global, space, seat): `herdr-graph log [global|teamspace|seat]` → `LogQuery::run` over `HistoryWalker` from the pinned HEAD, with path membership taken at each commit's own tree. Roster `last_change` uses the same matcher. Delivered.
- B4 (all sessions by space/seat/clone, timestamps, newest first): `herdr-graph sessions [scope|clone c]` → `Inventory::load` → `inspect::sessions::build`, sorted by `started` descending with an ns-id tie-break, retired objects included. Delivered.
- B5 (staleness shown, not notified): `loaded::status` matches identity by native session id, then compares the record against `fit_hook` over HEAD (cap 9000) and sets current, stale or unknown. Roster and stack render it. Nothing notifies. Delivered (Codex caveat as in B2).
- B6 (log hides bookkeeping by default): `LogQuery::passes_filters` drops `Category::Bookkeeping` unless `--all`. Delivered.
- B7 (harness-native files: presence, size, mtime, never content): harness_files.rs uses only `fs::metadata`, `symlink_metadata` and `read_dir`, and a test (`content_never_opened`) uses a mode-000 file. Only grep-level review here; the discovery rules were not reviewed.
- B8 + S1 (`roster --live`, unmanaged ids only): `roster.live` is registered in production `compose_with` (compose.rs @@ -358) with a real Herdr client and the `SeatReadState` the LazyMap fills on every read outcome. `overlay` reports a failed snapshot or threads read in `live_unavailable`. `--unmanaged` lists only workspace, tab and pane ids and labels. Delivered.

## Cross-task seams observed

- The threads-degraded seam that the earlier final review inv-64d86974 flagged is closed at head. `LazyMap::inspect` records every read outcome in `SeatReadState`, a read that was never made counts as unavailable, and roster_live maps both to `ThreadsRead::Unavailable`.
- That review also asked for the README degraded-state paragraph to be corrected. It is still wrong at head (README.md:100). The code (`inspect::roster::overlay`) nulls clone `live` only when the Herdr snapshot fails. When only threads is unavailable, clone `live` stays populated and only `threads_live.seat_refusals`/`seat_lost` go null. README commits a5a1f91 and 2a80c88 did not change this sentence.
- The hook and the loaded record agree on the delivered bytes: `delivered()` and `fit_hook` share one omitted-prefix-plus-last-cut model, and `cut_body_bytes` subtracts the block header. One edge case breaks this. When the budget covers only part of the cut block's header, `kept` is "\n", `cut_body_bytes` is `Some(0)`, and the record lists the last part as delivered `cut` with the digest of an empty body, although none of its header or body reached the session (instructions.rs:505-506, ledger hg-blw.29 minor).
- writer/recovery.rs now calls the shared `parse_trailers`. Its semantics (last occurrence wins, an unparseable value is None) are byte-for-byte the removed loop's.
- `ops` keeps the daemon's existing default of 200 (`limit` was already 200 daemon-side), so plain `ops` output does not change.
- The Codex `harness_hook` doctor check is always a warning (`ok: true`, `warn: true`), even when the hook is current, because graph cannot read Codex trust. It never fails doctor, but it is a permanent warning.

## Ledger triage

- The `Recurring minor` missing-evidence cluster (×6 across hg-blw.7, .8, .5, .12, .11, .29) is a process-class item with no product effect. Deferred.
- The hg-blw.29 minor on the fit_hook partial-header case names a known product defect (the loaded record misstates a delivery). Under the triage rule it was misgraded, so it is promoted to Must fix below. It is a small fix.
- The hg-blw.11 minor on the `daemon_composed::session_end_flows_to_transcripts` timeout is a flake attributed to load, with no recorded location. It goes under Untested scope for the caller's full-suite sweep.
- The hg-blw.5 minor on `cli::tests::inspect_stubs_not_implemented` failing is resolved: that test no longer exists at head (git grep finds nothing).
- The parked hg-blw.11 finding (the `sessions --limit 1` assertion in the inspect_sweep rig cannot fail) is Deferred OK. The rest of that finding (`--by`, `--all`, `--limit` on log) was fixed according to the ledger.
- There are no BLOCKED-AUTH lines. The hg-blw.2 BLOCKED was an environment block (container unusable) and was later re-run.
- All other deferred minors are judged Deferred OK (listed below).

## Verdict
not ready (bounded review only: whole-range review outstanding and no full-suite run at 85d7810; README --live degraded contract misstated; fit_hook partial-header case records an undelivered part as cut)

## Must fix before landing
- README.md:100 says that when "the threads view is unavailable … every clone's `live` is `null`"; src/inspect/roster.rs `overlay` nulls clone `live` only on a Herdr snapshot failure, while threads-only unavailability keeps `live` and nulls `threads_live.seat_refusals`/`seat_lost`; correct the JSON contract text (carried from final review inv-64d86974 Must fix, still unfixed at head).
- src/bootstrap/instructions.rs:505-506 `fit_hook`: when the budget reaches only part of the cut block's header, `kept` is "\n" and `cut_body_bytes` is `Some(0)`, so `loaded::delivered` records the last part as delivered `cut` with the digest of "" although its header and body never reached the session; treat that case as omitted (or skip recording the part) and add the test (ledger hg-blw.29 minor, promoted as a known product defect).
- A whole-range final review of bfeb8f3..85d7810 is still outstanding: this bounded review left most of src/inspect (roster, log, stack, inventory, output, harness_files, fixture, mod), src/store/instruction_source.rs and every new integration test unread.

## Untested scope
- No full-suite measurement of 85d7810 exists; the caller's sweep runs after this invocation, so no claim that the branch is tested is made here.
- Provenance gap: 85d7810f30207cdd956a50915228ec528341189d:docs/superpowers/runs/2026-11-07-graph-state-inspection/run.md:99 inv-64d86974-dab5-4fe4-a772-e304978dabd1: incomplete claimed collection.
- Provenance gap: 85d7810f30207cdd956a50915228ec528341189d:docs/superpowers/runs/2026-11-07-graph-state-inspection/run.md:99 inv-64d86974-dab5-4fe4-a772-e304978dabd1: incomplete split bundle: missing cluster report or slices.
- Unreviewed (partially read, not in coverage): src/inspect/roster.rs (lines 1-235 and 781-1779) and src/inspect/log.rs (lines 331-1021, text rendering and tests).
- Unreviewed product paths: src/inspect/fixture.rs, src/inspect/harness_files.rs (beyond the metadata-only grep), src/inspect/inventory.rs, src/inspect/mod.rs, src/inspect/output.rs, src/inspect/stack.rs, src/store/instruction_source.rs, src/bootstrap/tests.rs.
- Unreviewed test paths: tests/daemon_composed.rs, tests/daemon_free_reads.rs, tests/inspect_live.rs, tests/inspect_loaded.rs, tests/inspect_log.rs, tests/inspect_roster.rs, tests/inspect_sessions.rs, tests/inspect_stack.rs, tests/inspect_sweep.rs, and evidence/hg-blw.24/final-run.log.
- Unreviewed hunks: src/cli/ops.rs `@@ -189,6 +360,127 @@ mod tests {`; src/plan/ops.rs `@@ -655,13 +729,13 @@`, `@@ -682,7 +756,7 @@`, `@@ -865,7 +939,7 @@`, `@@ -951,13 +1025,180 @@` (tests).
- Unreviewed docs: every path under docs/superpowers/runs/2026-11-07-graph-state-inspection/ (design doc beyond the Requirement baseline and §2, roast records, coverage and findings ledgers, friction.md, and the inv-4b650ab2 and inv-64d86974 review archives).
- Codex B2/B5: byte equality of the SessionStart payload `session_id` with the occupant's native id is unverified against a real Codex (parked P1); a mismatch reads `unknown` with a note.
- daemon_composed::session_end_flows_to_transcripts timed out once in hg-blw.11 and passed on rerun, and no location was recorded; watch it in the full-suite sweep.
- The fit_hook partial-header cut case has no test (see Must fix).
- The B7 harness-file discovery rules (Codex Git-root bound, Claude ancestor walk, user-level origin and `env: inspector`) were not reviewed here; hg-blw.24 claims the user-level origin fix.

## Deferred OK
- Missing-evidence recurring cluster ×6 (hg-blw.7, .8, .5, .12, .11, .29): process records only, no product effect.
- src/daemon/doctor.rs `harness_hook_checks`: the Codex check warns even when the hook is current (it cannot read trust); `ok: true`, so doctor does not fail; consider wording it as informational.
- src/inspect/log.rs `LogQuery::run`: `--since` skips older commits instead of stopping (it tolerates clock skew), so a `--since` query that does not reach `--limit` scans up to `--max-scan`; output stays correct.
- src/inspect/log.rs `LogQuery::run`: a row marked `malformed: [tree]` counts toward `--limit` even when the commit might be out of scope; this is rare and visible.
- src/inspect/scope.rs `paths_at` (Teamspace): only the teamspace folder is in scope, so observed commits under archive/seats/ or transcripts/ for that teamspace's seats are attributed only through `action.affected`/`retired` (plausible; not confirmed against layout).
- hg-blw.7 dangling-parent integrity check over all records regardless of scope (a corrupt unrelated record fails a scoped view; exit 6 is honest).
- hg-blw.8 `ops --seat/--teamspace` resolves names before the daemon call, so with the daemon down it exits 1/2 instead of 5 (documented).
- hg-blw.5 perf risk: `paths_at` per scanned commit for scoped `log`; measure before adding a cache.
- hg-blw.1 `retired(reversible)` text gap; noisy error rows for retired seats under `--all`; `effective_manager` errors dropped to `manager: null`.
- hg-blw.3 brittle self-grepping source test and the `word()` empty-string fallback (unreachable for unit enums).
- hg-blw.6 unrelated rustfmt hunks in tests/daemon_composed.rs; degraded live text not checked through the CLI binary; additive `binding_from_earlier_server` field.
- hg-blw.9 loosened `reserved_slots_serialize` assertion and the synthetic `none_after_root` setup.
- hg-blw.2 non-hermetic `stack_is_read_only` (stats the real HOME), the early-return plus `unreachable!` control flow in cli/instructions.rs, and one `note` for an AGENTS.md that is both shadowed and a Claude fallback.
- hg-blw.12 untested Codex trust note (now covered by the setup.rs unit tests), the simulated occupant commit in tests/inspect_loaded.rs, and the hook flag-parity unit test covered at integration level.
- hg-blw.24 untested Codex ancestor-walk variant; `codex_bare` assumes a 19-character rollout timestamp (used only for the note, never as a match).
- hg-blw.22 rustfmt-only hunks, no red run, one leak count of 4 later back to 0, and the `lost` slot read under the snapshot lock.
- hg-blw.28 characterization test with no red run and the 10 s hook timeout asserted but not exercised.
- hg-blw.27 duplicate serde-word helpers in sessions.rs and stack.rs.
- hg-blw.23 stack's hard-coded `DEFAULT_MAX_SCAN`, silent `fill_last_changes` error drops, unit-only degrade coverage, and the seat_paths_at equivalence test limited to present seats.
- hg-blw.26 Scope::resolve Store→Integrity mapping covered only by the extracted-function unit test.
- Parked hg-blw.11: the `sessions --limit 1` flag-reach assertion cannot fail in a rig without sessions; `--since` already proves the flag reaches the sessions view.
