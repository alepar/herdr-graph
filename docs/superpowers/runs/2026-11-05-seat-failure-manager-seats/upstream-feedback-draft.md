# 2026-11-05-seat-failure-manager-seats: a seeded Edge add to a review bead holds its dependent forever (coordinator.js:1564), plus three cleanup/labeling defects that stalled 4 of 10 invocations

Plugin: 6.4.2-alepar6.8 (skills sha 34fd607). Run: 78 beads (epic hg-k4l, 49 tasks plus 28 review beads), 10 super-code invocations, 43 task landings, design roast 3 rounds, code roast 4 rounds (capped), 1 code redesign. About 2026-11-04 to 2026-11-06.

## Defects

### 1. A resume-seeded `Edge add: X <- <review bead>` holds X forever: `waitingAdded` requires the blocker to be in `completed`, and review beads never enter it
- **Evidence:**
- Ledger line `Edge add: hg-k4l.24 <- hg-k4l.41 (triage)`. hg-k4l.41 is `review: hg-k4l.23`, closed.
- coordinator.js:821-825 seeds `addedDeps` from every retained `Edge add:` line, whatever kind the blocker is.
- coordinator.js:1564: `const waitingAdded = ids.filter(id => !heldBack.includes(id) && [...(addedDeps.get(id) ?? [])].some(d => !completed.has(d)))`.
- Invocations 4 and 5 both: `Detector: round 1 — 0 ready · … · waiting on deps 6`. In the same rounds the edge auditor wrote "only hg-k4l.24 is ready at the head of the chain".
- Friction: "invocations 3-5 stalled with hg-k4l.24 ready".
- Invocations 4 and 5 print `Profile: unavailable — no timed task dispatch`.
- Local patch, scratchpad only (shipped file unchanged; `cmp` shows the original is identical to the installed coordinator.js): `.some(d => !completed.has(d) && !(trackerReady.has(id) && !localDependency(d)))`.
- **Premise to verify:** once bd reports the dependent ready, bd's closed edge is authoritative for a non-local (review-bead) blocker. Also check whether an in-run triage edge to a review bead (line 2784) has the same hold, not only a resume-seeded one.
- **Suggested fix shape:** treat a review-bead blocker as satisfied when its reviewed task is in `completed`, or when bd reports the dependent ready. When an invocation ends stalled with tracker-ready ids under an admission hold, name the hold in its exit and Detector line instead of reporting `0 ready`.

### 2. `split-oversized` labels children with the *first* `sp:` label, not the epic's `sp:<epic>` label
- **Evidence:**
- split-oversized:76 `sp=$(o '[.labels[]? | select(startswith("sp:"))][0] // ""')`.
- `bd show hg-k4l.7` labels: `['sp:demoted-by-session', 'sp:hg-k4l']`. `sp:demoted-by-session` is stamped by super-design §Promotion Review.
- Friction: "the coordinator's split of hg-k4l.7 into hg-k4l.23/.24/.25 (and review bead hg-k4l.41) dropped the sp:hg-k4l tree label, so the in-run ready query returned 0 ready and invocations 3 and 4 stalled with hg-k4l.24 ready; orchestrator restored the label".
- `review-bead` copies every `sp:` label, so the review bead inherited the loss.
- **Premise to verify:** bd returns labels sorted, so any `sp:` flag sorting before `sp:<epic id>` wins. `ready-in-tree`'s fast path filters on `--label sp:<epic>`.
- **Suggested fix shape:** select `sp:<root epic id>` explicitly, or copy all `sp:` labels the way `review-bead` does. Mirror the change in the JQ_UNAVAILABLE manual-filing text.

### 3. `remove-task-worktree` can never remove a task worktree in a repo with submodules
- **Evidence:**
- remove-task-worktree:108 `if ! git worktree remove "$wt" 2>/dev/null; then echo "kept: $wt — git worktree remove refused"; return 3`.
- Friction: "git worktree remove refuses worktrees containing (even deinitialised) submodules … orchestrator removed 20 verified-clean, fully merged task worktrees". Later rounds needed 4, 5 and 8 more manual removals.
- "each kept a ~50G target/"; "/var free fell 659G → 414G".
- Stale branches collided with later re-cuts: `Profile: rework — … hg-k4l.24 4m (SETUP_FAILED, SETUP_FAILED; never landed) · hg-k4l.17 2m (SETUP_FAILED; never landed)`. Blockers hg-8gz, hg-3w6 and hg-idv were filed and closed.
- **Premise to verify:** `git worktree remove` without `--force` refuses any worktree that contains submodules (git documents this).
- **Suggested fix shape:** once the script's own clean, merged and no-process checks pass, deinit submodules and remove with `--force`. The `--force` is gated by those checks, not unconditional.

### 4. `stop-run-processes` exits 4 when a Linux ancestor's `/proc/<pid>/cwd` is unreadable; only the lsof (macOS) branch has the uid-ownership escape
- **Evidence:**
- stop-run-processes ~line 126: `if ! acwd=$(readlink "/proc/$a/cwd" 2>/dev/null); then unknown_roots "ancestor $a cwd inspection failed"; exit 4; fi`. The `ancestor_uid != uid` skip exists only in the `else` (lsof) branch.
- Friction: "stop-run-processes exits 4 when it cannot inspect unrelated session processes (sshd-session, systemd --user, sd-pam); remove-task-worktree then keeps every task worktree and branch, and the stale branches collide with later stack cuts … invocation 2 stalled".
- **Premise to verify:** those ancestors are privileged or non-dumpable, so `readlink /proc/<pid>/cwd` fails even for the same uid. Defect 3 shares the symptom; separate the two causes when reproducing.
- **Suggested fix shape:** on the `/proc` path, exclude an ancestor whose cwd cannot be read when it is foreign-uid or non-dumpable, as the lsof branch already does. An unreadable ancestor is not a cleanup candidate.

### 5. `ledger-check` reports `landed≠completed` when its own ledger read failed
- **Evidence:**
- progress.md:871 `Metrics: ledger-check landed≠completed: 43 vs 2 — ledger-only [hg-k4l.1, … 41 ids] · … gaps: read-ledger: ledger transport malformed — missing or malformed "# ledger-digest:" header; read-ledger: Git task-landing lookup failed: Not reported. The already-merged command exited 0 and printed 43 lines, but the classifier stopped my response …`.
- `already-merged --landed` shows 43 distinct hg-k4l task landings, equal to the 43 distinct success-path `Merge:` ids. The earlier invocation's check said `ok`.
- **Premise to verify:** "completed 2" is the invocation-scoped count left after a failed tracker reconciliation.
- **Suggested fix shape:** when transport or Git lookup fails, print `ledger-check unavailable — <cause>`. Never print a mismatch computed from partial inputs.

### 6. One final-review cluster that was never produced degrades every later review permanently, and that cluster was the run's own artifact directory
- **Evidence:**
- Friction: "invocation 4 final review never produced cluster-2 (this run's own docs/superpowers/runs directory …); report-status --collect exit 3".
- progress.md: `Final review: split (inputs 391K > 40% of 200K) · clusters 23`, then six consecutive `Final review: bounded (captured source available; omission provenance unavailable)` lines (invocations 5 to 10).
- All four roast-pr reports: `review readiness: coverage bounded, provenance unavailable — basis: "… (inv-26c52189 cluster-2 never produced); inspected: all product paths … unreviewed: docs/superpowers/runs/** (436 run-artifact f…`.
- Friction: "[converged] unavailable for this loop".
- The run's final verdict: "not ready (unreviewed: whole-epic coverage and omission provenance remain incomplete)".
- **Premise to verify:** run artifacts under `docs/superpowers/runs/<run>/` are bookkeeping, not product under review. Every super-auto run commits them to its branch, so every run's final review includes them.
- **Suggested fix shape:** exclude the active run directory from final-review scope by construction. Let a later complete collection over the same scope supersede an earlier gap, so the degraded state does not stick.

### 7. The report drafter is told to Write `report.draft.md`, and the Claude Code harness refuses report-file writes from subagents
- **Evidence:**
- super-auto/report-prompt.md:7 "The drafter writes the whole report except the status block to `<run-dir>/report.draft.md`".
- Friction: "report drafter's Write was refused by the harness ('subagents should return findings as text'); orchestrator wrote report.draft.md from the drafter's returned text verbatim".
- **Premise to verify:** the refusal is general Claude Code subagent policy, not a local setting.
- **Suggested fix shape:** the drafter returns the draft as text and the orchestrator writes the file. That already worked as the fallback.

### 8. The disposition projection keeps findings from superseded final reviews as outstanding
- **Evidence:**
- `scope-dispositions --project`: `sources 53 · occurrences 631 · findings 550 · repaired 1 · rejected 0 · outstanding 549 · deferred 6 · unrouted 0 · gaps 0`.
- The ledger shows `durable attempts 8 · historical FIXED 8`, and about 12 fix beads landed (hg-k4l.45, .46, .54 to .58, .60, .61, .65, .66).
- Report: "The projection holds 630 outstanding finding groups, mostly from superseded final reviews". Ten invocations each ran a final review, split into up to 23 clusters.
- **Premise to verify:** a later final review over the same scope supersedes the earlier one's open findings.
- **Suggested fix shape:** mark a superseded review's findings superseded, or report them apart from live outstanding findings. Record a repair when a routed fix bead lands.

## Run metrics
### Judge panel
- roast-design r1: independence same-family (Claude), seat-differentiated panel · rung Workflow · `seat-agreement: panels 33 · rr 0.79 · rg 0.73 · fg 0.58 · unanimous 0.55 · ground-loo 0.69 (n=26)`
- roast-design r2: same-family · `panels 7 · rr 0.71 · rg 0.86 · fg 0.57 · unanimous 0.57 · ground-loo 0.80 (n=5)` · delta: 3 new, 9 resolved, 1 regressed
- roast-design r3: same-family · `panels 9 · rr 0.33 · rg 0.67 · fg 0.44 · unanimous 0.22 · ground-loo 0.67 (n=3)` · delta: 6 new, 4 resolved
- roast-pr r1: same-family · `panels 27 · rr 0.96 · rg 0.85 · fg 0.81 · unanimous 0.81 · ground-loo 0.85 (n=26)` · provenance unavailable
- roast-pr r2: same-family · `panels 2 · all 1.00 (n=2)` · delta: 1 new, 3 resolved, 1 regressed, 3 punch-listed
- roast-pr r3: same-family · `panels 3 · all 1.00 (n=3)` · delta: 1 new, 1 resolved, 1 regressed (a first attempt died on a usage limit; its "clean" verdict is void and it was re-run)
- roast-pr r4: same-family · `panels 2 · all 1.00 (n=2)` · delta: 2 new, 2 resolved · cap round
### Fix loop
- `Metrics: completions — review clean 41 · after fix pass 2 · parked 1 · re-entry closes 0 · dispatched early 23 · cancelled 0 · verification-only 1`
- `Metrics: fix-pass — entered 2 · FIXED 2 · BLOCKED 0 (legacy event lines); durable attempts 8 · historical FIXED 8; repaired findings unavailable`
### Merge-back
- `Metrics: merges 43 · merge-failed 2 · rebase-conflicts 6 · seam-reviews 12 (fixed 3) · check-fails 3 (fixed 1)`
- Final `ledger-check`: `landed≠completed: 43 vs 2` (false; see Defect 5). The previous invocation's check: `ok · append-retried 1 · gaps: Recovery gap: 741 …progress.md`.
- `Merge:` lines: 45 in total.
- rebase conflict: 6 (hg-k4l.13, .8, .6, .23, .27, .58)
- seam-review fixed: 3 (.16, .8, .21)
- seam-review cleared: 9
- check fail→fixed: 1 (.13)
- check fail → blocker: 2 (hg-k4l.1, submodule missing)
- Reconciliation: Git (`already-merged --landed`) shows 43 distinct hg-k4l task landings, and success-path `Merge:` lines show 43 distinct ids. Ledger-only: none. Git-only within hg-k4l: none.
- Git also lists 30 `hg-45e.*` landings from pre-run base history; these are not this run's.
- Verification-only completions: 1 (separate from landings).
### Coverage
- `coverage-round-1 · requirements: 16 · mapped: 16 · unmapped: 0` (R17 to R20 appended from r-new)
- `coverage-round-2 · findings 19 → 15 · novel 12/15 (80%) · widening: no`
- `scope-filter: 4 in-scope · 3 punch-listed` (round 1) · `scope-filter: 2 in-scope · 0 punch-listed` (round 2). Rounds 3 (redesign) and 4 (cap) have no scope-filter line.
### Timing
- inv1: `wall 37m … beads dispatched 1, landed 0` · `bound — no model: nothing landed` · bottleneck hg-k4l.1 13m (78%) · `context … coordinator 526K/1000K (53%), 0 compactions`
- inv2: `wall 1h33m … landed 18` · `bound — graph lower bound 38m … model of the run 58m … vs actual 1h01m (95%) → merge-lane-bound: 19m of the 20m above the graph's bound is the serial merge lane` · `merge lane — busy 48m of 57m (83%) · seam … 12m (26% of busy) · 26 merge dispatches, first try not merged for 8 of 18 · queue wait 1h49m total, peak 5 waiting` · waits: hot-file 15m (2) · what-if: unlimited merge lanes → 39m (−19m, 32%); seam off lane → 46m (−12m, 21%) · `concurrency … task-slot concurrency peak 7, cap 6` · coordinator 557K (56%)
- inv3: `wall 32m … landed 4` · graph-bound, 94% · planning 5m (28%) · coordinator 582K (58%)
- inv4, inv5: `Profile: unavailable — no timed task dispatch in the runs given (transcripts missing)` · coordinator 601K, 623K
- inv6: `wall 1h28m … landed 5` · graph-bound, 100% · bottleneck hg-k4l.22 57m (75%), `poll 35m` · what-if: faster hg-k4l.22 → −25m (33%) · coordinator 640K (64%)
- inv7: `wall 37m … landed 5` · graph-bound, 97% · planning 8m (30%) · merge lane 90% busy · coordinator 701K (70%)
- inv8: `wall 31m … landed 2` · bead-bound, 98% · planning 6m (28%) · merge lane 97% busy · coordinator 738K (74%)
- inv9: `wall 54m … landed 7 · verification-only 1` · graph-bound, 95% · `planning 16m (40%)` · runtime peak 12 of 16 · `peak≥80% 1 · coordinator 846K/1000K (85%)`
- inv10: `wall 41m … landed 2` · graph-bound, 99% · bottleneck hg-k4l.67 15m (58%) · coordinator 857K (86%), 0 compactions
- What-if figures are schedule-model estimates, not measurements. No compactions were recorded in any invocation.

### Bead graph
(Rows abbreviated. The 28 `review: <task>` beads are uniform: "Review, fix and merge of <task>, tracked separately so <task>'s dependents can start once it is implemented.")

| id | type | title | what (first sentence of description) |
| --- | --- | --- | --- |
| hg-k4l | epic | Seat failure path and manager seats (hg-qam, hg-au7, hg-3mc) | Root epic for super-auto run 2026-11-05-seat-failure-manager-seats. |
| hg-k4l.1 | task | Integrity error types: StoreError::Integrity, unresolved helper, IpcErrorCode::Integrity | Spec §4.1, §4.2. |
| hg-k4l.2 | task | cli::daemon_error: exit codes 5/6/2 for /seat, skill failure step, hook clause | Spec §4.4, §4.5. |
| hg-k4l.3 | task | Remove daemon-free fallbacks from who/ops/request/undo (hg-3mc) | Spec §4.13. |
| hg-k4l.4 | task | model::manager: manager pointer fields and the pure resolver | Spec §4.6. |
| hg-k4l.5 | task | plan::managers module skeleton and graph.toml write helper | Spec §4.7. |
| hg-k4l.6 | task | NodeManagerSet kind | Spec §4.7 (second bullet). |
| hg-k4l.7 | task | First seat becomes node manager (split into .23/.24/.25) | Spec §4.8. |
| hg-k4l.8 | task | Node manager retirement guard in make_plan | Spec §4.9 (plans bullet). |
| hg-k4l.9 | task | observe::classify: node manager closure reopens instead of retiring | Spec §4.9 (observation bullet). |
| hg-k4l.10 | task | Unassigned node-manager warning on every graph command | Spec §4.10. |
| hg-k4l.11 | task | seat.resolve manager fields and node-manager integrity suffix | Spec §4.3, §4.12, §4.5. |
| hg-k4l.12 | task | Integrity error: migrate seat.resolve references, make silent skips fatal | Spec §4.2. |
| hg-k4l.13 | task | ManagerEdit kind: plan manager set/clear | Spec §4.7, §4.6. |
| hg-k4l.14 | task | Shipped node-manager and office templates | Spec §4.8 (templates). |
| hg-k4l.15 | task | Reparenting on seat retirement (split into .26/.27) | Spec §4.11. |
| hg-k4l.16 | task | who and show render the manager chain | Spec §4.12. |
| hg-k4l.17 | task | Undo of a seat retirement restores reparented manager pointers | Spec §4.11. |
| hg-k4l.18 | task | Route every daemon-requiring command's client errors through cli::daemon_error | Spec §4.4. |
| hg-k4l.19 | task | seat.resolve reports list and manager duty in the seat skill | Spec §4.12 (reports). |
| hg-k4l.20 | task | Document manager seats, node manager rules and seat exit codes | Spec (whole). |
| hg-k4l.21 | task | make_plan refuses plans that would leave a manager chain cyclic | Spec §4.9. |
| hg-k4l.22 | task | Integration sweep: seat failure path and manager seats end to end | Spec. |
| hg-k4l.23 | task | First-seat helper, SeatCreate path and fixture opt-outs | Split from hg-k4l.7. |
| hg-k4l.24 | task | Application apply/hydrate and TemplateEdit additions use the first-seat helper | Split from hg-k4l.7. |
| hg-k4l.25 | task | SeatResurrect, TeamspaceResurrect and undo restore use the first-seat helper | Split from hg-k4l.7. |
| hg-k4l.26 | task | Apply-side reparenting in do_retire_seat with rewrite records on every route | Split from hg-k4l.15. |
| hg-k4l.27 | task | Plan previews list reparent_effects in every retire-effect builder | Split from hg-k4l.15. |
| hg-k4l.28 to .44, .47 to .53, .59, .72 to .77 | task | review: <task> (28 beads) | Review, fix and merge of <task>, tracked separately … |
| hg-k4l.45 | task | Fix fixtures broken by first-seat node-manager assignment (composition defect A) | Covers final review inv-17c351e9 composition defect A. |
| hg-k4l.46 | task | Inventory the graph.toml writer in the instruction-writer guard (composition defect B) | Covers final review inv-17c351e9 composition defect B. |
| hg-k4l.54 | task | Fix chained reparent rows on multi-seat retirement and their undo | Covers [Should-fix] src/undo/preview.rs:545, [Must-fix] inv-f2700d23 #1. |
| hg-k4l.55 | task | Cycle guard must catch cycles through retired seats in a teamspace default | Covers [Should-fix] cycle_guard.rs:119. |
| hg-k4l.56 | task | Teamspace retire must not revert its own default_manager reparent rewrite | Covers [Nit] core_kinds.rs:796. |
| hg-k4l.57 | task | Observed node-manager closure decided against a concurrent node-manager move | Covers [Nit] classify.rs:283. |
| hg-k4l.58 | task | Reopen the node manager when it is closed after an observed move | Covers [Must-fix] inv-f2700d23 #2. |
| hg-k4l.60 | task | Re-read the teamspace record after every reparenting seat loop | Covers [Should-fix] mutations.rs:242. |
| hg-k4l.61 | task | Node-manager closure: check pane survival before clearing clones | Covers [Should-fix] mutations.rs:209. |
| hg-k4l.62 | task | Per-clone survival decides observed node-manager closure | Owns classify's per-clone `survived` set (code redesign 1). |
| hg-k4l.63 | task | Reparent computes its target only when a live record names the retiring seat | Owns plan::managers::reparent's target computation. |
| hg-k4l.64 | task | Confirm landed reparent, undo and cycle-guard fixes with regression tests | Owns verification at head of .54/.55/.56. |
| hg-k4l.65 | task | Remove committed evidence logs | Owns the top-level evidence/ directory. |
| hg-k4l.66 | task | CHANGELOG Unreleased entries | Owns CHANGELOG.md `## Unreleased`. |
| hg-k4l.67 | task | Integration sweep: code redesign 1 end to end | Spec §4.9, §4.11 (amended). |
| hg-k4l.68 | task | Planner tab lookup excludes foreign-teamspace tabs | tab_for_seat's fallback must refuse a foreign-teamspace tab. |
| hg-k4l.69 | task | Node manager reopen tests for tab path and pre-set moved_out | Add required tests (4) and (5). |
| hg-k4l.70 | task | daemon_error row 1 only for NotInHerdr | Map only ClientError::NotInHerdr to exit 2. |
| hg-k4l.71 | task | Reparent live-holder set excludes co-retiring seats | Decision (orchestrator, option A from blocker hg-s2f): seats only. |

| dependent | blocker | reason (from `blocked-by` line, or `unstated`) |
| --- | --- | --- |
| hg-k4l.2 | hg-k4l.1 | consumes IpcErrorCode::Integrity |
| hg-k4l.3 | hg-k4l.2 | consumes cli::daemon_error |
| hg-k4l.4 | hg-k4l.1 | consumes StoreError::Integrity and the unresolved helper |
| hg-k4l.5 | hg-k4l.4 | consumes GraphRecord.node_manager_seat and model::manager |
| hg-k4l.6, .7, .13, .23, .24, .25 | hg-k4l.5 | consumes plan::managers graph.toml write helper |
| hg-k4l.8, .15, .21, .26, .27 | hg-k4l.5 | consumes plan::managers module |
| hg-k4l.9, .10, .16 | hg-k4l.4 | consumes GraphRecord.node_manager_seat / model::manager |
| hg-k4l.10 | hg-k4l.5 | consumes NODE_MANAGER_SET_USAGE |
| hg-k4l.11 | hg-k4l.2 / .12 / .4 | exit-6 wrapper text / seat_reply integrity classification / model::manager resolver |
| hg-k4l.12 | hg-k4l.1 | consumes StoreError::Integrity, unresolved helper and classifier |
| hg-k4l.14 | hg-k4l.7, .23, .24 | first-seat assignment; consumes split of hg-k4l.7 (.14 <- .25 cut mid-run by Edge cut) |
| hg-k4l.16 | hg-k4l.3 | consumes daemon-only who command path |
| hg-k4l.17 | hg-k4l.15, .26, .27 | reparent rewrites; consumes split of hg-k4l.15 |
| hg-k4l.17 | hg-k4l.44 (review bead) | triage: wait for hg-k4l.27's review/merge before re-cut |
| hg-k4l.18 | hg-k4l.2 | consumes cli::daemon_error |
| hg-k4l.19 | hg-k4l.11 / .14 | seat.resolve manager fields / node-manager AGENTS.md include_str! |
| hg-k4l.20 | hg-k4l.17, .6, .9, .18 | consumes landed behaviour for docs |
| hg-k4l.22 | 26 leaves (.1 to .21, .23 to .27) | consumes all leaves (integration sweep) |
| hg-k4l.22 | hg-k4l.45 | same test files; needs green full suite |
| hg-k4l.24 | hg-k4l.23 | calls the first-seat helper and fx_fresh fixture |
| hg-k4l.24 | hg-k4l.41 (review bead) | triage: hold until hg-k4l.41 merges hg-k4l.23 (the edge behind Defect 1) |
| hg-k4l.25 | hg-k4l.23 / .24 | first-seat helper / acceptance covers .24's paths |
| hg-k4l.27 | hg-k4l.26 | reparent_effects beside .26's Rewrite/reparent |
| hg-k4l.58 | hg-k4l.57 | closure decision moved into CascadeMutation |
| hg-k4l.67 | hg-k4l.62, .63, .64, .68, .69, .70, .71 | consumes all leaves (integration sweep) |
| hg-k4l.68 | hg-k4l.62 | LiveIndex::foreign_to predicate |
| hg-k4l.69 | hg-k4l.62, .68 | tests (4)/(5) need .62's survival contract and .68's fence exemption |
| hg-k4l.71 | hg-k4l.63 | live-holder set for .63's lazy target |
| each review bead | its reviewed task | unstated |

## Design questions

### A. Should seam review and fix run on the serial merge lane?
- **Evidence:** inv2 `merge-lane-bound: 19m of the 20m above the graph's bound is the serial merge lane`; `seam reviews and fixes 12m (26% of busy)`; `queue wait 1h49m total, peak 5 waiting`; what-if seam off lane → −12m (21%), unlimited lanes → −19m (32%), against a model-vs-actual of 95%. inv7 and inv8 merge lanes were 90% and 97% busy.
- **For moving seam work off the lane:** the lane only has to serialize the merge itself. Seam review of a rebased branch can run in parallel and be re-validated cheaply at merge time.
- **Against:** a seam review done before queueing can go stale if another merge lands first. Doing it on the lane guarantees it reviews the exact base. This run had 6 rebase conflicts in 45 merges, so staleness is not rare.

If upstream decides otherwise, please state the position explicitly so downstream can reconcile against words rather than silence.

### B. Should a relaunch replan only the residual graph incrementally?
- **Evidence:** `planning 16m (40% of first dispatch to last landing)` (inv9), `8m (30%)` (inv7), `6m (28%)` (inv8), `5m (28%)` (inv3). Eight of ten invocations were relaunches, most after stalls (Defects 1 to 4), with 2 to 7 beads left to land.
- **For incremental replanning:** most of the plan is unchanged across relaunches, so a delta plan would cut a fixed tax off every relaunch.
- **Against:** the full replan is what picks up amended requirements and triage comments (this run had a code redesign and new fix beads between invocations). An incremental path risks dispatching against stale plan rows.

If upstream decides otherwise, please state the position explicitly so downstream can reconcile against words rather than silence.

## Doc gaps
- Worktree setup (run and task worktrees) does not initialise submodules.
- Evidence: "run worktree created by plain `git worktree add` lacked the third_party/herdr-threads submodule; mergeCheck failed twice on hg-k4l.1 → blocker beads hg-9su/hg-9bp; orchestrator ran `git submodule update --init`".
- Invocation 1: `wall 37m · … beads dispatched 1, landed 0`.
- Suggested: a `.gitmodules` preflight plus `git submodule update --init` in worktree setup. This pairs with Defect 3 for submodule repos generally.

## Already fixed — do not re-litigate
- none. The coordinator.js:1564 change is a local scratchpad patch applied by this run's orchestrator. The installed coordinator.js is unchanged.

## Not established
- By default: the coordinator's ledger-append path is fire-and-forget and can lose a line without the coordinator noticing. Every ledger-derived count in `## Run metrics` (Fix loop, Merge-back) is therefore a lower bound. The `Metrics: ledger-check` line is the one cross-check that exists, and in this run's final invocation it was itself wrong (Defect 5).
- **500KB Workflow script limit hit by design-roast context:** reported by the orchestrator, but recorded in neither friction.md nor run.md. Scratchpad sizes are suggestive but not conclusive:
- roast-d1.mjs 369,374 bytes
- roast-d2.mjs 458,497 bytes
- roast-d3 was assembled with `tree-context3-small.md` (4.7KB) instead of `tree-context3.md` (48KB)

No measured failure is cited, so this is not filed as a finding.
- **Context-window classification:** run.md's context line warns that planner and finalReview run "on a 200K model", and final reviews split at "40% of 200K" (17 and 23 clusters). Yet every Profile line reads windows as `claude-opus-5-5 1000K` and the coordinator peaked at 857K/1000K. I could not establish whether subagents really had 200K windows, so I cannot say whether the splits (one of which produced Defect 6's gap) were needed.
- **Attribution of the invocation 3 to 5 stalls** is shared between Defect 1 (seeded edge), Defect 2 (label) and "hg-k4l.24 still in pendingRetry" (friction, invocation 3). Their individual contributions per invocation are not separated.
- The roast-code r3 first attempt died on an account usage limit. This is environmental and single-occurrence, so it is not filed.
- hg-k4l.22's 35m of polling (`toolbox run -c ht-build cargo fmt` 9m) and the flaky parallel test (hg-yu6) look project- or environment-specific. Not filed.
- The 30 `hg-45e.*` Git landings come from pre-run base history; I excluded them from reconciliation. Completed-ID reconciliation from the tracker was not run independently; only the ledger's own check line was read.
- None of the what-if savings were measured.

## Verification bar
- **Defect 1:** a coordinator harness replay. Use a retained ledger containing `Edge add: X <- <review bead>` with the review bead closed and bd reporting X ready. X must be admitted, and an equivalent in-run triage edge must be admitted too.
- **Defect 2:** run `split-oversized` on a bead labelled `sp:demoted-by-session` plus `sp:<epic>`. Every child must carry `sp:<epic>` and appear in `ready-in-tree <epic>`.
- **Defect 3:** in a scratch repo with a submodule, create a task worktree, merge it, deinit the submodule, then run `remove-task-worktree`. It must remove the worktree and branch, and it must still refuse a dirty or unmerged one.
- **Defect 4:** run `stop-run-processes` from a Linux login session (sshd, systemd --user ancestry). It must not exit 4 on unreadable foreign or non-dumpable ancestors, and it must still exit 4 when a candidate under the root cannot be inspected.
- **Defect 5:** feed `ledger-check` a malformed digest and a failed Git lookup. It must print "unavailable", not `landed≠completed`.
- **Defect 6:** a final review whose collection misses one cluster, followed by a complete collection. Review readiness must recover, and the run directory must not be in scope.
- **Defects 7 and 8:** a live super-auto finish under Claude Code, where the report drafter returns text only. Then run `scope-dispositions --project` across two final reviews of the same scope; superseded findings must not count as outstanding.
- One live multi-invocation super-code round on a repo with submodules would have caught Defects 1 to 4 together.

---
If a premise above is wrong, stop and say so rather than improvising a larger change.
