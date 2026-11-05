# Final review (bounded) — epic hg-66u, threads seat as the clone identity

## Review frame

Bounded captured-source review of `super-auto/threads-seat-identity`, base `0a84c67554aa3fc8e8caf20db4163683ab30af61`, head `573ed7703db91cfe49f119af1465b5b1004f2d96` (both verified with `git rev-parse --verify`). The review-inputs sidecar reported status unavailable with sourceAvailable true, wholeRangeReady false, 318 selected, 0 omitted, and two provenance gaps on the `inv-4aa37d70` archive record (run.md:73). No whole-range package was built or loaded. I inspected 42 of the 318 changed paths in full from captured Git: every changed production source file under `src/`, plus the skills, the design docs, Cargo.lock, the submodule pin, the instruction-writer inventory test and the epic spec. Tests, fakes and run archives were not inspected; they are listed under Untested scope. The caller checkout was clean at head `573ed77` when the review started and was not touched. No full-suite run of this branch exists; that measurement is still owed to the caller.

## Requirement baseline trace

The spec has a `## Requirement baseline` (B1–B11) and its human scope change list is `- none`. At head, open beads include the gates hg-66u.1 (D1), hg-66u.2 (D2), hg-66u.3 (G5) and hg-66u.36, plus the gated work hg-66u.14, .15, .21, .24, .31, .32, .33, .40, .43, .44 and the private-rig smokes hg-66u.17 and .18.

- B1 (1:1 mapping, threads authoritative): `CloneRecord.threads_seat` is write-once through `bookkeeping.threads_seat` (src/threads/assignment.rs), and the resolve-seat want fills it from a verified, token-stamped binding. Delivered in part. The graph's own clone matcher still runs (src/observe/matcher.rs clone passes; planner `LiveIndex` live_ref and rebind adoption), so there are still two sources of truth, under the pre-D1 per-clone precedence. Deleting the matcher (hg-66u.14, hg-66u.15) is gated on D1 and still open.
- B2 (hard dependency on threads): delivered. Seat reads go through the production `LazyMap` wired in `compose.rs`. A threads outage leaves bindings alone (`ThreadsRead::Unavailable`).
- B3 (threads re-matching as good or better, then delegate): SeatObservation overlays threads' answer, and a parity test bead (hg-66u.23) exists. The actual delegation, removing the graph matcher, is not done (hg-66u.14/.15 open).
- B4 (frictionless, unattended restarts): `plan_resume` relaunches a clone after a cold restart only when it had an agent, has a resumable session (`p.resume.is_some()`), has a `threads_seat`, and its committed `seat_fact` reads Unresolved after the incarnation change. Clones without a resumable session (fresh-agent path, hg-66u.21) and unseated clones (hg-66u.33) are not relaunched and still need an operator. Partially delivered, and never demonstrated on a real Herdr (hg-66u.17 open).
- B5 (`herdr update --handoff` unattended): the code path exists. Preserved pane, tab and workspace ids match in any incarnation, and SeatObservation binds from a threads `Resolved` read. hg-66u.25 is a fake-composed reconnect test. The real-rig handoff smoke (hg-66u.17) is open, so this is undemonstrated rather than missing.
- B6 (no Herdr change): delivered. The continuity env reaches the agent as a space-prefixed `export` typed through `send_keys` before `agent.start` (src/herdr/client.rs `export_line`), with values limited to `[A-Za-z0-9_.-]`.
- B7 (undo reopens the tab, resumes the session, unretires the seat): undo now commits a Restore intent and reopens the clone's own pane. Pre-D2 it *succeeds* the final-retired seat with a fresh seat; it never unretires the original. `UnretireSeat` and the finalize wants (hg-66u.31, hg-66u.32) are gated on D2 and open. Not delivered as requested.
- B8 (agent exit keeps seat and memberships): delivered. `membership_seat` keys desired invitations on `threads_seat` with no occupant test, and the release logic fires only on succession or a missing want.
- B9 (continuity key): not delivered. Every start site calls `agent_env(&clone, false)` (src/reconcile/herdr_exec.rs start and replace paths, with the comment "No key reservation exists before D1"). No `ReserveContinuityKey` call exists, and `purge_tombstone(&clone, None)` passes no reservation. The reservation, keyed-launch and KeyConflict production work (hg-66u.43, .44, .40) is open behind gate hg-66u.1.
- B10 (undo fully hands-off, memberships restored by threads): not delivered. The interim succession re-invites memberships on the new seat, and they need one re-acceptance. D2 is open.
- B11 (H2 Option D): delivered. `accept_command` and its formatter are gone, `render_seat` prints thread and constraint only, and skills/seat/SKILL.md step 4 delegates acceptance to herdr-threads.

## Integration seam analysis

- Restore launch ownership has a hole. `start_needed` skips every clone with any `Restore` intent (src/reconcile/planner.rs `start_needed`). `plan_resume` computes `eligible` with `p.resume.is_some()` before either trigger, and `DesiredPane.resume` is `None` when the clone has no ended native session. So a restored clone with no resumable session (for example, one retired before its first agent session ended) gets the reopened pane and the seat succession, and then no launch ever. Its intent never clears, because `reattached_restore` needs an occupant. `clone_status_with_journal` lists only `launched = true` restores as stuck, so status never shows this case. Spec §9 says "resume … when the harness supports it, else fresh", and §13 says the restore gets one launch attempt per epoch.
- Restore flow, end to end (read, not run): undo/resurrection call `write_restore_intent`, which clears `seat_fact`. The plan path creates the pane with its nonce and token. SeatObservation's Retired arm is held off while `threads_done = false`. `succession()` reads the fresh Retired-final fact and the verified pane, issues `SucceedSeat` with a stable op key, and the assignment admits the change only against the matching seat and epoch. `plan_resume` then emits one `StartAgent` per epoch (identity rev fixed at 0). The executor admits `MarkLaunched` before dispatch. `ClearOnReattach` fires on Resolved plus an occupant. The pieces compose as the spec describes, apart from the hole above.
- `ThreadsSeatMap::inspect` (src/threads/mapping.rs) reads seats one at a time and returns the first error for the whole batch. A single seat id that threads refuses (unknown or malformed) makes every step `Unavailable`, which silently switches SeatObservation off for every clone. Only Rejected/Unsupported reach `status.threads.seat_view_error`. I did not verify what v3.0.0 returns for an unknown seat.
- Undo/resurrection write a Restore intent without checking the invariant that at most one non-retired clone holds a seat. `check_free` excludes retired clones, so a retired clone's seat could in principle be held by an active clone when it is undone. Pre-D2 every retired seat is final, so threads will not hand it to another pane; the risk only becomes real with D2.
- Purge before D2: `purge_tombstone` keeps a `Finalize` tombstone for every seated clone. §8 says a clone whose seat is already final is purged at once, and §14.2 limits always-tombstone to after D2. No want removes these tombstones until hg-66u.31 lands. They are inert (filtered from status, planning and undo), so the cost is accumulation only.
- `membership_seat` keeps desired invitations for retired, unpurged clones (§14.11). Pre-D2 their seats are final-retired, so a retired clone with no prior invitation, or an Ordinary invitation polled out of Pending, produces an invite against a retired seat. That is one attempt per rev, not a loop.
- CURRENT-DESIGN.md "Undo", first bullet, still says to adopt the caller's pane, which contradicts the new second bullet. The DESIGN-NOTES "Undo terminal adoption" section is also stale.

## Ledger triage

- The recurring blocker (no C toolchain on the host for merge checks, ×3) is an environment issue, not a product defect.
- The recurring minor `missing-evidence` (×9 across 9 tasks) is a process-class problem: task pass counts are not backed by retained logs. Together with the deferred full-suite sweep, this means no task-level test claim on this branch can be checked from disk.
- Task 16 (hg-66u.29) and Task 26 (hg-66u.26) record two open run failures: `instruction_source_writers_are_inventoried` and `wiring::sweep_action_and_effect_envelopes_round_trip` (timeout). I re-graded these as must-confirm, because a minor that names a failing test was misgraded. At head, tests/instruction_writers.rs lists src/plan/core_kinds.rs and src/threads/assignment.rs as expected writers, which suggests the first failure is fixed. Neither is measured.
- Task 16 also records that the rewritten tests/e2e_private_herdr.rs adoption tests were compile-checked only. That is untested scope.
- Task 15's `unseated-arm-deferred` and Task 16's `deferred-acceptance` (key relink) are D1 baseline scope (B4, B9), and the must-fix items cover them.
- Task 10's `ordinary-invite-succession` is harmless pre-D2, because threads ends a final-retired seat's memberships. Its `unverified-assumption` (invites to retired seats) matches the seam item above. Both are acceptable to defer, but the assumption should be re-checked when D2 lands.
- Task 12's `async-admission-unobserved` and Task 15's `async-admit-window` are acceptable minors. A late writer rejection of Assign leaves the effect Done for that rev and does not retry it, which is consistent with "never retried", but nothing surfaces it.
- Task 26's `follow-up-pane-derived-identity` (ServiceAck in transcripts/delivery.rs, behind the threads-service-ack feature) is a follow-up.
- Task 14's "tab moved between workspaces n/a" parity call is acceptable, since tabs now bind by preserved id.
- The ledger has no parked findings and no BLOCKED-AUTH lines.

## Verdict
not ready (baseline rows B1, B3, B4, B7, B9, B10 not delivered behind open D1/D2 gates, restore-launch gap, bounded coverage, no full-suite run)

## Must fix before landing
- B9 not delivered: every agent start uses `agent_env(&clone, false)` (src/reconcile/herdr_exec.rs), no `ReserveContinuityKey` exists and purge passes no reservation; hg-66u.43/.44/.40 open behind gate hg-66u.1, and the scope-change list is `none`.
- B7 not delivered: undo does not unretire the seat; pre-D2 it succeeds the final-retired seat with a new one (src/threads/effects.rs `succession`, `succeed_seat`); `UnretireSeat` work hg-66u.31/.32 open behind gate hg-66u.2.
- B10 not delivered: the interim succession re-invites memberships on the successor seat and needs one re-acceptance; restoring memberships hands-off depends on D2 (hg-66u.31/.32 open).
- B1/B3 not delivered: the graph's clone re-matching (src/observe/matcher.rs clone passes, planner `LiveIndex` live_ref/rebind adoption, `/seat` rebind proposals, `clone rebind --pane`) still runs beside threads; deletion hg-66u.14/.15 open behind gate hg-66u.1.
- B4 partially delivered: `plan_resume` relaunches after a cold restart only clones with a resumable session and a `threads_seat` (src/reconcile/planner.rs `restart_covered`/`eligible`); the fresh-agent path hg-66u.21 and the unseated arm hg-66u.33 are open, so those clones still need an operator.
- Restore launch gap: a restored clone with `DesiredPane.resume = None` is never launched; `start_needed` skips every Restore intent and `plan_resume` requires `p.resume.is_some()`, so the intent never clears and `stuck_restores` (src/daemon/clone_status.rs, `launched = true` only) never names it; spec §9 "else fresh" and §13 "one launch attempt per epoch" are not met.
- Confirm by a measured run that the recorded run failures `instruction_source_writers_are_inventoried` and `wiring::sweep_action_and_effect_envelopes_round_trip` pass at head; both are open run-failure gaps in the ledger with no owner and no measured base.

## Untested scope
- No full-suite measurement of this branch exists (deferred to the caller: default, test-support and private-herdr feature runs); no claim here rests on a test run.
- Provenance gap: 573ed7703db91cfe49f119af1465b5b1004f2d96:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:73 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete claimed collection.
- Provenance gap: 573ed7703db91cfe49f119af1465b5b1004f2d96:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:73 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete split bundle: missing cluster report or slices.
- 276 changed paths not inspected: all test files (src/*/tests.rs, src/threads/service_tests.rs, tests/daemon_composed.rs, tests/integration_sweep.rs, tests/e2e_private_herdr.rs, tests/herdr_client.rs, tests/herdr_spikes.rs), fakes (src/herdr/fake.rs, src/threads/fake.rs), the src/undo/adopt.rs deletion, evidence/hg-66u.23/observe-tests.log, docs/reviews/2026-10-29-seat-review-H2-identity-design.md, the MVP design edit, and every run-archive path under docs/superpowers/runs/ except the epic spec.
- Real-Herdr qualification is absent: the private-rig smokes hg-66u.17 (live handoff and cold restart, pre-D1) and hg-66u.18 (post-gates) are open, so B4 and B5 are not demonstrated on a real Herdr.
- The rewritten tests/e2e_private_herdr.rs adoption tests were compile-checked only, never run (ledger Task 16).
- Shell-export delivery on real shells (bash, zsh, sh, fish) and the `send_keys`-before-`agent.start` ordering on real Herdr are not verified.
- The `ThreadsSeatMap` TargetUnresolved → NotYet mapping, and v3.0.0's answer to `SeatInspect` for an unknown seat id, are not exercised against a live service.
- Gate bead hg-66u.36 ("Gate: fix loop exited") is open; this review did not inspect it.

## Deferred OK
- `ThreadsSeatMap::inspect` fails the whole batch on the first seat error, so one refused seat id disables SeatObservation for all clones (src/threads/mapping.rs); follow-up candidate: per-seat errors, or skip the bad seat with a status entry.
- Undo/resurrection do not check the one-non-retired-clone-per-seat invariant when writing a Restore intent; harmless pre-D2 because every retired seat is final; recheck with D2 (hg-66u.31/.32).
- Purge pre-D2 keeps an inert `Finalize` tombstone for every seated clone, contrary to §8's "purged at once" for final seats; no want drains them until hg-66u.31.
- Retired, unpurged clones keep invitation wants (§14.11), so pre-D2 a single invite may target a final-retired seat; verify threads' behaviour when D2 lands (ledger Task 10 unverified-assumption).
- Ordinary invitations are not released on succession (ledger Task 10); harmless because threads ends a final-retired seat's memberships.
- An asynchronous Assign rejection is unobserved by `resolve_seat`, and the `MarkLaunched` admit window is asynchronous (ledger Tasks 12, 15); neither retries a different seat.
- Stale docs and comments: CURRENT-DESIGN.md Undo bullet 1 still says to adopt the caller's pane, the DESIGN-NOTES "Undo terminal adoption" section is stale, the undo-adoption comments remain, and `UndoDeps.herdr` is unused (ledger Task 16).
- ServiceAck recipients in src/transcripts/delivery.rs (feature threads-service-ack) still use the pane-derived seat (ledger Task 26).
- Recurring process-class missing-evidence (×9) and the recurring host toolchain merge-check blocker (×3) are process and environment issues, not product defects.
- Remaining task minors (loose SplitPane assertion, test placement, signature drift, formatting noise, TDD order, partial evidence log, the tab-move parity n/a) are acceptable as recorded.
