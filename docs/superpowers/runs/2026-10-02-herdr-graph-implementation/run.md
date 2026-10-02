# super-auto run — 2026-10-02-herdr-graph-implementation

flags: planOneShot=f skipPlanRoast=f skipCodeRoast=f autonomous=t
resumeChange: 2026-10-02 · "/goal create working MVP for herdr-graph, pushed to new github.com/alepar/herdr-graph repo" · autonomous=t from here (no further questions; remaining root-brainstorm sections decided Mode B); scope narrowed from Q5 "full contract" to a working MVP across all pillars with listed deferrals; goal adds creating a private GitHub repo alepar/herdr-graph and pushing
phase: design

idea: Implement the approved herdr-graph design. Start by reading IMPLEMENTATION-HANDOFF.md, then CURRENT-DESIGN.md and DESIGN-NOTES.md in the documented precedence order. The user explicitly authorized implementation and considers the design converged; historical design-only restrictions are superseded. Preserve approved decisions, resolve routine implementation details, and retain required checkpoints without re-asking settled design questions. Git and local embedded-Dolt Beads have been initialized and the design baseline committed; verify prerequisites. The Herdr skill is installed at .claude/skills/herdr/SKILL.md. Use the handoff for scope, integration evidence and validation expectations. Do not restart the memory observer or disturb existing user sessions.
spec: 2026-10-02-herdr-graph-mvp-design.md
epic: hg-zmi
branch: super-auto/herdr-graph-implementation
base: main

approvals:
- root-brainstorm Q1 · human · summarizer ACK queue → amend herdr-threads (service SendMessage w/ required receipts + service reads); request sent to w4:p1, threads-amendment-request.md
- root-brainstorm Q2 · human · confirmation = TTY [y/n] in human terminals; agents relay with --confirm <plan-hash> after explicit user yes
- root-brainstorm Q3 · human · baseline (a)-(d) auto scope accepted; summarizer = single seat/single pane, main agent dispatches subagents per request, /loop 1h leftover scan; relaunch of an absent occupant of the active summarizer seat is covered by the confirmed plan that made it active (no grant/flag)
- root-brainstorm Q4 · human · Rust Herdr plugin, depends on herdr-threads lib (local, no remote → committed symlink third_party/herdr-threads)
- root-brainstorm Q5 · human · full contract tiered tests → narrowed by /goal to working MVP (resumeChange)
- root-brainstorm sections · human · "OK for all 5 sections" (section 1 presented; sections 2-5 approved in advance, decided in spec)
