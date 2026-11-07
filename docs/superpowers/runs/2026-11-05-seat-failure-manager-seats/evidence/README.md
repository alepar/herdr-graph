# Test evidence (summaries)

The raw cargo logs that used to sit here were removed: they embedded absolute local paths. Counts are the sums of
the `test result:` lines of each run (`cargo test --locked --offline --no-fail-fast`, in toolbox `ht-build`).

| Run | Commit | Passed | Failed | Ignored |
| --- | --- | --- | --- | --- |
| hg-k4l.22 full suite, attempts 1-4 | integration sweep | 1042 / 1046 / 955 / 1046 | 0 | 1 / 1 / 0 / 1 |
| hg-k4l.22 tier 3 (private Herdr), attempts b-f | integration sweep | 0 / 0 / 1 / 26 / 2 | 1 / 1 / 0 / 4 / 1 | 0 |
| sweep-1 (Phase 6) | 8e3d34c | 1060 | 0 | 1 |
| rework, `--features test-support` | c9fd1bc | 1088 | 0 | 1 |
| B11 and reopen-loop rework, `--features test-support` | this commit | 1091 | 0 | 1 |
| `e2e_node_manager_failing_agent_does_not_respawn_loop` (`--features private-herdr`, Herdr 0.9.5) | this commit | 1 | 0 | 0 |

Plain `cargo test` does not build `tests/manager_seats_sweep.rs` (feature `test-support`); the first three rows
were taken without it.
