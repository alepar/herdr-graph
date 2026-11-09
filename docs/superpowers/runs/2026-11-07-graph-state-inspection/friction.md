# friction log — 2026-11-07-graph-state-inspection
- [2026-11-08] roast-design r1 · rung: Workflow
- [2026-11-08] mergeCheck covers default and --all-features (cargo check --all-targets); task worktrees need git submodule update --init (third_party/herdr-threads) — added to the epic's Global Constraints
- [2026-11-08] hg-build container lost /etc/passwd mid-run (every toolbox exec fails: 'unable to find user alepar'); hg-blw.2 fix pass BLOCKED on it; podman restart left it wedged in Stopping, podman kill failed in crun; direct signal to its init was refused by the permission layer — needs a human
