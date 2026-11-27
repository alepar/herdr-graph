# Final review inv-2, cluster 13 (evidence-hg-lnw-23)

## Scope read

The cluster is four evidence logs under evidence/hg-lnw.23/, added by commits c95536b and 6bd4575: cargo-test.log, cargo-test-rebased.log, tier3-adopted-unlabelled.log and tier3-adopted-unlabelled-rebased.log. No source code is in the cluster. The root spec for hg-lnw has no `## Requirement baseline` section, so that paragraph does not apply.

## What the logs show

- cargo-test-rebased.log: whole-suite run, exit=0. The lib suite shows 1536 passed, and every other target is ok, including test_isolation. This is the log that stands for the merged head.
- cargo-test.log: the pre-rebase run, exit=101. test_isolation fails on tests/threads_real.rs:50 (raw Command::new). That is the hg-lnw.22 blocker, which was merged before this tree was rebased, and the rebased log shows it cleared. The failing log stays committed beside the passing one; its role is not stated in the log, but the file name pair makes the order clear.
- tier3-adopted-unlabelled.log and the rebased copy: 4 passed, 0 failed, exit=0 for cold_restart_brings_every_seat_kind_back, keyless_cold_restart_without_session_returns_to_its_seat, late_rebind_binds_the_fresh_agent_at_its_next_prompt and undo_restored_agent_that_exits_is_relaunched_live. These are the adopted-pane scenarios that cover the agent_name_taken retry fix.
- The only compiler warning in the logs is the unused field `herdr` in src/threads/tests.rs:245, already noted as a minor in the ledger (build-warning).

## Fit to the epic

The evidence backs the hg-lnw.23 fix (retry agent.start under a pane-qualified name on agent_name_taken), which in turn serves the epic's frictionless-restart requirement for a second unlabelled adopted pane. The logs are raw tool output with an exit marker, and nothing in them looks fabricated or edited. The tier-3 logs run only 4 of the 42 e2e tests (38 filtered), which is the stated scope of the evidence.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- The tier-3 evidence filters 38 of 42 e2e tests; the rest are not exercised in this cluster's logs (the full-suite log covers the non-e2e targets only if e2e is gated or ignored by default).

## Deferred OK
- cargo-test.log (exit=101, pre-rebase isolation failure) is kept next to the passing rebased log; a short note on which one is current would remove ambiguity for a later reader.
- The unused `herdr` field warning in src/threads/tests.rs:245 shows in both cargo logs; already recorded as a minor.
