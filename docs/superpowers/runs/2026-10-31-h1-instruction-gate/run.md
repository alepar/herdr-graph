# super-auto run — 2026-10-31-h1-instruction-gate

flags: planOneShot=t skipPlanRoast=f skipCodeRoast=f autonomous=t
phase: roast-design
codeMechanism: Workflow
runTag: h1-instruction-gate-20261031T205333
context: Context: claude · planner=200K/167K · implementer=200K/167K · reviewer=200K/167K · finalReview=200K/167K · assembler=200K/167K · warn: planner, finalReview on a 200K model → set ANTHROPIC_DEFAULT_OPUS_MODEL=<model>[1m] or config.models.<role> to a 1M model · warn: implementer, reviewer, assembler on a 200K model → set ANTHROPIC_DEFAULT_SONNET_MODEL=<model>[1m] or config.models.<role> to a 1M model

idea: Implement H1 rev 3 design from /tmp/claude-1001/herdr-graph-seat-review-H1.md (copied to docs/superpowers/runs/2026-10-31-h1-instruction-gate/input-h1-review.md). Fully autonomous; design roast and code roast both on.
branch: super-auto/h1-instruction-gate
base: main
spec: 2026-10-31-h1-instruction-gate-design.md
epic: hg-45e
roastDesignRound: 2
roast-design: 2026-10-31-h1-instruction-gate-roast-design-1.md
findingsLedgerDesign: findings-ledger-design.md
parked:
- 2026-10-31-h1-instruction-gate-roast-design-1.md · escalation · "§3.2: whether a malformed/unsupported Codex .rules file aborts Codex startup is unverified; setup writes the file even when syntax is unconfirmed (check in an isolated CODEX_HOME before writing)"
- 2026-10-31-h1-instruction-gate-roast-design-1.md · escalation · "§4.4: Codex may cap hook output at ~2,500 tokens (additionalContextLimit) per one seat's source; unsettled whether a 9000-byte render exceeds it"

approvals:
- top-split · auto · hg-45e.1 LEAF, hg-45e.2 LEAF, hg-45e.3 LEAF, hg-45e.4 LEAF, hg-45e.5 LEAF, hg-45e.6 LEAF, hg-45e.7 LEAF, hg-45e.8 LEAF, hg-45e.9 LEAF, hg-45e.10 LEAF, hg-45e.11 LEAF, hg-45e.12 LEAF, hg-45e.13 LEAF, hg-45e.14 LEAF, hg-45e.15 LEAF
- coverage-round-1 · R1..R20 canonical (coverage-round-1-requirements.md) · requirements: 15 · mapped: 14 · unmapped: 1 (R15) · cl-3 applied auto, cl-4 applied auto, cl-5 rejected auto, cl-6 rejected auto, cl-7 applied auto, cl-8 rejected auto, cl-9 rejected auto, cl-10 applied auto, cl-11 applied auto, cl-12 applied auto, cl-13 applied auto, cl-14 rejected auto, cl-1/cl-2 flag-sweep kept auto
- coverage-round-2 · requirements: 20 · mapped: 19 · unmapped: 3 (R3, R15, R16) · divergence findings 12 → 17, novel 76%, widening: yes (cap kept at 2; design roast covers) · cl-15 applied auto, cl-16 applied auto, cl-17 rejected auto, cl-18 rejected auto, cl-19 applied auto, cl-20 rejected auto, cl-21 rejected auto, cl-22 rejected auto, cl-23 applied auto, cl-24 rejected auto, cl-25 applied auto, cl-26 applied auto, cl-27 applied auto, cl-28 rejected auto, cl-29 applied auto, cl-30 applied auto, cl-31 applied auto

stepBackDesign-round-1: local — five round-1 findings form two small clusters plus one standalone gate-wording gap, each cheap to fix locally; no owning decision worth redesigning (no mechanical triggers)
designRoast-round-1-fixes: C1 admissibility predicate (spec §2.2a; hg-45e.3/.4/.6/.14/.15), C2 seat-layer preservation (spec §1.5, §4.4; hg-45e.4/.6/.7/.11, new hg-45e.17), C3 tty prompt refusal under agent markers (spec Key decisions, §3.3, §3.5; hg-45e.10/.11) — applied inline, auto; re-roast: design decisions changed
