# super-auto run — 2026-11-07-graph-state-inspection

flags: planOneShot=f skipPlanRoast=f skipCodeRoast=f autonomous=t
phase: design
codeMechanism: Workflow
runTag: graph-state-inspection-20261108T225851
context: Context: claude · planner=200K/167K · implementer=200K/167K · reviewer=200K/167K · finalReview=200K/167K · assembler=200K/167K · warn: planner, finalReview on a 200K model → set ANTHROPIC_DEFAULT_OPUS_MODEL=<model>[1m] or config.models.<role> to a 1M model · warn: implementer, reviewer, assembler on a 200K model → set ANTHROPIC_DEFAULT_SONNET_MODEL=<model>[1m] or config.models.<role> to a 1M model

idea: Implement epic hg-blw (graph state inspection) per the approved design 2026-11-07-graph-state-inspection-design.md, fully autonomous, both roasts on. User's words, relayed by the main-tab coordinator: "hg-blw for impl, /superpowers:super-auto fully autonomouth both roasts are on"
branch: super-auto/graph-state-inspection
base: main
