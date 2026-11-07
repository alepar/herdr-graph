# Final review (bounded): epic hg-k4l, branch super-auto/seat-failure-manager-seats

## Scope of this pass

- Source: base 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784, head 7abbe99682f0722e3f8a126370474da5eb0b2290. Both were verified with `git rev-parse --verify`. The caller checkout was clean at head 7abbe99 and was not switched.
- Route: bounded. The review-inputs sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, 503 selected, 0 omitted and 2 retained gaps (inv-26c52189 at run.md:82: incomplete claimed collection; incomplete split bundle). The failed assembly and its sidecar are kept unchanged. No whole-range package was built.
- Inspected from captured Git: 37 changed paths and 115 hunks (listed in bounded-coverage.json). Production code was prioritized: cli::daemon_error, model::manager, every plan::managers module except edit.rs (read in part only), core_kinds, kind.rs, observe classify/mutations/step, reconcile desired, the node-manager warning, bootstrap seat_reply/instructions/show, the seat CLI and skill, templates, threads/who, undo preview/compensate, ipc and registry. Source outside the diff was opened only to trace baseline rows (planner.rs, observe/step.rs, reconcile/bookkeeping.rs, templates/kinds.rs).
- Tests: no full-suite measurement of this head exists, and the caller runs `cargo test` after this invocation. evidence/hg-k4l.22/full-suite-head.log (955 lib tests plus integration targets green, 1 pre-existing ignored packaging test) was run in the hg-k4l.22 task worktree, not at 7abbe99. Do not read the branch as tested.

## Requirement baseline trace (from production entry points at head)

- B1, B14: `who`, `ops`, `request` and `undo` now route every ClientError through `daemon_error::report`. The direct-read functions are deleted, and `status` keeps its documented fallback. Delivered.
- B2: `Unavailable`, `Frame`, `Remote{Unavailable}` and `Remote{VersionMismatch}` map to exit 5 with the retry text. Delivered.
- B3: skills/seat/SKILL.md step 1 has the exit 5/6/other failure branch, and HOOK_PROMPT points to it. Delivered.
- B4: `IpcErrorCode::Integrity` and the `unresolved` helper are in place. The three silent skips in bootstrap/instructions.rs now fail, `seat_reply` maps through `resolve_error` with the node-manager suffix, and the CLI exits 6. Delivered.
- B5: the pointers exist on seat, teamspace and graph.toml. `seat.resolve` returns `manager` and `node_manager` with `threads_seat`. Delivered.
- B6: every retirement route goes through `do_retire_seat`, which calls `reparent`, and previews come from `reparent_effects`. Retirement itself is delivered. Undo of a multi-seat retirement restores the wrong pointer (see Must fix).
- B7: `plan manager set|clear` and `plan node-manager set` are OrgKinds. The observed reparent rides the sanctioned closure retirement, as the spec states. Delivered.
- B8: the `make_plan` guard (`admit`), the classify exemption and the CascadeMutation apply-time recheck are in place. `effective_manager` returns None for the node manager, ManagerEdit refuses a node-manager subject, and NodeManagerSet clears the target's own manager. Delivered.
- B9: `who` (WhoReply.manager_chain) and `show <seat>` print the manager line. Delivered.
- B10/S1: the first_seat helper is wired into SeatCreate, ApplicationApply, TemplateEdit Create additions, SeatResurrect, TeamspaceResurrect and undo restore. The node-manager and office templates are in EXAMPLE_FILES. Delivered. TemplateEdit `AddOp::Resurrect` is not a candidate, but S1 ("created") covers that, and the warning applies.
- B11: closing the pane, tab or workspace of an unmoved node manager is exempt from retirement, and the planner reopens it. Not delivered when the node manager is closed after an observed move (see Must fix).
- B12: `node-manager set` exists, and `cli::main` calls `node_manager_warning::warn` before dispatch for every command except init, setup, daemon and `seat --hook-prompt`. Delivered.
- B13: NodeManagerSet rewrites explicit managers and teamspace defaults that named the old node manager, sets the old node manager's manager to the new one and clears the target's. Delivered.

## Cross-task composition notes

- The must-fix items from the earlier final reviews (inv-17c351e9, inv-8b6d4df1, inv-e03ec4c5, inv-26c52189) are resolved at this head, with these exceptions: first-seat wiring, shipped templates, reports and duties, the skill's self-routing, the existence of undo pointer restore, the CURRENT-DESIGN section, the fixture opt-outs and the hg-k4l.22 sweep are all present. Still open: the chained-reparent undo defect, the move-then-close reopen gap, the committed evidence logs and the AWAITING_HG_K4L_3 exemption.
- Chained reparent trace: TeamspaceRetire.mutate retires seats in `list_seats` order, and `reparent` uses retiring = {seat}. For x->a->b->nm with a and b in the retired teamspace, where a is listed before b, the recorded rows are x:a->b and then x:b->nm. `pointer_restores` judges each row against the current value nm. Row 1 is skipped with a false "changed since the retirement" warning, and row 2 restores x to b, not a.
- Move-then-close trace: classify puts a node manager whose pane left its tab alive into `moved_out`, and observe/mutations.rs:595 persists `moved_out = true`. When that pane is later closed, the node manager branch marks it absent. reconcile/planner.rs:610 then skips `create_tab` for a moved_out seat, and no production path writes `moved_out` back to false (bookkeeping callers never pass Some(false)). The node manager stays Active with no pane, and the `make_plan` guard refuses the plan-side remedies.
- The tier-3 e2e rig calls `support::node_manager_opt_out`, so no real-Herdr test exercises the node manager's closure exemption or reopening.

## Ledger triage

- `Recurring minor` missing-evidence (×6 across hg-k4l.4, .18, .3, .11, .21, .24): this is process evidence hygiene, not a product defect. It goes under Deferred OK.
- Parked hg-k4l.22 (Important): e2e_summary_lands_in_archived_seat_folder and e2e_undo_from_bound_pane_adopts_and_keeps_it fail identically on pre-epic main 0dbe477 (evidence/hg-k4l.22/e2e-two-main.log). They are pre-existing known defects, so they do not gate this branch. They do need a tracking bead, and the undo tier-3 path this epic changed remains unverified.
- hg-k4l.45 evidence-logs-committed: main twice removed this kind of log (be43cf8, be87704). This branch adds 8 logs (4769 lines) under evidence/ that embed home-directory paths. This is regraded to must-fix as a repo-policy breach.
- hg-k4l.2 and hg-k4l.18 stale fallback sites: the code was fixed by hg-k4l.3, but the AWAITING_HG_K4L_3 exemption remains in the daemon_error source scan, so a fallback reintroduced in who/ops/request/undo would not be caught.
- hg-k4l.17 chain-cycle-risk: undo is an OrgKind, so `admit`'s cycle guard sees its seat.manager restore effects. Acceptable.
- hg-k4l.17 restore-skip-silent and hg-k4l.6 plan-race-gap: both look covered by the confirm-time recompute (`node_manager_effect` carries graph_sha, and a changed set of rewrites diffs the plan). No test pins this.
- hg-k4l.24 AddOp::Resurrect not a first-seat candidate: S1 limits B10 to "created", and the warning plus `node-manager set` cover the gap.

## Verdict
not ready (bounded review: chained-reparent undo restores wrong manager (B6), node manager closed after an observed move is never reopened (B11), evidence logs committed; whole-range review and full-suite run outstanding)

## Must fix before landing
- B6 / spec §4.11: undo of a multi-seat retirement restores the wrong manager. src/plan/managers/reparent.rs `reparent` records chained rows per holder (x:a->b then x:b->nm when a is retired before b in TeamspaceRetire/ApplicationRetire/TemplateEdit), and src/undo/preview.rs `pointer_restores` judges each row against the current value in forward order, so x is restored to b with a false "changed since the retirement" warning. Collapse rows per holder and field (first old, last new) or unwind them in reverse, and add a two-hop regression test.
- B11 ("gets reopened if it ever is closed"): when the node manager is closed after an observed move, it is never reopened. classify sets `moved_out` (src/observe/classify.rs:326, persisted at src/observe/mutations.rs:595), a later pane closure only marks it absent, src/reconcile/planner.rs:610 skips create_tab for moved_out seats, and nothing in production clears `moved_out`, so the node manager stays Active with no pane while the make_plan guard blocks plan remedies. Clear `moved_out` (or let the planner ignore it) for the live node manager once it has no live pane, and add an observe plus reconcile test.
- Remove evidence/hg-k4l.22/*.log and evidence/hg-k4l.45/*.log (8 files, 4769 lines, home-directory paths) from the branch, per main's policy in be43cf8 and be87704. Keep copies under the git-ignored .superpowers/ workspace.

## Untested scope
- No full-suite measurement exists at head 7abbe99; the caller runs `cargo test` after this invocation (the hg-k4l.22 log is from a task worktree tree 3d9c859, not this head).
- Provenance gap, unreviewed: 7abbe99682f0722e3f8a126370474da5eb0b2290:docs/superpowers/runs/2026-11-05-seat-failure-manager-seats/run.md:82 inv-26c52189-6b83-43ed-a202-49a87db495d9: incomplete claimed collection.
- Provenance gap, unreviewed: 7abbe99682f0722e3f8a126370474da5eb0b2290:docs/superpowers/runs/2026-11-05-seat-failure-manager-seats/run.md:82 inv-26c52189-6b83-43ed-a202-49a87db495d9: incomplete split bundle: missing cluster report or slices.
- Unreviewed production paths: src/cli/{ops,request,undo,status,plan,show,instructions}.rs hunks (only grepped for daemon_error routing and deleted direct readers), src/templates/kinds.rs (only first-seat candidate selection read), src/plan/managers/edit.rs (read in part only; not in coverage), src/daemon/{client,doctor}.rs, src/model/{change,effective,graph,mod,seat,teamspace}.rs, src/ports/store.rs, src/store/{init,instruction_source}.rs, src/writer/mod.rs.
- Unreviewed test paths: src/*/tests.rs (bootstrap, model, observe, plan, plan/managers, reconcile, store, templates, threads, transcripts, undo, writer) and tests/{daemon_composed,daemon_error_commands,daemon_free_reads,instruction_writers,integration_sweep,manager_seats_sweep,node_manager_warning,seat_exit_codes,writer_concurrency}.rs plus tests/support/mod.rs.
- Unreviewed docs and artifacts: README.md, docs/superpowers/specs/INDEX.md, evidence/**, and about 410 docs/superpowers/runs/** run artifacts other than the design doc.
- No real-Herdr (tier 3) test covers the node manager's closure exemption or reopening, because the e2e rig opts the graph out of the node manager; only fake-Herdr observe tests cover it (src/observe/tests.rs node_manager_tab_close_is_not_retired_and_reopens).
- The undo-from-bound-pane tier-3 path (e2e_undo_from_bound_pane_adopts_and_keeps_it) fails before and after the epic, so undo adoption combined with this epic's first-seat and pointer-restore changes is unverified against a real Herdr.
- No test covers undo after a two-hop chained reparent, or a node manager closed after an observed move.

## Deferred OK
- Recurring missing-evidence minors across six task reports are process hygiene only; no product impact.
- Drop the stale AWAITING_HG_K4L_3 exemption in src/cli/daemon_error.rs `no_command_matches_unavailable_itself` now that hg-k4l.3 has landed.
- File a bead for the two pre-existing tier-3 failures (e2e_summary_lands_in_archived_seat_folder, e2e_undo_from_bound_pane_adopts_and_keeps_it) that reproduce on main 0dbe477.
- TemplateEdit AddOp::Resurrect is not a first-seat candidate (src/templates/kinds.rs TemplateEdit plan); S1's "created" reading and the unassigned warning cover it.
- Plan-time StoreError::Integrity surfaces as exit 1 through PlanError::Store rather than exit 6; the spec scopes exit 6 to daemon replies and `show`.
- `Pointers::load` scans every seat and teamspace on each plan admission, each observer pass and each CLI invocation (warning); efficiency only.
- The hg-k4l.6 plan-race-gap and hg-k4l.17 restore-skip-silent look covered by confirm-time recompute; add a test when convenient.
- The e2e helpers strip the warning and manager lines by string prefix (tests/e2e_private_herdr.rs json_of and show), which is fragile to wording changes.
