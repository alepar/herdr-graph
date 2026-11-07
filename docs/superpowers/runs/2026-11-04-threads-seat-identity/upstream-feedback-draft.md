# 2026-11-04-threads-seat-identity: a final-review collection gap that cannot be cleared keeps every later review bounded and stops the roast loop from converging

Plugin: 6.4.2-alepar6.8 (skills @ 34fd607b). Run: 85 beads (84 tasks + epic; 69 closed, 16 open: epic, 3 external gates, 12 gated tasks), 11 super-code launches (10 profiled invocations, 9h26m summed wall), 3 design-roast rounds, 4 PR-roast rounds plus a post-cap audit (`roastCodeExit: capped`), 2026-11-04 → 2026-11-06.

## Defects

### 1. One over-window final-review cluster became a collection gap that nothing could clear. It made every later final review and roast round "bounded", so the roast loop could not converge even on a round with 0 candidates.
- **Evidence:**
  - Ledger, inv-4aa37d70: `Final review: split (inputs 282K > 40% of 200K) · clusters 21`. run.md collection: `not ready (… cluster 4 unreviewed; unreviewed: cluster 4 (runs-top): over the window)`. Parked: `final review cluster 4 (runs-top) unreviewed: over the context window; bundle file never written (collection gap, unrecoverable)`.
  - Cluster 4 is the run's own record directory. Bead hg-66u.71 sizes it: "about 258k tokens … run.md (~87k tokens) and profile-inv-cc29fc83.json (~58k) dominate". run.md is 486 KB in 186 lines, with single `finding-source` lines up to 45 KB.
  - All 6 later invocations (c9eda770, f287b835, ef2aebda, 923e6061, aae6b25f, 4daa8441, e33fc801, 2992d8ec) settled `not ready (unreviewed: whole-epic coverage and omission provenance remain incomplete)` with `assemblyArchive: failed-bounded-assembly-v1`. Each lists as Must-fix item 1: "Whole-range review incomplete: this pass is bounded (selection provenance unavailable; gaps run.md:72 inv-4aa37d70 …)".
  - Every PR roast (r1–r4 and the post-cap audit) carries `[low coverage] [review readiness incomplete]` with basis `collection gap: inv-4aa37d70 cluster-4 bundle missing`.
  - r4: `super-roast verdict: clean (0 nits)`, `raw 0 → deduped 0`, "`[converged]` is withheld because `convergenceEligible` is false" → `roastCodeExit: capped`, then a post-cap audit round.
  - The gap was actually reviewed. hg-66u.71 and hg-66u.72 ("Review cluster 4 part A/B … and the inv-4aa37d70 gap pass") landed review-clean (618c280..6c5af0a). Every later invocation and roast still cited the gap.
- **Premise to verify:** the provenance and readiness computation reads every retained collection record, and a gap stays `unrecoverable` with no record kind that can discharge it. Also verify whether the run directory belongs in the reviewed diff at all (`context:` shows finalReview on a 200K model; with 1M the cluster may have fit, but the cannot-clear part stands either way).
- **Suggested fix shape:** three options, each independent:
  - re-split an over-window cluster instead of declaring it unreviewed;
  - keep machine-generated run bookkeeping (run.md approvals JSON, profile JSON) out of final-review clusters, or review it by summary;
  - let a later recorded review covering the gap's paths discharge the gap for later provenance and convergence.

### 2. Every worktree cleanup in the run failed because `systemd --user`, `(sd-pam)` and `sshd-session`, all owned by the same user, hide their `/proc/<pid>/cwd`. 0 processes were stopped and retained task worktrees grew to 42.
- **Evidence:**
  - Every `Process sweep:` line reads `0 stopped · 0 survived · 48 unknown` (once `72 unknown`), with kept rising 13 → 14 → 17 → 18 → 32 → 36 → 37 → 0 → 39 → 42. Reasons: `processes 1950, 2008, 4145621, 4145802 cwd/executable/environment inspection failed` and `remove-task-worktree exit 4`.
  - run.md: `worktreesKept: 42 retained`, `processSweep: stopped 0 · survived 0 · unknown 48`.
  - On the host, those pids are `systemd --user`, `(sd-pam)` and two `sshd-session: alepar@notty`, all uid alepar. `readlink /proc/1950/cwd` → `Permission denied` (non-dumpable).
  - In `stop-run-processes`, the `/proc` loop does `stat` owner == uid, then `readlink "$d/cwd"`. On failure it calls `absent "$pid" || unknown_roots …` and exits 4. The uid-based exemption exists only for ancestors (the macOS comment).
- **Premise to verify:** a same-uid process whose `/proc` links are unreadable cannot have been started by the run, or could be shown safe another way (for example its start time is before the run, or it is not dumpable and not descended from the run).
- **Suggested fix shape:** classify same-uid, non-dumpable processes that are not descendants of the run as out of scope (report them, but do not count them as unknown), so a single login-session process cannot block every cleanup.

### 3. The report format repair is rejected by construction when the parser error is extra text after a `- none` marker: the only valid repair deletes a line, and repair-check (iv) requires equal line counts.
- **Evidence:**
  - friction.md: `report format repair rejected: …roast-design-2.md (repair-check (iv) line count differs; original stands; parser error: none marker has finding text in Escalations (need human))`.
  - Round 3 hit the same error (`trailing ---`), and the repair was not re-dispatched because it had already failed once.
  - `diff` between original and repair: the only change is the removed trailing `---` after `## Escalations (need human)` / `- none`.
  - Reproduced: `report-verdict --repair-check …design-2.md …design-2-format-repair.md` → `(iv) line count differs`, exit 4.
  - Both originals stand while still failing `--complete`.
- **Premise to verify:** the `---` comes from the reporter/template, not the orchestrator, and dropping a trailing horizontal rule or blank-equivalent line cannot change finding content.
- **Suggested fix shape:** either have `--complete` ignore a trailing thematic break after a none marker, or let repair-check (iv) also drop `---` lines before comparing. Also stop the reporter from emitting the trailing rule.

### 4. The Finish ledger re-read failed in 5 of 10 invocations. The well-formed digest is relayed through an LLM echo agent, which loses or garbles its header.
- **Evidence:**
  - Ledger lines 585–588, 602–605 and 747–750: `Metrics: UNAVAILABLE (merges|completions|fix-pass|ledger-check) — the Finish ledger re-read returned null or unavailable transport (malformed)`.
  - Lines 675 and 837: `ledger-check ok … gaps: read-ledger: ledger transport malformed — missing or malformed "# ledger-digest:" header`.
  - Run locally, `ledger-digest progress.md` prints a correct `# ledger-digest: 268 of 839 lines` header, 54,031 bytes.
  - Line 500 shows a related misclassification: `ledger-check landed≠completed: 29 vs 13 — ledger-only [16 ids]`. All 16 IDs (hg-66u.4, .5, .6, .7, .8, .11, …) landed in earlier invocations. Git (`already-merged --landed`) and the ledger's check-pass `Merge:` lines agree on the same 39 hg-66u task IDs, so this was not a real gap.
- **Premise to verify:** that the "mechanical agent that echoes it" (ledger-digest header comment) is the lossy step, not the script. That the line-500 mismatch compares a cumulative ledger set against a set scoped to one invocation.
- **Suggested fix shape:** read the digest from a file or tool output directly instead of through a model echo (or chunk and checksum it). Scope the completed-ID comparison to the same population as the ledger set.

### 5. Pre-flight resolved `mergeCheck` but never ran it. A host missing a C linker, and task worktrees with uninitialized submodules, were first found after implementation, at merge.
- **Evidence:**
  - friction.md: `super-code pre-flight: repo has no CI config; mergeCheck covers default and --all-features` (configured, not executed).
  - Then: `host has no C toolchain …; all 6 first-round merges failed mergeCheck; unplanned coordinator stop (in-flight work lost)`.
  - Ledger: `Recurring blocker: ×3 across 3 task(s) … merge check runs on host lacking C linker toolchain`, and `Slowness: every merge check failed 'host lacks C linker' (6 blockers) → unplanned stop`.
  - Second relaunch: `Slowness: hg-66u.4 merge check compiled against the stale submodule checkout → mergeCheck/sweep now run git submodule update --init first`. hg-66u.4 shows 4 `check fail → blocker` merges before it passed.
  - `Metrics: … check-fails 8 (fixed 0)`.
- **Premise to verify:** one `mergeCheck` run on the integration tip, in a worktree created the same way as task worktrees, costs less than one failed round. `git worktree add` does not initialize submodules for the merge agent today.
- **Suggested fix shape:** at pre-flight, run `mergeCheck` once in a fresh task-style worktree (with `git submodule update --init` when `.gitmodules` exists) and refuse to dispatch on failure.

### 6. The coordinator ended `ready-drained` while a review bead closed just after the round's readiness query, so an invocation had to be re-entered to dispatch work that was already ready.
- **Evidence:**
  - friction.md: `two invocations ended ready-drained/stalled while a review bead closed just after the round's readiness query (hg-66u.28 became ready after Finish began); re-entered each time`.
  - Detector: `round 2 — 0 ready · topped-up 0 · … review re-entries 2`.
  - The cost of each re-entry, from profiles: 5 of 10 invocations landed exactly 1 bead. The time after the last landing was 11–46m per invocation (190m of 566m summed wall, 34%), and each of those paid for a full Finish and final review.
- **Premise to verify:** the early-unblock review bead (`review-bead` split) can close after `bd-ready`/`bd-ready-recheck` returns empty and before Finish. The post-closure recheck does not cover review-bead closes.
- **Suggested fix shape:** before returning `ready-drained`, re-query readiness once after the last in-flight review or merge settles (or treat a pending review-bead close as in-flight).

### 7. Roast `finding-source` artifact paths were written relative to the repository root, but `scope-dispositions` resolves them relative to run.md's directory, so existing reports read as unavailable.
- **Evidence:**
  - `scope-dispositions --project run.md` (exit 3): `source artifact unavailable: docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-05-threads-seat-identity-roast-pr-2.md`, and the same for `roast-pr-1.md`. Both files exist.
  - The script resolves `path.resolve(path.dirname(run), s.artifact)`.
  - super-auto SKILL.md says to run `report-verdict --complete --source roast-code/round-<N> <report>` "so its `artifact` equals the path recorded in `roast-code:`", but gives no working directory. `roast-code:` records bare filenames.
  - Final-review sources (`reviews/inv-…/…md`, run-dir-relative) resolve fine.
- **Premise to verify:** `report-verdict` echoes its path argument verbatim into `artifact`.
- **Suggested fix shape:** have the helper write the artifact path relative to run.md's directory (or the projector also try the repository root), and state the working directory in SKILL.md.

## Run metrics
### Judge panel
- design r1 (1 of 3): independence `same-family (Claude) — seat-differentiated panel · rung: Workflow`; `seat-agreement: panels 41 · rr 0.78 · rg 0.76 · fg 0.63 · unanimous 0.59 · ground-loo 0.75 (n=32) · reproduce 9/29/3 · refute 5/34/2 · ground 17/21/3`
- design r2 (2 of 4): same independence; `panels 14 · rr 0.86 · rg 0.86 · fg 0.71 · unanimous 0.71 · ground-loo 0.83 (n=12) · reproduce 12/2/0 · refute 10/4/0 · ground 14/0/0`
- design r3 (3 of 4): same; `panels 12 · rr 0.58 · rg 0.83 · fg 0.58 · unanimous 0.50 · ground-loo 0.86 (n=7) · reproduce 9/3/0 · refute 4/8/0 · ground 9/3/0`
- pr r1 (1 of 3): same; `panels 22 · rr 0.64 · rg 0.77 · fg 0.59 · unanimous 0.50 · ground-loo 0.79 (n=14) · reproduce 7/13/2 · refute 5/17/0 · ground 11/10/1`
- pr r2 (2 of 4): same; `panels 5 · rr 1.00 · rg 0.80 · fg 0.80 · unanimous 0.80 · ground-loo 0.80 (n=5) · reproduce 4/1/0 · refute 4/1/0 · ground 5/0/0` (reporter failed on usage limit; engine fallback report)
- pr r3 (3 of 4): same; `panels 2 · rr 0.00 · rg 1.00 · fg 0.00 · unanimous 0.00 · ground-loo n/a (n=0) · reproduce 2/0/0 · refute 0/2/0 · ground 2/0/0`
- pr r4 (4 of 4): same; no seat-agreement line (0 panels)
- pr post-cap audit: same; `panels 5 · rr 0.60 · rg 0.80 · fg 0.80 · unanimous 0.60 · ground-loo 1.00 (n=3) · reproduce 4/1/0 · refute 4/1/0 · ground 5/0/0`
- Verdicts and deltas:
  - design r2: `11 new · 9 resolved · 1 regressed`
  - design r3: `6 new · 10 resolved · 2 regressed`, converged exit
  - pr r1: Should-fix (7)
  - pr r2: Should-fix (4)
  - pr r3: Nit (3), `2 new · 1 carried · 3 resolved`
  - pr r4: clean (0 nits), `1 resolved · 2 punch-listed`
  - audit: Should-fix (5)
  - All PR rounds: `[low coverage] [review readiness incomplete]`
### Fix loop
- Latest complete (ledger line 835–836): `Metrics: completions — review clean 35 · after fix pass 2 · parked 0 · re-entry closes 2 · dispatched early 10 · cancelled 0 · verification-only 1`; `Metrics: fix-pass — entered 2 · FIXED 2 · BLOCKED 0 (legacy event lines); durable attempts 10 · historical FIXED 9; repaired findings unavailable`.
- scope-dispositions (exit 3, partial): 51 sources (roast + final-review), 757 raw occurrences / 757 groups, of which 687 are findings and 70 other-section groups. Repaired 0 · rejected 0 · deferred 0 · outstanding 687 · unrouted 26 · gaps 28 (26 `scope-filter decision without finding-route`, 2 `source artifact unavailable`, see Defect 7).
- Durable fix history (9 FIXED) is not reflected as repaired in the projection.
- Task-review sources are absent from the projection (roast and final-review only).
### Merge-back
- `Metrics: merges 39 · merge-failed 8 · rebase-conflicts 3 · seam-reviews 16 (fixed 0) · check-fails 8 (fixed 0)`.
- Latest `ledger-check ok … gaps: read-ledger: ledger transport malformed`; 3 earlier invocations `UNAVAILABLE` (Defect 4).
- 47 `Merge:` lines: 39 `check pass`, 8 `check fail → blocker` (6 toolchain, 2 more for hg-66u.4 submodule); 3 with rebase conflict (1/1/2 files).
- Distinct task landings:
  - `already-merged --landed` and the ledger's check-pass `Merge:` set agree on 39 hg-66u task IDs. Ledger-only: none. Git-only: none (Git output also lists base-history hg-45e.* tasks, not this run).
  - Completed-only: hg-66u.65 (verification-only, no landing).
- `baseAbsorbed: 0dbe477 → eae4a3e, 0 conflicted files` (0 landings).
### Coverage
- coverage-round-1 `requirements: 14 · mapped: 14 · unmapped: 0`
- coverage-round-2 `requirements: 21 · mapped: 21 · unmapped: 0 · divergence: findings: 19 → 20 novel: 12/20 (60%) widening: yes`
- coverage-amend-design-1 `requirements: 21 · mapped: 21 · unmapped: 0`
- coverage-amend-code-1 `requirements: 21 · mapped: 21 · unmapped: 0`
- scope-filter:
  - r1: no line
  - r2: `9 in-scope · 1 punch-listed` (6 `defaulted in-scope`)
  - r3: `14 in-scope · 2 punch-listed` (13 `defaulted in-scope`)
### Timing
Per invocation, chronological (header wall / task graph / after last landing; bound model vs actual; bottleneck; waits; merge lane; coordinator context):
- cc29fc83: wall 1h54m / 1h08m / 46m.
  - bound: bead-bound, model 1h05m vs 1h08m (96%).
  - bottleneck: hg-66u.37 16m (23%).
  - waits: 1m.
  - merge lane: busy 32m of 51m, seam 43% of busy, 26 dispatches, stack-parent wait 16m.
  - coordinator 569K/1000K.
  - what-if: none >1%.
- 4aa37d70: 51m / 14m / 37m.
  - bound: 13m vs 14m (94%).
  - bottleneck: hg-66u.6 8m.
  - merge lane: busy 76%.
  - coordinator 589K.
  - what-if: unlimited merge lanes −1m.
- c9eda770: 54m / 43m / 11m.
  - bound: graph-bound .28→.30→.29, 42m vs 43m (98%).
  - bottleneck: hg-66u.29 20m.
  - coordinator 603K.
  - what-if: cut .29<-.30 −10m.
- f287b835: 44m / 32m / 11m.
  - bound: 32m vs 32m (100%).
  - bottleneck: hg-66u.26 29m (89%).
  - coordinator 613K.
  - what-if: −13m.
- ef2aebda: 1h20m / 1h07m / 13m.
  - bound: graph-bound .58→.70, 1h04m vs 1h07m (95%).
  - bottleneck: hg-66u.70 39m.
  - waits: slot 22m (6).
  - merge lane: queue wait 1h16m total, peak 5.
  - coordinator 701K.
  - what-if: −11m.
- 923e6061: 50m / 32m / 18m.
  - bound: 31m vs 32m (97%).
  - bottleneck: hg-66u.75 10m.
  - coordinator 756K.
  - what-if: −2m.
- aae6b25f: 36m / 22m / 14m.
  - bound: 22m vs 22m (99%).
  - bottleneck: hg-66u.80 12m.
  - coordinator 767K.
  - what-if: −4m.
- 4daa8441: 43m / 28m / 15m.
  - bound: 28m vs 28m (100%).
  - bottleneck: hg-66u.81 14m.
  - coordinator 803K (80%).
  - what-if: −6m.
- e33fc801: 33m / 20m / 12m.
  - bound: 20m vs 20m (99%).
  - bottleneck: hg-66u.17 14m.
  - coordinator 823K (82%).
  - what-if: −6m.
- 2992d8ec: 1h01m / 48m / 13m.
  - bound: 29m vs 48m (61%).
  - bottleneck: hg-66u.83 34m (poll 28m).
  - waits: retry 30m.
  - coordinator 852K (85%).
  - what-if: −7m.

All invocations: 0 agent compactions and 0 coordinator compactions. What-if lines are schedule-model estimates. Model-vs-actual was 94–100% except 2992d8ec (61%).

### Bead graph
| id | type | title | what (first sentence of description) |
| --- | --- | --- | --- |
| hg-66u | epic | Threads seat as the clone identity (graph side) | Root epic for super-auto run 2026-11-04-threads-seat-identity. |
| hg-66u.1/.2/.3 | task (open) | Gate: herdr-threads D1 / D2 / G5 released | Gate bead: closed by the super-auto orchestrator when the herdr-threads coordinator announces the release. |
| hg-66u.4 | task | Bump herdr-threads to v3.0.0 | Spec §1. |
| hg-66u.5 | task | /seat drops accept_command; skill delegates acceptance | Spec §4. |
| hg-66u.6 | task | Seat contract: ThreadsSeatView port and SeatStatus | Spec §6. |
| hg-66u.7 | task | Clone threads_seat: write-once assignment and resolve-seat want | Spec §2. |
| hg-66u.8 | task | Participation keyed to the clone's threads seat (D3) | Spec §3. |
| hg-66u.9/.10/.12/.16/.19/.20/.22 | task (closed, superseded) | continuity-key launch / SeatObservation / resume driver / unretire / assignment / succession / same-seat restore | Spec sections; replaced by .27–.34 rewrite. |
| hg-66u.11 | task | Tabs and workspaces match by preserved Herdr ids | Spec §7. |
| hg-66u.13 | task (closed, split) | Reserve continuity keys and surface key conflicts (D1) | Spec §5, §9, §10. |
| hg-66u.14 | task (open) | Delete the graph's clone re-matching | Spec §6. |
| hg-66u.15 | task (open) | Remove /seat rebind proposals and clone rebind --pane | Spec §6. |
| hg-66u.17 | task | Configuration smoke on a private rig (pre-D1) | Spec §11. |
| hg-66u.18 | task (open) | Configuration smoke: fresh agents, pane closed during downtime, undo | Spec §11. |
| hg-66u.21 | task (open) | Resume driver: fresh-agent path after cold restart (D1) | Spec §9. |
| hg-66u.23 | task | Parity check: old matcher scenarios vs SeatObservation | Spec §6, requirement B3. |
| hg-66u.24 | task (open) | Honour herdr-threads G5 limbo disposition | Spec §10. |
| hg-66u.25 | task | Graph daemon reconnects across herdr update --handoff | Spec §1, requirement B5. |
| hg-66u.26 | task | Integration sweep (ungated tree) | Root integration sweep. |
| hg-66u.27 | task | ThreadsSeatAssignment and SeatIntent | Owns CloneRecord.threads_seat and seat_intent. |
| hg-66u.28 | task | SeatObservation: intent-gated Retired arm, pre-D1 precedence | Owns SeatObservation. |
| hg-66u.29 | task | Restore intent drives seat succession (pre-D2) | Owns undo/resurrection commits writing Restore. |
| hg-66u.30 | task | Resume driver: single launcher for restart and restore | Owns the planner resume rule. |
| hg-66u.31/.32/.33 | task (open) | Unretire/finalize wants; same-seat restore; unseated clone resolves seat | Gated on D2 / D2 / D1. |
| hg-66u.34 | task (closed, split) | Launch agents with the continuity key env | Owns HERDR_THREADS_CONTINUITY_KEY. |
| hg-66u.35 | task | Clone purge command | Spec §14.1-3. |
| hg-66u.36 | task | Gate: fix loop exited | Gate bead (final-SHA evidence). |
| hg-66u.37/.38 | task | Continuity-key env builder; export rejection in status | Split from hg-66u.34 (~143K vs ~167K usable window). |
| hg-66u.39 | task (closed, split) | Continuity key reservation, keyed launches, backfill | Split from hg-66u.13 (~175K). |
| hg-66u.40/.43/.44/.68 | task (open) | KeyConflict; key reservation; keyed launches; key-link want | Split from .13/.39; gated on D1. |
| hg-66u.41–.56, .73, .74 | task | review: <task> | Review, fix and merge of <task>, tracked separately so dependents start early. |
| hg-66u.57–.66, .69, .70, .75, .77–.84 | task | fix-loop and sweep-fix beads (purge, stuck restores, refusals, guards, docs, tests) | "Owns …" / "Covers: <finding key>". |
| hg-66u.67 | task (closed, split) | Review the unreviewed run documents and bounded-range gaps | Owns completing the bounded final review: cluster 4. |
| hg-66u.71/.72 | task | Review cluster 4 part A / part B and the inv-4aa37d70 gap pass | Split from hg-66u.67: read surface ~258k tokens. |
| hg-66u.76 | task (closed, split) | SeatObservation acts destructively only on genuine per-seat answers | Covers r2 findings; split to .79/.80. |

| dependent | blocker | reason (from `blocked-by` line, or `unstated`) |
| --- | --- | --- |
| hg-66u.6 | .4 | stated |
| hg-66u.7 | .4, .27 | stated |
| hg-66u.8 | .7, .27 | stated |
| hg-66u.10 | .6, .19 | stated |
| hg-66u.12 | .9, .10 | stated |
| hg-66u.13 | .1, .6, .30, .35 / .7 | stated / unstated |
| hg-66u.14 | .1, .11, .13, .23, .33, .39, .40, .43, .44 | stated |
| hg-66u.15 | .1, .13, .33, .39, .40, .43, .44 | stated |
| hg-66u.16 | .2, .13, .39, .40, .43, .44 | stated |
| hg-66u.17 | .7, .8, .11, .25, .28, .30, .36 | stated |
| hg-66u.18 | .3, .14, .21, .24, .32, .33, .36, .68 | stated |
| hg-66u.20 | .7, .10 | stated |
| hg-66u.21 | .13, .30, .39, .40, .43, .44, .58 | stated |
| hg-66u.22 | .16 | stated |
| hg-66u.23 | .7, .28 | stated |
| hg-66u.24 | .3, .28, .30 | stated |
| hg-66u.26 | .4, .5, .6, .7, .8, .11, .23, .25, .27, .28, .29, .30, .34, .35, .37, .38 | stated |
| hg-66u.28 | .6, .25, .27, .52 | stated |
| hg-66u.29 | .7, .27, .28, .30 | stated |
| hg-66u.30 | .27, .28, .34, .35, .37, .38 | stated |
| hg-66u.31 | .2, .13, .27, .35, .39, .40, .43, .57 | stated |
| hg-66u.32 | .29, .30, .31, .68 | stated |
| hg-66u.33 | .1, .7, .13, .27, .30, .39, .40, .43, .44 | stated |
| hg-66u.35 | .27 | stated |
| hg-66u.38 | .37 | stated |
| hg-66u.39 | .1, .6, .7, .29, .30 | stated |
| hg-66u.40 | .1, .6, .7, .30, .39, .43 | stated |
| hg-66u.43 | .1, .6, .7, .29, .30 | stated |
| hg-66u.44 | .1, .6, .7, .29, .30, .31, .43 | stated |
| hg-66u.68 | .1, .43, .44 | stated |
| hg-66u.70 | .58 | stated |
| review beads .41–.56, .73, .74 | their task | unstated |

## Design questions

### A. Should a finding whose fix is an existing open bead behind an external gate keep being routed in-scope Must-fix every round?
- **Evidence:**
  - scope-filter r2 defaulted 6 rows to in-scope, and r3 defaulted 13: `in-scope — unrouted by filter — defaulted in-scope`. Examples: "B9 not delivered … hg-66u.43/.44/.68/.40 open (D1)" and "B7 … hg-66u.31/.32 open behind gate hg-66u.2 (D2)".
  - The same gated rows are the Must-fix lists of all 8 final reviews after the first, and the run ended `capped`.
  - The edge audit agreed: "the whole critical path … sits behind the external D1 gate hg-66u.1".
- **For keeping them in scope:** they are real, undelivered requirements. Demoting them risks a "done" report that quietly drops baseline rows, which the run's own coverage rules ("hand it back open") exist to stop.
- **For routing them elsewhere:** they cannot change within the run. Re-raising them each round spends judge/fix capacity, inflates Must-fix counts, and makes convergence impossible. Routing to the owning gated bead (an "owned, gate-blocked" disposition carried into the hand-back) keeps them visible without blocking convergence.

If upstream decides otherwise, please state the position explicitly so downstream can reconcile against words rather than silence.

## Doc gaps
- super-auto SKILL.md (roast bookkeeping, around line 401) does not say which working directory to run `report-verdict --complete --source …` from, or that `artifact` must be relative to run.md's directory (Defect 7).
- No guidance on keeping run-record artifacts (run.md approvals JSON, profile JSON) out of final-review clusters, although they are committed on the integration branch and grow with every invocation (Defect 1).

## Already fixed — do not re-litigate
none known

## Not established
- By default: the coordinator's ledger-append path is fire-and-forget and can lose a line without the coordinator noticing, so every ledger-derived count in `## Run metrics` (Fix loop, Merge-back) is a lower bound. The `Metrics: ledger-check` line is the one cross-check that exists, and here it was itself unavailable or malformed in 5 of 10 invocations.
- The analysis was done by a single fresh-context agent (Claude Opus 5.5, model ID claude-opus-5-5) that also triaged and drafted. No separate analyst dispatch ran.
- Defect 6 (ready-drain race) rests on one friction line and one Detector line. The exact close time against query time was not measured.
- Defect 1's re-split fix might be unnecessary if finalReview ran on a 1M-context model (`context:` warned `finalReview on a 200K model`). The part where the gap cannot be cleared stands either way.
- The 34% after-last-landing share is summed from profile headers. Not every minute of it is final review.
- Dropped as project- or environment-specific, or as single flukes:
  - leaked private test-rig `herdr server` processes exhausting the toolbox pids cgroup (the project's tests);
  - the rustc ICE (EAGAIN);
  - the roast-code r2 reporter failing on an account usage limit;
  - a claimed block on subagent report drafters writing files (no evidence found in the sources).
- friction.md contains 4 stray `Profile:` lines (what-if/context/unmeasured) among its event lines. Which writer put them there was not established.
- Roast rounds r3/r4 and the design rounds have no retained `finding-source` records. Whether this is coordinator non-compliance or a skill gap was not established.

## Verification bar
- Defect 1: a fixture run whose final review has one cluster over the window. Assert either that it is re-split, or that a later recorded review of its paths clears `provenance unavailable` and lets a 0-candidate roast round reach `[converged]`.
- Defect 2: run `stop-run-processes` on a systemd Linux host with an active ssh session and confirm exit 0 with `systemd --user`/`sd-pam`/`sshd-session` reported but not `unknown`.
- Defect 3: `report-verdict --repair-check` on this run's `roast-design-2.md` / `-format-repair.md` pair must exit 0, or `--complete` must accept the original.
- Defect 4: have Finish read a 54 KB / 268-line digest 10 times through the real transport, with zero malformed headers.
- Defect 5: pre-flight on a host without `cc`, and on a repo with a submodule, must stop before the first dispatch.
- Defect 7: running `scope-dispositions --project` on this run.md must report 0 `source artifact unavailable` gaps.

---
If a premise above is wrong, stop and say so rather than improvising a larger change.
