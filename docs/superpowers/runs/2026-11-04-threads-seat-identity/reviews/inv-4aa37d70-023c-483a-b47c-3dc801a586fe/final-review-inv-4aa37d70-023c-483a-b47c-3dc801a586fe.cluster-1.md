# Final review: cluster 1 (root), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735
(both verified with `git rev-parse --verify <sha>^{commit}`; review source bound in the ledger, exit 0).
Caller checkout observed clean on `super-auto/threads-seat-identity` at 6576a11 at the start and end of the review.

Cluster inputs: slices 000001-000004 and 000100 (CURRENT-DESIGN.md, Cargo.lock, DESIGN-NOTES.md,
GRAPH_STATE.md, third_party/herdr-threads submodule pointer), digest.md for context, spec
`docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md`.

## Pin bump (spec section 1)

- The submodule moves cdc8423 -> 95e9ebac860cb28766edf3efbbc9fafebb3be7f9, which is exactly the
  `v3.0.0` tag of herdr-threads (`git describe` = v3.0.0, `tag --points-at` = v3.0.0).
- `Cargo.toml` keeps `herdr-threads = { path = "third_party/herdr-threads" }`; `Cargo.lock` changes
  only the version line 2.6.4 -> 3.0.0. The upstream `[dependencies]` block is unchanged between the
  two pins (the upstream Cargo.toml diff adds only test/bin targets), so the dependency list in the
  lock is still correct. `cargo metadata --locked --offline` in the ht-build toolbox succeeds
  (LOCK_OK), so the lock is consistent with the manifest at head.
- `CURRENT-DESIGN.md` now says "threads 3.0.0's canonical node identity and wire protocol 9".
  `PROTOCOL_VERSION` is 9 in `src/protocol/wire.rs` at both pins, so the protocol number is
  accurate and the line the spec asked to update was updated.

## Documentation updates (spec sections 3 and 14)

- `CURRENT-DESIGN.md` "Threads and policy" gains the seat-keyed participation rule (issued once
  the seat is known, survives agent exit, `/clear` and relaunch; released only on succession or a
  vanished want; retirement releases nothing; acceptance is not liveness). This matches spec
  section 3 and section 14.11, and matches `clone_wants` at head (`src/threads/effects.rs:384`),
  which gates desired invitations on `membership_seat` rather than on an occupant.
- `CURRENT-DESIGN.md` undo section documents `clone purge <clone>` with plan/confirm, no
  retention timer, the Finalize tombstone, seatless deletion and `clones.retired_unpurged`.
  Traced at head: `ClonePurge` is registered (`src/plan/core_kinds.rs:49`, verbs `clone purge`),
  refuses non-retired clones, and `purge_tombstone` keeps a `Finalize{seat}` tombstone or deletes
  the record; the `clones` status provider (`src/daemon/compose.rs:509`) returns
  `{"retired_unpurged": [...]}` from `src/daemon/clone_status.rs`.
- `DESIGN-NOTES.md:331` now says cleanup follows seat lifetime and acceptance is not liveness, as
  section 3 requires.
- `GRAPH_STATE.md` adds the clone threads seat, its succession history and seat intent to the
  Clones row, plus the write-once/succession/one-live-clone/tombstone paragraph. It matches the
  spec's ThreadsSeatAssignment and CloneRecord contracts (key abstractions section).

## Requirement baseline trace

- B2 (hard dependency on threads): delivered in this cluster. The graph builds against a pinned
  herdr-threads v3.0.0 path dependency; the lock is consistent.
- B6 (Herdr not modified): holds for this cluster. The full diff touches no Herdr project source and
  no Herdr submodule; the only `src/herdr/` changes are the graph's own Herdr adapter and its fake
  (another cluster's scope).
- B1, B3, B4, B5, B7, B8, B9, B10, B11: the requested behavior is implemented (or still gated) in
  code outside this cluster (threads effects/assignment/observation, plan kinds, bootstrap `/seat`,
  launcher, undo). Those paths leave the root cluster and are the seam reviewer's to trace. For
  context only: `bd list --parent hg-66u` at review time shows the D1 gate (hg-66u.1), D2 gate
  (hg-66u.2) and G5 gate (hg-66u.3) open, along with the gated beads behind them (hg-66u.14, .15,
  .21, .24, .31, .32, .33, .43, .44) and the smoke beads (.17, .18); the seam reviewer should decide
  whether B3, B4, B5, B7, B9 and B10 are delivered at this head or are the gated remainder that
  spec section 10 says the run reports as open.

## Findings

- Membership lifetime is documented in its post-D2 form without a caveat. `CURRENT-DESIGN.md`
  ("memberships end at purge") and `DESIGN-NOTES.md:331` ("last until purge") state it as the
  contract. At this head D2 has not been released, and spec section 14.10 says a graph retire always
  closes the clone's pane. Every pane-close retirement is final before D2 (section 2), so threads
  ends the seat's memberships when the clone is retired, not when it is purged. CURRENT-DESIGN is
  the authorized contract, so this does not block, but readers of the current state will be misled
  until D2 lands.
- `CURRENT-DESIGN.md` says a seatless clone's record is removed at purge; `GRAPH_STATE.md` says a
  seatless clone with a key reservation keeps a `ReleaseKey` tombstone. Both describe real phases:
  production purge passes `reservation = None` (`src/plan/core_kinds.rs:1892,1923`, "hg-66u.13
  passes the recorded key reservation"), so the CURRENT-DESIGN wording is correct only before D1.
  This is a wording gap, not a behavior defect.
- Cross-cluster note for the seam/plan reviewer, not a defect in this cluster: spec section 8 says a
  clone whose seat is already final ("every seat before D2") is purged at once with no intent, and
  section 14.2 limits "always writes a tombstone" to after D2. At this head `purge_tombstone` writes
  `Finalize{seat}` for every seated clone. No production code emits `SeatTransition::TombstoneDone`
  (only tests do; the finalize want is hg-66u.31, gated on D2), so before D2 every purged seated
  clone stays as a tombstone indefinitely. CURRENT-DESIGN documents the implemented behavior
  ("until threads finalizes the retirement"), so the doc and the code agree with each other but not
  with spec section 8's pre-D2 sentence.
- The "Threads 2 node identity and reviewed recovery" heading still reads "updated 2026-10-18"
  although its body now names threads 3.0.0. This is cosmetic.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test exercises the claim in CURRENT-DESIGN.md that `status` shows retired-unpurged clones at `clones.retired_unpurged` end to end. `clone_status_lists_retired_unpurged_clones` (src/plan/tests.rs:3599) calls `clone_status` directly, not through the daemon's `clones` status provider.
- The v3.0.0 pin compatibility was checked here only for lock consistency (`cargo metadata --locked`). Build, unit tests and `tests/threads_real.rs` against v3.0.0 belong to the adapter cluster and were not re-run in this review.

## Deferred OK
- CURRENT-DESIGN.md and DESIGN-NOTES.md describe the post-D2 membership lifetime ("end at purge") with no pre-D2 caveat; add one, or revisit when the D2 gate (hg-66u.2) closes.
- CURRENT-DESIGN.md omits the ReleaseKey tombstone for a seatless clone with a key reservation; correct before D1, update with hg-66u.43/hg-66u.13.
- Seam/plan cluster to confirm: before D2, purge writes a Finalize tombstone for every seated clone, against spec section 8 ("purged at once, with no intent"), and nothing in production clears it until hg-66u.31.
- The "updated 2026-10-18" date on the CURRENT-DESIGN.md threads identity heading is stale after the 3.0.0 edit.
