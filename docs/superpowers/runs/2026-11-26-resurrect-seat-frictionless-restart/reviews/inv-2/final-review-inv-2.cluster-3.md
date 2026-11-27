# Final review inv-2, cluster 3 (docs-specs)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 985d1eb6faaa2e92e4e01f430f0f2c07618bf6ad. Files: docs/superpowers/specs/2026-11-07-onboarding-existing-spaces-design.md and docs/superpowers/specs/INDEX.md.

## Scope read

The onboarding spec gets a status line update, a new prerequisite (threads 3.6.0), rewritten G4 rows for restart-with-fresh-start and tab-closed, a reworded resurrect paragraph, and a Post-Implementation Notes section. INDEX.md gets one row for the hg-lnw design.

## Requirement baseline trace for this cluster

The cluster is documentation only. B1 (undo reuses the closed seat) and B2 (frictionless restart) are delivered by code clusters that other reviewers trace. This cluster only has to describe them accurately.

## Accuracy checks against the reviewed head

- The 3.6.0 pin matches: third_party/herdr-threads is at tag v3.6.0 (7c3c090), and src/daemon/doctor.rs names service_session_v4 and "must be at least 3.6.0".
- The 7 d and 24 h windows match the undo preview text (src/undo/preview.rs) and the adoption warning (src/plan/seat_adopt.rs).
- The clones.restart_held and clones.resurrect_held names and the remedy `herdr-threads human seat rebind SEAT --pane PANE --operator` match src/daemon/clone_status.rs and src/daemon/doctor.rs.
- The 90 s grace and the resume-driver description are consistent with the planner (src/reconcile/planner.rs plan_resume, RelaunchOccupant).
- No stale 3.1.0 or v3 claim remains outside the intentional history notes. The old "follow-up, not part of these phases" wording is replaced.
- INDEX.md row format matches its neighbours, the link path exists, and the status "implemented" agrees with the in-branch state.

## Observations

- The G4 table cells are long single lines, which is consistent with the existing table style.
- The caller worktree was clean when the review began.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- none
