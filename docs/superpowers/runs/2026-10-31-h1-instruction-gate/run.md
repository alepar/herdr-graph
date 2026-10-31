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
epic: hg-45e
approvals:
- top-split · auto · hg-45e.1 LEAF, hg-45e.2 LEAF, hg-45e.3 LEAF, hg-45e.4 LEAF, hg-45e.5 LEAF, hg-45e.6 LEAF, hg-45e.7 LEAF, hg-45e.8 LEAF, hg-45e.9 LEAF, hg-45e.10 LEAF, hg-45e.11 LEAF, hg-45e.12 LEAF, hg-45e.13 LEAF, hg-45e.14 LEAF, hg-45e.15 LEAF
