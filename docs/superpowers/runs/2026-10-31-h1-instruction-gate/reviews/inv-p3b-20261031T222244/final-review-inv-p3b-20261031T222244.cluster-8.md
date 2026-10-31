# Final review — cluster 8 (src-plan), epic hg-45e

Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify`). Review source bound via `dispatch-text.mjs bind-review-source` (exit 0).

Files in scope: src/plan/{commands,core_kinds,instructions,kinds_extra,line_diff,mod,tests,types}.rs (slices 000059–000066). Spec: docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md §2.1–§2.5, §2.2a, §4.4.

## Verification performed

- Created a review-owned detached worktree at the head (`.worktrees/review-c8-inv-p3b-20261031T222244`), initialised the submodule from the local main checkout, and seeded `target/` by reflink copy. `cargo test --locked --offline --lib plan::` gave 126 passed, 0 failed.
- Added two throwaway probe tests in that worktree only, ran them, then removed the worktree with `git worktree remove --force`:
  - Probe 1: a non-UTF-8 seat `AGENTS.md` committed directly with git2, then `instructions clear seat a1 AGENTS.md`. The plan succeeds. The confirmed apply ends `state=Failed` with `mutation instructions_edit: teamspaces/alpha/seats/a1/AGENTS.md is not UTF-8`. Nothing is written and the writer does not halt (`fail_op`).
  - Probe 2: AGENTS.md goes from `a\nb\n` to `a\r\nb\r\n`. The effect's `diff` is `""`, so `render` prints no diff block. The effect line shows only differing `before_sha` and `after_sha`.
- Caller checkout state after the review: HEAD bce6589, `git status --short` empty. Nothing in the caller checkout changed apart from the authorised ledger binding.

## Spec conformance (cluster on its own terms)

- §2.1/§2.2: `InstructionsKind` parses `set|clear <scope> [<target>] <file> [--from <path>]` through `instruction_source::parse_words` (closed file set, name rule). `--from` is resolved against `caller.cwd`, like `seat override --section`. `set` without `--from` and `clear` with `--from` are usage errors. `no change` and `no such instruction file` are plan errors (tested). Retired targets reject `retired_object` (tested for a seat).
- §2.2a: the content check runs at parse time and again in `plan()`. `seat override --section` now runs `check_content` with the section name and line (tested for a tag char and a bare CR).
- §2.3: the `global` effect object is the graph id (tested).
- §2.4: the effect is `instructions.set|clear` with `{file, before_sha, after_sha, diff}` and `absent` sentinels. Staleness uses the generic `same_effects` recompute (the stale test passes). The diff is an in-crate LCS with 3 lines of context (unit-tested: empty sides, middle change, hunk merge and split, no trailing newline). `render` prints the diff as an indented block, and `describe` drops the `diff` key (tested). The affected-seats warning text matches the spec exactly, and per scope it covers all live seats, the teamspace's live seats, or the seat itself (tested).
- §2.5: the mutation writes an `InstructionsEdit` action. `affected[0]` is the target, and `compensation` is `{file, before?, after?}` as full text, with a key omitted when that side is absent (tested for set and clear).
- §4.4: plan-time cap warnings use a `PatchedTree` of the edit for each affected seat, which is more exact than the spec's "byte delta" estimate. `seat override --section` sizes only when sections are set (tested above and below the cap).
- Registration: `InstructionsKind` is registered once in `register_core_kinds`.

## Findings

- Plan/apply inconsistency on a non-UTF-8 current file (confirmed by probe 1). `plan()` reads the current file with `from_utf8_lossy` and accepts the plan. `mutate()` turns the same bytes into `MutationError::Bug`, so a user-approved `set`/`clear` ends as a Failed op flagged as a bug instead of a plan-time rejection. Reachable only through a direct git commit or a pre-H1 `content write` of arbitrary bytes. The manual git path still works and nothing is corrupted. Fix later by rejecting at plan time with guidance, or by recording the compensation lossily or omitting it.
- Invisible-diff approvals (confirmed by probe 2). `str::lines()` hides CRLF/LF and trailing-newline-only changes, so the user approves an effect with no diff block. The content check prevents hidden payloads (a bare CR is rejected), so the change has no meaning for instructions. It still weakens "approve the exact change".
- The diff prints `before` lines raw. If a committed file is inadmissible (bypass vector), its control or escape characters reach the approval display unescaped. `after` is always admissible.
- `line_diff` falls back to a full delete/insert above 4M cells. That is acceptable for instruction files and was already a deferred minor.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- seat `rules/<name>.md` set/clear through `plan instructions` (only seat AGENTS.md and global/teamspace rules set are exercised in src/plan/tests.rs)
- `clear` for global and teamspace scopes, and `clear ... --from` usage rejection
- `--from` resolved relative to `caller.cwd` (tests pass absolute paths only)
- retired teamspace target and teamspace target given by id
- the single-part `seat AGENTS.md will be cut` cap-warning branch and the mixed omitted-plus-cut wording
- stale_plan when the file is deleted or created between plan and apply (only modify-after-plan is tested)
- set/clear when the current committed file is non-UTF-8 or otherwise inadmissible (probe shows plan OK, apply Failed)
- CRLF content and CRLF/LF-only or trailing-newline-only edits (empty diff block)

## Deferred OK
- src/plan/instructions.rs mutate: a non-UTF-8 current instruction file passes planning but fails the approved apply as a Failed op (`... is not UTF-8`); reject at plan time or store the compensation lossily (follow-up bead)
- src/plan/line_diff.rs: line-ending-only and trailing-newline-only changes give an empty diff, so render shows no block; consider a `\ No newline at end of file` / CRLF marker line
- src/plan/instructions.rs: the diff prints `before` text raw; an inadmissible committed file could inject terminal escapes into the approval display; escape or placeholder non-admissible lines
- src/plan/line_diff.rs: the 4M-cell fallback degrades to a full delete/insert (already deferred in task 2)
- src/plan/kinds_extra.rs SeatOverride::plan: cap sizing only when sections are set; clearing sections only shrinks the output (already deferred in task 14)
