# 2026-10-02-herdr-graph-implementation: whole-epic review keeps finding composition defects that per-task and merge seam reviews miss

Plugin: superpowers 6.4.2-alepar4.6 (marketplace superpowers-alepar). Run: super-auto, 78 beads (20 plan leaves, 7 design-fix bookkeeping, 18 review beads, 32 fix tasks), 2 design-roast + 2 code-roast rounds, 7 super-code invocations, 129 commits / ~50.8k lines (Rust), 2026-10-02.

## Defects

### 1. Per-task review and merge seam-review missed composition defects; every pass's whole-epic review then cost a full extra pass
- **Evidence:** pass 1 `Metrics: merges 19 · merge-failed 0 · rebase-conflicts 0 · seam-reviews 0 (fixed 0) · check-fails 0 (fixed 0)`; every pass-1 `Merge:` line reads `seam-review none`. The whole-epic final review was "not ready" after pass 1 (9 must-fix), pass 2 (2 must-fix + 1 should-fix), pass 3 (4), pass 4 (2), fix-loop r1 (2) — each time on cross-task seams (session replacement × observer, deferred wake × other subsystems, undo adoption × reconciler live index, startup hook × daemon startup window). 7 coordinator invocations to converge.
- **Premise to verify:** the seam-review trigger is textual overlap / rebase-on-sibling-commits, not "consumes" dependency edges; the epic review is the first point composed behaviour is examined.
- **Suggested fix shape:** also trigger a composed/seam review when a merged task consumes an interface from another task merged in the same pass (the `blocked-by … consumes` lines), or run an interim whole-epic review at a mid-depth checkpoint (e.g. when a composition-root bead merges).

### 2. Finish metrics depend on an LLM `read-ledger` dispatch that provider safeguards rejected; 5 of 7 passes have no metrics
- **Evidence:** `[read-ledger:finish] failed: API Error: … safeguards flagged this message … [reasoning_extraction]` on sonnet, then opus; ledger lines `Metrics: UNAVAILABLE (…) — the Finish ledger re-read returned null; no counts derived` for every pass from pass 3 on.
- **Premise to verify:** all `Metrics:` counts are derivable by regex from structured `Merge:` / `Task N (…):` ledger lines; the rejection is triggered by the dispatch prompt/content.
- **Suggested fix shape:** compute Finish metrics in the coordinator script by parsing the ledger; keep any model dispatch optional with a parser fallback on null.

### 3. review-package BASE/HEAD taken from transcribed SHAs or cwd HEAD → pending-retry cycles and reviews of the wrong commit
- **Evidence:** `pending retry — … The BASE SHA in task-13-report.md … has 39 characters. A 'c' was dropped`; `pending retry — … review-package <PLAN_FILE> 2e3dcb3 HEAD, run while that worktree's HEAD was the integration branch`; minors: "Review package was built against commit 6a8c233 rather than HEAD because the sandbox would not let the shell cd into the task worktree" (twice).
- **Premise to verify:** the coordinator knows each task branch (`task-<bead>`) and its merge-base/stack parent.
- **Suggested fix shape:** resolve BASE/HEAD mechanically in the coordinator (`git -C`, full SHAs, branch refs); never accept SHAs or bare `HEAD` from agent prose.

### 4. Reviewers often cannot execute in the task worktree and accept the implementer's reported results
- **Evidence:** 16 `minor (deferred)` lines say results were "taken from report" / "not re-run" / blocked by the sandbox (e.g. "the full TEST_PATHSPECS git diff command was blocked by the sandbox"; "matcher.rs and the rebind-pass behavior were not read in full or traced; judged from the report"); 13 minors record no observed RED run; those tasks still read `review clean`.
- **Premise to verify:** the task's test command can be run via `git -C` / `--manifest-path` without `cd`.
- **Suggested fix shape:** coordinator runs the task's verify command itself and attaches the output to the review package; a verdict resting on "results taken from report" is recorded as unverified, not clean.

### 5. Merge check = default-feature compile; feature-gated test targets broke invisibly until the final sweep
- **Evidence:** six launches used `mergeCheck: "nice -n 10 cargo check --all-targets"`; every `Merge:` line `check pass`; first full sweep `FAIL @ 75b528b — default 617/0, test-support 597/2, private-herdr 636/2`.
- **Premise to verify:** super-auto/super-code can derive the build-feature/tag set the plan's test tiers use.
- **Suggested fix shape:** derive mergeCheck to compile every feature combination the test tiers use (cargo: `--all-features` or the tier list), or run the tier matrix's compile once per pass.

### 6. A task agent ran the product CLI against the real HOME; the refused cleanup quarantined a finished, green task as BLOCKED-AUTH
- **Evidence:** `BLOCKED-AUTH — … The task itself is implemented, tested and committed … every suite is green. I'm returning BLOCKED_AUTH only because of one refused cleanup command … rm ~/.config/<product>/config.toml … I ran <product> init /tmp/… --with-examples outside the private env.` Re-entry cost a pass; the leftover config would have pointed a later live install at a scratch instance.
- **Premise to verify:** isolating ad-hoc probes (temp HOME/XDG) is project-agnostic for CLIs that write user config; "work complete + out-of-tree side effect needs human cleanup" is distinguishable from "blocked".
- **Suggested fix shape:** implementer-brief rule: run the product binary only under a temp HOME/XDG; a distinct outcome (complete + human-action note) so the task merges instead of being quarantined.

### 7. Edge audit triggers by round index; under earlyUnblock it skipped the deep, under-width pass and fired on an empty graph
- **Evidence:** pass 1 `Detector: round 1 — 1 ready · topped-up 19 · cap 8 · peak in-flight 5 · … stacked 13 · idle slots 3` (depth 11) — no audit; the only audit: `Edge audit: round 2 — open leaves 0, depth 0, achievable width 0 vs cap 8; changes: none`.
- **Premise to verify:** the audit trigger is round-based; with earlyUnblock stacking, a whole pass completes in one round.
- **Suggested fix shape:** trigger on a sustained idle-slots / peak-vs-cap gap at top-up points; skip when open leaves = 0.

## Run metrics
### Judge panel
- design · iteration 1 of 3 · same-family (Claude) — seat-differentiated panel · `seat-agreement: panels 59 · rr 0.68 · rg 0.73 · fg 0.49 · unanimous 0.46 · ground-loo 0.68 (n=40) · reproduce 24/33/2 · refute 9/46/4 · ground 38/19/2`
- design · iteration 2 of 3 · same-family (Claude) · `seat-agreement: panels 22 · rr 0.59 · rg 0.77 · fg 0.45 · unanimous 0.41 · ground-loo 0.69 (n=13) · reproduce 14/7/1 · refute 6/14/2 · ground 18/3/1` · `[converged]`
- PR · iteration 1 of 3 · same-family (Claude) · `seat-agreement: panels 65 · rr 0.83 · rg 0.62 · fg 0.60 · unanimous 0.52 · ground-loo 0.63 (n=54) · reproduce 23/42/0 · refute 22/43/0 · ground 46/19/0`
- PR · iteration 2 of 3 · same-family (Claude) · `seat-agreement: panels 6 · rr 1.00 · rg 1.00 · fg 1.00 · unanimous 1.00 · ground-loo 1.00 (n=6) · reproduce 6/0/0 · refute 6/0/0 · ground 6/0/0` · `[converged]`
### Fix loop
- pass 1: `Metrics: completions — review clean 18 · after fix pass 1 · parked 0 · re-entry closes 0 · dispatched early 13 · cancelled 1`; `Metrics: fix-pass — entered 1 · FIXED 1 · BLOCKED 0`
- pass 2 (cumulative): `completions — review clean 26 · after fix pass 1 · parked 0 · re-entry closes 0 · dispatched early 13 · cancelled 1`; `fix-pass — entered 1 · FIXED 1 · BLOCKED 0`
- passes 3–7: UNAVAILABLE (defect 2)
### Merge-back
- pass 1: `Metrics: merges 19 · merge-failed 0 · rebase-conflicts 0 · seam-reviews 0 (fixed 0) · check-fails 0 (fixed 0)`; `ledger-check ok · append-failed 0 · append-retried 0`
- pass 2 (cumulative): `merges 27 · merge-failed 0 · rebase-conflicts 0 · seam-reviews 5 (fixed 1) · check-fails 0 (fixed 0)`; `ledger-check ok`
- all 57 `Merge:` lines: conflict 0 · seam-review cleared 16 · seam-review fixed 3 · check fail→fixed 0 · check fail 0 · → blocker 0
### Coverage
- coverage round 1: `requirements: 23 · mapped: 23 · unmapped: 0` (24 findings applied: 13 GAP, 11 UNOWNED-SEAM)
- coverage round 2: R1–R28 read back; `findings 22 → 15 · novel 13/15 (87%) · widening: no`
- code fix round 1: `scope-filter: 18 in-scope · 9 punch-listed`; round 2: `regressionPass-round-2: 2 filed · no re-roast`
### Bead graph
Plan leaves at launch (20): seam contract → store → writer → plan engine → reconciler → observer → undo → composition root → real-agent smoke → e2e flows → integration sweep (critical path, depth 11, width 1.8; parallelism pass applied 0, parked 2).

| dependent | blocker | reason (from `blocked-by` line) |
| --- | --- | --- |
| .2 | .1 | consumes model record schemas and Store trait |
| .3 | .2 | consumes Store tree-building helpers and committed reads |
| .6 | .3, .4 | consumes Writer Mutation registration API; daemon IPC command registry |
| .7 | .6, .5 | consumes plan effect types; FakeHerdr |
| .8 | .7 | consumes reconciler effect predictions |
| .10 | .8, .9 | consumes cascade / application action records |
| .18 | .3 .4 .7 .8 .9 .10 .11 .12 .13 .17 | composition root registers each component |
| .16 | .13 .12 .5 .18 | real-agent smoke over the composed daemon |
| .19 | .18 .10 .9 .5 .13 .15 .16 .17 .14 | e2e flows; matrix consumes smoke results |
| .20 | .1–.19 | all leaves (integration sweep) |

## Design questions

### A. Should `minor (deferred)` be allowed to carry known product defects, ignored tests and data-loss paths?
233 `minor (deferred)` lines were recorded; some were real defects (e.g. two e2e tests `#[ignore]`d for product defect D1, fixed only after a whole-epic review; a data-loss path in a directory move helper). The report contract does not surface minors. For a severity floor (ignored tests / data loss never minor) plus a count-only rollup: deferral currently equals loss. Against: 233 lines would swamp reports and fix loops; most are cosmetic. If upstream decides otherwise, please state the position explicitly so downstream can reconcile against words rather than silence.

### B. Should a sweep failure introduced by the sweep-fix pass itself get one regression-only pass?
The single sweep-fix pass fixed 4 failures but its re-run failed a test that was green in the previous sweep (`re-run FAIL … private-herdr 646/1 … reported as it stands (fix pass spent)`). For: mirrors the code roast's `[fix-regression]` regression-only pass; the run otherwise ends degraded on a self-inflicted failure. Against: risks unbounded loops; the report is transparent anyway. If upstream decides otherwise, please state the position explicitly so downstream can reconcile against words rather than silence.

## Doc gaps
- **Workflow refuses a coordinator `scriptPath` inside the plugin cache** ("not a readable/added dir"); the run copied `coordinator.js` into the session scratchpad. Document or script that copy (premise: reproduce on a fresh install).
- **No-remote repos:** native `EnterWorktree` bases on `origin/<default>`; a repo with no remote needs `git worktree add` + `EnterWorktree` by path + an ignore entry for the worktree dir. Document the fallback in pre-flight guidance.

## Already fixed — do not re-litigate
none

## Not established
- The coordinator's ledger-append path is fire-and-forget and can lose a line; every ledger-derived count above is a lower bound. Passes 3–7 have no `Metrics:` lines (defect 2), so their counts are reconstructed from `Merge:`/`Task` lines.
- That a mid-run composed review would have caught the seam defects earlier is an inference, not measured.
- The two remaining project test issues (a parallel-flaky undo test, one failing rename e2e) are project-level and not counted as skill findings.
- Single-run observations; analyst model: opus.

## Verification bar
- Multi-pass super-code on a fixture epic with a forced null `read-ledger` dispatch: metrics still populate.
- A task whose report contains a mangled SHA: the review diff is still correct.
- A cargo project with a feature-gated test target broken at merge: the merge check fails at that merge.
- An earlyUnblock run with depth ≥ 8 and peak < cap: the edge audit fires in that pass.
- A fixture epic where two tasks meet only through a consumed interface with a semantic bug: a seam/composed review fires before Finish.

---
If a premise above is wrong, stop and say so rather than improvising a larger change.
