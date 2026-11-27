super-roast verdict: Should-fix (2 confirmed) [converged]
mode: pr        iteration: 2 of 3
profile (assumed): Not supplied by the caller; inferred from the repository. herdr-graph is a single-operator local developer tool: a Rust daemon that manages coding-agent panes and seats in the Herdr terminal multiplexer, paired with a per-user herdr-threads daemon on the same host. It has no network-exposed surface and no external users; the state at stake is the operator's own agent sessions, seats and thread memberships, and upgrades are manual and operator-ordered. Blast radius is low, so resilience and observability gaps weigh less here, but a restart that leaves seats unbound counts as real because that is what the change exists to prevent.
inputs: super-auto/resurrect-seat-frictionless-restart@ec289035 vs main@60e287aa
delta vs prior: 2 new confirmed (0 Blocking) · 0 carried (0 Blocking) · 3 resolved · 0 regressed (0 Blocking) · 1 punch-listed (open)
coverage: scouts 17/17 (correctness, security, premortem, simplicity-design, hot-path-perf, concurrency-async, regression, api-contract, observability, testing, test-causality-lifecycle, test-isolation-environment, test-suite-cost, dependency, hygiene-docs, deploy-safety, data-migrations) · raw 6 → deduped 3 → panel 2 · spot 1 · promoted 0 · judge completion 100% · remainder-capped: 0 · review readiness: coverage complete, provenance available — basis: "/var/home/alepar/AleCode/herdr-graph/.worktrees/super-auto/resurrect-seat-frictionless-restart/.superpowers/sdd/hg-lnw-plan/assembled/roast-code-resurrect-seat-frictionless-restart-20261126T231316-r2/review-inputs.json: status complete, no gaps, captured 60e287aadd08af10779df39e625200f28d4b7fa3..ec28903535a259b47ea45d7c16e81c6643e06ca7"
independence: same-family (Claude) — seat-differentiated panel · rung: manual fan-out
seat-agreement: panels 2 · rr 0.50 · rg 0.50 · fg 1.00 · unanimous 0.50 · ground-loo 1.00 (n=1) · reproduce 1/1/0 · refute 2/0/0 · ground 2/0/0
lane-yield (found/confirmed/unique/refuted): correctness 0/0/0/0 · security 0/0/0/0 · premortem 1/1/0/0 · simplicity-design 1/1/0/0 · hot-path-perf 0/0/0/0 · concurrency-async 1/1/1/0 · regression 0/0/0/0 · api-contract 1/1/0/0 · observability 1/1/0/0 · testing 0/0/0/0 · test-causality-lifecycle 0/0/0/0 · test-isolation-environment 0/0/0/0 · test-suite-cost 1/0/0/1 · dependency 0/0/0/0 · hygiene-docs 0/0/0/0 · deploy-safety 0/0/0/0 · data-migrations 0/0/0/0
prior confirmed findings: [Should-fix] src/ports/threads.rs:179; src/daemon/compose.rs:1031 — resolved (commit 9e1dd41, hg-lnw.24: graph-local endpoint failures are now retryable restore refusals; no scout re-surfaced the non-retried rebind; the breadth of that fix is the first confirmed finding below). [Should-fix] CHANGELOG.md:24 — resolved (commit 7101406, hg-lnw.25: the 3.6.0 daemon floor is checked at discovery; CHANGELOG.md:24 and :29 now name 3.5.x; not re-surfaced). [Nit] src/model/clone.rs:100; src/model/clone.rs:107 — resolved (commit d939a73, hg-lnw.26: legacy retired facts take keyed from the clone's keyless; not re-surfaced). [Nit] src/threads/adapter.rs:272; src/threads/adapter.rs:467 — punch-listed (open): the caller left it unfixed and no packet re-surfaced it.

## Confirmed findings
- [Should-fix] src/threads/adapter.rs:374; src/threads/adapter.rs:423; src/threads/adapter.rs:496 — A ResurrectSeat (restart rebind or undo resurrect) that threads answers ServiceBusy stays in ServiceThreads' shared `pending` list and is replayed at the head of every later graph mutation. While threads keeps answering busy for that one seat, every other threads mutation in the process fails with ServiceBusy: other clones' rebinds and resurrects, invites, notify, ensure_thread and finalize. Graph cannot drop the entry, not even after the clone's want goes Obsolete. (new this iteration) [lanes: concurrency-async]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: third_party/herdr-threads src/client/service.rs:909-915 returns `ServiceCallError::Api { pending: Some(key) }` for ServiceBusy ("the intent stays pending for replay").
    src/threads/adapter.rs `call_error` (~385) pushes that key into the process-wide `pending` Vec.
    `restore()` (adapter.rs ~496-512) forgets only `Api { pending: false }`, so a busy ResurrectSeat stays pending.
    Every `mutate_raw` (adapter.rs ~423) calls `replay_pending` first; at ~374 it returns `Err(call_error(e))` on the first still-busy key, so the caller's own operation is never submitted.
    The Obsolete paths in src/threads/effects.rs do not touch the list, and `ServiceThreads::new` reloads it from the intent journal.
    The threads design (third_party/herdr-threads docs/superpowers/specs/2026-11-26-service-restore-held-seat-design.md D3/D5) answers ResurrectSeat with ServiceBusy under observation lag and while a kept claim on the target or OLD persists, so busy is the expected answer right after a Herdr restart, when the rebind wants fire for every held clone.
    Bound, per the reproduce seat: store/kept_continuity.rs `KEPT_CLAIM_TTL_MS` is 1 h, so a kept claim blocks for about an hour at most; lag-driven busy clears within a pass.
    The replay machinery exists on base (60e287a adapter.rs ~233-257); the diff adds the mutation that triggers it predictably, so no seat marked it pre-existing.
    All three seats confirmed at Should-fix by code reading; no seat ran the spike. Severity kept at Should-fix under the profile: the stall hits the restart rebind this change exists to deliver, but it is bounded, so no floor applies.
  fix-shape hint: Do not let one seat's busy restore intent gate unrelated mutations: replay only the caller's own key (or skip past a still-busy restore intent), and forget the intent when its want goes Obsolete. Add a fake-threads test where seat A's ResurrectSeat always answers ServiceBusy and seat B's call must still be submitted.
- [Nit] src/daemon/compose.rs:764; src/daemon/compose.rs:797; src/daemon/compose.rs:801; src/daemon/compose.rs:807; src/daemon/compose.rs:812; src/daemon/compose.rs:872; src/ports/threads.rs:86 — The hg-lnw.24 fix re-types every graph-local Endpoint failure (explicit config, host claim, guard, namespace bind mismatch / recovery required, intent dir) from ThreadsError::Rejected to ThreadsError::Disconnected for every threads port consumer, not only in the restore mapping. Persistent operator-action faults now turn ordinary threads effects and deliveries into endlessly retried Pending rows with no Failed row, attention item, stderr line, seat_view_error or observe log. The only distinction is a `graph-local: ` string prefix that no code reads. (new this iteration; introduced by the fix for the prior finding at src/ports/threads.rs:179) [lanes: premortem, simplicity-design, api-contract, observability]
  verdict: confirmed (reproduce ✗ REJECT at FYI, outvoted 2-1 / refute ✗-survived / ground ✓)
  evidence: Head src/daemon/compose.rs:764/801/807/812/872 map explicit-config, host-claim, guard, bind and intent-dir errors to `ThreadsError::Disconnected("graph-local: ...")`; base 60e287aa mapped them to `Rejected`.
    The Endpoint doc comment (compose.rs:735) and src/ports/threads.rs:86-88 state this as endpoint-wide and port-wide policy.
    src/threads/effects.rs:1546-1553 `port_outcome` maps Disconnected to `ExecOutcome::Transient` and Rejected to `Failed`; src/transcripts/delivery.rs:41-45 does the same.
    src/reconcile/mod.rs:846-855 handles Transient by setting `retry_at` (backoff capped at 5 min) and marking the row Pending, with no eprintln and no `mark_attention`; the Failed arm does both. The ground seat found no max-attempts cap.
    compose.rs:1182 records `seat_view_error` only for Rejected/Unsupported, and src/observe/step.rs:683 logs "threads seat read refused" only for Rejected/Unsupported.
    The rewritten test `seat_view_rejection_is_visible_in_threads_status_and_records_nothing` carries the comment "(A graph-local failure is retryable and never shows here.)".
    Still visible: the Pending row keeps `last_error`, the threads connection probe reports connected:false with the graph-local text, and the doctor `threads` line fails on it.
    The reproduce seat's REJECT rests on those pull surfaces, on base already returning Disconnected for some persistent local faults (state dir not found, base compose.rs:764-774), and on retry being preferable once the operator fixes the cause. The refute seat weighed the same three points and still confirmed, so the tally stands.
    Seat severities: refute Nit, ground Should-fix. demoted: profile states a single-operator local tool with no external users, and the fault stays visible in status and doctor, so losing the push-side signal for a persistent local fault is a Nit here.
  fix-shape hint: Keep the restore retry but narrow it: a typed local variant (as `MutateError::Local` at adapter.rs:397) that `SeatRestoreOutcome::from_error` maps to Unreachable, or split transient local faults (stale-snapshot guard, host claim, intent IO) from namespace recovery-required refusals; alternatively raise an attention item after repeated graph-local Transients.

## Not verified (beyond panel cap)
- none

## Not verified (dedupe failed or judge lost)
- none

## Beyond remainder cap (count only)
- none

## Rejected (with reason)
- none

## Unverified nits (spot-checked)
- tests/e2e_private_herdr.rs:3522 — The new tier-3/tier-4 restart tests run the real daemon with the production 90 s relaunch grace and no test-support knob to shorten it. Each test waits up to about 90 s in setup for its adopted keyless clone's first agent, and two pay a second grace-length wait after the restart. The four new e2e_private_herdr tests measure 715-777 s of serial wall time. [lanes: test-suite-cost]
  spot check: REJECT at FYI. The facts hold: `relaunch_grace` is hardcoded to 90 s at src/reconcile/mod.rs:64, and the evidence logs record 776.74 s and 715.41 s. The design accepts the wait (line 104 "a fresh keyless StartAgent waits the full 90 s grace"; line 151 "no new grace constant is added"), and tier 3 is specified to run against the real daemon (line 136). A test-support knob would decouple `relaunch_grace` from `SEAT_LOST_AFTER`, which the finding itself concedes. The cost is wall time in an opt-in serial tier; no test outcome is affected.

## Escalations (need human)
- none
