# Final review, inv-3, cluster 4 (evidence-hg-lnw-10)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified as commits). Cluster input: one new file, evidence/hg-lnw.10/packaging-readme.log (152 lines), slice 000099.diff. The epic has no Requirement baseline section in its bd record, so the baseline paragraph does not apply.

## What the cluster is

A captured `cargo test --test packaging` run for task hg-lnw.10 (docs: keyless undo windows and restart behaviour). The log is a full cold build of dependencies, herdr-threads v3.6.0 and herdr-graph v0.3.0, followed by `tests/packaging.rs`: `readme_has_required_sections ... ok`, 1 passed, 3 filtered out, exit=0. The file contains no product code; it only evidences that the README section check passed in the task worktree.

## Assessment against the task

- The log shows the dependency graph resolved to herdr-threads v3.6.0 from third_party, consistent with the epic's pin move.
- The test is a README-sections presence check. It cannot show that the README, CURRENT-DESIGN.md, CHANGELOG.md or the onboarding spec text describes the new behaviour (task acceptance bullets). Those files belong to other clusters; this log is supporting evidence only.
- The log embeds two absolute paths of the task worktree (a build-time artifact of cargo output). Harmless, but the paths point at a worktree that is expected to be cleaned up.
- The log records no commit hash, so it cannot be tied to a specific source revision from its own content.

## Seam notes

- Whether the README text satisfies the hg-lnw.10 acceptance bullets (resurrect_held, restart_held, pre-3.6.0 daemon error, invitation re-issue wording) is for the docs cluster reviewer, not traced here.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- The log covers only the README required-sections test; the content accuracy of the docs described by hg-lnw.10 is not evidenced here and belongs to the docs cluster.

## Deferred OK

- The log carries no source revision and embeds task-worktree absolute paths; cosmetic, no action needed.
