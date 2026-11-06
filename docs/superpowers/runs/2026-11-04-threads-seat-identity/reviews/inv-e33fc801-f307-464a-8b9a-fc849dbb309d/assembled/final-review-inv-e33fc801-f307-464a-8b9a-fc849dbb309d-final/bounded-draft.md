# Final review (bounded): epic hg-66u, threads seat as the clone identity

Captured source: base 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784, head e4bf390519a0daf410f90769098c3a9acbba3bcf (both verified with `git rev-parse --verify`). Invocation inv-e33fc801-f307-464a-8b9a-fc849dbb309d; review source bound in the ledger before any write.

## Scope of this pass

Bounded captured-source review. The review-inputs sidecar is status unavailable, sourceAvailable true, wholeRangeReady false, no omissions, two retained gaps (run.md:80 inv-4aa37d70: incomplete claimed collection; incomplete split bundle). No whole-range package was built or loaded. I read the full changed-path inventory (387 paths) and inspected the production launch, observation, seat-assignment, restore, participation, undo and /seat paths hunk by hunk from captured Git, plus the design doc's requirement baseline and sections 1-12, and the gate/gated bead states in `bd`. The exact inspected hunks are in bounded-coverage.json. The caller checkout was on head e4bf390 with a clean status at start; nothing was switched or restored. No full-suite measurement of this branch exists yet: the sweep is deferred to the caller, so nothing here treats the branch as tested.

## Requirement baseline trace

- B1 (1:1, threads authoritative): partly delivered. `threads_seat` is write-once through `ThreadsSeatMutation` (src/threads/assignment.rs), with `check_free` enforcing one live clone per seat and succession admitted only against a matching Restore epoch. SeatObservation (src/observe/seat_observation.rs) overrides the matcher when threads reads Resolved, Retired or KeyConflict. But `match_snapshot` still runs first in `step_once` (src/observe/step.rs) and decides location whenever threads reads Unresolved, is unreachable, or the clone has no seat. Two sources of truth remain, which is the pre-D1 interim the spec describes, not the requested end state.
- B2 (hard dependency on threads): delivered. Threads being unreachable changes no binding (`ThreadsRead::Unavailable`).
- B3 (delegate re-matching to threads): not delivered. The clone passes of observe/matcher.rs and the planner's LiveIndex rebind and live_ref re-matching are still in place; hg-66u.14 and hg-66u.15 are open behind gate hg-66u.1 (D1, open).
- B4 (unattended restarts): partly delivered. `plan_resume` (src/reconcile/planner.rs) relaunches only clones that pass `restart_covered`: `had_agent`, a resumable session (`p.resume.is_some()`), a `threads_seat`, and an Unresolved fact read after the incarnation change. Fresh-agent and unseated restarts (hg-66u.21, hg-66u.33) are open behind D1. The private-rig headless smoke passes with shell clones only, so the driver has no real-Herdr evidence (ledger, Task 25).
- B5 (unattended resume after `herdr update --handoff`): not shown. `e2e_live_handoff_keeps_clones_on_their_seats` (tests/e2e_private_herdr.rs) is an unconditional `support::skip`. The only handoff evidence is the fake-backed daemon_composed test, which the ledger records as having a racy assertion (Task 41).
- B6 (no Herdr changes): delivered. The key is delivered by a validated, space-prefixed `export` line typed through `send_keys` before `agent.start` (src/herdr/client.rs `export_line`); `StartAgent` gains only a port-side `env` field.
- B7 (undo reopens the tab, resumes, un-retires the seat): not delivered as requested. Undo, resurrection and seat reactivation all go through `restore_commit` (src/plan/core_kinds.rs) and write a Restore intent; pre-D2 the threads step is `SucceedSeat`, which assigns a new seat, so the original seat is never un-retired. hg-66u.31 and hg-66u.32 are open behind gate hg-66u.2 (D2, open).
- B8 (agent exit keeps seat and memberships): delivered. `membership_seat` ignores occupancy; releases fire only on a seat change or a vanished want (src/threads/effects.rs `clone_wants`).
- B9 (continuity key): not delivered. Every start site passes `agent_env(&clone, false)` (src/reconcile/herdr_exec.rs start and replace paths), so `HERDR_THREADS_CONTINUITY_KEY` is never exported and nothing calls `ReserveContinuityKey`; hg-66u.43, .44, .68 and .40 are open behind D1.
- B10 (memberships restored hands-off on un-retire): not delivered. The interim succession re-invites on the successor seat, and the new memberships need accepting again.
- B11 (/seat stops printing accept commands): delivered. `accept_command` and its formatter are removed, `render_seat` prints thread and constraint only, and skills/seat/SKILL.md step 4 delegates acceptance to herdr-threads.

The baseline has no human scope change rows ("- none"), so gating on unreleased threads features does not excuse B3, B7, B9 or B10, nor the missing parts of B1, B4 and B5.

## Integration seams examined

- Restore lifecycle across tasks composes correctly in the inspected code. Every Restore writer (resurrection, undo `restore_mutate`, `SeatActivate`) uses `restore_commit`. Every clone-retirement writer (CascadeMutation, `do_retire_seat`, `CloneRetire`) calls `clear_restore_on_retire`. A grep of src at head found no other `lifecycle = CloneLifecycle::Retired` writer. The Retired arm in SeatObservation skips a clone whose Restore is not yet acknowledged, and `write_restore_intent` drops the stale `seat_fact`, so succession acts only on a read taken after the commit.
- The one launch per epoch is enforced by `MarkLaunched`, which is admitted before `start_agent` in src/reconcile/herdr_exec.rs. A start that then times out (`Unknown`) or is transient leaves the epoch marked, so nothing relaunches it within the incarnation, as the spec requires (Unchanged behavior, amend-code-1). An operator must notice it through status.
- `start_needed` now excludes every `restart_covered` clone even with no incarnation change pending. A clone whose seat reads Unresolved in steady state (for example after its agent exits, if threads v3.0.0 unresolves the seat then; the ledger escalation says this is unverified) is left to `plan_resume`, which fires only once per incarnation change. This matches B8's intent (no auto-relaunch), but it rests on the unverified threads behaviour.
- Tab and workspace matching by preserved id now applies in any incarnation (src/observe/matcher.rs, planner LiveIndex `workspace`/`seat_tab`/`tab_owner`). The run's parked escalation (roast-pr-1) says Herdr's `reserve_workspace_ids` can reissue the highest deleted workspace id after a restore. If so, an absent bound id can be reissued to an unrelated workspace and linked by equality. This is unverified external behaviour and is listed below as untested.
- `SeatActivate` writes a Restore for every seated active clone and clears its occupant and runtime, assuming the deactivation close actually closed the pane. If the close failed or did not run, the clone's still-live pane and agent would be rematched by token, and the restore launch would be refused as "agent already running". I could not confirm in the inspected hunks whether a test covers this.

## Ledger triage

- The ledger's own Metrics lines are UNAVAILABLE (malformed Finish re-read), and one earlier line reports `landed≠completed: 29 vs 13`, so an absence in the ledger is not evidence of cleanliness.
- Recurring minor `missing-evidence` (×14 across 14 tasks): a process-class cluster. Most task test claims rest on report prose, which makes the caller's full-suite measurement the only real evidence. Kept as one grouped observation; each task's identity is retained in the ledger.
- Recurring blocker "merge check runs on host lacking C linker toolchain" (×3: hg-66u.5, .11, .27): environmental. It ties to the `herdr_client` linker abort noted in Task 52. It must be settled by the full-suite run on a host with a linker, not waived.
- Task 25 (hg-66u.17) minors (skipped live-handoff test reported as ok, headless smoke without agents, agent-bearing restart parts unmet, the stale-matcher e2e test never run) name a skipped test and open acceptance items, so they were misgraded and are triaged as must-fix below.
- Task 41 (hg-66u.66) racy assertion in `assert_clones_recover` weakens the only B5 evidence. Must-fix together with B5.
- run.md parked "run-regression open: daemon_composed subprocess::* fail in whole-file run (daemon exits 101), pass alone" is a known failing test. Must-fix: measure it green or fix it.
- The parked instruction_writers and integration_sweep regressions are recorded resolved by hg-66u.65 at tree 50a8cba. The inventory now lists src/plan/core_kinds.rs and src/threads/assignment.rs (inspected), but this needs re-measuring at head in the suite run.
- The remaining minors (timing-based test, weak or vacuous assertions, stale comments, dead `not_launchable`/`finalize` branches, the transcripts ServiceAck path still on `PaneSeatMap::seat_for`, doc drift) are acceptable follow-ups.

## Must fix before landing

- Baseline B3 not delivered: the clone re-matching (observe/matcher.rs clone passes run via `match_snapshot` in src/observe/step.rs, planner LiveIndex rebind and live_ref re-matching) is not delegated to threads; hg-66u.14/hg-66u.15 open behind D1 gate hg-66u.1; no human scope change row.
- Baseline B1 only partial: write-once `threads_seat` and one-live-clone-per-seat hold, but the old matcher still decides location whenever threads reads Unresolved or is unreachable, so threads is not yet the single authority (same open beads as B3).
- Baseline B9 not delivered: every start site uses `agent_env(&clone, false)` in src/reconcile/herdr_exec.rs, so `HERDR_THREADS_CONTINUITY_KEY` is never exported and no reservation exists; hg-66u.43/.44/.68/.40 open behind D1.
- Baseline B7 not delivered as requested: undo and resurrection succeed a new seat (`SucceedSeat`, src/threads/effects.rs) instead of un-retiring the original; hg-66u.31/hg-66u.32 open behind D2 gate hg-66u.2.
- Baseline B10 not delivered: the interim restore re-invites memberships on the successor seat, and they must be accepted again, which is not hands-off; un-retire membership restore is gated on D2.
- Baseline B4 partial: `restart_covered` (src/reconcile/planner.rs) requires `had_agent`, a resumable session and a `threads_seat`, so fresh agents and unseated clones are not relaunched after a cold restart (hg-66u.21/hg-66u.33 open, D1); the headless private-rig smoke passes with shell clones only, so the resume driver is unproven on real Herdr.
- Baseline B5 unproven: `e2e_live_handoff_keeps_clones_on_their_seats` is an unconditional skip that reports ok, and the fake-backed `assert_clones_recover` (tests/daemon_composed.rs) can pass before the new incarnation is processed (ledger Task 41); provide a real live-handoff qualification or a human scope change row.
- Known failing test: `cargo test --features test-support --test daemon_composed` subprocess::* fail in a whole-file run (daemon exits 101) and pass alone (run.md parked run-regression); measure it green or fix it.
- No full-suite measurement of this branch exists (sweep deferred to the caller); instruction_writers, integration_sweep, herdr_client (linker abort) and the feature-flag matrices must be measured at head e4bf390.
- Post-gate configuration smokes hg-66u.18 (fresh agents, pane closed during downtime, undo keeps seat) are open.
- A whole-range final review is still outstanding: this pass is bounded, and the provenance gaps for inv-4aa37d70 (incomplete claimed collection; incomplete split bundle) remain.

## Untested scope

- Real-Herdr live handoff (`herdr update --handoff`) with idle agents, bare panes and an exited agent: never run (skipped test).
- Agent-bearing attended and headless cold restart (auto-resumed agent reattach, resumable-session launch, idle/exited agents, bare pane): unmet per ledger Task 25.
- `e2e_herdr_restart_rebind_no_mass_retirement_no_duplicates` may encode the old matcher and was not run (ledger Task 25).
- Preserved-id tab/workspace matching in any incarnation against Herdr reissuing a deleted highest workspace id after restore (parked roast-pr-1 escalation, UNVERIFIED external).
- Threads v3.0.0 seat state after an agent exits with the pane open (Resolved vs Unresolved), which `start_needed`/`restart_covered` depend on (parked design-roast escalation, UNVERIFIED external).
- `SeatActivate` restore when the deactivation close did not actually close the pane (live pane and agent remain): not traced to a test in this pass.
- `export_line` delivery and history behaviour under real zsh, fish and sh panes; a shell rejecting `export` falls back silently.
- Restore whose start returns `Unknown` or a transient error after `MarkLaunched`: by design it is not relaunched within the incarnation, and only status shows it; no end-to-end test was confirmed in this pass.
- Unreviewed changed paths (not inspected in this bounded pass): src/threads/mapping.rs, src/threads/fake.rs, src/threads/tests.rs, src/threads/service_tests.rs, src/model/clone.rs, src/model/tests.rs, src/daemon/clone_status.rs, src/herdr/fake.rs, src/herdr/tests.rs, src/observe/tests.rs, src/plan/tests.rs, src/reconcile/tests.rs, src/store/tests.rs, src/transcripts/capture.rs, src/transcripts/mod.rs, src/transcripts/tests.rs, src/undo/adopt.rs (deleted), src/undo/preview.rs, src/undo/tests.rs, src/bootstrap/tests.rs, tests/daemon_composed.rs, tests/e2e_private_herdr.rs, tests/herdr_client.rs, tests/herdr_spikes.rs, tests/integration_sweep.rs, Cargo.lock, third_party/herdr-threads (submodule bump), skills/graph/SKILL.md, evidence/hg-66u.23/observe-tests.log.
- Unreviewed hunks: src/threads/effects.rs from `@@ -1019,6 +1193,25 @@ pub struct ThreadsExecutor {` onward (executor side of ResolveSeat, SucceedSeat, AckRestore, ClearRestore, DropTombstone, Invite with `expect_seat`); src/daemon/compose.rs from `@@ -440,9 +537,9 @@` onward (status registration, LazyMap seat view and threads-call guard, fakes).
- Unreviewed docs and run artifacts: CURRENT-DESIGN.md, DESIGN-NOTES.md, GRAPH_STATE.md, README.md, docs/reviews/2026-10-29-seat-review-H2-identity-design.md, docs/superpowers/specs/INDEX.md, the MVP design doc edit, the threads-seat-identity design doc beyond sections 1-12 and the tail, and every file under docs/superpowers/runs/2026-11-04-threads-seat-identity/ (roasts, ledgers, profiles, run.md, settled-tree roasts, review archives for inv-4aa37d70, inv-cc29fc83 and the bounded invocations).
- Provenance gap retained: e4bf390519a0daf410f90769098c3a9acbba3bcf:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:80 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete claimed collection.
- Provenance gap retained: e4bf390519a0daf410f90769098c3a9acbba3bcf:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:80 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete split bundle: missing cluster report or slices.
- 26 outOfRange entries (h1-instruction-gate run archives) are informational; they were neither selected nor waived, and not reviewed.

## Deferred OK

- Stale doc comment on `Services::seat_view` ("unused until SeatObservation", src/daemon/compose.rs) now that the observe step reads it.
- Unreachable `not_launchable` branch in src/daemon/clone_status.rs and the `finalize` tombstone label in `ClonePurge::plan`; both are pre-D2 leftovers that hg-66u.31/D1 work will revisit.
- src/transcripts/delivery.rs still resolves ServiceAck recipients via `PaneSeatMap::seat_for` (ledger Task 26); a pane-derived identity on an ack path is a follow-up, not a regression.
- `restore_mark_failed` journal meta lingers after retirement (ledger Task 33); it is epoch-filtered in status.
- Test-quality minors (fixed 500 ms sleep in the guard test, vacuous `purged_seated_clone_is_not_revived_by_seat_resurrect`, fixture-simulated undo, unused binding, missing RED runs): hardening follow-ups.
- Recurring missing-evidence process cluster (×14): fix it in the workflow so reports keep raw logs; it is covered for this branch by the full-suite must-fix item.
- Doc drift punch-list items (CURRENT-DESIGN.md:65, README.md:89 tombstone, threads 3.0.0 and resume-driver status entries) and historical threads 2.6.4 references in README.md.

## Verdict
not ready (baseline rows B3, B7, B9, B10 undelivered and B1, B4, B5 partial or unproven behind open D1/D2 gates; known failing daemon_composed run; no full-suite measurement; bounded review only)

## Must fix before landing
- see the Must fix before landing section above (same items)

## Untested scope
- see the Untested scope section above (same items)

## Deferred OK
- see the Deferred OK section above (same items)
