# Final review inv-2, cluster 6 (evidence-hg-lnw-11)

Source: base 60e287aa, head 985d1eb6. Inputs: slices 000062-000064 (evidence/hg-lnw.11/final.log, related.log, restart-held.log), digest, bead hg-lnw.11, design section 4.5.

## Scope and method

The cluster holds three captured cargo test logs from the hg-lnw.11 task worktree. No production code is in it. I read all three slices, checked result lines and failure markers, and spot-checked that the tests the logs name exist at the reviewed head (src/reconcile/tests.rs, src/daemon/doctor.rs) and that the production code they exercise exists (restart_held in src/daemon/clone_status.rs, restart_held_check in src/daemon/doctor.rs, wired into the doctor checks list).

## Log content

- restart-held.log: 6 tests, 6 passed, 0 failed. Names cover the refused, errored and never-attempted rebind cases, the rebind-succeeded and resolved-by-itself no-row cases, and the doctor finding naming the operator remedy. These map one to one onto the acceptance criteria of hg-lnw.11.
- final.log: 33 passed, 0 failed (doctor, clone_status, reconcile restart_held filter), including late_rebind_after_grace_places_the_seat, which covers the clears-on-resolve path.
- related.log: 411 passed, 0 failed; 411 "... ok" lines match the count.
- Each log shows one compiler warning (unused field `herdr` in src/threads/tests.rs Fx). It is cosmetic and sits outside the hg-lnw.11 files.
- The logs record the task worktree path in the Compiling line, which is expected for captured evidence.

## Requirement baseline

The epic has no Requirement baseline section, so the baseline paragraph does not apply. Delivery of the restart_held row and doctor finding is traced in the production-code clusters, not here.

## Limits of this cluster

The logs show unit tests against the fake only. The "no rig leak" acceptance item has no explicit check output in these logs; unit tests here start no private rigs, so the risk is low.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The hg-lnw.11 acceptance item "no rig leak" has no leak-check output in the captured logs; the tests use the fake and start no rigs, so none is expected.

## Deferred OK

- The dead-code warning for the `herdr` field in src/threads/tests.rs appears in every log; trivial cleanup, outside this cluster.
