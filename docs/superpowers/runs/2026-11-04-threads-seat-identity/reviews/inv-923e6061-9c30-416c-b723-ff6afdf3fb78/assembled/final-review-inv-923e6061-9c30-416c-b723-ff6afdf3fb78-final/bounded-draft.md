# Final review (bounded): hg-66u, threads seat as the clone identity

## Review route and source

- Range reviewed: 0a84c67554aa3fc8e8caf20db4163683ab30af61..3b2fc2d83e4f5cab4b1b65f0d643e096bf87e969 on super-auto/threads-seat-identity. Both commits verified with `git rev-parse --verify`. The caller checkout was at 3b2fc2d with a clean status at the start of the review and was not switched.
- This is a bounded review of captured source. The selection sidecar (review-inputs.json) reports status unavailable, sourceAvailable true and wholeRangeReady false, with no omissions. It keeps two provenance gaps, both for inv-4aa37d70-023c-483a-b47c-3dc801a586fe at run.md:77: "incomplete claimed collection" and "incomplete split bundle: missing cluster report or slices". No whole-range package was built or loaded.
- Changed-path inventory: 355 paths. 292 are run documents under docs/superpowers/runs. The other 63 are code, tests, docs, Cargo.lock and the third_party/herdr-threads gitlink.
- I inspected 115 hunks in 22 production paths (listed in bounded-coverage.json). These are the seat-identity core: SeatObservation, the observe step, ThreadsSeatAssignment, the threads effects and executor, the seat map and port, the resume driver in the planner and Herdr executor, the env export in the Herdr client, undo and resurrection, clone purge, `/seat` Option D, and the daemon wiring up to the lazy seat view.
- No cargo command was run. No full-suite measurement of this head exists: the caller runs the full-suite sweep after this invocation. Treat the branch as untested.

## Requirement baseline trace

The spec has a `## Requirement baseline` section (B1 to B11) and its human scope changes list is "none". So every baseline row that is not delivered fails the landing standard. At this head the three gate beads are still open: hg-66u.1 (D1), hg-66u.2 (D2) and hg-66u.3 (G5). Their dependent beads are open too.

- B1 (1:1, threads authoritative): partial. `threads_seat` is write-once (src/threads/assignment.rs), and SeatObservation takes precedence when threads speaks (src/observe/seat_observation.rs). However, `match_snapshot` still runs on every step (src/observe/step.rs) and the planner LiveIndex still re-matches clones. Deleting the matcher is hg-66u.14 and hg-66u.15, both open and gated on D1. The graph still has two sources of truth.
- B2 (hard dependency on threads): delivered. The pin is v3.0.0 (hg-66u.4). The production seat view is wired through `production_ports` and LazyMap (src/daemon/compose.rs).
- B3 (delegate re-matching to threads): not delivered. Same evidence as B1: the delegation step is hg-66u.14 and hg-66u.15, both open.
- B4 (restarts unattended): partial. The restart arm of the resume driver on the session path is present (`plan_resume` and `restart_covered` in src/reconcile/planner.rs). It needs `had_agent` and a resumable session. The fresh-agent and unseated paths are hg-66u.21 and hg-66u.33, both open (D1). The private-rig smoke hg-66u.17 is open, so no real-Herdr evidence exists.
- B5 (`herdr update --handoff` unattended): the code path exists (daemon reconnect hg-66u.25, composed test hg-66u.66/.77). Real-rig verification (hg-66u.17) is open, so it is listed as untested scope, not as missing.
- B6 (Herdr unchanged): delivered. The env goes in as a shell export typed through `send_keys` before `agent.start` (src/herdr/client.rs `export_line`).
- B7 (undo reopens tab, resumes, un-retires the seat): partial. The undo commit writes `Restore` intents (src/undo/compensate.rs, src/plan/core_kinds.rs `do_resurrect_seat`). The pre-D2 interim then succeeds a *new* seat (`SucceedSeat` in src/threads/effects.rs) instead of un-retiring. `UnretireSeat` and the finalize want are hg-66u.31 and hg-66u.32, both open (D2).
- B8 (agent exit keeps seat and memberships): delivered in code. `membership_seat` replaces `occupant_of`, and release happens only on a seat change or when the want is gone (src/threads/effects.rs clone_wants).
- B9 (continuity key): not delivered. Every start site calls `agent_env(&clone, false)` (src/reconcile/herdr_exec.rs), so `HERDR_THREADS_CONTINUITY_KEY` is never emitted. Reservation, keyed launch, key-link and KeyConflict production are hg-66u.43, .44, .68 and .40, all open. `seat_status` never produces KeyConflict (src/threads/mapping.rs).
- B10 (hands-off undo, memberships restored on un-retire): not delivered. The interim succession path re-invites memberships on the new seat, and they must be accepted again. That is the opposite of hands-off, and the fix is gated on D2 (hg-66u.31/.32).
- B11 (H2 Option D): delivered. `accept_command` is removed from `pending_invitations` and `render_seat`, and skills/seat/SKILL.md step 4 now delegates to herdr-threads.

## Integration seams examined

- Dormant seat or teamspace vs the Retired arm. `seated_clones` filters on the clone's own lifecycle only, not on its seat or teamspace being Dormant. Deactivation closes the tab and leaves the clone Active. Before D2, threads final-retires the closed pane's seat, and the next successful read hits `SeatStatus::Retired`. With no `Restore` intent present, that read unbinds the clone and pushes it to `retire` (a pane-cascade retirement). So deactivating a seat retires its clones, and reactivating it cannot bring them back. Bead hg-66u.80 records this defect and is still open. The defect is present at this head.
- Restore launch marking. `MarkLaunched` is admitted before `start_agent` is dispatched (src/reconcile/herdr_exec.rs). If the start then fails, `launched` is already Some. The restore trigger in `plan_resume` requires `launched: None`, and the restart arm takes over only after the next incarnation change (`launched_before`). This at-most-once behaviour is deliberate, so a failed restore start waits for an operator or a restart.
- One-live-clone-per-seat invariant. The spec (§2) says the writer's validator rejects any commit that violates it. In practice only `ThreadsSeatMutation::check_free`, plus duplicate pre-checks in `resolve_seat` and `succeed_seat`, enforce it. Undo and resurrection commits reactivate clones with their recorded `threads_seat` and never check it. Before D2 this cannot happen, because a final-retired seat is never resolved for another pane. Once D2 brings un-retire, the gap matters.
- The admission result of an Assign is not observed (`admit_threads_seat` returns before the writer's verdict, as the ledger notes for Task 12 and Task 15). A rejected Assign comes back as the same want with the same op key, which yields the same seat. It loops quietly instead of surfacing the rejection the spec promises.
- Seat reads are sequential: one `SeatInspect` RPC per seated clone per observe step, each with a 5 s budget (src/threads/mapping.rs `inspect`). With N clones and a slow service, one step can stall for up to N times 5 s. Not a correctness problem, but worth watching as the clone count grows.
- Doc drift in src/threads/effects.rs. The `clone_wants` doc comment now sits above `resolvable_pane`, and the `resolve_seat` doc comment sits on `clear_restore`. `Services.seat_view` in compose.rs still says "unused until SeatObservation". The planner has two equivalent meta helpers (`sync_meta`, `put_meta_if_changed`).

## Ledger triage

- Recurring missing-evidence cluster (×12 tasks): a process gap, not a product defect. Combined with the deferred full suite, though, no retained test log supports any pass claim at this head.
- Recurring blocker (C toolchain on the merge-check host, ×3): environmental. Those tasks later closed via toolbox runs.
- Two minors were misgraded and are promoted to must-fix. Task 16 and Task 26 name `instruction_source_writers_are_inventoried` and `sweep_action_and_effect_envelopes_round_trip` as failing (claimed to fail at base, never measured). Task 52 names a `herdr_client` linker abort that nobody investigated. hg-66u.65 is closed with no recorded reason, so whether these are fixed is unknown.
- Task 10 (old seat keeps its Ordinary membership after succession): acceptable before D2, because threads ends a final-retired seat's memberships itself.
- Task 12 and Task 15 (async admission): kept as a seam note above.
- Task 26 (transcripts/delivery.rs still derives the ServiceAck recipient from the pane via `seat_for`): pane-derived identity survives behind a feature flag. Acceptable as a follow-up, but B1 is not fully met until it moves to `threads_seat`.
- Task 32 (vacuous purge-resurrect test), Task 41 (racy recovery assertion, partly addressed by hg-66u.77), Task 51 (fully renumbered pane ids unrecoverable by design) and Task 33 (status gap for a pending Restore with no refusal): acceptable as follow-ups.
- No `parked` or `BLOCKED-AUTH` lines were found. The Metrics lines are UNAVAILABLE (ledger re-read malformed), so missing counts do not mean the run was clean.

## Verdict
not ready (baseline rows B1, B3, B4, B7, B9, B10 not delivered; gates D1/D2/G5 open; seat deactivation retires seated clones hg-66u.80 open; no full-suite run; bounded review only)

## Must fix before landing
- Baseline B1 not delivered: the old clone matcher (`match_snapshot` in src/observe/step.rs, planner LiveIndex) still runs beside SeatObservation; deletion beads hg-66u.14/hg-66u.15 open behind gate hg-66u.1 (D1); no human scope change row covers it.
- Baseline B3 not delivered: re-matching is not delegated to threads; same evidence as B1 (hg-66u.14, hg-66u.15 open, D1 gate open).
- Baseline B4 not delivered for fresh agents and unseated clones: `restart_covered` needs `had_agent` plus a resumable session and a `threads_seat` (src/reconcile/planner.rs); fresh-agent and unseated paths hg-66u.21/hg-66u.33 open (D1); private-rig smoke hg-66u.17 open.
- Baseline B7 not delivered as requested: undo succeeds a new seat via `SucceedSeat` (src/threads/effects.rs) instead of un-retiring the original; `UnretireSeat`/finalize beads hg-66u.31/hg-66u.32 open behind gate hg-66u.2 (D2).
- Baseline B9 not delivered: every start site passes `agent_env(&clone, false)` (src/reconcile/herdr_exec.rs), so `HERDR_THREADS_CONTINUITY_KEY` is never emitted; reservation/keyed-launch/key-link/KeyConflict beads hg-66u.43/.44/.68/.40 open (D1).
- Baseline B10 not delivered: the interim restore re-invites memberships on the succeeded seat, which need accepting again (not hands-off); un-retire membership restore gated on D2 (hg-66u.31/.32 open).
- Known product defect hg-66u.80 (open): `seated_clones` (src/observe/seat_observation.rs) ignores Dormant seats and teamspaces, so after deactivation the pre-D2 final-retired pane seat hits the Retired arm and retires the clone; it cannot be restored by reactivation.
- Misgraded minors naming failing tests: `instruction_source_writers_are_inventoried`, `sweep_action_and_effect_envelopes_round_trip` (Task 16/26) and the `herdr_client` linker abort (Task 52) must be measured green or fixed in the end-of-epic full-suite run; hg-66u.65 closed with no recorded reason.

## Untested scope
- No full-suite measurement of head 3b2fc2d exists; the caller runs it after this invocation, so the branch is untested.
- Bounded review only: inspected 115 hunks across 22 production paths listed in bounded-coverage.json; the whole-range review is still outstanding.
- Unreviewed changed paths: all 292 run documents under docs/superpowers/runs/ (including the spec beyond line 340, roasts, ledgers, profiles and the archived inv-4aa37d70 review bundle).
- Unreviewed changed paths: src/daemon/clone_status.rs, src/observe/matcher.rs, src/observe/mutations.rs, src/observe/mod.rs, src/transcripts/capture.rs, src/transcripts/mod.rs, src/threads/fake.rs, src/herdr/fake.rs, src/config.rs, src/cli/session.rs, src/model/change.rs, src/reconcile/session.rs, src/daemon/mod.rs.
- Unreviewed hunks: src/daemon/compose.rs from `@@ -949,6 +1132,111 @@ mod lazy {` onward (test and fakes hunks); src/undo/preview.rs `@@ -425,32 +418,6 @@` and `@@ -465,31 +432,18 @@`.
- Unreviewed test paths: src/observe/tests.rs, src/reconcile/tests.rs, src/threads/tests.rs, src/threads/service_tests.rs, src/undo/tests.rs, src/plan/tests.rs, src/model/tests.rs, src/herdr/tests.rs, src/transcripts/tests.rs, src/bootstrap/tests.rs, src/store/tests.rs, tests/daemon_composed.rs, tests/e2e_private_herdr.rs, tests/herdr_client.rs, tests/herdr_spikes.rs, tests/instruction_writers.rs, tests/integration_sweep.rs.
- Unreviewed docs and pins: CURRENT-DESIGN.md, DESIGN-NOTES.md, GRAPH_STATE.md, README.md, docs/reviews/2026-10-29-seat-review-H2-identity-design.md, docs/superpowers/specs/INDEX.md, skills/graph/SKILL.md, evidence/hg-66u.23/observe-tests.log, Cargo.lock, third_party/herdr-threads gitlink.
- Provenance gap: 3b2fc2d83e4f5cab4b1b65f0d643e096bf87e969:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:77 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete claimed collection.
- Provenance gap: 3b2fc2d83e4f5cab4b1b65f0d643e096bf87e969:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:77 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete split bundle: missing cluster report or slices.
- B5 live handoff and B4 cold restart have no private-rig evidence (hg-66u.17 open; hg-66u.18 post-gates smoke open); composed tests use fakes only.
- G5 limbo disposition (hg-66u.24) unexecuted; gate hg-66u.3 open.
- Threads-side behaviour assumed but not verified here: what `SeatInspect` reports for a pane whose agent exited with the pane open (B8 path and restart-arm triggering).

## Deferred OK
- Restore launch is at-most-once per epoch: `MarkLaunched` precedes dispatch, so a failed `start_agent` leaves the restored pane bare until operator action or the next incarnation change (src/reconcile/herdr_exec.rs).
- One-live-clone-per-seat is enforced in the assignment mutation, not by the writer's validator as spec §2 states; undo/resurrection commits do not check it; unreachable before D2, required before hg-66u.31 lands.
- Assign admission result is unobserved (Task 12/15); a rejected Assign loops on the same seat silently instead of being surfaced.
- `SeatInspect` is issued sequentially per seated clone with a 5 s budget each (src/threads/mapping.rs); step latency scales with clone count when threads is slow.
- Doc drift in src/threads/effects.rs (`clone_wants` and `resolve_seat` doc comments misplaced), the stale `Services.seat_view` comment in src/daemon/compose.rs, and duplicate `sync_meta`/`put_meta_if_changed` helpers in src/reconcile/planner.rs.
- Ordinary membership on the old seat is not released on succession (Task 10); threads ends final-retired memberships itself before D2.
- transcripts/delivery.rs ServiceAck still derives the recipient seat from the pane via `seat_for` (Task 26); move to `threads_seat` with the D1 work.
- Recurring missing-evidence cluster (12 tasks) and toolchain merge-check blocker (3 tasks): process gaps, no product effect.
- Test-quality minors: vacuous purge-resurrect test (Task 32), racy recovery assertion (Task 41), timing-based guard test (Task 36), test placement and fixture shortcuts (Tasks 9, 34, 43, 51).
