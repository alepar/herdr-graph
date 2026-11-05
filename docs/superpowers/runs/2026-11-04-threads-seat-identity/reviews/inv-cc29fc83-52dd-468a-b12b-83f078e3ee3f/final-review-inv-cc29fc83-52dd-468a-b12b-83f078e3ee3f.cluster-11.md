# Final review: cluster 11 (src-observe), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head e4984a492e2d3ce8344b9be83d56a3471049fd60 (both verified with `git rev-parse --verify`). Caller checkout observed at e4984a4, clean, before and after the review.

Files: src/observe/matcher.rs, src/observe/mutations.rs, src/observe/tests.rs (slices 000044-000046).

## Scope and spec mapping

- matcher.rs implements spec §7 (tabs and workspaces across incarnations): the cross-incarnation majority vote is replaced by preserved-id equality. A seat with a bound `tab_id` links by equality in any incarnation; a vanished id links nothing and stays bound, so the seat lands in `unmatched` and becomes Unknown (no vote, no cascade). Workspaces: token first, then bound `workspace_id` by equality in any incarnation, same rule. Containment voting survives only for objects with no bound id (first bind).
- mutations.rs implements spec §13 / §14.6 "the retire commit clears it on re-retirement": the observed pane-close cascade (`CascadeMutation::retire_clone`) now calls `ThreadsSeatAssignment`'s `clear_restore_on_retire`, so the intent is cleared through the owning abstraction rather than written directly. The other retire commits (plan/core_kinds.rs:424, 1811; undo/compensate.rs:494) call the same helper; those are other clusters.
- tests.rs adds three restart cases: preserved ids rebind tab and workspace with no clone match (the old vote had nothing to count); renumbered tab ids with tokens kept leave the seat Unknown with its old binding, no ClosureCascade, while the workspace still binds by token; renumbered ids without tokens leave the teamspace Unknown.

## Requirement baseline trace (cluster share)

- B4 / B5 (frictionless restart and live handoff): for tabs and workspaces, the production entry point is `match_snapshot`, called from the observer each step. With Herdr's preserved ids (governing review §1, citing Herdr `persist/restore.rs:325-355`; fake `restart` models it, `renumber_tabs_and_workspaces` models the counterfactual), seats and teamspaces rebind to the new incarnation without operator action. Delivered for this cluster's share. Clone rebinding across restart goes through threads (§6) and the resume driver (§9), which sit in other clusters; that is the seam reviewer's to trace.
- B6 (no Herdr change): the cluster relies only on existing Herdr id preservation. Met.
- B1 / B3: §7 says tabs and workspaces stay graph-matched; the clone passes remain until D1 (the gate in §6/§10). Consistent with the spec; deleting the clone passes is gated and outside this cluster.
- B7 / B10 (undo): the cascade clearing a Restore intent on re-retirement is the cluster's only part of the intent lifecycle. Covered by `threads::tests::pane_close_after_threads_done_retires_and_clears_the_restore_intent`, which drives the real `cascade_request` path.

## Verification run

- Review-owned detached worktree at e4984a4 (target reflink-seeded, submodule copied), in toolbox ht-build: `cargo test --lib -- observe:: threads::tests::pane_close` gave 43 passed, 0 failed. Worktree removed afterwards.
- Discrimination check by reading: under the base code, the second new test would rebind the seat to the renumbered tab through the token-matched clone's vote (Present), and the first would get no vote at all (Unknown). Both assertions therefore separate the new behaviour from the old.

## Observations

- Same-incarnation behaviour change: the old first arm matched any bound record in the current incarnation, including one with `tab_id = None`, and did not vote. The new guard `Some(b) if b.tab_id.is_some()` sends a bound record with no tab id to containment voting in every incarnation. Seat bindings always carry a tab id in practice (planner and exec write it), so this is theoretical; it matches the updated module doc ("a seat with no bound tab id takes the tab holding its matched panes").
- The guard and inner `if let` repeat each other (`Some(b) if b.tab_id.is_some()` then `if let Some(tid) = b.tab_id.as_ref()`); a `Some(Binding { tab_id: Some(tid), .. })` pattern would say it once. Cosmetic.
- A seat or teamspace whose preserved id vanished during downtime stays Unknown for good: the stale binding is kept, and nothing rebinds it or cascades it. This matches §7 ("an id that no longer exists in the new incarnation -> Unknown (no vote)") and the existing close-while-daemon-down behaviour. Recovery is an operator action.
- `clear_restore_on_retire` clears a Restore intent on any cascade retire, whether or not `threads_done` is set. §13 states the acknowledged case and §8 the refusal case ("retiring the clone again clears it"). Undo restores `runtime = absent()`, so no cascade can fire against a restoring clone before its new pane is bound, and unconditional clearing is consistent with both.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test covers a bound seat or teamspace record with `tab_id`/`workspace_id = None` in the current incarnation, which now goes to containment voting where the base code did not vote (src/observe/matcher.rs:212, 261).
- No real-Herdr rig test in this cluster asserts tab and workspace id preservation through matching across a cold restart or live handoff; the evidence is the fake plus the governing review's Herdr source citation and the fake-level `src/herdr/tests.rs` assertion (spec §11 qualification belongs to other beads).
- No test covers the observed pane-close cascade clearing a Restore intent whose threads step is still pending (`threads_done = false`); only the acknowledged case is tested.

## Deferred OK
- Collapse the duplicated guard and `if let` in the tab and workspace bound-id arms into one pattern match (src/observe/matcher.rs:212-218, 261-267); cosmetic.
- A seat or teamspace whose preserved id vanished during downtime stays Unknown indefinitely, with no automatic rebind; this is spec-conformant (§7) and recovery is an operator action.
