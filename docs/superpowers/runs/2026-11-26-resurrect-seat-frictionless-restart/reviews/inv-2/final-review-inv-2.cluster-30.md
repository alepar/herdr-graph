# Final review inv-2, cluster 30 (src-reconcile)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 985d1eb6faaa2e92e4e01f430f0f2c07618bf6ad (both verified with `git rev-parse --verify`). Review source bound to the ledger by `bind-review-source` (exit 0). Caller checkout observed at HEAD 985d1eb, clean, before and during the review.

Files: src/reconcile/herdr_exec.rs, src/reconcile/planner.rs, src/reconcile/tests.rs (slices 000150 to 000152). Spec: docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md, which has a `## Requirement baseline` (B1 to B4, no human scope changes).

## What the cluster changes

- `keyless_start_gate` (herdr_exec.rs): a non-relaunch `StartAgent` of a seated keyless clone with no seat intent, inside the 90 s grace after an incarnation change, is deferred in 5 s polls until the fact reads `Resolved` on the clone's own pane, read at or after `changed_at`, and `seat_unresolved:<clone>` names this incarnation. Past the grace `grace_remaining` returns None, so the gate cannot hold beyond 90 s. This matches spec §4 "Gate" and the accepted full-grace wait when `Unresolved` is never read.
- `seat_unresolved:<clone>` is written by `plan_resume` on the first fresh `Unresolved` read of an incarnation, via `put_meta_if_changed` (the pattern already used there for `resume_blocked`). `rebind_refused_key` and `rebind_refused_in` are the incarnation-scoped refusal record that the threads cluster writes and the status cluster reads.
- `restart_covered` counts a `launched:` meta as had-agent for non-shell harnesses (§4.3). In `start_needed` this changes nothing, because `start_needed` already requires that no `launched:` meta exists. In `plan_resume` it opens the restart relaunch for a clone graph launched whose session was never captured. Test `restart_arm_needs_an_earlier_agent` was updated to the new behaviour.
- hg-lnw.3: a keyless clone's restore launch no longer records `launched:`, so `start_needed` still relaunches it once when its agent exits with the pane open, like an adopted clone. One restore launch per epoch still holds, through the `MarkLaunched` admission.

## Baseline trace for this cluster

- B2 (frictionless restart). The production path is the daemon reconcile loop, then `plan_effects` and `plan_resume`, which emit `RelaunchOccupant` or `StartAgent`, then `HerdrExecutor::start`. Within this cluster the B2 pieces are delivered: keyed relaunches never wait on the gate, keyless fresh starts open on placement or at the grace, never-observed keyed clones relaunch fresh, and shell seats launch nothing. The rebind effect's emission and its outcome mapping live in src/threads/effects.rs, and the `restart_held` row lives in src/daemon/clone_status.rs. Those paths leave this cluster and are the seam reviewer's to trace. The reconcile tests here exercise them end to end on the fakes.
- B1 (resurrect on undo). This cluster only touches the restore launch's bookkeeping (hg-lnw.3). The resurrect call itself is in the threads cluster (seam).
- B3 and B4 are pin and adapter rows. They are not in this cluster.
- Documented divergence: a keyless `RelaunchOccupant` waits the full 90 s grace and is not opened early by placement. The Post-Implementation Notes record this. The wait is bounded and needs no operator action, so B2 still holds.

## Lost-response gap in the hg-lnw.3 fix

`HerdrExecutor::start` re-runs a `StartAgent` row in `EffectStatus::Unknown` (a lost response; see `lost_response_*` tests). When it finds the agent on the pane, it calls `self.record_launch(p)` unconditionally. The `restore_launch && p.keyless` exemption sits after this early return, so it does not apply there. Take an undo-restore launch of a keyless clone whose response is lost. The mark was admitted before the dispatch, so the intent reads `Restore { launched: Some(_) }`. The re-run then writes `launched:`, and once the agent exits with the pane open, `start_needed` never relaunches it. That is the trial finding 3 defect again, on the lost-response path. To fix it, apply the same exemption in the Unknown branch: skip `record_launch` when `p.keyless` and the intent is a `Restore` that is not restart-owned.

## Findings

- herdr_exec.rs `start`: the `EffectStatus::Unknown` branch records `launched:` for a keyless undo-restore launch whose response was lost, which undoes the hg-lnw.3 fix on that path (the agent is not relaunched after it exits).
- herdr_exec.rs `keyless_start_gate` skips the gate for any `seat_intent.is_some()`, not only a pending `Restore`. A keyless clone carrying another intent variant across a restart starts without waiting for placement. Harmless for B2 (the start still enrolls), but it is wider than the doc comment says.
- The gate applies to every keyless non-relaunch start inside the grace, including an ordinary exited-agent relaunch that has nothing to do with a restart hold. Worst case it waits 90 s. This is bounded and accepted by the spec's reasoning.

## Verdict
not ready (keyless undo-restore launch with a lost response still records launched:, re-breaking hg-lnw.3 on that path)

## Must fix before landing
- src/reconcile/herdr_exec.rs `HerdrExecutor::start`, Unknown-status branch: apply the hg-lnw.3 exemption (no `record_launch` for a keyless clone's restore launch) there too, and add a reconcile test with a LostResponse fault on the restore `StartAgent` followed by the agent exiting.

## Untested scope
- No test covers the lost-response (Unknown) re-run of a keyless restore StartAgent, or the relaunch after its agent exits.
- No test shows that a keyed non-relaunch StartAgent inside the grace bypasses keyless_start_gate. The code path is trivial (`!p.keyless`).
- No test covers a keyless clone with a non-Restore seat intent across a restart, which bypasses the gate.

## Deferred OK
- Narrow the gate's `seat_intent.is_some()` bypass to a pending Restore, or reword the doc comment to match the broader behaviour.
- Keyless RelaunchOccupant does not open early on placement (full 90 s grace); this is documented in the design's Post-Implementation Notes and stays bounded.
