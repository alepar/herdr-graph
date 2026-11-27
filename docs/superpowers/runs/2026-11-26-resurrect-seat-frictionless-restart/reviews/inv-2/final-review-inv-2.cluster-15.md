# Final review inv-2, cluster 15 (evidence-hg-lnw-25)

## Scope read

The cluster is four committed logs under evidence/hg-lnw.25/ (commit 77bdd06): cargo-check-all.log, cargo-test-lib.log, red-mutation.log, threads-real.log. No production code is in the cluster. The logs back bead hg-lnw.25 (the 3.6.0 daemon floor is checked, not assumed from v4 registration).

## Evidence assessment

- cargo-check-all.log: exit=0, one dead-code warning (`Fx.herdr` never read, src/threads/tests.rs:245). Harmless.
- cargo-test-lib.log: 1539 passed, 0 failed, 0 ignored, exit=0. It includes `threads::service_tests::discover_refuses_a_3_5_daemon_by_name_and_accepts_3_6 ... ok`, the test that pins acceptance criterion 2 (a fake 3.5.x daemon gets a named upgrade error).
- red-mutation.log: with the floor check mutated out, that test fails (`unwrap_err()` on an `Ok` value, exit=101). So the test discriminates between a daemon floor check and none.
- threads-real.log: the real herdr-threads channel/invite/membership/notify/release test passes against the 3.6.0 vendored daemon, exit=0.
- The logs record absolute paths of a task worktree. They are build transcripts, not secrets.
- The RED log cites src/threads/service_tests.rs:1384, while the head has the test at line 1409. Later fixes (hg-lnw.28/29) shifted it. The logs are point-in-time evidence from the task commit, not from head; this does not affect their validity as RED/GREEN proof.

## Requirement baseline

The cluster adds no production entry point. The floor behaviour it evidences is traced by the production-code reviewers. Nothing here lowers or fails a baseline row.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- The evidence logs were produced at the task commit, not at integration head (RED log line number 1384 vs 1409 at head); the integration-tip evidence from hg-lnw.28/29 supersedes them.
