# Final seam review, inv-1, epic hg-lnw

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 8f8ec538ca1f1e4d8d897804aae726472f753ef9, each verified with `git rev-parse --verify <sha>^{commit}`. `bind-review-source` for inv-1 exited 0. The caller checkout was at 8f8ec538 with a clean status at the start and the end of this review. No review worktree was created, no caller file was edited and no test was run. Every read used `git show`/`git diff` at the captured head, the assembled clusters.json and the five cluster review files.

Spec: bead hg-lnw and its design doc at head. The doc's `## Requirement baseline` has B1 to B4 and no human scope change rows.

## Cluster inputs

- Cluster 1 (root): not ready. Docs say every daemon older than 3.6.0 is refused by name.
- Cluster 2 (docs): not ready. The design doc of record contradicts the shipped behaviour, and its Post-Implementation Notes section is empty.
- Cluster 18 (src): ready, with deferred items only.
- Cluster 19 (tests): ready, with untested-scope items only.
- Cluster 20 (third-party): ready. The pin is 7c3c090, tag v3.6.0.
- Clusters 3 to 17 (evidence-hg-lnw-*): no review file. They hold committed test logs only and were not dispatched.

## Composition across clusters

- Pin (third_party) to lock (root) to adapter (src): the gitlink at 7c3c090 is v3.6.0. Cargo.lock resolves herdr-threads 3.6.0. `ServiceThreads::new` uses `with_level(GRAPH_SERVICE_LEVEL)` with V4 (adapter.rs:62, 281), which is the ht-lzhi API in that range. The interfaces agree.
- Port (src/ports/threads.rs) to fake (src/threads/fake.rs) to real adapter: `resurrect_seat` and `rebind_held_seat` return `SeatRestoreOutcome`. The fake answers `AlreadyResolved` on a replay and `NotFound` past finalization (fake.rs:421, 433, 470). The real adapter maps `SeatResurrected` to Done, `SeatAlreadyResolved` to AlreadyResolved and `NotFound` to definitive (adapter.rs:105-125). Tier-5 `threads_real` checks the same Done-then-AlreadyResolved sequence against the real 3.6.0 daemon through `production_ports`. So the tier-1/2 sweep (tests cluster, which uses the src fake) and production follow the same contract.
- Planner and executor (src) to the tier-3 tests (tests): the tests drive the production CLI and the composed daemon. `keyless_cold_restart_without_session_returns_to_its_seat` sees the `threads.rebind_held_seat` effect Done before the relaunched agent checks in, with no `restart_held` and no `rebind_refused`. That covers the whole chain: the observer reads Unresolved, `rebind_wants` emits, `plan_resume` records `seat_unresolved:`, and `keyless_start_gate` defers and then opens. The evidence logs (hg-lnw.9 tier3-final-rebased, hg-lnw.23 rebased) come from the merged task branches, not from 8f8ec538.
- Printed remedy (src clone_status.rs:66-67) to docs (root) to real threads (third_party): the strings agree with `herdr-threads human seat rebind SEAT --pane PANE --operator`. No test feeds graph's printed string to real threads, because tier 5 builds its own argv. Listed under Untested scope.
- Daemon version contract across root, src and third_party: the root docs, the adapter comment and the error text (adapter.rs:64-68, "older than 3.6.0"), the doctor upgrade step (doctor.rs:114) and the Cargo.toml comment all assume that only a pre-3.6.0 daemon lacks v4. I confirmed the cluster 1 finding in the submodule: v3.5.0 already defines `SERVICE_SESSION_CAPABILITY_V4` (src/protocol/service.rs:30 at tag v3.5.0). A 3.5.x daemon therefore registers graph silently. The U2 held-seat rebind then answers NotFound, and graph records it as a definitive `rebind_refused` and shows `clones.restart_held` with no upgrade hint. The behaviour only goes wrong once all three clusters are combined. No single cluster owns the guarantee.

## Requirement baseline trace for the whole tree

- B1 (undo of a closed adopted tab resurrects the original seat): the path runs from the production `undo` CLI through plan/apply and the composed daemon. `ThreadsSource` emits `threads.resurrect_seat`, then `ThreadsExecutor` calls `ServiceThreads::restore` (`ResurrectSeat` with replace), then `AcknowledgeResurrected`, then the unchanged restore launch. Tier-3 adopt tests on a real 3.6.0 daemon check the seat id, the external membership and that no succession happened. Delivered.
- B2 (a Herdr restart is frictionless, with no operator rebind per tab): the path is the composed daemon after a cold restart, with the rebind want, the adapter U2 call, the keyless start gate and resume whenever a session exists. Tier 3 shows every seat kind returning with no operator command. If threads stays down past the grace, the fresh agent enrolls at its next prompt (evidence/hg-lnw.9/late-rebind-pinned-order.log). This is delivered against the required 3.6.0 daemon. On a 3.5.x daemon, which the docs say is refused by name, B2 degrades silently into the operator-rebind friction. That is a documented-guarantee defect, carried as a Must fix item. It is not a baseline miss on the supported daemon.
- B3 (v4 through the upstream level option): `with_level(V4)` comes from the pinned upstream. A refused registration gives a named error and there is no fallback. Delivered.
- B4 (upstream rebind of a restart-held keyless seat before relaunch): `rebind_held_seat` sends `ResurrectSeat` with no replace (ht-0f0r at the pin), and resume is unchanged. Delivered.

## Verdict
not ready (a 3.5.x daemon silently accepts v4 despite the "refused by name" guarantee in the root docs and src error text, and B2's restart rebind degrades on it; the design doc of record is stale; evidence clusters 3-17 are unreviewed)

## Must fix before landing
- Pre-3.6.0 guarantee (cluster 1, seam across root, src and third_party): CHANGELOG.md, README.md, Cargo.toml, CURRENT-DESIGN.md, src/threads/adapter.rs:64-68 and src/daemon/doctor.rs:114 claim that any daemon older than 3.6.0 refuses service_session_v4 by name, but herdr-threads v3.5.0 serves V4, so a 3.5.x daemon registers silently and B2's rebind degrades to rebind_refused and clones.restart_held with no upgrade hint; either correct the claim everywhere or add a real guard (a version check, or mapping the held-seat rebind NotFound to the named upgrade error).
- Design doc of record (cluster 2): append a dated Post-Implementation note to docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md covering: rebind NotFound is definitive and SeatAlreadyResolved is done; the resurrect op key has a refusal count; SeatAlreadyResolved is a resurrect done outcome; the invitation reset runs on every keyless resurrect acknowledgement and skips rejected rows; resumed relaunches wait the full 90 s grace; the remedy is the `human seat rebind ... --operator` form; and the hg-lnw.23 pane-qualified agent-name retry.
- Unreviewed: cluster 3 (evidence-hg-lnw-10): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 4 (evidence-hg-lnw-11): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 5 (evidence-hg-lnw-12): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 6 (evidence-hg-lnw-13): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 7 (evidence-hg-lnw-14): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 8 (evidence-hg-lnw-15): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 9 (evidence-hg-lnw-21): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 10 (evidence-hg-lnw-22): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 11 (evidence-hg-lnw-23): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 12 (evidence-hg-lnw-3): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 13 (evidence-hg-lnw-4): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 14 (evidence-hg-lnw-6): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 15 (evidence-hg-lnw-7): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 16 (evidence-hg-lnw-8): committed test-log evidence only, not dispatched (coordinator: logs, not code).
- Unreviewed: cluster 17 (evidence-hg-lnw-9): committed test-log evidence only, not dispatched (coordinator: logs, not code).

## Untested scope
- No test suite was run at merge head 8f8ec538; every cited lib, tier-3 and tier-5 log comes from a merged task branch.
- No test runs graph against a 3.5.x daemon, the one older daemon that accepts service_session_v4.
- The operator remedy graph prints (clone_status::operator_rebind_remedy) is never fed to real threads; the tier-5 operator_rebind_resurrects_and_places test hand-builds its argv.
- late_rebind_binds_the_fresh_agent_at_its_next_prompt asserts the late order only when the race produces it.
- The composed sweep's 24 h to 7 d window does not check graph's own ordinary invitation reset.
- Tier 5 is skipped in full-suite runs; its evidence comes only from the separate hg-lnw.8 and hg-lnw.22 runs.
- The compose::lazy host-claim failure path before ServiceThreads is built has no test.
- ack_restore with plain Acknowledge for a graph-keyed clone whose threads seat is keyless is not exercised.

## Deferred OK
- restart_held and rebind_wants fire for any Unresolved seat in the incarnation, not only restart-held ones (status noise, for example after UnretireSeat of a keyed seat).
- compose::lazy host-claim failure maps to Rejected (Failed) for every threads mutation; Disconnected would keep them retryable.
- ack_restore invitation reset keys on clone.keyless instead of the fact's keyed flag.
- The doctor resurrect_held test fixture remedy text differs from the production string (cosmetic).
- An undo-restored keyless agent that exits before its first check-in is not relaunched (lingering Restore intent, pinned by a test).
- Force the late-rebind order deterministically, and make the tier-5 remedy test run graph's printed remedy.
- Tier-3 runtime of about 15 min from repeated 90 s grace waits, as the spec accepts.
- interim-status.md is a dated interim record superseded by later evidence; INDEX.md lists the hg-lnw spec as draft.
- An empty /tmp/.git directory, not created by this epic, breaks stop-run-processes temp-root checks; a human should run `rmdir /tmp/.git`.
- Reflink-seeded target/ dirs in the task worktrees should be removed with the worktrees after merge.
