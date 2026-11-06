# Final review (bounded) — epic hg-k4l, branch super-auto/seat-failure-manager-seats

## Review basis

Captured source: base 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784, head 88b79e9e93fa99ff2a116b57aacaeed7a4926f6f (both verified with `git rev-parse --verify`). Caller checkout observed at HEAD 88b79e9, clean, before reading. bind-review-source succeeded.

This is a bounded captured-source review, not a whole-range review. The selection sidecar (review-inputs.json) reports status unavailable, sourceAvailable true, wholeRangeReady false, no omissions, and two retained provenance gaps on run.md:129 (inv-26c52189-6b83-43ed-a202-49a87db495d9: incomplete claimed collection; incomplete split bundle). No whole-range package was built or loaded. The changed-path inventory is 560 paths: 81 product, test, template and doc paths outside docs/superpowers/runs, and 479 run-archive paths under docs/superpowers/runs/2026-11-05-seat-failure-manager-seats (review bundles of earlier invocations, roast reports, coverage ledgers). Inspected hunks are listed in bounded-coverage.json; everything else is unreviewed here.

Spec read: bd show hg-k4l and the design doc at head (docs/superpowers/runs/2026-11-05-seat-failure-manager-seats/2026-11-05-seat-failure-manager-seats-design.md), including its Requirement baseline (B1–B14, S1).

## Requirement baseline trace (within inspected scope)

- B1, B14: who (src/cli/who.rs), ops (src/cli/ops.rs:65), request (src/cli/request.rs:136,147) and undo (src/cli/undo.rs:39,106,128,138) route every ClientError through cli::daemon_error::report; the who direct-read fallback is deleted. status keeps its fallback as the spec allows. Delivered.
- B2: daemon_error::classify maps Unavailable, Frame, Remote{Unavailable,VersionMismatch} to exit 5 with the retry-later sentence. Delivered.
- B3: seat_failure appends the carry-on sentences for exits 5 and 6; skills/seat/SKILL.md step 1 has the failure branch; HOOK_PROMPT names it. Delivered.
- B4: IpcErrorCode::Integrity is reached through resolve_error in seat_reply; bound_paths uses unresolved() for seat, clone and teamspace. The silent-skip conversions in src/bootstrap/instructions.rs were not inspected here.
- B5, B9: seat.resolve carries manager, node_manager, reports; who renders manager_chain; show appends the manager line for seats. Delivered.
- B6: do_retire_seat calls plan::managers::reparent for every route; TeamspaceRetire fills acc.retiring before the loop; CascadeMutation fills it and removes the live node manager. Delivered.
- B7: pointer writes go through ManagerEdit and NodeManagerSet plan kinds and the retirement path; make_plan runs managers::admit. Delivered.
- B8: refuse_node_manager_retirement refuses seat.retire, seat.deactivate, runtime.close_tab, teamspace.retire, runtime.close_workspace and last-clone retire or close for the live node manager; ManagerEdit refuses a manager on the node manager. Delivered.
- B10, S1: first_seat::assignment is wired into SeatCreate, TeamspaceResurrect, SeatResurrect, both template kinds and undo restore; office and node-manager templates are in EXAMPLE_FILES. Delivered at the call-site level; template-kind hunks not inspected in depth.
- B11: CascadeMutation keeps the live node manager (ends only non-survived clones, keeps seat and teamspace Active); planner gates_recreation ignores moved_out for the node manager and foreign_to excludes other teamspaces' workspaces. Delivered in code; the FakeHerdr tests for spec §4.9 (1)–(5) were not run by this review.
- B12: plan node-manager set exists; cli::main calls node_manager_warning::warn before dispatch. Delivered.
- B13: NodeManagerSet rewrites explicit managers and teamspace defaults naming the old node manager, points the old node manager at the new one, and clears the new one's own manager. Delivered.
- No baseline row was found undelivered within the inspected scope.

## Findings

- Important: seat_reply now calls Pointers::load (every seat and teamspace record plus graph.toml) for every /seat (src/bootstrap/mod.rs hunk @@ -260,7 +332,25 @@), and who/show do the same through named_manager_chain; a malformed TOML in any one unrelated seat record now makes /seat exit 1 for every seat on the node, where before only the broken seat's own records were read. Task 9's deferred [suffix-read-scope] is one symptom of this. Not a baseline violation (spec §4.2 keeps read failures Internal), but it widens the failure domain of the one command every seat runs at startup. Recommend a follow-up that scopes the load or degrades the manager fields instead of failing the reply.
- Minor: node_manager_warning::warn runs Pointers::load (all seat records) from git on every CLI invocation except init/setup/daemon/hook-prompt, session-report included; cost grows with graph size. Acceptable now; note for later.
- Minor: in the observed Tab cascade for the node manager, retire_seat ends its clones and then retire_clone visits the same clones again; end_occupant is idempotent (occupant is taken), so this is harmless, and the kept counter over-counts without effect.

## Ledger triage

- Parked (Task 22, hg-k4l.22): the tier-3 tests e2e_summary_lands_in_archived_seat_folder and e2e_undo_from_bound_pane_adopts_and_keeps_it (tests/e2e_private_herdr.rs) were parked as pre-existing with no run at base recorded; the undo test exercises undo-restore behavior this epic changed (hg-k4l.25). A known-failing test is must-fix: run both at base 0dbe477 and head 88b79e9 and record the results, or fix them.
- Recurring minor, missing-evidence, x9 across 9 tasks: per-task test claims are prose-only with no tree-bound evidence. Process class; acceptable to defer, but it means no task-level pass claim can stand in for the full-suite sweep.
- Task 11 [plan-race-gap] (node-manager set misses holders created after planning): likely closed by the apply-time recompute that graph_sha feeds (node_manager_effect doc comment); this review did not verify the recompute path. Deferred OK pending that check.
- Task 19 [chain-cycle-risk]: make_plan runs managers::admit over every plan's effects (src/plan/kind.rs @@ -70,6 +70,7 @@), so restored seat.manager effects in an undo plan are cycle-checked; untested. Deferred OK.
- Task 19 [restore-skip-silent], Task 36 [known-limit-unhandled] (node_manager_move_warning still fires via observed moves), Task 4 [silent-error-swallow] (ManagerEdit only accepts live targets), Task 27 [preview-self-rewrite], Task 35 [stale-record-clone], Task 38 [unreachable-filter-edge]: Deferred OK.
- Task 2 [missing-cli-test], Task 5 [missing-edge-case-test] exit-5 for mutating commands, Task 10 [missing-integration-test], Task 34 still_bound branch, Task 26 lenient fallback with no node manager, Task 40 cycle through a retiring ancestor: test-coverage gaps, listed as untested.
- Task 28 [evidence-logs-committed]: about 3000 lines of evidence logs committed in the tree; cleanup before landing is advisable but not blocking.
- No BLOCKED-AUTH lines in the ledger. The Metrics ledger-check line reports a recovery gap at progress.md:741, so absence of entries there is not proof of a clean ledger.

## Verdict
not ready (bounded review only; parked failing tier-3 e2e tests unresolved; no full-suite measurement of head 88b79e9 exists)

## Must fix before landing
- Resolve the parked Task 22 item: run e2e_summary_lands_in_archived_seat_folder and e2e_undo_from_bound_pane_adopts_and_keeps_it (tests/e2e_private_herdr.rs) at base 0dbe477 and head 88b79e9 and record results, or fix them; the undo test covers behavior hg-k4l.25 changed.
- Complete a whole-range review of 0dbe4772..88b79e9: this bounded review did not cover the unreviewed paths and hunks or the provenance gaps listed below.
- Run the full suite (cargo test, plus private-Herdr tier 3 where available) on head 88b79e9 and record it; no full-suite measurement of this branch exists yet.

## Untested scope
- No full-suite measurement of head 88b79e9 exists; the caller's sweep runs after this invocation.
- Provenance gap: 88b79e9:docs/superpowers/runs/2026-11-05-seat-failure-manager-seats/run.md:129 inv-26c52189-6b83-43ed-a202-49a87db495d9 incomplete claimed collection.
- Provenance gap: 88b79e9:docs/superpowers/runs/2026-11-05-seat-failure-manager-seats/run.md:129 inv-26c52189-6b83-43ed-a202-49a87db495d9 incomplete split bundle (missing cluster report or slices).
- Unreviewed product paths: src/bootstrap/instructions.rs, src/bootstrap/tests.rs, src/cli/instructions.rs, src/cli/ops.rs, src/cli/plan.rs, src/cli/request.rs, src/cli/status.rs, src/cli/undo.rs, src/daemon/client.rs, src/daemon/doctor.rs, src/daemon/registry.rs, src/ipc.rs, src/model/change.rs, src/model/effective.rs, src/model/graph.rs, src/model/mod.rs, src/model/seat.rs, src/model/teamspace.rs, src/plan/kinds_extra.rs, src/plan/mod.rs, src/ports/store.rs, src/reconcile/herdr_exec.rs, src/store/init.rs, src/store/instruction_source.rs, src/templates/kinds.rs, src/templates/structure.rs, src/writer/mod.rs.
- Partly reviewed: src/plan/core_kinds.rs (only the do_retire_seat and TeamspaceRetire hunks), src/undo/preview.rs (only the pointer_restores hunk).
- Unreviewed test paths: src/model/tests.rs, src/observe/tests.rs, src/plan/tests.rs, src/plan/managers/tests.rs, src/reconcile/tests.rs, src/store/tests.rs, src/templates/tests.rs, src/threads/tests.rs, src/transcripts/tests.rs, src/undo/tests.rs, src/writer/tests.rs, and every file under tests/ (daemon_composed, daemon_error_commands, daemon_free_reads, e2e_private_herdr, instruction_writers, integration_sweep, manager_seats_sweep, node_manager_warning, seat_exit_codes, support/mod.rs, writer_concurrency).
- Unreviewed docs and config: .gitignore, CHANGELOG.md, CURRENT-DESIGN.md, README.md, docs/superpowers/specs/INDEX.md, templates/node-manager/AGENTS.md, and the 479 run-archive paths under docs/superpowers/runs/2026-11-05-seat-failure-manager-seats.
- Spec §4.9 FakeHerdr tests (1)–(5) and the observed node-manager reopen path were read in code only, not executed.
- Ledger test gaps: spawned-binary exit 6 and still-starting daemon (Task 2), exit 5 for mutating commands (Task 5), daemon-backed who manager line and show exit 6 end to end (Task 10), node manager still_bound branch (Task 34), lenient reparent fallback with no node manager (Task 26), cycle through a retiring ancestor (Task 40), pointer_restores cycle (Task 19).

## Deferred OK
- Pointers::load blast radius in seat_reply, who and show (see the Important item above): follow-up bead to scope the load or degrade the manager fields.
- node_manager_warning per-invocation full seat load: revisit if graphs grow large.
- Recurring missing-evidence process minor (x9 across 9 tasks).
- Task 11 plan-race-gap, subject to confirming the apply-time recompute.
- Task 19 restore-skip-silent and chain-cycle-risk; Task 36 known-limit-unhandled; Task 4 silent-error-swallow; Task 27 preview-self-rewrite; Task 35 stale-record-clone; Task 38 unreachable-filter-edge.
- Task 28 committed evidence logs (cleanup advisable).
- Remaining cosmetic and test-style minors in the ledger (Tasks 1, 3, 6, 7, 8, 10, 14, 15, 17, 22, 23, 24, 25, 30, 32, 43, 44, 37).
