# Final review inv-3, cluster 19 (evidence-hg-lnw-6)

Source: base 60e287aa, head 11ac415. Both commits verified. Cluster is six captured log files under evidence/hg-lnw.6/ (task hg-lnw.6, the herdr-threads 3.1.0 to 3.6.0 pin). No production code or spec behavior lives in this cluster, so the review checks the logs for internal consistency and honesty.

## Evidence read

- build.log and build-all-features.log: both compile herdr-threads v3.6.0 and herdr-graph, finish, exit 0. This matches the pin commit eba9803.
- check-all-targets-all-features.log: exit 0, one dead_code warning (field `herdr` never read, src/threads/tests.rs:245). Warning only.
- test-run1-compile-break.log: a kept record of the first run failing with E0004 (ServiceOperation::ResurrectSeat not covered in src/threads/service_tests.rs). Exit 101. It is correctly named as the pre-fix run and is the expected consequence of the new 3.6.0 enum variant; the following test.log compiles, which shows the fix.
- test.log: 1526 lib tests passed, 0 failed, integration binaries all ok (one binary reports 1 ignored; several binaries report 0 tests, consistent with gated or private-Herdr tiers). Doc-tests present. Ends with a zero exit.
- test-isolation.log: 17 passed, 0 failed, exit 0. The interleaved "running 1 test" lines are output of nested helper runs, not a failure.

## Requirement baseline trace

The epic hg-lnw description carries no `## Requirement baseline` section in the bd record read, and this cluster contains only evidence logs, so no baseline row is traced here. Behavioral delivery of ResurrectSeat and frictionless restart belongs to the source clusters and the seam reviewer.

## Assessment

The logs are consistent with each other and with the commit that pins threads 3.6.0. Absolute paths embedded in the logs name a task worktree under .worktrees, which is cosmetic. No log shows a failing or leaked-rig condition.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Logs record unit and integration tiers only; the 0-test binaries (adopt_private_herdr, e2e_private_herdr and similar) show those private-Herdr tiers did not execute in this capture, so they are not evidenced here.

## Deferred OK

- The dead_code warning on Fx.herdr in src/threads/tests.rs:245 could be cleaned up in a later pass.
