# Final review, cluster 8 (evidence-hg-lnw-15)

## Scope examined

The cluster is four committed log files under evidence/hg-lnw.15/: lib-all.log (full lib run, 1526 tests, exit 0), and threads-run1/2/3.log (the `threads::` filter, 207 tests). No production code or test source is in the cluster. The code it evidences is bead hg-lnw.15 (invitation reset on resurrect success), reviewed with its own cluster.

## What the logs show

- lib-all.log: 1526 passed, 0 failed.
- threads-run1.log: exit 101, operator_resurrect_ack_carries_the_reset failed at tests.rs:5452 (threads_done false).
- threads-run2.log: exit 101, the same test failed at tests.rs:5456 (invitation minted 1, expected 2).
- threads-run3.log: 207 passed, exit 0.
- The failures differ in line and symptom, which points to a timing-dependent test (a fixed step count in `steps(&fx, 6)`), not a deterministic logic fault. No file in the cluster records the cause or a fix.

## Reproduction at the reviewed head

At 8f8ec538 in the caller checkout (hg-build toolbox): the single test passed 9 of 9 runs, and the full `threads::` filter (213 tests at head) passed 10 of 10 runs. No /tmp/hg-* leak after the runs. The flake seen in runs 1 and 2 did not reproduce, so it is either fixed by later commits or rare under this load. The evidence directory still ships two red logs with no note.

## Requirement baseline

The cluster is evidence only; no baseline row is traced here. Delivery of the reset itself belongs to the code clusters.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- operator_resurrect_ack_carries_the_reset failed in 2 of 3 recorded runs (threads-run1/2) and passed 19 of 19 at head; the cause of the earlier flake is not recorded, so a rare timing dependence in the fixed `steps(&fx, 6)` count cannot be ruled out.

## Deferred OK
- Committed evidence includes two failing logs (threads-run1/2) with no note explaining them; a one-line note or removal would avoid misleading later readers.
