# super-auto run — 2026-10-02-herdr-graph-implementation

flags: planOneShot=f skipPlanRoast=f skipCodeRoast=f autonomous=t
resumeChange: 2026-10-02 · "/goal create working MVP for herdr-graph, pushed to new github.com/alepar/herdr-graph repo" · autonomous=t from here (no further questions; remaining root-brainstorm sections decided Mode B); scope narrowed from Q5 "full contract" to a working MVP across all pillars with listed deferrals; goal adds creating a private GitHub repo alepar/herdr-graph and pushing
phase: roast-design

idea: Implement the approved herdr-graph design. Start by reading IMPLEMENTATION-HANDOFF.md, then CURRENT-DESIGN.md and DESIGN-NOTES.md in the documented precedence order. The user explicitly authorized implementation and considers the design converged; historical design-only restrictions are superseded. Preserve approved decisions, resolve routine implementation details, and retain required checkpoints without re-asking settled design questions. Git and local embedded-Dolt Beads have been initialized and the design baseline committed; verify prerequisites. The Herdr skill is installed at .claude/skills/herdr/SKILL.md. Use the handoff for scope, integration evidence and validation expectations. Do not restart the memory observer or disturb existing user sessions.
spec: 2026-10-02-herdr-graph-mvp-design.md
epic: hg-zmi
roast-design: 2026-10-02-herdr-graph-mvp-roast-design-1.md
branch: super-auto/herdr-graph-implementation
base: main

roastDesignRound: 2

parked:
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "Herdr 0.9.1 pane_closed vs tab_closed event order on multi-pane close unverified — designed defensively (grouping window + snapshot containment), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "whether closing a workspace's last tab auto-closes the workspace unverified (v0.9.3 doc says yes) — designed defensively (induced-closure correlation), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "[[startup]] detached daemon lifetime unverified — designed defensively (setsid double-fork + CLI auto-ensure), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "agent_session reporting depends on installed Herdr Claude integration — designed defensively (graph's own SessionStart hook reports session_id/transcript_path), spike in hg-zmi.5"
- 2026-10-02-herdr-graph-mvp-roast-design-1.md · escalation · "terminal_id reuse across Herdr restart unverified — designed defensively (graph token via pane metadata; raw ids never trusted across incarnations), spike in hg-zmi.5"

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
- root-brainstorm sections · human · "OK for all 5 sections" (section 1 presented; sections 2-5 approved in advance, decided in spec)
