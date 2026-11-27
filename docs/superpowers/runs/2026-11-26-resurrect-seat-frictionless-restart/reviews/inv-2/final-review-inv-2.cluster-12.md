# Final review inv-2, cluster 12 (evidence-hg-lnw-22)

## Scope reviewed

Five log files under evidence/hg-lnw.22 (red, green, rebased test_isolation runs and two tier 5 threads_real runs), against hg-lnw.22 acceptance criteria. The epic description carries no Requirement baseline section, so the task criteria are the standard.

## Evidence reading

- The red log shows the lint failing exactly as the bead describes: tests/threads_real.rs:50 raw std::process::Command::new, 21 passed, 1 failed.
- The green and rebased logs show test_isolation at 22 passed, 0 failed, including test_sources_spawn_only_through_isolated_helpers.
- The tier 5 logs show threads_real at 9 passed, 0 failed, exit=0, including production_discovery_reaches_real_threads_daemon and the resurrect window tests.
- At head 985d1eb, tests/threads_real.rs ThreadsDaemon::command builds through support::private_herdr::private_command, so the fix (4327d3f) is present. The diff of tests/test_isolation.rs over the range only bumps the pinned version assertions 3.1.0 to 3.6.0; the lint rules are not loosened.
- Logs embed task-worktree paths and cargo compile noise; harmless, and they predate later tier 5 changes (hg-lnw.28, hg-lnw.29), which carry their own evidence.
- A process-cwd leak check found 0 processes under /tmp/hg-*; one stale /tmp/hg-* directory exists with no owner process, not attributable to this review.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- none
