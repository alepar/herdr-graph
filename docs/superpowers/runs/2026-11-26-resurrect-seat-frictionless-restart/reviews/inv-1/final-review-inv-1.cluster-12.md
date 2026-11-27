# Final review, cluster evidence-hg-lnw-3 (invocation inv-1)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 8f8ec538ca1f1e4d8d897804aae726472f753ef9 (both verified as commits). Cluster files: evidence/hg-lnw.3/red.log and evidence/hg-lnw.3/green-summary.log (slices 000059, 000060). The epic has no "Requirement baseline" section, so the spec and task criteria are the standard.

## Evidence read

- red.log: one filtered run of reconcile::tests::undo_restored_agent_that_exits_is_relaunched fails with "left: 0, right: 1" (no start call where one was expected), 1449 other tests filtered out. That is the failing state hg-lnw.3 describes (an undo-restored seat whose agent exited with the pane open is not relaunched).
- green-summary.log: 31 "test result" lines, all ok, ending in exit=0. The lib suite count is 1450, which equals the 1449 filtered plus the one red test, so red and green cover the same suite.
- The named test exists at head (src/reconcile/tests.rs line 5285; the red log's line 5245 predates later commits that shifted it). Its helper undo_restored_exited asserts exactly one restore launch before the agent exits, matching the criterion "a restore epoch still launches at most once via the restore path".
- The cluster is evidence logs only, with no production code. The fix itself lives in src/reconcile and is the src cluster's to judge.

## Observations

- green-summary.log carries only result lines, no test names, so it alone cannot show that the named test ran in the green run. The 1450 count makes it consistent with the red run's 1449+1.
- The task report records a narrow race (agent exits before ClearRestore lands, so the launched Restore intent still blocks a relaunch), pinned as current behavior by a test. That is design scope for the src cluster.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- green-summary.log lists no test names, so the green run of the named test is inferred from the 1450 count rather than shown directly.

## Deferred OK

- Lingering Restore intent when the agent exits before ClearRestore lands is not relaunched (narrow race, pinned by a test, recorded in the hg-lnw.3 report).
