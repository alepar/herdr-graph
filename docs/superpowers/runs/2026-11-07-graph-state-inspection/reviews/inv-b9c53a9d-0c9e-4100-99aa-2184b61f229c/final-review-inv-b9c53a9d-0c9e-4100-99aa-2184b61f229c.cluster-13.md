# Final review — cluster 13 (docs-3), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 2dfa656c7f98715fe66ca53c253de19f369a857d (both verified with `git rev-parse --verify`). The caller checkout was at 2dfa656 with a clean status when the review started. Review source binding exited 0.

## Scope of this cluster

Twelve new run-record files under docs/superpowers/runs/2026-11-07-graph-state-inspection/ (slices 000005–000016):
- design roast round 2 report (2026-11-08-graph-state-inspection-roast-design-2.md)
- redesign cycle 1 amendment record (amend-design-1.md)
- coverage-amend dispositions (coverage-amend-design-1-dispositions.md)
- coverage findings, rounds 1 and 2
- coverage ledger
- the derived requirement list R1–R15 (coverage-round-1-requirements.md)
- four coverage reviewer summaries
- the design findings ledger (findings-ledger-design.md)

These files are process records. None ships product behavior, so no Requirement baseline row is delivered or withheld by this cluster alone. Each baseline row's production path runs through src/ and tests/, which are other clusters' to trace (seam reviewer).

## Consistency against the spec at head

- The R1–R15 requirement list covers every baseline row: B1 (R1), B2 (R4–R6), B3 (R7–R9), B4 (R10), B5 (R6), B6 (R7), B7 (R5), B8 (R2), and S1 (R3). R6 is marked as restated by redesign cycle 1, and it matches the spec at head: hook-only writer keyed by native_session_id, and `seat --instructions` / `seat.resolve` never write.
- In amend-design-1.md, the `requirements:` block says B1–B5, B7, B8 and S1 are preserved. That matches spec §2.1–§2.5 at head. It does not lower any baseline row. B4 was later strengthened by c7eb809, which added `total` and the "showing N of total" footer.
- The four confirmed findings of roast round 2 are all addressed in the spec at head (commit c7eb809):
  - Codex re-trust cost: spec lines 287 and 572.
  - Sessions `total` and footer: spec lines 392 and 401.
  - hg-blw.11 slot name: the bead now says `clones[].instructions.status`.
  - InstructionsView type ownership: moved to hg-blw.7, which is closed.
- Spot checks of the code at head agree:
  - src/cli/setup.rs:64 prints the re-trust notice.
  - src/daemon/doctor.rs:269 warns about the changed hook.
  - src/inspect/sessions.rs carries `total` and prints the footer at line 194.
  - src/inspect/loaded.rs:33 defines CODEX_FORM_NOTE, applied at line 230.
- The run.md parked line 50 records the round-2 punch list as applied inline.
- coverage-amend-design-1-dispositions.md P1 still says "the repo has no evidence Codex's SessionStart payload carries session_id". Roast round 2 superseded that premise, and the spec at head (line 261) and run.md parked line 47 carry the corrected wording. The dispositions file is a dated snapshot, so this is acceptable.
- The coverage ledger's C16 and C20 entries use the superseded `clones[].loaded` slot name. They are historical rows written before redesign cycle 1, and the live bead text uses `clones[].instructions`.

## Observed evidence beyond the cluster (for other reviewers)

- The beads hg-blw.10 (docs) and hg-blw.11 (integration sweep) were open when the review ran (`bd list --parent hg-blw`). These are the owners of derived R13 and R14. No baseline row maps to them alone, so this is noted for the seam and final reviewer, not judged here.
- The spec has no `stdin_payload` record field and no "hook predates --stdin-payload; rerun setup" note. Code at head has both: src/inspect/loaded.rs:60 and :222, with a test at tests/inspect_loaded.rs. Roast round 2 rejected this as a spec gap (FYI) and asked that the bead and spec be aligned. That divergence belongs to the loaded-record cluster.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- B2/B5 for Codex seats: whether a real Codex SessionStart payload `session_id` byte-equals the occupant native_session_id that graph takes from Herdr `agent_session` / `resume` argv. The roast round 2 escalation and run.md parked line 47 record it as unverified, and the only test of a Codex clone reaching `current` (tests/inspect_loaded.rs, codex_hook_records_the_payload_session) uses a synthetic occupant id. This path leaves this cluster; tracing it is the seam reviewer's job, and the parked degraded verdict has no human scope change row behind it.

## Deferred OK
- findings-ledger-design.md was rewritten at eb840e6, after the c7eb809 punch list, yet it still marks the four round-2 findings `open`, while run.md line 50 and the spec record them as applied. Mark them resolved@r2-punch (or say that `open` means not re-roasted) so a later reader or tool does not count four open Should-fix design findings.
- coverage-amend-design-1-dispositions.md P1 keeps the superseded "no evidence Codex sends session_id" premise. Consider a one-line pointer to roast round 2 and spec §2.3.
- coverage-ledger.md C16/C20 name the superseded `clones[].loaded` slot. This is a historical record, so it can stay, or get a superseded-by note.
