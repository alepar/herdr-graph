# Cluster evidence-hg-lnw-12 review (inv-2)

## What the cluster is

Two added log files from task hg-lnw.12 (integration sweep): evidence/hg-lnw.12/cargo-test.log (1872 lines) and evidence/hg-lnw.12/check-all.log (16 lines). No source changes. Source captured at 985d1eb, base 60e287a, both verified as commits.

## Evidence content

- cargo-test.log: lib unit run reports 1536 passed, 0 failed, 0 ignored. Integration binaries all report ok, including daemon_composed (22), daemon_ipc (13), inspect_loaded (16), integration_sweep (27), test_isolation (17), writer_concurrency (3). The only "ignored" line is the packaging release-build test, ignored by design with its run command stated. Log ends with exit 0.
- check-all.log: cargo check across the workspace, exit 0. One dead-code warning (field `herdr` in src/threads/tests.rs Fx) is test-only and harmless.
- Paths in the logs name the task worktree for hg-lnw.12, consistent with commit e8b6e69 merged as 8f8ec53 into the integration branch.

## Against the acceptance criteria of hg-lnw.12

- cargo test tiers 1-2 green: shown by the log.
- Flow-to-test tracing and the no-unwired-surface sweep are not evidenced by these logs; they belong to the test source and the task's own notes, reviewed in other clusters.
- Rig-leak check is not captured in the logs. The test_isolation binary includes sigkill-with-live-rig helper tests, whose truncated output is expected.

## Requirement baseline

Logs are evidence artifacts with no production behavior; no baseline row is traceable from this cluster. Row delivery is for the code clusters and the seam reviewer.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- Rig-leak check (no /tmp/hg-* processes after the run) is not recorded in the logs; tiers 3 and above (private Herdr) run zero tests in this log.

## Deferred OK
- Test-only dead-code warning for field `herdr` in src/threads/tests.rs Fx.
