# Final review, cluster 2 (docs), epic hg-66u

Captured source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head e4984a492e2d3ce8344b9be83d56a3471049fd60 (both verified with `git rev-parse --verify <sha>^{commit}`). Review source bound to invocation inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f via `dispatch-text.mjs bind-review-source` (exit 0). Caller checkout observed at HEAD e4984a4, `git status --short` clean, before and during the review; no caller files were changed.

Cluster files: `docs/reviews/2026-10-29-seat-review-H2-identity-design.md` (slice 000003) and `docs/superpowers/specs/INDEX.md` (slice 000027). Both changed in commits 6583164 and ff46e81.

## Scope and method

- Read both slices, the digest, the root spec at head (`docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md`, the `spec_id` of `bd show hg-66u`), the full H2 identity design at head, and `threads-ht-ma6i-protocol-shapes.md` to check the D2 statements against the agreed threads shapes.
- Checked the INDEX row's link target exists at head (`git cat-file -e`), and checked the status vocabulary against INDEX history (the H1 row was added as `draft` in 3925f1f and flipped to `implemented` in ec817e9).
- Gate beads hg-66u.1, hg-66u.2 and hg-66u.3 (D1, D2, G5) all read `open` at review time.

## Analysis of the H2 identity design edit

- The status line now reads "approved direction" and names the implementation route. That matches the epic, which cites this file as the governing review.
- The new D2 bullets record B10 (undo fully hands-off: un-retire restores requirement episodes and memberships) and the user's choice of a reversible retired state over a true un-retire. Both match spec §8 step 2 and §3, and the protocol shapes ("No re-invite after un-retire: memberships and requirement episodes are never released while reversible").
- The changed risk bullet ("graph re-checks wants after reattachment and releases stale ones") matches spec §8 step 4.
- The edit is incomplete. Several unchanged sentences in the same document now contradict the new bullets or the spec:
  - Line 154 (D2): "Close behaves exactly as today: memberships released". The new line 161 and the protocol shapes say a reversibly retired seat keeps its memberships.
  - Line 191 (§5 item 4): "Undo and resurrection → reopen tab, `UnretireSeat`, relaunch with the key, re-invite." The new D2 text says hands-off with no re-invite. Spec §8 re-invites only on the final-retired succession branch.
  - Line 167 (new): "Finalization: when the graph's own retirement becomes final (purge or policy)". Spec §14.1 says purge is an explicit operator command and there is no automatic retention timer. The protocol shapes also say no timer. "or policy" has no counterpart in the spec.
  - Lines 144 and 185 still name the continuity key `HERDR_GRAPH_CLONE=<cl_>`. The spec's key decision and §5 use `HERDR_THREADS_CONTINUITY_KEY=<cl_ id>` (with `HERDR_GRAPH_CLONE` kept beside it).
- The root spec is the acceptance document, and §14 states that it supersedes conflicting text. These are documentation inconsistencies, not delivery defects. The governing review does not say that the spec takes precedence, though, so a reader who starts from it gets contradictory D2 guidance.

## Analysis of the INDEX row

- The link `../runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md` resolves at head.
- The summary matches the spec's key decisions. "undo via UnretireSeat" is the post-D2 path; before D2, undo goes through seat succession (§8 interim). As a one-line summary this is acceptable.
- Status `draft` follows the repository convention: rows are added as draft and flipped to implemented when the run completes, as the H1 row was. That is correct while the D1, D2 and G5 gates are open.

## Requirement baseline tracing for this cluster

- The spec has a `## Requirement baseline` (B1-B11, no human scope changes). Neither cluster file is a production entry point. The H2 edit records B7 and B10 faithfully, and it lowers no row.
- B1-B11 are delivered, or not, by code paths in other clusters (src/, skills/, tests/). Tracing them from production entry points is for the code-cluster and seam reviewers. That includes the D1/D2-gated rows B7, B9 and B10, which cannot be fully delivered while gates hg-66u.1 and hg-66u.2 are open. This cluster does not call those rows undelivered.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- Docs only: no test covers documentation consistency. The INDEX link target was checked by hand at head; the H2 design's internal consistency was checked by reading it, and four stale statements were found (listed under Deferred OK).
- Baseline rows B1-B11 (and especially the gated B7, B9, B10 while hg-66u.1 and hg-66u.2 are open) are not traceable from this cluster; that is for the seam and code-cluster reviewers.

## Deferred OK
- docs/reviews/2026-10-29-seat-review-H2-identity-design.md:154 says pane close releases memberships, which contradicts the new D2 bullet at line 161, spec §3 and the protocol shapes (memberships kept while reversible); reword to "memberships kept while reversibly retired, ended at finalization".
- docs/reviews/2026-10-29-seat-review-H2-identity-design.md:191 lists "re-invite" as an undo step, which contradicts the hands-off decision (B10) recorded at line 161; limit re-invite to the final-retired succession branch as in spec §8.
- docs/reviews/2026-10-29-seat-review-H2-identity-design.md:167 says finalization happens on "purge or policy", but spec §14.1 and the protocol shapes say there is no automatic timer and only explicit purge or an operator's final retire; drop "or policy".
- docs/reviews/2026-10-29-seat-review-H2-identity-design.md:144 and :185 name the continuity key `HERDR_GRAPH_CLONE`, which the spec superseded with `HERDR_THREADS_CONTINUITY_KEY` (agreed with the threads coordinator); update, or add a note that the root spec takes precedence.
- docs/superpowers/specs/INDEX.md: the threads-seat-identity row must flip from `draft` to `implemented` (with the summary checked against what shipped) when the gated subtree lands; it is correct as draft while D1, D2 and G5 are open.
