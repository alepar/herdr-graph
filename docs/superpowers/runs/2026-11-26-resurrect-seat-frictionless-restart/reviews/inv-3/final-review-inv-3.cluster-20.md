# Final review, cluster 20 (evidence-hg-lnw-7)

Source: base 60e287aa, head 11ac415. Cluster holds six captured cargo logs under evidence/hg-lnw.7/ (no code). The epic description carries no Requirement baseline section, so the baseline paragraph does not apply.

## Evidence read

- base.log: run on a pristine base checkout (/tmp/sp-*/hg-lnw.7/base), 281 lib tests, 279 pass. The two daemon::lock tests fail, so the lock failures predate the task.
- lib.log: 288 lib tests, 286 pass. The same two daemon::lock tests fail.
- lib2.log: 288 lib tests, 287 pass. Only is_held_reports_holder fails. The failing set changes between runs with no code change, which marks the tests as flaky.
- check-all.log: cargo check of all targets, exit 0, one dead_code warning in src/threads/tests.rs (field herdr never read).
- lib-final.log: 1533 lib tests, 0 failed, exit 0. Both lock tests pass here.
- tier5.log: tests/threads_real.rs, 4 passed, including production_discovery_reaches_real_threads_daemon and the real invite/membership/notify/release test. This is the real-daemon path for the v4 adapter.

The logs support hg-lnw.7's acceptance line "cargo test green in hg-build" through lib-final.log and tier5.log. The two red runs are explained by the baseline run failing identically. The logs record no unit-test names for the v4 level pin or the error-variant mapping in a way that can be tied to this cluster; those are the adapter cluster's to judge.

## Observations

- The evidence logs are committed artifacts that embed absolute worktree and /tmp paths. They are harmless but machine specific.
- lib-final.log was captured in the task worktree and not at head 11ac415. The cluster cannot show it ran at head; later merges (hg-lnw.30 and others) are covered by other clusters' evidence.
- The flaky daemon::lock tests assert exclusive flock behaviour across open file descriptions inside one process. A parallel test that forks may inherit a descriptor, which fits the intermittent pattern. This is a hypothesis, not verified here, and no cargo was run.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- lib-final.log and tier5.log were captured at the hg-lnw.7 task tip, not at integration head 11ac415, so green at head rests on later tasks' own evidence and the seam review.

## Deferred OK

- daemon::lock::tests::second_acquire_fails_while_held and is_held_reports_holder fail intermittently, also on the base checkout (base.log); file a follow-up to find the cause (likely descriptor inheritance across parallel tests).
- The dead_code warning for the unused Fx::herdr field in src/threads/tests.rs remains in every log.
