# Final review, cluster evidence-hg-lnw-15 (inv-3)

Source: base 60e287a, head 11ac415. Cluster slices 000114 to 000117: four captured log files under evidence/hg-lnw.15/. No source code is in this cluster.

## What the cluster contains

- lib-all.log: cargo test --lib, 1526 passed, 0 failed, exit 0. It lists operator_resurrect_ack_carries_the_reset and operator_resurrect_after_refusal_is_acknowledged_and_launches_once as ok.
- threads-run1.log and threads-run2.log: filtered threads runs, each 206 passed and 1 failed. Both fail operator_resurrect_ack_carries_the_reset (an assertion that the clone is done, then "the settled invitation was re-issued" left 1 right 2). These are the intermediate red iterations of the bead's operator-path test, committed in 7356026 together with the final test.
- threads-run3.log: filtered threads runs, 207 passed, 0 failed, exit 0. This is the green run after the fix.

## Assessment against the hg-lnw.15 criteria

- The evidence supports the operator-path acceptance item (reset carried in the acknowledgement for the operator resurrect) with a red-to-green sequence and a green full lib run at the commit that landed the change.
- The logs are inert artifacts. They are not read by any build or test, so they cannot affect behavior.
- The logs embed the absolute worktree path in compile lines. This is cosmetic.
- threads-run1 shows a compile of herdr-threads v3.1.0, the old pin, because the run predates the 3.6.0 move. That is consistent with the task order and is not a defect in the final tree.
- The two failing logs are not labelled as superseded red runs. A reader who skims the directory sees failures with no note. The commit body does not explain them either.
- No requirement baseline row is traced here: the cluster holds no production path. The reset behavior itself belongs to the source clusters for src/threads/effects.rs, assignment.rs and fake.rs.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The logs show a single lib run at 7356026. They do not show the tier-5 real-threads row (tests/threads_real.rs, 24 h to 7 d resurrect yields a fresh pending invitation) the bead names; that is covered, if at all, by other evidence directories and the seam reviewer.

## Deferred OK

- threads-run1.log and threads-run2.log are red runs kept without a label; a one-line README or rename (for example red-) would stop them reading as an open failure.
