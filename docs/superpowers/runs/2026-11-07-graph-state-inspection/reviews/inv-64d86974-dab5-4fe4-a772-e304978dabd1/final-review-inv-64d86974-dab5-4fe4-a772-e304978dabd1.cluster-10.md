# Final review, cluster 10 (tests), invocation inv-64d86974-dab5-4fe4-a772-e304978dabd1

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298 (both verified with `git rev-parse --verify`). The caller checkout stayed at b9caa7f with a clean status throughout.

Files reviewed: tests/daemon_composed.rs, tests/daemon_free_reads.rs, tests/inspect_live.rs, tests/inspect_loaded.rs, tests/inspect_log.rs, tests/inspect_roster.rs, tests/inspect_sessions.rs, tests/inspect_stack.rs, tests/inspect_sweep.rs (slices 000082-000090). Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md, which has a Requirement baseline (B1-B8, S1).

## Test runs at head

The runs used a review-owned detached worktree, /var/home/alepar/AleCode/herdr-graph/.worktrees/review-c10-inv64d8, at b9caa7f. Its target was reflinked from the task-hg-blw.11 worktree, whose tree is identical to head (ba17a28). Everything ran in the `hg-build` toolbox:

- `cargo test --test inspect_loaded --test inspect_log --test inspect_roster --test inspect_sessions --test inspect_stack --test daemon_free_reads`: all pass (11 + 3 + 3 + 2 + 3 + 4).
- `cargo test --features test-support --test inspect_loaded`: 12 pass, including `seat_json_through_the_daemon_leaves_the_record_unchanged`.
- `cargo test --features private-herdr --test inspect_live --test inspect_sweep -- --test-threads=1`: all pass (inspect_live 2, inspect_sweep 5, 13.5 s).
- Leak check after every run (`readlink /proc/*/cwd | grep -c '^/tmp/hg-'`): 0.

## Baseline rows traced through this cluster

- B1 (which seats are active) and B8 (`roster --live`): `inspect_roster.rs` runs the daemon-free binary. Tier 3 runs the real daemon: `inspect_live.rs` and `inspect_sweep.rs` match the committed binding against a real Herdr snapshot. The R14 test waits for `live.agent` to become non-null.
- S1 (`--unmanaged` only enumerates): inspect_live checks that a stray workspace and pane are listed and the graph pane is not, and that the section is absent without the flag. The sweep checks that `--unmanaged` without `--live` is refused.
- B2 and B5 (loaded files and staleness, shown not notified): `r14_chain_through_the_setup_hook` reads the hook command back from the settings that `setup claude` wrote and runs it through `sh -c` with a Claude payload. `session-report` reaches the real daemon, the occupant lands, and both roster and stack read `current`. A committed seat AGENTS.md edit then makes them `stale`, with that commit as `path_last_touched`, and re-injection makes them `current` again. This is a production entry point end to end, as the spec's phase-2 test plan asks. The tier-1 `inspect_loaded.rs` covers the `unknown` cases (mismatched id, null id, Codex null id, older hook without the flag). It also checks that readers never write the record, that a write failure changes neither output nor exit code, and that the hook text is byte-identical with and without `--stdin-payload`.
- B3 (activity scoped global/space/seat): the sweep runs `log`, `log seat foreman`, `--by`, `--limit`, `--since` and `--max-scan` against a real history, with renames and one seat-requested change.
- B4 (sessions by space/seat/clone, newest first): the binary is reached with `--limit`, `--since` and `clone <c>`. Only keys and counts are asserted (see the untested scope list).
- B6 (bookkeeping hidden by default): the sweep shows zero bookkeeping rows by default and a non-zero count under `--all`.
- B7 (harness files: presence only): the sweep asserts only the stack keys `harness_files`, `harness_files_note` and `imports_followed: false`. Discovery and the never-read property are proved by src unit tests (the inspect cluster).

## Threads-degraded overlay

tests/inspect_live.rs (`assert_eq!(v["live_unavailable"], json!(null))` and `assert!(v["threads_live"].is_object())`) and tests/inspect_sweep.rs `spec_keys_and_flags_reach_their_views` (`assert_eq!(live["live_unavailable"], Value::Null)`) both run against a `PrivateHerdr` rig that starts no herdr-threads service. Neither file uses `support::threads_bin`, and the private HOME has no threads plugin. In that state the daemon's seat read fails in `LazyMap::locate` with `ThreadsError::Disconnected` (src/daemon/compose.rs:743-756). The slot update at compose.rs:1113-1118 records only Rejected/Unsupported, so `roster.live` (src/daemon/roster_live.rs:64-82) reports `ThreadsRead::Ok` with empty refusals and lost. Spec §2.2 says the opposite for this state: `seat_refusals`/`seat_lost` must be null, and `live_unavailable.threads` must name the reason. The two tier-3 assertions therefore lock in the spec violation. The production gap was already raised by the previous seam review (inv-b9c53a9d, cluster 3), and nothing in the range after 2dfa656 touches it. It belongs to the daemon cluster and the seam reviewer. The test side is in this cluster: once the gap is fixed, these assertions will fail, and as written they certify the wrong behavior as passing.

## Notes on assertion strength

- `read_only_every_view_and_json_contract` compares `git status --porcelain --ignored` before and after each view to cover `.graph-local`. With the default untracked mode, an ignored directory collapses to one `!! .graph-local/` line, so a view that created a new file under `.graph-local/loaded/` would not change that output. The read-only property for the loaded record is still proved by `readers_never_write_the_record` (tier 1). The Git facts (HEAD, count, index, tree) are compared exactly.
- `r14_chain_through_the_setup_hook` does not assert that the clone has no occupant before the hook runs, which is the spec's stated starting point. It does assert that the occupant's `native_session_id` becomes the payload's id before it checks `current`.
- `every_component_reports_the_one_pinned_head` requires at least two distinct heads across 25 reads with a concurrent writer. This proves the writer really interleaved, and each document is checked against the tree its own `head` names.

## Verdict
not ready (tier-3 roster --live tests assert live_unavailable null in a rig with no threads service, locking in the spec §2.2 threads-degraded violation)

## Must fix before landing

- tests/inspect_live.rs and tests/inspect_sweep.rs (`spec_keys_and_flags_reach_their_views`) assert `live_unavailable == null` and an object-valued `threads_live` in a rig that starts no herdr-threads service. Spec §2.2 (the `roster --live` degraded state under B8) requires null `seat_refusals`/`seat_lost` and `live_unavailable.threads` naming the reason there. Either assert that degraded form or start a real threads service (as tests/e2e_private_herdr.rs does through `support::threads_bin`) before asserting null. The matching production gap (Disconnected never reaches the slot, src/daemon/compose.rs:1113-1118, src/daemon/roster_live.rs:64-82) is the daemon cluster's and the seam reviewer's to trace.

## Untested scope

- `roster --live` with the threads service unreachable (Disconnected) is not exercised at any tier. The only threads-degraded test writes `seat_view_error` by hand.
- Tier-3 `roster --live` asserts no `agent {kind,status}` values (R14 waits only for non-null), no earlier-server binding (`PrivateHerdr` restart), and no degraded `live data unavailable: herdr` first text line through the binary.
- No binary-level test asserts populated `sessions` rows: newest-first order, timestamps, teamspace/seat scope, `clone <c>` rows, the transcript column, or the `showing N of total` footer. The sweep checks only keys, `--limit` and `--since`.
- No integration test drives a truncated injection end to end: a `cut` part, `omitted` labels, `truncated: true`, or a cap-boundary move turning the record stale.
- The installed Claude `hook_command` (`p=$(cat); ...`) is never run with a held-open stdin. `held_open_stdin_still_prints_and_exits` runs only the bare binary with `--stdin-payload`.
- The Codex `codex session id form differs` note is not exercised by an integration test with a Codex-harness occupant whose id came from Herdr `agent_session` or `resume` argv. `codex_hook_records_the_payload_session` lands a Claude-harness occupant.
- No integration test checks harness-file discovery content (paths, `origin`, `harnesses`, `note`, `env: inspector`) in a seat cwd that holds CLAUDE.md or AGENTS.md files. The sweep checks keys only.
- B5 (no reload notice on staleness) has no test asserting that a stale record produces no threads notice or effect.
- `ops --seat/--teamspace --json` against a live daemon with requester-attributed ops checks only the `ops` key, not the filtering.

## Deferred OK

- tests/inspect_sweep.rs `read_only_every_view_and_json_contract`: the `git status --porcelain --ignored` check collapses `.graph-local/` to one line, so it cannot detect new files there. Use `--ignored=matching` with `-uall`, or hash the directory, if that check is meant to carry weight.
- tests/inspect_sweep.rs `r14_chain_through_the_setup_hook`: assert that the clone has no occupant before the hook, to match the spec's stated starting point.
- tests/inspect_loaded.rs `seat_json_through_the_daemon_leaves_the_record_unchanged` asserts only exit != 5. `readers_never_write_the_record` ignores reader exit codes, and the final re-injections in `hook_chain_reaches_current_once_the_occupant_lands` and the sweep R14 test are not checked for success. All of these were deferred minors already.
- tests/inspect_loaded.rs `sync_worktree` and `git_status` spawn git outside `TestRoot` with the real HOME gitconfig (existing precedent, previously deferred).
- tests/inspect_live.rs: the "adds no commit" count can race background daemon commits (previously deferred plausible flake).
- tests/inspect_sessions.rs encodes exit 2 for `--since nonsense` against exit 1 for other usage errors (previously deferred).
- tests/daemon_composed.rs: rustfmt-only churn mixed into the change.
- README "Test tiers" (line 231) names tests/inspect_live.rs as the inspection views' one tier-3 test and omits tests/inspect_sweep.rs (readme cluster).
