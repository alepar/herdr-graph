super-roast verdict: Nit (1 confirmed) [converged]
mode: pr        iteration: post-cap audit
profile (assumed): herdr-graph is a single-operator local developer tool: a Rust daemon that manages coding-agent panes and seats in the Herdr terminal multiplexer, paired with a per-user herdr-threads daemon on the same host. It has no network-exposed surface and no external users. The state at stake is the operator's own agent sessions and seats, and upgrades are manual. Blast radius is low, so coverage gaps with no demonstrated misbehaviour weigh as Nits.
inputs: super-auto/resurrect-seat-frictionless-restart@40a02b9e vs r2-head@ec289035
delta vs prior: 1 new confirmed (0 Blocking) · 0 carried (0 Blocking) · 0 resolved · 0 regressed (0 Blocking) · 3 punch-listed (open)
coverage: scouts 13/13 (correctness, security, premortem, simplicity-design, hot-path-perf, concurrency-async, regression, testing, test-causality-lifecycle, test-isolation-environment, test-suite-cost, hygiene-docs, observability) · raw 1 → deduped 1 → panel 1 · spot 0 · promoted 0 · judge completion 100% · remainder-capped: 0 · review readiness: coverage complete, provenance available — basis: "/var/home/alepar/AleCode/herdr-graph/.worktrees/super-auto/resurrect-seat-frictionless-restart/.superpowers/sdd/hg-lnw-plan/assembled/roast-code-resurrect-seat-frictionless-restart-20261126T231316-audit/review-inputs.json: status complete, no gaps, captured ec28903535a259b47ea45d7c16e81c6643e06ca7..40a02b9e0042c6f4ecb6af47b6037f2ae886f43d"
independence: same-family (Claude) — seat-differentiated panel · rung: manual fan-out
seat-agreement: panels 1 · rr 1.00 · rg 1.00 · fg 1.00 · unanimous 1.00 · ground-loo 1.00 (n=1) · reproduce 1/0/0 · refute 1/0/0 · ground 1/0/0
lane-yield (found/confirmed/unique/refuted): correctness 0/0/0/0 · security 0/0/0/0 · premortem 0/0/0/0 · simplicity-design 0/0/0/0 · hot-path-perf 0/0/0/0 · concurrency-async 0/0/0/0 · regression 0/0/0/0 · testing 1/1/1/0 · test-causality-lifecycle 0/0/0/0 · test-isolation-environment 0/0/0/0 · test-suite-cost 0/0/0/0 · hygiene-docs 0/0/0/0 · observability 0/0/0/0

## Confirmed findings
- [Nit] src/reconcile/herdr_exec.rs:615 — The new keyless lost-response test never exercises two of the three discriminators in the added `restore_launch` condition (`!restart_owned` and `*epoch == e.op`), so deleting either would leave the suite green. (new this iteration) [lanes: testing]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: herdr_exec.rs start() computes `restore_launch = !restart_owned && matches!(seat_intent, Some(Restore { epoch, launched: Some(_), .. }) if *epoch == e.op)` and skips record_launch only when `restore_launch && p.keyless`.
    The only added test, `undo_restored_agent_whose_start_response_was_lost_is_relaunched_after_exit` (src/reconcile/tests.rs ~5300), uses a Restore intent whose epoch equals the start effect's op and sets no incarnation change.
    So `restart_owned` is always false and the epoch always matches.
    The other agent.start LostResponse tests (tests.rs:1174, 1843) do not combine a keyless clone, a marked Restore and a restart relaunch.
    All three seats confirmed at Nit. No production misbehaviour is shown for the two uncovered inputs.
    demoted: none needed; seats all rated Nit, and the profile (single-operator local tool) agrees with a coverage-only gap on narrow races.
  fix-shape hint: Add two tests: a restart relaunch of a keyless clone whose Restore launched before the incarnation change with a lost response, and a stale-epoch Unknown start under a newer marked Restore.

## Not verified (beyond panel cap)
- none

## Not verified (dedupe failed or judge lost)
- none

## Beyond remainder cap (count only)
- none

## Rejected (with reason)
- none

## Unverified nits (spot-checked)
- none

## Escalations (need human)
- none

Prior findings, tracked outside the recognized sections: three prior confirmed findings are punch-listed (open) and were not re-surfaced by any packet this round: [Should-fix] src/threads/adapter.rs:374; :423; :496 (busy ResurrectSeat intent gates unrelated threads mutations); [Nit] src/daemon/compose.rs:764 and related (graph-local Endpoint failures retyped to Disconnected); [Nit] src/threads/adapter.rs:272; :467. The caller left them unfixed. The prior Unverified nit at tests/e2e_private_herdr.rs:3522 was rejected at spot check and was not re-surfaced.
