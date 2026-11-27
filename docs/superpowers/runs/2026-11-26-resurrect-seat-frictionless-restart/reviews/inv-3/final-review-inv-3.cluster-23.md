# Final review inv-3, cluster 23 (src-daemon)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified with `git rev-parse --verify`). Caller checkout observed at HEAD 11ac415 with untracked `evidence/sweep/` only; no cargo was run (a sweep runs in the integration worktree). Inputs: slices 000175-000177 (`src/daemon/clone_status.rs`, `src/daemon/compose.rs`, `src/daemon/doctor.rs`), the digest, `bd show hg-lnw`, and the run design doc with its `## Requirement baseline` (B1-B4, no human scope changes).

## Scope of the cluster

- `clone_status.rs`: adds `clones.restart_held` (spec §4.5) and `clones.resurrect_held` (spec §3) rows, plus `operator_rebind_remedy` printing the `herdr-threads human seat rebind SEAT --pane PANE --operator` form (the divergence is recorded in the design's post-implementation notes, hg-lnw.27).
- `compose.rs`: the production `LazyThreads` port gains `resurrect_seat` and `rebind_held_seat`, each taking the namespace guard and forwarding to `ServiceThreads`; endpoint-local failures (explicit config, namespace guard and bind, host claim, intent dir) become `Disconnected("graph-local: ...")` instead of `Rejected`, so `SeatRestoreOutcome::from_error` maps them to the retryable `Unreachable`; host claim is factored into `Endpoint::host` and now also feeds `ServiceThreads::with_system_clock`; a below-floor daemon (`ErrorKind::Unsupported` from discovery) surfaces its own message as a retryable disconnect.
- `doctor.rs`: `restart held` and `resurrect held` WARN findings from the `clones` component, and an upgrade-step hint when the threads disconnect names a v4 refusal or the 3.6.0 floor.

## Baseline trace

- B1 (resurrect on undo): this cluster supplies the production port path. `compose.rs:148` installs `LazyThreads` as the daemon's threads port; `LazyThreads::resurrect_seat` reaches `ServiceThreads::resurrect_seat` (`src/threads/adapter.rs:772`), not a stub. The degraded fallback (`resurrect_held` row, doctor finding with operator remedy) is produced by the live `clones` status provider (`compose.rs` `register_status` -> `clone_status_with_journal`) and read by `doctor()`. The executor decision (`ThreadsExecutor::resurrect_seat`) is outside this cluster: seam reviewer's to trace.
- B2 (frictionless restart): `LazyThreads::rebind_held_seat` reaches `ServiceThreads::rebind_held_seat` (`adapter.rs:783`); graph-local failures are now retryable, so a transient local fault (stale epoch snapshot, host claim, intent dir) cannot record a definitive `rebind_refused` and strand the seat for the incarnation. The `restart_held` row matches §4.5: fresh `Unresolved` read (`read_at >= changed_at`), after `SEAT_LOST_AFTER` from `incarnation:changed_at`, reason from `rebind_refused_in` else "rebind did not complete", clears when the fact reads anything else, active clones only. Planner emission and keyless gate are outside this cluster: seam reviewer's.
- B3 (v4 via upstream): doctor names the upgrade step for both refusal forms (`level_refused` below 3.5.0, `daemon_floor` for 3.5.x); tested in `threads_check_names_the_upgrade_step_for_v4_refusal` and `..._for_a_3_5_daemon`. Pin and adapter registration are other clusters.
- B4 (upstream rebind for restart-held seats): same production path as B2; the remedy string is also exercised against real threads in tier 5 (`operator_rebind_remedy` is public for `tests/threads_real.rs`).
- All four rows are delivered for the part this cluster owns; no baseline gap found inside the cluster.

## Correctness notes

- `from_error` keeps `Rejected` definitive (`InvalidRequest`) and only threads' own answers now produce it; the five new tests drive both restore calls through the real `LazyThreads` for each local failure kind and assert `Refused{Unreachable, retryable: true}`.
- Reclassifying local failures as `Disconnected` also changes non-restore effects: `port_outcome` maps `Disconnected` to `Transient`, so a permanent local misconfiguration (for example half-set explicit socket/instance env) now retries with backoff instead of failing the effect. It still surfaces as `threads: connected=false` with the `graph-local:` message in status and doctor, so it is not silent. Acceptable.
- `seat_view_error` no longer shows a graph-local failure (the test now uses a real `Unsupported` answer from a stand-in socket); the connection probe covers that visibility.
- `ServiceThreads` construction now requires a successful host claim. Before, only the seat map needed it; now every lazy threads call fails (retryably) if the Herdr socket's parent cannot be claimed. Consistent with the seat map and covered by `lazy_host_claim_failure_is_a_retryable_restore_refusal`.

## Findings

- `restart_held` does not check that the bound pane still exists. A clone whose pane vanished across the restart gets a `resume_blocked` row ("retire the clone") and, if its fact reads `Unresolved` after the grace, also a `restart_held` row whose remedy rebinds onto that missing pane. Spec §4 keeps "pane missing -> resume_blocked unchanged"; the second, contradictory doctor WARN is operator noise, not a functional defect (the planner does not emit the rebind there).
- The doctor wiring of `restart_held_check` / `resurrect_held_check` into `doctor()` has no doctor-level test; the check functions and the status rows are unit-tested, and the status rows are asserted in tier-3 and sweep tests.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- `doctor()` including the `restart held` and `resurrect held` findings in a full report (only the per-check functions are unit-tested).
- A permanent graph-local misconfiguration on non-restore effects now retrying as `Transient` indefinitely has no test pinning the intended behaviour.

## Deferred OK

- Suppress the `restart_held` row (or its rebind remedy) for a clone that also has a `resume_blocked` row, so doctor does not print two contradictory remedies for a missing pane.
