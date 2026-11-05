# super-auto run — 2026-11-05-seat-failure-manager-seats

flags: planOneShot=f skipPlanRoast=f skipCodeRoast=f autonomous=t
phase: roast-design
codeMechanism: Workflow
runTag: seat-failure-manager-seats-20261105T074235
context: Context: claude · planner=200K/167K · implementer=200K/167K · reviewer=200K/167K · finalReview=200K/167K · assembler=200K/167K · warn: planner, finalReview on a 200K model → set ANTHROPIC_DEFAULT_OPUS_MODEL=<model>[1m] or config.models.<role> to a 1M model · warn: implementer, reviewer, assembler on a 200K model → set ANTHROPIC_DEFAULT_SONNET_MODEL=<model>[1m] or config.models.<role> to a 1M model

idea: Implement what was scoped in this session: Beads hg-qam (H3 /seat failure path: exit 5 daemon unavailable, exit 6 IpcErrorCode::Integrity naming the node manager, silent reference drops made fatal, skill/hook failure step, NoInstance exit 2), hg-au7 (manager seats: seat/teamspace manager pointer, mandatory node_manager_seat root that is assigned to the first seat, assignable/movable via `plan node-manager set`, unset-warning on every command, retire reparenting, never retired while holding role, reopened on tab close overriding containment retirement, who/show manager chain, /seat surfaces manager), and hg-3mc (remove daemon-free fallbacks from who/ops/request/undo; status to confirm). Design record: docs/reviews/2026-10-29-seat-review-H3.md. Fully autonomous; both roasts (design and code) on.
branch: super-auto/seat-failure-manager-seats
base: main
spec: 2026-11-05-seat-failure-manager-seats-design.md
epic: hg-k4l
roastDesignRound: 1
roast-design: 2026-11-05-seat-failure-manager-seats-roast-design-1.md
findingsLedgerDesign: findings-ledger-design.md
stepBackDesign-round-1: local — three clusters (exit classification, wire compat, node-manager invariant) plus two independent nits; no decision change needed
skillSource: /var/home/alepar/.aisw/profiles/claude/claude-1/plugins/cache/superpowers-alepar/superpowers/6.4.2-alepar6.8/skills @ 34fd607b17373d09b2099cf2bec3745b91c10cb7 (6.4.2-alepar6.8)

approvals:
- top-split · auto · hg-k4l.1 LEAF, hg-k4l.2 LEAF, hg-k4l.3 LEAF, hg-k4l.4 LEAF, hg-k4l.5 LEAF, hg-k4l.6 LEAF, hg-k4l.7 LEAF, hg-k4l.8 LEAF, hg-k4l.9 LEAF, hg-k4l.10 LEAF, hg-k4l.11 LEAF, hg-k4l.12 LEAF, hg-k4l.13 LEAF, hg-k4l.14 LEAF, hg-k4l.15 LEAF, hg-k4l.16 LEAF · promotion review: SPLIT .1/.5 applied; PROMOTE .7/.8/.11 resolved by splitting into leaves (sp:demoted-by-session, spec answers the design)
- coverage-round-1 · canonical R-list: R1..R16 in coverage-round-1-requirements.md, R17..R20 appended from r-new · specsAt baa47d8 · requirements: 16 · mapped: 16 · unmapped: 0 · auto applied C4 C5 C7 C9-C22; auto rejected C6 (sanctioned observed mutation) C8 (S1); flag-sweep C1-C3 kept
- coverage-round-2 · findings 19 → 15 · novel 12/15 (80%) · widening: no · auto applied C23-C28 C30-C32 C35-C38; auto rejected C33; parked C29 C34

parked:
- coverage-ledger.md#C29 · escalation · "B7: observed closure cascades reparent manager pointers outside plans (existing sanctioned observed mutation); human to approve or require a reparent plan"
- coverage-ledger.md#C34 · escalation · "B12: unassigned-node-manager warning is suppressed for init, setup, daemon, seat --hook-prompt and graphs with no live seat; human to confirm the narrowing"
- 2026-11-05-seat-failure-manager-seats-roast-design-1.md · degraded-verdict · "B11 partial: an observed out-of-plan move of the node manager's pane or tab is tolerated and surfaced (warning), not reverted — Herdr exposes no move primitive; plan-driven placement changes are refused"
