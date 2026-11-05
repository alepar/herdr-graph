# Final review, cluster 18 (src-threads), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify`). Caller checkout observed at head 6576a117, clean, before and during the review. Files: src/threads/{assignment,effects,fake,mapping,mod,service_tests,tests}.rs (slices 000082-000088). Spec: docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md, including its Requirement baseline (B1-B11, no human scope changes).

## Scope and method

Static review of the cluster diff against spec sections 2, 3, 4, 6, 8, 13, 14 and the key abstractions (ThreadsSeatAssignment, ThreadsSeatView, SeatStatus, PaneSeatMap, ThreadsSource/ThreadsExecutor). Opened outside the cluster only to trace named risks: production callers of the assignment helpers (`git grep` at head), seat deactivation in src/plan/core_kinds.rs, the herdr-threads v3.0.0 submodule (ContinuityStatus, SeatInspect NotFound, ResolveSeat idempotency), and bead status (`bd list --parent hg-66u`). The test suite was not re-run for this review; the only test log on disk (.superpowers/sdd/hg-66u-plan/test.log, 01:46) predates the later merges and is not evidence for this head.

## What the cluster delivers

- ThreadsSeatAssignment (`bookkeeping.threads_seat`, registered in `register_mutations`): write-once Assign with idempotent same-seat success, rejection of a different seat and of a seat held by another live, non-tombstone clone; Succeed admitted only against a Restore intent whose seat and epoch match and whose threads step is unacknowledged, with an idempotent replay; Acknowledge (stale epoch is a no-op); MarkLaunched once per epoch and only after threads_done; ClearOnReattach for the matching epoch; TombstoneDone deletes only tombstones. Helpers write_restore_intent (nothing for a seatless clone, §14.5), clear_restore_on_retire and purge_tombstone (Finalize for a seated clone, ReleaseKey for a reserved seatless one, none otherwise). This matches the spec's abstraction and §14.6 transitions.
- Resolve-seat want (`threads.resolve_seat`): only for an active clone with no threads_seat, no Restore intent, Present availability and a binding stamped with its own graph token; refusal (TargetUnresolved) is Deferred with a stable epoch-salted operation key, so a retry replays the same resolution and never allocates afresh (§2). The pre-check for a live holder returns NeedsRevision instead of retrying.
- Participation keyed to the seat (§3, §14.11): desired invitations exist while the clone is active, or retired and not a tombstone, with threads_seat known, independent of occupancy; Required release happens on succession (link.seat differs) or when the want is gone; agent exit, /clear, relaunch and clone retirement release nothing; the invite executor invites the recorded seat without re-resolving; `expect_occupant` became `expect_seat`; ThreadsLink.occupant is attribution only. Migration with links already on the backfilled seat causes no re-invite (tested).
- /seat Option D (§4): PendingInvitation drops accept_command and its formatter.
- ThreadsSeatView real implementation on ThreadsSeatMap (per-seat SeatInspect, exhaustive mapping of v3.0.0 ContinuityStatus: Remote and target-less Resolved read Unresolved, Retired reads Final) and FakeThreadsSeatView (unset seat reads Unresolved; injected errors fail the whole read).
- Tests: 30-odd new or flipped cases in src/threads/tests.rs and one wire-level seat-view test in service_tests.rs cover the transitions above, the flipped B8 tests, backfill, unreachable threads and the v3 fixture mapping.

## Requirement baseline trace for this cluster

- B8 (agent exit keeps seat and memberships): delivered. Production path: daemon ThreadsSource::system_wants -> clone_wants keys desired/satisfied/released on membership_seat; occupancy is not consulted. Tests occupancy_end_keeps_required_memberships and clear_or_relaunch_on_the_same_seat_issues_no_release_or_reinvite.
- B11 (/seat stops printing accept commands): the cluster's half is delivered (pending_invitations reports thread and constraint only); render_seat and skills/seat/SKILL.md are other clusters.
- B2 (hard dependency; binding waits when threads is unreachable): the cluster's half is delivered: resolve and invite outcomes on Disconnected/ServiceBusy are Transient, nothing is written (test threads_unreachable_issues_no_release_or_invite, unreachable_threads_writes_no_seat_and_retry_reuses_the_op_id).
- B1, B3, B4, B5 (threads authoritative for location and liveness; frictionless restart and handoff): this cluster supplies ThreadsSeatView::inspect and seat_status, but `git grep` at head finds no production caller of `seat_view.inspect` (Services.seat_view is constructed in src/daemon/compose.rs and never read; SeatObservation hg-66u.28 and the resume driver hg-66u.30 are open). The consumer path leaves this cluster (observe/, reconcile/): it is the seam reviewer's to trace, named here rather than judged.
- B7 (undo reopens, resumes and restores the seat), pre-D2 interim branch: spec §8 and the ThreadsSource key abstraction place the succession want (and the acknowledgement and clear-on-reattach wants) in `ThreadsSource::system_wants`, which is this cluster. At head clone_wants produces only resolve, invite, release and poll wants; SeatTransition::Succeed, Acknowledge, MarkLaunched and ClearOnReattach have no production producer (only Assign is admitted, src/threads/effects.rs:1190), and write_restore_intent has no production caller (the undo/resurrection commit is outside this cluster). Bead hg-66u.29 (succession, pre-D2 interim, ungated) is open. B7 is not delivered.
- B9 (continuity key) and B10 (hands-off undo via un-retire): the reserve-key, unretire and finalize wants that the spec places in ThreadsSource are absent; their beads (hg-66u.43, hg-66u.31, hg-66u.32) wait on gates hg-66u.1 and hg-66u.2, both open. Per the epic's constraint they are handed back as open, not faked; the baseline still counts them as undelivered.

## Findings

- src/threads/effects.rs clone_wants: an Active clone whose graph seat or teamspace is Dormant (seat deactivate, src/plan/core_kinds.rs ~1257, leaves clones Active) gets membership_seat = None, so `desired` is empty and the release loop `continue`s; its Pending/Accepted Required memberships are never released. Base released them (occupant_of returned None for an inactive clone). The code comment and spec §3 say a requirement is released when "the want is gone"; this case contradicts both and has no test. Impact is limited if deactivation also closes the panes (threads then final-retires the seat and ends memberships), but the graph's records stay Accepted.
- src/threads/mapping.rs ThreadsSeatMap::inspect: one per-seat error aborts the whole batch. herdr-threads v3.0.0 answers SeatInspect for an unknown seat with NotFound (store/queries.rs seat_inspect), which api_error maps to Rejected; per the spec callers then "log and change nothing", so a single stale threads_seat (threads store reset, seat row removed) would freeze seat observation for every clone. Latent until SeatObservation consumes the view.
- src/threads/effects.rs membership_seat for retired clones: on v3.0.0 every pane-close retirement is final and threads ends the seat's memberships; a retired-unpurged clone with a Pending ordinary seat-wide invitation is polled, can settle as Released/Retired, and then reads as an unsatisfied Ordinary want, so an Invite against a final-retired seat is issued and fails (Rejected -> Failed in port_outcome). Not observed in a run; inferred from the code (already recorded as an unverified assumption on hg-66u.8).
- The one-live-clone-per-seat invariant is enforced only inside ThreadsSeatMutation (Assign, Succeed); the spec says "a commit that would violate it is rejected by the writer's validator", and no writer-level validator exists, so an undo/resurrection commit reactivating a clone whose seat another live clone now holds is not rejected. The undo commit is outside this cluster; flagged for the seam reviewer.

## Verdict
not ready (B7 pre-D2 succession want absent from ThreadsSource; B9/B10 gated and undelivered)

## Must fix before landing
- B7 not delivered: ThreadsSource::system_wants (src/threads/effects.rs clone_wants) has no succession want, acknowledgement or clear-on-reattach want; SeatTransition::Succeed/Acknowledge/MarkLaunched/ClearOnReattach are admitted only from tests and write_restore_intent has no production caller (git grep at 6576a117); the ungated pre-D2 bead hg-66u.29 is open.
- B9 not delivered: no reserve-key want or ReserveContinuityKey port call in this cluster (spec §2, §5, §14.4); hg-66u.43 waits on gate hg-66u.1 (D1), open; hand back as open, do not fake.
- B10 not delivered: no unretire or finalize want derived from SeatIntent (spec §8, §14.2); hg-66u.31/hg-66u.32 wait on gate hg-66u.2 (D2), open; hand back as open, do not fake.

## Untested scope
- Release of a Required invitation through the `!wanted` branch (thread id of the seat or teamspace channel changes while the seat stays) has no test.
- Active clone of a Dormant seat or teamspace: membership behaviour (no release) is untested.
- ThreadsSeatMap::inspect with several seats where one returns NotFound or another non-transport error is untested.
- ThreadsSeatMap::resolve_seat TargetUnresolved -> NotYet mapping against a real or wire-level peer is untested (fake only).
- Writer rejection of an admitted Assign after resolve_seat returned Done (async admission) is unobserved and untested.
- Seam reviewer's trace: production consumer of ThreadsSeatView (SeatObservation, hg-66u.28) and the resume driver (hg-66u.30) for B1/B3/B4/B5 lie outside this cluster.
- This head's test suite was not re-run for this review; the on-disk test.log predates the final merges.

## Deferred OK
- Dormant-seat Required memberships not released (effects.rs clone_wants `continue` on membership_seat None); low impact while deactivation closes panes and threads finalizes the seat; fix by releasing when the want is gone regardless of membership_seat.
- ThreadsSeatView::inspect fails the whole batch on one per-seat Rejected; should become per-seat results before SeatObservation (hg-66u.28) consumes it.
- Ordinary re-invite against a final-retired seat for retired-unpurged clones on v3.0.0 (inferred, unverified; carries hg-66u.8's deferred assumption).
- No writer-level validator for one live clone per threads_seat outside ThreadsSeatMutation; relevant once undo writes Restore intents (seam).
- resolve_seat reports Done before the writer's Assign admission result is known (recorded hg-66u.7 deferred minor).
- clone_wants lost its doc comment to resolvable_pane (recorded hg-66u.7 doc-drift minor).
