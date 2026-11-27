# Final review, cluster evidence-hg-lnw-21

## Scope reviewed

Cluster evidence-hg-lnw-21 holds two committed logs: evidence/hg-lnw.21/cargo-test-lib.log and evidence/hg-lnw.21/cargo-test-lib-rebased.log (slices 000080 and 000081). Base 60e287aa and head 985d1eb6 both resolve as commits. The root spec has no Requirement baseline section, so that rule does not apply.

## Evidence check

- Both logs report 1533 passed, 0 failed, 0 ignored for the lib test target. The pass count matches 1533 "... ok" lines in the committed file.
- The rebased log also compiles tests/threads_real.rs and ends exit=0. The only warning is an unused `herdr` field in src/threads/tests.rs, which is not part of hg-lnw.21.
- Bead hg-lnw.21 asks for the remedy string `herdr-threads human seat rebind SEAT --pane PANE` in status, doctor, fixtures and docs. At the reviewed head src/daemon/clone_status.rs builds it through `operator_rebind_remedy`, and the doctor and reconcile fixtures use the same form. Remaining bare "seat rebind" mentions are prose ("held-seat rebind"), not printed remedies.
- Tier 5 test `operator_rebind_resurrects_and_places` takes the printed remedy from `operator_rebind_remedy`, runs it through the real daemon's human namespace, and asserts the bare form is refused with "require immediate human namespace". That exercises the remedy against threads 3.6.0, not only string equality.
- The printed remedy still carries `--operator`. The bead said to verify flags against the submodule. The tier 5 test runs it successfully against the real daemon, so the flag is accepted. The tier 5 run itself is outside this cluster's logs.
- No leaked `/tmp/hg-*` processes were present at review time (count 0).

## Seams

- Whether the tier 5 test passes at the head is the tier-5 evidence cluster's to judge. These logs cover the lib target only.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- These logs run only `cargo test --lib`; the real-daemon acceptance of the printed remedy is covered by tier 5 test operator_rebind_resurrects_and_places, which this cluster does not log.

## Deferred OK

- The unused `herdr` field warning in src/threads/tests.rs is cosmetic and unrelated to hg-lnw.21.
