# Final review inv-3, cluster 10 (evidence-hg-lnw-21)

## Scope and method

The cluster is two committed log files under evidence/hg-lnw.21/ (cargo-test-lib.log, cargo-test-lib-rebased.log), slices 000118 and 000119. There is no source change in it. I checked them against hg-lnw.21's acceptance criteria (remedy strings use the threads 3.6.0 form, fixtures and docs updated, cargo test green in hg-build, no rig leak). The review ran no cargo, as instructed.

## Observations

Both logs report "running 1533 tests" with 1533 lines ending " ... ok" and no failure lines. The first log ends "test result: ok. 1533 passed; 0 failed; 0 ignored" and "exit=0". The rebased log ends with exit=0 after compiling the integration tests; its tail shows only the executable line for threads_real, not a result summary for the integration binaries.

Both logs carry one compiler warning, a dead-code warning for the field `herdr` in struct Fx at src/threads/tests.rs:245. At the reviewed head the field is at line 259, so the line number in the logs is stale, but the warning itself remains.

The logs record no rig-leak check output. The no-leak criterion is not evidenced by this cluster.

Spot check at head 11ac415: a git grep for "seat rebind" over src, README.md, CURRENT-DESIGN.md and CHANGELOG.md finds only prose mentions of "the held-seat rebind" and no printed remedy in the old `herdr-threads seat rebind ... --operator` form. The code-side acceptance criterion holds as far as this check reaches. The remedy-string content itself belongs to the code clusters, not this one.

The logs embed task-worktree absolute paths and are provenance only; they are not a requirement-baseline delivery path.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The logs show no post-run check for leaked test rigs, so that hg-lnw.21 criterion is unevidenced here.
- The rebased log ends before any integration-binary results (threads_real and others); only the lib tier is shown passing.

## Deferred OK

- The dead-code warning on Fx.herdr in src/threads/tests.rs (line 259 at head) is cosmetic; clean it up in a later pass.
