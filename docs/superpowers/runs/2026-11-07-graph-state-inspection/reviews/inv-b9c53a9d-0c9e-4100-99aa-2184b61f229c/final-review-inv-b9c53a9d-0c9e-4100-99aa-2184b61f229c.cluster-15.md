# Final review: cluster 15 (docs-5), epic hg-blw

## Source and inputs

- Invocation inv-b9c53a9d-0c9e-4100-99aa-2184b61f229c. Base bfeb8f3104dd3783d67a46570c9f272e935012b5 and head 2dfa656c7f98715fe66ca53c253de19f369a857d, both verified with `git rev-parse --verify <sha>^{commit}`. `bind-review-source` ran before this file was written and exited 0. The artifact did not exist before that.
- Caller checkout observed at HEAD 2dfa656 with a clean `git status --short`, before and during the review. No caller file was edited.
- Cluster files: the archived review reports of the previous invocation inv-4b650ab2-a313-4bcb-9196-17e40854f3ab under docs/superpowers/runs/2026-11-07-graph-state-inspection/reviews/inv-4b650ab2-a313-4bcb-9196-17e40854f3ab/: cluster-7 (src-plan), cluster-8 (src-store), cluster-9 (src-writer) and the seam report. Slices 000026 to 000029. Context: digest.md, `bd show hg-blw --json`, and run.md at head.

## What the cluster is

- All four files are new, docs-only, and come from one commit, 0c85b62 "docs: collect invocation 1 review; intent for invocation 2". No product code is in this cluster.
- They are records of a review taken at the earlier head 6eb6cec. They are not claims about 2dfa656. Their content is out of date at the reviewed head, and that is expected: for example, d1e9ffb removes `inspect_stubs_not_implemented`, and the hg-blw.12 work was merged after them. This matches the archive convention on main, where docs/superpowers/runs/*/reviews/ holds 831 earlier `inv-*` files.

## Integrity checks

- Byte identity: each archived file has the same sha256 as its workspace original in .superpowers/sdd/hg-blw-plan/ (cluster-7 2648107…, cluster-8 3d75cfd…, cluster-9 0323295…, seam ab099df…). The archive is a faithful copy.
- Ledger agreement: the run.md `finding-source` records (lines 40-43) name these artifacts by archive-relative path. Their verdicts match the files' Verdict lines: ready, ready, ready, and the seam "not ready (…B2, B5, B7…)". Their occurrences match the files' extractable sections in order. The `review-collection` record (line 44) cites the seam verdict word for word.
- Spot checks of cited source lines at 6eb6cec all hold:
  - src/daemon/compose.rs ~408 registers `plan::ops::register_commands`.
  - src/daemon/compose.rs ~1108-1119 sets `seat_view_error` only on `Rejected`/`Unsupported` and leaves it unchanged on any other error.
  - src/cli/mod.rs 219-226 holds `inspect_stubs_not_implemented` running `sessions` with `unwrap_err()`.
  - src/cli/instructions.rs has only `InstructionsAction::Show`.
- Grammar: each file ends with Verdict, Must fix before landing, Untested scope and Deferred OK, in that order, with flat bullets.
- Cluster-8 is the exception: it also has a full extra copy of those three sections above its Verdict, plus a Concerns section that repeats its two deferred items. Because of this, run.md line 41 holds the same findings twice: 8 untested occurrences for 4 distinct items, and 4 deferred occurrences plus 2 concerns for 2 distinct items. Triage could count those findings twice. Nothing is lost, and the seam report already merges them, so it does not affect correctness.

## Requirement baseline

- The design doc's Requirement baseline rows (B1-B8, S1) describe product behaviour. This cluster delivers none of them and cannot keep any from being delivered.
- Whether B2, B5 and B7 are delivered at 2dfa656 is not decided by these records, because they describe 6eb6cec. Tracing those rows at the reviewed head belongs to the src/tests cluster reviewers and the seam reviewer.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- Not every line citation in the four archived reports was re-checked against 6eb6cec: four citations were spot-checked and all held, and the remaining citations were not re-traced.

## Deferred OK
- Archived cluster-8 report (reviews/inv-4b650ab2-…/final-review-inv-4b650ab2-….cluster-8.md) repeats its Must fix, Untested scope and Deferred OK sections above the Verdict and restates its deferred items under Concerns, so run.md line 41 records each of those findings twice (8 untested occurrences for 4 items, 6 concern/deferred occurrences for 2 items); dedupe at triage or have the report-format check reject duplicate terminal sections.
