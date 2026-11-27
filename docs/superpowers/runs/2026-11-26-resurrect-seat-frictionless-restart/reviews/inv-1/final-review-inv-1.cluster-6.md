# Final review inv-1, cluster 6 (evidence-hg-lnw-13)

Source: base 60e287a, head 8f8ec53 (both verified as commits). Cluster files: five logs under evidence/hg-lnw.13/. They carry no code; they are the recorded proof for task hg-lnw.13 (restart rebind, keyless relaunch gate bounded by the grace).

## Evidence read

- cargo-test-full.log: full `cargo test` run, exit 0. lib 1490 passed, 0 failed; every integration binary green (daemon_composed 22, integration_sweep 22, test_isolation 17, and the rest). Two threads_real/adopt binaries ran 0 tests (feature or tier gated), which matches the other tasks' logs.
- lib-reconcile.log: reconcile filter, 142 passed, 0 failed.
- lib-threads.log: threads filter, 212 passed, 0 failed.
- red-mutation.log: with the rebind and gate code mutated out, 17 reconcile tests fail (the restart rows, rebind outcome rows, threads-lag, always-erroring, never-emitted, ordinary-restart-refuses-none, undo-then-restart). The tests do detect the removal.
- red-mutation-gate-only.log: with only the start gate mutated, 5 reconcile tests fail (never_emitted, never_unresolved waits full grace, always_erroring, never_observed_keyless, threads_lag). Each acceptance row about the gate is pinned by a test that goes red.

## Requirement baseline trace

- B2 (frictionless restart) and B4 (upstream rebind, graph calls it before relaunch): the logs show the planner emits rebind and gates keyless start in the fake-driven reconcile tests. The real production wiring (daemon through real threads adapter against a v4 service) is outside this cluster and outside these logs; the real-adapter tier (threads_real) ran 0 tests in cargo-test-full.log. That path belongs to the seam reviewer and the src/threads cluster, not undelivered here.
- B1 and B3 (resurrect on undo, v4 session) are not exercised by this task's evidence; other clusters own them.

## Observations

- The mutation logs do not state which mutation was applied; only test names and counts are recorded. The commit message and task text identify it, so it is reproducible but not self-describing.
- The "no rig leak" acceptance item has no artifact in this evidence set; the logs show a clean exit only.
- Both lib logs carry one pre-existing-looking dead_code warning (field `herdr` in src/threads/tests.rs Fx); harmless, belongs to the src cluster.
- Logs were compiled from the task worktree path, not the integration worktree; the later merge commits re-ran other tasks' suites, and the head-level state is covered by hg-lnw.12 and hg-lnw.23 logs.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Real herdr-threads v4 service rebind of a restart-held seat is not covered by this evidence; threads_real ran 0 tests in cargo-test-full.log, so the end-to-end B4 path rests on fake-driven reconcile tests here.
- No recorded rig-leak check for the hg-lnw.13 test run.

## Deferred OK

- Mutation logs would be clearer with the applied mutation named in a header line.
