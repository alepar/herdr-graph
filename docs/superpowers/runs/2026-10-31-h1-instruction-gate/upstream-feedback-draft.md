# 2026-10-31-h1-instruction-gate: run worktree never initializes submodules, so every merge check fails

Run: super-auto, autonomous, plugin superpowers 6.4.2-alepar6.6. 30 task beads landed; design roast converged at round 2; code roast capped at round 3 with 0 Blocking; phase-6 sweep green (1694 passed, 0 failed).

## Defects

### 1. super-auto pre-flight creates the run worktree without `git submodule update --init`
- **Evidence:** "all 8 merge attempts failed mergeCheck ... third_party/<submodule> (a path dependency) was empty ... Cost: one full invocation (~28 min, 100 agents, 46 min of rework dispatch time)"; `Metrics: merges 0 · merge-failed 8 · check-fails 8 (fixed 0)`. Task worktrees initialized the submodule themselves, so task-level checks passed.
- **Premise to verify:** the run worktree is created by `git worktree add` in super-auto §Run directory with no submodule step.
- **Suggested fix shape:** init submodules recursively when the run worktree is created, and assert in pre-flight that submodule paths are populated before the first mergeCheck.

### 2. A shared integration-environment merge-check failure is not recognized; fix passes record FIXED with base == head
- **Evidence:** 6 Recovery records `"outcome":"FIXED","phase":"check"` with identical base/head (some `"evidenceOnly":true`), each followed by `check fail → blocker`; every BLOCKED cause named the same missing submodule; `fix-pass ... historical FIXED 6` while `check-fails 8 (fixed 0)`.
- **Premise to verify:** merge-check failure causes are comparable across tasks.
- **Suggested fix shape:** after N consecutive same-cause merge-check failures on different tasks, stop dispatch and report `integration-blocked`; reject a FIXED outcome whose head equals base.

### 3. Early-unblock stacks onto several unmerged parents that conflict with each other
- **Evidence:** "early-unblock stacked <two tasks> onto several unmerged parent branches whose changes conflicted with each other (<three shared files>); both failed at workspace setup and the run stalled with 3 beads open"; profile `rework — ... 3m (SETUP_FAILED) · ... 2m (SETUP_FAILED)`; a `Human action:` line asking to remove the retained stack worktree by hand.
- **Premise to verify:** parent-to-parent conflicts are detectable before setup (trial merge of the parent heads).
- **Suggested fix shape:** trial-merge parents before multi-parent stacking; on conflict fall back to waiting for the merges, and discard the stack worktree automatically on SETUP_FAILED.

### 4. remove-task-worktree / stop-run-processes fail closed on unreadable user-session processes, so no task worktree is ever removed
- **Evidence:** "fail closed (exit 4) on user-session processes whose environ is unreadable (sshd-session, systemd --user, defunct zombies), so every landed task worktree was retained"; `processSweep: ... unknown 8 ... none run-owned`; retained worktrees each held a ~49 GB diverged reflinked `target/` (1.2 TB apparent) and another seat had to free disk (/var 70 GB → 228 GB).
- **Premise to verify:** run ownership can be decided from cwd/open files under the root without reading environ.
- **Suggested fix shape:** treat an unreadable environ with no cwd or open fd under the root as not run-owned, and skip zombies, instead of returning unknown for the whole root.

### 5. git refuses to remove a worktree with an initialized submodule; remove-task-worktree does not handle it
- **Evidence:** "git also refuses to remove worktrees with an initialized submodule"; the orchestrator added its own reaper that deleted only `target/` when git refused.
- **Premise to verify:** deinit (or `--force`) is safe once the task branch is merged and the tree is clean.
- **Suggested fix shape:** deinit submodules before `git worktree remove` for merged, clean task worktrees.

### 6. One malformed final-review cluster report makes provenance unavailable for the rest of the run
- **Evidence:** "three final-review cluster reports (1, 6, 13) were not strictly extractable (empty Findings section / unmarked finding line); collection exit 3"; the next final review "ran in bounded mode (16/199 paths) because the ... collection gap made omission provenance unavailable"; every later code-roast round carried `[review readiness incomplete]` and could not converge; the assembler returned the same typed result every round.
- **Premise to verify:** a single cluster report can be re-requested or leniently parsed without redoing the whole review.
- **Suggested fix shape:** on a cluster extraction failure, re-dispatch only that cluster with its format contract (or accept a degraded lenient parse) so one malformed report cannot block convergence for the run.

### 7. Finish ledger re-read truncates at the transport boundary
- **Evidence:** `Metrics: UNAVAILABLE (merges) — the Finish ledger re-read returned null or unavailable transport (truncated)` for 4 invocations; `read-ledger: ledger transport truncated — truncated (142 of 143 lines)`; `ledger-check landed≠completed: 29 vs 5`.
- **Premise to verify:** the truncation is in the read transport, not the file.
- **Suggested fix shape:** read the ledger by file path or in checked chunks; report "read incomplete" instead of a landed/completed mismatch.

### 8. scope-dispositions drops a cluster whose final-review member is in scope
- **Evidence:** "the setup-preexisting-ownership cluster was dropped by scope-dispositions (its only roast member was punch-listed) although its final-review member (setup codex overwrites a pre-existing rules file: data-loss path) is in scope under the autonomous final-review rule"; run.md `cluster dropped: setup-preexisting-ownership`.
- **Premise to verify:** scope-dispositions only sees roast keys when deciding a cluster.
- **Suggested fix shape:** drop a cluster only when every member, final-review members included, is out of scope.

### 9. The report drafter is told to write a file the harness forbids subagents to write
- **Evidence:** "the report drafter's Write to report.draft.md was refused by a harness rule (subagents return text, not report files); the orchestrator wrote the returned draft verbatim".
- **Premise to verify:** the restriction applies to Claude Code general-purpose subagents generally.
- **Suggested fix shape:** make "return the draft text; the orchestrator persists it" the default path in report-prompt.md.

## Run metrics

### Judge panel
- design r1: `rr 0.85 · rg 0.69 · fg 0.69 · unanimous 0.62` (26 panels); design r2: `rr 0.70 · rg 1.00 · fg 0.70` (10); PR r1: `rr 0.93 · rg 0.71 · fg 0.79 · unanimous 0.71` (28); PR r2: 2 panels; PR r3: 3 panels; post-cap audit: none. All `same-family (Claude) — seat-differentiated panel · rung: Workflow`.

### Fix loop
- Code roast: r1 Should-fix 7 → r2 Nit 1 (5 resolved) → r3 Should-fix 2 (fix-regression, 3 resolved) → capped; post-loop fix round 1 bead; post-cap audit clean. `scope-filter: 5 in-scope · 2 punch-listed` (r1), `1 in-scope` (r2).

### Merge-back
- 29 reconciled merges, 8 failed merges (all inv-p3, submodule), 2 rebase conflicts, 8 seam reviews; later invocation counts unavailable (truncated ledger).

### Coverage
- Coverage r1 `requirements: 15 · mapped: 14 · unmapped: 1`; r2 `20 · 19 · 3`, divergence 12 → 17, novel 76%, widening yes (cap held at 2).

### Timing
- inv-p3 28m, nothing landed; inv-p3b 1h00m, merge-lane-bound; fix-loop invocations bead-bound with planning 26–34% of the critical path.

### Bead graph
- 18 leaves planned, depth 7 → 6 after the graph pass (1 edge dropped), width 3.0; 30 task beads landed in total incl. fix beads.

## Design questions
- none filed (planning cost on small fix rounds and moving seam review off the merge lane were considered and left out as low-confidence).

## Doc gaps
- Implementer prompts lack a "never commit test logs or evidence files" rule: task agents twice committed logs under a top-level `evidence/` directory (flagged Must-fix by a final review).
- super-roast PR-mode reproduce seats carry no build-location guard: they built cargo targets on the shared `/tmp` tmpfs (~19.7 GB, `/tmp` hit 0 MB free).
- The sweep guidance does not warn that a long TMPDIR breaks Unix-socket tests (`path must be shorter than SUN_LEN`); the first sweep was MEASUREMENT INVALID.

## Already fixed — do not re-litigate
- none

## Not established
- The analyst read its inputs from one assembled prompt file rather than inline in the dispatch.
- Roast reports had no retained source extraction; the code findings ledger was not rebuilt after r2 and the design ledger not after design r2.
- Whether defect 4's root is environ reading specifically, or any inspection failure, was not isolated.

## Verification bar
- Defects 1, 4, 5: reproduce on any repo with a submodule path dependency.
- Defect 2: inject a constant mergeCheck failure and confirm the run stops after N same-cause failures.
- Defect 6: feed one cluster report with an empty Findings section and confirm the next invocation's provenance is still available.
