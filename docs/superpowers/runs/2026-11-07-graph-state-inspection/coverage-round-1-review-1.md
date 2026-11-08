requirements:
R1 → hg-blw.1, hg-blw.7
R2 → hg-blw.6
R3 → hg-blw.6
R4 → hg-blw.2, hg-blw.9
R5 → hg-blw.2
R6 → hg-blw.4
R7 → hg-blw.5, hg-blw.7
R8 → hg-blw.8
R9 → hg-blw.9
R10 → hg-blw.3, hg-blw.7
R11 → hg-blw.7
R-new: no inspection command (roster, roster --live, instructions stack, sessions, log, ops) creates a Git commit or moves HEAD, and a test checks this → (unmapped)
R-new: an agent or operator can find the new views: README/CLI help/agent-facing instruction text names roster, sessions, log, instructions stack and the ops filters → (unmapped)
R-new: sessions rows name the teamspace, seat and clone each session belongs to → hg-blw.3

Findings (verbatim summary of reviewer 1, round 1):

F1 · GAP · B2/B7 (R5) — harnesses load more than CLAUDE.md/AGENTS.md/AGENTS.override.md in cwd+ancestors: user-level (~/.claude/CLAUDE.md, $CLAUDE_CONFIG_DIR, ~/.codex/AGENTS.md), project variants (.claude/CLAUDE.md, CLAUDE.local.md), rules/memory dirs; whose cwd for another seat / --clone is unstated. Fix: extend hg-blw.2 to a presence-only list per harness resolved against the target clone's cwd and harness config dir, or record the limit as a scope note.
F2 · GAP · R11 — only hg-blw.4 asserts no Git write; roster --live and ops go through the daemon. Fix: integration sweep asserts HEAD/commit count unchanged after every view, or one acceptance line per view bead.
F3 · GAP · R4 — hg-blw.2 must produce changed:{commit,at} but has no history source until hg-blw.5/.9; forces a throwaway walk. Fix: hg-blw.2 consumes HistoryWalker (dep on hg-blw.5), or drop changed from hg-blw.2 and give it to hg-blw.9.
F4 · GAP/UNOWNED-SEAM · Inventory session history (hg-blw.7 → hg-blw.3) — Inventory contract exposes only the current occupant session, not full sessions[] or purge tombstones. Fix: add them to hg-blw.7's Inventory owns/acceptance.
F5 · GAP · B4 — hg-blw.3 acceptance never states rows carry start/end timestamps and teamspace/seat/clone labels in text output. Fix: add an acceptance line.
F6 · UNOWNED-SEAM · CloneLive overlay and roster-level live fields (hg-blw.1 ↔ hg-blw.6) — no task owns CloneLive's shape, the roster-level seat_refusals/seat_lost slot, or the roster.live RPC contract. Fix: hg-blw.6 owns CloneLive, the roster-level live fields and the roster.live RPC.
F7 · GAP · R1 — manager, reload_required and pane availability are absent from hg-blw.1's acceptance shape. Fix: add them.
F8 · GAP · discoverability — no task documents the new commands in README or the agent-facing instruction/skill text. Fix: a docs leaf.
F9 · UNOWNED-SEAM · ops.list RPC filter params (hg-blw.8 ↔ daemon/pending_for) — no owns: line for the extended RPC contract. Fix: hg-blw.8 owns it.
F10 · GAP (weak) · R6/B5 — every pre-upgrade session reads unknown until its next injection. Fix: note it in user-facing text, or accept.

No ORPHAN findings.
