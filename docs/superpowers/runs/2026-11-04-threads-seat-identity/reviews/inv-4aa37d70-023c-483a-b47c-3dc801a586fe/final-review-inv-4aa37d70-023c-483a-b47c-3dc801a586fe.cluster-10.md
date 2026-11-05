# Final review, cluster 10 (src-daemon), epic hg-66u

Invocation: inv-4aa37d70-023c-483a-b47c-3dc801a586fe. Captured source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735. I verified both with `git rev-parse --verify <sha>^{commit}`. The review source binding (`dispatch-text.mjs bind-review-source`) exited 0. The caller checkout was at 6576a11 with a clean `git status --short` before and after the review.

Cluster files: src/daemon/clone_status.rs, src/daemon/compose.rs and src/daemon/mod.rs (slices 000059 to 000061).

## What the cluster changes

- `clone_status` (new, src/daemon/clone_status.rs) lists retired clones that are still inside their undo window as `{clone, name, seat, retired_at}`, and leaves out purge tombstones (`is_tombstone()`). `register_status` now takes the store and registers it as the `clones` status component, read at `store.head()` on every status call. This delivers spec §14.1: "`status` lists retired-unpurged clones".
- `Services` gains `seat_view: Arc<dyn ThreadsSeatView>` and `seat_view_error: SeatViewError`. `Services::new` defaults to `FakeThreadsSeatView`, which reads every seat as `Unresolved` and so never makes threads speak. `from_env` replaces both with the production pair from `production_ports`. `production_threads` is kept as a thin wrapper for existing callers (tests/daemon_composed.rs, tests/threads_real.rs).
- `production_ports` builds one `LazyMap` and shares it through `Arc` as both `PaneSeatMap` and `ThreadsSeatView`. This matches the spec's Key abstractions: "the same struct as `ThreadsSeatMap`, sharing its client, host and clock", "built once in daemon compose next to `ThreadsSeatMap`". `compose_with` passes `seat_view` into `RuntimeLoop` (src/observe/step.rs:120). There it stays unused until SeatObservation (hg-66u.28, open).
- `LazyMap` was refactored into a private `map()` helper. That helper locates the endpoint and builds or rebuilds the cached `ThreadsSeatMap`, and `seat_for`, the new `resolve_seat` and the new `inspect` all call it. `inspect` records a `Rejected`/`Unsupported` answer in the shared slot and clears the slot on success. Transport errors leave the slot alone, which matches the spec's error split: Disconnected/ServiceBusy versus Rejected/Unsupported.
- `register_threads_connection_with` puts `seat_view_error` into the `threads` status component when the slot is set. The old `register_threads_connection` delegates to it with an empty slot.
- The test-support Herdr replay log (`mod fakes`, `cfg(feature = "test-support")`) records `StartAgent.env` with `serde(default)`, so logs recorded before this change still replay.

## Call-guard regression in LazyMap

src/threads/namespace.rs:127 documents the contract: "Shared production call boundary; the guard must cover the external call only." The guard is a shared flock on `.graph-local/threads-calls.lock` plus an epoch check (`check_call`). `ThreadsReconnect` takes the same lock exclusively in the writer's apply phase (src/writer/mod.rs:275-280). Holding the guard therefore makes a namespace transition wait for in-flight external calls, and makes new calls see the new epoch.

At base, `LazyMap::seat_for` held `_guard` across `map.seat_for(pane).await` (base compose.rs:853-887). Every `LazyThreads` method still holds it across the whole call (compose.rs:792-889). At head, `_guard` is a local of `LazyMap::map()` (compose.rs:915), so it is dropped when `map()` returns. The external calls `self.map().await?.seat_for(pane)`, `.resolve_seat(pane, op)` and `.inspect(seats)` (compose.rs:960, 968, 978) then run with no call guard at all. That is a regression for `seat_for`, and the same gap applies to both new methods.

`resolve_seat` is the most exposed of the three. It performs a threads-side `ResolveSeat` and is reachable in production through the resolve-seat want (src/threads/effects.rs:1174). Its result is admitted write-once into `threads_seat` through bookkeeping sub `threads_seat`, which is not in the writer's epoch-checked `namespace_write` list (src/writer/mod.rs:286-291). A reconnect that commits while such a call is in flight is therefore not serialized against it.

The fix is local and small. Keep the guard alive across the delegated call: have `map()` return the guard together with the map, or take `self.endpoint.guard()` in each of the three methods before calling `map()`.

## Other observations

- The `seat_view_error` slot also captures `Rejected` errors that come from endpoint setup rather than from threads refusing the seat contract: a malformed explicit socket/instance pair, a namespace `check_call` recovery error, or a `claim_host` failure, all mapped to `Rejected` in `Endpoint`/`map()`. In these cases `status` shows the real message, but under a field documented as "threads refuses the seat contract (an older service)". The message is still accurate, so this is only a labeling issue.
- `clone_status` uses `layout::all_clones(tree).unwrap_or_default()`. A store read error therefore shows as an empty `retired_unpurged` list instead of an `error` field. The `store.head()` failure branch right beside it does report an error.
- `Services::new` builds a `FakeThreadsSeatView` in non-test code. It is harmless because the fake never speaks (Unresolved) and `from_env` always replaces it. The crate already exports `FakeThreads` outside `cfg(test)`, so this follows existing practice.

## Verification performed

- I ran `cargo test --locked --offline --lib -- daemon:: plan::tests::clone_status` in the ht-build toolbox. It used the caller's sources at head (submodule at 95e9ebac, v3.0.0) and a review-owned target dir reflinked from the run worktree, /var/home/alepar/AleCode/herdr-graph/.review-inv-4aa37d70-cluster-10/target, which I removed afterwards. Result: 48 passed, 0 failed. The run included `seat_view_rejection_is_visible_in_threads_status_and_records_nothing`, `cached_client_rediscovers_and_refuses_a_fresh_node_at_same_socket`, `reconnect_background_connection_probe_reports_healthy` and `clone_status_lists_retired_unpurged_clones`.
- I checked the guard regression statically against the base and head sources (namespace.rs:121-145, writer/mod.rs:275-291, compose.rs at both revisions). No test exercises a concurrent reconnect against a LazyMap call.

## Seam handoff (paths that leave this cluster)

These rows are traced only as far as this cluster's boundary. The production paths that complete them belong to the seam reviewer.
- B1/B3 (threads is authoritative for location and liveness): this cluster wires the production `ThreadsSeatView` into `RuntimeLoop`, but at head nothing calls `inspect` in production. The consumer, SeatObservation, is hg-66u.28 and still open.
- B2 (threads unreachable means binding waits): `LazyMap::inspect` returns transport errors unchanged and invents no state, as the port contract requires. Whether the observer holds the last state is for SeatObservation to show.
- B7/B10 (undo restores the same seat): the `clones` status component lists the seat of every retired-unpurged clone. Restore intents, un-retire and relaunch are produced outside this cluster (hg-66u.29, .30, .32).
- B4/B5 (unattended restart and handoff): this cluster only adds the env field to the test replay log. The resume driver and handoff reconnect are outside it.

## Verdict
not ready (LazyMap drops the threads call guard before seat_for/resolve_seat/inspect reach threads, regressing the base guard on seat_for)

## Must fix before landing
- src/daemon/compose.rs:914-978: `LazyMap::map()` drops `_guard` (the shared `threads-calls.lock` flock plus epoch check) before the external call. As a result `seat_for` (guarded at base), `resolve_seat` (a threads write that feeds the write-once `threads_seat`) and `inspect` all reach threads unguarded, which breaks the namespace.rs:127 contract and lets a `ThreadsReconnect` commit mid-call. Keep the guard alive across each delegated call.

## Untested scope
- No test shows that a `LazyMap` call holds `threads-calls.lock` for its whole duration, for example a test where an exclusive holder blocks `seat_for`/`resolve_seat`/`inspect`. Such a test would have caught the guard regression.
- The clear-on-success branch of `LazyMap::inspect`'s `seat_view_error` slot is not exercised, because it needs a service that answers `SeatInspect`. Only the Rejected path is tested, and that path is simulated through an explicit-endpoint error rather than a real older service answering `Unsupported`.
- No composed-daemon test checks that the `clones` status component is registered and reachable through `status`. Only `clone_status` itself is tested, in src/plan/tests.rs.
- The `clones` component's `store.head()` error branch and the error swallowed by `all_clones(...).unwrap_or_default()` are not tested.

## Deferred OK
- `seat_view_error` is also filled by endpoint-setup `Rejected` errors (bad explicit endpoint, namespace recovery, claim_host), which the field's doc describes as an older service refusing the seat contract. The message itself stays accurate.
- `clone_status` hides a store read error behind an empty `retired_unpurged` list. It could report `{"error": ...}` the way the `store.head()` branch does.
- `Services::new` defaults `seat_view` to `FakeThreadsSeatView` in non-test code. This is harmless (it never speaks, and `from_env` overrides it) and consistent with the exported `FakeThreads`.
- `ThreadsReconnect` never rewrites `threads_seat` or `seat_intent` (src/threads/reconnect.rs has no reference to either), so seat ids from the old node outlive a reconnect. This is outside this cluster and predates the guard regression; it is noted for the seam reviewer.
