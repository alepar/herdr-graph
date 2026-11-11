# Final review: cluster 12 (docs-reviews), epic hg-blw

Invocation inv-64d86974-dab5-4fe4-a772-e304978dabd1. Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298. I verified both with `git rev-parse --verify <sha>^{commit}`. `bind-review-source` exited 0 before this file was written, and the file did not exist before then. The caller checkout was at HEAD b9caa7f with an empty `git status --short` when the review started and again before this file was written. It was not switched, restored or edited. The review is static: all reads used `git show b9caa7f:<path>`. I made no worktree, ran no build and no tests, and started no rigs.

## Scope of this cluster

- 28 files under docs/superpowers/runs/2026-11-07-graph-state-inspection/reviews/ (slices 000020-000047). All are archived final-review run records:
  - invocation 1 (inv-4b650ab2…, head 6eb6cec): ten cluster reviews plus the seam review;
  - invocation 2 (inv-b9c53a9d…, head 2dfa656): sixteen cluster reviews plus the seam review.
- The files hold no code, CLI surface or acceptance criteria. No row of the design's `## Requirement baseline` (B1-B8, S1) is delivered by this cluster, so there is no production path to trace here. Baseline delivery belongs to the code clusters and the seam reviewer.
- Context I read: `bd show hg-blw --json` and `bd list --parent hg-blw --all`. Every child bead is closed and only the epic is open. I also read run.md at head.

## Integrity of the archived records

- Every one of the 28 files ends with the four terminal headings (Verdict, Must fix before landing, Untested scope, Deferred OK) and has exactly one `## Verdict`.
- One exception was already known: inv-4b650ab2 cluster-8 repeats its terminal sections. Its `## Must fix before landing`, `## Untested scope` and `## Deferred OK` each appear twice (lines 41-56 and 60-69). The inv-b9c53a9d seam review already recorded this, with its run.md double count, under Deferred OK.
- inv-4b650ab2 cluster-2 has a blank line between `## Verdict` and the verdict text. The verdict itself is present.
- The archived verdicts match the `finding-source` verdicts recorded in run.md (lines 34-44 and 48-64). The `review: not ready (…)` line under codeBuckets matches the inv-b9c53a9d seam verdict. So the records can be traced from the run log.
- Both invocations keep their assembled bundles (clusters.json, digest, slices) next to the reviews, so each verdict can be checked against the inputs it was given.
- The records quote the host account name. It appears in the toolbox error string and in absolute paths. inv-b9c53a9d already deferred this until the run records are published outside the repo. I found no credentials. The only `token` hits are `token: None` lines in quoted diffs.
- These are point-in-time records, and they should stay unedited. Each describes the branch as it stood at its own head. For example, inv-4b650ab2 says hg-blw.10 and hg-blw.11 had delivered nothing, and both landed later (111b9ae, 1163f89, ba17a28).

## Carried items from invocation 2 at the reviewed head

The archived inv-b9c53a9d seam review ends `not ready` with four Must fix items. I checked each one at b9caa7f. `git log 2dfa656..b9caa7f` has no commit touching src/daemon/compose.rs, src/daemon/roster_live.rs, src/inspect/loaded.rs, src/inspect/harness_files.rs, src/bootstrap/setup_claude.rs or the design doc. The src diff since 2dfa656 is 53 added help lines.

- Threads degraded state.
  - `LazyMap::inspect` (src/daemon/compose.rs:1113-1119) still leaves the slot alone on `Err(_) => {}` for every error except Rejected/Unsupported.
  - `roster_live::handle` (src/daemon/roster_live.rs:64-72) still decides availability from that slot alone.
- Codex form-differs note. src/inspect/loaded.rs:229-236 still sets `CODEX_FORM_NOTE` only when `stem(loaded) == stem(occupant)`. That cannot fire for a `rollout-<ts>-<uuid>` occupant stem against a bare payload id.
- User-level Claude file origin.
  - `discover_claude` (src/inspect/harness_files.rs:217-248) still walks the ancestors before it adds the user-level `CLAUDE.md`.
  - `Acc::add` uses `entry().or_insert`, so the first add wins. `$HOME/.claude/CLAUDE.md` therefore keeps `origin: ancestor` and no `env: inspector` whenever the cwd is under $HOME.
- Hook stdin read.
  - The installed Claude hook (src/bootstrap/setup_claude.rs:211) still starts with `p=$(cat); `, and `owned_hook_current` (line 169) still requires that prefix.
  - The design doc is also unchanged since 2dfa656, so the held-open never-blocks guarantee has been neither honored nor dropped.
- None of the four has a tracking bead (all hg-blw children are closed), a `parked:` entry, or any other disposition in run.md. The only record of them is in this cluster's archived files.
- The code defects themselves belong to the clusters that own those files and to the seam reviewer. From this cluster's side, the archived record says the branch is not ready, and nothing at head resolves or disposes of that.

## Verdict
not ready (all four Must fix items of the archived inv-b9c53a9d seam review are unchanged at head with no fix, bead or recorded disposition)

## Must fix before landing
- The four Must fix items of reviews/inv-b9c53a9d-0c9e-4100-99aa-2184b61f229c/final-review-inv-b9c53a9d-0c9e-4100-99aa-2184b61f229c.md are open at b9caa7f, and no commit since 2dfa656 touches their files: compose.rs:1113-1119 plus roster_live.rs:64-72 (threads unreachable reported as read), loaded.rs:229-236 (Codex form-differs note cannot fire), harness_files.rs:217-248 (first-wins `Acc::add` gives `$HOME/.claude/CLAUDE.md` the ancestor origin), and setup_claude.rs:211 (unbounded `p=$(cat)` against the design's never-blocks guarantee). Fix each one, or record a disposition for it (a bead, or a human scope change for the design guarantee), before landing; the owning code clusters and the seam reviewer hold the code fixes.

## Untested scope
- This review ran no build or tests; whether the carried items are still present is established by static reads at b9caa7f and by `git log 2dfa656..b9caa7f`, not by running a failing test.
- I did not re-check every line citation inside the 28 archived reports against their own heads (6eb6cec, 2dfa656); I checked only the terminal-section structure, the verdict agreement with run.md, and the four carried Must fix items.

## Deferred OK
- inv-4b650ab2 cluster-8 repeats its three terminal sections (lines 41-56 and 60-69); leave the archive unedited and correct the double count in run.md's finding-source record instead, as inv-b9c53a9d already deferred.
- The archived reviews quote the host account name in toolbox errors and absolute paths; redact before the run records are published outside the repo.
- inv-4b650ab2 cluster-2 puts a blank line between `## Verdict` and its verdict; this is harmless in a frozen record.
