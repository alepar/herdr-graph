# Final review (bounded) — epic hg-k4l, branch super-auto/seat-failure-manager-seats

Invocation inv-e03ec4c5-84cf-4206-b724-64be1d4173e8. Captured source: base 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784, head 6bfbb00df3654fdff2dcb6c2fcac7e431895a0bf (both verified with `git rev-parse --verify`). Review source bound in the ledger before any write.

## Route and method

The review-inputs sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, no omissions, and two retained provenance gaps for inv-26c52189-6b83-43ed-a202-49a87db495d9 (incomplete claimed collection; incomplete split bundle). This is therefore a bounded captured-source review. No whole-range package was built or loaded; the failed assembly and its sidecar were left untouched.

The range changes 485 paths: 405 run-record files under docs/superpowers/runs/2026-11-05-seat-failure-manager-seats (about 120k added lines), 4 evidence logs under evidence/hg-k4l.45, and 76 source, test and doc paths. I read every hunk of every changed production file under src/ (tests modules excluded), the new plan::managers and model::manager modules in full, cli::daemon_error and cli::node_manager_warning in full, the skill, README, CURRENT-DESIGN.md, specs INDEX, tests/integration_sweep.rs and tests/support/mod.rs, and the design doc including its Requirement baseline. Test modules (src/*/tests.rs, src/plan/managers/tests.rs, tests/*.rs other than the two above) were read only as function-name inventories, not as inspected hunks. Beyond the diff I traced first-seat call sites, plan-effect kinds and verbs, EXAMPLE_FILES, and the bead status of every hg-k4l child.

## Requirement baseline trace

- B1, B2, B3, B14: who, ops (list/get/check_instruction), request list and undo candidates no longer read the graph directly; every daemon-calling command touched routes through cli::daemon_error, which maps Unavailable, Frame, Remote{Unavailable}, Remote{VersionMismatch} to exit 5 with the retry-later text. Delivered from the real CLI entry points.
- B4: StoreError::Integrity via `unresolved`, IpcErrorCode::Integrity, CommandError::from_store; the three silent skips in bootstrap/instructions.rs (template_ref, missing application, missing application template) now fail; seat_reply maps integrity to exit 6 with the node-manager suffix and no partial reply. Delivered.
- B5: optional `manager` / `default_manager` / `node_manager_seat` pointers resolved only through model::manager; seat.resolve carries `manager` and `node_manager` SeatRefs with threads_seat. Delivered.
- B6: reparent runs inside do_retire_seat for every retirement route; previews list reparent_effects in SeatRetire, TeamspaceRetire, CloneRetire (last clone), TemplateEdit withdrawal and ApplicationRetire; rewrites recorded under `reparented`; undo restores only pointers still holding the reparent value. Delivered.
- B7: ManagerEdit and NodeManagerSet are OrgKinds through make_plan; observed-cascade reparenting rides on the sanctioned non-plan containment retirement as the spec states. Delivered.
- B8: make_plan admission refuses seat.retire, seat.deactivate, runtime.close_tab of the node manager, teamspace.retire / runtime.close_workspace of its teamspace, and clone.retire / runtime.close_pane covering all of its live clones; ManagerEdit refuses the node manager as subject; NodeManagerSet clears the new holder's manager. Delivered.
- B9: who and show render `manager: a (st) -> ... -> root`; show maps chain integrity to exit 6. Delivered.
- B10: NOT delivered. Only SeatCreate calls first_seat::assignment (src/plan/core_kinds.rs:996, 1063, 1084). ApplicationApply (create_member_seat) and TemplateEdit member additions do not (hg-k4l.24 open), so on a fresh graph `application apply` creates the first seats with no node manager and the graph falls into the warning state. No `node-manager` seat template or `office` team template ships in EXAMPLE_FILES (hg-k4l.14 open), so the template route the human named does not exist. S1 narrows "seated" to "created"; application apply creates seats, so S1 does not cover the gap.
- B11: the observer exempts the node manager from tab, pane and workspace cascades (classify.rs) and CascadeMutation re-checks live_node_manager at apply; reopening relies on the existing reconcile planner. Delivered in code; the reopening tests in src/observe/tests.rs were inventoried by name only.
- B12: `plan node-manager set <seat>` exists; cli::main prints the unassigned warning before dispatch for every command except init, setup, daemon and `seat --hook-prompt`. Delivered.
- B13: NodeManagerSet rewrites explicit manager pointers and teamspace defaults naming the old holder, points the old holder at the new one, and clears the new holder's own manager; seats that reached the old holder by fallback follow automatically. Delivered.

## Cross-task integration observations

- The first-seat split (hg-k4l.7 into .23/.24/.25) landed only its first third, yet first_seat.rs's module doc, CURRENT-DESIGN.md ("the first seat created on a graph that has none is assigned the role") and README ("The node manager is the first seat created on a graph") describe the complete behaviour. Docs and code disagree at head.
- Five epic tasks are still open at the reviewed head: hg-k4l.14 (shipped templates), hg-k4l.19 (seat.resolve `reports`, node-manager self-identification and duty text), hg-k4l.22 (integration sweep), hg-k4l.24 and hg-k4l.25 (first-seat helper in template, resurrect and undo-restore paths). The ledger's completion metrics count only completed tasks, so they read clean while scope is outstanding.
- The fixture opt-out (plan::managers::testing::opt_out, tests/support::node_manager_opt_out) writes a dangling node_manager_seat so legacy fixtures keep their meaning; it is test-only and does not leak into production paths. It does mean those fixtures run with the unassigned warning active.
- cli::daemon_error's source-scan guard still allow-lists who.rs, ops.rs, request.rs and undo.rs under AWAITING_HG_K4L_3 (src/cli/daemon_error.rs:282) after hg-k4l.3 merged, so the guard does not protect the four commands Part C changed. Verified at head: none of them matches ClientError::Unavailable today.
- Composition of the admission guards is sound on inspection: retire and cycle guards both run once in make_plan, which covers the apply-time replan and undo; the cycle guard maps seat.create, seat.resurrect, seat.manager, teamspace.default_manager, graph.node_manager_seat and seat.retire, so undo pointer restores are also cycle-checked at plan time.
- Outside Herdr with no daemon, who/ops/request/undo now exit 2 with "not inside a Herdr pane" rather than reading the graph. That is the spec §4.4 table, but an operator in a plain terminal loses those reads entirely; status remains the diagnostic.

## Ledger triage

- Ledger lines 107 and 118 (stale fallback sites) are resolved at head by hg-k4l.3; the remaining artefact is the AWAITING_HG_K4L_3 allow-list above.
- Ledger line 282 (evidence logs committed): evidence/hg-k4l.45/{final,integ,lib-after,lib-before}.log, 3157 lines of test output, land in the repository tree. Graded must-fix: cheap to drop and permanent once merged.
- Ledger line 188 (NodeManagerSet plan race): a seat pointed at the old holder after planning changes the recomputed effect list, and the graph_sha in the node-manager effect makes a changed graph.toml differ, so apply-time replan should reject stale_plan. Acceptable.
- Ledger lines 289 and 290 (undo restore skip, chain-cycle risk): the replan and the make_plan cycle guard cover both; untested, recorded below.
- Recurring minor `missing-evidence` (hg-k4l.4, hg-k4l.18, hg-k4l.3) and the no-red-run entries are process records, not product defects.
- No parked, BLOCKED-AUTH or Recurring blocker lines in the ledger.

## Verdict
not ready (bounded review: baseline row B10 undelivered for application apply and shipped templates; five epic tasks open; no full-suite run)

## Must fix before landing
- B10 not delivered: first_seat::assignment is called only from SeatCreate (src/plan/core_kinds.rs:996/1063/1084); ApplicationApply via create_member_seat and TemplateEdit member additions never assign the node manager (hg-k4l.24 open), so `application apply` on a fresh graph leaves no node manager.
- B10 not delivered: no shipped `node-manager` seat template or `office` team template in EXAMPLE_FILES (src/bootstrap/examples.rs; hg-k4l.14 open), so "graph templates provide default templates which create the first seat with the node manager role" has no producer.
- Spec §4.8 incomplete: SeatResurrect, TeamspaceResurrect and undo restore do not use the first-seat helper (hg-k4l.25 open), while src/plan/managers/first_seat.rs module doc, CURRENT-DESIGN.md and README state the full behaviour; land the code or correct the docs.
- Spec §4.12 incomplete: seat.resolve has no `reports` list, does not tell the node manager it holds the role, and skills/seat/SKILL.md carries no manager duties (listen, detect, unblock) from the human request (hg-k4l.19 open).
- src/cli/daemon_error.rs:282 AWAITING_HG_K4L_3 still exempts who.rs, ops.rs, request.rs and undo.rs from the no-Unavailable source scan after hg-k4l.3 merged; drop the list so the guard covers Part C.
- evidence/hg-k4l.45/*.log (3157 lines of test output) are committed into the repository tree (ledger line 282); remove them before landing.

## Untested scope
- No full-suite measurement of this head exists; the sweep is deferred to the caller and hg-k4l.22 (integration sweep) is open, so the branch is untested end to end.
- Provenance gap: 6bfbb00df3654fdff2dcb6c2fcac7e431895a0bf:docs/superpowers/runs/2026-11-05-seat-failure-manager-seats/run.md:81 inv-26c52189-6b83-43ed-a202-49a87db495d9: incomplete claimed collection.
- Provenance gap: 6bfbb00df3654fdff2dcb6c2fcac7e431895a0bf:docs/superpowers/runs/2026-11-05-seat-failure-manager-seats/run.md:81 inv-26c52189-6b83-43ed-a202-49a87db495d9: incomplete split bundle: missing cluster report or slices.
- Bounded review: no whole-range review has been performed; a whole-range final review remains outstanding.
- Unreviewed hunks: all test modules (src/bootstrap/tests.rs, src/model/tests.rs, src/observe/tests.rs, src/plan/tests.rs, src/plan/managers/tests.rs, src/reconcile/tests.rs, src/store/tests.rs, src/templates/tests.rs, src/threads/tests.rs, src/transcripts/tests.rs, src/undo/tests.rs, src/writer/tests.rs) and tests/{daemon_composed,daemon_error_commands,daemon_free_reads,instruction_writers,node_manager_warning,seat_exit_codes,writer_concurrency}.rs.
- Unreviewed hunks: evidence/hg-k4l.45/{final,integ,lib-after,lib-before}.log and the 404 run-record files under docs/superpowers/runs/2026-11-05-seat-failure-manager-seats other than the design doc.
- No spawned-binary test for exit 6 or a still-starting daemon (ledger line 106); exit 5 for mutating commands cancel, reassign, request ack/complete and undo apply untested (ledger line 127).
- Daemon-backed `who` manager line and `show` exit 6 have no end-to-end test (ledger line 151).
- Lenient observed-cascade reparent fallback with no live node manager untested (ledger line 179); ManagerEdit retired-subject branches untested (ledger line 169).
- Undo pointer restore that would form a cycle is untested (ledger line 290); node-manager reopening after tab/workspace close was inventoried by test name only; tier-3 private Herdr runs not observed.

## Deferred OK
- Ledger 87/88: StoreError::because and CommandError::from_store; both are now used (manager cycle text, who and seat_reply).
- Ledger 101: short_rev length 12 vs brief example 7; cosmetic.
- Ledger 108: NotInHerdr text without prefix; moot now that every caller goes through cli::daemon_error.
- Ledger 112: cycle_from ends a walk on a dangling pointer; ManagerEdit validates the manager exists and manager_chain reports dangling pointers.
- Ledger 132/133: setter unit tests and the then-unused NODE_MANAGER_SET_USAGE; the constant is now the single spelling in the warning and refusals.
- Ledger 138: node_manager_suffix loads all pointers, degrading to the user form on an unrelated corrupt record; acceptable.
- Ledger 144: retire_seat needs allow(clippy::too_many_arguments); style.
- Ledger 150/158/159/160/187/194: duplicated test helpers, weak assertions and test placement; test-quality cleanups.
- Ledger 188: NodeManagerSet plan race covered by apply-time replan and graph_sha.
- Ledger 200: seat.resurrect modelled as un-retire keeping the explicit manager; correct.
- Ledger 206: TeamspaceRetire preview may list a default_manager rewrite on the teamspace being retired; apply does the same.
- Ledger 283: integration_sweep compiles `mod support` unconditionally; required for the opt-out helper.
- Ledger 289: restore_mutate skips pointers changed since planning silently; apply-time replan makes that case stale.
- Ledger 295: verified at head that show/list/path stay daemon-free and the warning's exclusions match `applies()`.
- Recurring minor missing-evidence (hg-k4l.4, hg-k4l.18, hg-k4l.3) and the no-red-run entries: process records.
- node_manager_warning::warn loads every seat record on each CLI invocation; a cost, not a defect.
