# Final review inv-3, cluster 5 (evidence-hg-lnw-11)

Source: base 60e287aa, head 11ac415 (both verified as commits). Cluster files: evidence/hg-lnw.11/final.log, related.log, restart-held.log (new files, plain cargo test output).

## Scope of the cluster

The cluster holds only captured test logs for task hg-lnw.11 (restart-held doctor row and reconcile grace behaviour). No production code is in it. The epic description carries no `## Requirement baseline` section, so the baseline paragraph does not apply.

## Consistency of the logs

- final.log: 33 passed, 0 failed, 1463 filtered out. related.log: 411 passed, 0 failed, 1085 filtered out. restart-held.log: 6 passed, 0 failed, 1490 filtered out. Each sums to 1496 tests, so all three runs used the same binary (herdr_graph-af06ac88911be9ca).
- related.log lists 411 "ok" lines, matching the summary. No FAILED or panicked lines in any log.
- The test names the logs cite exist at the reviewed head (reconcile/tests.rs restart_held_row_after_grace_when_rebind_refused and late_rebind_after_grace_places_the_seat; daemon/doctor.rs restart_held_check_names_the_operator_remedy).
- The restart-held cases cover the spec behaviour: held row after grace on refused, errored and never-attempted rebind; no row when the rebind succeeds or the seat resolves by itself; late rebind places the seat; doctor names the operator remedy.

## Observations on the evidence

- Logs were produced in the task worktree path, not the integration head, so they prove the task commit, not the merged head 11ac415. The integration test sweep covers the head.
- The compile warning "field `herdr` is never read" in src/threads/tests.rs:245 appears in every log. It is test-only dead code and does not affect results.
- Logs embed absolute local paths; harmless.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Logs are from the task worktree, not the integration head; head-level verification rests on the separate background sweep.

## Deferred OK

- Test-only dead_code warning on Fx.herdr in src/threads/tests.rs:245 could be cleaned up in a later pass.
