# super-auto run — 2026-11-04-threads-seat-identity

flags: planOneShot=f skipPlanRoast=f skipCodeRoast=f autonomous=t
phase: design
codeMechanism: Workflow
runTag: threads-seat-identity-20261105T073527
context: Context: claude · planner=200K/167K · implementer=200K/167K · reviewer=200K/167K · finalReview=200K/167K · assembler=200K/167K · warn: planner, finalReview on a 200K model → set ANTHROPIC_DEFAULT_OPUS_MODEL=<model>[1m] or config.models.<role> to a 1M model · warn: implementer, reviewer, assembler on a 200K model → set ANTHROPIC_DEFAULT_SONNET_MODEL=<model>[1m] or config.models.<role> to a 1M model

idea: Fully autonomous; both roasts (design roast and PR roast) on. Scope: herdr-graph side of the approved design docs/reviews/2026-10-29-seat-review-H2-identity-design.md (read also docs/reviews/2026-10-29-seat-review-H2.md and docs/reviews/2026-10-29-seat-review-H2-identity.md). Includes: H2 Option D (bead hg-s6o: /seat drops accept_command, skill step 5 delegates acceptance to herdr-threads); clone.threads_seat write-once binding; participation keyed to the clone/threads seat (not occupant), kept across agent exit (D3); agent launch/resume sets HERDR_GRAPH_CLONE=<cl_>; delete clone re-matching in favour of SeatInspect; undo/resurrection via threads UnretireSeat + relaunch with key; G4 headless resume driver; tab/workspace matching by preserved Herdr ids. Threads-side work (G1 v2.8.2 release, G2, D1 continuity key, D2 reversible retirement with requirement restore, G5) is owned by the herdr-threads coordinator (seat snSGTpBk7xGd, thread dOqG4VQNjQ4); graph tasks that depend on unreleased threads features must be sequenced/blocked on them, not faked. Constraints: Herdr is not modified; do not touch the user's live graph instance or live Herdr layout; another session may be editing src/cli/seat.rs and skills — check main is current before starting and rebase as needed.
branch: super-auto/threads-seat-identity
base: main
spec: 2026-11-04-threads-seat-identity-design.md
skillSource: /var/home/alepar/.aisw/profiles/claude/claude-1/plugins/cache/superpowers-alepar/superpowers/6.4.2-alepar6.8/skills @ 34fd607b17373d09b2099cf2bec3745b91c10cb7 (6.4.2-alepar6.8)
