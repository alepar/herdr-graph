# Final review inv-1, cluster 11 (evidence-hg-lnw-23)

Source: base 60e287aa, head 8f8ec538. Cluster files: four log files under evidence/hg-lnw.23/ (slices 000055-000058). Read-only review.

## What the cluster holds

Four captured command logs for task hg-lnw.23 (agent_name_taken retried under a pane-qualified name): cargo-test.log and cargo-test-rebased.log (full cargo test), tier3-adopted-unlabelled.log and tier3-adopted-unlabelled-rebased.log (private-herdr e2e_private_herdr, 4 tests). No source is in the cluster.

## Requirement baseline trace

The design doc's baseline rows are B1 (resurrect on undo), B2 (frictionless restart), B3 and B4 (upstream fixes for the v4 session and the restart rebind). Human scope changes: none. This cluster touches B2 only, for adopted unlabelled panes. Trace from the production entry point: src/herdr/client.rs start_agent (line ~608) calls agent_names(label, kind, pane), tries the readable name, and retries once under the pane-qualified name when Herdr answers agent_name_taken (line ~626). That is the path reconcile uses to launch an agent, including the fresh start after a Herdr restart. The rest of B2 (rebind, resurrect) is in other clusters and is the seam reviewer's.

## Evidence checks

- cargo-test-rebased.log: lib suite 1536 passed, 0 failed; integration targets all ok; 1665 ok lines, one ignored (the packaging release-build test, ignored by design); no FAILED lines. The reconcile::tests::two_unlabelled_claude_panes_both_start test is present and ok in both logs. That covers acceptance item 1 (reconcile test against a fake that reproduces agent_name_taken; src/herdr/fake.rs applies Herdr's uniqueness rule at lines ~972-983).
- tier3 logs: four e2e tests, 4 passed, exit 0, both before and after the rebase (776.74s and 715.41s). Covers acceptance item 2 on the private rig. The e2e test file at head has no remaining pane-label workaround comment; I did not trace which test starts two unlabelled panes.
- No leak check output is captured in the cluster (acceptance item 3, "no rig leak"); this is a caller-side check.
- The logs record the compile path of the task worktree, nested under the integration worktree. Cosmetic.
- The one compiler warning (unused field herdr in src/threads/tests.rs Fx) appears in the logs; it is test-only dead code outside this cluster.
- The caller checkout shows the observed modified run.md and untracked reviews/ directory; cause not attributed.

## Judgement

The evidence is internally consistent and shows green on the rebased branch. It adds no production behavior, so no defect can live in it.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Rig-leak check after the tier-3 runs is not captured in the logs; the caller's post-run check covers it.
- Which tier-3 test starts agents in two unlabelled panes was not traced; the reconcile unit test covers the two-pane naming case.

## Deferred OK

- Unused field warning in src/threads/tests.rs (Fx.herdr) shown in the cargo logs; test-only, outside this cluster.
