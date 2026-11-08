# Final review: cluster 5 (runs-reviews-inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f), epic hg-66u

## Source and method

- Invocation inv-4aa37d70-023c-483a-b47c-3dc801a586fe. `dispatch-text.mjs bind-review-source` exited 0 and echoed base 0a84c67554aa3fc8e8caf20db4163683ab30af61 / head 6576a117e595097ead48ca7178935cf039916735 before this file was written. Both SHAs verified with `git rev-parse --verify <sha>^{commit}`.
- Caller checkout observed at HEAD 6576a117e595097ead48ca7178935cf039916735 with an empty `git status --short`. Nothing in it was switched, restored or edited, and no worktree was created; all reads used `git show <sha>:<path>` and `git grep <sha>`.
- Inputs: clusters.json entry for this cluster (20 files, slices 000028-000047), digest.md, `bd show hg-66u --json`, `bd list --parent hg-66u --all`, and the root spec at head (docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md), which has a `## Requirement baseline` (B1-B11, Human scope changes: none).

## What the cluster holds

- The 19 cluster reviews and the seam review of the previous final review, invocation inv-cc29fc83, committed by 777a056 ("code invocation 1 settled") under docs/superpowers/runs/2026-11-04-threads-seat-identity/reviews/inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f/. That review ran at head e4984a4 and returned "not ready" (seam review and clusters 3, 12, 16, 18, 19 not ready; the other 14 ready).
- No production code, tests or spec text. The files are point-in-time run evidence, so the review question is fidelity (do the committed copies match what the review produced, and are their claims true at their own head) and whether anything they flag is still live at the head under review.
- The 73 files of the record's `assembled/` bundle in the same directory are not in this cluster; generated-input-provenance.md lists them as generated inputs, so they are excluded by design rather than orphaned.

## Record fidelity

- All 20 committed files are byte-identical (`cmp`) to the originals in .superpowers/sdd/hg-66u-plan/. run.md lines 27-48 hold one finding-source entry per file with the same verdict strings, so the record and the ledger agree.
- Spot checks at the record's own head e4984a4 hold: `clear_restore_on_retire` is called at exactly src/observe/mutations.rs:252, src/plan/core_kinds.rs:424 and 1811, and src/undo/compensate.rs:494; `admit_threads_seat` and `write_restore_intent` had no non-test caller; `agent_env(&clone, false)` is hard-coded on every start path in src/reconcile/herdr_exec.rs.
- One citation drift: the seam review cites the `agent_env` sites as herdr_exec.rs:549, 575, 679, but at e4984a4 the calls are at 549, 560 and 658. Lines 575 and 679 are the `record_launch_env(..., &env)` calls that consume the same value, so the claim stands and only the line numbers are wrong.
- The cluster 12 and cluster 16 reviews disagree on the pre-D2 `Finalize` tombstone. The seam review settles that explicitly and carries cluster 16's item forward, so the record is internally consistent.
- Every record states its head (e4984a4). Its statements about B1, B8 and the v3.0.0 pin are now out of date, because hg-66u.4, .7, .8 and .6 landed afterwards (5ef0a1d, 3dd9034, a316f81, 496fd43). That is expected for a point-in-time record and not a defect, as long as nobody reads it as current status.

## Record verdict items re-checked at 6576a11

- No commit after e4984a4 touches src/plan/core_kinds.rs, src/threads/assignment.rs or tests/instruction_writers.rs (`git log e4984a4..6576a11 -- <those paths>` is empty). The record's three code-level items are therefore unchanged at the head under review:
- Writer inventory. I emulated `instruction_source_writers_are_inventoried` statically: same walk rule (skips `tests/` dirs and `tests.rs`), and production text cut at the first `#[cfg(test)]` line. Over src at 6576a11 it finds src/plan/core_kinds.rs (line 1925, `None => cx.tree.delete_file(&loc.record_path)`, which has no `#[cfg(test)]` line at all) along with the five inventoried files. The expected set lacks src/plan/core_kinds.rs, so the assertion still fails at head.
- Seat resurrect. The `SeatResurrect` plan loop (src/plan/core_kinds.rs:1504-1514) and the loop at :1579 select clones by `lifecycle == Retired && retired_with(...)` with no `is_tombstone()` check. Only `do_resurrect_seat` (:513) skips tombstones, so the record's cluster 12 defect is still present.
- `purge_tombstone` (src/threads/assignment.rs:77-81) still writes `Finalize{seat}` for every seated clone with no D2 condition, and spec §8/§14.2 are unchanged since e4984a4. Unlike at the record's head, this is now reachable: hg-66u.7 (3dd9034) gives clones a production `threads_seat`, so a pre-D2 purge of a seated clone now leaves a tombstone that nothing removes until hg-66u.31.
- No bead tracks any of the three (`bd search instruction_writers` returns nothing; the resurrect hits are unrelated). The only hand-off is the finding-source entries in run.md, waiting on fix-loop gate hg-66u.36. Defects in those files belong to the clusters that own them. I list them below so the seam review cannot miss them, but they do not count against this cluster's verdict.

## Requirement baseline trace

- This cluster contains no production entry point, so every row's path (B1-B11) leaves the cluster and belongs to the seam reviewer, who must trace it at 6576a11 rather than rely on this record's e4984a4 trace. In particular, B1 and B8 have moved since the record (hg-66u.7 and hg-66u.8 landed). B4, B5, B7, B9 and B10 still depend on open beads hg-66u.28, .29 and .30 or the gated .31-.33, .43 and .44.
- No row is delivered or withheld by these files. They are evidence, and as evidence they are accurate at their head.

## Findings

- Outside this cluster, still present at 6576a11: tests/instruction_writers.rs `instruction_source_writers_are_inventoried` fails, because src/plan/core_kinds.rs:1925 (`ClonePurge::mutate`, `cx.tree.delete_file`) is not in the expected set (record cluster 19 and seam item; static emulation of the test's scan at head). Owner: the tests/src-plan clusters.
- Outside this cluster, still present at 6576a11: the `SeatResurrect` plan/mutate loops (src/plan/core_kinds.rs:1504-1514, :1579) revive purge tombstones, so resurrecting a seat whose only clone was purged commits an Active seat with no clone (record cluster 12). Owner: the src-plan cluster.
- Outside this cluster, still present and now reachable at 6576a11: `purge_tombstone` (src/threads/assignment.rs:77-81) writes `Finalize{seat}` before D2, against spec §8; with hg-66u.7 assigning seats in production, pre-D2 purges now leave tombstones that nothing removes (record cluster 16 and seam item). Owner: the src-threads cluster.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No automated check binds the committed record copies to their run.md finding-source entries (for example by a hash). Fidelity rests on the manual byte comparison done in this review.

## Deferred OK
- Seam-review citation drift: the `agent_env(&clone, false)` sites are given as src/reconcile/herdr_exec.rs:549, 575, 679, but at e4984a4 they are at 549, 560 and 658 (575 and 679 are the matching record_launch_env calls); the substance holds.
- The committed cluster-9, cluster-12 and cluster-19 records (and run.md) embed absolute local paths under /var/home/alepar/; harmless in this private repo, but they put the home layout into history.
- The record's B1, B8 and herdr-threads-pin statements reflect e4984a4 and were superseded by hg-66u.4/.6/.7/.8; the record is labelled with its head and is not current status.
- The cluster-16 record has a blank line between "## Verdict" and its verdict line; the collector parsed it correctly (run.md line 35).
