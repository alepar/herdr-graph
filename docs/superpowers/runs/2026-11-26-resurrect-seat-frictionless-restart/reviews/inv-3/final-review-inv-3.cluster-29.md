# Final review inv-3, cluster 29 (src-reconcile)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified with `git rev-parse --verify <sha>^{commit}`). Review source bound with `dispatch-text.mjs bind-review-source` (exit 0). Caller checkout observed at HEAD 11ac415, with one untracked path `evidence/sweep/` (the background test sweep), before and during the review. No cargo was run; the sweep's own log was read.

Scope: `src/reconcile/herdr_exec.rs`, `src/reconcile/planner.rs`, `src/reconcile/tests.rs` (slices 000188-000190). Spec: `docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md` with its Requirement baseline (B1-B4, no human scope changes) and Post-Implementation Notes.

## What the cluster changes

- `keyless_start_gate` (herdr_exec.rs) holds a non-relaunch `StartAgent` of a seated keyless clone with no seat intent, inside the 90 s grace, until the fact reads `Resolved` on the clone's own pane, read at or after `incarnation:changed_at`, and `seat_unresolved:<clone>` names this incarnation. It re-polls every 5 s, capped by the grace left. Keyed and unseated clones and clones with a seat intent skip it. `RelaunchOccupant` keeps its unchanged full-grace deferral. This matches spec section 4 "Gate" as amended by the hg-lnw.27 note (a resumed relaunch waits the full grace).
- `plan_resume` (planner.rs) records `seat_unresolved:<clone>` on the first fresh `Unresolved` read of the incarnation. Because the meta comes from an earlier or equal read and the gate reads the current fact, a `Resolved` fact seen alongside it is necessarily a later read. That is the freshness argument the spec gives.
- `restart_covered` counts a `launched:` meta as having had an agent for non-shell harnesses (spec 4.3). It is threaded into both `start_needed` and `plan_resume`.
- `rebind_refused_key` / `rebind_refused_in` / `seat_unresolved_key` / `seat_unresolved_in` are journal-meta helpers. The rebind effect and the `restart_held` row that consume `rebind_refused` live in the threads and daemon clusters.
- hg-lnw.3 / spec 4.4: a keyless clone's restore launch no longer records `launched:`, on the dispatch path and (since hg-lnw.30, cd0e869) on the lost-response `Unknown` re-run. The re-run identifies the restore launch by `SeatIntent::Restore { epoch, launched: Some(_) }` with `epoch == e.op`. That holds because `plan_resume` mints the restore `StartAgent` with `mk_as(epoch, ...)`. So the ordinary `start_needed` relaunch-on-exit rule still applies to an undo-restored adopted clone. This closes the inv-2 cluster-30 Must fix.

## Production trace for the baseline rows

- B2 (frictionless restart), reconcile side: the production reconcile loop plans through `plan_effects`. For a keyless no-session or never-observed clone, `start_needed`/`plan_resume` emit a `StartAgent`/`RelaunchOccupant`, and `HerdrExecutor::start` applies the gate, which opens on placement or at the grace. No operator step is needed on this path. For keyed clones the relaunch never waits on the gate. For shell seats nothing launches. A missing pane keeps `resume_blocked`. Emitting `threads.rebind_held_seat` and running it is in the threads cluster (`src/threads/effects.rs`), and the `clones.restart_held` row is in the daemon cluster (`src/daemon/clone_status.rs`). Those paths leave this cluster and are the seam reviewer's to trace. The reconcile tests here drive them through the fake threads (`restarted(...)` harness) and do not stub them out.
- B1 (resurrect on undo), reconcile side: the restore launch keeps its `threads_done` gate and once-per-epoch mark (`restore_epoch_launches_at_most_once_via_restore_path`). After a keyless restore launch the ordinary exit-relaunch rule still applies (`undo_restored_agent_that_exits_is_relaunched` and the lost-response variant). The resurrect want itself is in the threads cluster (seam).
- B3, B4: no reconcile code. Not applicable to this cluster.
- No row is undelivered within this cluster.

## Evidence consulted

- `evidence/sweep/sweep-11ac415754b5edcedf3790ca4aa050e9fbeabba5.log` (untracked, written by the in-progress sweep at head): the lib run reports `1550 passed; 0 failed`. That includes the four hg-lnw.3/.30 tests and the restart-gate suite. The sweep was still running when it was read, so later tiers were not judged here.
- The spec section 4 table rows map to the tests `restart_keyed_session_seat_rebinds_and_resumes_without_waiting` (rows 1-2), `restart_keyless_session_seat_waits_for_placement_then_resumes` (row 3, amended), `restart_keyless_no_session_starts_fresh_on_the_original_seat` (row 4, no-session half), `restart_shell_seat_is_rebound_and_nothing_launches` (row 5), `restart_never_observed_keyed_clone_rebinds_then_starts_fresh_keyed` (row 7), `restart_never_observed_keyless_clone_starts_after_placement` (row 8) and `restart_missing_pane_keeps_resume_blocked` (row 9). The gate fallbacks are covered by the tests for the stale-`Resolved` lag, always-erroring, never-emitted, unreachable and never-`Unresolved` cases.

## Concerns

- The spec section 4 row "adopted (keyless), no session, or resume fails" promises a fresh start after a failed resume. No reconcile path does that. `RelaunchOccupant` sets `resumed:<clone>` (one relaunch per incarnation) and records `launched:`, so after a `--resume` that exits at once, neither `plan_resume` nor `start_needed` starts the agent again. The seat is still placed by the rebind, so the threads hold the user objected to (B2) is gone. Only the agent stays down until someone starts it. Keyed clones behave the same way, and did at base. This is a spec-table gap, not a B2 regression, but nothing tests it or documents it.
- The `RelaunchOccupant` and post-restart ordinary start paths still record `launched:` for keyless clones. After a restart relaunch, an adopted clone whose agent later exits with the pane open is no longer relaunched by `start_needed`, the same effect hg-lnw.3 removed for the restore path. This is pre-existing at base, and spec 4.4 covers only the undo-restore case.
- The hg-lnw.30 detection relies on the Restore intent still being present at the `Unknown` re-run. If `ClearOnReattach` lands between the lost response and the re-run, `seat_intent` is `None` and `launched:` is recorded, so the hg-lnw.3 break returns on that interleaving. The window is about one reconcile pass against an agent's check-in time, so it is unlikely. The test runs the clear only after the re-run.
- `restart_covered` now accepts `launched:` alone, and `incarnation:changed_at` is never cleared. A keyed clone whose agent graph launched but which never reported a session can therefore get one `RelaunchOccupant` per incarnation whenever its fact reads `Unresolved` after an exit, long after the restart itself. The same already held for clones with sessions, so the rule is consistent. It is just broader than "after a restart".

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test covers the "resume fails" half of spec section 4 row 4 (a keyless `--resume` relaunch whose agent exits at once); the code does not start a fresh agent there.
- No test covers `ClearOnReattach` landing before the lost-response re-run of a keyless restore `StartAgent`.
- No test covers a keyless gate whose fact reads `Resolved` on a pane other than the clone's own, inside the grace; the code keeps it closed, but nothing asserts it.
- No test covers an adopted keyless clone after a restart `RelaunchOccupant` whose agent later exits with the pane open, where it is not relaunched.
- The sweep at head was still in progress when read; only its lib tier is confirmed green here.

## Deferred OK
- Decide whether a failed `--resume` after a restart should fall back to a fresh start (spec section 4 row 4), or amend the row; this is pre-existing behaviour for keyed clones too.
- Exempt keyless clones from `record_launch` on restart relaunches too, or record the restore and restart launches in a way `start_needed` can tell apart, so an adopted clone keeps relaunch-on-exit after a Herdr restart (pre-existing, outside spec 4.4).
- Make the hg-lnw.30 restore-launch detection independent of the intent still being present at the `Unknown` re-run (for example, by persisting that the effect was a restore launch when it is marked).
