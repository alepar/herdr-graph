# Final review inv-3, cluster 8 (evidence-hg-lnw-14)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified as commits).

## Scope reviewed

The cluster is four captured cargo logs under evidence/hg-lnw.14/: final.log, focused-1.log, focused-2.log, lib.log. No source or test code is in the cluster. They are evidence for bead hg-lnw.14 (resurrect outcome mapping and clones.resurrect_held status). The epic has no Requirement baseline section, so that paragraph does not apply.

## Reading of the logs

- final.log: full lib test run, 1497 passed, 0 failed. One dead_code warning (field herdr in src/threads/tests.rs Fx), not a failure.
- lib.log: second full lib run, 1497 passed, 0 failed, same warning.
- focused-1.log: 48 resurrect-focused tests, 44 passed, 4 failed. The failures are operator_resurrect_after_refusal_is_acknowledged_and_launches_once, resurrect_not_found_then_final_takes_succession, resurrect_not_found_then_resolved_acknowledges and resurrect_refused_meta_clears_on_ack_restore. These are the bead acceptance tests for the NotFound and operator-resurrect paths. It is a red-then-green record.
- focused-2.log: the same 48 tests, 48 passed, including the four that failed in focused-1. So the failing run precedes the fix and the evidence shows the transition.
- The bead acceptance items (success acknowledges, NotFound then succession or acknowledge, transient retry, persistent refusal recorded once with the launch gated, meta clearing on ack and succession, held row requiring the current epoch, replan under a new op key, SeatAlreadyResolved acknowledges) each have a named passing test in focused-2.log.
- The logs were produced in the task worktree for hg-lnw.14 at its own commit (891902a), not at the integration head. The 1497 count is therefore a historical datum; later integration commits (for example hg-lnw.30) are covered by other clusters and the background sweep.

## Observations

- The logs contain no leaked paths of concern beyond local worktree paths, and no secrets.
- Committing a failing log (focused-1) is honest evidence, but a reader who only greps for "FAILED" will see 4 failures in the tree; the file name is the only signal that it is superseded.
- Nothing in the cluster can fail the product; it holds no executable content.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The logs are from the hg-lnw.14 task commit, not the integration head 11ac415; current-head test status rests on the background sweep, not on this cluster.

## Deferred OK

- focused-1.log records 4 failing tests that were fixed before focused-2.log; consider a note or rename marking it superseded so the evidence directory does not read as red.
