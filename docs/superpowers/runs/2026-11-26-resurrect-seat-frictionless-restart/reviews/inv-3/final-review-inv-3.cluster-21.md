# Final review inv-3, cluster 21 (evidence-hg-lnw-8)

Source: base 60e287aa, head 11ac415 (both verified as commits). Inputs: slices 000167-000169, three new evidence logs under evidence/hg-lnw.8/. The epic description has no Requirement baseline section, so that paragraph does not apply.

## Content reviewed

The cluster is test-run evidence only, no production code.
- callers.log: two runs. test_isolation `threads_binary_with_a_different_version_is_rejected` (1 passed) and adopt_private_herdr (4 passed, includes the adopted-seat daemon-restart test), both exit=0.
- tier5.log and tier5-rebased.log: threads_real, 9 tests passed, exit=0, before and after a rebase. Names cover adopt on the checked-in seat, operator rebind resurrect-and-place, rebind placing a restart-held seat, resurrect before 24h, between 24h and 7d, and after finalization (not found), plus real discovery and channel/membership/notify/release.

## Assessment

The logs are consistent with each other and with the epic goals (resurrect with memberships, restart-held seat placed, pin bump to a threads version that rejects mismatched binaries). The rebased run matches the pre-rebase run, so the rebase did not change outcomes. The logs name the task worktree path in their Compiling lines and carry no commit SHA, so they cannot be tied to the exact head. Interleaved cargo output in the tests is cosmetic. The root_watchdog test passing in each run indicates rig cleanup was checked.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Logs record the task worktree, not integration head 11ac415; later merges (for example hg-lnw.30) are not covered by this evidence.

## Deferred OK

- Evidence logs carry no commit SHA or date header; add one in future evidence captures.
