# Final review inv-2, cluster 27 (src-observe)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 985d1eb6faaa2e92e4e01f430f0f2c07618bf6ad (both verified as commits; review source bound via bind-review-source, exit 0).

Inputs: slices/000145.diff (src/observe/seat_observation.rs), slices/000146.diff (src/observe/tests.rs), digest.md for context, epic hg-lnw and its design doc docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md.

## What the cluster changes

- `fact_state` in src/observe/seat_observation.rs now carries the two new `SeatStatus::Retired` fields, `keyed` and `retired_at`, into `SeatFactState::Retired` unchanged. This is the observation-side half of design section 3 ("The seat fact gains the keyed/keyless distinction, read from `continuity_key` in `SeatInspect`") and the input the reversible-window logic needs ("Windows, derived from the seat's `retired_at`").
- src/observe/tests.rs updates every `SeatStatus::Retired` literal and two `SeatFactState::Retired` assertions to the new shape, all with `keyed: true, retired_at: None`. No assertion was weakened: the two fact assertions still check `reversible: false` and now also pin the copied fields.

## Analysis

- Mapping is a straight field copy, exhaustive on the enum (a pattern with all named fields, so a future field on `SeatStatus::Retired` fails to compile here rather than being silently dropped).
- `fact_needs_write` compares `old.state != fact.state`. With the new fields, a legacy stored fact (deserialized with `keyed` defaulting to `true` via `legacy_keyed`, `retired_at` `None`) differs from a fresh read of a keyless seat or one with a `retired_at`, so the fact is rewritten on the next pass. That is the desired refresh. A threads-reported `retired_at` is a stable timestamp, so it does not cause write churn on every pass.
- Serialization compatibility of the stored fact lives in src/model/clone.rs (`#[serde(default = "legacy_keyed")]`, `#[serde(default, skip_serializing_if = "Option::is_none")]`), outside this cluster; it looks consistent with the mapping here.
- Requirement baseline: the cluster is a plumbing step for the baseline rows on keyless undo (ResurrectSeat routing needs `keyed` on the fact) and the reversible windows (need `retired_at`). The producer of those values (src/threads/mapping.rs `seat_status`, tested by `g5_disposition_maps_from_seat_inspect`) and the consumers (src/threads/effects.rs, src/undo) are outside this cluster; end-to-end delivery of those rows is the seam reviewer's to trace. Within this cluster, the production path `observe` -> `fact_state` delivers the fields with no loss.
- The fake (src/threads/fake.rs) emits `keyed: false` with `retired_at: Some(..)`, and threads/undo tests drive the observe loop against it, so the keyless propagation through `fact_state` is exercised indirectly.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test in src/observe/tests.rs asserts that `keyed: false` or `retired_at: Some(..)` on a `SeatStatus::Retired` read is copied into the committed `SeatFactState::Retired`; every observe-level case uses `keyed: true, retired_at: None`, so propagation of the keyless and timestamp values is covered only indirectly through fake-driven threads/undo tests outside this cluster.

## Deferred OK
- none
