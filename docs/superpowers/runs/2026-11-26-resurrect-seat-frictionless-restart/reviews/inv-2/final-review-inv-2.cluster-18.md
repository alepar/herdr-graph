# Final review inv-2, cluster 18 (evidence-hg-lnw-3)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 985d1eb6faaa2e92e4e01f430f0f2c07618bf6ad (both verified as commits).

## Scope reviewed

Two new evidence files for task hg-lnw.3 (relaunch an undo-restored seat whose agent exits with the pane open, spec section 4.4): `evidence/hg-lnw.3/red.log` (slice 000111) and `evidence/hg-lnw.3/green-summary.log` (slice 000110). No source code is in this cluster. The root spec has no Requirement baseline section the cluster could be traced against, and the acceptance criteria of hg-lnw.3 are what the evidence supports.

## Assessment

- red.log shows `reconcile::tests::undo_restored_agent_that_exits_is_relaunched` failing with `left: 0, right: 1` (zero relaunches where one is expected). That is the symptom the task describes, so the RED run is consistent with the bug.
- The test exists at the reviewed head (src/reconcile/tests.rs:5285, helper `undo_restored_exited` just above it). The line number in red.log (5245) differs from head because later merges shifted the file; this is expected for a log captured at the task commit.
- green-summary.log is a cargo test summary: 31 `test result: ok` lines, 0 failed, one test ignored in a single binary, ending `exit=0`. It does not name the target test, but the RED test passing is implied by the whole suite being green.
- The evidence files are inert text; they do not affect production behaviour and carry no secrets or paths of concern.
- The rig-leak check (acceptance item 3) is not recorded in the evidence; this is not an evidence defect for this cluster since the logs are test output.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- green-summary.log lists only result lines, so it does not show that the named RED test is among the passing ones, and no leak-check output (count of `/tmp/hg-*` cwd processes) is recorded for the hg-lnw.3 run.

## Deferred OK

- none
