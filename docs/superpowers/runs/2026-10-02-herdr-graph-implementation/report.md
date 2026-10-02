status: completed with 0 unresolved Blocking, 7 escalations [degraded: final review: not ready, sweep: FAIL @ 1237eb1 (2563 passed, 1 failed)]
metrics: pending (upstream-feedback not yet run)

# herdr-graph MVP — super-auto run report (2026-10-02)

Branch `super-auto/herdr-graph-implementation` @ `1237eb1` vs `main` @ `5a04fc7`: 129 commits; product code/tests/skills/templates: 175 files, +50,777 lines. Epic `hg-zmi` is closed. Sources: `run.md` (this directory), the super-code ledger `.superpowers/sdd/hg-zmi-plan/progress.md` in the integration worktree (git-ignored; disappears with the worktree), the bead tree `bd list --label sp:hg-zmi --all`, the roast reports in this directory.

**How to read the status line.** 0 Blocking: neither design roast (2 rounds, converged) nor code roast (2 rounds, converged) confirmed a Blocking finding. The 7 escalations are all **design-roast** external-fact questions about Herdr 0.9.1 behaviour that no judge could verify (listed under Remaining); each was designed around defensively, none was adjudicated. `codeBuckets.escalated` is empty (the one quarantined task, hg-zmi.20, was re-entered and landed). The final whole-epic review (super-code, after the sweep-fix pass) still says **not ready** and the full-suite sweep re-run has **1 failing test** — see Remaining.

## Implemented

Source: closed beads under `hg-zmi` and the ledger's `Task N (hg-zmi.X): complete (commits …)` lines; `run.md` `codeBuckets.completed` = hg-zmi.1–.20, .46–.77 (hg-zmi.21–.27 are closed design-fix bookkeeping beads; `review: …` beads are super-code bookkeeping). Every task merged with one light review; only hg-zmi.7 needed its fix pass. Merge checks: 0 merge failures, 0 rebase conflicts (ledger `Metrics:` lines, passes 1–2; later passes' metrics were `UNAVAILABLE`, see Gotchas).

Core build (super-code pass 1–2):
- hg-zmi.1 crate scaffold, model schemas, port traits, IPC envelope (1e7d878..cb7a917)
- hg-zmi.2 Git-backed store: committed-revision reads, layout, slugs, init (7182084..54e8d1f)
- hg-zmi.3 SQLite journal, serialized writer, preconditions, crash recovery (e56a846..c44e362)
- hg-zmi.4 daemon, UDS IPC, CLI framework, status/doctor (6b3f67b..23ce61d)
- hg-zmi.5 Herdr NDJSON client, PrivateHerdr fixture, FakeHerdr, isolation guard (76e10ed..60ecb43)
- hg-zmi.6 plan/confirm/apply core + lifecycle kinds (7ac0881..78feff7); hg-zmi.17 remaining kinds, ops/cancel/reassign/check-instruction, reminders (83cff32..20eff90)
- hg-zmi.7 reconciler, effect fencing, retries, session replacement (07f7be7..babcae0, fix pass)
- hg-zmi.8 observer as level-triggered snapshot differ with cascades (d800a3b..b1040c6)
- hg-zmi.9 templates + applications, live propagation, withdrawal, re-add (29616da..67f47f2)
- hg-zmi.10 undo CLI with caller-pane adoption (c784907..6865d24)
- hg-zmi.11 herdr-threads integration: system channels, required invites, participation, notify, cleanup (b900f52..f7bad87)
- hg-zmi.12 transcripts and processing requests, coverage, delivery adapters (b9eecbd..0cfd772)
- hg-zmi.13 `/seat`, setup hook, skills, shipped templates incl. system-summarizer (a0695ea..deaa2ed)
- hg-zmi.14 plugin manifest, build script, README (60ecb43..75ce9d7)
- hg-zmi.15 configuration smoke (1ba9dd1..561ceb0); hg-zmi.16 real-agent smoke harness (ddf5502..9f0f7ba)
- hg-zmi.18 daemon composition root (27720d9..5e3dcb0); hg-zmi.19 tier-3 e2e flows + verification matrix (b0d4efc..a8b773c); hg-zmi.20 integration sweep (432a4d5..ac60ce4)

Final-review fixes (passes 2–4): hg-zmi.46 threads discovery in default installs; .47 doctor/halted writer; .48 non-blocking session replacement; .49–.51 D3/D2/D1 (tab rename, rebind re-stamp, undo adoption race); .52 worktree_dirty Notify; .53 replacement survives observed exit; .54 undo of a bound caller pane; .55 CLI during daemon startup; .56 no 2 s polling for open-ended defers; .57 doctor test (missing threads = WARN); .58 undo removes added exclusion; .59 rename Notify (§7.2); .60 SessionStart hook within 10 s; .61 durable session-end → request.

Code-roast fixes (fix loop round 1): hg-zmi.62 effect write-ahead (C1); .63 durable attention signals (C2); .64 single timeout budget (C3); .65 writer error classification (C4); .66 crash-atomic writes (C5); .67 typed caller context (C7); .68 atomic check-then-act (C9); .69 undo race-guard test; .70 writer crash tests. Review/regression pass: .71 no duplicate ids on re-apply; .72 withdrawal keeps reused seats; .73 spool idempotent occupancy; .74 recovery scan skips live resumed sessions. Sweep-fix pass: .75 composed-daemon tests wait for ready; .76 Herdr-restart rebind; .77 test isolation (temp HOME/state, guaranteed child reaping — user request).

Also delivered outside the code: the herdr-threads amendment request (service-authored ACK-required messages + service reads), accepted by herdr-threads as epic `ht-5nb` (`threads-amendment-request.md`, `threads-amendment-reply.md`); graph ships it behind cargo feature `threads-service-ack` (default off) and uses a Notify fallback until it lands.

## Remaining

**Failing test at the tip** (`run.md` `codeBuckets.sweep`, `sweepFix`): the single phase-6 sweep fix pass is spent, so this is reported as it stands.
- `e2e_rename_tab_renames_seat_moves_path_history` (tier `--features private-herdr`, tests/e2e_private_herdr.rs:480): after an observed tab rename the old seat folder `teamspaces/t/seats/foreman` still exists. Sweep @ 1237eb1: default 634/0, test-support 648/0, private-herdr 646/1, threads-service-ack 635/0. It passed in the previous sweep @ 75b528b, so a later change (likely hg-zmi.66 crash-atomic working-tree writes or hg-zmi.77) regressed the working-tree move.

**Final whole-epic review "not ready" items not fixed** (super-code review after the sweep-fix pass; recorded in `run.md` `codeBuckets.review`):
- `undo_from_pane_bound_to_other_clone_keeps_that_pane` failed 6/6 under parallel `cargo test --features test-support --test daemon_composed` in the reviewer's runs, alongside `git: object not found` daemon log lines (passes alone and with `--test-threads=1`; passed in the sweep re-run). Cause unproven: harness temp-dir teardown vs. a real re-stamp race.
- Writer can strand an op in `applying` when `finish_failed` itself errors (src/writer/mod.rs:210, :221) — also round-2 roast `[Should-fix] src/writer/tests.rs:777`.
- `.graph-local/worktree_dirty` is append-only: one local edit makes `doctor` FAIL and `/seat` list the file forever.
- Attention notices for seatless requesters (human TTY plans, summarizer relaunch) or before a channel exists are marked `logged` and dropped; **needs your decision** on routing (spec §4.1 says the affected seat's channel, which cannot reach a blocked agent itself).
- `docs/verification-matrix.md` and the README summary describe code @ ab1c681 (pre-.53) — stale.

**Code-roast punch list** (never filed as beads):
- Round 1 scope filter `punch-list` (`run.md` `scopeFilter-round-1`), each **out of scope (filtered)**: `[Nit] src/bootstrap/content.rs:25` and `:55` (content-write path validation), `[Nit] src/daemon/ensure.rs:67; src/daemon/server.rs:147` (no build-version handshake after plugin rebuild), `[Nit] src/herdr/client.rs:168` and `[Nit] src/reconcile/mod.rs:394` (silent error drops), `[Nit] src/reconcile/mod.rs:123` (N+1 effect queries), `[Nit] src/writer/mod.rs:263` (repo mutex held across sleep), `[Nit] tests/writer_concurrency.rs:198` (weak linearizability test), `[Nit] third_party/herdr-threads:1; Cargo.toml:26` (see below).
- Round 2 converged; its 13 non-regression confirmations are the punch list (`2026-10-02-herdr-graph-mvp-roast-pr-2.md`): `[Should-fix] src/transcripts/capture.rs:125` (stale live report — swept by hg-zmi.73's idempotency rule, not separately re-verified), `[Should-fix] src/writer/tests.rs:777` (above), plus Nits/FYI on `note_identity` stale decision, spool `skip_known` breadth, spool backlog not surfaced in doctor, `spool_report` not using `fsutil::write_atomic`, skills not documenting `apply` exit code 3, `fold_legacy_meta` on every construction, and the round-1 repeats.

**Design escalations parked** (`run.md` `parked:`; designed defensively, never verified): Herdr 0.9.1 event order on multi-pane close; whether closing a workspace's last tab closes the workspace; detached daemon lifetime from a `[[startup]]` hook; `agent_session` without Herdr's Claude integration; terminal_id reuse across restart; Herdr `resume_agents_on_restore` resuming agents outside graph; live handoff vs. the incarnation rule.

**Parked graph changes** (`run.md` `graph-pass: depth 11→11 · width 1.8→1.8 · applied 0 · parked 2`): drop `hg-zmi.19 <- hg-zmi.16`; seam-contract the observer/reconciler boundary.

**Untested scope** (final review): tier 4 never ran — real Claude/Codex launch and resume, session replacement of a live agent, the real SessionStart hook, the summarizer ACK/complete flow, trust-dialog detection, spike 5. Service-ACK delivery waits on herdr-threads `ht-5nb`. Herdr live handoff, a real lost create response, truly dropped events, and the opt-in real herdr-threads test (`HG_REAL_THREADS=1`) were not exercised at the tip.

**Spec deferrals** (spec §Scope): cross-tab clone transfer, application template switch, assisted repair-plan generation, multi-pane restoration placement, reminder cadence tuning, Codex transcript discovery without `agent_session`.

**Portability:** `third_party/herdr-threads` is a committed symlink to `/Users/alepar/AleCode/herdr-threads` (herdr-threads has no remote). A fresh clone on another machine must re-point it (README).

## Gotchas & surprises

- **herdr-threads could not do what the approved design assumed.** The registered service could only Notify (no ACK obligations, no reads). Per your decision the gap went to herdr-threads as an amendment (accepted as `ht-5nb`, queued behind its `ht-p03`); graph ships a Notify-based fallback meanwhile.
- **Design roast redesign** (`stepBack-round-1` design): the first observer design mapped Herdr events straight to retirements with a 30 s correlation window; 9 of 23 confirmed findings plus 3 escalations came from that one decision. It was replaced by a level-triggered, intent-classified snapshot differ with a persisted baseline, incarnations and graph tokens (spec §4.3). The code roast later found no defect in that differ (`2026-10-02-herdr-graph-mvp-roast-pr-1-step-back.md`).
- **Per-task review is blind to composition.** Each super-code pass's whole-epic review found new cross-task seam defects (session replacement × observer; Deferred wake × threads/transcripts; undo adoption × live index; hook × daemon startup), costing four extra code passes before the code roast (`run.md` `codeBuckets.review`).
- **A probe wrote into your real HOME.** hg-zmi.20 ran `herdr-graph init` unisolated, leaving `~/.config/herdr-graph/config.toml` → `/private/tmp/hgx/i`; its cleanup was refused by the harness. You approved the deletion and it is gone; hg-zmi.77 enforces temp HOME/state and child reaping for every test. One orphaned test daemon from hg-zmi.76 (private temp instance) was found and stopped.
- **Provider safeguards** rejected the coordinator's ledger-read dispatch (sonnet, then opus) in every pass from pass 3 on, so later passes' `Metrics:` lines read `UNAVAILABLE`; the work itself was unaffected (`friction.md`).
- **Workflow tooling:** the coordinator script had to be copied out of the plugin cache to launch; the worktree-isolated session refuses compound shell commands (`friction.md`). The merge check compiled only default features until the sweep-fix pass, so feature-gated tests were never compiled per merge before that.

## Entrypoints

1. `src/model/` — record schemas, ids, lifecycle (the seam contract everything else codes against).
2. `src/store/` then `src/writer/` and `src/journal/` — committed reads, the serialized writer, crash recovery.
3. `src/plan/` — plan/confirm/apply and the organizational change kinds.
4. `src/observe/` and `src/reconcile/` — the snapshot differ and the reconciler sharing one loop step (spec §4.3–4.4).
5. `src/daemon/compose.rs` — the composition root wiring every component, loop and command.
6. `src/cli/` and `skills/seat`, `skills/graph` — the user/agent surface; `templates/system-summarizer` for the summarizer role.
7. `tests/e2e_private_herdr.rs`, `tests/daemon_composed.rs` — the integration evidence.

## Smells

- **The failing rename e2e and the parallel-flaky undo test** sit on the working-tree/move and adoption paths that were patched repeatedly (D1, F2, hg-zmi.66) — the area most likely to still hide a race.
- **Live-identity resolution in the reconciler's `LiveIndex`** accumulated precedence rules across D1/D2/D3/F2 fixes with inconsistent incarnation filters (pass-3 review finding D); a stale binding could claim a fresh pane after a Herdr restart.
- **hg-zmi.7 merged after its one fix pass without re-review** (ledger).
- **Regression-pass fixes hg-zmi.73–.74 merged with no re-roast** (`run.md` `regressionPass-round-2`), and hg-zmi.73 was relied on to also close `capture.rs:125` by its idempotency rule without separate verification.
- **Sweep-fix pass hg-zmi.75–.77 merged; its re-run is FAIL (1 test)** — the fixes themselves were verified only by that one re-run.
- **Reviewer evidence was often second-hand:** the ledger records many "results taken from report" / "reviewer could not cd into the worktree" lines and tests written without a red run, so several regression tests may not catch their regression.
- `cargo fmt --check` reports thousands of hunks; the code has never been formatted.
