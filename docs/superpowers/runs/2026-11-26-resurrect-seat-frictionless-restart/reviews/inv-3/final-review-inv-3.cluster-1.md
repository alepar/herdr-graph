# Final review inv-3, cluster 1 (root)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified with `git rev-parse --verify <sha>^{commit}`). Caller checkout HEAD observed at 11ac415, status `?? evidence/sweep/` only; no cargo run (a background sweep owns the integration worktree). Cluster files: CHANGELOG.md, CURRENT-DESIGN.md, Cargo.lock, Cargo.toml, README.md (slices 000001 to 000005). Spec: `bd show hg-lnw`, design `docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md` at head, which has a `## Requirement baseline` (B1 to B4, no human scope changes).

## What the cluster changes

- Cargo.lock moves `herdr-threads` 3.1.0 to 3.6.0. The submodule gitlink at head is `7c3c09082e8894ebceec9c060b769b07e359a2ad`, which is tagged `v3.6.0` in the submodule and whose Cargo.toml says `version = "3.6.0"`; this matches README's release-commit claim.
- Cargo.toml only rewrites comments: the `threads-service-ack` feature comment now describes v4 registration, the Notify fallback for a daemon without send-time receipts, and the named errors for older daemons; a pin comment is added above the `herdr-threads` path dependency. No dependency or feature change.
- CHANGELOG Unreleased gains six Changed entries (3.6.0 pin, keyless undo resurrect windows, frictionless restart, undo-restored relaunch, the two held status rows, the version floor) and a migration note (upgrade threads first, schema 19 one-way).
- CURRENT-DESIGN.md updates the pin paragraph, the Undo keyless paragraph (resurrect want, op key with refusal count, invitation reset, `resurrect_held`), adds the restart rebind paragraph, and replaces the "resurrect out of scope" line.
- README rewrites the Keyless seats section (windows, restart rebind, the two status rows), the threads integration paragraph (v4, no fallback), the dependency-target paragraph (3.6.0, schema 19), adds the 3.6.0 floor paragraph and the tier-5 test-support binary note.

## Claims checked against the code at head

- Named errors: `level_refused` in src/threads/adapter.rs produces `herdr-threads daemon refused service_session_v4 (<detail>); upgrade the herdr-threads daemon to >= 3.6.0`; `daemon_floor` produces `herdr-threads daemon must be at least 3.6.0 (found <version>); upgrade the herdr-threads daemon and restart it`, called from `discover` on the published descriptor's `software_version`. Strings in README, CHANGELOG and CURRENT-DESIGN match verbatim.
- The adapter builds its client with `.with_level(GRAPH_SERVICE_LEVEL)` where `GRAPH_SERVICE_LEVEL = ServiceSessionLevel::V4` (adapter.rs:62, :310): B3 (U1, caller-chosen level, pinned upstream release) is delivered from the production adapter.
- Status rows: `restart_held` and `resurrect_held` in src/daemon/clone_status.rs carry `{clone, name, seat, pane, reason, since, remedy}`; the reason text "rebind refused: ..." / "rebind did not complete", the 90 s (`SEAT_LOST_AFTER`) thresholds from incarnation change and from the first refusal, and the remedy `herdr-threads human seat rebind SEAT --pane PANE --operator` (`operator_rebind_remedy`) match the docs. Doctor checks are named `restart held` and `resurrect held` (src/daemon/doctor.rs:147, :170), as documented.
- The undo preview window wording (src/undo/preview.rs:506, :509) and the adoption plan keyless warning (src/plan/seat_adopt.rs:516) exist, as CHANGELOG claims.
- Production wiring for B1/B2/B4: `ThreadsPort::resurrect_seat` and `rebind_held_seat` are implemented by the real adapter (adapter.rs:772, :783) and forwarded by the daemon composition (src/daemon/compose.rs:1024, :1040); the planner emits `threads.resurrect_seat` and `threads.rebind_held_seat` wants (src/threads/effects.rs:576, :780) and the executor dispatches them (effects.rs:2782, :2803). `restart_covered` lives in src/reconcile/planner.rs:711. The detailed behaviour of those paths (gates, outcome handling, the relaunch fix) belongs to the src clusters and the seam reviewer; at the level these docs describe, the paths exist from production entry points.
- Tier-5 note: `tests/support/threads_bin.rs` builds `threads-bin-test-support` and honours `HG_TEST_THREADS_SUPPORT_BINARY`, as README says.
- docs/verification-matrix.md at head carries the "Threads pin: v3.6.0 and the 3.1.0 to 3.6.0 contract audit" section that CHANGELOG, README and CURRENT-DESIGN point to.
- Remaining `3.1.0` mentions in README and CURRENT-DESIGN are historical (the November 7 qualification, the audit range), not stale pin text.

## Requirement baseline trace

- B1 (resurrect the closed seat on undo): documented consistently in all three docs; production path exists (planner want, executor, real adapter `ResurrectSeat`). Behaviour detail is the src and seam reviewers' to trace.
- B2 (frictionless restart): documented; rebind want is emitted from the production planner and served by the real adapter; held seats degrade to a status row only when threads refuses or is unreachable, as the design's fallback allows. Behaviour detail belongs to the seam reviewer.
- B3 (upstream level fix, pin that release): delivered in this cluster: submodule at v3.6.0, lockfile 3.6.0, adapter `with_level(V4)`.
- B4 (upstream rebind of restart-held seats): documented; production path via `rebind_held_seat` = `ResurrectSeat` with no `replace`. Behaviour detail belongs to the seam reviewer.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No cargo build or test was run for this review (the integration worktree is reserved for a background sweep); the doc-to-code checks above are by source reading at the head commit only.

## Deferred OK
- README and CURRENT-DESIGN say graph refuses a daemon below 3.6.0 by its published version, but `daemon_floor` (src/threads/adapter.rs:227) passes any `software_version` it cannot parse as `major.minor.patch` and treats `3.6.0-rc.1` as 3.6.0; such a daemon then registers or fails on v4 as usual, so the docs could say "a parseable version below 3.6.0".
- Cargo.toml's feature comment still says a daemon without send-time receipts falls back to Notify while README says every daemon graph accepts advertises that capability; both are true (the fallback remains, tested by `send_time_receipt_capability_absence_falls_back_but_disconnect_does_not`), but the two texts could state the same thing the same way.
