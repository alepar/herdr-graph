# Cluster evidence-hg-lnw-6 review (inv-2)

Source: base 60e287aa, head 985d1eb6 (both verified with git rev-parse). Scope: six log files under evidence/hg-lnw.6/ (slices 000117-000122). No source files in this cluster; no Requirement baseline section applies to log-only files.

## Evidence read

- build.log and build-all-features.log: both finish the dev profile with exit 0.
- check-all-targets-all-features.log: exit 0, one dead-code warning (`Fx.herdr` never read in src/threads/tests.rs). A warning, not an error.
- test-run1-compile-break.log: a deliberate record of the first run failing with E0004 (ServiceOperation::ResurrectSeat not covered in src/threads/service_tests.rs). This is the expected RED step after the pin bump; the later test.log shows it resolved.
- test.log: 1526 lib tests pass and every `test result` line reports 0 failed; no FAILED or panicked lines; exit 0.
- test-isolation.log: 17 tests pass, exit 0. Interleaved output comes from helper subprocess tests and is not a failure.
- All logs are inert text and contain only worktree paths, no secrets.

## Assessment

The cluster is supporting evidence for the threads 3.6.0 pin bump and the ResurrectSeat match arm. Nothing here changes behavior, and the logs are consistent with each other. The recorded `herdr` dead-code warning is cosmetic and sits outside this cluster's files. Behavioral delivery of the epic rows is for the source clusters and the seam reviewer.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- The unused `Fx.herdr` field warning in src/threads/tests.rs appears in check and test logs; clean up in a later pass.
