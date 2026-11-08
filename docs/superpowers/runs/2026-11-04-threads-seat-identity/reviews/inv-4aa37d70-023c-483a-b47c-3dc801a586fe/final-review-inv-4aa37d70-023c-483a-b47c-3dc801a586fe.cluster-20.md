# Final review, cluster 20 (src-undo), epic hg-66u

Invocation: inv-4aa37d70-023c-483a-b47c-3dc801a586fe
Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify`).
Cluster files: src/undo/compensate.rs, src/undo/preview.rs, src/undo/tests.rs (slices 000092 to 000094).
Spec: docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md (sections 8, 13, 14 and the Requirement baseline).

## What the cluster changes

- `compensate.rs::displace` now calls `threads::assignment::clear_restore_on_retire` when an undo retires the clone that the caller's pane was bound to. This matches spec section 14.6 and section 8: every commit that retires a clone clears its Restore intent. Every production site that sets `CloneLifecycle::Retired` at head (observe/mutations.rs:251, plan/core_kinds.rs:423 and :1810, undo/compensate.rs:493) now calls the helper.
- `preview.rs::compute_restore` adds a pre-check. If a clone that the action retired is missing (a seatless purge deletes the record) or is a Finalize/ReleaseKey tombstone (a seated purge keeps one), the whole undo is refused with "clone <id> was purged; its undo window has ended". Without this check a tombstone would have been restored: `purge_tombstone` keeps `lifecycle = Retired` and the original `retired` stamp, so `retired_by_act` matched it and the old code would have brought an inert tombstone back to life. The check therefore enforces section 13's "tombstones are inert: not ... undone", and section 14.1's rule that purge ends the undo window. Both `restore_preview` (plan) and `restore_mutate` (apply) go through `compute_restore`, so the refusal applies at plan time and again at apply time.
- `tests.rs` registers the threads mutations in the fixture and adds `undo_of_a_purged_clones_retirement_is_refused`. The test covers both purge shapes: a seatless clone, whose record is deleted, and a seated clone, which becomes a Finalize tombstone. It also checks that the undo plans before the purge.

## Verification run

- I ran `cargo test --lib undo::` in a review-owned detached worktree at head (`.worktrees/review-c20-4aa37d70`, target reflink-seeded, herdr-threads submodule initialised there): 31 passed, 0 failed, including the new test.
- The caller checkout stayed at 6576a11 with a clean status throughout.

## Requirement baseline trace (undo entry point)

- The production entry point is `herdr-graph undo <act>`: `UndoKind::plan` calls `restore_preview`, and `mutate` calls `restore_mutate` (src/undo/compensate.rs).
- At head, `restore_mutate` makes a retired clone `Active` with `retired = None` and `runtime = absent()`. It still adopts the caller's pane through `adoption`/`displace`. It writes no `seat_intent`.
- `threads::assignment::write_restore_intent` exists, but no production code calls it (git grep at head finds only its definition).
- Spec section 8 requires the undo commit to write `Restore{seat, epoch}`, reopen the tab through the plan path with a create-nonce binding, stop adopting the caller's pane, and drive succession before D2 or unretire after D2. None of that is wired into the undo path. Beads hg-66u.29 (the ungated pre-D2 interim) and hg-66u.32 (gated on D2) are both still open.
- A seated clone undone at head therefore keeps a write-once `threads_seat` that points at a seat threads has final-retired. The clone is either bound to the caller's pane, which has a different threads seat, or to a fresh pane with no seat succession. This is the 1:1 break that section 8 cites as its reason for removing adoption.
- B10 (hands-off membership restore on un-retire) depends on the D2 gate hg-66u.2, which is still open, plus hg-66u.31 and hg-66u.32. Per the epic constraints it must not be faked. It remains undelivered.
- Whether SeatObservation's intent-gated Retired arm (hg-66u.28, also open) composes with the undo commit is for the seam reviewer to trace. It lies outside this cluster.

## Findings

- B7 (requirement baseline) is not delivered on the undo path. `herdr-graph undo` of a seated clone's retirement writes no Restore intent (`write_restore_intent` has no production caller), does not reopen the tab through the plan path, and does not drive seat succession (pre-D2) or unretire (D2). Ungated bead hg-66u.29 and gated bead hg-66u.32 are open. Evidence: src/undo/compensate.rs `restore_mutate` at 6576a11.
- B1 (requirement baseline) is broken by undo at head. `restore_mutate` still adopts the caller's pane (`adoption`/`displace`), so a restored seated clone can be bound to a pane whose threads seat differs from its write-once `threads_seat`. Spec section 8 removes adoption for exactly this reason. Removing it is in hg-66u.29's scope.
- B10 (requirement baseline) is not delivered. Hands-off membership restore on un-retire needs the D2 gate hg-66u.2 plus hg-66u.31 and hg-66u.32, all open. No human scope change row covers it, so the hand-back must list it as open, and it must not be faked.
- The new pre-check in `compute_restore` makes the later standalone branch `let Some(rec) = read_clone(..) else { warn "no longer exists; left alone" }` (src/undo/preview.rs ~line 216) unreachable for clone ids in `act.retired`. This is dead code, not a defect.
- The purge refusal now rejects the whole action, including a tab-close cascade, if any one of its clones was purged. Before this change, a missing clone produced only a warning and the rest was restored. The code comment states the change is intentional, and the spec does not decide between the two behaviours.

## Verdict
not ready (B7 and B10 are undelivered and B1 is broken on the undo path: no Restore intent, caller-pane adoption kept; hg-66u.29, hg-66u.32 and the D2 gate are open)

## Must fix before landing
- B7 not delivered: `graph undo` of a seated clone's retirement writes no Restore intent, does not reopen the tab through the plan path, and drives no succession or unretire (`write_restore_intent` has no production caller at 6576a11; src/undo/compensate.rs `restore_mutate`); land hg-66u.29 and, after the D2 gate, hg-66u.32.
- B1 broken by undo: `restore_mutate` still adopts the caller's pane via `adoption`/`displace`, so a restored seated clone can be bound to a pane whose threads seat is not its write-once `threads_seat` (spec section 8; hg-66u.29 scope).
- B10 not delivered: hands-off membership restore on un-retire depends on the open D2 gate hg-66u.2 plus hg-66u.31 and hg-66u.32; no human scope change row covers it; list it open in the hand-back and do not fake it.

## Untested scope
- No test undoes a closure cascade that retired several clones when only one of them was later purged.
- No test shows that `displace`'s new `clear_restore_on_retire` call drops a Restore intent (the case cannot occur in production today because nothing writes Restore).
- No test pins whether `graph undo` candidates should still list actions whose clones were purged.

## Deferred OK
- Remove the now-unreachable "clone ... no longer exists; left alone" branch in `compute_restore` (src/undo/preview.rs ~line 216).
- Optionally mark purged-clone actions as not undoable in the undo candidates listing.
