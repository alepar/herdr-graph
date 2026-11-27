# Final review inv-3, cluster 12 (evidence-hg-lnw-23)

Source: base 60e287aa, head 11ac415. Cluster holds four evidence logs under evidence/hg-lnw.23/ (slices 000125 to 000128). No source code in the cluster. The epic hg-lnw has no Requirement baseline section, so the baseline paragraph does not apply.

## Evidence read

- cargo-test-rebased.log: full run, exit=0. Lib 1536 passed, test_isolation 17 passed, every other test binary ok, no FAILED lines.
- cargo-test.log: the pre-rebase run, exit=101. tests/test_isolation.rs test_sources_spawn_only_through_isolated_helpers failed on a raw Command::new at tests/threads_real.rs:50. Cargo stopped at that binary, so later binaries (threads_real, writer_*) did not run in this log.
- At head, tests/threads_real.rs has no Command::new, and the rebased log passes test_isolation 17 of 17. The failure in the older log is superseded and was fixed by the rebase.
- tier3-adopted-unlabelled.log and tier3-adopted-unlabelled-rebased.log: e2e_private_herdr, 4 tests (cold restart of every seat kind, keyless cold restart returns to its seat, late rebind, undo-restored agent relaunched live), all ok, exit=0, 776s and 715s.

## Assessment

The logs support the hg-lnw.23 claim: the full suite and the four tier-3 restart and adopted-pane scenarios pass on the rebased branch. The retained failing log is honest evidence of the pre-rebase state, but a reader could mistake it for current status. The logs were produced from the task worktree at an earlier commit, not at head 11ac415 (later commits, such as hg-lnw.30, came after), so they are not head-exact evidence. The tier-3 scenarios also cover the restart requirement only through the private-herdr rig, and the logs carry no information about the .30 change.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Evidence logs predate head 11ac415 (the hg-lnw.30 keyless-restore change landed later), so no log here shows the full suite or tier 3 at the reviewed head.

## Deferred OK

- cargo-test.log is a superseded failing run kept beside the passing rebased log; consider labelling it superseded in a follow-up.
