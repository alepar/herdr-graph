# Final review, cluster 3 (daemon), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298 (both verified with `git rev-parse --verify <sha>^{commit}`). Caller checkout observed at HEAD b9caa7f, clean, at the start and end of the review. The review source binding was recorded in the ledger via `bind-review-source` (exit 0) before this file was written.

## Scope and method

Cluster files: src/daemon/compose.rs, src/daemon/doctor.rs, src/daemon/mod.rs, src/daemon/roster_live.rs (slices 000060-000063). Spec: design doc docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md (Requirement baseline B1-B8, S1; §2.1, §2.2 `--live`, §2.3 Codex trust). Code was read at the head commit; source beyond the diff was opened only to trace the threads-degraded path (LazyMap::inspect in compose.rs, Endpoint::locate, observe/step.rs observe_seats) and the overlay's incarnation check (inspect/roster.rs). No tests were run in this review.

Prior reviews of this cluster (inv-4b650ab2 at 6eb6cec, inv-b9c53a9d at 2dfa656) both returned not ready on the same threads-degraded defect. `git diff --stat 2dfa656 b9caa7f -- src/daemon src/inspect/roster.rs src/observe src/threads tests/inspect_live.rs` is empty, so the defect is unchanged at this head, and no bead under hg-blw tracks it (all children closed).

## What the cluster delivers

- roster.live is registered in the production composition (compose_with, reached from compose via Services::from_env) with the real Herdr client, the LazyMap seat-view error and refusal slots, and the runtime's SeatLost slot. It pins one HEAD inside spawn_blocking, resolves the scope against that HEAD, builds the roster through inspect::roster::build_with_bindings (the daemon builds no second roster), takes exactly one Herdr snapshot, and passes a snapshot failure through as an error string rather than `.ok()`. live_pane_matches uses `Incarnation::same_server` against the snapshot's incarnation in the overlay. This covers B8 and S1 (the unmanaged flag is forwarded to the overlay) for the Herdr side.
- doctor gains harness_hook_checks: an outdated owner-marked hook warns with the setup command, and a current Codex hook warns that trust must be granted in /hooks, which graph cannot read (§2.3). warn_check keeps ok=true, so doctor's outcome is not failed by it.
- Unit tests cover one snapshot per call with no commit, snapshot failure and a missing client, a set seat_view_error, and rejected seat and clone scopes; doctor_flags_outdated_hook covers current and outdated hooks for both harnesses.

## Threads-degraded path traced

- src/daemon/roster_live.rs:64-82 reports ThreadsRead::Ok whenever the SeatViewError slot is None.
- That slot is written only by LazyMap::inspect (compose.rs, around line 1105), and only for Rejected/Unsupported; `Err(_) => {}` leaves it untouched for every other error.
- When herdr-threads is not installed or not running, Endpoint::locate returns ThreadsError::Disconnected ("herdr-threads state directory not found" / "herdr-threads is not running"), so the slot stays None (or holds a stale earlier value), refusals keep the last successful read, and observe_seats maps the failure to its own ThreadsRead::Unavailable without exposing it.
- Result: with the threads service down, roster --live returns live_unavailable null and seat_refusals/seat_lost as empty or stale values, presented as a complete read. Spec §2.2 requires "threads seat view not running: seat_refusals/seat_lost are null and live_unavailable.threads names the reason", and §2.1 requires the overlay never be silently empty. The same holds before the first seat read completes after daemon start.
- The tier-3 test tests/inspect_live.rs:188-192 asserts live_unavailable null and threads_live as an object in a private rig; that belongs to the tests cluster, but it pins the defective behaviour rather than catching it.

## Verdict
not ready (roster --live still reports threads as fully read when the threads service is unreachable, violating spec §2.2 degraded-state rule for B8)

## Must fix before landing
- src/daemon/roster_live.rs:64-82 with src/daemon/compose.rs LazyMap::inspect: when the threads seat read fails for any reason other than Rejected/Unsupported (Disconnected from Endpoint::locate, transport, discovery) or has not yet completed, roster.live must return seat_refusals/seat_lost null and live_unavailable.threads naming the reason (spec §2.2, B8); record the last seat-read outcome in a slot the handler reads, and add a test that drives a Disconnected seat read instead of writing seat_view_error by hand.

## Untested scope
- No test covers roster.live while the threads service is unreachable (Disconnected) or before the first seat read; the only threads-degraded test sets seat_view_error directly.
- No test covers an integrity StoreError during scope resolution in roster.live and its exit code.
- roster.live with a non-empty seat or teamspace scope, or with all:true, is not exercised at the handler level.
- roster.live with a loaded record present (the instructions object inside the daemon reply) is not exercised at the handler level.
- harness_hook_checks with an unreadable or malformed settings file (the Err arm) has no test.
- No cluster tests were run in this review; pass status at b9caa7f is not measured here.

## Deferred OK
- src/daemon/roster_live.rs:50-52: Scope::resolve errors are all mapped to bad_request; route a store failure through CommandError::from_store so an integrity failure exits 6 under --live as it does daemon-free.
- src/daemon/roster_live.rs:36-41: non-string scope words are dropped silently rather than rejected; only a malformed client can send them.
- src/daemon/doctor.rs:267-270: the Codex re-trust warning is permanent once the current hook is installed, since graph cannot read trust; consider limiting it to a recently changed hook if setup ever records when it rewrote the command.
- src/daemon/doctor.rs:262: a malformed settings file is reported by both harness_gate and harness_hook; cosmetic.
