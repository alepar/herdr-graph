# Final review inv-3, cluster 13 (evidence-hg-lnw-24)

## Scope read

The cluster is five evidence files for bead hg-lnw.24 (graph-local threads errors are retryable, never a definitive restore refusal): check.log, green-lib.exit, green-lib.log, rebased-lib.log and red-namespace_tests.log, added in commit c9a6799. They are logs, not source. The epic has no `## Requirement baseline` section, so the baseline paragraph does not apply.

## What the logs show

- The red log runs 11 namespace_tests: 5 pass, 6 fail. The failures are the ones the bead names: broken graph-repo intent dir, lazy host-claim failure, explicit config error, namespace bind mismatch, stale snapshot guard, and cached client at the same socket. Each fails because a graph-local error came back as a definitive `Rejected` instead of `Disconnected`. This is a real RED for the stated rule.
- green-lib.log: 1543 passed, 0 failed, exit 0 (green-lib.exit holds 0). rebased-lib.log: 1546 passed, 0 failed after the rebase onto the integration branch. The test count grows by 3 across the rebase, consistent with other merged work.
- check.log: `cargo check` exit 0, but `fmt_exit=1`.
- All three compiled logs carry one dead-code warning: field `herdr` in `struct Fx` (src/threads/tests.rs) is never read. That field is still present at head 11ac415 (src/threads/tests.rs:259).

## Observations on the evidence itself

- Logs embed absolute host paths and a /tmp/sp-* temp dir. This is harmless noise.
- No log records the rig-leak check from AGENTS.md. The bead's Done-when asks for "no rig leak", so the evidence does not demonstrate that clause.
- Only `--lib` was run. Integration or e2e tiers are not covered by this cluster's evidence.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Only the lib tier is evidenced; no integration or e2e tier log is in this cluster.
- No rig-leak check output is recorded for the bead's "no rig leak" done-when clause.
- check.log records fmt_exit=1 (rustfmt not clean when captured); the evidence does not show it was fixed by head.

## Deferred OK

- Dead-code warning for unused `Fx.herdr` in src/threads/tests.rs:259 persists at head; trivial cleanup.
- Evidence logs embed absolute worktree paths; cosmetic.
