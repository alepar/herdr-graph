# Cluster 17 (evidence-hg-lnw-29) review

## Scope and method

The cluster is three evidence files under evidence/hg-lnw.29/: a RED log, a tier 5 GREEN log and source-tree.txt. No production code is in the slice. Base and head commits verified with rev-parse. Slices 000107 to 000109 read in full, and the asserted test (tests/threads_real.rs, around line 590 to 630 at head) read.

## Evidence consistency

- source-tree.txt records commit 80596ca (the test commit) and its tree b4dcaa36; both match git (80596ca^{tree} is b4dcaa36). The evidence is bound to the tested source.
- The RED log fails production_discovery_reaches_real_threads_daemon at the assertion "cached endpoint must not register or dispatch against a fresh node", which is exactly the graph-local Disconnected refusal the test expects. The failure is the intended one, not a compile or harness error.
- The GREEN log shows 9 passed, 0 failed, exit=0, including production_discovery_reaches_real_threads_daemon, the resurrect, rebind and adopt tests. This covers the epic's resurrect and restart-held placement behaviors at tier 5 against a real threads daemon.
- Rig leak check run after review: 0 processes with cwd under /tmp/hg-*.
- Logs are verbose (full dependency compile output interleaved with test lines) but are evidence only.

## Requirement trace

The cluster adds no production behavior; baseline rows are traced by the clusters owning the code. The evidence supports resurrect-on-undo and frictionless restart only as tier 5 test results.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- Evidence logs include raw dependency compile noise and the interleaved FAILED line in the RED log; cosmetic only.
