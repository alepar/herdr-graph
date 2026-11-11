# Final review: cluster 10 (tests), epic hg-blw

Invocation inv-4b650ab2-a313-4bcb-9196-17e40854f3ab. Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 6eb6cec786cde8ca070fbf9aed46bb0609ff63ac (both verified with `git rev-parse --verify`). The review source was bound in the ledger before this file was written (bind-review-source exit 0). Caller checkout observed at HEAD 6eb6cec, clean working tree, before and during the review.

## Scope reviewed

- tests/daemon_composed.rs (slice 000043): `roster.live` added to the registered-command list, plus rustfmt re-wrapping.
- tests/daemon_free_reads.rs (slice 000044): exit-5 cases for `ops --limit/--json`, `ops --unresolved --json`, `roster --live`, `roster --live --unmanaged --json`.
- tests/inspect_live.rs (slice 000045): tier-3 `roster --live --unmanaged` against a private Herdr server and the real daemon.
- tests/inspect_log.rs, tests/inspect_roster.rs, tests/inspect_sessions.rs (slices 000046-000048): daemon-free binary smoke tests under `TestRoot`.

Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md at head, including its Requirement baseline (B1-B8, S1).

## Verification method

The `hg-build` toolbox is unusable on this host (`toolbox run -c hg-build true` fails: "unable to find user alepar: no matching entries in passwd file", exit 255), the same failure task 9 reported. No test was run; nothing was started, so no rig can have leaked. Everything below comes from reading the code at head. Where a claim depends on runtime behavior, I traced it through the source at head.

## Test hygiene

- inspect_log, inspect_roster and inspect_sessions build every child through `TestRoot::command` and start no daemon. `TestRoot::drop` asks any daemon to shut down, reaps the root and removes it. They meet the AGENTS.md rule.
- inspect_live: `Rig` owns a `PrivateHerdr`, and `Drop for Rig` runs `reap_root(&herdr.root)` before the `PrivateHerdr` field drops and stops the server. This happens on success, failure and panic, including a panic inside `Rig::start` after `rig` is bound. That meets the rule. It only signals the detached daemon; it does not send a graceful `shutdown` call as `TestRoot` does. That is acceptable, since `reap_root` escalates to SIGKILL.

## Baseline row trace (tests cluster view)

- B1 (active seats): daemon-free `roster --json` / `roster global --json` are covered at binary level, and scope, `--all` and the manager are covered by unit tests in src/inspect/roster.rs.
- B3 (latest modifications, global/space/seat): binary tests cover instance and `global` only. Teamspace and seat scoping, `--by`, `--since/--limit` and `stopped_early` are covered by unit tests in src/inspect/log.rs (scope_by_path, scope_by_affected_object, by_is_and_filter, since_and_limit, max_scan_report).
- B4 (sessions by space/seat/clone): the binary covers the instance and an unknown clone. Scopes, ordering and tombstones are covered by unit tests in src/inspect/sessions.rs.
- B6 (bookkeeping hidden): unit test bookkeeping_hidden_unless_all. There is no binary-level check.
- B8 / S1 (`roster --live`, `--unmanaged` enumerates only): covered by tier-3 inspect_live, the exit-5 cases and roster_live unit tests. The tier-3 test has a wrong threads assertion (below).
- B2, B5, B7: no test in this cluster exercises them, and at head there is no producer to test. src/cli/instructions.rs has only the `Show` action (no `stack`), nothing in src writes `.graph-local/loaded/`, and `InstructionsView` is always `unknown()` or `error()`. Beads hg-blw.2 (instructions stack, open with a review bead hg-blw.19) and hg-blw.12 (loaded record writer, task 9 BLOCKED) are not merged into the branch. The production path for these rows is outside this cluster, so the seam reviewer has to trace delivery. From the tests side they are fully untested.

## Analysis of inspect_live threads assertion

The rig is `PrivateHerdr::start()` only, and no herdr-threads daemon runs in it. tests/e2e_private_herdr.rs:405 says so for the same fixture ("no herdr-threads service runs here"). In production, `Endpoint::locate` (src/daemon/compose.rs:743-757) maps "state directory not found" and "not running" to `ThreadsError::Disconnected`. `LazyMap::inspect` (compose.rs, ThreadsSeatView impl) records only `Rejected | Unsupported` into `seat_view_error` and leaves it `None` for `Disconnected`. So `roster.live` returns `ThreadsRead::Ok` with empty refusals and lost lists. Spec §2.2 requires the opposite when the threads seat view is not running: `seat_refusals`/`seat_lost` null, with `live_unavailable.threads` naming the reason. The test asserts `live_unavailable == null` and `threads_live.is_object()` in exactly that state. It therefore pins the spec violation as correct behavior, and it would fail once the production path is fixed. The production fix belongs to the src-daemon cluster and the seam reviewer. The wrong assertion belongs to this cluster.

## Analysis of the red lib test

`cli::tests::inspect_stubs_not_implemented` (src/cli/mod.rs:219, in the src-cli cluster's file but part of the test suite) runs `sessions` and asserts the error contains "not implemented". `sessions` has been implemented since 8befb82. Merges kept `sessions` in the stub list while dropping `roster --live` and `log`: the list is `[roster, sessions, log]` at 6e829ab and `[sessions]` from a9dec8a to head. The test also runs the real command outside any `TestRoot`, so it reads the developer's real config and instance. Tasks 4 and 6 reported it failing (isolation tripwire on real ~/.config). At head, `cargo test --lib` cannot be green. The next step is either `Ok` (so `unwrap_err` panics) or a real-environment error that does not mention "not implemented".

## Findings

- tests/inspect_live.rs `roster_live_matches_the_graph_pane_and_lists_only_unbound_objects` asserts `live_unavailable == null` and `threads_live` is an object in a rig with no herdr-threads service. This contradicts spec §2.2 degraded state (B8). The test passes because of the production gap in src/daemon/compose.rs (Disconnected is never surfaced), not because the threads read worked.
- src/cli/mod.rs:219 `inspect_stubs_not_implemented` still lists the implemented `sessions` and runs it against the real user environment, so the lib test suite is red at head and the test breaks isolation.
- tests/inspect_live.rs compares `commit_count()` across `roster --live` calls while the daemon's reconciler and observer run in the background. Any background commit in that window (an observed runtime fact, bookkeeping) makes the "adds no commit" assertion fail falsely. Plausible flake, not reproduced. The unit test `one_call_takes_one_snapshot_and_adds_no_commit` already pins the property deterministically.
- tests/inspect_log.rs `log_global_and_clone_scopes` checks only the trailing ops pointer for `log global`. It does not assert that the init commit, which touches graph.toml, is listed under global scope, so a global scope that matched nothing would still pass.
- tests/inspect_sessions.rs pins `sessions --since nonsense` to exit 2 (clap's parse error), while `roster clone x` and `log clone x` usage errors exit 1. README lists exit 2 as "no instance configured, or not inside a Herdr pane". Usage-error exit codes stay inconsistent across the new views. This predates the epic as a clap default but is now encoded in a test.
- tests/daemon_composed.rs carries rustfmt-only re-wrapping unrelated to the change (already a deferred minor of task 7).

## Verdict
not ready (tier-3 roster --live test asserts live_unavailable null with no threads service, pinning a §2.2/B8 violation; lib suite red at head from stale inspect_stubs_not_implemented)

## Must fix before landing
- tests/inspect_live.rs: the rig runs no herdr-threads service, yet the test asserts `live_unavailable == null` and `threads_live` is an object. Spec §2.2 (baseline row B8) requires `live_unavailable.threads` and null `seat_refusals`/`seat_lost` in that state. Assert that degraded form, or run a real threads daemon, as in the threads smoke rig in tests/e2e_private_herdr.rs, before asserting null. The matching production gap (Disconnected never reaches `seat_view_error`, src/daemon/compose.rs) is the src-daemon/seam reviewer's to trace.
- src/cli/mod.rs:219 `cli::tests::inspect_stubs_not_implemented` still runs the implemented `sessions` outside `TestRoot` and expects "not implemented", so `cargo test --lib` is red at head and the test reads the real user environment. Drop the case, or the whole test now that no stub remains. (The file belongs to the src-cli cluster; it is raised here because it is the suite's failure.)

## Untested scope
- B2/B5/B7 (`instructions stack`, loaded-record staleness, harness-native files): no test anywhere at head, and no production path either. There is no `stack` action in src/cli/instructions.rs and no `.graph-local/loaded/` writer, and hg-blw.2 and hg-blw.12 are unmerged. Delivery is the seam reviewer's to trace.
- No test of the suite at the reviewed head was run: the hg-build toolbox fails ("unable to find user alepar"). Pass/fail of every test in this cluster is unverified.
- `roster --live` degraded Herdr state (`live_unavailable.herdr`, first text line `live data unavailable: herdr: ...`) is not exercised through the CLI binary. Only the overlay and in-process handler tests cover it.
- `binding_from_earlier_server: true` / `live_pane_matches: false` after a Herdr restart is unit-only. Nothing at tier 3 restarts the private server (`PrivateHerdr::restart` exists).
- The per-clone `live.agent {kind, status}` is not asserted at tier 3.
- `log` teamspace/seat scopes, `--by`, `--all` (B6 bookkeeping filter) and `stopped_early` are unit-only. There is no binary-level test with seat-bearing history.
- `roster` daemon-free with seats (`last_change`, `scan` envelope, `--all`, seat/teamspace scope) has no binary test. The binary test uses an empty init instance only.
- `sessions` scoped by teamspace/seat/existing clone, and the `showing N of total` footer, are not tested through the binary.
- `ops --seat/--teamspace` with a known seat and the daemon down (should exit 5) is not tested. The fixture has no seat (already a deferred minor of task 5).

## Deferred OK
- tests/inspect_live.rs: the commit-count "adds no commit" check can race background daemon commits. This is a plausible flake; the unit test pins the property.
- tests/inspect_log.rs: `log global` does not assert that the init commit row is present under global scope.
- tests/inspect_sessions.rs: exit 2 for `--since nonsense` vs exit 1 for other usage errors. The exit-code inconsistency is now encoded in a test.
- tests/inspect_live.rs: `Rig` teardown relies on `reap_root` signals rather than a graceful daemon `shutdown` call. This is safe, since it escalates to SIGKILL.
- tests/daemon_composed.rs: rustfmt-only re-wrapping mixed into the change.
