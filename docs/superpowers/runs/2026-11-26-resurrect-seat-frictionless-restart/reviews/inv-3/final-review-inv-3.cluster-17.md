# Final review inv-3, cluster 17 (evidence-hg-lnw-3)

## Scope reviewed

Two added evidence logs for hg-lnw.3: evidence/hg-lnw.3/red.log and evidence/hg-lnw.3/green-summary.log, at head 11ac415. No requirement baseline section was found in the digest, so the baseline paragraph does not apply.

## Analysis

- red.log shows the single test reconcile::tests::undo_restored_agent_that_exits_is_relaunched failing with left 0 / right 1 (no relaunch), which matches the hg-lnw.3 symptom: restored seat whose agent exits is not restarted.
- The test exists at head (src/reconcile/tests.rs line 5285). The panic line 5245 in the log predates later edits to that file; this is expected drift, not a defect.
- green-summary.log lists 1450 lib tests passed plus integration binaries, zero FAILED lines, ends with exit=0. The lib count (1450) is consistent with red.log's "1449 filtered out" plus the one test.
- The logs are evidence only; they carry no executable behavior. The fix itself (planner restore branch) lies outside this cluster and is judged by its own cluster.
- I did not run cargo, per instructions; the logs were not re-verified by execution.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- none
