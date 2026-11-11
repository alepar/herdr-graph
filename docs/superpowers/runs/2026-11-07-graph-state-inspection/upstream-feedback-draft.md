# 2026-11-07-graph-state-inspection: finding projection counts Rejected/Deferred-OK as outstanding and never registers repairs

Plugin: 6.4.2-alepar6.8. Run: super-auto, 1 root epic, 11 design leaves + 12 fix/sweep beads (23 landings), design roast 2 rounds (1 redesign), code roast 3 rounds (capped, 0 Blocking), 6 coordinator invocations, 2026-11-08 → 2026-11-11 (includes a host restart and a profile switch).

## Defects

**1. The projection marks Rejected and Deferred-OK entries as outstanding, and repairs never register**
- **kind:** defect
- **claim:** `scope-dispositions --project` flags every occurrence group `outstanding: true`, including roast "Rejected (with reason)" entries and final-review "Deferred OK" entries. It records 0 repaired even after two fix rounds whose roast deltas report resolved findings. `report-status` then filters on `f.outstanding` and reports all 593 as open.
- **evidence:**
  - Projection metrics: `"occurrences": 593 … "findings": 498, "otherSectionGroups": 95 … "repaired": 0, "rejected": 0, "outstanding": 498, "deferred": 4`.
  - Yet all 593 groups have `outstanding: true`, including 44 "Rejected (with reason)" groups (r1 14, r2 15, r3 15) and about 220 "Deferred OK" groups.
  - report-status prints `outstanding findings: 593`, plus 593 `finding acceptance relevance unproven:` gaps. The status line and the qualifier line repeat them, 1186 in total.
  - Roast deltas: r2 `9 resolved`, r3 `4 resolved`.
  - report.md: "Roast groups outstanding (116), none with a recorded disposition".
- **premise:** Upstream must confirm three things:
  - Section semantics: Rejected means disposed, and Deferred OK means non-blocking by the reviewer's own word.
  - The delta's "resolved" keys can be joined to the prior round's projection refs.
  - Fix beads (`sp:fix-round-N`) carry or can carry the finding ref, so a repair can be recorded.
- **fix-shape:**
  - Classify section kind in the projection: Rejected → rejected, Deferred OK / Untested scope → non-outstanding or a separate bucket.
  - Mark prior-round findings repaired when a later roast's delta lists them resolved.
  - Keep report-status's `outstanding` consistent with `metrics.outstanding`.

**2. report-status emits one unbounded line per unjudged finding**
- **kind:** defect
- **claim:** report-status writes a separate gap string for every outstanding finding, with no cap. The output becomes unreadable, and the real qualifiers are buried in it.
- **evidence:**
  - `report-status.txt` is 180,354 bytes.
  - It contains 1186 `finding acceptance relevance unproven: [{"source":…,"section":…,"position":N}]` entries on two lines.
  - Source: `super-auto/scripts/report-status:395` calls `gap(...)` per finding.
- **premise:** Consumers read the qualifier lines, which report.md does (it quotes them), so a count plus a pointer is enough.
- **fix-shape:** Aggregate these gaps into a count per source and section, for example `finding acceptance relevance unproven: 593 (roast-code/r1 Rejected 14, …); see scope-dispositions --project`. Fixing finding 1 shrinks the list as well.

**4. A triage edge to a review bead is invisible to mid-round readiness, so the coordinator stalls**
- **kind:** defect
- **claim:** Triage added `hg-blw.10 <- hg-blw.20` (a review bead), but the planner's deps rows never included it. When hg-blw.20 closed, readiness never saw the blocker clear. The no-progress guard stopped the run as `stalled` while hg-blw.10 was ready, and a human-driven re-invocation was needed.
- **evidence:**
  - Friction: "coordinator stopped 'stalled' with hg-blw.10 ready: a triage-added edge (hg-blw.10 <- hg-blw.20 review bead) was not in the planner's JS deps rows, so mid-round readiness never saw it clear".
  - Detector: `round 2 — 0 ready · topped-up 0 · cap 4 · peak in-flight 0 … idle slots 4 · waiting on deps 2`.
  - Edge audit round 2: "hg-blw.10's blockers are all closed, so it is ready … Peak in-flight of 0 therefore points to a dispatch stall".
  - Code: `coordinator.js:2011` skips ids whose `addedDeps` blocker is not `dependencyReady`. For a non-local id, `dependencyReady` reads `externalStatus` (line 1540).
- **premise:** Upstream must confirm that `externalStatus` is not refreshed for blockers added mid-run. It is likely snapshotted at plan time, and review beads are external to the planner's rows.
- **fix-shape:** When `addEdge` adds an external blocker, add it to the tracked status set and refresh its status at each round boundary, or before the stall guard fires. Treat "ready id held only by a now-closed added blocker" as progress, not a stall.

**6. `report-verdict --repair-check` rejects a repair that only removes stray separators from the header**
- **kind:** defect
- **claim:** The round-3 roast report started with a stray `---`. The checked format repair removed it, and `--repair-check` rejected the repair as "(ii) header lines differ". The original could not be extracted either, so the run used an unaccepted repair, outside guidance.
- **evidence:**
  - `diff pr-3.md pr-3-format-repair.md` shows only `1d0 < ---` and `66d64 < ---`.
  - Friction: "report-verdict --repair-check rejected the round-3 format repair with '(ii) header lines differ' … the original cannot be extracted (--complete exit 4)".
  - `super-code/scripts/report-verdict:290-292`: `head()` takes all lines before the first `## ` and requires exact positional equality.
- **premise:** Separator-only lines carry no verdict content, and the roast writer can emit stray `---` lines (it happened here).
- **fix-shape:** Before the header comparison in `--repair-check`, drop separator-only lines (`^-{3,}\s*$`) and blank lines from `head()`. Separately, have super-roast's writer stop emitting a leading `---`.

**7. Task worktree cleanup fails closed on an uninspectable user-uid process unrelated to the run**
- **kind:** defect
- **claim:** `remove-task-worktree --sweep`, through `stop-run-processes`, exits 4 when some process's cwd, exe or environment cannot be read, even if that process has nothing to do with the run. Every task cleanup in the run hit this, and the coordinator removed 7 merged worktrees by hand.
- **evidence:**
  - Friction: "remove-task-worktree --sweep fails closed (exit 4) on an unrelated root-owned sshd-session whose cwd/env cannot be inspected … removed the 7 merged worktrees by hand".
  - Ledger: `Cleanup: hg-blw.12 — unknown: … process 1910 cwd inspection failed`.
  - report.md: "Process sweep: … unknown (processes 1910, 1943, 3894, 5065: cwd/executable/environment inspection failed)" and "Worktree kept: /tmp/sp-f11883d5".
  - Code: `stop-run-processes:167-179` skips only processes where `stat -c %u /proc/PID` differs from the user's uid. A non-dumpable process that `stat` shows as the user's uid (sshd-session is a likely case) passes that filter, then fails `readlink cwd`, which sets unknown.
- **premise:** Upstream must reproduce this with a non-dumpable process (an SSH login session) on Linux, and confirm such processes cannot have been started from a task worktree.
- **fix-shape:** Treat a process whose `/proc/PID/cwd` and `exe` are unreadable and that is not a descendant of the run as out of scope. For example, check `/proc/PID/status` for non-dumpability or a privileged parent. Report the process instead of failing the whole sweep.

**9. A run-critical return file lives only on tmpfs**
- **kind:** defect
- **claim:** An invocation's return value, with 15 per-task check records, was written only to tmpfs and lost in a host restart. That left a permanent "not transcribed" evidence gap in root acceptance.
- **evidence:**
  - report-status: "invocation inv-4b650ab2…: 15 returned task check records … not transcribed — the return file was on tmpfs lost in the 2026-11-10 host restart".
  - The caller's input notes: "the first invocation's return value (lost on tmpfs at a host restart)".
- **premise:** Upstream must confirm the return file path is under `/tmp/sp-*` and is transcribed into the ledger only after the invocation returns.
- **fix-shape:** Write invocation return records, or at least the check records, to the run's durable workspace (`.superpowers/sdd/<plan>/`) as they are produced. Alternatively, have transcription fall back to the per-task `task-*-report.md` files, which survived.

## Run metrics
### Judge panel
- 2026-11-08-graph-state-inspection-roast-design-1.md: mode: design iteration: 1 of 3 · independence: same-family (Claude) — seat-differentiated panel · rung: Workflow · seat-agreement: panels 33 · rr 0.64 · rg 0.76 · fg 0.39 · unanimous 0.39 · ground-loo 0.62 (n=21) · reproduce 16/17/0 · refute 4/29/0 · ground 24/9/0
- 2026-11-08-graph-state-inspection-roast-design-2.md: mode: design iteration: 2 of 4 · independence: same-family (Claude) — seat-differentiated panel · rung: Workflow · seat-agreement: panels 7 · rr 0.86 · rg 0.57 · fg 0.43 · unanimous 0.43 · ground-loo 0.50 (n=6) · reproduce 4/3/0 · refute 3/4/0 · ground 4/2/1
- 2026-11-10-graph-state-inspection-roast-pr-1.md: mode: PR iteration: 1 of 3 · independence: same-family (Claude) — seat-differentiated panel · rung: Workflow · seat-agreement: panels 27 · rr 0.89 · rg 0.78 · fg 0.81 · unanimous 0.74 · ground-loo 0.83 (n=24) · reproduce 13/14/0 · refute 12/15/0 · ground 17/10/0
- 2026-11-10-graph-state-inspection-roast-pr-2.md: mode: PR iteration: 2 of 3 · independence: same-family (Claude) — seat-differentiated panel · rung: Workflow · seat-agreement: panels 6 · rr 0.83 · rg 0.83 · fg 0.67 · unanimous 0.67 · ground-loo 0.80 (n=5) · reproduce 5/1/0 · refute 4/2/0 · ground 6/0/0
- 2026-11-11-graph-state-inspection-roast-pr-3-format-repair.md: mode: PR iteration: 3 of 3 · independence: same-family (Claude) — seat-differentiated panel · rung: Workflow · seat-agreement: panels 2 · rr 0.00 · rg 1.00 · fg 0.00 · unanimous 0.00 · ground-loo n/a (n=0) · reproduce 2/0/0 · refute 0/2/0 · ground 2/0/0
### Fix loop
Metrics: merges 23 · merge-failed 0 · rebase-conflicts 2 · seam-reviews 7 (fixed 1) · check-fails 0 (fixed 0)
Metrics: completions — review clean 21 · after fix pass 2 · parked 1 · re-entry closes 0 · dispatched early 6 · cancelled 0
Metrics: fix-pass — entered 3 · FIXED 2 · BLOCKED 1 (legacy event lines); durable attempts 4 · historical FIXED 3; repaired findings unavailable (use scope-dispositions --project)
Metrics: ledger-check ok · append-failed 0 · append-retried 0 · scope: observed ledger; completed after available tracker reconciliation · git-only [] · ledger-only-git [] · gaps: none in observed sources
### Merge-back
See the Fix loop block's `Metrics: merges` and `ledger-check` lines (23 merges, 2 rebase conflicts, 7 seam reviews, 0 check fails). Landing reconciliation: ledger, Git and completed IDs agree (ledger-only [], git-only [], completed-only []); `baseAbsorbed`: none.
### Coverage
- coverage round 1: requirements: 11 · mapped: 11 · unmapped: 0 (R12–R14 appended)
- coverage round 2: requirements: 15 · mapped: 14 · unmapped: 1 (R15, proposed that round)
- code fix round 1: scope-filter: 12 in-scope · 1 punch-listed
- code fix round 2: scope-filter: 5 in-scope · 1 punch-listed
- code fix round 3: scope-filter: 0 in-scope · 2 punch-listed (at the cap; step-back not dispatched)
### Timing
none
### Bead graph
| id | type | title | what (first sentence of description) |
| --- | --- | --- | --- |
| hg-blw.33 | task | Sweep fix: tests/inspect_loaded.rs raw CLAUDE_CONFIG_DIR env breaks the isolation lint | Covers: test_isolation::test_sources_spawn_only_through_isolated_helpers (fails in tiers 1, test-sup |
| hg-blw.32 | task | Fix: fit_hook partial-header cut is recorded as omitted, not delivered | Covers: final-review Must-fix src/bootstrap/instructions.rs:505-506 fit_hook partial header; final-r |
| hg-blw.31 | task | Fix: unreadable historical tree is a typed degraded signal in roster and log | Covers: [Should-fix] src/inspect/roster.rs:587; src/inspect/roster.rs:588, [Nit] src/inspect/log.rs: |
| hg-blw.30 | task | Fix: roster.live threads snapshot owned by the observe loop's full read | Covers: [Should-fix] src/daemon/roster_live.rs:76; src/daemon/roster_live.rs:79; src/daemon/roster_l |
| hg-blw.29 | task | Test coverage: sessions rows, truncated injection, stack clone collision, setup and fit_hook edges | Covers: final-review untested: populated sessions rows (order, timestamps, clone rows, footer); trun |
| hg-blw.28 | task | Fix: README tier-3 list and the held-open-stdin hook rationale | Covers: final-review: README.md:231 omits tests/inspect_sweep.rs; seam Must-fix setup_claude.rs:211  |
| hg-blw.27 | task | Fix: log kind words use the serde word (native_session, not nativesession) | Covers: [Nit] src/inspect/roster.rs:588 |
| hg-blw.26 | task | Fix: roster.live maps store integrity errors to exit 6 | Covers: [Nit] src/daemon/roster_live.rs:52; final-review untested: integrity StoreError in roster.li |
| hg-blw.25 | task | Fix: ops --json row shape and requester-filter test through the CLI | Covers: [Nit] src/cli/ops.rs:120, [Should-fix] tests/inspect_sweep.rs:568; final-review untested: op |
| hg-blw.24 | task | Fix: Codex session-id form note and user-level harness-file origin | Covers: final-review: harness_files.rs:217-248 user-level CLAUDE.md labelled ancestor; loaded.rs:229 |
| hg-blw.23 | task | Fix: history scans degrade per row and honour the caller's walker and cap | Covers: [Should-fix] src/inspect/log.rs:68; src/inspect/roster.rs:580, [Should-fix] src/inspect/load |
| hg-blw.22 | task | Fix: roster --live threads state is read-live, not last-known | Covers: [Should-fix] src/daemon/roster_live.rs:64; final-review: B8 threads-degraded (roster_live.rs |
| hg-blw.12 | task | Loaded record keyed by native session id, hook-only writer | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.11 | task | Integration sweep: graph state inspection end to end, read-only | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.10 | task | Docs: README and CLI help for roster, sessions, log, instructions stack and ops filters | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.9 | task | roster last_change per seat from one HistoryWalker pass | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.8 | task | ops --seat/--teamspace/--limit/--json: requester-filtered journal listing | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.7 | task | Seam contract: inspect module — Scope, View output envelope, Inventory, CLI group stubs | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.6 | task | roster --live: Herdr agent kind/status and binding match via one daemon snapshot | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.5 | task | log [<scope>] [--since] [--limit] [--by <seat>] [--all] [--json]: HistoryWalker and LogQuery | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.4 | task | Loaded-instructions record and staleness (current/stale/unknown/truncated) | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.3 | task | sessions [<scope> / clone <c>] [--since] [--limit] [--json]: all native sessions, newest first | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.2 | task | instructions stack <seat> [--clone c] [--json]: per-part manifest and harness files | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw.1 | task | roster [<scope>] [--all] [--json]: seats, clones, pane, occupant, threads state | Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-desi |
| hg-blw | epic | Graph state inspection: roster, instruction stack, sessions, scoped log | Implement the approved design at docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07- |

| dependent | blocker | reason (from `blocked-by` line, or `unstated`) |
| --- | --- | --- |
| hg-blw.21 | hg-blw.10 | unstated |
| hg-blw.20 | hg-blw.12 | unstated |
| hg-blw.19 | hg-blw.2 | unstated |
| hg-blw.18 | hg-blw.9 | unstated |
| hg-blw.17 | hg-blw.6 | unstated |
| hg-blw.16 | hg-blw.5 | unstated |
| hg-blw.15 | hg-blw.1 | unstated |
| hg-blw.14 | hg-blw.3 | unstated |
| hg-blw.13 | hg-blw.8 | unstated |
| hg-blw.12 | hg-blw.1 | Roster instructions field |
| hg-blw.12 | hg-blw.5 | HistoryWalker last-commit-touching-path |
| hg-blw.12 | hg-blw.19 | unstated |
| hg-blw.12 | hg-blw.2 | InstructionManifest Part.digest and the stack view |
| hg-blw.11 | hg-blw.12 | all leaves (integration sweep) |
| hg-blw.11 | hg-blw.2 | all leaves (integration sweep) |
| hg-blw.11 | hg-blw.6 | all leaves (integration sweep) |
| hg-blw.11 | hg-blw.8 | all leaves (integration sweep) |
| hg-blw.11 | hg-blw.9 | all leaves (integration sweep) |
| hg-blw.11 | hg-blw.10 | all leaves (integration sweep) |
| hg-blw.11 | hg-blw.1 | all leaves (integration sweep) |
| hg-blw.11 | hg-blw.7 | all leaves (integration sweep) |
| hg-blw.11 | hg-blw.3 | all leaves (integration sweep) |
| hg-blw.11 | hg-blw.5 | all leaves (integration sweep) |
| hg-blw.10 | hg-blw.9 | final flags and JSON shape of its view |
| hg-blw.10 | hg-blw.20 | unstated |
| hg-blw.10 | hg-blw.6 | final flags and JSON shape of its view |
| hg-blw.10 | hg-blw.3 | final flags and JSON shape of its view |
| hg-blw.10 | hg-blw.7 | CLI groups and flags |
| hg-blw.10 | hg-blw.1 | final flags and JSON shape of its view |
| hg-blw.10 | hg-blw.2 | final flags and JSON shape of its view |
| hg-blw.10 | hg-blw.8 | final flags and JSON shape of its view |
| hg-blw.10 | hg-blw.5 | final flags and JSON shape of its view |
| hg-blw.10 | hg-blw.12 | final flags and JSON shape of its view |
| hg-blw.9 | hg-blw.5 | HistoryWalker and LogQuery |
| hg-blw.9 | hg-blw.1 | Roster struct |
| hg-blw.8 | hg-blw.7 | View output envelope |
| hg-blw.6 | hg-blw.1 | Roster builder |
| hg-blw.5 | hg-blw.7 | boundary contract |
| hg-blw.4 | hg-blw.2 | InstructionManifest Part.digest and the stack view |
| hg-blw.4 | hg-blw.1 | Roster instructions field |
| hg-blw.3 | hg-blw.7 | boundary contract |
| hg-blw.2 | hg-blw.7 | View output envelope |
| hg-blw.2 | hg-blw.5 | HistoryWalker last-commit-touching-path |
| hg-blw.1 | hg-blw.7 | boundary contract |

## Design questions

**3. One missing final-review cluster permanently blocks review readiness, and that cluster was the run's own records**
- **kind:** design-question
- **claim:** Final review cluster 13 of inv-64d86974 held only run bookkeeping: design doc, run.md, ledgers, roast and coverage records, friction.md. It overflowed the window and was never written. Nothing can recover a missing cluster, so all three later code-roast rounds ran bounded with provenance unavailable. The run could never reach review-ready.
- **evidence:**
  - Friction: "review collection for inv-64d86974 stays at gaps (cluster-13 file never written, no recovery path for a missing split cluster)".
  - Friction, per round: "code roast r1/r2/r3: assembler exit 2 'review input selection: provenance unavailable; captured source available' (inv-64d86974 cluster-13 gap)".
  - report.md: "cluster 13 (docs-top: design doc, run.md, ledgers, roast/coverage records, friction.md) was never written because it exceeded the context window".
  - Final review inputs grew from 122K to 211K to 295K (splits into 10, 16 and 13 clusters). run.md is 432,909 bytes.
  - report-status: `final review: not ready (unreviewed: whole-epic coverage and omission provenance remain incomplete)`.
- **premise:** Upstream must confirm that run-record paths (`docs/superpowers/runs/**`, `.superpowers/**`) sit in the reviewed diff range, and are not deliberately reviewed content.
- **fix-shape (the design question):**
  - Option A: exclude the skills' own run artifacts from final-review and roast assembly. They are bookkeeping, not product, and on long runs they dominate input size.
  - Option B: keep them, since run records can carry spec amendments, and instead add a recovery path that re-dispatches only a missing cluster under the same invocation, so one oversized cluster does not taint every later round.

**5. Dependents are released and stacked when a blocker is closed as implemented but has not merged**
- **kind:** design-question
- **claim:** Twice, a dependent was dispatched stacked on a blocker branch that was closed as implemented but still had its review and fix open. The stack drifted, triage had to add an edge to the review bead, and the task was cancelled and re-cut.
- **evidence:**
  - `Edge add: hg-blw.12 <- hg-blw.19 (triage)`: "The bead graph records a blocks edge on hg-blw.2, which is closed as implemented, but none on hg-blw.19, the bead that actually lands it".
  - `Edge add: hg-blw.10 <- hg-blw.20 (triage)`: "setup stacked the unmerged task-hg-blw.12 branch … add/add conflict".
  - `Merge: hg-blw.12 — rebase conflict: 3 files`.
  - Metrics: `cancelled 1`, `stacked 3`.
- **premise:** The ledger shows stacking pays off when the review is clean (`dispatched early 6`), so this cost has to be weighed against that gain.
- **fix-shape (the design question):**
  - Option A, release early: when a blocker's review enters a fix pass, which changes the stacked commits, automatically add `dependent <- review-bead` edges, or restack automatically after the fix merges. This keeps early dispatch and removes triage round-trips.
  - Option B, release on merge: never stack on a blocker whose review is not clean. Simpler, but it forfeits early dispatch.

If upstream decides otherwise, please state the position explicitly so downstream can reconcile against words rather than silence.

## Doc gaps

**8. The task worktree lifecycle does not handle repos with git submodules**
- **kind:** doc-gap
- **claim:** Task worktree setup does not run `git submodule update --init`, so builds fail until the epic gets a manual constraint. `remove-task-worktree` calls `git worktree remove` without `--force`, which git refuses for worktrees with initialized submodules.
- **evidence:**
  - Friction: "task worktrees need git submodule update --init (third_party/herdr-threads) — added to the epic's Global Constraints".
  - Friction: "plain git worktree remove refuses worktrees with submodules; … with --force only for the submodule rule".
  - `remove-task-worktree:108` runs `git worktree remove "$wt"`, and its header says "never --force".
  - `grep -i submodule` over the skills finds only `using-git-worktrees/SKILL.md`'s detection guard.
- **premise:** The skills support repos with submodules. Upstream must check git's documented behaviour: "working trees containing submodules cannot be moved or removed" without `--force`.
- **fix-shape:** In task-worktree setup, run `git submodule update --init --recursive` when `.gitmodules` exists. In `remove-task-worktree`, once the clean-status and ancestry guards have passed, allow `--force` when the only refusal is submodule presence, for example after `git submodule deinit --all`.

## Already fixed — do not re-litigate
none

## Not established
- By default: the coordinator's ledger-append path is fire-and-forget and can lose a line without the coordinator noticing, so every ledger-derived count in `## Run metrics` (Fix loop, Merge-back) is a lower bound. The `Metrics: ledger-check` line is the one cross-check that exists.
- Timing: no `Profile:` lines were written in any invocation, so no critical-path, bottleneck or what-if figures exist.
- The first coordinator invocation's return value was lost on tmpfs at a host restart (its per-task check records survive only in task reports).
- Dropped at triage: fix passes merging without re-review (2 fix-pass merges in this run is too thin to argue a rate).
- Excluded by the analyst: the hg-build container losing /etc/passwd (environment), `concurrency: 4` (project choice), the skipped r3 step-back (single recorded deviation).
- Analyst model: opus.

## Verification bar
- Re-run `scope-dispositions --project` and `report-status` on this run's `run.md`: Rejected and Deferred-OK groups should leave the outstanding count, and report-status output should stay bounded.
- A coordinator dryRun where triage adds an edge to a review bead that closes mid-round: the dependent must dispatch without a stall.
- `report-verdict --repair-check` on an original/repair pair that differ only by `---` lines must exit 0.
- `stop-run-processes` on Linux with an unrelated non-dumpable same-uid process (an SSH session) must not exit 4.
- A task worktree lifecycle in a repo with a git submodule: setup builds, cleanup removes without manual `--force`.

