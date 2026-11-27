# Final review inv-3, cluster 7 (evidence-hg-lnw-13)

Source: base 60e287aadd08, head 11ac415754b5. Cluster holds five captured logs under evidence/hg-lnw.13/, added in commit a698687. No production code is in the cluster.

## Evidence read

- cargo-test-full.log: 1490 lib tests, 0 failed, 0 ignored, exit 0. The three doc/bin harnesses run 0 tests and pass.
- lib-reconcile.log: 142 reconcile-filtered tests pass. lib-threads.log: 212 threads-filtered tests pass.
- red-mutation.log: with the production change mutated out, 17 reconcile tests fail. red-mutation-gate-only.log: with only the keyless gate mutated, 5 fail. Both show the new tests bite.
- The tests named in the hg-lnw.13 acceptance (threads-lag, always-erroring rebind, never-emitted rebind, undo-then-restart, shell seat, keyed session seat) appear as ok in the full log and as FAILED in the mutation logs.
- The one compiler warning (unused field `herdr` in src/threads/tests.rs) is test-only and is not a defect of this cluster.

## Observations

- Logs were captured in the task worktree for hg-lnw.13 at its commit, not at integration head 11ac415. Later merges (for example hg-lnw.30) are not covered by them. Evidence value is historical; the integration sweep is the current gate.
- Logs embed absolute worktree paths in the Compiling line. Harmless.
- No baseline row can be traced from a log file; row delivery belongs to the planner/executor clusters.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Logs predate integration head 11ac415; the running integration test sweep is the only evidence at head.

## Deferred OK

- none
