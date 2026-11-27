# Final review inv-3, cluster 33 (tests-support)

Scope: tests/support/{agent_stub,mod,private_herdr,threads_bin}.rs, base 60e287a to head 11ac415. Test-only support code; no production entry point is in this cluster, so no requirement-baseline row can be traced here (the root spec carries no separate baseline section that this cluster could fail alone).

## Analysis

The stub claude mirrors the two real hooks (threads SessionStart and UserPromptSubmit, graph session-report) against the rig's own daemon, graph instance and socket only. All paths derive from the stub's own directory under the rig root, so it cannot reach another project's state. Start numbering is line count of claude.starts, matching the starts() reader.

threads_bin.rs: resolve() was generalized by override env, target dir and features. The default build is unchanged (same target dir, no features). The test-support build uses a separate target dir, so the two builds do not clobber each other. The matrix guard still forbids nested Cargo and now names the right override variable. Version check applies to both. The feature test-support exists in the pinned Cargo.toml, and HT_TEST_* clock overrides exist in the pinned source. test_support_binary returns Result and its one caller (tests/threads_real.rs) panics with the reason, so there is no skip path.

private_command is a pure extraction from PrivateHerdr::command with identical behavior (env_clear, env, current_dir). The threads_real.rs caller uses it.

freeze_at sends SIGSTOP to graph's daemon from inside the stub; the caller test resumes it with SIGCONT. A panic between the two leaves a stopped daemon until rig teardown; teardown must use SIGKILL or SIGCONT-then-TERM. This was not exercised by a run in this review (a sweep is running in the integration worktree; cargo was not run).

stub.env values are single-quoted without escaping; rig paths are ulid-derived under /tmp/hg-*, so no quote can occur.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Teardown of a rig whose graph daemon is SIGSTOPped by freeze_at when the test panics before SIGCONT was not run or read; confirm teardown escalates to SIGKILL so no /tmp/hg-* process leaks.
- No cargo run in this review (sweep in progress); behavior of the stub on a real Herdr was judged by reading only.

## Deferred OK

- The sleep 2 in the stub before the graph report is a timing assumption about Herdr agent detection; acceptable for test support.
