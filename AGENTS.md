# Agent notes for herdr-graph

## Build and test in `hg-build`

- Build and test herdr-graph only in this project's toolbox, `hg-build`:
  `toolbox run -c hg-build cargo test` (setup and test tiers: README "Test tiers").
- Never use `ht-build`. It belongs to herdr-threads, and it is shared with that project's agents.
- Do not use any other container either (for example `durak-dev`). Do not install build packages on the host.

## Never leave test rigs running

Tests start private Herdr servers, daemons and shells. Each one has its HOME or cwd under `/tmp/hg-*`. Leaked rigs fill the toolbox's pid limit and break other projects' test runs (EAGAIN).

- After every test run, including one that failed, timed out or was killed, check that no test processes are left:

  ```sh
  for p in $(pgrep -u "$USER"); do readlink /proc/$p/cwd; done | grep -c '^/tmp/hg-'
  ```

  Any count above 0 means a leak. Stop those processes, then remove their `/tmp/hg-*` dirs.
- Do not hard-kill a test run (SIGKILL, a task stop, a timeout) unless you have to. If you do, run the check above straight after.
- Every test you add or change must stop its rig when it ends: on success, on failure and on panic. It goes through `TestRoot` or `PrivateHerdr` (see README "Test isolation").
