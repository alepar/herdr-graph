# Final review: cluster evidence-hg-lnw-10

Scope: evidence/hg-lnw.10/packaging-readme.log (slice 000029), a new 152-line cargo test transcript for task hg-lnw.10 (docs).

## Source binding

bind-review-source succeeded (exit 0). Both commits verify: base 60e287a, head 8f8ec53.

## Analysis

The log is a clean build transcript from the task worktree followed by `tests/packaging.rs` filtered to `readme_has_required_sections`: 1 passed, 0 failed, 3 filtered out, exit=0. The test exists at the reviewed head.

The test checks README structure only. It is real evidence for the "README still has its required sections" regression guard after the hg-lnw.10 rewrite. It does not prove the docs content criteria of hg-lnw.10 (keyless undo window, restart rebind, restart_held and resurrect_held status rows, pre-3.6.0 daemon error, invitation re-issue text).

Spot check of README at head: restart_held (line 98), resurrect_held (line 99) and the pre-3.6.0 daemon error with the doctor finding and upgrade step (line 239) are present. The 24 h to 7 d invitation statement and the CURRENT-DESIGN, CHANGELOG and onboarding spec edits belong to the root and docs clusters and were not judged here.

The log is a captured artifact, not code. No spec requirement is implemented in this cluster, and the root spec has no requirement baseline section that this cluster could fail.

## Observations

The log embeds task-worktree absolute paths. This is harmless as historical evidence.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The packaging test checks README section headings only, not the hg-lnw.10 behaviour text; content accuracy is for the docs and root cluster reviewers.

## Deferred OK

- none
