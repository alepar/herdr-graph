# super-auto run — 2026-11-05-seat-failure-manager-seats

flags: planOneShot=f skipPlanRoast=f skipCodeRoast=f autonomous=t
phase: design
codeMechanism: Workflow
runTag: seat-failure-manager-seats-20261105T074235
context: Context: claude · planner=200K/167K · implementer=200K/167K · reviewer=200K/167K · finalReview=200K/167K · assembler=200K/167K · warn: planner, finalReview on a 200K model → set ANTHROPIC_DEFAULT_OPUS_MODEL=<model>[1m] or config.models.<role> to a 1M model · warn: implementer, reviewer, assembler on a 200K model → set ANTHROPIC_DEFAULT_SONNET_MODEL=<model>[1m] or config.models.<role> to a 1M model

idea: Implement what was scoped in this session: Beads hg-qam (H3 /seat failure path: exit 5 daemon unavailable, exit 6 IpcErrorCode::Integrity naming the node manager, silent reference drops made fatal, skill/hook failure step, NoInstance exit 2), hg-au7 (manager seats: seat/teamspace manager pointer, mandatory node_manager_seat root that is assigned to the first seat, assignable/movable via `plan node-manager set`, unset-warning on every command, retire reparenting, never retired while holding role, reopened on tab close overriding containment retirement, who/show manager chain, /seat surfaces manager), and hg-3mc (remove daemon-free fallbacks from who/ops/request/undo; status to confirm). Design record: docs/reviews/2026-10-29-seat-review-H3.md. Fully autonomous; both roasts (design and code) on.
branch: super-auto/seat-failure-manager-seats
base: main
spec: 2026-11-05-seat-failure-manager-seats-design.md
skillSource: /var/home/alepar/.aisw/profiles/claude/claude-1/plugins/cache/superpowers-alepar/superpowers/6.4.2-alepar6.8/skills @ 34fd607b17373d09b2099cf2bec3745b91c10cb7 (6.4.2-alepar6.8)
