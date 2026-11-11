super-roast verdict: Nit (2 confirmed) [low coverage] [review readiness incomplete]
mode: PR        iteration: 3 of 3
profile (assumed): herdr-graph 0.3.0 is a local, single-operator Herdr plugin: a Rust CLI plus a long-running per-user daemon over a Git-backed instance repository, reached over a local socket. It has no network-exposed surface and no multi-tenant users. The data at stake is the operator's own graph repository, which Git history keeps recoverable, and the code under review is read-only inspection views with a new, unreleased `--json` surface. Blast radius is low, so resilience, observability, cost and contract-consistency findings are rated Nit unless they contradict the change's own stated spec contract with a wrong value.
inputs: super-auto/graph-state-inspection@8654603 vs main@bfeb8f3 (bounded: product paths only)
delta vs prior: 1 new confirmed (0 Blocking) · 0 carried (0 Blocking) · 4 resolved · 1 regressed (0 Blocking) · 2 punch-listed (open)
coverage: scouts 14/14 (correctness, security, premortem, simplicity-design, hot-path-perf, concurrency-async, regression, api-contract, observability, testing, test-causality-lifecycle, test-isolation-environment, test-suite-cost, hygiene-docs) · raw 2 → deduped 2 → panel 2 · spot 0 · promoted 0 · judge completion 100% · remainder-capped: 0 · review readiness: coverage bounded, provenance unavailable — basis: "review input selection: provenance unavailable; captured source available (inv-64d86974 cluster 13 never written; later bounded invocations inherit it); inspected: all 45 product paths outside docs/superpowers/runs/ at bfeb8f3104dd3783d67a46570c9f272e935012b5..86546039ab87a90825bea52c4f560a900283df59, every hunk; unreviewed: 311 run-record paths under docs/superpowers/runs/ (listed in /var/home/alepar/AleCode/herdr-graph/.worktrees/super-auto/graph-state-inspection/.superpowers/sdd/hg-blw-plan/assembled/roast-code-r3-bounded/unreviewed-paths.txt)"
independence: same-family (Claude) — seat-differentiated panel · rung: Workflow
seat-agreement: panels 2 · rr 0.00 · rg 1.00 · fg 0.00 · unanimous 0.00 · ground-loo n/a (n=0) · reproduce 2/0/0 · refute 0/2/0 · ground 2/0/0
lane-yield (found/confirmed/unique/refuted): correctness 0/0/0/0 · security 0/0/0/0 · premortem 1/1/1/0 · simplicity-design 0/0/0/0 · hot-path-perf 0/0/0/0 · concurrency-async 0/0/0/0 · regression 1/1/1/0 · api-contract 0/0/0/0 · observability 0/0/0/0 · testing 0/0/0/0 · test-causality-lifecycle 0/0/0/0 · test-isolation-environment 0/0/0/0 · test-suite-cost 0/0/0/0 · hygiene-docs 0/0/0/0

## Confirmed findings            ← consumed by super-design, one task per finding
- [Nit] src/observe/step.rs:814 — The `roster --live` threads snapshot is written only when a step reaches `observe_seats`. A step that ends earlier (Herdr snapshot failure via `on_disconnect`, a failed `store.head()?`, or a failed `DesiredRuntime::load(...)?` on an unparseable HEAD record) leaves the last snapshot in place with no age marker, so `roster --live` reports the last successful read's `seat_refusals` as current with `live_unavailable.threads: null` for as long as the condition lasts; a daemon whose Herdr was never reachable reports "no threads seat read yet" permanently. (regressed: fixed incompletely, see iteration 2 `src/daemon/roster_live.rs:76; src/daemon/roster_live.rs:79; src/daemon/roster_live.rs:78; src/daemon/roster_live.rs:80`) [lanes: premortem]
  verdict: confirmed (reproduce ✓ / refute REJECT-outvoted / ground ✓)
  evidence: src/observe/step.rs `step_once` (about 808-833) runs `store.head()?`, then `herdr.snapshot()` with `Err(e) => return self.on_disconnect(e, &tuning).await`, then `DesiredRuntime::load(...)?`, and only then `self.observe_seats(...)`.
  evidence: the only writes to `seat_read` are step.rs:651 and :677, both inside `observe_seats`. `on_disconnect` (952-1012) never touches it. A grep at 8654603 by the final gate finds no other writer.
  evidence: src/daemon/roster_live.rs:78-86 maps `last = Some(Ok(()))` to `ThreadsRead::Ok` with the stored refusals. The snapshot has no timestamp and the reader has no age check.
  evidence: the round-2 fix (71a9055) doc comment says the slot "never keeps a stale success". That does not hold on these paths. README.md:98 calls the read "once-per-step".
  evidence: the new composed tests (tests/daemon_composed.rs:2154-2234) cover the successful, failed, never-read and seated-to-empty cases. None covers a step that skips the seat read after an earlier success.
  evidence: the refute seat rejected on materiality and contract. README.md:98 says threads is unavailable "only before the daemon's first step, or when its last read failed", and the last read did not fail. During a Herdr outage the same reply sets `live_unavailable.herdr` and nulls every clone's `live`. With a bad HEAD record the roster build reports it first. Both confirming seats accept each of these points and confirm only the stale-slot mechanism and the overbroad comment, at Nit. The refute seat itself calls the comment "slightly overbroad".
  evidence: this is the stale-state half of the iteration 2 finding on a different trigger. The empty-seated-set path named there is fixed (step.rs:641-656).
  severity: confirming seats rated Nit / Nit. No floor applies: the slot feeds only the read-only overlay and causes no wrong write or reconcile decision.
  fix-shape hint: on the step paths that skip `observe_seats`, set `seat_read` to `Some(Err("no seat read this step: <reason>"))` or clear it, or correct the doc comment and README wording to say the slot holds the last completed read.
- [Nit] src/observe/step.rs:651 — The hg-blw.30 fix makes `roster --live` report the threads half as healthy (`live_unavailable: null`, `seat_refusals: {}`, `seat_lost: []`) whenever no clone is seated, without contacting threads, so a threads service that is down or never located is indistinguishable from a healthy read in that state. Design §2.2 line 206 ("threads seat view not running: `seat_refusals`/`seat_lost` are null and `live_unavailable.threads` names the reason") was left unchanged, while README.md now says "With no seated clone that read is empty and healthy". (new; introduced by the round-2 fix) [lanes: regression] [fix-regression]
  verdict: confirmed (reproduce ✓ / refute REJECT-outvoted / ground ✓)
  evidence: src/observe/step.rs:641-656, in `if seats.is_empty()`, writes `SeatReadSnapshot { last: Some(Ok(())), refusals: BTreeMap::new() }` under the comment "Nothing seated, nothing to read: healthy and empty." and returns before `self.seat_view.inspect(&seats)`.
  evidence: src/daemon/roster_live.rs:78-85 maps `Some(Ok)` to a healthy threads half.
  evidence: design line 206 and line 207 ("`live_unavailable` is `null` when everything was read") are unchanged at 8654603. `git diff 86bc78b 8654603` on README.md removes "without a reachable threads service `--live` is always in that degraded form" and adds the empty-and-healthy sentence. README and design now disagree.
  evidence: the iteration 2 fix-shape hint asked for this exact reset ("reset the snapshot to an Ok read with empty refusals"). The behaviour is deliberate and tested (`roster_live_threads_healthy_on_an_instance_with_no_seated_clone`, tests/daemon_composed.rs).
  evidence: the refute seat rejected because the behaviour is deliberate, the per-seat fields are correctly empty with nothing seated, the Herdr half answers "is an agent running right now", and a threads outage surfaces on the first cycle after a clone is seated. Both confirming seats accept these points and confirm only the unreconciled spec text. The refute seat concedes the README and design wording is "out of step".
  evidence: no seat verified the claim that tests/inspect_live.rs and tests/inspect_sweep.rs had their assertions flipped. Both files changed in this round.
  severity: confirming seats rated Nit / Nit. The live values are accurate for the empty state; the defect is spec drift.
  fix-shape hint: amend design §2.2 lines 206-207 to carve out the zero-seat case as empty and healthy. Probing threads reachability with no seat is the alternative, but it reverses the direction the iteration 2 report asked for.

## Not verified (beyond panel cap)   ← severe candidates the panel cap left unverified — listed, never dropped
- none

## Not verified (dedupe failed or judge lost)   ← findings a pipeline failure left unjudged — listed, never dropped
- none

## Beyond remainder cap (count only)   ← low-severity candidates the dedupe remainder cap dropped; the count survives, the claims do not
- none

## Rejected (with reason)        ← so re-roasts don't re-litigate
- src/inspect/log.rs:131 — `log --since` became a filter, so a history longer than max_scan reports `stopped_early: true`. reason: rejected 2/1 in iteration 2; not re-raised; rejection stands.
- src/bootstrap/setup_claude.rs:211; tests/inspect_loaded.rs:487 — unbounded `p=$(cat)` in the Claude SessionStart hook command. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/cli/ops.rs:141; src/cli/ops.rs:201 — `ops.list` reply and argument changes with no IPC_VERSION bump. reason: rejected 2/1 in iteration 1; not re-raised; rejection stands.
- src/daemon/roster_live.rs:48; src/store/history.rs:128 — `roster.live` history walks contend with the writer on the repo Mutex. reason: rejected 2/1 in iteration 1; not re-raised; rejection stands.
- src/daemon/roster_live.rs:47 — `roster.live` build runs in spawn_blocking with no deadline or concurrency limit. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/inspect/scope.rs:228 — teamspace scope leaves out its seats' transcripts. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/inspect/log.rs:53 — `log --by <seat>` drops commits whose OperationRecord failed to parse. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/daemon/roster_live.rs:64 — fixed 10 s Herdr timeout after an unbounded build can exceed the client's CALL_TIMEOUT. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/plan/ops.rs:479 — `ops.list` calls `store.head()` synchronously on a tokio worker. reason: rejected 2/1 in iteration 1; not re-raised; rejection stands.
- src/cli/roster.rs:73 — `roster --live` against an old daemon exits 1, not 5. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/daemon/doctor.rs:268; src/daemon/doctor.rs:269; src/daemon/doctor.rs:266 — permanent WARN from the `harness_hook codex` doctor check. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- tests/inspect_sweep.rs:597 — every_component_reports_the_one_pinned_head cannot force a commit inside one view invocation. reason: rejected 2/1 in iteration 1; not re-raised; rejection stands.
- tests/inspect_sweep.rs:154 — `Rig::settle` uses a fixed 1.5 s quiet window. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/inspect/loaded.rs:156 — a failed loaded-record write is discarded silently. reason: rejected 3/3 in iteration 1; not re-raised; rejection stands.
- src/inspect/roster.rs:94 — `LastChange` has no `malformed` field for a commit with an unreadable record. reason: rejected 2/1 in iteration 1; not re-raised; rejection stands.

## Unverified nits (spot-checked)
- none

## Escalations (need human)      ← UNVERIFIED externals, incomplete panels, unsettled panels, material dissent
- none

## Prior-report tracking (iteration 2 confirmed findings; outside the recognized sections)
- [Should-fix] src/inspect/roster.rs:587; src/inspect/roster.rs:588 — resolved. 7bfbd25 makes `fill_last_changes` set `last_change_unreadable: <commit>` for every seat still unmatched at an unreadable tree and stop matching it against older commits (roster.rs:563-620). Design line 169 and README.md:96 document the field.
- [Should-fix] src/daemon/roster_live.rs:76; :79; :78; :80 — regressed (fixed incompletely). The empty-seated-set path is fixed (step.rs:641-656, tests `roster_live_threads_healthy_on_an_instance_with_no_seated_clone` and `roster_live_threads_recovers_when_the_seated_set_empties`). The stale-snapshot fault remains on the step paths that skip the seat read (confirmed above at src/observe/step.rs:814, now Nit). The fix also left the design text unreconciled (new Nit above at src/observe/step.rs:651).
- [Should-fix] src/daemon/compose.rs:1143 — resolved. tests/daemon_composed.rs adds `roster_live_reports_a_successful_seat_read_with_its_refusals` and `roster_live_reports_a_failed_seat_read` through the composed daemon.
- [Nit] src/inspect/log.rs:136; src/inspect/log.rs:118 — resolved. README.md:140 and design lines 169 and 353 now name `malformed: ["tree"]`, say the row may be outside the scope, and say it counts toward `--limit`.
- [Nit] src/inspect/sessions.rs:59; src/inspect/stack.rs:376 — punch-listed (open). The caller's scope filter left it unfixed and no scout re-raised it. `fn word` (sessions.rs:59) and the inline conversions (stack.rs:376, :392) are still present at 8654603.
- [FYI] src/daemon/compose.rs:1137 — resolved. 71a9055 moved the snapshot write out of `LazyMap::inspect`; the only writers are step.rs:651 and :677, inside the observe loop's full read under the step lock, so the one-seat finalize check no longer overwrites it.
- [Nit] CURRENT-DESIGN.md:96 (iteration 1) — punch-listed (open). It is in the caller's punch list, the file is unchanged between 86bc78b and 8654603, and no scout re-raised it. It was already outside the iteration 2 Confirmed section; the delta line's punch-listed count of 2 includes it.
- The resolved marks for roster.rs:587, log.rs:136 and compose.rs:1137 rest on a direct read of 8654603 by the final gate (git grep and file reads), not on packet evidence. The mark for compose.rs:1143 rests on the premortem packet's evidence plus the test names in the diff. No test was run for any mark.
- Both confirmed findings this round were decided 2/1 with the refute seat rejecting. In each case the confirming seats' evidence already addresses the refute seat's cited points and all three seats place the residue at documentation or Nit level, so the default route stands and neither is escalated as material dissent.
- Iteration 2's two unverified nits (src/inspect/roster.rs:1157, src/inspect/loaded.rs:308) and iteration 1's unverified nits and remainder-capped candidates are not restated here; see those reports.
- Review readiness is incomplete (coverage bounded, provenance unavailable), so the absence of a finding in this round is not evidence that the 311 unreviewed run-record paths are clean. `[converged]` is not asserted: the engine reports `convergenceEligible: false`.
