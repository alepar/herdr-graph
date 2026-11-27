# Final review inv-2, cluster 16 (evidence-hg-lnw-28)

Source: base 60e287a, head 985d1eb. Cluster holds seven evidence files under evidence/hg-lnw.28/ and no code.

## Evidence read

- tier5-threads-real.log: 9 of 9 tests pass, including operator_rebind_resurrects_and_places and production_discovery_reaches_real_threads_daemon, exit 0.
- lib-clone-status.log and rebased-lib-clone-status.log: 4 clone_status/plan tests pass, exit 0. One dead-code warning on src/threads/tests.rs Fx.herdr, benign.
- mutation-red-operater.log: a deliberate `--operater` typo makes operator_rebind_resurrects_and_places fail on the real herdr-threads CLI. This shows the test drives the real binary and is sensitive to the rebind invocation.
- rebased-tier5-threads-real.log and integration-tip-production-discovery.log: production_discovery fails ("cached endpoint must not register or dispatch against a fresh node"), 8 of 9 pass. This is the RED state that hg-lnw.29 then fixed. evidence/hg-lnw.29/tier5-threads-real.log shows 9 of 9 green at tree c2b8a10 (source-tree.txt), which sits in the integration history below head.
- rebased-source-tree.txt records tree 6a2a93f, which resolves to a real tree object in the repo.

## Assessment

The files are historical run logs. The failing logs are labeled by name (rebased, integration-tip) and are superseded by the green hg-lnw.29 run, so they do not misstate current state. The cluster contains no production code path, so the requirement baseline is a matter for the code clusters and the seam reviewer.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The newest tier 5 green log (hg-lnw.29) was captured at tree c2b8a10, not at head 985d1eb, which is a merge commit. No evidence in this cluster covers the merged head itself.

## Deferred OK

- The rebased and integration-tip logs for hg-lnw.28 record a failing run (production_discovery) and read as red evidence without a pointer to hg-lnw.29 where it was fixed; a one-line note would help future readers.
