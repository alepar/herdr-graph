# Final review, cluster 10 (tests), epic hg-blw

Invocation inv-b9c53a9d-0c9e-4100-99aa-2184b61f229c. Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 2dfa656c7f98715fe66ca53c253de19f369a857d (both verified with `git rev-parse --verify`). The source binding ran before this file was written and exited 0.

## Scope reviewed

These are the cluster's files, from slices 000064 to 000071:

- tests/daemon_composed.rs: `roster.live` is now in the registered-command list. The rest is rustfmt churn, already logged as a deferred minor for hg-blw.6.
- tests/daemon_free_reads.rs: `ops --limit/--unresolved --json` and `roster --live [--unmanaged --json]` now exit 5 when the daemon is down.
- tests/inspect_live.rs: a tier-3 `roster --live` test against a private Herdr.
- tests/inspect_loaded.rs: the loaded record tested through the real setup hook commands.
- tests/inspect_log.rs, tests/inspect_roster.rs, tests/inspect_sessions.rs and tests/inspect_stack.rs: binary-level smoke tests.

The spec is the design doc that the hg-blw epic references (docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md). Its Requirement baseline (B1-B8, S1) and its Unchanged behavior list were used as the acceptance standard.

## Test runs

All runs used a review-owned detached worktree at head (/var/home/alepar/AleCode/herdr-graph/.worktrees/review-inv-b9c53a9d-c10). Its target/ was reflink-seeded from the caller's build. Every run went through `toolbox run -c hg-build`.

- `cargo test --test inspect_loaded --test inspect_log --test inspect_roster --test inspect_sessions --test inspect_stack --test daemon_free_reads`: all passed (4 + 11 + 3 + 3 + 2 + 3).
- `cargo test --features test-support --test inspect_loaded --test daemon_composed`: all passed (24 + 12). This includes `seat_json_through_the_daemon_leaves_the_record_unchanged`.
- `cargo test --features private-herdr --test inspect_live -- --test-threads=1`: 2 passed. No SKIPPED line was printed, so the test really ran against a private Herdr.
- Probe: I temporarily added an eprintln to the review worktree's copy of the seat-json test, then reverted it. `seat --json` exits 0 there, so the test really reaches `seat.resolve` through the daemon. Its assertion (`!= Some(5)`) is still weaker than what actually happens.
- The project leak check (`/tmp/hg-*` cwd count) printed 0 after every run.
- Caller checkout: HEAD stayed at 2dfa656 and `git status --short` stayed empty throughout.

## Coverage against the baseline

These tests drive the real `herdr-graph` binary, and the hook tests use the exact strings that `hook_command` and `codex_hook_command` from setup produce. So the behaviors they cover are reached from production entry points:

- B2 and B5: a real hook injection writes `.graph-local/loaded/<clone>.json`. `roster --json` and `instructions stack` then read:
  - `unknown` before the occupant lands;
  - `current` after it lands;
  - `stale` with `modified` and `path_last_touched` after a rule edit;
  - `current` again after a fresh injection.
  There is no notice and no commit, and the graph repo's `git status` stays clean.
- The tests also cover:
  - a mismatched session id and a null session id;
  - a Codex payload carrying `session_id`;
  - older flagless hooks, which print the same bytes and set the note;
  - `session-report` receiving an identical `capture` from the old and new commands;
  - stdin held open, which must still finish in under 8 s;
  - an unbound caller, which writes nothing;
  - a record-write failure, which changes neither output nor exit code;
  - readers (`seat --instructions`, `roster`, `stack`, and `seat --json` through the daemon) never writing the record.
  Together these pin the Unchanged-behavior items on hook text and on `session-report`.
- B8 and S1: tier 3 covers the following against a private Herdr.
  - `live_pane_matches: true` for the committed binding.
  - `binding_from_earlier_server: false`.
  - `live_unavailable: null`.
  - `unmanaged.panes` listing a stray pane but not the graph's pane.
  - `unmanaged` being absent without `--unmanaged`.
  - The text form showing no unavailability line.
  - No commit added.
  Exit 5 with the daemon down is pinned in daemon_free_reads.
- B1 is covered by roster's `--json` envelope (schema 1, head equals main, `live: false`, no socket created), which inspect_loaded runs against a populated roster.
- B3, B4, B6 and B7 are covered here only by binary smoke tests on an empty instance:
  - `log` lists the init commit with null op and kind, puts the ops pointer on stderr under `--json`, rejects `clone` scope, and its help text covers global and instance scope;
  - `sessions` on an empty instance returns `tombstones: 0`, `total: 0`, a 40-hex head and no socket, exits 1 for an unknown clone and exits 2 on a bad `--since`.
  Their row-level behavior is covered by the inspect-module unit tests, for example:
  - `newest_first_with_tiebreak`, `includes_retired_and_archived` and `scopes` in src/inspect/sessions.rs;
  - `bookkeeping_hidden_unless_all` and `by_is_and_filter` in src/inspect/log.rs;
  - `content_never_opened` in src/inspect/harness_files.rs.
  Tracing those rows from the CLI entry point into the inspect modules belongs to the seam reviewer and the owning clusters, not this one.

## Test quality notes

- Rig hygiene meets AGENTS.md.
  - Every spawned CLI or hook process goes through `TestRoot::command`, or through `PrivateHerdr::command` inside `Rig`.
  - `Rig::drop` calls `reap_root` before `PrivateHerdr` stops the server.
  - The test-support daemon is reaped with the `TestRoot`.
- inspect_loaded runs `git reset --hard` and `git status --porcelain` with a bare `Command::new("git")` that inherits the real HOME and gitconfig. `tests/integration_sweep.rs` already does the same, and there is no global `core.fsmonitor` on this host, so nothing leaks today. It is still a hermeticity gap.
- Some assertions are weaker than the behavior they test.
  - `seat_json_through_the_daemon_leaves_the_record_unchanged` only checks that the exit code is not 5. The probe shows the real code is 0.
  - `readers_never_write_the_record` ignores the readers' exit codes, so a reader that failed early would pass vacuously.
  - The final re-injection in `hook_chain_reaches_current_once_the_occupant_lands` is not checked for success.
  None of these hides a defect at head, because sibling tests show the same commands succeed.
- `codex_hook_records_the_payload_session` lands a Claude-harness occupant against a `hook-codex` record. The status rule compares ids only, so it reads `current`. The test never builds a Codex occupant derived from `agent_session`, and the `codex session id form differs` note is pinned only by the unit test `codex_form_differs_note`.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No binary-level test lists a populated `sessions` or `log` result: newest-first ordering, the `clone <c>` scope on a real clone, `--by`, bookkeeping hidden by default and `--all` are covered only by src/inspect unit tests, and the CLI tests run against an empty init instance.
- Tier-3 `roster --live` asserts only the happy path: `agent {kind,status}`, the `seat_refusals`/`seat_lost` contents (it checks only that `threads_live` is an object), a binding from an earlier Herdr server, and the degraded `live data unavailable: herdr` first line are not exercised against a real Herdr or through the CLI binary (degraded text is already a deferred minor for hg-blw.6).
- No integration test covers a truncated injection end to end (`truncated: true`, a `cut` part, `omitted` labels, a cap-boundary move turning the record stale); only the unit tests in src/inspect/loaded.rs cover it.
- No integration test drives the Codex form-difference note (`codex session id form differs`) with a Codex-harness occupant whose id comes from Herdr `agent_session` or `resume` argv; only the unit test covers it.

## Deferred OK
- tests/inspect_loaded.rs seat_json_through_the_daemon_leaves_the_record_unchanged asserts only exit != 5; the observed exit is 0 and could be pinned as Some(0).
- tests/inspect_loaded.rs readers_never_write_the_record ignores reader exit codes, and hook_chain_reaches_current_once_the_occupant_lands does not check the final re-injection succeeded; both pass vacuously if a command fails early.
- tests/inspect_loaded.rs sync_worktree and git_status spawn git outside TestRoot with the real HOME gitconfig (same precedent as tests/integration_sweep.rs); harmless today, but a global core.fsmonitor would start a daemon outside the rig.
- tests/inspect_loaded.rs codex_hook_records_the_payload_session lands a Claude-harness occupant for a hook-codex record; the status rule is id-only, so this passes, but it is not a realistic Codex occupant.
- tests/daemon_composed.rs rustfmt churn and tests/daemon_free_reads.rs filtered `ops --seat` exiting 1 before the daemon call are already logged deferred minors (hg-blw.6, hg-blw.8).
