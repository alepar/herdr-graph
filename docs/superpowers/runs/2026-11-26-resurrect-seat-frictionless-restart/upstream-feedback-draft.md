# 2026-11-26-resurrect-seat-frictionless-restart: final review splits on committed evidence logs and run records, sized against a wrong context window

Plugin: 6.4.2-alepar6.9. Run: 25 task landings over 3 super-code invocations (ordinary-subagent mechanism); 2 design-roast rounds, 2 code-roast rounds plus a post-cap audit (all manual fan-out), 3 final reviews, a phase-6 sweep with 2 sweep-fix beads. Wall time about 12h, 356 timed agents, 2026-11-26 to 2026-11-27. Status: parked draft, not filed (autonomous run).

## Defects

### 1. Final-review assembly sizes and clusters the skills' own run records and committed evidence logs
- Evidence: inv-1 inputs 638K split into 20 clusters, 15 of them evidence logs only; inv-2 876K into 34 clusters (one run-records cluster over the window); inv-3 1048K into 34 clusters. The branch diff was ~248K lines, of which ~208K were docs/superpowers/runs and ~33K evidence/; product code was ~5.4K src and ~2.5K tests. Task reviews flagged `evidence-in-repo` / `committed-evidence-logs` too.
- Fix shape: exclude run-record and evidence paths from final-review sizing and clustering (or fold them into one summary cluster); decide whether evidence logs belong on the feature branch at all.

### 2. Preflight, final-review threshold and profile disagree on the context window
- Evidence: preflight warned `finalReview=200K ... on a 200K model`; the profile's model table said 1000K for the same models, and the coordinator itself peaked at 807K/1000K. Final review inv-1 and inv-2 were sized against a 258K fallback; inv-3 against 1000K.
- Fix shape: one window source (the model table) for preflight, final-review sizing and profile; say "unknown" instead of a false 200K warning.

### 3. A post-loop final-review Must-fix can go unrouted while the headline says "0 unresolved Blocking"
- Evidence: the last final review's Must-fix (a correctness gap in a goal path) arrived after the one allowed post-loop fix round was spent; no finding-route exists for it, `parked: none`, and report-status counts 0 unresolved Blocking.
- Fix shape: route any later final-review Must-fix to parked/escalated automatically, and count unrouted Must-fix items in the status headline.

### 4. scope-dispositions projection reports false gaps on a normal run
- Evidence: exit 3 with escalation keys outside `[SEV]` form; roast-pr-1/2 reported "source artifact unavailable" although present in the run dir; landed `fix:` routes not counted as dispositions (381 findings "outstanding"); findings ledgers never advance past r1.
- Fix shape: accept severity-less escalation keys, resolve artifact paths relative to the run dir, treat a landed fix route as a disposition, rebuild the ledger each round; add this run.md as a fixture.

### 5. Format check and repair-check leave no valid repair when text follows `- none` in the last section
- Evidence: happened twice (design roast r1 with a trailing `---`, post-cap audit with a trailing prose note); both repairs were rejected by repair-check (iv), so the original stood failing the format check.
- Fix shape: let a layout-only repair move trailing non-finding text out of the section, or make the format check ignore a horizontal rule / section-external note; give the reporter a place for "prior findings outside the recognized sections".

### 6. Merge check runs against a stale submodule checkout after a gitlink bump
- Evidence: a pin-bump task merged with exit 0, then mergeCheck compiled against the old submodule checkout and cargo rewrote Cargo.lock; the coordinator re-merged with `git submodule update` and kept that prefix for the rest of the run.
- Fix shape: run `git submodule update --init --recursive` after each merge, before mergeCheck, when `.gitmodules` exists.

### 7. Task-worktree removal fails for every worktree in a repo with a submodule
- Evidence: `git worktree remove` refused all 25 task worktrees (initialized submodule); the coordinator force-removed each after checking status and merge state.
- Fix shape: in remove-task-worktree, pass `--force` once its own clean/merged checks pass and the submodule is the only blocker.

### 8. Cleanup helpers exit 4 when an ancestor of the temp root holds a stray `.git`
- Evidence: an empty `/tmp/.git` (not created by the run) made `stop-run-processes` and `remove-task-worktree` fail "git worktree inventory failed" for every `/tmp/...` temp root, all run long; processSweep could only report `unknown`. Separately, stop-run-processes on a whole task worktree once stopped the build container's conmon (its cwd was the worktree).
- Fix shape: run the inventory with `git -C <repo>` or `GIT_CEILING_DIRECTORIES`; exclude container runtime processes from the match.

### 9. Default test pathspecs miss Rust inline test modules
- Evidence: task reviews hit `test-pathspec-gap` repeatedly; `src/**/tests.rs` and `*_tests.rs` diffs had to be read by hand.
- Fix shape: add those to the default Rust pathspecs, or warn when a diff touches `#[cfg(test)]` files outside them.

### 10. Profile bound/lever lines ignore an external gate that dominated the run
- Evidence: 7h55m of an 11h55m critical path was pre-availability waiting on an external release gate, yet the profiles said "graph-bound: reshape" and "bead-bound: split hg-lnw.N"; the bottleneck line named a bead off that path.
- Fix shape: classify externally closed gates as `external-bound`, suppress reshape/split levers then, and derive the bottleneck from the same path as the bound.

## Design questions

### 11. Should code that lands after its last review get one?
- Evidence: a docs fix pass merged after NEEDS_FIX without re-review; two sweep-fix beads (one in the shared observe loop) merged after the last final review and roast.
- For: those are exactly the changes a final review exists to see. Against: extra rounds per run; naming "unreviewed after fix" in the status qualifiers may be enough.
- If upstream decides otherwise, please state the position explicitly.

### 12. Manual fan-out cost when Workflow is not authorized
- Evidence: ~300 dispatches for roasts and final reviews ran through the coordinator's context (peak 81%, one compaction) because the user had not opted into Workflow.
- Question: make the manual rung cheaper (batched judges, compact cluster verdicts, fewer per-evidence clusters) rather than infer consent.

## Doc gaps

### 13. Report drafter is told to write a file, which Claude Code subagents are refused
- Evidence: the drafter's write of `report.draft.md` was refused ("Subagents should return findings as text"); the orchestrator wrote the returned text.
- Fix shape: drafter returns the draft as text; the orchestrator writes the file.

## Not established
- No measurement shows the skipped/unreviewed clusters hid a defect; the cost claim stands, quality impact is unmeasured.
- #10 attributes the pre-availability time to the external gate from the detector's gated list; the profiler does not attribute it itself.
- Each finding is a single-run observation except #5 (twice in this run). Ledger-derived counts are lower bounds.
- Flaky project tests are project-specific and not filed.
