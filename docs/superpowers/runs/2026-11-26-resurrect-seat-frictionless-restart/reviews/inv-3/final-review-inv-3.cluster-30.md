# Final review inv-3, cluster 30 (src-threads)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified with `git rev-parse --verify <sha>^{commit}`). Review source bound to the ledger by `bind-review-source` (exit 0). Caller checkout observed at HEAD 11ac415, with only `evidence/sweep/` untracked (the background sweep's output), before and during the review. No cargo run, no worktree created; this review reads code only.

Files: src/threads/adapter.rs, assignment.rs, effects.rs, fake.rs, mapping.rs, service_tests.rs, tests.rs (slices 000191 to 000197). Spec: docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md, which has a `## Requirement baseline` (B1 to B4, no human scope changes).

`git diff --stat 985d1eb..11ac415 -- src/threads` is empty: the only commits since the inv-2 head (hg-lnw.30, cd0e869) touch src/reconcile/herdr_exec.rs and src/reconcile/tests.rs. This cluster's code is the same as at inv-2.

## Requirement baseline trace

- B1 (undo of a closed keyless seat brings back the original seat): `ThreadsSource` wants -> `clone_wants` -> `resurrectable` (Restore intent, `threads_done: false`, fact `Retired{reversible: true, keyed: false}` on the Restore seat, verified pane) emits `threads.resurrect_seat`. `ThreadsExecutor::resurrect_seat` re-checks the want, reads `replace` from the pane map, calls `ThreadsPort::resurrect_seat`, and on `Done` or `AlreadyResolved` writes `AcknowledgeResurrected` (sets `threads_done` and retires Ordinary Pending invitation rows on that seat in the same write). The real adapter sends `ResurrectSeat{seat, target, host, replace, operation}` over the v4 session. Keyed reversible facts stay on `UnretireSeat` (`unretirable` now requires `keyed: true`). Final facts keep succession. `mapping::seat_status` derives `keyed` from `continuity_key`. Threads 3.6.0 `seat_view` keeps the key on a bound keyed seat, so a keyed reversible close reads keyed. Delivered in this cluster. The undo command creating the Restore intent, the gated restore launch and the `clones.resurrect_held` row are the seam reviewer's to trace.
- B2 (frictionless Herdr restart): `ThreadsSource` wants include `rebind_wants`, which emits `threads.rebind_held_seat` on every pass for an active seated clone whose fact reads `Unresolved` at or after `incarnation:changed_at`, whose bound pane is live, and which has no `rebind_refused` for the incarnation. The executor maps `Done`/`AlreadyResolved` to done, retryable refusals to Transient (retried after the grace too), and definitive ones to a single `rebind_refused:<clone>` meta. `is_implied` re-derives liveness from `rebind_wants`. Delivered in this cluster. The keyless relaunch gate, `restart_covered` and `clones.restart_held` live in src/reconcile and src/daemon (seam).
- B3 (v4 via the upstream level): `PersistentServiceClient::new(..).with_level(GRAPH_SERVICE_LEVEL)` with `V4`. A refused registration (`Unsupported`) maps to the named `level_refused` error with no v3 or Notify fallback (`delivery_capability` no longer downgrades on `Unsupported`). `discover` enforces the 3.6.0 floor from the descriptor's `software_version`. Delivered.
- B4 (upstream rebind of a held seat, graph calls it before relaunch): `rebind_held_seat` is `ResurrectSeat` with `replace: None` on the saved pane (`restore(seat, pane, None, op)`); `SeatAlreadyResolved` maps to `AlreadyResolved`. Delivered in this cluster. Ordering against the relaunch is the reconcile gate's (seam).

## Code observations

- `restore` forgets a definitive (non-pending) completion from the client intent journal, so a retry under the same key is a fresh decision; pending ones stay queued and are replayed with their original envelope. `replay_pending` only surfaces errors that are still pending, so another key's definitive refusal is never reported as this op's answer.
- `restore_outcome` classifies transport, busy and outcome-unknown codes as retryable and everything else as definitive, consistent with `SeatRestoreRefusal::retryable` in src/ports/threads.rs.
- The resurrect op key carries the refusal count, and a re-plan with unchanged `replace` and pane is deferred without a call. This matches the hg-lnw.27 post-implementation note.
- The fake's seat-lifecycle model (opt-in `enable_resurrect`) covers both windows, `replace` must be bare, target already owned, already resolved, held-seat pane mismatch, reconcile pending and foreign key. It mirrors the 3.6.0 contract as the spec states it.

## Verdict
ready

## Must fix before landing
- none

## Untested scope

- No test in this cluster drives `ThreadsExecutor::resurrect_seat` against the real `ThreadsSeatMap`, whose `seat_for` issues `ResolveSeat` and can mint a bare seat that then becomes `replace`; only tier 3 (tests/adopt_private_herdr.rs, outside this cluster) exercises that combination.
- `daemon_floor` has no case for a pre-release `3.6.0-rc.N` string, which passes the floor.
- A resurrect that keeps answering `NotFound` while graph's fact still reads `Retired{Reversible}` retries without bound and never gets a `resurrect_held` row (that row keys on the definitive-refusal meta); no test pins this behaviour.
- This review ran no tests; it relies on the committed evidence and on the background sweep the caller runs.

## Deferred OK

- `resurrect_seat` reads `replace` from `seat_for`, which on the real map calls `ResolveSeat` and may mint a bare seat when the pane has none, so production usually passes a graph-minted `replace`; threads retires it in the same decision, so it works, but the spec's "otherwise None" wording does not say so.
- `daemon_floor` accepts `3.6.0-<pre>` builds as meeting the 3.6.0 floor; harmless unless a pre-release without ht-lzhi or ht-0f0r is deployed.
- `restore` ignores the result of `forget_completed`; a failed forget would replay a cached refusal once under the same key, bounded and self-healing (noted at task 9).
- Registration refusal detection (`is_level_refused`) keys on the error message prefix; a reword of `level_refused` would silently change the resurrect mapping from `Unsupported` to the generic local-error path.
- Persistent resurrect `NotFound` on a fact that still reads reversible has no operator-visible row; per spec §3 it is a deferred retry, and it needs a threads and graph disagreement to persist.
