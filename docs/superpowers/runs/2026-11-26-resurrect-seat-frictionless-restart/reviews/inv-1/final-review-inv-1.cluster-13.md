# Final review: cluster 13 (evidence-hg-lnw-4)

Source: base 60e287aa, head 8f8ec538 (both verified). Cluster holds five committed test logs for hg-lnw.4 (undo of a closed keyless seat resurrects the original seat inside the 7 d window). No code. The root spec has no Requirement baseline section, so that paragraph does not apply.

## What the logs show

- full.log: 1482 lib tests pass, plus the integration binaries (daemon_composed 22, others 3-17 each), 0 failed, one ignored in a small binary.
- final-lib-repeat.log: three further lib runs, each 1482 passed, 0 failed.
- undo.log: 62 tests filtered to undo and seat paths, all pass.
- focused.log: 50 tests, 49 pass, 1 fails: undo::tests::restore_preview_names_the_keyless_seat_window saw the window note twice (left 2, right 1). This is a red-phase or mid-development run. The test at the reviewed head asserts exactly one note and the later green runs include it, so the preview duplication was fixed afterwards.
- final-lib.log: despite its name, this run is red: 1481 passed, 1 failed, daemon::lock::tests::is_held_reports_holder (assertion !is_held at src/daemon/lock.rs:108). src/daemon/lock.rs is not touched by the epic diff, and the three repeat runs and full.log pass, so this is an unrelated flake under parallel load. The file name overstates what the log proves.

## Assessment

The green evidence (full.log, final-lib-repeat.log, undo.log) supports the hg-lnw.4 acceptance items for lib tests. The two red logs are committed without annotation; a reader scanning final-lib.log would take a failing run as the final result. That is a documentation wart, not a delivery gap. The logs were produced in the task worktree on the task branch, not at merge head 8f8ec538; the integration-level run is the seam reviewer's and the coordinator's, and is already recorded as an untested item in the previous final review.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The logs come from the task-4 worktree, not from merge head 8f8ec538; no run of this cluster's tests at the merged head is in the cluster.
- The daemon::lock::tests::is_held_reports_holder flake (seen once in final-lib.log) has no root-cause evidence; only the passing repeats argue it is unrelated.

## Deferred OK

- final-lib.log records a red run under a "final" name and focused.log records a red run, both unannotated; note in the evidence or rename so the green runs are the visible result.
- Investigate the is_held_reports_holder flakiness (is_held true after drop under load) as a separate bead; lock.rs is outside this epic.
