# Final review, cluster 3 (runs-other), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify`; caller checkout HEAD equals head, status clean at review start). Review source bound via `dispatch-text.mjs bind-review-source` (exit 0).

## Scope

One file, one slice (`slices/000006.diff`): `docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-design.md`, section 7.1 "Channels", two bullets rewritten. Spec section 3 explicitly asks for this update: "Update spec §7.1 / `CURRENT-DESIGN.md` ... to say cleanup follows seat lifetime and acceptance is not liveness."

## Analysis against the spec

- Invitation bullet: now keyed on the recorded `threads_seat`, no occupant needed, addressed to that seat, and states "acceptance is not liveness". Matches spec section 3 and B8/B11 intent. Checked against head code: `clone_wants` in `src/threads/effects.rs:384` builds desired invitations whenever `membership_seat(c)` is `Some`, with no occupant condition.
- Cleanup bullet: cleanup follows seat lifetime (spec section 3, 14.10, 14.11); agent exit, `/clear`, relaunch on the same pane issue no release and no re-invite (B8); release only on succession or want disappearance; retirement releases nothing; memberships end at purge. Checked against head: `membership_seat` (`src/threads/effects.rs:102`) is live for Active clones and for Retired non-tombstone clones, matching 14.11; the release loop (`effects.rs:459-485`) releases a Required Pending/Accepted invitation only when `link.seat != threads_seat` or the want is gone, and skips everything when no live seat is recorded. `clone purge` exists (`src/plan/core_kinds.rs:1838`) and writes a Finalize/ReleaseKey tombstone (`src/threads/assignment.rs:75`). The text and the head behavior agree.
- The rewrite is consistent with the parallel update in `CURRENT-DESIGN.md:55` (outside this cluster, read only to confirm consistency).
- Requirement baseline rows B8 and B11 touch this file only as documentation; their production delivery paths (effects planner, `/seat` rendering) live in other clusters and are the seam reviewer's to trace. This cluster does not claim or block delivery of any row.

## Concerns

- "or when the want disappears (opt-out, channel removed)" is applied to Required invitations, but opt-outs only filter Ordinary seat-wide threads (`effects.rs:417-421`); for Required invitations only channel removal applies. The wording is copied from spec section 3, so it is faithful to the spec, merely imprecise.
- "threads retires the seat on the verified closure ... Memberships end at purge" describes post-D2 behavior. Gates hg-66u.1/.2/.3 are open at head; on threads v3.0.0 a pane-closure retirement is final and threads ends the memberships at closure, not at purge. The paragraph does not say it is gated on D2.
- Other sentences in the same file are now stale and were not touched: line 160 says a gap-time agent exit makes "§7.1 cleanup ... still fire" (§7.1 cleanup no longer reacts to occupancy end); line 233 says `/seat` prints "pending invitations with the exact `herdr-threads accept-required` commands", which spec section 4 (B11) removed and head code no longer emits (`src/bootstrap/tests.rs:712` asserts `accept_command` is absent); line 155 still describes clone re-matching order, which spec section 6 deletes after D1. The spec only mandated the §7.1 update, so these are documentation drift, not a spec failure.
- The cross-reference "spec threads-seat-identity §3, §14.10-11" names no path; a reader has to find the run directory.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- none

## Deferred OK
- Tighten §7.1 wording so "opt-out" applies only to Ordinary invitations and note that "memberships end at purge" and reversible retirement hold once the D2 gate (hg-66u.2) releases.
- Update the stale sentences in the MVP design (line 160 §7.1 cleanup on occupancy end, line 233 `/seat` accept-required commands, line 155 clone re-matching order) or mark them superseded by the threads-seat-identity spec.
- Give the "spec threads-seat-identity" cross-reference a repo path.
