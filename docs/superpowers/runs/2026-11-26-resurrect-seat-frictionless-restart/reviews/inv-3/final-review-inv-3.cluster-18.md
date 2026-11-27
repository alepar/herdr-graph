# Final review inv-3, cluster 18 (evidence-hg-lnw-4)

Source: base 60e287a, head 11ac415, both verified as commits. The cluster is five log files under evidence/hg-lnw.4/, all added in commit 8fc8386 (hg-lnw.4, undo of a closed keyless seat resurrects the original seat). They carry no production code, so no baseline row can be undelivered here. Behavior is traced by the code clusters.

## What the logs show

- full.log: the full lib run, 1482 passed, 0 failed, followed by every integration binary green.
- final-lib.log: a second lib run with 1481 passed and 1 failed, daemon::lock::tests::is_held_reports_holder (assertion at src/daemon/lock.rs:108, the post-drop `!is_held`).
- final-lib-repeat.log: three further lib runs, each 1482 passed, 0 failed.
- undo.log: the undo filter, 62 passed, 0 failed.
- focused.log: a 50-test focused run with 1 failure, undo::tests::restore_preview_names_the_keyless_seat_window (two preview lines where one was expected). This is a pre-fix intermediate run. undo.log and full.log both show that test passing afterwards.

## Assessment

The failing lock test is not touched by this branch. src/daemon/lock.rs last changed in f9ce9e2 (rustfmt), long before hg-lnw. It passes in full.log and in all three repeats, so it reads as a pre-existing flake in a test that releases a flock and immediately probes it, not a regression. The committed evidence is therefore mixed: one red lib log is committed next to green ones. The repeat log is the supporting record that the red run was transient.

The logs embed absolute worktree paths in the Compiling line and a dead_code warning for Fx.herdr in src/threads/tests.rs. Both are cosmetic.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- daemon::lock::tests::is_held_reports_holder failed once in evidence/hg-lnw.4/final-lib.log (src/daemon/lock.rs:108) and passed in 4 other lib runs. It is unrelated to this epic's diff. Track it as a separate flaky-test follow-up.
- evidence/hg-lnw.4/focused.log and final-lib.log commit red runs as evidence. Consider keeping only the final green logs or annotating the failures.
