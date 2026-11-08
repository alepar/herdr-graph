# Final review (bounded): hg-66u, threads seat as the clone identity

Range: 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784..b943d9532e100c58cb84c59efc0c1bdbb4aeb07e (branch super-auto/threads-seat-identity, 443 changed paths). Invocation inv-cef1709c-2f16-4a29-a694-c154c775b2ea.

## Review route

The review-input sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, omitted 0 and two retained provenance gaps (inv-4aa37d70 at run.md:84), so this is a bounded captured-source review, not a whole-range review. The failed assembly and the original sidecar were left untouched. I read the full changed-path inventory, the spec's Requirement baseline (B1..B11 and human scope change S1), the latest prior bounded review (inv-3f52876e at 1e0b2e35), the child bead states (all 94 children of hg-66u closed), and concentrated on the delta since that review: hg-66u.14 (matcher deletion under S1, commits f18003c..155acc1) and hg-66u.18 (LazyThreads forwarding e3bfbec and the post-gate rig smokes 1fc97d1). The hunks actually inspected are listed in bounded-coverage.json. I also read head source beyond the diff (src/observe/step.rs, src/observe/diff.rs rebind, src/reconcile/mod.rs step/step_fresh, src/ports/threads.rs) to trace baseline rows; those diff hunks are not claimed as reviewed. No full-suite run of this head exists; the caller runs it after this invocation, so the branch is not treated as tested.

## Status of the prior review's blocking items

- Critical LazyThreads gap (B7, B10): fixed in e3bfbec. `impl ThreadsPort for LazyThreads` (src/daemon/compose.rs, head ~953-975) now forwards unretire_seat, finalize_retirement and release_continuity_key under the endpoint guard. Every ThreadsPort method with a trait default is now forwarded (checked against src/ports/threads.rs:115-231). The regression test is the private-rig e2e e2e_close_then_undo_keeps_the_seat_and_memberships, which the hg-66u.18 report says failed before the fix; no default-suite composed test covers it.
- Purge tombstones never completing in production: resolved by the same forwarding. The separate hg-66u.31 stuck-tombstone defect (a Finalize for a seat threads does not know stays Deferred forever, no operator drop) is unchanged and carried below.
- hg-66u.14 (B1, B3): delivered under human scope change S1. The matcher's terminal-id, cwd, native-session and rebind-adopted passes are deleted (src/observe/matcher.rs). A seated clone is linked only to the pane its same-incarnation binding names and is otherwise located by SeatObservation. A seatless clone is located by token, else by its preserved pane id in any incarnation, unless the pane carries another live clone's token. LiveIndex::pane in src/reconcile/planner.rs follows the same rule, plus the journal live ref for create correlation.
- hg-66u.18 rig acceptance: run on Herdr 0.9.5 with herdr-threads 3.1.0, and the evidence logs are retained (evidence/hg-66u.18). The plain run had 7 ok, two of them skipped and reported as ok. G5 passed with a test-support build and over the real 10 minute window (661 s). The rig ran on tree 1fc97d1, whose src/ and tests/ are identical to head.

## Requirement baseline trace

- B1 (1:1, threads authoritative): seated clones are located only by threads (SeatObservation), threads_seat is write-once, and the matcher never re-matches a seated clone by token or older binding. Delivered.
- B2 (hard dependency): with threads unreachable, observe_seats returns Unavailable and changes no binding. After a restart, rebind marks seated clones Unknown (src/observe/step.rs rebind `m.unmatched` arm), and nothing launches without a post-restart threads answer (plan_resume). Delivered.
- B3 (re-matching as good or better, then delegate): delegated for seated clones; seatless clones use the S1 preserved-pane-id rule. Delivered as amended by S1.
- B4 (frictionless restarts): the resume driver (plan_resume restart arm: restart_covered and restart_unseated, RelaunchOccupant with grace) uses bound_pane (any incarnation). Fake-backed sweep_headless_cold_restart_resumes now models preserved pane ids. On a real rig, keyed reattach, unseated seating and G5 pass, but the graph launching the fresh keyed agent itself is unmet on a rig (no agent binaries). Delivered in code; real-agent acceptance is open.
- B5 (herdr update --handoff): code path is the same seat-located rebind; FakeHerdr::live_handoff tests exist. The rig test is skipped (it would install a release) and prints ok. Real-Herdr acceptance is open (verification matrix row "assumed").
- B6 (Herdr unchanged): no Herdr change in the range. Delivered.
- B7 / B10 (undoable retirement, hands-off membership restore): production path now forwards unretire; rig test shows same seat id, same invitation ids, no succession or re-invite. Delivered.
- B8 (agent exit keeps seat and memberships): membership keyed to threads_seat; note that a same-incarnation Unresolved answer now unbinds the clone (Unknown) rather than leaving the token match, so an agent-exited clone reads Unknown until something checks in. Seat and memberships are kept. Delivered; status semantics changed.
- B9 (continuity key): keyed env and reservation in place; rig keyed check-in reattaches the same seat. Delivered.
- B11 (/seat stops printing accept commands): unchanged since prior review, delivered there.

## Cross-task seams

- hg-66u.14 x SeatObservation: Unresolved now calls unbind before the stale-occupant check. That makes the guard `!m.clone_pane.contains_key(&clone)` in src/observe/seat_observation.rs vacuous for every Unresolved clone. Not wrong, but the guard and its punch-listed negative test no longer protect anything.
- hg-66u.14 x planner: plan_resume targets idx.bound_pane, which accepts a seated clone's binding from any incarnation, while LiveIndex::pane and the matcher accept only the same incarnation for seated clones. These are consistent only under spec §9's preserved-pane-id assumption (ledger minor, hg-66u.14).
- hg-66u.18 x threads G5 hold: per the hg-66u.18 report (config 4), after a restart threads holds fresh seats back while any seat is unresolved. A seated clone that never re-checks in (relaunch blocked by "last pane missing", a key conflict, or a harness failure) therefore holds back seating of every unseated clone. This is cross-project and was not measured with real agents.
- Reconciler contract: Reconciler::step_fresh plans on a fresh snapshot whose observations are not committed. It has no production caller at head (only tests), so the stricter seated-clone LiveIndex::pane cannot create duplicate panes from it today.

## Ledger triage

No parked findings, parked completions or BLOCKED-AUTH lines for this invocation's tasks. The run.md parked list holds sweep and roast degraded-verdict items, which are covered by the measurement item below. Recurring blocker (no C toolchain on the host, hg-66u.5/.11/.27, and again hg-66u.14's fix pass) is environmental, but it left hg-66u.14's test fix unmeasured, which is graded must-fix below. Recurring minor missing-evidence (x15 tasks, plus x3 for the gated G5 run) is process-class and stays as an evidence gap; the x3 G5 part is now partly answered by evidence/hg-66u.18/e2e-g5-real-window.log. hg-66u.18's two open-acceptance-item minors and the TargetUnresolved invite defect are misgraded as minor (an acceptance gap and a known product defect) and are listed as must-fix. The hg-66u.31 stuck-tombstone minor is a known product defect and is retained as must-fix from the prior review.

## Verdict
not ready (bounded review; no full-suite run at head; hg-66u.14 test fix unmeasured; B4 graph-launched agent and B5 live handoff lack real-rig acceptance)

## Must fix before landing
- [Important] Measure the full suite at b943d95 before landing: threads::tests::unseated_reserved_clone_gets_its_seat_from_the_reservation_answer failed in both hg-66u.14 runs (evidence/hg-66u.14/cargo-test.log:1013-1018, reserve op vs key_link op) and its fix 155acc1 was type-checked only (no C linker), and the run.md parked run-regressions (instruction_writers inventory, integration_sweep sweep_action_and_effect, daemon_composed subprocess parallelism) have no measurement at this head.
- [Important, B4, B5] Hand back as open acceptance items, not passes: graph-launched fresh keyed agent after a cold restart is unmet on a real rig (configs 1 and 4), live handoff is skipped on the rig and the skipped test prints ok, and G5 "no resume attempted" is vacuous for a shell clone (ledger hg-66u.18 open-acceptance-item lines).
- [Important] Known product defect carried from inv-3f52876e (ledger hg-66u.31 stuck-tombstone): src/threads/effects.rs finalize_seat leaves a Finalize tombstone whose seat threads does not know Deferred forever with no operator drop; fix or file it with an owner.
- [Minor] Known product defect (ledger hg-66u.18 follow-up-unrecorded): while a clone is retired the graph still plans an invite effect that fails TargetUnresolved ("invite target missing, retired"); stop planning invites for a retired clone's invitations or file the follow-up bead.
- [Minor] Test names overstate B4 coverage: tests/e2e_private_herdr.rs e2e_cold_restart_headless_resumes_via_the_driver (shell clones, no agent launched) and e2e_unseated_clone_headless_cold_restart_is_launched_and_seated (the launch is unmet; a fixture check-in stands in); rename them so they are not read as launch evidence.
- [Minor] Doc drift: README.md:89 and README.md:130 still say the threads submodule is pinned at v3.0.0 (95e9eba) while it is at v3.1.0 (482e0d2), and CURRENT-DESIGN.md:98 still says the current adapter uses threads 3.0.0 while CURRENT-DESIGN.md:56 says 3.1.0.

## Untested scope
- No full-suite measurement of head b943d9532e100c58cb84c59efc0c1bdbb4aeb07e exists; the caller runs the sweep after this invocation.
- Provenance gap: b943d9532e100c58cb84c59efc0c1bdbb4aeb07e:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:84 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete claimed collection.
- Provenance gap: b943d9532e100c58cb84c59efc0c1bdbb4aeb07e:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:84 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete split bundle: missing cluster report or slices.
- Bounded review: a whole-range review remains outstanding; only the hunks in bounded-coverage.json were inspected.
- Unreviewed hunks of inspected paths: src/daemon/compose.rs (all hunks except the LazyThreads one), src/observe/matcher.rs (@@ -206 and @@ -254), src/reconcile/planner.rs (all hunks not listed in coverage).
- Unreviewed product paths in this range: src/threads/* (effects, mapping, adapter, assignment, fake, mod), src/plan/* (core_kinds, grammar, kinds_extra), src/undo/*, src/transcripts/*, src/bootstrap/*, src/cli/*, src/model/* (clone, change, launch), src/herdr/client.rs, src/ports/threads.rs, src/observe/step.rs and mutations.rs, src/reconcile/herdr_exec.rs, desired.rs and session.rs, src/daemon/clone_status.rs, doctor.rs and mod.rs, src/config.rs, scripts/herdr-release-smoke, skills/graph and skills/seat, Cargo.lock, third_party/herdr-threads pin.
- Unreviewed tests: src/*/tests.rs, src/threads/service_tests.rs, tests/daemon_composed.rs, tests/e2e_private_herdr.rs (outlined only), tests/integration_sweep.rs (delta read only), tests/herdr_client.rs, tests/herdr_spikes.rs, tests/instruction_writers.rs.
- Unreviewed docs and evidence: DESIGN-NOTES.md, GRAPH_STATE.md, docs/reviews/2026-10-29-seat-review-H2-identity-design.md, docs/superpowers/specs/INDEX.md, the docs/superpowers/runs/** run documents and evidence/** logs other than evidence/hg-66u.18 and the hg-66u.14 failure excerpt.
- The LazyThreads D2 forwarding is exercised only by the gated private-rig e2e test; no default-suite test drives unretire, finalize or release through production_ports.
- Live herdr-threads 3.1.0 checks in tests/threads_real.rs (ReserveContinuityKey, UnretireSeat, FinalizeRetirement) were not run (ledger hg-66u.24, .31, .43, .44).
- Behaviour of CLI and status for seated clones that are now Unknown whenever threads answers Unresolved in the same incarnation (agent exited, restored before check-in) was not reviewed.
- G5 seat hold: an unresolved seated clone that is never relaunched holds back seating of new or unseated clones after a restart; not measured with real agents.
- Recurring missing-evidence cluster (x15 tasks): per-task pass counts have no retained logs or source-tree-bound evidence records.

## Deferred OK
- S1-accepted risk: a seatless clone binds by preserved pane id in any incarnation, so if Herdr reissues an id to a different pane after a restore that drops tokens, it can bind the wrong pane (design-roast id-reissue dissent); the human accepted this rule in S1.
- hg-66u.14 write-only Binding.terminal_id, now documented as diagnostic only (src/model/common.rs).
- hg-66u.14 bound_pane accepts any-incarnation bindings for the seated resume target; consistent with spec §9's preserved-id assumption.
- The vacuous `!m.clone_pane.contains_key(&clone)` guard in seat_observation's stale-occupant branch after Unresolved unbinds; harmless.
- hg-66u.18 out-of-scope-file: the compose.rs forwarding fix is justified by the regression it caught.
- hg-66u.18 default-skip G5 test: it ran in both the test-support and real-window modes with retained logs.
- Dead-code warning for E2e::start_with_threads in tests/e2e_private_herdr.rs.
- Recurring blocker: merge check on a host without a C toolchain (hg-66u.5, .11, .27); environmental.
- Carried from inv-3f52876e without change: hg-66u.8 ordinary-invite-succession, hg-66u.7 async admission and doc drift, hg-66u.43 string-matched Conflict, hg-66u.40 host-unaware conflict mapping, hg-66u.30 async MarkLaunched window, hg-66u.58 stale restore_mark_failed meta, hg-66u.81 dead not_launchable branch, hg-66u.15 legacy CloneRebind variant, hg-66u.78 renumbered-pane restart, hg-66u.23 tab-moved n/a, hg-66u.11 older-incarnation tab fallthrough.
- Test-quality and process minors carried from inv-3f52876e (timing-based guards, weak assertions, fixture-simulated paths, tdd-order, formatting churn, committed evidence logs); none hides a product defect found here.
