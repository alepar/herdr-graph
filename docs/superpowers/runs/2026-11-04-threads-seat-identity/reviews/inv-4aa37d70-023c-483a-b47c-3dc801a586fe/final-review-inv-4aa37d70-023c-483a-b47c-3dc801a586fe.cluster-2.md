# Final review, cluster 2 (docs), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735
(both verified with `git rev-parse --verify <sha>^{commit}`). Caller checkout observed at head
6576a117, working tree clean; it was not switched or modified. Review-source binding recorded in the
ledger before this file was written.

## Scope

Files in this cluster (slices 000005, 000052):
- `docs/reviews/2026-10-29-seat-review-H2-identity-design.md`: status line promoted from
  "proposal" to "approved direction"; §4 D2 rewritten to record the user's 2026-11-04 decisions
  (undo fully hands-off with requirement episodes and memberships restored on un-retire; reversible
  retired state rather than true un-retire; finalization rule); §7 risk replaced (re-acceptance on
  undo becomes "restoring episodes must not resurrect a requirement the graph has since released").
  Commit 6583164.
- `docs/superpowers/specs/INDEX.md`: new row for the root spec, status `draft`. Commit ff46e81.

## Analysis against the spec and requirement baseline

The root spec has a `## Requirement baseline` (B1-B11, no human scope change rows). This cluster is
documentation only; it delivers no runtime behaviour, so no baseline row can be delivered or
undelivered inside it. Each row's production path leaves the cluster and is the seam reviewer's to
trace:
- B1, B3 (1:1 seat identity, threads re-matching): code clusters src-threads, src-observe,
  src-reconcile; seam reviewer.
- B2, B6 (hard threads dependency, no Herdr change): seam reviewer.
- B4, B5 (unattended restart and live handoff): resume driver and D1-gated work; seam reviewer,
  including whether the D1 gate (hg-66u.1/.2/.3) has released and, if not, that the hand-back lists
  the blocked rows as open per the epic's global constraints.
- B7, B10 (undoable retirement, hands-off undo restoring memberships): D2-gated; seam reviewer.
  The review doc in this cluster now records B10 correctly in §4 D2.
- B8 (agent exit keeps seat and memberships): src-threads; seam reviewer.
- B9 (continuity key): src-model/src-herdr; seam reviewer. Code at head uses
  `HERDR_THREADS_CONTINUITY_KEY` (`src/model/launch.rs:9`), matching the spec.
- B11 (H2 Option D): skills cluster; seam reviewer.

The edited D2 text agrees with spec §8: reversible retired state, `UnretireSeat` returns the seat
to unresolved with memberships and requirement episodes kept, no change to the
`seats_retirement_terminal` trigger, and the §7 risk matches spec §8 step 4 (graph re-checks wants
after reattachment and releases stale requirements). The INDEX link resolves to the spec at head,
and `draft` follows the repository convention: earlier rows were added as `draft` and flipped to
`implemented` in a finish commit (ec817e9 for H1).

Residual inconsistencies in the review doc at head, now that it is labelled "approved direction":
- §4 D1 (line 144) and §5 item 2 (line 185) still name the continuity key `HERDR_GRAPH_CLONE=<cl_>`;
  the spec's key decisions and the code use `HERDR_THREADS_CONTINUITY_KEY=<cl_ id>` (the neutral
  name agreed with the threads coordinator), with `HERDR_GRAPH_CLONE` kept separately.
- §5 item 4 (line 191) still ends undo with "re-invite", which contradicts the hands-off D2 decision
  recorded by this same diff (re-invite applies only to the succession branch in spec §8).
- §4 D2 finalization says "purge or policy"; spec §8 and §14 say the reversible window ends only
  through the purge tombstone's finalize want or an operator final retire, with no retention timer.
- §4 D2 first bullet still reads "memberships released" at close, juxtaposed with the new restore
  bullet; read together they are consistent (released, then restored on un-retire), but the spec's
  wording is "kept".
The spec is the stated acceptance standard and is explicit on all four points, so none of these
misdirects the implementation; they are documentation hygiene.

## Findings

- Review doc §4 D1 and §5 item 2 name the continuity key env var `HERDR_GRAPH_CLONE`; spec and code use `HERDR_THREADS_CONTINUITY_KEY` (docs/reviews/2026-10-29-seat-review-H2-identity-design.md lines 144, 185).
- Review doc §5 item 4 still lists "re-invite" after undo, contradicting the hands-off D2 decision this diff added (line 191).
- Review doc §4 D2 finalization mentions "policy", which the spec rules out (no retention timer; spec §8, §14) (line 167).

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- none

## Deferred OK
- Align the H2-identity review doc with the spec: continuity key env var name (lines 144, 185), drop "re-invite" from §5 item 4 (line 191), drop "or policy" from D2 finalization (line 167); documentation only, the spec is authoritative and explicit.
- INDEX.md row status `draft` should be updated at finish per convention; if gated subtrees (D1/D2/G5) remain unexecuted at landing, use a status that says so rather than `implemented`.
