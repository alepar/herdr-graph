# Final review inv-3, cluster 6 (evidence-hg-lnw-12)

Source: base 60e287aa, head 11ac415, both verified as commits. Cluster files: evidence/hg-lnw.12/cargo-test.log and evidence/hg-lnw.12/check-all.log (slices 000103, 000104; 000104 is the check-all log). The cluster holds captured logs only, no production code.

## Log content

- cargo-test.log: lib run of 1536 tests plus the integration binaries; the sum of every "test result" line is 1670 passed, 0 failed. It ends with "exit 0". Test names containing the word "failed" are scenario names, not failures.
- check-all.log: cargo check of herdr-threads v3.6.0 and herdr-graph v0.3.0 finishes with "exit 0".
- Both logs carry one warning: `field herdr is never read` at src/threads/tests.rs:245 (struct Fx, test-only dead code). It is harmless and does not fail the build.
- The root spec (hg-lnw) has no Requirement baseline section, so there are no baseline rows to trace here.

## Provenance and staleness

- The logs were produced in the task worktree for hg-lnw.12 (commit e8b6e69). The head reviewed here includes later commits such as hg-lnw.30 and the fix round, so these logs are evidence for the sweep task at that point, not for head. The orchestrator's background sweep at head is the authoritative run.
- The logs embed absolute worktree paths. This is cosmetic.
- This cluster has nothing that composes with other clusters beyond serving as the sweep record; seam tracing belongs to the seam reviewer.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The logs predate the post-hg-lnw.12 commits through 11ac415; passing status at head rests on the integration worktree sweep, not on these files.

## Deferred OK

- The dead-code warning on Fx.herdr in src/threads/tests.rs:245 could be cleaned up in a later pass.
