# Final seam review (bounded): epic hg-k4l, branch super-auto/seat-failure-manager-seats

Invocation: inv-37061003-e563-4f35-a1c7-7329d5b760fb

## Source and method

- Captured range: base 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784, head 8397ca11c2db4fd5fe117fe9eedb7073d921bde6, both verified with `git rev-parse --verify <sha>^{commit}`. `dispatch-text.mjs bind-review-source` exited 0 and echoed the expected invocation, base and head before anything was written.
- Caller checkout observed at HEAD 8397ca1, `git status --short` empty. Not switched, restored or edited; no worktree was created and nothing was built or run. Source reads used `git diff`, `git show 8397ca1:<path>` and `git grep 8397ca1`.
- Selection sidecar review-inputs.json: status unavailable, sourceAvailable true, wholeRangeReady false, 535 changed paths, 0 omitted, 2 gaps (run.md:110 inv-26c52189: incomplete claimed collection; incomplete split bundle, missing cluster report or slices). Bounded route taken; the failed assembly and the sidecar are preserved.
- Scope chosen: the product delta since the last whole-epic seam review (inv-26c52189 at 90e9118) and the fix-loop delta since inv-f2700d23 (7abbe99..8397ca1, beads hg-k4l.54 to hg-k4l.58), traced against each prior Must-fix item, plus the B10 template and the manager-duty producers. Coverage JSON lists exactly the 32 hunks read.

## Prior items traced at head

- B6 undo of chained reparents (inv-f2700d23 #1): addressed. `reparent` now takes the mutation's full `retiring` set (src/plan/managers/reparent.rs), filled by TeamspaceRetire, ApplicationRetire, TemplateEdit and CascadeMutation, so each live holder gets one row with its original `old`; `pointer_restores` also collapses rows per (holder, field) as a defence. Two-hop tests exist for teamspace retire, application retire, undo and the observed workspace close.
- B11 move-then-close (inv-f2700d23 #2): addressed. Classification no longer special-cases the node manager; CascadeMutation decides keep-or-retire against `live_node_manager` at apply time, and `node_manager_closed` clears `moved_out` once no live clone is bound, so the planner reopens it. Tests `node_manager_closed_after_move_is_reopened` and `node_manager_moved_into_closed_tab_is_reopened` assert a CreateTab.
- B10 first seat: `first_seat::assignment` is now called from TeamspaceResurrect, SeatCreate, SeatResurrect, TemplateEdit, ApplicationApply and undo restore. The shipped `templates/node-manager` seat template and `templates/office` team template exist and are in `EXAMPLE_FILES`.
- Manager duties (B5 second half): `seat_reply` returns `reports`, `is_node_manager` and `node_manager_duties` (the node-manager AGENTS.md); skills/seat/SKILL.md step 3 tells managers to listen, detect and unblock and the node manager to follow its duties, and the duty text says it has no manager and goes to the user.
- hg-k4l.55 cycle guard: the new default manager is now also a walk subject in both `refuse_cyclic_chains` and ManagerEdit validation.
- hg-k4l.56: TeamspaceRetire and the retired branch of `CascadeMutation::retire_teamspace` re-read the teamspace record after the seat loop. The kept branch was not changed; see below.
- Evidence logs (inv-26c52189, inv-f2700d23): not addressed. evidence/hg-k4l.45/{final,integ,lib-after,lib-before}.log and evidence/hg-k4l.22/{e2e-two-base,e2e-two-head,e2e-two-main,full-suite-head}.log are still tracked at head.

## Seam analysis of the fix loop

- hg-k4l.56 and hg-k4l.57 compose wrongly in `CascadeMutation::retire_teamspace` (src/observe/mutations.rs:227-246 at head). hg-k4l.57 moved the node-manager keep decision into this function; its keep branch still writes `ts.rec.clone()`, read before the `retire_seat` loop. hg-k4l.56's re-read covers only the retire branch below it. If the node manager's teamspace has a default_manager that is another of its seats and its workspace is closed, `do_retire_seat` reparents that default (s -> node manager) and records the row, then the keep branch overwrites the teamspace record with the stale default s, now a retired seat. Store and action record disagree, the teamspace's default points at a retired seat, and an undo then warns "changed since the retirement" and leaves it. `observed_workspace_close_keeps_own_default_rewrite` covers only a teamspace without the node manager.
- `node_manager_closed` clears `moved_out` only when no non-retired clone is bound. A workspace closure applies `retire_seat` to the node manager, which first marks every live clone absent, including a moved clone that may still be alive in another workspace; the planner would then open a second pane. This matches pre-existing behaviour for workspace cascades of other seats, so it is listed under deferred work, not as a blocker.
- Skill step 3 still says "when it is null, go to the node_manager", which for the node manager names itself. The new `node_manager_duties` text ("You have no manager; when something is beyond you, tell the user") covers it, so this is cosmetic.

## Ledger triage

- Parked (hg-k4l.22): e2e_undo_from_bound_pane_adopts_and_keeps_it and e2e_summary_lands_in_archived_seat_folder fail the same way on pre-epic main 0dbe477. Not caused by this epic, but they are known failing tier-3 tests and need their own bead.
- Recurring minor ×7 [missing-evidence]: process-class; reports carry test results in prose only. No product impact. It does mean no structured test evidence exists for those tasks.
- Fix-loop minors: no two-hop TemplateEdit withdrawal test (hg-k4l.54); src/undo/tests.rs touched outside the file list (hg-k4l.56, justified); no multi-clone node-manager test of the `still_bound` branch (hg-k4l.58). Deferred OK, but the first and third belong in a follow-up test bead.
- Roast escalation still parked for the human: node-manager reopen has no backoff or attempt cap (src/observe/classify.rs), so a harness that exits at once could loop close -> CreateTab. Unverified externally.
- Parked escalations C29 (B7, observed cascades reparent outside plans) and C34 (B12 warning narrowing) still need the human's decision; they are not resolved by this review.
- Metrics report check-fails 3 (fixed 1) and merge-failed 2; no full-suite measurement of 8397ca1 exists. The caller runs it after this invocation.

## Verdict
not ready (bounded review only; node-manager teamspace keep branch reverts a reparented default_manager; evidence logs still committed; no full-suite run at head)

## Must fix before landing
- src/observe/mutations.rs `CascadeMutation::retire_teamspace` keep branch (hg-k4l.56 x hg-k4l.57 seam): it writes the pre-loop `ts.rec.clone()` back, reverting a default_manager that `do_retire_seat` just reparented, so the node manager's teamspace keeps a default pointing at a retired seat while the ClosureCascade record lists the rewrite; re-read the record after the loop as the retire branch does, and add a test for a workspace close of the node manager's teamspace whose default_manager is another of its seats.
- Remove evidence/hg-k4l.45/*.log (4 files) and evidence/hg-k4l.22/*.log (4 files) from the branch, as flagged by inv-26c52189 and inv-f2700d23 and per main's policy in be43cf8 and be87704; keep copies under the git-ignored .superpowers/ workspace.
- Whole-range review outstanding: this bounded review covers 32 hunks in 13 of 535 changed paths; the remaining paths and the two selection gaps (run.md:110 inv-26c52189 incomplete claimed collection and incomplete split bundle) need a whole-range review before landing.

## Untested scope
- whole-epic review incomplete; bounded inspected hunks only; all remaining hunks and provenance gaps remain unreviewed
- Provenance gap: run.md:110 inv-26c52189-6b83-43ed-a202-49a87db495d9 incomplete claimed collection.
- Provenance gap: run.md:110 inv-26c52189-6b83-43ed-a202-49a87db495d9 incomplete split bundle (missing cluster report or slices).
- No full-suite (`cargo test`) measurement exists at 8397ca1; the caller runs it after this invocation, and this review ran no tests.
- Tier 3 (`--features private-herdr`) at head is unmeasured; two tier-3 tests are known to fail on main and on this branch.
- No test of a workspace close of the node manager's own teamspace with a reparented default_manager.
- No two-hop TemplateEdit withdrawal reparent test; no multi-clone node-manager test of the `still_bound` branch.
- Node-manager reopen loop under a crash-looping harness is unmeasured (parked roast escalation).
- Workspace close while the node manager's clone has moved into another workspace (a live pane marked absent, so a second pane may open) is untested.

## Deferred OK
- Recurring [missing-evidence] minor (x7, hg-k4l.4/.18/.3/.11/.21/.24/.55): process-class reporting gap, no product effect.
- hg-k4l.56 touching src/undo/tests.rs outside its file list: justified as the acceptance test.
- Skill step 3 "when it is null, go to the node_manager" self-routes for the node manager; the node_manager_duties text overrides it.
- Workspace cascades mark a moved-but-alive clone absent (pre-existing for every seat); for the node manager this may open a second pane; follow-up bead.
- Pre-existing tier-3 failures e2e_undo_from_bound_pane_adopts_and_keeps_it and e2e_summary_lands_in_archived_seat_folder (fail on main 0dbe477); file a separate bead.
