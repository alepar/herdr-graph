# Final review inv-3, cluster 32 (tests)

## Scope and method

Reviewed source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified with `git rev-parse --verify`). The caller checkout's HEAD is 11ac415; its only status entry is the untracked `evidence/sweep/` (a test sweep running in the background, not started by this review). No cargo was run. The review read the cluster's files: tests/adopt_private_herdr.rs, tests/e2e_private_herdr.rs, tests/integration_sweep.rs, tests/test_isolation.rs and tests/threads_real.rs (slices 000200, 000201, 000202, 000207, 000208), the digest, the epic hg-lnw and its design doc (Requirement baseline B1 to B4, section 6 Tests). The in-progress sweep log `evidence/sweep/sweep-11ac415754b5edcedf3790ca4aa050e9fbeabba5.log` was read as observed evidence at this exact head.

## Requirement baseline trace

- B1 (undo of a closed adopted tab brings back the original seat): tier 3 in tests/adopt_private_herdr.rs drives the real graph CLI (`undo` via plan/apply) against real Herdr and the real threads 3.6.0 daemon, and checks seat id, no succession, the replaced bare seat retired and the external membership kept (scenarios 1, 5, 7). Tier 2 (`sweep_keyless_close_then_undo_*`) covers the 24 h and 7 d windows and the preview text through the composed daemon with the fake. Tier 5 covers each window through `production_ports`. The production code path itself belongs to the src clusters and the seam reviewer.
- B2 (frictionless Herdr restart): tests/e2e_private_herdr.rs `cold_restart_brings_every_seat_kind_back` and `keyless_cold_restart_without_session_returns_to_its_seat`, and tests/adopt_private_herdr.rs `never_observed_keyless_clone_starts_after_restart`, restart a real Herdr and assert no `restart_held` row, no `rebind_refused` meta, the original seat on its pane, and resumes or a fresh start with no operator step. Trial finding 3 is pinned by `undo_restored_agent_that_exits_is_relaunched_live`.
- B3 (v4 through the upstream U1 change): tier 5 builds graph's ports through `daemon::compose::production_ports` and gets `Done` from `resurrect_seat`, which needs a v4 session against 3.6.0. tests/test_isolation.rs moves the pinned version to 3.6.0.
- B4 (U2 rebind of a restart-held seat): tier 5 `rebind_places_a_restart_held_seat_on_its_pane` (Done, then AlreadyResolved), and tier 3 observes a `threads.rebind_held_seat` effect `Done` before any relaunched agent checked in.
- The task-21 seam break (tier 5 `production_discovery_reaches_real_threads_daemon` asserting `Rejected`) is fixed at head (80596ca): it now expects `Disconnected` with the `graph-local:` prefix.

## Test quality notes

- Rig teardown: `ThreadsDaemon` now copies the root, socket and env instead of borrowing `PrivateHerdr`, so tests can restart Herdr under it. Every tier-5 test declares the daemon after the Herdr, so it drops first, and Drop still reaps the root. The late-rebind test SIGSTOPs the graph daemon through the agent stub. `E2e::drop` sends SIGCONT and then SIGKILL after an argv check, so a panic while the daemon is frozen does not leak it.
- The sweep at this head shows the lib tier green (1550 passed). Tier 3 `close_restart_then_undo_resurrects_the_seat` passed, and the other tier-3 and tier-5 tests were still running when this review read the log.
- The sweep shows a default-tier failure outside this cluster's files: tests/instruction_writers.rs `std_fs_writers_are_inventoried` fails because src/threads/service_tests.rs (changed in this epic by a4a5da7, 7101406 and 9e1dd41) now contains `std::fs::write` and is not in the inventory. Plain `cargo test` is red at this head.

## Verdict

not ready (default cargo test fails at head: instruction_writers inventory misses src/threads/service_tests.rs)

## Must fix before landing

- Default `cargo test` fails at head: tests/instruction_writers.rs `std_fs_writers_are_inventoried` does not list src/threads/service_tests.rs (new std::fs::write calls from this epic); add it with its reason or route the writes through a listed helper (cause in the src-threads cluster).

## Untested scope

- The late-rebind e2e test passes in either order and does not prove that one prompt binds the agent after a late rebind.
- Real-threads coverage of U2's NotFound, ServiceBusy and InvalidRequest answers is missing.
- Keyless undo after 7 d is tested end to end only with the fake threads.
- Tier 3 and tier 5 passes at head 11ac415 were not yet confirmed by the running sweep.

## Deferred OK

- Fixed sleeps in tier 3 (95 s threads outage, 10 s no-relaunch-loop check) make these tests slow but match the 90 s grace they pin.
