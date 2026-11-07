# Review: merge of main into super-auto/threads-seat-identity (2026-11-07)

Scope: merge `4595898` (parents `db31c68` branch, `0908fea` main) and fix `a2bf5a7`. HEAD `7334746` adds only
docs, so the code reviewed is the code at `a2bf5a7`. This is a read-only review. The only file written is this report.

Result: 0 Blocking, 1 Important, 1 Minor.

## What was checked

- (a) Conflict resolutions. Every conflicted file named in `merge-main-2026-11-07.md` was diffed against both
  parents: `src/observe/step.rs`, `src/reconcile/planner.rs`, `src/undo/compensate.rs`, `src/undo/preview.rs`,
  `src/undo/commands.rs`, `src/cli/plan.rs`, `skills/graph/SKILL.md`, `docs/superpowers/specs/INDEX.md` and the test
  files. The merger's account holds:
  - step.rs keeps main's `verify_ends`/`applied_ends`, `cascade_request_with(.., survived)`,
    `move_request_following` with `follow_of`, and the node-manager warning. It keeps the branch's
    `observe_seats`, the baseline unbinding and `append_seat_ops`.
  - planner.rs keeps main's `foreign_to`, `gates_recreation` and the `others` filter, on top of the branch's `LiveIndex::pane`.
  - undo keeps main's `compute_restore(.., Some(&plan.effects))` and pointer restores. Only adoption is gone, as the branch spec requires.
  - SKILL.md and cli/plan.rs carry both sides.
- (d) Deleted or weakened tests. Test names were compared across the merge base `0dbe477`, both parents and the merge:
  - No test is missing from the merge except the 24 that the branch itself deleted on purpose (adoption,
    `clone rebind`, matcher restart and rebind, /seat rebind proposals, and accept-command listing). All of them
    already existed at the merge base.
  - For each test that main added or changed, I diffed its body in the merge against main's version. Every
    difference is either the branch's intended rewrite (adoption tests become "reopen own pane", `CloneRebind` becomes a
    legacy kind, and the seat view is wired into the sweep fixtures) or a helper that follows the test in the file.
  - No assertion of main's was dropped outside the adoption semantics, which the branch removes by design.
- (b) Auto-merged semantics.
  - `classify.rs` `pane_survives` and `follow_of` read the post-SeatObservation `matches`, so for seated clones
    survival and the followed move come from threads' binding. That is correct.
  - `herdr_exec.rs` `reclaim_home` and the `CreateTab` implied check compose with the branch's preserved-id tab matching.
  - `SeatActivate.mutate`, used by main's `node-manager set` to activate a dormant target, goes through the branch's
    `restore_commit`, so seated clones get a Restore intent there.
  - `do_retire_seat` and `retire_clone` both clear the Restore intent on retirement.
- (c) The `a2bf5a7` fix. Both kept-node-manager paths in `CascadeMutation` now write the Restore intent:
  `retire_seat` for workspace and tab cascades, and `retire_clone` for pane cascades and SeatObservation
  retirements. Survived clones are skipped, which is correct because their seat stays Resolved. Main's tab path with a
  survivor (`moved_out`, no cascade) reaches `retire_clone` through SeatObservation once threads reads Retired.
  Finding 1 below is the one ordering this fix does not cover.
- Tests run in `hg-build`: `cargo test --lib node_manager` passed 49 of 49. `--test manager_seats_sweep` passed 17
  and `--test node_manager_warning` passed 5. `daemon_composed` runs 0 tests without the `test-support` feature, so
  that run proves nothing. Leak check afterwards: 0 processes with cwd under `/tmp/hg-*`.

## Findings

### 1. Important: when Herdr reports the closure before threads does, the node manager's Restore is acknowledged without succession

Files:
- `src/observe/mutations.rs:286-289` and `:383-384` (Restore written at cascade time)
- `src/threads/assignment.rs:68` (`write_restore_intent` clears `seat_fact`)
- `src/threads/effects.rs:522-546` (`succession`: `Unresolved | Resolved {..}` gives `Acknowledge`)
- `src/observe/seat_observation.rs:151-170` (Retired arm: only a `threads_done: false` Restore holds)

The fix writes the Restore intent when the cascade commits, and the cascade can come from the Herdr diff. The spec's
§8 restore assumes the seat is already Retired when the Restore is written, which holds for undo, resurrection and
reactivation. For the node manager the two can arrive in either order.

Failure sequence, which I traced through the code but did not reproduce:

1. The node manager's seated pane closes. In step N the Herdr snapshot no longer has the pane, but threads still
   reads the seat `Resolved { pane: <old> }` (or `Unresolved`) because it has not processed the close yet.
   SeatObservation's `Resolved` arm does not bind, because the pane is not live, and it does not unbind. So the
   baseline still holds the clone, the diff reports the closure, and classify emits a pane, tab or workspace cascade.
2. `CascadeMutation` keeps the clone and calls `write_restore_intent`, which sets `seat_fact = None` and
   `Restore{seat-1, threads_done: false}`. In the same step, `fact_needs_write` compares against the pre-cascade
   record (`Resolved{old}` equals `Resolved{old}`), so no fact is rewritten.
3. In step N+1 threads still reads `Resolved{old}` (or `Unresolved`). The fact is written. `succession()` returns
   `Acknowledge`, nothing checks that the Resolved pane is live or is the clone's bound pane, and `ack_restore`
   sets `threads_done: true` with no succession.
4. The planner reopens the pane (B11). The restore launch, guarded by `threads_done: true, launched: None`, may start
   the agent in it.
5. Threads then reads `seat-1` `Retired{Final}`. The Restore is no longer pending, so the Retired arm puts the clone
   in `out.retire`. `retire_clone` runs again for the node manager: it ends the occupant (`PaneClosed`, although
   the reopened pane is alive), sets the runtime absent (dropping the binding to the live reopened pane) and writes a
   new Restore epoch.

Result: one pane close produces two closure cycles. The second records a false `PaneClosed` end for a session that
is still running, and it unbinds a live pane. Whether the second Restore then succeeds depends on the planner finding
that pane again through `live_ref`, so that `verified_pane` can hold. I did not verify that step. If it fails, the
clone stays with a pending Restore and no binding.

The fix's own test (`seated_node_manager_retired_seat_is_kept_and_restored`) covers only the opposite order: threads
reads Retired first and the pane is still in the snapshot.

Suggested direction, not prescriptive: for the node manager, acknowledge a cascade-written Restore only on a
`Resolved` read whose pane is live and bound to the clone. Alternatively, defer the Restore write to the Retired arm:
the Herdr-path cascade ends and absents the clone without an intent, and SeatObservation's Retired read writes the
Restore instead of a retirement.

### 2. Minor: no test drives the node manager's seated reopen to succession

File: `src/observe/tests.rs:2969-3009` (`seated_node_manager_retired_seat_is_kept_and_restored`).

The test checks that the clone stays Active with a pending Restore, and that later steps leave it unchanged while
threads reads Retired. Nothing runs the reconciler and the threads executor through the reopened pane's create, the
`SucceedSeat` want, `threads_done` and the restore launch for a node manager. Main's B11 reopen tests
(`manager_seats_sweep`, `reconcile` node-manager tests) use seatless clones.

Failure scenario: a regression where the node manager's reopened pane never gets a verified binding (for example
`reclaim_home` or `CreateTab` handling skips the clone's binding write-back) would leave every seated node manager
stuck on a pending Restore after its first close, and the current suite would stay green. Main's private-herdr e2e,
`e2e_node_manager_failing_agent_does_not_respawn_loop`, covers the reopen loop with a failing agent and not threads
succession. Neither the merger nor this review ran that tier.
