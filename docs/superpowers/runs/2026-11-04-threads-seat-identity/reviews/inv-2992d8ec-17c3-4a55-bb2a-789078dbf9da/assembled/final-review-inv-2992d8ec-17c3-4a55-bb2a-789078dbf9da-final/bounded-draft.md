# Final whole-epic review (bounded): hg-66u, threads seat as the clone identity

Range 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784..7b8cacc0fd7d945cec497bc03ef9901834432e75 on super-auto/threads-seat-identity (155 commits, 399 changed paths). Both commits verified with `git rev-parse --verify`. Caller checkout observed at HEAD 7b8cacc, status clean, before and during this pass; nothing in it was changed.

## Scope of this pass

- Bounded route: review-inputs.json reports status unavailable, sourceAvailable true, wholeRangeReady false, 399 selected, 0 omitted, 2 retained gaps (inv-4aa37d70 cluster bundle incomplete). No whole-range package was built or loaded.
- Read from captured Git: the whole production diff under src/ (every hunk outside the tests.rs files), the small doc, skill, lockfile and submodule hunks, tests/instruction_writers.rs, tests/herdr_spikes.rs, src/bootstrap/tests.rs, src/store/tests.rs, and the hg-66u.17 smoke hunk of tests/e2e_private_herdr.rs. The epic spec was read at head (sections 1-12 and Unchanged behavior / Post-Implementation Notes), not as a diff.
- Not read: the large unit and integration test files, the fakes' test usage, the rest of tests/e2e_private_herdr.rs, evidence/, docs/reviews/, and the roughly 335 run-archive paths under docs/superpowers/runs/.
- Bead state at review time: gates hg-66u.1 (D1), hg-66u.2 (D2) and hg-66u.3 (G5) are open; the gated beads hg-66u.14, .15, .18, .21, .24, .31, .32, .33, .40, .43, .44 and .68 are open; every ungated bead is closed.

## Baseline trace

- B1 (1:1, threads authoritative): partly delivered. `CloneRecord.threads_seat` is write-once through ThreadsSeatAssignment (src/threads/assignment.rs), with a one-live-clone-per-seat check, and SeatObservation (src/observe/seat_observation.rs) overlays Resolved, Retired and KeyConflict on the matcher from the production observe step (src/observe/step.rs `observe_seats`). The graph's own clone re-matching still runs: src/observe/matcher.rs clone passes, the planner LiveIndex live_ref and rebind paths, `/seat` rebind proposals and `clone rebind --pane`. Their removal (hg-66u.14, hg-66u.15) waits on D1.
- B2 (hard dependency on threads): delivered as a design property. The pin is v3.0.0 (Cargo.lock, third_party/herdr-threads 95e9eba), and the production ports come from `production_ports` in src/daemon/compose.rs.
- B3 (threads re-matching as good or better, then delegate): parity work hg-66u.23 is closed, but nothing is delegated yet; the matcher remains the source for every clone whose seat reads Unresolved or is unread.
- B4 (frictionless restarts): partly delivered. `plan_resume` (src/reconcile/planner.rs) relaunches after an incarnation change only when the clone has a resumable session and a seat whose Unresolved fact was read after the change (`restart_covered`). The fresh-agent path (hg-66u.21) and the unseated arm (hg-66u.33) wait on D1. The real-rig cold-restart smoke runs shell clones only.
- B5 (live handoff unattended): the production path exists. HerdrClient reconnect (hg-66u.25), the terminal-id pass of the matcher, and SeatObservation Resolved rebinding all run from the daemon loop. It is qualified only against FakeHerdr::live_handoff; the real-Herdr live-handoff test is an unconditional skip.
- B6 (no Herdr change): delivered. Env reaches the agent by a typed shell export before `agent.start` (src/herdr/client.rs `export_line`, `start_agent`), and Herdr is unchanged.
- B7 (undo reopens tab, resumes session, unretires seat): partly delivered. Undo, resurrection and seat reactivation all commit through `restore_commit` (src/plan/core_kinds.rs). The Retired arm is suppressed while the Restore is unacknowledged. The plan path reopens the clone's own pane, `succeed_seat` (src/threads/effects.rs) replaces the final-retired seat, and the resume driver launches once per epoch with `MarkLaunched`. The seat is not un-retired: a new seat id replaces it. UnretireSeat (hg-66u.31, hg-66u.32) waits on D2.
- B8 (agent exit keeps seat and memberships): delivered. `membership_seat` and the invite want key on `threads_seat`, not the occupant. A Required release fires only on a seat change or a vanished want, and the invite executor invites the recorded seat with `expect_seat`.
- B9 (continuity key): not delivered. Every start site passes `agent_env(&clone, false)` (src/reconcile/herdr_exec.rs start, lost-response branch, replace_session), and there is no ReserveContinuityKey port, reservation field or reserve want. `ClonePurge` passes `None` as the reservation. All of this waits on D1.
- B10 (undo hands-off, memberships restored on un-retire): not delivered. Interim succession re-invites on the successor seat, and the agent must accept once.
- B11 (/seat Option D): delivered. `PendingInvitation` lost `accept_command`, `render_seat` prints thread and constraint only, and skills/seat/SKILL.md step 4 delegates acceptance to herdr-threads.
- Human scope changes: none. The undelivered rows above therefore stay open. The spec's gating constraint governs delivery order but does not discharge any row.

## Integration seams examined

- Observe step: threads-unbound clones are removed from the baseline matches before the diff, so a seat-driven Unknown or retirement is never also reported as a pane close. Threads-decided retirements go through one `cascade_request(CascadeRule::Pane, ...)` op, and `clear_restore_on_retire` runs on every retire path (cascade, seat retire, clone retire).
- Pane uniqueness: a threads Resolved claim displaces a matcher claim on the same pane, and a displaced clone that threads then re-binds is taken back out of `unbound`. This matches spec section 6.
- Restore chain: `restore_commit` drops `seat_fact`, so succession acts only on a read taken after the commit. `succession` requires a verified pane, meaning Present and stamped with the clone's own token. `SeatTransition::Succeed` is admitted only against the matching Restore seat and epoch, with `check_free`. `start_needed` skips every clone with a Restore intent, and `plan_resume` gives the restore launch the epoch's op id with an identity revision of the mark-failure count. `ClearOnReattach` needs Resolved under the Restore seat plus an occupant. These pieces compose correctly in the code read here.
- Restart chain: `seat_observation::observe` ends a stale occupant when an Unresolved seat is read after an incarnation change and the last pane is a bare shell (hg-66u.78), which lets `restart_covered` fire. `resumed_key` limits the arm to one relaunch per incarnation change. A Restore launched before the change is handed to the restart arm (`launched_before`, `restart_owned` in herdr_exec).
- Dormant seats: `seated_clones` inspects only clones with a desired pane, so deactivation cannot retire them, and `SeatActivate` writes a Restore for each seated clone (hg-66u.80).
- Threads call guard: `LazyMap::map` now returns a `Guarded` that holds the threads-calls flock across `seat_for`, `resolve_seat` and `inspect` (hg-66u.61). This resolves the earlier review's guard-drop item.
- Per-seat refusals: `classify_inspect` treats only `NotFound` as seat-scoped, and `track_refusals` makes a clone Unknown only after `SEAT_LOST_AFTER` (90 s), never retiring it.
- Store writers: tests/instruction_writers.rs now inventories the purge delete in src/plan/core_kinds.rs and the tombstone delete in src/threads/assignment.rs. This resolves the earlier review's red `instruction_source_writers_are_inventoried`, but the fix has not been run on this branch.
- Restore with no resumable session: the hg-66u.81 orchestrator decision launches it fresh before D1. This adds behavior and lowers no baseline row; it supersedes a spec body line, which is not a human row.

## Ledger triage

- Recurring blocker ×3 (merge checks on a host without a C toolchain; hg-66u.5, .11, .27): an environment and process cluster, not a product defect. Together with Metrics check-fails 8 (fixed 0), merge-time checks produced no verified green runs.
- Recurring minor ×15 (process-class missing-evidence): per-task pass claims live in reports only, with no retained logs. This is treated as an evidence gap, not a defect in the code.
- Task 25 (hg-66u.17) open-acceptance-item: the live-handoff smoke is skipped but reports ok. This was misgraded as minor (it is a skipped test) and is raised to must-fix below.
- Task 25 name-overstates-coverage: `e2e_cold_restart_headless_resumes_via_the_driver` passes without running the resume driver. Also raised: the test name claims coverage it does not have.
- Task 25 open-acceptance-item for agent-bearing cold restart, plus stale-test-risk (`e2e_herdr_restart_rebind_no_mass_retirement_no_duplicates` was not run): carried as untested scope.
- Task 41 racy-assertion (`assert_clones_recover` can pass before the new incarnation is processed): B5's only automated evidence is weakened, so it is folded into the B5 must-fix item.
- Task 55 environment-dependent-failure (whole-file daemon_composed fails until about 50 stale herdr servers are stopped): needs a human action, carried as untested scope.
- Tasks 10, 12, 15, 26, 33, 4, 5, 54 and the remaining hygiene minors: judged acceptable, see the last section.
- No parked lines and no BLOCKED-AUTH lines are present. The ledger digest header is malformed and several Finish metrics were UNAVAILABLE, so the absence of ledger lines is not evidence of a clean task.

## Verdict
not ready (baseline rows B1, B3, B4, B7, B9, B10 undelivered behind unreleased D1/D2 gates; real-Herdr live handoff skipped; bounded review only; no full-suite run)

## Must fix before landing
- B1 not fully delivered (row B1): the graph's clone re-matching still runs beside threads (src/observe/matcher.rs clone passes, the planner LiveIndex live_ref and rebind paths, /seat rebind proposals, clone rebind --pane); hg-66u.14 and hg-66u.15 are open behind gate hg-66u.1 (D1), no human scope change row covers it, so hand it back open.
- B3 not delivered (row B3): re-matching is not delegated to threads; any clone whose seat reads Unresolved or is unread is still placed by the old matcher; delivery is the same D1-gated deletion (hg-66u.14, hg-66u.15).
- B4 partly delivered (row B4): plan_resume relaunches after a cold restart only a seated clone with a resumable session (src/reconcile/planner.rs restart_covered, eligible); the fresh-agent path hg-66u.21 and the unseated arm hg-66u.33 are open behind D1, so those clones still need an operator.
- B7 not delivered (row B7): graph undo does not un-retire the seat; before D2 it succeeds the final-retired seat with a new seat id (src/threads/effects.rs succeed_seat); UnretireSeat work hg-66u.31 and hg-66u.32 is open behind gate hg-66u.2 (D2).
- B9 not delivered (row B9): every agent start passes agent_env(&clone, false) in src/reconcile/herdr_exec.rs (start, lost-response branch, replace_session), so HERDR_THREADS_CONTINUITY_KEY is never exported, and there is no ReserveContinuityKey port, reservation field or reserve-key want (ClonePurge passes None); hg-66u.43, .44, .68, .33 and .40 are open behind D1, and an unreserved key must not be emitted meanwhile.
- B10 not delivered (row B10): the interim succession re-invites memberships on the successor seat and needs one re-acceptance; hands-off restore of memberships needs D2 (hg-66u.31, hg-66u.32).
- B5 has no real-Herdr qualification: tests/e2e_private_herdr.rs e2e_live_handoff_keeps_clones_on_their_seats is an unconditional support::skip that reports ok (ledger Task 25, misgraded minor: skipped test). The only automated B5 evidence is the fake-Herdr assert_clones_recover in tests/daemon_composed.rs, which ledger Task 41 flags as racy (it can pass before the post-handoff incarnation is processed). Make the skip visible as an open acceptance item rather than a pass, list live-handoff qualification open in the hand-back, and add the settle before that assertion.
- tests/e2e_private_herdr.rs e2e_cold_restart_headless_resumes_via_the_driver passes without exercising the resume driver: its clones are shell harness with no agents (ledger Task 25 name-overstates-coverage). Rename it, or carry B4's real-rig driver path as an explicitly open acceptance item, so a green run is not read as B4 qualification.

## Untested scope
- whole-epic review incomplete; bounded inspected hunks only; all remaining hunks and provenance gaps remain unreviewed
- Provenance gap retained: 7b8cacc0fd7d945cec497bc03ef9901834432e75:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:81 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete claimed collection
- Provenance gap retained: 7b8cacc0fd7d945cec497bc03ef9901834432e75:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:81 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete split bundle: missing cluster report or slices
- No full-suite measurement of head 7b8cacc exists; the caller runs the sweep after this invocation, so the branch must not be treated as tested, and no claim here rests on a test run.
- Unreviewed test paths: src/threads/tests.rs, src/reconcile/tests.rs, src/observe/tests.rs, src/undo/tests.rs, src/plan/tests.rs, src/herdr/tests.rs, src/model/tests.rs, src/transcripts/tests.rs, src/threads/service_tests.rs, tests/integration_sweep.rs, tests/daemon_composed.rs, tests/herdr_client.rs, and every tests/e2e_private_herdr.rs hunk except @@ -2384,4 +2385,316 @@.
- Unreviewed non-code paths: evidence/hg-66u.23/observe-tests.log, docs/reviews/2026-10-29-seat-review-H2-identity-design.md, and the roughly 335 run-archive paths under docs/superpowers/runs/ (roasts, refmaps, ledgers, prior review bundles, run.md); the epic spec was read at head, not reviewed as a diff, and its sections 13-16 and Key abstractions were not read.
- Real-Herdr agent-bearing configurations are unmet: idle and exited agents, auto-resumed agent reattach, resumable-session launch after cold restart and a bare pane after restart (ledger Task 25); the post-gates smoke hg-66u.18 is open.
- e2e_herdr_restart_rebind_no_mass_retirement_no_duplicates may encode the old matcher and was not run (ledger Task 25).
- Shell-export delivery (leading-space history suppression, export K=V on bash, zsh, sh and fish) and send_keys-before-agent.start ordering on a real Herdr are not verified; the HerdrClient::start_agent export-failure arms are untested.
- ThreadsSeatMap TargetUnresolved -> NotYet mapping and the v3.0.0 SeatInspect NotFound answer for an unknown seat are not exercised against a live service (ledger Tasks 12, 52).
- G5 limbo disposition (hg-66u.24, gate hg-66u.3) and the whole D1/D2-gated subtree are unexecuted.
- Whole-file tests/daemon_composed.rs at default parallelism fails on this host until about 50 stale herdr servers are stopped (ledger Task 55); a human must stop them before the sweep can read as a verdict on the code.
- Merge-time checks gave no verified green runs (Recurring blocker x3, Metrics check-fails 8 fixed 0), and the recurring missing-evidence cluster (x15) leaves per-task pass claims unverifiable from disk.
- The ledger is incomplete (malformed ledger-digest header; several Finish metrics UNAVAILABLE), so a missing ledger line is not evidence of a clean task.

## Deferred OK
- Doc drift, no product effect: CURRENT-DESIGN.md Undo bullet 1 still says to adopt the command's terminal pane, contradicting the new bullet 2; CURRENT-DESIGN.md (purge bullet) and GRAPH_STATE.md say a purged seated clone leaves a Finalize tombstone, but pre-D2 purge_tombstone (src/threads/assignment.rs) deletes it outright; DESIGN-NOTES.md "Undo terminal adoption" section is stale; README.md says every resolution uses a fresh operation key, which now holds for seat_for only (resolve_seat and succeed_seat use stable keys); docs/superpowers/specs/INDEX.md lists the spec as draft. Fix in one doc pass.
- Stale code text: the ClonePurge comments "hg-66u.13 passes the recorded key reservation" (src/plan/core_kinds.rs) name a closed bead, since the reservation work lives in hg-66u.43; the not_launchable branch in src/daemon/clone_status.rs is unreachable since hg-66u.81 (ledger Task 54); undo-adoption comments remain and UndoDeps.herdr is unused (ledger Task 16).
- Ordinary invitations are not released on seat succession (ledger Task 10); harmless before D2 because threads ends a final-retired seat's memberships itself; reassess in hg-66u.31 and hg-66u.32.
- Asynchronous admission windows (ledger Tasks 12, 15): resolve_seat returns Done before a late Assign rejection is known, and MarkLaunched is admitted asynchronously; neither retries a different seat, and later retries go Obsolete.
- Status coverage gaps (ledger Task 33): threads_pending is listed only after a refusal is recorded; restore_mark_failed meta lingers after retire but is epoch-filtered.
- ServiceAck recipients in src/transcripts/delivery.rs (feature threads-service-ack) still use the pane-derived seat (ledger Task 26); follow-up for B1 consistency once matching is delegated.
- ThreadsSeatMap::inspect issues one SeatInspect per seated clone sequentially, holding the threads-calls guard, on every observation step; acceptable at current graph sizes, follow-up to batch if the observe step lengthens.
- Task 4 fallthrough-owned-by-other in tab_for_seat, and Task 5 loose SplitPane assertion and deferred-rebind (addressed by hg-66u.11): narrow, disclosed, no data loss.
- Remaining hygiene minors (test placement, formatting noise, TDD order, signature drift, partial evidence log, weak or vacuous test setups in Tasks 32, 34, 43, 39, the tab-move parity n/a in Task 14): acceptable as recorded; tighten purged_seated_clone_is_not_revived_by_seat_resurrect so it cannot pass vacuously.
