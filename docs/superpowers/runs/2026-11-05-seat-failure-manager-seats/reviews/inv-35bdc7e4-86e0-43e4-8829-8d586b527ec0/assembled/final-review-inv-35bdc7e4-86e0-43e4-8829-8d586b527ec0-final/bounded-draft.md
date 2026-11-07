# Final review (bounded) — epic hg-k4l, super-auto/seat-failure-manager-seats

Captured source: base 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784, head 96a4657f49405a93fd7b288d51fe168f00a2a0b0 (both verified with `git rev-parse --verify`). Caller checkout observed at 96a4657, status clean before this review wrote its own bundle files.

## Scope of this bounded pass

The review-input sidecar is `status: unavailable`, `sourceAvailable: true`, `wholeRangeReady: false`, no omissions, two retained gaps (run.md:122, inv-26c52189: incomplete claimed collection; incomplete split bundle). This is therefore a bounded captured-source review, not a whole-range review. I read the full changed-path inventory (551 paths: 91 outside `docs/superpowers/runs`, the rest run archives) and inspected the production hunks listed in bounded-coverage.json from captured Git. Test files, evidence logs, run archives and several plan/managers files were not inspected (listed under Untested scope). No build or test was run; no full-suite measurement of this head exists (the sweep is deferred to the caller).

## Requirement baseline trace (code-level, from production entry points)

- B1/B14: `who`, `ops` (list/get/check-instruction), `request list` and `undo` candidates no longer read the graph directly; every daemon failure goes through `cli::daemon_error::report`. `status` keeps its fallback as specified. Delivered in code.
- B2/B3: `daemon_error::classify` maps Unavailable, Frame, Remote{Unavailable, VersionMismatch} to exit 5 with the retry text; `seat` adds "carry on ... run /seat again later"; the seat skill gained the exit-5/6 failure branch and HOOK_PROMPT names it. Delivered in code.
- B4: `StoreError::Integrity` via `unresolved`, the three silent skips in `template_participation` now fail, `referenced_seat_template` failures map to integrity, `resolve_error` adds HEAD and the node-manager suffix, IPC code `integrity` without an IPC_VERSION bump. Delivered in code.
- B5/B7/B8: pointers on seat/teamspace/graph.toml, resolution only in `model::manager`, writes only through ManagerEdit / NodeManagerSet / retirement routes; `managers::admit` in `make_plan` refuses retiring, deactivating or closing the live node manager and cyclic chains; `effective_manager` returns None for the node manager. Delivered in code.
- B6: `do_retire_seat` calls `reparent` for every route; previews use `reparent_effects`; rewrites recorded under `reparented`; undo restores via `pointer_restores`. Delivered in code.
- B9: `who` renders the manager line from `manager_chain`; `show <seat>` appends it and maps integrity to exit 6. Delivered in code.
- B10/S1: `first_seat::assignment` wired into SeatCreate, ApplicationApply, TemplateEdit additions, SeatResurrect, TeamspaceResurrect and undo restore; shipped `node-manager` seat template and `office` team template added to EXAMPLE_FILES. Delivered in code.
- B11: plan guard refuses placement changes; observed closures keep the node manager (decided at apply time in `CascadeMutation`), teamspace stays Active with runtime absent, and reopening relies on the existing reconcile planner. Delivered in code; runtime reopening was not exercised here.
- B12/B13: `node_manager_warning::warn` runs from `cli::main` for every command except init/setup/daemon/`seat --hook-prompt`; `node-manager set` assigns or moves, rewriting explicit reports and teamspace defaults, pointing the old node manager at the new one and clearing the new one's own manager. Delivered in code.

## Cross-task seam: observed workspace closure of a seat whose pane moved out

hg-k4l.61 (c14b8a8) changed the workspace branch of `observe::classify` to drop every surviving clone from the cascade, for every seat in the closed workspace, not only the node manager: `seat_group(d, &seat.id, CascadeRule::Workspace, true, |c| pane_survives(cx, c))`. The surviving clone therefore gets no `PaneClosed` end fact and is not in the cascade's retire list. `CascadeMutation::retire_seat` only honours the `closed` set when `nm == Some(seat)`; for any other seat it still ends the occupant of every live clone (`end_occupant(.., PaneClosed, ..)`), absents its runtime, and `do_retire_seat` retires the clone. At base the classifier listed that clone and emitted its end fact, so record and event agreed. At head the graph records the occupancy as ended and the clone retired, but `observe::step` never broadcasts `SessionEnded` for it, so session-end consumers (transcript processing) miss that session. Either gate the classifier filter on the node manager or make the mutation and the classifier agree for every seat, and add a test for a non-node-manager seat whose pane moved to another workspace before its home workspace closed.

## Other observations from the inspected hunks

- `node_manager_warning::warn` loads every seat and teamspace record on every CLI invocation, hooks such as `session-report` included; correct but adds a full tree scan per command.
- `bootstrap::seat_ref` calls `layout::all_clones` once per referenced seat, so a node manager with many reports scans all clones per report.
- Outside a Herdr pane with the daemon down, `who`, `request list`, `ops` and `undo --list` now print "not inside a Herdr pane; this session has no graph seat" (spec §4.4 table); accurate for agents, slightly misleading for an operator terminal.
- `who` now fails with exit 6 when the queried seat's manager chain is broken, instead of answering the identity; consistent with B4 but a broader blast radius than the seat itself.

## Ledger triage

- Parked (hg-k4l.22): `e2e_summary_lands_in_archived_seat_folder` and `e2e_undo_from_bound_pane_adopts_and_keeps_it` fail at base, main and head (evidence/hg-k4l.22/e2e-two-{base,main,head}.log, all with "threads service disconnected: herdr-threads state directory not found"). The fix pass judged them pre-existing; the evidence supports that, but known-failing tests are a must-fix-or-track item, not a silent minor.
- Recurring minor (missing-evidence, ×9 across 9 tasks): process class only; no product impact. The full-suite gap it implies is covered by the deferred sweep.
- hg-k4l.61 known-limit (two-clone node manager, one clone moved while home tab open, not marked moved_out): the move is still surfaced through the moves list warning, and a later home-tab close marks it moved_out; acceptable as a known limit.
- hg-k4l.6 plan-race-gap: apply recomputes the plan and compares its hash, so a seat pointed at the old node manager after planning makes the apply stale rather than leaving it unrewritten; acceptable.
- hg-k4l.17 restore-skip-silent / chain-cycle-risk: same recompute guard plus `admit`'s cycle check on the undo plan; acceptable.
- hg-k4l.60 stale-record-clone in SeatResurrect: the block predates the epic; not verified that `do_resurrect_seat` leaves the teamspace record untouched.
- No BLOCKED-AUTH lines were found in the ledger.

## Verdict
not ready (bounded review only: whole-range review outstanding, full suite unmeasured, workspace-closure end-fact regression, known-failing e2e tests untracked)

## Must fix before landing
- A whole-range final review of 0dbe477..96a4657 is still outstanding: this pass is bounded by the sidecar gaps run.md:122 inv-26c52189 (incomplete claimed collection; incomplete split bundle: missing cluster report or slices).
- No full-suite measurement (`cargo test`, tiers 1-2, plus tier 3 where available) exists for head 96a4657; the sweep is deferred to the caller and the branch must not be treated as tested until it runs.
- src/observe/classify.rs workspace cascade (hg-k4l.61, c14b8a8) drops surviving clones of every seat, but src/observe/mutations.rs CascadeMutation::retire_seat still ends and retires those clones for non-node-manager seats, so their SessionEnded event is never broadcast while the record says PaneClosed; gate the filter on the node manager or align the mutation, with a test.
- Parked hg-k4l.22: e2e_summary_lands_in_archived_seat_folder and e2e_undo_from_bound_pane_adopts_and_keeps_it (tests/e2e_private_herdr.rs) fail at base, main and head; file a tracking bead or get the human's explicit acceptance before landing rather than leaving known-failing tests in the ledger.

## Untested scope
- Provenance gap: run.md:122 inv-26c52189-6b83-43ed-a202-49a87db495d9 incomplete claimed collection.
- Provenance gap: run.md:122 inv-26c52189-6b83-43ed-a202-49a87db495d9 incomplete split bundle (missing cluster report or slices).
- Not inspected: src/cli/daemon_error.rs beyond its first 200 lines (unit tests), src/plan/managers/cycle_guard.rs beyond line 120, src/plan/managers/first_seat.rs beyond line 110, src/plan/managers/edit.rs, src/plan/managers/tests.rs.
- Not inspected: src/model/change.rs, src/model/mod.rs, src/model/tests.rs, src/plan/mod.rs, src/plan/kinds_extra.rs, src/plan/tests.rs, src/store/init.rs, src/store/instruction_source.rs, src/store/tests.rs, src/writer/mod.rs, src/writer/tests.rs, src/daemon/doctor.rs, src/cli/instructions.rs, src/templates/structure.rs, src/templates/tests.rs, src/transcripts/tests.rs.
- Not inspected: src/bootstrap/tests.rs, src/observe/tests.rs, src/reconcile/tests.rs, src/threads/tests.rs, src/undo/tests.rs and every file under tests/ (daemon_composed, daemon_error_commands, daemon_free_reads, e2e_private_herdr, instruction_writers, integration_sweep, manager_seats_sweep, node_manager_warning, seat_exit_codes, support/mod.rs, writer_concurrency).
- Not inspected: evidence/hg-k4l.22/*.log, evidence/hg-k4l.45/*.log, docs/superpowers/specs/INDEX.md and the ~460 files under docs/superpowers/runs/2026-11-05-seat-failure-manager-seats (design, roasts, coverage, findings ledgers, review archives).
- B11 reopening after an observed closure depends on the existing reconcile planner; not exercised in this pass.
- No spawned-binary test for exit 6 or for a daemon still starting (hg-k4l.2), nor for exit 5 on mutating commands cancel/reassign/request ack/complete/undo apply (hg-k4l.3).
- Lenient reparent fallback with no live node manager (hg-k4l.26), retired-subject rejections in plan/managers/edit.rs (hg-k4l.13), TemplateEdit withdrawal two-hop reparent (hg-k4l.54), non-null threads_seat on a reports entry (hg-k4l.19), daemon-backed who manager line and show exit 6 end to end (hg-k4l.16), multi-clone node manager still_bound branch (hg-k4l.58) all lack tests.
- SeatResurrect writes a teamspace record cloned before do_resurrect_seat (hg-k4l.60); not verified that the earlier call leaves that record untouched.

## Deferred OK
- Recurring missing-evidence minors across 9 tasks: process only; the deferred full-suite sweep is the real measurement.
- hg-k4l.61 two-clone node manager move not marked moved_out: the move is still surfaced and a later home-tab close sets the flag.
- hg-k4l.6 plan-race-gap and hg-k4l.17 restore-skip-silent / chain-cycle-risk: apply-time recompute and admit's cycle check cover them.
- hg-k4l.27 TeamspaceRetire preview may list a default_manager rewrite of the teamspace being retired: preview and apply agree.
- hg-k4l.11 node_manager_suffix reads all pointers, degrading to the user form on an unrelated corrupt record: safe fallback.
- hg-k4l.2 NotInHerdr text without the daemon-unavailable prefix: now mapped by daemon_error to the exit-2 line.
- hg-k4l.45 evidence logs committed under evidence/ and the integration_sweep support gate change: repo hygiene only.
- node_manager_warning full tree scan per command and seat_ref all_clones scan per reference: efficiency follow-ups.
- Clippy too_many_arguments on CascadeMutation helpers, duplicated test helpers, weak assertions and string-prefix filters in tests: cleanup follow-ups.
