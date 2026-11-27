# Final review inv-2, cluster 32 (src-undo)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 985d1eb6faaa2e92e4e01f430f0f2c07618bf6ad (both verified with `git rev-parse --verify`). Inputs: slices 000160.diff (src/undo/preview.rs) and 000161.diff (src/undo/tests.rs), digest.md, epic hg-lnw and its design doc `docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md` at head.

## Scope of the cluster

The cluster adds `seat_window_note` and calls it from both `clone.resurrect` sites in `restore_preview`, so the `herdr-graph undo` preview names, per keyless seated clone, which of the three windows from design section 3 applies: under 24 h (everything returns), 24 h to 7 d (memberships and history return, obligations settled, graph re-issues its own seat-wide invitations), and final (fresh seat, graph channels re-invited, external memberships lost). This is the design's "The undo plan (`herdr-graph undo` preview) names which of the three applies for each seat" requirement, the warning the coordinator's trial found missing.

## Requirement baseline trace

- B1 (resurrect the original seat on undo): the cluster delivers only the preview text; the resurrect itself is `threads.resurrect_seat` in src/threads, another cluster, which the seam reviewer traces. The preview is reachable from production: `herdr-graph undo` -> `undo::compensate` (src/undo/compensate.rs:147) -> `restore_preview`, whose `PlanBody.warnings` are printed by src/plan/commands.rs:78. The integration test `restore_preview_names_the_keyless_seat_window` drives that same `plan("undo <act>")` path.
- B2, B3, B4 are outside this cluster (restart rebind, pin, v4 session).

## Correctness analysis

- Window thresholds use `KEYLESS_OBLIGATIONS_SETTLE_AFTER` (24 h) and `KEYLESS_REVERSIBLE_FOR` (7 d) from src/ports/threads.rs, the same constants the rest of the epic uses, so preview and fake agree.
- The retire time comes from the fact's `retired_at`, falling back to `c.retired.at`, matching the design's "derived from the seat's retired_at because threads exposes no clock fields".
- Keyed seats (fact `keyed: true`, or no fact and `!c.keyless`) are silent, consistent with the unchanged `UnretireSeat` path. A fact for a different seat is filtered out and treated as unread.
- An unread fact on a keyless clone produces a note with the conditional suffix "if threads reports the close reversible", which is honest given that execution follows the fact read at execution time.
- A `Retired{reversible: false}` fact is final regardless of age, correct.
- A reversible fact older than 7 d is reported final; at execution threads would answer `NotFound` and the next read takes succession, so the preview matches the eventual outcome.
- Negative age (clock skew) falls into the under-24 h branch; harmless.

## Concerns

- A keyless clone whose committed fact is still a pre-close read (`Resolved`/`Unresolved`/`KeyConflict`, e.g. threads unreachable when the tab closed; nothing clears `seat_fact` on retirement, src/threads/assignment.rs:76 is the only reset) hits `Some(_) => return None` and the preview is silent, while the unread (`None`) case gets the conditional note. That is the same missing-warning symptom the trial reported, in a degraded case; treating a non-`Retired` fact like `None` (conditional note from `c.retired.at`) would close it.
- A keyed seat whose fact is `Retired{reversible: false}` also takes fresh-seat succession and loses external memberships, yet the preview is silent for it. This is pre-existing keyed behaviour and outside the three keyless windows the design names, so it is noted only.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- The second `clone.resurrect` site in `restore_preview` (undo of a clone-only retirement whose seat is not restored) has no test showing the window note; the integration test covers only the tab-cascade path where the seat is restored.
- No test for a stale pre-close fact (`Resolved` on a retired keyless clone) in `seat_window_note`.
- No test for a fact whose seat differs from `threads_seat` (the `.filter` fallback to unread).

## Deferred OK
- seat_window_note is silent for a retired keyless clone whose committed fact is a pre-close non-Retired read; consider emitting the conditional note from `c.retired.at` as for an unread fact (src/undo/preview.rs, `Some(_) => return None`).
- Keyed seats with a final retirement get no preview warning that external memberships are lost; pre-existing and outside the design's keyless windows.
