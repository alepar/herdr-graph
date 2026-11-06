# hg-66u.82 sweep-fix: daemon_composed subprocess tests

Verdict: environmental (thread/PID exhaustion of the ht-build toolbox container). Not a run regression; no code change.

## Environment (step 1), measured 2026-11-06 during this task
- Toolbox container cgroup `pids.max` = 2048; `pids.current` = 1867..1885 before any test of this task started.
- Per-user `ulimit -u` = 771367, `nproc` = 32 (so each tokio runtime starts 32 workers).
- 54 `herdr server` processes under the user, 50 of them 1 to 3 days old (stale private test rigs). Each holds about 34 threads
  (`tokio-rt-worker` count in the container: 1792 of the ~2700 threads). That is about 1800 of the 2048 slots.
- Headroom is therefore about 160-180 threads. The whole-file run starts one daemon per `subprocess::` test in parallel (32 test
  threads, nproc-sized tokio runtime per daemon), exceeds it, and the daemon dies at startup with
  `failed to spawn thread: Os { code: 11, kind: WouldBlock }` (EAGAIN) before it writes any log, hence the empty
  "daemon did not become ready; log: " in sweep.log and the rustc EAGAIN ICE right after it.

## Runs (task worktree at c34e4c5, `cargo test --features test-support --test daemon_composed`, inside ht-build)
| config | result |
|---|---|
| whole file, default parallelism, run 1 | FAILED 16 passed / 6 failed (finished 30.16s); 6 x `daemon did not become ready; log: ` at tests/daemon_composed.rs:1405, plus EAGAIN message |
| whole file, default, run 2 | FAILED 16 / 6 (30.14s), EAGAIN message x3 |
| whole file, default, run 3 | hung >8 min with the pid cgroup saturated; stopped with stop-run-processes (no verdict) |
| whole file, `RUST_TEST_THREADS=2` | ok. 22 passed; 0 failed (8.64s), all six subprocess tests green |
| each of the four named tests alone | ok (1 passed each, 0.5-1.1s) |

## Pre-run comparison, main 0a84c67 (detached worktree, same toolbox, same pid pressure)
| config | result |
|---|---|
| whole file, default, run 1 | FAILED 15 passed / 6 failed (30.18s) |
| whole file, default, run 2 | FAILED 15 / 6 (30.17s) |
Same six tests fail on the pre-run tree, so the failure does not come from this run's changes.
(The sweep named four; two more, crash_after_split_pane and crash_between_commit_and_journal, fail the same way here
because they are the same kind of test; which tests lose the race varies with timing.)

## Conclusion
Environmental: the container's 2048-thread budget is consumed by stale private herdr servers. Reducing concurrent daemons
(RUST_TEST_THREADS=2) makes the whole file green on the run tree. The four tests are unmodified and pass.

## Human action (recorded in friction.md as well)
Stop the stale `herdr server` processes (1-3 days old, ~50 of them) that hold the toolbox thread budget; never matched by this
task's stop script, so they were left alone. Optional follow-up (not done here): the test rig could cap tokio worker threads
for subprocess daemons or the test file could limit concurrency; raising WAIT_SECS or adding retries would not help.
