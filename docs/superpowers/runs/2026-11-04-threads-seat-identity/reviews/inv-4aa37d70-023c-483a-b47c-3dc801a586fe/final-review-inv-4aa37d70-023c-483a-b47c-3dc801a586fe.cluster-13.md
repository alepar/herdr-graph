# Final review: cluster 13 (src-observe), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify`). The source binding was recorded in progress.md through dispatch-text.mjs bind-review-source (exit 0). The caller checkout stayed clean at 6576a11 for the whole review.

Files: src/observe/matcher.rs, src/observe/mutations.rs, src/observe/step.rs, src/observe/tests.rs (slices 000069 to 000072).

## What the cluster changes

- matcher.rs (hg-66u.11, spec §7): seat tabs and teamspace workspaces now link by equality on the bound Herdr id in any incarnation, not only the bound one. A bound id that is missing from the snapshot no longer falls through to the containment vote, so the object goes unmatched. The vote now runs only for objects that have no bound id (first bind). For workspaces the token pass still runs first. This matches §7 ("token first where present; an id that no longer exists in the new incarnation -> Unknown (no vote)") and the Unchanged-behavior line "Tabs and workspaces match by token, then preserved Herdr id".
- mutations.rs: the observed closure cascade now calls `threads::assignment::clear_restore_on_retire` when it retires a clone. That puts the observe retire path alongside the plan (core_kinds.rs:424, :1811) and undo (compensate.rs:494) retire paths, as §13 and §14.6 require ("the retire commit clears the intent").
- step.rs (hg-66u.6): RuntimeLoop takes an `Arc<dyn ThreadsSeatView>` and stores it. The field is `#[allow(dead_code)]`, with the doc comment "unused until SeatObservation (hg-66u.28)".
- tests.rs: three restart tests cover the preserved-id rebind, a missing tab id, and a missing workspace id with no token. Both constructors pass the fake seat view.

## Verification run

- `cargo test --lib -- observe::` ran at head in a review-owned detached worktree (toolbox ht-build, herdr-threads submodule at 95e9ebac = v3.0.0): 42 passed, 0 failed. The worktree and its target have been removed.
- I traced the matcher logic by hand. A bound id claimed by another seat in the same step is not linked, and the object is marked Unknown. A bound record with `tab_id`/`workspace_id = None` still goes to containment. The unmatched sweep is unchanged, and the tests show that an unmatched object in an old incarnation produces no ClosureCascade.
- Production use of `seat_view` at head: compose.rs builds a real LazyMap view and passes it to RuntimeLoop. The step never reads it, and nothing else under src/observe reads SeatStatus.

## Requirement baseline trace (observation side)

- B1 and B3: the spec (§6, Key abstractions SeatObservation) delivers "location and liveness from threads" through the observation step: it polls ThreadsSeatView each step, maps Resolved/Unresolved/Retired/KeyConflict to binding, availability and retirement, and applies pre-D1 precedence over the old matcher. At head the step carries the port but never calls it. Clone binding and availability still come only from the old matcher. The owning bead hg-66u.28 is open. Its blockers (hg-66u.6, .25, .27, .52) are all closed, and it waits on no gate, because §6 says the reader "land[s] before D1 and run[s] alongside the old matcher". hg-66u.10 (the earlier SeatObservation bead) is closed with no close reason, and its work moved to hg-66u.28. The design doc has no human scope change row ("- none"). Within this cluster, therefore, B1 and B3 are not delivered.
- B4 and B5 (frictionless restart and handoff): for tabs and workspaces the matcher change delivers them. With ids preserved, seats and teamspaces rebind in the new incarnation without votes and without creates (restart_with_preserved_ids_rebinds_tab_and_workspace_without_votes). For clones, the same test asserts that the clone stays Unknown after a token-less restart. Recovering it from threads depends on the SeatObservation work above. The resume-driver launch (hg-66u.30, open) leaves this cluster and belongs to the seat reviewer.
- B6 (Herdr not modified): respected. The change only reads ids.
- B7 and B10: the observe retire path now clears a Restore intent, consistent with §13. The un-retire flow itself is outside this cluster.

## Risk analysis: Herdr id reuse across incarnations

The old incarnation guard was the only thing stopping a stale bound id from matching an unrelated object after a cold restart. docs/herdr-spikes.md (Spike 3) records that Herdr ids are positional (`w1`, `w1:t1`, `w1:p1`) and "may coincide across a restart without meaning anything, which is why diffs run only within one incarnation". Spec §7 deliberately relies on Herdr preserving ids across restore and handoff. In that case, equality is correct. Two cases are different. If Herdr starts without restoring the layout, or a workspace or tab was closed during downtime and Herdr later reissued its id, the bound id now names an unrelated tab or workspace. The seat or teamspace would be marked Present against the wrong object. The old code could not do this across incarnations, because the containment vote needed matched clone panes. The spike shows workspace and pane ids reused across a restart, but it does not record whether tab ids are reused. The fake allocates every id from a monotonic counter, so no test can produce this collision. The implementation follows the spec, so this is a spec-level risk rather than a defect in the cluster. It is recorded below as a follow-up and as untested scope.

## Concerns

- Dropping the incarnation guard on bound tab and workspace ids allows a stale positional id to match an unrelated object after a cold restart without layout restore, or after an id is reissued (docs/herdr-spikes.md Spike 3). It is spec-conformant (§7), but it weakens the old guard's protection in the edge case where the spec's premise fails.
- `seat_view` sits in RuntimeLoop as dead code under `#[allow(dead_code)]`. That is fine as staging, but it shows that the observation-side requirement is still unbuilt at the reviewed head.

## Verdict
not ready (B1/B3 not delivered: observe step holds ThreadsSeatView but never reads it; SeatObservation hg-66u.28 open and ungated)

## Must fix before landing
- B1 (threads seat authoritative for clone location and liveness) is not delivered at head: src/observe/step.rs:118-120 stores `seat_view` as `#[allow(dead_code)]` "unused until SeatObservation (hg-66u.28)", no observation code reads SeatStatus, and hg-66u.28 is open with every blocker closed and no gate (spec §6: the reader lands before D1 and runs alongside the matcher); land SeatObservation so the step binds and marks clones from threads under the per-clone pre-D1 precedence.
- B3 (re-matching delegated to threads, as-good-or-better) is not delivered at head for the same reason: clone re-matching after restart or handoff still comes only from the old matcher, and observe/tests.rs restart_with_preserved_ids_rebinds_tab_and_workspace_without_votes asserts the clone stays Unknown after a token-less restart; this is fixed by the same SeatObservation landing (hg-66u.28), not by restoring a matcher fallback.

## Untested scope
- Stale or reissued positional Herdr tab and workspace ids across a cold restart: FakeHerdr allocates ids from a monotonic counter (src/herdr/fake.rs tab_id/ws_id), so no test shows a bound id matching an unrelated tab or workspace in a new incarnation.
- Tab id preservation across a real Herdr cold restart is unverified: docs/herdr-spikes.md Spike 3 records reuse of workspace and pane ids only, and the private-rig smoke hg-66u.17 is open.
- The observed closure cascade clearing a pending (threads_done = false) Restore intent: src/threads/tests.rs pane_close_after_threads_done_retires_and_clears_the_restore_intent covers only the acknowledged case.

## Deferred OK
- Decide whether cross-incarnation bound-id equality for tabs and workspaces needs a corroborating signal, such as a label nonce or name match per docs/herdr-spikes.md Spike 3, before trusting an id whose object may have been recreated; the spec (§7) chose bare equality, so this is a spec follow-up, not a cluster defect.
