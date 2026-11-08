# Final review: cluster 1 (root-docs), epic hg-66u

Invocation inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f. Source bound with `bind-review-source` (exit 0).
Captured range: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head e4984a492e2d3ce8344b9be83d56a3471049fd60
(both verified with `git rev-parse --verify <sha>^{commit}`). Caller checkout observed at HEAD
e4984a4, working tree clean; no worktree was created (all reads via `git show <head>:<path>`).

## Scope and method

Cluster files: `CURRENT-DESIGN.md` (slice 000001, +1 line) and `GRAPH_STATE.md` (slice 000002, +4/-2).
Both are documentation of the graph's contract and committed state. Each sentence added was traced
against the code at head and against the root spec
(`docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md`,
§2, §8, §13, §14 and "Unchanged behavior") and the beads that own the behavior (hg-66u.35, hg-66u.27,
hg-66u.31).

## Claims traced to head

- CURRENT-DESIGN `clone purge <clone>` (plan/confirm): `ClonePurge` is an `OrgKind` registered in
  `src/plan/core_kinds.rs:49` with verb `("clone","purge")`, routed generically by
  `src/plan/kind.rs:106`; plan emits a warning that the undo window ends. Matches spec §14.1.
- "undo and seat resurrection no longer bring it back": `src/undo/preview.rs:136-143` refuses an undo
  whose action retired a clone that is missing or a tombstone; `do_resurrect_seat`
  (`src/plan/core_kinds.rs:513`) skips tombstones. Tests: `undo_of_a_purged_clones_retirement_is_refused`,
  `seat_resurrect_skips_a_purged_clone`.
- "No automatic retention timer": no timer exists; consistent with §14.1.
- "A clone that held a threads seat leaves an inert tombstone (identity plus `Finalize{seat}`)... a
  seatless clone's record is removed": `purge_tombstone` (`src/threads/assignment.rs:77`) and the
  mutate arm (`core_kinds.rs` ClonePurge::mutate) do exactly this; the reservation argument is `None`
  until hg-66u.43, so at head a seatless clone is always deleted, which is what the doc says.
- "`status` lists retired-unpurged clones under `clones.retired_unpurged`": `register_status`
  (`src/daemon/compose.rs:458-467`) registers provider `clones` over `clone_status`, which emits
  `retired_unpurged` and excludes tombstones.
- GRAPH_STATE write-once and succession: `ThreadsSeatMutation` (`src/threads/assignment.rs`) rejects
  a different seat on `Assign`, admits `Succeed` only against `Restore{seat=from, epoch}` with
  `threads_done=false`, pushes `from` to `threads_seat_history`. Matches the doc and spec §2.
- GRAPH_STATE "at most one non-retired clone holds a given seat": `check_free` enforces it on
  `Assign` and `Succeed`; see the item on enforcement scope below.
- GRAPH_STATE tombstone sentence: tombstone keeps id, seat, name, name history, retirement,
  threads seat and history, and the intent; "only its identity" is a fair summary. ReleaseKey is
  described for a key reservation, which matches §14.3 and the model, though no production path
  produces it before hg-66u.43.
- GRAPH_STATE table row for Clones now lists the threads seat and seat intent, matching
  `CloneRecord` fields `threads_seat`, `threads_seat_history`, `seat_intent` (`src/model/clone.rs`).
- CURRENT-DESIGN line 95 still names threads 2.6.4 / wire protocol 9. That is accurate at head: the
  submodule gitlink is unchanged at cdc8423f (tag v2.6.4) and the v3.0.0 bump (hg-66u.4) has not
  landed in this range. Not a defect of this cluster; hg-66u.4 owns the update.

## Requirement baseline trace

The spec has a `## Requirement baseline` section (B1-B11, no human scope changes). This cluster
holds only documentation; none of B1-B11 is delivered by a doc line, and every production path for
them (seat resolution, SeatObservation, matcher deletion, resume driver, undo/un-retire, continuity
key launch, `/seat` acceptance removal) leaves the cluster and is the seam reviewer's to trace.
Observed evidence for that reviewer: at review time the beads carrying most rows are still open
(hg-66u.4, .6, .7, .8, .14, .15, .28-.33, .43, .44, gates .1/.2/.3), so B1, B3, B4, B5, B7, B9 and
B10 have no complete production path at head on the face of the tree. The docs in this cluster do
not claim any of those rows as delivered; they describe only purge, status and the record fields,
which exist at head.

## Findings

- GRAPH_STATE.md:49 states "At most one non-retired clone holds a given seat" as a state invariant,
  but at head it is enforced only inside `ThreadsSeatMutation` (`check_free` on Assign/Succeed); the
  undo and resurrection commits that turn a retired seated clone active do not check it, whereas
  spec §2 says the writer's validator rejects any violating commit. Not reachable at head (no
  production Assign path until hg-66u.7, and threads does not hand a retired seat to another pane),
  so it is a code-cluster/seam item, not a doc defect.
- CURRENT-DESIGN.md:64 "until threads finalizes the retirement" is slightly imprecise: before D2
  every seat is already final, and the tombstone is removed only by the D2-gated finalize want
  (hg-66u.31) on reading the seat final; until that lands, seated tombstones persist indefinitely.
- CURRENT-DESIGN.md:64 "a seatless clone's record is removed" becomes incomplete once hg-66u.43
  records key reservations (ReleaseKey tombstone, §14.3); GRAPH_STATE already covers that case.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test exercises the `clones` status provider through the composed daemon (`register_status` in src/daemon/compose.rs); only `clone_status` is unit-tested (src/plan/tests.rs `clone_status_lists_retired_unpurged_clones`).
- Tombstone removal ("until threads finalizes"/"until threads acknowledges") has no production producer at head; only the `TombstoneDone` transition is unit-tested, the finalize want is gated on D2 (hg-66u.31).
- ReleaseKey tombstone described in GRAPH_STATE.md is produced only by a unit test of `purge_tombstone`; the purge command passes no reservation until hg-66u.43.

## Deferred OK
- CURRENT-DESIGN.md:64 should name the ReleaseKey tombstone for a seatless clone with a key reservation when hg-66u.43 lands (that bead touches the purge path).
- CURRENT-DESIGN.md:64 wording on tombstone removal could say the finalize want removes it on reading the seat final; refine with hg-66u.31.
- CURRENT-DESIGN.md:95 threads 2.6.4 / wire protocol 9 line is updated by hg-66u.4 (v3.0.0 bump), which has not landed in this range.
- GRAPH_STATE.md:49 one-live-clone-per-seat invariant is enforced only on Assign/Succeed, not on undo/resurrection reactivation; spec §2 asks for writer-validator enforcement; owner is the code cluster/seam reviewer.
