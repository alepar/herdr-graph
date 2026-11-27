# Final review inv-3, cluster 11 (evidence-hg-lnw-22)

Source: base 60e287aa, head 11ac4157 (both verified as commits). The binding step succeeded.

## Scope read

The cluster holds five captured cargo logs under evidence/hg-lnw.22/ and no production code. I read all five slices (000120 to 000124) and the digest header.

## Analysis of the logs

- red-test_isolation.log shows 21 of 22 passing. test_sources_spawn_only_through_isolated_helpers fails on a raw std::process::Command at tests/threads_real.rs:50 (R1). This is a coherent red step for the scanner fix.
- green-test_isolation.log and rebased-test_isolation.log show 22 of 22 passing for the same test binary hash (2ace6a44c5b75759). The red run uses that same hash, so the red and green runs are comparable.
- tier5-threads_real.log and rebased-tier5-threads_real.log show 9 of 9 passing with exit=0. The first run took 189s because the herdr-threads dev build ran inside the tests. The rebased run took 28s on a warm build.
- The tier5 log covers rebind, resurrect before 24h, resurrect between 24h and 7d, resurrect after finalization (not found), production discovery and the channel/invite/notify/release flow. Only the 22 captured test names and results are asserted by the logs, not the semantics behind them.
- The logs record local worktree absolute paths. They are inert.
- The cluster has no baseline row of its own to trace to a production entry point. It is supporting evidence, so any trace belongs to the clusters that own the code under test.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The logs are point-in-time captures from the task worktree (rebased logs included). Nothing in them proves the tests pass at head 11ac4157, and this review did not run cargo as instructed.

## Deferred OK

- The tier5 log embeds about 90 lines of interleaved dependency compile noise. It is cosmetic only.
