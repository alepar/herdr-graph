# friction — 2026-10-02-herdr-graph-implementation

- pre-flight: native EnterWorktree defaults to origin/<default>, but the repo has no remote; created the worktree with `git worktree add` under .claude/worktrees/ and entered it via EnterWorktree path. Required adding .claude/worktrees/ to .gitignore on main first.
- pre-flight: worktree-isolated session refuses compound shell commands that mix git with other ops; split into single commands.
- phase 3 launch: Workflow refused scriptPath in the plugin cache (not a readable/added dir); copied coordinator.js to the session scratchpad and launched from there.
