# Final review inv-3, cluster 16 (evidence-hg-lnw-29)

## Scope and method

The cluster is three evidence files under evidence/hg-lnw.29/ (slices 000145 to 000147): a RED log, a source-tree record and a tier 5 log. No production code is in the cluster. The root spec (bd show hg-lnw) has no `## Requirement baseline` section, so the baseline paragraph does not apply.

## Evidence consistency

- source-tree.txt records commit 80596ca (test: tier 5 fresh-node check expects the graph-local retryable refusal) and tree b4dcaa36. Both exist; 80596ca is an ancestor of the reviewed head 11ac415 and its tree hash equals the recorded tree.
- red-production-discovery.log shows the new test production_discovery_reaches_real_threads_daemon failing with the "cached endpoint must not register or dispatch against a fresh node" assertion, a genuine RED before the fix.
- tier5-threads-real.log shows 9 passed, 0 failed, exit=0 for tests/threads_real.rs, including production_discovery_reaches_real_threads_daemon and the resurrect, rebind and adopt tests, against herdr-threads v3.6.0 (submodule pin 7c3c090 at head).
- The logs are raw captures with absolute worktree paths; harmless, and consistent with the other evidence/hg-lnw.* logs.

## Drift after the evidence commit

Between 80596ca and the head, src/reconcile/herdr_exec.rs and src/reconcile/tests.rs changed (hg-lnw.30, keyless restore launch with a lost response). The tier 5 log therefore does not cover the head's source. The change is in the reconcile launch path, which the threads_real rig does not obviously exercise, but the log alone does not prove it.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- tier5-threads-real.log was captured at 80596ca; hg-lnw.30 (cd0e869) later changed src/reconcile/herdr_exec.rs, and no tier 5 re-run at the head is in this cluster. The integration or seam reviewer should confirm a tier 5 run at the final head.

## Deferred OK
- none
