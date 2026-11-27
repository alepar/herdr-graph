# Final review inv-3, cluster 14 (evidence-hg-lnw-25)

Source: base 60e287aa, head 11ac415, both verified as commits. Source binding appended to the ledger (exit 0).

## Scope read

The cluster is four captured logs under evidence/hg-lnw.25/, no source code: cargo-check-all.log, cargo-test-lib.log, red-mutation.log, threads-real.log. Reviewed as evidence for hg-lnw.25 (the 3.6 pin / daemon-name discovery task), not as production behavior.

## Evidence assessment

- cargo-check-all.log: exit=0, both herdr-threads v3.6.0 and herdr-graph compile; one dead_code warning for the `herdr` field of `Fx` in src/threads/tests.rs.
- cargo-test-lib.log: 1539 passed, 0 failed, 0 ignored, exit=0. The count and result line are consistent with the listing.
- red-mutation.log: the single test discover_refuses_a_3_5_daemon_by_name_and_accepts_3_6 fails under mutation with unwrap_err on an Ok(Discovered{...}) value, exit=101. This shows the test bites. The panic line (service_tests.rs:1384) differs from the head position of the test (line 1409) because later commits moved it; this is expected drift, not a defect.
- threads-real.log: tests/threads_real.rs real_threads_channel_invite_membership_notify_release passes against the real 3.6.0 herdr-threads, exit=0, 77s.
- The logs embed absolute task-worktree paths; harmless.
- The logs are captured at the task commit, not re-run at head 11ac415; no cargo was run in this review per instruction.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- The dead_code warning on `Fx.herdr` in src/threads/tests.rs (visible in the logs) is cosmetic; remove the field or use it in a later cleanup.
