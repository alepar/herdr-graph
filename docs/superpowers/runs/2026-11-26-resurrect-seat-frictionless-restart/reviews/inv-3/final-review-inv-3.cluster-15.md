# Final review inv-3, cluster 15 (evidence-hg-lnw-28)

Source: base 60e287a, head 11ac415 (both verified as commits). The cluster holds seven evidence files under evidence/hg-lnw.28/ and no product code.

## Evidence read

- tier5-threads-real.log: threads_real 9 of 9 pass, exit=0, including operator_rebind_resurrects_and_places (graph's printed rebind remedy run against real herdr-threads 3.6.0) and production_discovery_reaches_real_threads_daemon.
- mutation-red-operater.log: the same test fails when the remedy flag is misspelled (--operater), so the test does exercise the printed remedy. Valid red/green pair for the hg-lnw.28 criterion.
- lib-clone-status.log and rebased-lib-clone-status.log: 4 clone_status tests green before and after the rebase.
- rebased-tier5-threads-real.log and integration-tip-production-discovery.log: production_discovery fails at tests/threads_real.rs:623 (expected Rejected, now a graph-local refusal). This is the seam break caused by hg-lnw.24; commit 80596ca (hg-lnw.29) changed the assertion to the graph-local retryable refusal, and the head test source now matches. These two logs are superseded red evidence, not a defect in this cluster.
- rebased-source-tree.txt: one tree hash, no content to judge.
- Logs embed task-worktree paths and compile noise; harmless.

## Requirement baseline trace

- B2 (frictionless restart) and B4 (rebind of restart-held seats): this cluster's evidence shows the operator remedy path and resurrect windows against the real daemon. Production entry points are traced by the code clusters; nothing in this cluster contradicts delivery.
- B1 and B3: resurrect windows pass in tier 5 against real 3.6.0. Seam tracing belongs to the seam reviewer.

## Notes

- The evidence tree for the final green run (tier5 log) was produced at the task commit, not at the merged head; hg-lnw.29 evidence supersedes it for the tip.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The green tier 5 log in this cluster predates the merged head 11ac415; head-level tier 5 coverage rests on the hg-lnw.29 evidence, which is another cluster's.
- No rig-leak check is recorded in these logs.

## Deferred OK

- The rebased and integration-tip logs record a failing production_discovery run with no pointer to hg-lnw.29 where it was fixed; a one-line note would help readers.
- The unused Fx.herdr field warning in src/threads/tests.rs appears in the lib logs.
