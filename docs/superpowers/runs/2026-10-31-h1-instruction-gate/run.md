# super-auto run — 2026-10-31-h1-instruction-gate

flags: planOneShot=t skipPlanRoast=f skipCodeRoast=f autonomous=t
phase: design
codeMechanism: Workflow
runTag: h1-instruction-gate-20261031T205333
context: Context: claude · planner=200K/167K · implementer=200K/167K · reviewer=200K/167K · finalReview=200K/167K · assembler=200K/167K · warn: planner, finalReview on a 200K model → set ANTHROPIC_DEFAULT_OPUS_MODEL=<model>[1m] or config.models.<role> to a 1M model · warn: implementer, reviewer, assembler on a 200K model → set ANTHROPIC_DEFAULT_SONNET_MODEL=<model>[1m] or config.models.<role> to a 1M model

idea: Implement H1 rev 3 design from /tmp/claude-1001/herdr-graph-seat-review-H1.md (copied to docs/superpowers/runs/2026-10-31-h1-instruction-gate/input-h1-review.md). Fully autonomous; design roast and code roast both on.
branch: super-auto/h1-instruction-gate
base: main
spec: 2026-10-31-h1-instruction-gate-design.md
