# Final review inv-3, cluster 27 (src-plan)

Source: base 60e287aa, head 11ac4157. Cluster files: src/plan/seat_adopt.rs, src/plan/tests.rs (slices 000185, 000186).

## Analysis

The cluster is a single string change: the keyless-seat preview warning in the SeatAdopt plan, and its exact-match assertion in src/plan/tests.rs. The old text said an undo gave the pane a fresh threads seat, which is no longer true after the ResurrectSeat work.

The new text claims four things. Each matches the design (design.md lines 90-93, README line 90, src/undo/preview.rs line 512):
- Undo within 7 days brings the seat back.
- Within 24 h everything returns.
- After 24 h, receipts and ordinary invitations owed to the seat are settled.
- After 7 days the pane gets a fresh seat and memberships graph does not own are lost.

Code and test strings are byte-identical, so the test stays green. The text omits that graph re-invites its own channels after 7 days. That is a harmless omission in a preview warning, since README and undo preview state it.

The Requirement baseline rows (resurrect on undo, frictionless restart) are delivered in other clusters. This cluster only carries operator-facing wording. No path leaves this cluster.

Not run: cargo, because a sweep is active in the integration worktree.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Warning text is asserted only as an exact string; its 24 h and 7 d claims are not tied to the threads constants (cargo was not run here).

## Deferred OK

- The warning does not mention that graph re-invites its own channels after 7 days (README and undo preview do).
