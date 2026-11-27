# Final review inv-3, cluster 28 (src-ports)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified with `git rev-parse --verify`). Caller checkout observed at HEAD 11ac415, status `?? evidence/sweep/` only (the background sweep's output); not touched. Review-source binding via `dispatch-text.mjs bind-review-source` exited 0.

Cluster input: `src/ports/threads.rs`, slice `slices/000187.diff` (+108/-3). The slice matches `git diff base..head -- src/ports/threads.rs` exactly. No cargo was run here: a test sweep runs in the integration worktree, and this cluster is type and trait definitions whose behaviour is exercised by tests in other clusters.

## What the cluster changes

- `ThreadsError::Rejected` gains a doc contract: definitive refusals only; graph-local failures are `Disconnected` with a `graph-local: ` prefix so they retry. Spot-checked at head: `src/daemon/compose.rs` (lines ~537, 764, 801-812, 872) wraps namespace, host claim and intent-dir failures as `Disconnected("graph-local: ...")`; `src/threads/mapping.rs` turns an unparseable pane id into `Rejected`, which the comment names as a malformed request. Consistent.
- `SeatStatus::Retired` gains `keyed: bool` and `retired_at: Option<Timestamp>`. Design section 3 needs the keyed/keyless split (from `continuity_key`) and the window clock (from `retired_at`); `src/threads/mapping.rs::seat_status` fills both from `SeatInspection` (`continuity_key.is_some()`, `summary.retired_at` millis). `SeatStatus` is not persisted; it is mapped into `SeatFactState` in `src/observe/seat_observation.rs`, so the added non-optional field has no stored-data compatibility cost in this cluster.
- New `SeatRestoreRefusal` with a `retryable()` split. Transient set (`ServiceBusy`, `StaleHostObservation`, `Unreachable`, `OutcomeUnknown`) matches the design's transient list in sections 3 and 4.2 (busy, stale host observation, timeout/unreachable, outcome unknown). Definitive set (`NotFound`, `Unauthorized`, `TargetAlreadyOwned`, `InvalidRequest`, `Unsupported`, `InstanceMismatch`) matches section 3's definitive list; `NotFound` being non-retryable at the port level is fine because the effect layer re-plans from the re-read fact as section 3 requires.
- New `SeatRestoreOutcome` (`Done`, `AlreadyResolved`, `Refused{reason, retryable}`) with `refused()` as the documented single constructor and `from_error()` mapping transport errors. `AlreadyResolved` models the U2 contract's `SeatAlreadyResolved` (design section 1); `src/threads/effects.rs` counts it as done on both the resurrect and the rebind path, as the design requires.
- `KEYLESS_OBLIGATIONS_SETTLE_AFTER` (24 h) and `KEYLESS_REVERSIBLE_FOR` (7 d) match the threads 3.5.0 windows in design section 3; they are used in production by `src/undo/preview.rs::seat_window_note` (the undo-plan warning) and by the fake.
- `ThreadsPort::resurrect_seat(seat, pane, replace, op)` and `rebind_held_seat(seat, pane, op)` with additive `Unsupported` defaults. Signatures match design sections 3 and 4 (rebind is ResurrectSeat with `replace: None`, per section 1).

## Requirement baseline trace for this cluster

The spec has a `## Requirement baseline` (B1-B4, human scope changes: none). This cluster defines the port only; it delivers no row on its own.

- B1 (resurrect on undo): the port method exists and is overridden by the real adapter (`src/threads/adapter.rs` `resurrect_seat` -> `restore` -> `restore_outcome`) and by the production composition (`src/daemon/compose.rs` `LazyThreads::resurrect_seat`), so production does not fall through to the `Unsupported` default. The planner/effect path from undo to the call is the seam reviewer's to trace.
- B2 (frictionless restart): `rebind_held_seat` is likewise overridden in adapter and compose. Planner emission and the keyless gate are outside this cluster: seam reviewer.
- B3 (v4 service session via upstream fix): not in this file (adapter `GRAPH_SERVICE_LEVEL`, pin at third_party/herdr-threads v3.6.0); outside this cluster.
- B4 (upstream U2 rebind): the port method is the graph-side surface of U2; the adapter sends ResurrectSeat with no `replace`. Delivery path outside this cluster: seam reviewer.

## Adapter-mapping spot check (named risk: refusal classification)

`src/threads/adapter.rs::restore_outcome` handles a refused v4 registration (`is_level_refused`) before falling back to `from_error`, so the graph-synthesized `level_refused` `Rejected` is classed `Unsupported`, not `InvalidRequest`. `LazyThreads::get()` in compose does not register, so its `from_error` calls only see `Disconnected` errors. The mapping is consistent at head.

## Concerns

- `SeatRestoreOutcome::Refused { reason, retryable }` has public fields and derives `Deserialize`, so the "only constructor" invariant (`retryable == reason.retryable()`) is a convention rather than something the type enforces. No producer at head violates it (all go through `refused()`), and the value is not persisted. Low risk.
- `from_error` maps every `ThreadsError::Rejected` to `InvalidRequest`. That is right for the documented contract, but a future caller handing it the `level_refused` error (itself a `Rejected`) would record `invalid_request` instead of `unsupported` in `resurrect_refused`/`rebind_refused` meta and doctor text. Today the only path that can see that error pre-filters it in `restore_outcome`. Both reasons are definitive, so behaviour is the same; only the operator-facing reason label would be wrong.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No cargo run in this review (the background sweep owns the integration worktree); port-type behaviour relies on src/threads/tests.rs and src/threads/service_tests.rs retryable/from_error tests, not re-run here.
- That threads keeps a keyed seat's continuity_key row while its retirement is reversible (so `keyed` reads true and undo picks UnretireSeat rather than ResurrectSeat) is a mapping/real-threads property outside this cluster; seam or tier-5 reviewer to confirm.

## Deferred OK
- Make `SeatRestoreOutcome::Refused` fields private or drop `Deserialize` so the retryable invariant is type-enforced.
- Have `from_error` (or its callers) route the `level_refused` error to `Unsupported`, so the recorded reason stays right if a new caller passes it through.
