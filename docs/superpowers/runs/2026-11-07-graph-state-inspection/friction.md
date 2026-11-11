# friction log — 2026-11-07-graph-state-inspection
- [2026-11-08] roast-design r1 · rung: Workflow
- [2026-11-08] mergeCheck covers default and --all-features (cargo check --all-targets); task worktrees need git submodule update --init (third_party/herdr-threads) — added to the epic's Global Constraints
- [2026-11-08] hg-build container lost /etc/passwd mid-run (every toolbox exec fails: 'unable to find user alepar'); hg-blw.2 fix pass BLOCKED on it; podman restart left it wedged in Stopping, podman kill failed in crun; direct signal to its init was refused by the permission layer — needs a human
- [2026-11-10] remove-task-worktree --sweep fails closed (exit 4) on an unrelated root-owned sshd-session whose cwd/env cannot be inspected, and plain git worktree remove refuses worktrees with submodules; removed the 7 merged worktrees by hand after a cwd check, a clean-status check and an ancestry check, with --force only for the submodule rule
- [2026-11-10] coordinator stopped 'stalled' with hg-blw.10 ready: a triage-added edge (hg-blw.10 <- hg-blw.20 review bead) was not in the planner's JS deps rows, so mid-round readiness never saw it clear; re-entered with a fresh invocation
- [2026-11-10] code roast r1: assembler exit 2 'review input selection: provenance unavailable; captured source available' (inv-64d86974 cluster-13 gap); ran the bounded direct review per super-auto phase 4 (product paths only); rung: Workflow
- [2026-11-10] review collection for inv-64d86974 stays at gaps (cluster-13 file never written, no recovery path for a missing split cluster); fix-loop re-entry proceeds with the gap retained and reported, since nothing can produce the original review
- [2026-11-10] code roast r2: same assembler provenance result; bounded direct review again; rung: Workflow
