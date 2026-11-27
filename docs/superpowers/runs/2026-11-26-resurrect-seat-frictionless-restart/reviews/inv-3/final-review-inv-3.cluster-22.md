# Final review inv-3, cluster 22 (evidence-hg-lnw-9)

Source: base 60e287a, head 11ac415 (both verified as commits). Cluster holds five evidence logs under evidence/hg-lnw.9/ (slices 000170 to 000174) and no production or test source.

## Cluster content

- tier3-run2-summary.log is an early tier-3 run with three failures: late_rebind_binds_the_fresh_agent_at_its_next_prompt, undo_restored_agent_that_exits_is_relaunched_live and private_process_info_is_shell.
- late-rebind-pinned-order.log is a pinned rerun of the late-rebind test: it passes (231.99s) and shows the fresh agent held with TargetUnresolved before the rebind, the graph effects ending in threads.rebind_held_seat, a resolved seat after the rebind and an available, bound seat after one prompt. That is the B2 restart-rebind path exercised against a real threads daemon.
- tier3-final-rebased.log is the final rebased tier-3 run: lib 1533 passed, adopt_private_herdr 8 passed, e2e_private_herdr 42 passed, herdr_client 7 passed (private_process_info_is_shell now green), threads_real 9 passed. One failure remains, test_sources_spawn_only_through_isolated_helpers, from tests/threads_real.rs:50 (raw Command::new, rule R1). Exit 101.
- base-057ad4d-isolation-lint.log shows the same R1 failure at the base commit, so it predates this task. base-private_process_info_is_shell.log shows that test failing once in six runs at base, so it is a pre-existing flake.

## Assessment against the requirement baseline

- B1 and B2 (resurrect on undo, frictionless restart) are production behaviour traced by the source clusters. This cluster supplies tier-3 end-to-end evidence for both: 42 e2e tests green in the final run, including the keyless undo, cold restart and exited-restored-agent scenarios, plus the pinned late-rebind run.
- The one red result in the final run (R1 lint at threads_real.rs:50) was fixed at hg-lnw.22 (commit 4327d3f, spawn through the isolated helper). At head, tests/threads_real.rs builds its command through support::private_herdr::private_command, and evidence/hg-lnw.22 holds the rebased test_isolation and tier-5 logs. The red line here is therefore stale, not open.
- The logs are plain test output with no secrets. They contain private-rig paths under /tmp/hg-* and worktree paths only.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- No single tier-3 run covers the head: the last full tier-3 log in this cluster predates hg-lnw.22, .23, .28 and .29. Later evidence directories cover those changes individually.

## Deferred OK

- private_process_info_is_shell flakes at base (1 of 6 runs); not caused by this epic.
