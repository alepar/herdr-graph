# Final review (bounded) — epic hg-66u, integration branch super-auto/threads-seat-identity

Invocation inv-4daa8441-9a36-4c1c-b79b-6915ec7c18ca. Captured source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 9c29e91b4892f1799cbf083f08f187475e2e3634 (both verified with `git rev-parse --verify`; caller checkout HEAD was 9c29e91 and its status clean when observed).

## Scope of this bounded pass

- Route: bounded captured-source review. The selection sidecar `assembled/final-review-inv-4daa8441-9a36-4c1c-b79b-6915ec7c18ca-final/review-inputs.json` reports status unavailable, sourceAvailable true, wholeRangeReady false, 378 selected paths, 0 omitted, and two retained gaps at run.md:79 (inv-4aa37d70: incomplete claimed collection; incomplete split bundle, missing cluster report or slices). No whole-range package was built or loaded; the failed assembly and the original sidecar were left in place.
- Read: the full changed-path inventory (378 paths, of which about 315 are run/review docs under `docs/superpowers/runs/`), the root spec in full at head (including the Requirement baseline, §13, §14, §16 and the Post-Implementation Notes), `bd show hg-66u` and the open-bead list, and the product source hunks listed in bounded-coverage.json (37 paths, 153 hunks): threads assignment, clone model, SeatObservation and the observe step, the planner resume driver, the Herdr executor and client export, threads effects/mapping/port, undo/plan restore writers, clone purge, status, transcripts export check, `/seat` Option D.
- Not read: all test files (`src/*/tests.rs`, `tests/*.rs`, `src/threads/service_tests.rs`), fakes (`src/herdr/fake.rs`, `src/threads/fake.rs`), `src/undo/adopt.rs` (deleted), `src/undo/tests.rs`, `src/daemon/compose.rs` from hunk `@@ -949,6 +1132,111 @@` on, DESIGN-NOTES.md, GRAPH_STATE.md, README.md, skills/graph/SKILL.md, docs/reviews, docs/superpowers/specs/INDEX.md, evidence/hg-66u.23, and the run/review document tree.
- No full-suite measurement of this head exists; the caller runs the sweep after this invocation. Nothing here treats the branch as tested.

## Requirement baseline trace

The root spec carries a `## Requirement baseline` with rows B1–B11 and `Human scope changes: none`. The plan's Global Constraint ("if a gate has not released ... the hand-back lists each requirement it blocks as open") is a plan rule, not a human scope change, so it does not lower the standard. Gates hg-66u.1 (D1), hg-66u.2 (D2) and hg-66u.3 (G5) are open, and with them hg-66u.14, .15, .21, .24, .31, .32, .33, .40, .43, .44, .68, plus the rig smokes hg-66u.17/.18.

- B1 (1:1, threads authoritative): partly delivered. `threads_seat` is write-once through `bookkeeping.threads_seat` (assignment.rs `Assign`/`Succeed` with `check_free`), and SeatObservation lets threads speak with Resolved/Retired/KeyConflict. The graph still re-matches clones itself: the matcher's clone passes and LiveIndex clone matching remain (hg-66u.14, .15 open, D1-gated). The spec Goal ("the graph no longer re-matches clones itself") is not met at head.
- B2 (hard dependency on threads): delivered. Pin v3.0.0 (Cargo.lock, submodule 95e9eba); an unreachable threads yields `ThreadsRead::Unavailable` and changes nothing.
- B3 (delegate re-matching to threads): not delivered; delegation is the matcher deletion (hg-66u.14/.15).
- B4 (frictionless unattended restarts): partly delivered. `plan_resume` restart arm relaunches a seated clone with a resumable session, an Unresolved fact read after the incarnation change and a bare bound pane. The fresh-agent restart path (hg-66u.21), the unseated arm (hg-66u.33) and the private-rig qualification (hg-66u.17) are open.
- B5 (`herdr update --handoff` unattended): production path present (preserved ids, SeatObservation rebinding from a Resolved seat), evidenced only by fake-Herdr tests; the ledger records the composed live-handoff assertion as racy (Task 41 hg-66u.66) and the rig smoke hg-66u.17 is open.
- B6 (no Herdr change): delivered; env reaches the agent by a typed ` export` line before `agent.start` (client.rs `export_line`).
- B7 (graph undo reopens the tab, resumes the session, unretires the seat): reopen and resume delivered through `restore_commit` plus the resume driver's restore trigger (fresh when nothing resumes, hg-66u.81). "Unretires the seat" is not delivered: pre-D2 the restore succeeds the final-retired seat with a new one (hg-66u.31/.32 open, D2-gated).
- B8 (agent exit keeps seat and memberships): delivered. `membership_seat` keys participation on `threads_seat`; Required links are released only when `link.seat != threads_seat` or the want disappears; occupancy and retirement release nothing.
- B9 (continuity key): not delivered. Every start site passes `agent_env(&clone, false)`; no reservation, reserve-key or key-link want exists (hg-66u.43/.44/.68 open).
- B10 (hands-off undo, memberships restored on un-retire): not delivered; pre-D2 succession re-invites and needs one re-acceptance.
- B11 (H2 Option D): delivered. `accept_command` and its formatter are gone, `render_seat` prints thread and constraint, skills/seat/SKILL.md step 4 delegates to herdr-threads.

## Integration seams examined

- Restore lifecycle across tasks composes as the spec intends at head: every Restore writer goes through `restore_commit` (resurrection, undo, SeatActivate), which drops the occupant, runtime and held `seat_fact`; the Retired arm skips a clone whose Restore is unacknowledged; `succession` acts only on a fresh Retired{final} fact and a token-verified reopened pane; `Succeed` sets `threads_done` and the new seat; the planner's restore trigger plans one StartAgent under the epoch's op; the executor admits `MarkLaunched` before dispatch; `ClearOnReattach` needs Resolved under `Restore.seat` plus an occupant. Every retire path (`do_retire_seat`, `CloneRetire`, the observed cascade) calls `clear_restore_on_retire`.
- Ordinary start rule and resume driver do not overlap: `start_needed` skips any clone with a Restore intent and any `restart_covered` clone.
- Invitations: the invite executor invites only for an active clone with a recorded seat and never re-resolves; `expect_seat` replaces `expect_occupant` on every write-back.
- Production wiring: `Services::from_env` replaces the default seat view with `LazyMap` from `production_ports`, which holds the threads-calls guard across each delegated call; `RuntimeLoop` reads it once per step.
- Pre-D2 purge deletes a seated clone record outright (`purge_tombstone` returns None for a seated clone), matching §14.2; undo preview refuses an action whose retired clone was purged.

## Ledger triage

- Recurring blocker "merge check runs on host lacking C linker toolchain" (hg-66u.5, .11, .27): environment, not a code defect; it means merge-time checks ran outside the prescribed toolbox, which feeds the outstanding full-suite measurement.
- Recurring minor `[missing-evidence]` ×12 (hg-66u.25, .5, .27, .38, .35, .8, .6, .29, .26, .65, .70, .77): task evidence lives only in structured replies with no retained logs; reassessed as process, kept as a group; it leaves no task-level test claim independently checkable, so it adds to untested scope rather than to the code list.
- Misgraded minors that name tests not run: Task 16 hg-66u.29 `[uncompiled-e2e-tests]` (tests/e2e_private_herdr.rs rewritten adoption tests compile-checked only); Task 52 hg-66u.79 `[suite-not-run]` (integration test binaries, `--all-targets --all-features`, the herdr_client linker abort); Task 32 hg-66u.57 `[suite-not-run]` (`--features test-support` / `--all-features`). Triaged as must-fix (run them).
- Task 41 hg-66u.66 `[racy-assertion]`: the live-handoff (B5) composed test can pass before the loop processes the new incarnation; it is the only fake-level evidence for a baseline row, so it is must-fix.
- Task 10 hg-66u.8 `[unverified-assumption]` (retired clones get fresh invite wants): resolved at head; `clone_wants` now guards invitations with `g.clone_active(c)`.
- Parked escalation run.md:98 (roast-pr-1): `LiveIndex::bound_pane` and preserved-id tab/workspace matching accept a bound id in any incarnation with no token/label corroboration, and Herdr v0.9.5 can reissue the highest workspace/pane ids after a restart; it awaits a human decision ("confirm as Should-fix"). Confirmed at head: `bound_pane` is a bare `pane_by_id` lookup used by both resume-driver triggers and the executor fallback.
- Parked roast-pr-3 degraded verdicts: verification debt (full suite at the final tip, e2e adoption tests on a private rig via hg-66u.17, whole-range final review unavailable because inv-4aa37d70 cluster-4 was never written) remains open at this head.
- Metrics carry `read-ledger:finish: ledger transport malformed — missing or malformed "# ledger-digest:" header`; ledger counts are therefore not evidence of a clean run.
- No BLOCKED-AUTH lines and no parked completion or parked finding lines (other than the escalations above) were found in the ledger.

## Findings

- [Must fix, baseline] B1 and B3 not delivered at head: the graph's clone re-matching (observe matcher clone passes, LiveIndex clone matching, `/seat` rebind proposals, `clone rebind --pane`) is still in place; hg-66u.14 and hg-66u.15 are open behind the D1 gate hg-66u.1.
- [Must fix, baseline] B9 not delivered: `src/reconcile/herdr_exec.rs` passes `agent_env(&clone, false)` at every start site, so no agent is ever launched with `HERDR_THREADS_CONTINUITY_KEY`; hg-66u.43, .44, .68 are open (D1).
- [Must fix, baseline] B7 ("unretires the seat") and B10 (memberships restored on un-retire) not delivered: pre-D2 restores go through seat succession to a new seat with one re-acceptance; hg-66u.31 and hg-66u.32 are open (D2 gate hg-66u.2).
- [Must fix, baseline] B4 partly delivered: the restart arm needs a resumable session and a recorded seat; fresh-agent restart (hg-66u.21), the unseated arm (hg-66u.33) and the real-rig qualification hg-66u.17 are open.
- [Must fix, PLAUSIBLE] `LiveIndex::bound_pane` (src/reconcile/planner.rs, hunk `@@ -144,6 +205,24 @@`) resolves the clone's committed pane id in any incarnation without a token or label check, and both resume-driver triggers and the executor fallback in `src/reconcile/herdr_exec.rs` (`idx.pane(&clone).or_else(|| idx.bound_pane(&clone))`) launch into it; if Herdr reissues that id after a restart to another bare-shell pane (run.md:98 cites `reserve_workspace_ids` / per-workspace pane numbering), the clone's resumed session starts in a foreign pane. The escalation awaits a human decision; it must be decided before landing.
- [Deferred OK] CURRENT-DESIGN.md Undo bullet says a purged seated clone leaves a `Finalize{seat}` tombstone; pre-D2 code deletes the record outright (`purge_tombstone`), so the doc describes post-D2 behaviour as current.
- [Deferred OK] `Services::new` in src/daemon/compose.rs defaults `seat_view` to `FakeThreadsSeatView` in non-test code; the production `from_env` overrides it, so it is a constructor footgun, not a live defect.
- [Deferred OK] `resolve_seat` and `MarkLaunched` admissions are durable enqueues; an apply-time rejection is not observed by the executor (ledger hg-66u.7, hg-66u.30). The bare-pane guard prevents a double launch; the cost is a silent retry loop on a seat held elsewhere, which the executor's own held-elsewhere pre-check mostly pre-empts.
- [Deferred OK] `stuck_restores` (src/daemon/clone_status.rs) lists `launched_no_reattach` from the moment the mark is written (before any check-in can arrive), never names an unacknowledged Restore with no refusal yet, and keeps an unreachable `not_launchable` branch after hg-66u.81.

## Verdict

not ready (baseline rows B1, B3, B4, B7, B9, B10 undelivered behind open D1/D2 gates; bound_pane corroboration awaits a human decision; no full-suite run; whole-range review incomplete)

## Must fix before landing

- Baseline B1/B3: deliver the deletion of the graph's clone re-matching (hg-66u.14, hg-66u.15) once D1 (hg-66u.1) releases, or obtain a human scope change row; evidence: matcher clone passes and LiveIndex clone matching present at 9c29e91.
- Baseline B9: deliver key reservation and keyed launches (hg-66u.43, hg-66u.44, hg-66u.68); evidence: `agent_env(&clone, false)` at every start site in src/reconcile/herdr_exec.rs.
- Baseline B7/B10: deliver un-retire with membership restore (hg-66u.31, hg-66u.32) after D2 (hg-66u.2); evidence: pre-D2 restore uses `SeatTransition::Succeed` to a new seat with re-acceptance.
- Baseline B4: deliver the fresh-agent restart path (hg-66u.21) and the unseated restart arm (hg-66u.33), and run the private-rig cold-restart and live-handoff qualification (hg-66u.17, hg-66u.18).
- Decide the open escalation at run.md:98: `LiveIndex::bound_pane` and preserved-id matching accept a bound id from any incarnation with no token or label corroboration, and the resume driver launches into it (PLAUSIBLE wrong-pane launch after a Herdr id reissue).
- Run the full suite at this head, including integration binaries, `--all-targets --all-features`, `--features test-support` and the herdr_client binary whose linker abort was never investigated (ledger hg-66u.79, hg-66u.57).
- Run the rewritten tests/e2e_private_herdr.rs adoption tests on a private rig; they were only compile-checked (ledger hg-66u.29).
- Fix the racy live-handoff assertion in tests/daemon_composed.rs `assert_clones_recover` (settle before asserting) so the B5 evidence can fail on regression (ledger hg-66u.66).

## Untested scope

- Whole-range review: this bounded pass inspected 37 changed paths (153 hunks); the remaining 341 changed paths are unreviewed, including every test file, the fakes, src/undo/tests.rs, src/daemon/compose.rs from hunk `@@ -949,6 +1132,111 @@` on, DESIGN-NOTES.md, GRAPH_STATE.md, README.md, skills/graph/SKILL.md, docs/reviews/2026-10-29-seat-review-H2-identity-design.md, docs/superpowers/specs/INDEX.md, evidence/hg-66u.23/observe-tests.log and the run/review document tree.
- Provenance gap: 9c29e91:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:79 inv-4aa37d70-023c-483a-b47c-3dc801a586fe incomplete claimed collection.
- Provenance gap: 9c29e91:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:79 inv-4aa37d70-023c-483a-b47c-3dc801a586fe incomplete split bundle (missing cluster report or slices); cluster-4 (runs-top) of that review was never written.
- No full-suite measurement of 9c29e91 exists; the run-regressions resolved at tree 50a8cba (instruction_writers, integration_sweep) are not re-measured at this head.
- Recurring missing-evidence (12 tasks): task test claims lack retained logs and cannot be checked independently.
- Unverified external assumptions from roast-design-1, untested on a real rig: 90 s grace versus Herdr auto-resume latency; a seat staying Resolved after its agent exits with the pane open on threads v3.0.0 (B8); headless cold restart restoring bare-shell panes; tab and workspace id preservation across live handoff.
- SeatActivate's `restore_commit` clears the binding of every seated active clone of the reactivated seat on the assumption that deactivation closed the panes; reactivation while a deactivation close is still pending or failed is not covered by any test read here.
- The real `ThreadsSeatMap::inspect` and `resolve_seat` `TargetUnresolved` paths are exercised only through fakes (ledger hg-66u.6, hg-66u.7, hg-66u.60).
- Herdr pane-id renumbering on a full restart is unrecoverable by design and untested (ledger hg-66u.78).

## Deferred OK

- CURRENT-DESIGN.md Undo bullet describes the post-D2 Finalize tombstone as current behaviour; fix in the hg-66u.31 doc pass.
- `Services::new` defaults the seat view to `FakeThreadsSeatView` outside tests; production overrides it.
- `resolve_seat` and `MarkLaunched` apply-time rejections are unobserved by the executor; the bare-pane guard prevents a double launch.
- `stuck_restores` reports `launched_no_reattach` immediately after the mark, omits an unacknowledged Restore with no refusal yet, and keeps an unreachable `not_launchable` branch (ledger hg-66u.58, hg-66u.81).
- Ordinary invitations are not released on succession (ledger hg-66u.8); the predecessor seat is final-retired and threads ended its memberships.
- ServiceAck recipients in src/transcripts/delivery.rs still derive identity through `PaneSeatMap::seat_for` (ledger hg-66u.26); feature-gated follow-up.
- Stale undo-adoption comments and unused `UndoDeps.herdr` (ledger hg-66u.29); cosmetic.
- Weak assertion in purged_seated_clone_is_not_revived_by_seat_resurrect (ledger hg-66u.57); pre-D2 purge deletes the record, so resurrect cannot find it.
