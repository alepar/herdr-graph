# Final review inv-1, cluster 9 (evidence-hg-lnw-21)

## Scope and method

The cluster holds two new log files, evidence/hg-lnw.21/cargo-test-lib.log and evidence/hg-lnw.21/cargo-test-lib-rebased.log. There is no source change in the cluster. The epic hg-lnw has no Requirement baseline section, so the baseline paragraph does not apply.
Base 60e287a and head 8f8ec53 both resolve as commits. The ledger binding (bind-review-source) succeeded.

## Evidence content

- Both logs end with `test result: ok. 1533 passed; 0 failed; 0 ignored` and `exit=0`. Each has 1533 `... ok` lines, matching the count. There is no FAILED or panicked line in either.
- The first log builds herdr-threads v3.6.0 from the submodule in the task worktree. The rebased log reruns the lib suite on the rebased tip with the same count.
- The only warning in each is the known pre-existing `field herdr is never read` in src/threads/tests.rs, recorded in earlier task reports.
- The rebased log has a short tail after the result (a `Finished test profile` line and an `Executable tests/threads_real.rs` line), consistent with a compile-only pass of the integration binary. It is not a failure.

## Claim check against the bead hg-lnw.21

- The bead asks that every printed remedy use the form threads 3.6.0 accepts. At head, src/daemon/clone_status.rs:67 and the doctor fixtures at src/daemon/doctor.rs:993, 1000, 1036 and 1042 print `herdr-threads human seat rebind SEAT --pane PANE --operator`.
- That form matches the submodule's own docs (third_party/herdr-threads/docs/release.md and install.md: `human seat rebind OLD --pane P ... --operator`) and its refusal text in src/cli/actor_route.rs ("person/operator actions require immediate human namespace").
- A search of src, README.md, CURRENT-DESIGN.md, CHANGELOG.md and the onboarding spec at head finds no `seat rebind` without the `human` prefix in the printed remedies. The remaining hits are prose about the rebind operation, not printed commands.
- The remedy-string correctness is pinned only by lib fixtures. The real-daemon acceptance of the printed command was a recorded minor (missing integration test) in progress.md and is a seam question for the integration reviewer, not this cluster.

## Notes on the evidence

- The logs record no rig-leak check output. The lib suite does not start private Herdr servers, so this is low risk, and the bead's criterion is covered by the task report.
- The logs are generated artifacts and carry absolute worktree paths. That is harmless.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The printed remedy command is never executed against a real threads 3.6.0 daemon in this evidence; only string fixtures check it (already a recorded minor on hg-lnw.21).
- The logs do not record a post-run rig-leak check.

## Deferred OK

- Real-daemon tier-5 test that runs the exact remedy string graph prints, recorded as a minor on hg-lnw.21.
