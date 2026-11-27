# Final review: cluster 8 (evidence-hg-lnw-13)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 985d1eb6faaa2e92e4e01f430f0f2c07618bf6ad (both verified as commits). The epic has no Requirement baseline section, so the spec and task criteria are the standard.

## Scope of the cluster

The cluster holds five log files under evidence/hg-lnw.13/, added in commit a698687 (hg-lnw.13, restart rebind and keyless start gate). No source files are in the cluster. The review therefore judges whether the logs support the task's acceptance criteria.

## What the logs show

- cargo-test-full.log: full lib run 1490 passed, 0 failed; every listed integration binary reports ok (no non-ok "test result" line).
- lib-reconcile.log: 142 reconcile tests passed, 0 failed; lib-threads.log: 212 passed, 0 failed.
- red-mutation.log: with the planner change mutated, 17 reconcile tests fail, covering the section 4 table rows (keyed, keyless, shell seat, session seat, never-observed clone, refusal recording, undo-then-restart).
- red-mutation-gate-only.log: with only the keyless start gate mutated, 5 tests fail, including the grace-bounded cases and the threads-lag case.
- The four tests the task acceptance names (always-erroring rebind at grace, never-emitted rebind, threads-lag stale Resolved then Unresolved, undo-then-restart once) each exist exactly once at head in src/reconcile/tests.rs and appear in the red sets or the green run.
- Logs were built in the hg-lnw.13 task worktree, so they are not from the integration head.

## Evidence currency

Evidence was captured at a698687. Later commits in the range changed src/threads/effects.rs (+104/-16 lines) and src/reconcile/tests.rs (+194 lines) for hg-lnw.21, .23 and .24. These logs do not cover those changes. Later clusters (hg-lnw.28 and .29 evidence at the integration tip) carry the current runs, so this is a currency note, not a gap in this cluster.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The hg-lnw.13 logs predate the hg-lnw.21/.23/.24 changes to src/threads/effects.rs and reconcile tests; current-head coverage rests on the later hg-lnw.28/.29 evidence, which is another cluster's to judge.

## Deferred OK

- none
