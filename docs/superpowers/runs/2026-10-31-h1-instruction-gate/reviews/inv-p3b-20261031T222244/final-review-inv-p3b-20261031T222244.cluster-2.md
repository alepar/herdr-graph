# Final review: cluster 2 (evidence), epic hg-45e

Invocation: inv-p3b-20261031T222244
Captured source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify <sha>^{commit}`; review-source binding recorded through dispatch-text.mjs bind-review-source, exit 0).
Caller checkout observed: HEAD bce6589afead4c031ac9ecfa3a094c9a5c4be161, `git status --short` clean, both before and after the review.

## Scope

Cluster `evidence` (slices 000028-000033), six new files:

- evidence/hg-45e.18/lib-tests.log (5 lines; commit 14af0c5)
- evidence/hg-45e.6/bootstrap-tests.log (7 lines; commit 90d0bd8)
- evidence/hg-45e.7/tests.log (37 lines; commit bab6ef5)
- evidence/hg-45e.9/features.log, full.log, tests.log (44/37/17 lines; commit 9d5c988)

These are the only product-tree changes in the cluster. All are captured cargo test output from task worktrees, not code.

## What the logs claim, and whether it holds

- hg-45e.18 lib-tests.log: full lib run, 695 passed, plus the four new admissibility/cap-warning tests by name. Consistent with the bead.
- hg-45e.6 bootstrap-tests.log: summary line only (41 passed, 642 filtered), no test names. Already a deferred minor (thin-evidence); it proves nothing about which tests ran.
- hg-45e.7 tests.log: 33 named filtered tests, all ok, covering hook render, truncation, empty matcher, Claude/Codex shapes. Consistent with the bead.
- hg-45e.9 full.log: `cargo check`, `cargo test`, `cargo test --features test-support`, all green at the task tree. The `### cargo check` header does not record its flags; the run took 2.45s, so it is not evidence of the epic's required `cargo check --locked --offline --all-targets --all-features`.
- hg-45e.9 features.log: contains a FAILED run of `golden::sweep_golden_path` ("the summary went back with the folder", tests/integration_sweep.rs:621) followed by a green 9-test run. task-15-report.md explains the failure as the stale `summaries/x.md` folder-move assertion, fixed to the derived `summaries/<tr>-<covered>.md` path; the head source at tests/integration_sweep.rs:621-624 matches that fix. Not a flake, but the committed log, read alone, records a failure.
- hg-45e.9 tests.log: per-module filtered runs, all green.

All six logs are snapshots of intermediate task trees. None was taken at the integration head, and several beads merged after them (hg-45e.14, hg-45e.18, hg-45e.5 among the last), so the counts (675, 695, 741 at head) do not describe the branch being landed.

## Independent verification at head

To check the named risk "the committed evidence does not cover the integration head", I ran the epic's global verification in a review-owned detached worktree at bce6589 (build dir seeded by reflink copy of the integration target, submodule contents copied from the integration worktree; worktree removed afterwards):

- `cargo check --locked --offline --all-targets --all-features`: exit 0.
- `cargo test --locked --offline`: exit 0; lib 741 passed; all integration binaries green (daemon_composed 14, integration_sweep 7, one pre-existing ignored test in a 3-test binary).
- `cargo test --locked --offline --features test-support`: exit 0; lib 744 passed; all binaries green.
- `cargo test --locked --offline --features private-herdr,test-support --test integration_sweep`: 9 passed including `golden::sweep_golden_path`, satisfying the hg-45e.9 golden-sweep criterion at head.

So the product is green at head; the defect in this cluster is the evidence files themselves, not what they describe.

## Findings

1. New top-level `evidence/` directory committed into the product tree. `main` has no `evidence/` path and no history for one; the epic spec and design doc do not ask for committed logs; no tracked file at head references `evidence/hg-45e*`. The files are per-task workflow scratch that belong with the run artifacts (`.superpowers/sdd/hg-45e-plan/` or `docs/superpowers/runs/2026-10-31-h1-instruction-gate/`), not at repo root. They also embed the local absolute path `/var/home/alepar/AleCode/herdr-graph/.worktrees/super-auto-h1-instruction-gate/.worktrees/...--task-hg-45e.9` and the build hash of a task worktree. Once merged to main they stay in history. Task 11's review already flagged this as a deferred minor (committed-evidence-file); at landing it is the one cheap, irreversible-after-merge item in this cluster. Fix: delete `evidence/` on the integration branch, or relocate it under the run directory if the workflow wants it kept.

2. features.log records a FAILED golden run with no annotation in the file. Readers of the committed log see a failure; the explanation lives only in the task report. Resolved by finding 1 if the directory is dropped; if it is relocated instead, annotate or trim the failed run.

3. The evidence set is stale and partial. It covers 4 of the epic's beads, none at head, and does not show the required all-targets/all-features check. My head run above closes that gap for this review; the committed logs should not be cited as landing evidence.

## Verdict
not ready (drop the stray top-level evidence/ test-log directory before landing; product suite verified green at head)

## Must fix before landing
- Remove the new top-level evidence/ directory (six task-worktree test logs from commits 90d0bd8, bab6ef5, 9d5c988, 14af0c5) from the integration branch, or move it under the run directory; main has no such path, the spec does not call for it, and the logs embed local absolute paths and a FAILED golden run.

## Untested scope
- No committed evidence was captured at the integration head bce6589; the logs are from intermediate task trees, so landing evidence is this review's head run (check all-targets/all-features, cargo test, --features test-support, private-herdr integration_sweep 9/9), not the committed files.
- hg-45e.9 full.log does not record the cargo check flags, so it does not evidence the epic's required cargo check --locked --offline --all-targets --all-features at that task tree.
- hg-45e.6 bootstrap-tests.log contains only a summary line, so it does not show which bootstrap tests ran.

## Deferred OK
- Evidence for the remaining beads (hg-45e.1, .2, .3, .4, .5, .8, .10, .13, .14, .17) exists only as prose in task reports; acceptable given the head suite is green.
- The features.log golden failure is explained in task-15-report.md as a stale summaries/x.md assertion and is fixed at head (tests/integration_sweep.rs:621-624); no product defect.
