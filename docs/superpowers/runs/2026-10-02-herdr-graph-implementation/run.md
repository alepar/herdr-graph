# super-auto run — 2026-10-02-herdr-graph-implementation

flags: planOneShot=f skipPlanRoast=f skipCodeRoast=f autonomous=t
resumeChange: 2026-10-02 · "/goal create working MVP for herdr-graph, pushed to new github.com/alepar/herdr-graph repo" · autonomous=t from here (no further questions; remaining root-brainstorm sections decided Mode B); scope narrowed from Q5 "full contract" to a working MVP across all pillars with listed deferrals; goal adds creating a private GitHub repo alepar/herdr-graph and pushing
phase: fix-loop

idea: Implement the approved herdr-graph design. Start by reading IMPLEMENTATION-HANDOFF.md, then CURRENT-DESIGN.md and DESIGN-NOTES.md in the documented precedence order. The user explicitly authorized implementation and considers the design converged; historical design-only restrictions are superseded. Preserve approved decisions, resolve routine implementation details, and retain required checkpoints without re-asking settled design questions. Git and local embedded-Dolt Beads have been initialized and the design baseline committed; verify prerequisites. The Herdr skill is installed at .claude/skills/herdr/SKILL.md. Use the handoff for scope, integration evidence and validation expectations. Do not restart the memory observer or disturb existing user sessions.
spec: 2026-10-02-herdr-graph-mvp-design.md
epic: hg-zmi
roast-design: 2026-10-02-herdr-graph-mvp-roast-design-1.md, 2026-10-02-herdr-graph-mvp-roast-design-2.md
roast-code: 2026-10-02-herdr-graph-mvp-roast-pr-1.md, 2026-10-02-herdr-graph-mvp-roast-pr-2.md
branch: super-auto/herdr-graph-implementation
base: main

roastDesignRound: 2
roastCodeRound: 2

parked:
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "Herdr 0.9.1 pane_closed vs tab_closed event order on multi-pane close unverified — designed defensively (grouping window + snapshot containment), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "whether closing a workspace's last tab auto-closes the workspace unverified (v0.9.3 doc says yes) — designed defensively (induced-closure correlation), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "[[startup]] detached daemon lifetime unverified — designed defensively (setsid double-fork + CLI auto-ensure), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "agent_session reporting depends on installed Herdr Claude integration — designed defensively (graph's own SessionStart hook reports session_id/transcript_path), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "terminal_id reuse across Herdr restart unverified — designed defensively (graph token via pane metadata; raw ids never trusted across incarnations), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-2.md · escalation · "Herdr resume_agents_on_restore resumes agents in restored panes outside graph's start path — designed defensively (adopt as occupancy start, 90s relaunch grace, idle-shell recheck), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-2.md · escalation · "Herdr live handoff (herdr update) vs incarnation rule — designed defensively (terminal_id match across incarnations when cwd/agent session agree), spike in hg-zmi.5"
- graph-pass · graph-change · "hg-zmi.19 <- hg-zmi.16 drop (safe no) — move tier-4 matrix row-fill to .16 or sweep; depth 11→10"
- graph-pass · graph-change · "proposal: seam-contract for observer/reconciler boundary (hg-zmi.7/.8/.11/.12/.13) — depth could reach 9"
graph-pass: depth 11→11 · width 1.8→1.8 · applied 0 · parked 2
codeRoastExit: converged at round 2 — Should-fix (15 confirmed) [converged], 0 Blocking; 2 [fix-regression] → regression-only pass; remaining 13 confirmed → report punch list
designRoastExit: converged at round 2 — Should-fix (14 confirmed) [converged], 0 Blocking; punch list of 14 applied inline to spec + beads (no re-roast per loop rule); 2 new escalations parked above

codeBuckets:
  completed: hg-zmi.1–.20, hg-zmi.46–.70 (pass 3: .53–.55; pass 4: .56–.59; fix-loop r1: .60–.70)
  escalated:
  pendingRetry:
  parked:
  stalled: false
  review: pass 1 not ready (9 must-fix → hg-zmi.46–.52 + .20 re-entry, all completed in pass 2); pass 2 not ready (F1, F2 must-fix; F3 should-fix → hg-zmi.53–.55, epic reopened, super-code pass 3); pass 3 not ready (A 2s polling, red doctor test, C exclusion undo, B rename Notify → hg-zmi.56–.59, pass 4); pass 4 not ready (S1 hook timeout, S2 durable session-end → hg-zmi.60–.61, open, carried into the phase-5 fix loop; S3–S8 deferred by reviewer); fix-loop r1 pass review not ready (F1 duplicate ids on re-apply, F2 withdrawal of reused seat → hg-zmi.71–.72, carried into round 2)
  sweep: SWEEP DEFERRED (caller-owned)
  slowness: []
  stopReason: ready-drained (pass 1); root-closed (passes 2, 3, 4, fix-loop r1)
  authRefused: hg-zmi.20 pass 1 — `rm /Users/alepar/.config/herdr-graph/config.toml` (user to delete it and /private/tmp/hgx/i); re-entry completed in pass 2

approvals:
- root-brainstorm Q1 · human · summarizer ACK queue → amend herdr-threads (service SendMessage w/ required receipts + service reads); request sent to w4:p1, threads-amendment-request.md
- root-brainstorm Q2 · human · confirmation = TTY [y/n] in human terminals; agents relay with --confirm <plan-hash> after explicit user yes
- root-brainstorm Q3 · human · baseline (a)-(d) auto scope accepted; summarizer = single seat/single pane, main agent dispatches subagents per request, /loop 1h leftover scan; relaunch of an absent occupant of the active summarizer seat is covered by the confirmed plan that made it active (no grant/flag)
- root-brainstorm Q4 · human · Rust Herdr plugin, depends on herdr-threads lib (local, no remote → committed symlink third_party/herdr-threads)
- root-brainstorm Q5 · human · full contract tiered tests → narrowed by /goal to working MVP (resumeChange)
- top-split · auto · hg-zmi.1..19 all LEAF (promotion review: hg-zmi.6 SPLIT applied → remainder hg-zmi.17; ISSUES fixed: +hg-zmi.18 daemon wiring, +hg-zmi.19 tier-3 e2e, edges .6←.4 .7←.5 .4←.2 .16←.18, dropped .13←.9; GitHub push kept outside tree)
- coverage-round-1 · canonical R-list: R1–R23 in coverage-round-1-requirements.md (+R24–R28 from reviewer R-new for round 2) · requirements: 23 · mapped: 23 · unmapped: 0 · auto: 24 findings applied (13 GAP, 11 UNOWNED-SEAM; seams adopted into existing seam contract hg-zmi.1), 0 rejected, 0 escalated — ledger c1–c24; edges added .13←.9,.11,.17; .18←.9,.10,.13,.17; .19←.13,.15,.16,.17
- coverage-round-2 · R-list read back from round 1 (R1–R28) · divergence: findings 22 → 15 · novel 13/15 (87%) · widening: no · auto: 16 findings — 12 applied, 2 partially applied, 2 rejected (covered by full descriptions) — ledger c25–c40; edges .19←.14, .11←.17, .11←.5; root integration sweep hg-zmi.20 created (deps on all 19 leaves). Coverage loop ended (fixed two rounds).
- stepBack-round-1: redesign — applied: edge-triggered event→retirement mapping with 30s correlation → level-triggered, intent-classified snapshot differ with persisted baseline + incarnation + graph tokens (dissolves 9 + 3 escalations); clusters C2–C6 + occupancy cleanup patched inline (design-fix tasks hg-zmi.21–.27, closed)
- stepBack-round-1: patch — 28 findings independent of the redesigned observer (design-1 redesign held); 9 rule clusters (C1 effect write-ahead, C2 durable attention signals, C3 timeout budget, C4 writer error classification, C5 crash-atomic writes, C6 silent error drops, C7 typed request context, C8 content target resolution, C9 atomic check-then-act) + 7 independent nits
- scopeFilter-round-1: [Nit] src/bootstrap/content.rs:25 punch-list — Content write path-validation gap in bootstrap content not goal-named; cluster C8 has no in-scope member.
- scopeFilter-round-1: [Nit] src/bootstrap/content.rs:55 punch-list — Archive-slug false positive in content write fails safe and is not goal-named; cluster C8 has no in-scope member.
- scopeFilter-round-1: [Nit] src/daemon/ensure.rs:67; src/daemon/server.rs:147 punch-list — Daemon version handshake across plugin rebuilds not goal-named; operational hardening that fails loudly.
- scopeFilter-round-1: [Nit] src/herdr/client.rs:168 punch-list — Missing event-reader log lines — observability improvement; behavior stays correct via re-snapshot; cluster C6 has no in-scope member.
- scopeFilter-round-1: [Nit] src/reconcile/mod.rs:123 punch-list — N+1 queries / no pruning — scaling improvement; no scale requirement in goal.
- scopeFilter-round-1: [Nit] src/reconcile/mod.rs:394 punch-list — Discarded journal-write errors — error-handling consistency/observability gap; cluster C6 has no in-scope member.
- scopeFilter-round-1: [Nit] src/reconcile/mod.rs:478 in-scope — Deferred effects re-run within a pass and over-count defer_n — wrong backoff in goal-named reconcile path; cluster C1.
- scopeFilter-round-1: [Nit] src/transcripts/mod.rs:147 in-scope — Commit wait as long as CLI timeout gives a transport error for an admitted op — misleading in goal-named transcripts path; cluster C3.
- scopeFilter-round-1: [Nit] src/transcripts/mod.rs:252 in-scope — Race can create duplicate transcript records for one file — incorrect behavior in goal-named transcript tracking.
- scopeFilter-round-1: [Nit] src/undo/compensate.rs:349 in-scope — Undo is goal-named and its apply-time race guard has no test — missing test for a goal-named behavior.
- scopeFilter-round-1: [Nit] src/writer/mod.rs:263 punch-list — Repo mutex held across bounded sleep — performance/structure improvement, not correctness.
- scopeFilter-round-1: [Nit] src/writer/mod.rs:291 in-scope — finish_committed failure after CAS leaves the op stuck applying / crash between finish_committed and supersede leaves supersession incomplete — goal-named crash recovery; cluster C4.
- scopeFilter-round-1: [Nit] src/writer/worktree.rs:117 in-scope — Non-atomic in-place worktree writes leave a torn file after crash, never repaired — breaks goal-named crash recovery; cluster C5.
- scopeFilter-round-1: [Nit] src/writer/worktree.rs:127 in-scope — Recovery overwrites previously preserved orphan files on key collision — incorrect behavior in goal-named crash recovery; cluster C5.
- scopeFilter-round-1: [Nit] tests/daemon_composed.rs:902 in-scope — Crash test never crashes in the Herdr-call-to-record window — missing goal-named crash-injection coverage of the lost-response path; cluster C1.
- scopeFilter-round-1: [Nit] tests/writer_concurrency.rs:198 punch-list — Stress-test strength improvement; interleavings covered deterministically elsewhere.
- scopeFilter-round-1: [Nit] tests/writer_crash.rs:84 in-scope — Crash-injection tests never exercise recovery with committed history or mid-queue crash — missing test for goal-named behavior.
- scopeFilter-round-1: [Nit] third_party/herdr-threads:1; Cargo.toml:26 punch-list — README/install-ordering gap around an accepted symlink decision; goal does not name fresh-clone builds; not a correctness defect.
- scopeFilter-round-1: [Should-fix] src/daemon/compose.rs:216 in-scope — First-pass timeout drops an in-flight Herdr effect leaving it Pending — incorrect behavior in goal-named Herdr driving; cluster C1.
- scopeFilter-round-1: [Should-fix] src/plan/commands.rs:33; src/cli/plan.rs:132 in-scope — plan.apply wait equals the client timeout, giving a transport error with no op id — misleading result in the goal-named plan/confirm/apply path.
- scopeFilter-round-1: [Should-fix] src/reconcile/herdr_exec.rs:333 in-scope — Mid-effect crash resumes non-idempotent effects as Pending, skipping lost-response recovery — incorrect behavior in goal-named Herdr driving; cluster C1.
- scopeFilter-round-1: [Should-fix] src/reconcile/mod.rs:114; src/reconcile/mod.rs:117 in-scope — Needs-revision/blocked notifications silently lost when threads is down — incorrect behavior in goal-named threads/reconcile integration; cluster C2.
- scopeFilter-round-1: [Should-fix] src/reconcile/mod.rs:463 in-scope — A Failed effect produces no signal — the goal-named reconciler hides an abandoned change; cluster C2.
- scopeFilter-round-1: [Should-fix] src/transcripts/mod.rs:477 in-scope — Completions misattributed to the source seat instead of the reporting summarizer — incorrect behavior in the goal-named transcript delivery path.
- scopeFilter-round-1: [Should-fix] src/writer/commit.rs:41 in-scope — Gitdir writes not fsynced, so recovery can see a commit_oid whose objects never reached disk — breaks goal-named crash-recoverable persistence.
- scopeFilter-round-1: [Should-fix] src/writer/mod.rs:242 in-scope — Deterministic corrupt-record error halts the whole writer instead of failing one op — incorrect behavior in the goal-named writer.
- scopeFilter-round-1: [Should-fix] src/writer/mod.rs:514 in-scope — Writer infra errors never halt — goal-named serialized crash-recoverable writer is incorrect under persistent journal failure.
- scope-filter: 18 in-scope · 9 punch-listed
- regressionPass-round-2: 2 filed · [Should-fix] src/transcripts/capture.rs:211; src/transcripts/capture.rs:131; src/transcripts/capture.rs:143, [Should-fix] src/transcripts/liveness.rs:118 · no re-roast (beads hg-zmi.73–.74; capture.rs:125 swept by the same idempotency rule; open review beads hg-zmi.71–.72 drain in the same super-code pass)
- root-brainstorm sections · human · "OK for all 5 sections" (section 1 presented; sections 2-5 approved in advance, decided in spec)
