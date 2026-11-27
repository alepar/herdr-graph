# Final review inv-3, cluster 26 (src-observe)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified with `git rev-parse --verify`). Caller checkout HEAD observed at 11ac415, status shows only untracked `evidence/sweep/`. Review source binding (`bind-review-source`, inv-3) succeeded.

Inputs: slices/000183.diff (src/observe/seat_observation.rs), slices/000184.diff (src/observe/tests.rs), digest.md.

## What the cluster changes

- `fact_state` in `src/observe/seat_observation.rs` now copies the two new `SeatStatus::Retired` fields, `keyed` and `retired_at`, into `SeatFactState::Retired` alongside the existing `reversible` mapping. This is a direct pass-through; no other arm changes.
- `src/observe/tests.rs` updates every `SeatStatus::Retired` construction and every `SeatFactState::Retired` assertion to the new shape, all with `keyed: true, retired_at: None`. No test behaviour changes.

## Analysis against the spec

The requirement baseline (B1 to B4) is the acceptance standard. This cluster touches B1 only: undo of a closed keyless seat must route to ResurrectSeat inside the 7 day window. The production chain is threads `seat_inspect` -> `threads::mapping::seat_status` (sets `keyed`, `retired_at`) -> `observe` builds the per-clone `SeatFact` through `fact_state` -> the committed `seat_fact` -> `undo/preview.rs` (line 485 onwards) reads `Retired { keyed, retired_at }` to choose ResurrectSeat versus UnretireSeat and compute the window age. The cluster's link in that chain is correct: `observe` is the only production writer of `seat_fact`, and it now carries both fields without loss. The links on either side (mapping and undo preview) belong to other clusters and the seam reviewer.

`fact_needs_write` compares the whole `SeatFactState`, so the fact is rewritten when `keyed` or `retired_at` changes. For facts persisted before this change, `keyed` is resolved at deserialisation (`legacy_keyed` default, overridden by the clone's `keyless` in `CloneRecord`'s `Deserialize`), and `retired_at` is absent (None). The first successful read after an upgrade therefore rewrites a retired fact if threads now reports a `retired_at`. That rewrite is harmless and gives every retired fact a fresh timestamp.

The observation arms themselves (unbind, retire, pending-restore guard) match on `SeatStatus::Retired { .. }` and are unaffected by the new fields, which is correct: keyed-ness changes the undo route, not whether observation retires the clone.

End-to-end coverage of the keyless pass-through exists outside this cluster: `src/threads/tests.rs` drives keyless closed-and-undone clones through the step loop (for example `resurrect_passes_the_bare_seat_of_the_reopened_pane_as_replace`), and those clones' retired facts come from this function.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test in src/observe/tests.rs asserts that `keyed: false` or `retired_at: Some(..)` from a `SeatStatus::Retired` lands unchanged in the committed `SeatFact`; every observe-level case uses `keyed: true, retired_at: None`, so the keyless pass-through is covered only indirectly by the threads step-loop tests.

## Deferred OK
- none
