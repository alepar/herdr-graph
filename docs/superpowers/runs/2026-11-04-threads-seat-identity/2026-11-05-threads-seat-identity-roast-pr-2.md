super-roast verdict: Should-fix (4 confirmed) [low coverage] [review readiness incomplete]
mode: pr        iteration: 2 of 4
profile (assumed): not supplied by the caller — infer it from the repository and state it — NOT applied: the reporter failed; severities are raw seat severities and routes are the engine defaults
inputs: super-auto/threads-seat-identity@110614cf07fdb40ec2edcc2fb6995eb4e44f6584 vs main@0a84c67554aa3fc8e8caf20db4163683ab30af61
coverage: scouts 17/17 (correctness, security, premortem, simplicity-design, hot-path-perf, concurrency-async, regression, data-migrations, deploy-safety, api-contract, observability, testing, test-causality-lifecycle, test-isolation-environment, test-suite-cost, dependency, hygiene-docs) · raw 10 → deduped 6 → panel 5 · spot 1 · promoted 0 · judge completion 100% · remainder-capped: 0 · review readiness: coverage bounded, provenance unavailable — basis: "review input selection: provenance unavailable; captured source available (collection gap: inv-4aa37d70 cluster-4 bundle missing); inspected: every path in /var/home/alepar/AleCode/herdr-graph/.worktrees/threads-seat-identity/.superpowers/sdd/hg-66u-plan/assembled/roast-code-threads-seat-identity-20261105T073527-r2/diffstat.txt via git diff 0a84c67554aa3fc8e8caf20db4163683ab30af61..110614cf07fdb40ec2edcc2fb6995eb4e44f6584"
independence: same-family (Claude) — seat-differentiated panel · rung: Workflow
seat-agreement: panels 5 · rr 1.00 · rg 0.80 · fg 0.80 · unanimous 0.80 · ground-loo 0.80 (n=5) · reproduce 4/1/0 · refute 4/1/0 · ground 5/0/0
lane-yield (found/confirmed/unique/refuted): correctness 1/1/0/0 · security 0/0/0/0 · premortem 1/1/0/0 · simplicity-design 1/0/0/1 · hot-path-perf 0/0/0/0 · concurrency-async 0/0/0/0 · regression 2/2/0/0 · data-migrations 1/1/0/0 · deploy-safety 0/0/0/0 · api-contract 0/0/0/0 · observability 0/0/0/0 · testing 1/1/0/0 · test-causality-lifecycle 1/1/1/0 · test-isolation-environment 0/0/0/0 · test-suite-cost 0/0/0/0 · dependency 0/0/0/0 · hygiene-docs 2/1/1/1

## Confirmed findings
- [Should-fix] src/threads/effects.rs:563; src/threads/effects.rs:562; src/threads/effects.rs:561 — Commit 518fa55 rewrote the satisfied test for every constraint so an Ordinary invitation row in state Released or Retired on the clone's current threads_seat counts as satisfied (migration_reinvite is only ever set on Required rows); a seat-wide participation that is left and later re-joined is never re-invited, contrary to design §3, main and the pre-fix head, and no test covers leave-then-rejoin. [lanes: correctness, regression, data-migrations, testing] [fix-regression]
- [Should-fix] src/threads/mapping.rs:161; src/threads/mapping.rs:172 — classify_inspect (f79f9ec) treats every ThreadsError::Rejected from SeatInspect as a per-seat refusal, but Rejected is api_error's catch-all for service-wide codes (InstanceMismatch, Unauthorized, CallerUnverified, StoreBusy, StoreCorrupt, IncompatibleSchema, DaemonVersionMismatch, UnknownWireVersion, etc.), so a sustained whole-call refusal makes every seated clone 'lost' after 90 s, unbound to Unknown with a 'retire and purge' remedy, where the design says a whole-read failure changes no binding. [lanes: premortem, regression] [fix-regression]
- [Nit] tests/daemon_composed.rs:1882 — In `live_handoff_under_a_running_loop_reconnects_and_creates_nothing`, `assert_clones_recover` checks "no re-invite after the handoff" right after a wait that can finish before the post-handoff reconcile step has planned or run its invite effects. Nothing later re-checks the count, so the test passes even if the handoff causes a re-invite. [lanes: test-causality-lifecycle]
- [Nit] CURRENT-DESIGN.md:65 — CURRENT-DESIGN.md (the consolidated contract) and GRAPH_STATE.md, as added in this diff, say a purged clone that held a threads seat stays as an inert Finalize tombstone until threads finalizes it, but a later commit in the same diff (faaad59) changed purge so a seated clone's record is deleted outright and existing Finalize tombstones are dropped; the docs now describe the opposite of what the head does. [lanes: hygiene-docs]

## Not verified (beyond panel cap)
- none

## Not verified (dedupe failed or judge lost)
- none

## Beyond remainder cap (count only)
- none

## Rejected (with reason)
- [rejected] src/model/clone.rs:70 — The fix pass adds two permanent compatibility layers for record shapes that only unmerged intermediate builds of this same branch could have written: the untagged `launched_compat` deserializer with a `LEGACY_LAUNCHED_AT` (UNIX_EPOCH) sentinel that leaks into clone_status.rs, and a full `threads.drop_tombstone` effect kind (const, Payload variant, want, executor method, registration) that drains pre-existing `Finalize` tombstones. — 2-of-3 REJECT

## Unverified nits (spot-checked)
- [Nit/FYI] CURRENT-DESIGN.md:67 — The diff adds an unattended resume driver: after a Herdr restart (90 s grace) or an acknowledged Restore, the daemon starts agents in bound panes on its own and types an env export into the pane shell. It also adds operator status entries `resume_blocked`, `stuck_restores` and `export_rejected`. CURRENT-DESIGN.md still lists "Native session resume versus fresh launch" as remaining to specify, and none of these behaviors or status entries appear in README.md, CURRENT-DESIGN.md or GRAPH_STATE.md.

## Escalations (need human)
- none