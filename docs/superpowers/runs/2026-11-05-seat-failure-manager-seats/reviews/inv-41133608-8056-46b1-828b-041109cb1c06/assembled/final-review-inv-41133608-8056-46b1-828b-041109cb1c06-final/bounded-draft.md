# Final review (bounded) — epic hg-k4l, branch super-auto/seat-failure-manager-seats

Base 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784, head 2d6490cf852199023eb0b54a34bad49476cabec5 (both verified with `git rev-parse --verify <sha>^{commit}`). Invocation inv-41133608-8056-46b1-828b-041109cb1c06; review source bound to the ledger before any write.

## Route and coverage

Bounded captured-source review. The selection sidecar (`review-inputs.json`) reports status unavailable, sourceAvailable true, wholeRangeReady false, no omissions, and two retained gaps: run.md:130 inv-26c52189-6b83-43ed-a202-49a87db495d9 "incomplete claimed collection" and "incomplete split bundle: missing cluster report or slices". The failed assembly and the original sidecar were left untouched. No whole-range package was built or loaded.

Changed-path inventory: 568 paths. About 483 are run artifacts under `docs/superpowers/runs/2026-11-05-seat-failure-manager-seats/` (412 of them under `reviews/`, 15 under `evidence/`). The other 85 are source, tests, templates and top-level docs. This pass read the captured diff hunks of 62 paths: every changed production source file under `src/` (not `*/tests.rs`), templates, the seat skill, README, CHANGELOG, CURRENT-DESIGN, specs INDEX, .gitignore and the `tests/e2e_private_herdr.rs` diff. It also read the requirement baseline and spec sections 1 to 4.5 of the design doc, and the run ledger. Test bodies were not reviewed.

## Requirement baseline trace (production entry points at head)

- B1, B14: `who`, `ops` (list, get, check-instruction), `request list` and `undo` candidates call the daemon only; every error goes through `cli::daemon_error::report` (src/cli/who.rs, ops.rs, request.rs, undo.rs). The direct-read fallbacks are removed.
- B2: `ClientError::Unavailable`, `Frame`, `Remote{Unavailable|VersionMismatch}` map to exit 5 with the "Retry in a bit; ... ask your manager or coordinator" line (src/cli/daemon_error.rs classify).
- B3: `seat` appends the "carry on ... run /seat again later" sentences on exit 5 (src/cli/seat.rs seat_failure), and the skill step 1 failure branch matches (skills/seat/SKILL.md). HOOK_PROMPT points at the failure step.
- B4: `StoreError::Integrity` comes from one helper `unresolved`. bootstrap (bound_paths, seat_parts, template_participation) and effective.rs `referenced_seat_template` raise it in place of the former Corrupt/silent skips. seat.resolve maps it to `IpcErrorCode::Integrity` and appends the node-manager suffix (src/bootstrap/mod.rs resolve_error, node_manager_suffix). The CLI maps it to exit 6.
- B5: `SeatRecord.manager`, `TeamspaceRecord.default_manager`, graph.toml `node_manager_seat` (serde default, skip if none). Resolution lives only in `model::manager::Pointers`.
- B6: `do_retire_seat` calls `plan::managers::reparent::reparent` before archiving the seat, so every route that uses it reparents. Plan previews list the same rewrites via `reparent_effects`: SeatRetire, CloneRetire last clone, TeamspaceRetire, ApplicationRetire, TemplateEdit withdrawals. The observed cascade reparents leniently.
- B7: pointer changes go through the ManagerEdit and NodeManagerSet plan kinds, or happen inside retirement mutations whose plans list them. Plan admission (`make_plan` -> `managers::admit`) runs the cycle guard and the node-manager guard for every kind, undo included.
- B8: `retire::refuse_node_manager_retirement` refuses seat.retire, seat.deactivate, runtime.close_tab, teamspace.retire and close_workspace on the NM, and refuses retiring or closing all of its clones. ManagerEdit refuses to give the NM a manager. NodeManagerSet clears the target's manager. `effective_manager` returns None for the live NM.
- B9: `who` renders a `manager:` chain line (threads::manager_line). `show <seat>` appends the same line, and a broken chain exits 6.
- B10 (with S1): `first_seat::assignment` is wired into SeatCreate, SeatResurrect, TeamspaceResurrect, ApplicationApply, TemplateEdit and undo restore. The office and node-manager templates ship in examples.
- B11: CascadeMutation keeps the live NM (ends and absents only its non-survived clones, keeps its teamspace Active). The planner and executor exempt the NM from the `moved_out` gate, and foreign-workspace placement is excluded. This was traced in code only (see the Untested scope section).
- B12: `node_manager_warning::warn` runs from `cli::main` for every command except init, setup, daemon and `seat --hook-prompt`. `plan node-manager set <seat>` exists.
- B13: NodeManagerSet rewrites seats whose explicit manager is the old NM, teamspaces whose default_manager is the old NM, and the old NM itself to the new one, and activates a dormant target.

On captured source, no baseline row is undelivered, judged by code reading.

## Own view: integration seams

- plan.create store errors: `src/plan/commands.rs:39` `plan_err` maps `PlanError::Store(s)` to `CommandError::internal` and bypasses the "one store-error classifier" `CommandError::from_store` (src/daemon/registry.rs:68). This epic adds new plan-time integrity sources: non-lenient `reparent_effects` -> `nearest_surviving_manager`, plus `Pointers` walks in NodeManagerSet and ManagerEdit. Example: `plan seat retire X` where X's explicit manager names a missing seat exits 1 ("herdr-graph: st_x: manager st_y not found ..."), not exit 6. The spec only requires the integrity mapping for seat.resolve (section 4.2). The CHANGELOG, however, says "/seat and every other command that calls the daemon now tell a transient outage from a broken graph ... Exit 6". The fix is a one-line routing through `from_store`, or else a narrower CHANGELOG claim.
- `who` output is now two lines (threads::WhoReply::render). `tests/real_agent_smoke.rs` `claude_pane_of` matches a pane when `who <pane>` stdout contains the sought seat name. With the manager line, a worker pane whose manager chain names that seat (for example "foreman") also matches. Test-only and plausible, not reproduced.
- CascadeMutation is idempotent when it handles the NM through more than one list: the NM seat in `seats` and its clones in `clones` after the teamspace loop are re-processed (end_occupant and absent again). This is harmless as written; `kept` double-counts but is only tested for > 0.
- docs/superpowers/specs/INDEX.md lists this spec as status "draft" while the branch implements it.

## Ledger triage

- Recurring minor (missing-evidence, ×9 across 9 tasks): process-class gap in task reports (no structured, tree-bound evidence). It does not affect the product. It weakens the claim that per-task tests ran on the tree that landed, so the final full-suite run is the only real measurement.
- Parked (hg-k4l.22, Important): `e2e_summary_lands_in_archived_seat_folder` and `e2e_undo_from_bound_pane_adopts_and_keeps_it` fail. The fixer says both fail identically on pre-epic main 0dbe477, and Recovery lines 474-475 record them as open deferred run failures. The undo test exercises undo restore from a bound pane, and this epic changed `compute_restore` and `restore_mutate` (first-seat assignment, pointer restores). A known failing test over changed code cannot be read as clean.
- hg-k4l.67 flaky-parallel-tests: `daemon_composed` subprocess::* and `daemon_ipc` fail intermittently under parallel `cargo test` (reproduced at base per report). No bead filed. The pending full-suite sweep is ambiguous until this is tracked.
- hg-k4l.6 plan-race-gap (pointers created after planning not rewritten): plan/apply.rs recomputes the plan at apply and rejects a differing effect list, so a new holder of the old NM makes the apply stale. Acceptable.
- hg-k4l.60 stale-record-clone (SeatResurrect clones `f.ts.rec` after `do_resurrect_seat`): `do_resurrect_seat` writes no teamspace record, so the clone is not stale. Acceptable.
- hg-k4l.61 known-limit (two-clone NM, one clone moved while home tab open, not marked moved_out): consistent with plain-seat behaviour, since moved_out is set only on tab closure with survivors, and the move warning still fires from `moves`. Acceptable as a documented limit.
- hg-k4l.4 cycle_from swallows Err: ManagerEdit resolves the new manager as a live seat first, and a dangling pointer surfaces as integrity elsewhere. Acceptable.
- hg-k4l.11 suffix-read-scope, hg-k4l.17 restore-skip-silent and chain-cycle-risk (admission covers undo plans), hg-k4l.27 preview-self-rewrite, hg-k4l.45 evidence logs committed, and the cosmetic and test-shape minors: acceptable to land.
- BLOCKED-AUTH lines: none in the ledger.
- Metrics: `landed≠completed 43 vs 2`, a malformed ledger-digest header and a Recovery gap at progress.md:741 mean the ledger is incomplete. The absence of findings there is not evidence of clean tasks.

## Verdict
not ready (bounded review only: whole-range review and full-suite run outstanding; parked failing e2e tests over changed undo code unresolved)

## Must fix before landing
- Parked hg-k4l.22: e2e_summary_lands_in_archived_seat_folder and e2e_undo_from_bound_pane_adopts_and_keeps_it (tests/e2e_private_herdr.rs) fail; the undo test covers restore paths this epic changed (src/undo/preview.rs compute_restore, src/undo/compensate.rs restore_mutate), so repair them or record a human disposition with a tracking bead and a base-vs-head run.
- hg-k4l.67 flaky daemon_composed subprocess::* and daemon_ipc tests under parallel cargo test have no tracking bead; file one and record base reproduction alongside the pending full-suite run so its result can be read.
- Whole-range final review is still outstanding: this bounded pass does not replace it (sidecar gaps at run.md:130 inv-26c52189-6b83-43ed-a202-49a87db495d9).

## Untested scope
- No full-suite measurement of head 2d6490cf852199023eb0b54a34bad49476cabec5 exists; the caller runs it after this invocation.
- Provenance gap: run.md:130 inv-26c52189-6b83-43ed-a202-49a87db495d9 incomplete claimed collection.
- Provenance gap: run.md:130 inv-26c52189-6b83-43ed-a202-49a87db495d9 incomplete split bundle (missing cluster report or slices).
- Ledger incomplete: landed≠completed 43 vs 2, malformed ledger-digest header, Recovery gap at progress.md:741.
- Unreviewed changed paths: every test body (src/*/tests.rs, src/plan/tests.rs, src/observe/tests.rs, src/plan/managers/tests.rs, src/model/tests.rs, src/undo/tests.rs, src/templates/tests.rs, src/threads/tests.rs, src/bootstrap/tests.rs, src/reconcile/tests.rs, src/store/tests.rs, src/writer/tests.rs, src/transcripts/tests.rs, tests/daemon_composed.rs, tests/manager_seats_sweep.rs, tests/daemon_error_commands.rs, tests/daemon_free_reads.rs, tests/node_manager_warning.rs, tests/seat_exit_codes.rs, tests/integration_sweep.rs, tests/instruction_writers.rs, tests/writer_concurrency.rs, tests/support/mod.rs).
- Unreviewed changed paths: inline #[cfg(test)] modules of new production files (src/model/manager.rs tests, src/plan/managers/first_seat.rs tests, src/cli/daemon_error.rs tests, src/cli/node_manager_warning.rs tests) were not read in full.
- Unreviewed changed paths: roughly 483 run artifacts under docs/superpowers/runs/2026-11-05-seat-failure-manager-seats/ (reviews/, evidence/, roasts, coverage, profiles, run.md); the design doc was read only for the requirement baseline and sections 1-4.5.
- B11 node-manager reopen after a real Herdr closure is not covered by the private-Herdr e2e suite (it opts the graph out of the node manager); only daemon_composed and unit tests exercise it.
- hg-k4l.58: no test for a multi-clone node manager where one clone closes while another stays bound (node_manager_closed still_bound branch).
- hg-k4l.26: lenient cascade reparent fallback with no live node manager is untested.
- hg-k4l.17: pointer_restores producing a manager cycle relies on plan admission and is untested.
- hg-k4l.2/hg-k4l.3: spawned-binary coverage of exit 6 and of exit 5 for mutating commands (cancel, reassign, request ack/complete, undo apply) was reported missing; whether later sweep tests closed it was not verified here.

## Deferred OK
- plan.create maps PlanError::Store(Integrity) to internal (src/plan/commands.rs:39), so plan-time integrity exits 1, not 6; spec §4.2 requires only seat.resolve, but route through CommandError::from_store or narrow the CHANGELOG "every other command" claim in a follow-up.
- tests/real_agent_smoke.rs claude_pane_of substring-matches `who` stdout, which now includes manager-chain names; tighten to the first line in a follow-up.
- docs/superpowers/specs/INDEX.md lists the spec as "draft" though implemented.
- Recurring missing-evidence minors across 9 tasks: process gap, no product impact.
- hg-k4l.6 plan-race-gap: covered by apply-time recompute in src/plan/apply.rs.
- hg-k4l.60 stale-record-clone: do_resurrect_seat writes no teamspace record.
- hg-k4l.61 two-clone node manager partial move not marked moved_out: consistent with plain-seat semantics.
- hg-k4l.4, hg-k4l.11, hg-k4l.17 restore-skip-silent, hg-k4l.27, hg-k4l.45, hg-k4l.22 string-prefix-filter, hg-k4l.67 weak-wait-condition and the remaining cosmetic or test-shape minors.
