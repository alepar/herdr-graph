# Final review inv-2, cluster 21 (evidence-hg-lnw-7)

## Scope reviewed

Cluster holds only six evidence logs for hg-lnw.7 under evidence/hg-lnw.7/ (base, check-all, lib, lib2, lib-final, tier5), base 60e287a to head 985d1eb. No production source is in the cluster. The epic has no Requirement baseline section in the bead description, so the baseline paragraph does not apply; acceptance is hg-lnw.7's criteria as evidenced by these logs.

## Evidence read

- base.log: pre-task lib run, 281 tests, 2 failures (daemon::lock::tests::is_held_reports_holder, second_acquire_fails_while_held at src/daemon/lock.rs:91 and :108). This shows the lock failures predate hg-lnw.7 and sit in a file the epic does not touch.
- lib.log: 288 tests, same two lock tests fail; lib2.log: 288 tests, only is_held_reports_holder fails. Failure set varies between runs, consistent with a load-dependent flake rather than a regression.
- lib-final.log: full lib run, 1533 passed, 0 failed, exit 0. This is the green run for the "cargo test green" criterion; it also contains the new v4 level, resurrect and doctor-upgrade-step tests (threads_check_names_the_upgrade_step_for_v4_refusal appears in lib.log and lib2.log).
- check-all.log: cargo check all targets, exit 0, one pre-existing dead_code warning in src/threads/tests.rs (field herdr unused).
- tier5.log: tests/threads_real.rs against a real threads daemon, 4 passed, including production_discovery_reaches_real_threads_daemon and adopt_resolves_the_checked_in_seat, exit 0.

## Assessment

The logs support hg-lnw.7's acceptance: v4 registration and port-outcome mapping tests run green in the final lib run, the real-daemon tier passes, and the red lock tests are shown at base to be pre-existing and unrelated. The flake was already recorded in friction.md and in inv-1 cluster reviews as a separate-bead item. Logs record absolute worktree paths of a sibling task worktree, which is cosmetic. Leak-check output (no /tmp/hg-* processes) is not captured in these logs.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The rig-leak check required by AGENTS.md after test runs is not captured in any hg-lnw.7 log, so "no rig leak" is unevidenced here.
- Whether the v3-daemon refusal path is exercised against a real older daemon is not shown; tier5.log runs only against the 3.6.0 daemon, and the refusal is covered by unit tests only.

## Deferred OK

- daemon::lock::tests::is_held_reports_holder and second_acquire_fails_while_held flake under load, also failing at base (base.log); pre-existing, lock.rs is outside the epic, track as a separate bead.
