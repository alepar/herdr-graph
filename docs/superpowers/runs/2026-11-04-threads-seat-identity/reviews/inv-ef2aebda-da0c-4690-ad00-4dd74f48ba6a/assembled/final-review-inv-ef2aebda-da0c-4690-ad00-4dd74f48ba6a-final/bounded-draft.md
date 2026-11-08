# Final review (bounded) — epic hg-66u, branch super-auto/threads-seat-identity

Captured source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 8be3a04fe03fce28f927e96a87da0eacc3ed538b (both verified with `git rev-parse --verify`). The caller checkout was at 8be3a04 with a clean status when read; no caller file was edited.

## Review route

The review-inputs sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, no omissions and two retained provenance gaps (inv-4aa37d70 at run.md:75: incomplete claimed collection; incomplete split bundle). This is therefore a bounded captured-source review: the full 339-path inventory was read, and 24 code/spec paths were inspected hunk by hunk from captured Git (listed in bounded-coverage.json). No whole-range package was built. A whole-range review remains outstanding.

## Requirement baseline trace

The spec has a `## Requirement baseline` section (B1-B11, no human scope change rows). Traced at head from production entry points:

- B1 (1:1 clone-seat mapping, threads authoritative): partial. `CloneRecord.threads_seat` is write-once through `bookkeeping.threads_seat` (src/threads/assignment.rs) and SeatObservation lets threads speak for Resolved/Retired/KeyConflict. The graph's clone matcher still runs (pre-D1 precedence), so the graph still re-derives location for every clone threads does not speak for.
- B2 (hard dependency on threads): delivered (pin v3.0.0, seat view wired in daemon compose).
- B3 (delegate re-matching to threads): not delivered. Matcher deletion hg-66u.14 and rebind removal hg-66u.15 are open behind gate hg-66u.1 (D1).
- B4 (frictionless restarts): partial. The session-path resume driver exists (plan_resume restart and restore arms). The fresh-agent path hg-66u.21 is open behind D1, and the reachability of the restart arm after a realistic headless cold restart is in doubt (see Must fix).
- B5 (herdr update --handoff): code path present (hg-66u.25, preserved-id equality hg-66u.11); evidence is a composed test whose assertion the ledger flags as racy; no private-rig run (hg-66u.17 open).
- B6 (no Herdr change): delivered. The env goes in as a space-prefixed `export` typed through `send_keys` before `agent.start` (src/herdr/client.rs export_line), with names and values validated.
- B7 (undo reopens tab, resumes session, unretires seat): not delivered as requested. Undo writes a Restore intent and reopens the clone's own pane; the pre-D2 interim then allocates a successor seat (succeed_seat) instead of un-retiring. hg-66u.31/.32 are open behind gate hg-66u.2 (D2).
- B8 (agent exit keeps seat and memberships): delivered. `membership_seat` ignores occupancy, and releases fire only on seat change or a vanished want.
- B9 (continuity key): not delivered. Every start site calls `agent_env(&clone, false)` (src/reconcile/herdr_exec.rs), so the key is never emitted; reservation and keyed launches (hg-66u.43/.44/.68/.33/.40) are open behind D1.
- B10 (undo fully hands-off, memberships restored on un-retire): not delivered. Pre-D2 succession re-invites on the successor seat and needs one re-acceptance.
- B11 (/seat Option D): delivered. `accept_command` was removed from `pending_invitations` and `render_seat`, and skills/seat/SKILL.md step 4 now delegates to herdr-threads.

## Cross-task seams examined

- SeatObservation (hg-66u.28) x reconciler closes: SeatObservation never checks seat or teamspace lifecycle or close predictions (described under Must fix).
- Resume driver (hg-66u.30) x observation occupancy: restart_covered and plan_resume need `occupant == None` and an ended session; occupancy ends only for matched clones (diff::occupancy iterates `matches.clone_pane`). Every resume test calls `test_end_occupant` and restarts the fake with `keep_metadata = true`.
- Assignment x undo/resurrection: write_restore_intent drops seat_fact; MarkLaunched is gated on threads_done; ClearOnReattach requires a Resolved fact plus an occupant. These compose consistently in the inspected code.
- Participation (hg-66u.8, .59): invites are gated on clone_active; retired, unpurged clones keep their links; a Released or Retired row on the recorded seat stays settled unless the backfill set migration_reinvite.

## Verdict

not ready (bounded review only; baseline rows B3, B7, B9, B10 undelivered behind open D1/D2 gates, B1/B4 partial, two plausible seam defects, no full-suite measurement)

## Must fix before landing

- B3 undelivered (and B1 partial): the graph's clone re-matching is not delegated to threads at head; observe/matcher.rs clone passes and the planner LiveIndex rebind and live_ref re-matching still run; hg-66u.14 and hg-66u.15 are open behind gate hg-66u.1 (D1); no human scope change row covers this.
- B9 undelivered: no continuity-key reservation or keyed launch exists; every agent start calls agent_env(&clone, false) in src/reconcile/herdr_exec.rs, so HERDR_THREADS_CONTINUITY_KEY is never exported; hg-66u.43, hg-66u.44, hg-66u.68, hg-66u.33 and hg-66u.40 are open behind D1.
- B7 and B10 undelivered: graph undo does not un-retire the seat or restore memberships hands-off; the pre-D2 interim (src/threads/effects.rs succeed_seat) allocates a successor seat and needs one re-acceptance; hg-66u.31 and hg-66u.32 are open behind gate hg-66u.2 (D2).
- B4 partial: the fresh-agent restart path (hg-66u.21) is open behind D1, so a clone with no resumable session is not relaunched unattended after a cold restart.
- B4 seam, PLAUSIBLE (src/reconcile/planner.rs plan_resume and restart_covered): the restart arm requires occupant None and an ended session. After a real headless cold restart in which Herdr drops metadata tokens and reallocates terminal ids (spec Problem description), the matcher does not match the clone, SeatObservation stays silent on Unresolved before D1, and diff::occupancy never ends the stale occupant. The arm then never fires and the agent is not resumed. All resume tests end the session via test_end_occupant and restart with keep_metadata=true. Verify on a private rig, or end the occupant once an Unresolved seat fact is read after an incarnation change.
- Seam, PLAUSIBLE (src/observe/seat_observation.rs seated_clones and observe): Active clones of a deactivated (Dormant) seat or teamspace are still inspected. plan_closes closes their tab or workspace as a predicted induced close, and threads retires those pane seats (spec §8). The Retired arm then emits an observed pane-close cascade that retires the clones, without checking seat lifecycle or predictions, so reactivation finds them retired instead of recreating them. No test covers deactivation with a seated clone.

## Untested scope

- No full-suite measurement of head 8be3a04 exists; the caller runs the sweep after this invocation, so the branch must not be treated as tested.
- Bounded review only; a whole-range review remains outstanding; provenance gap retained: 8be3a04fe03fce28f927e96a87da0eacc3ed538b:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:75 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete claimed collection.
- Provenance gap retained: 8be3a04fe03fce28f927e96a87da0eacc3ed538b:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:75 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete split bundle: missing cluster report or slices.
- Unreviewed code paths: src/threads/mapping.rs, src/threads/fake.rs, src/daemon/compose.rs, src/daemon/clone_status.rs, src/daemon/mod.rs, src/config.rs, src/plan/core_kinds.rs (purge command and resurrect intent hunks), src/transcripts/capture.rs, src/transcripts/mod.rs, src/undo/adopt.rs, src/undo/mod.rs, src/undo/preview.rs, src/herdr/fake.rs, src/observe/mod.rs, third_party/herdr-threads, Cargo.lock.
- Unreviewed test paths: src/observe/tests.rs, src/reconcile/tests.rs (read only the resume-test helpers), src/threads/tests.rs, src/threads/service_tests.rs, src/plan/tests.rs, src/model/tests.rs, src/herdr/tests.rs, src/undo/tests.rs, src/bootstrap/tests.rs, src/store/tests.rs, src/transcripts/tests.rs, tests/daemon_composed.rs, tests/integration_sweep.rs, tests/herdr_client.rs, tests/herdr_spikes.rs, tests/instruction_writers.rs, evidence/hg-66u.23/observe-tests.log.
- Unreviewed hunks: src/threads/mod.rs @@ -234,7 +215,6 @@; src/undo/compensate.rs @@ -426,80 +412,6 @@ (partly read); tests/e2e_private_herdr.rs hunks after @@ -971,49 +971,44 @@.
- Unreviewed docs: CURRENT-DESIGN.md, DESIGN-NOTES.md, GRAPH_STATE.md, README.md, skills/graph/SKILL.md, docs/reviews/2026-10-29-seat-review-H2-identity-design.md, docs/superpowers/specs/INDEX.md, docs/superpowers/runs/2026-10-02-herdr-graph-implementation design note, and the other 274 run-directory files under docs/superpowers/runs/2026-11-04-threads-seat-identity (roasts, ledgers, profiles, review archives); only the design doc was read.
- Real-Herdr qualification never ran: hg-66u.17 (live handoff and cold restart, pre-D1) and hg-66u.18 (post-gates) are open; B5 rests on tests/daemon_composed.rs assert_clones_recover, which the ledger flags as racy (Task 41).
- G5 limbo disposition (hg-66u.24, gate hg-66u.3) and the whole D1/D2-gated subtree are unexecuted.
- The rewritten undo tests in tests/e2e_private_herdr.rs are compile-checked only, never run (Task 16 minor).
- Recurring missing-evidence (x9 across 9 tasks): test pass counts exist only as report prose with no retained logs, so no per-task pass claim can be verified from disk.
- Recurring blocker (x3: merge check on a host without a C toolchain) and Metrics check-fails 8 (fixed 0): merge-time checks gave no verified green runs.
- The ledger is incomplete (landed 29 vs completed 13, transport truncation gaps), so absence of ledger findings is not evidence of a clean task.

## Deferred OK

- Task 10 ordinary-invite-succession: Ordinary links are not released on succession, but pre-D2 the predecessor seat is final-retired and threads ends its memberships itself; reassess in hg-66u.31/.32.
- Task 10 unverified-assumption (retired clone fresh invite): superseded; clone_wants now gates invites on clone_active, and the invite executor returns Obsolete for a non-active clone (§16.2).
- Task 12 async-admission-unobserved: the executor pre-checks the holder before admitting Assign, so only a race is left unsurfaced; follow-up to name a rejected Assign in status.
- Task 5 deferred-rebind (token-dropped tabs and workspaces after handoff): addressed by hg-66u.11, since LiveIndex and the matcher now bind tabs and workspaces by preserved ids across incarnations.
- Task 4 fallthrough-owned-by-other: narrow, disclosed edge in tab_for_seat; no data loss.
- Task 15 async-admit-window and unseated-arm-deferred: the first is bounded by Obsolete on later retries; the second belongs to the D1 beads already listed under Must fix.
- Task 33 status-coverage-gap and stale-meta: status-only gaps (threads_pending named only after a refusal; restore_mark_failed meta lingers but is epoch-filtered); follow-up.
- Task 34 behavior-change-undocumented: an Ordinary Released or Retired row on the recorded seat stays settled as a deliberate release; document it in code.
- Task 26 follow-up-pane-derived-identity: ServiceAck recipients in src/transcripts/delivery.rs still use PaneSeatMap::seat_for under feature threads-service-ack; follow-up for B1 consistency.
- Task 32 dead-code, weak-assertion and suite-not-run: tighten purged_seated_clone_is_not_revived_by_seat_resurrect so it cannot pass vacuously; the full-suite sweep covers the rest.
- Task 16 stale-comment and Task 12 doc-drift: comment cleanup only.
- Process-only minors (tdd-order, missing-red-run, test-placement, scope-beyond-brief, formatting-noise, build-env-deviation, untracked-submodule, stale-line-reference, ledger-sha-mismatch, unlabeled-findings, signature-drift, historical-version-reference, timing-based-test, unused-binding, weak-edge-case tests in Tasks 11 and 35): no product effect.
