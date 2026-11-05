# super-auto run — 2026-11-04-threads-seat-identity

flags: planOneShot=f skipPlanRoast=f skipCodeRoast=f autonomous=t
phase: roast-design
codeMechanism: Workflow
runTag: threads-seat-identity-20261105T073527
context: Context: claude · planner=200K/167K · implementer=200K/167K · reviewer=200K/167K · finalReview=200K/167K · assembler=200K/167K · warn: planner, finalReview on a 200K model → set ANTHROPIC_DEFAULT_OPUS_MODEL=<model>[1m] or config.models.<role> to a 1M model · warn: implementer, reviewer, assembler on a 200K model → set ANTHROPIC_DEFAULT_SONNET_MODEL=<model>[1m] or config.models.<role> to a 1M model

idea: Fully autonomous; both roasts (design roast and PR roast) on. Scope: herdr-graph side of the approved design docs/reviews/2026-10-29-seat-review-H2-identity-design.md (read also docs/reviews/2026-10-29-seat-review-H2.md and docs/reviews/2026-10-29-seat-review-H2-identity.md). Includes: H2 Option D (bead hg-s6o: /seat drops accept_command, skill step 5 delegates acceptance to herdr-threads); clone.threads_seat write-once binding; participation keyed to the clone/threads seat (not occupant), kept across agent exit (D3); agent launch/resume sets HERDR_GRAPH_CLONE=<cl_>; delete clone re-matching in favour of SeatInspect; undo/resurrection via threads UnretireSeat + relaunch with key; G4 headless resume driver; tab/workspace matching by preserved Herdr ids. Threads-side work (G1 v2.8.2 release, G2, D1 continuity key, D2 reversible retirement with requirement restore, G5) is owned by the herdr-threads coordinator (seat snSGTpBk7xGd, thread dOqG4VQNjQ4); graph tasks that depend on unreleased threads features must be sequenced/blocked on them, not faked. Constraints: Herdr is not modified; do not touch the user's live graph instance or live Herdr layout; another session may be editing src/cli/seat.rs and skills — check main is current before starting and rebase as needed.
branch: super-auto/threads-seat-identity
base: main
epic: hg-66u
roastDesignRound: 1
roast-design: 2026-11-05-threads-seat-identity-roast-design-1-roast-design-1.md
findingsLedgerDesign: findings-ledger-design.md
spec: 2026-11-04-threads-seat-identity-design.md
skillSource: /var/home/alepar/.aisw/profiles/claude/claude-1/plugins/cache/superpowers-alepar/superpowers/6.4.2-alepar6.8/skills @ 34fd607b17373d09b2099cf2bec3745b91c10cb7 (6.4.2-alepar6.8)

approvals:
- top-split · auto · hg-66u.1 LEAF, hg-66u.2 LEAF, hg-66u.3 LEAF, hg-66u.4 LEAF, hg-66u.5 LEAF, hg-66u.6 LEAF, hg-66u.7 LEAF, hg-66u.8 LEAF, hg-66u.9 LEAF, hg-66u.10 LEAF, hg-66u.11 LEAF, hg-66u.12 LEAF, hg-66u.13 LEAF, hg-66u.14 LEAF, hg-66u.15 LEAF, hg-66u.16 LEAF, hg-66u.17 LEAF, hg-66u.18 LEAF, hg-66u.19 LEAF, hg-66u.20 LEAF, hg-66u.21 LEAF, hg-66u.22 LEAF (promotion review: SPLIT .7→.19, SPLIT .13→.21, PROMOTE .10 and .16 overruled sp:demoted-by-session; spec amended c77dfe3)
- coverage-round-1 · R1-R14 (R15-R21 appended from r-new) · specsAt b992cab337bbcbe87dca1bcda205cdba3fa8c455 · requirements: 14 · mapped: 14 · unmapped: 0 · cov-3..cov-22 applied auto except cov-10 rejected auto (see coverage-ledger.md)
- coverage-round-2 · requirements: 21 · mapped: 21 · unmapped: 0 · cov-23..cov-46 applied auto · divergence: findings: 19 → 20 novel: 12/20 (60%) widening: yes 

parked:
- 2026-11-05-threads-seat-identity-roast-design-1-roast-design-1.md · escalation · "UNVERIFIED external: §9 90 s grace + point-in-time shell-foreground check vs Herdr auto-resume latency (spike: restore 20+ agent panes with a client attached, time foreground transitions, test late attach)"
- 2026-11-05-threads-seat-identity-roast-design-1-roast-design-1.md · escalation · "UNVERIFIED external: §9 assumes a pane seat stays Resolved after its agent exits with the pane open (B8); check on threads v3.0.0: agent checks in, exits, SeatInspect"
- 2026-11-05-threads-seat-identity-roast-design-1-roast-design-1.md · escalation · "material dissent: §7/§9 assume Herdr never reissues a public workspace/tab/pane id; Herdr reserve_workspace_ids can reissue a deleted highest workspace id after restore"
- 2026-11-05-threads-seat-identity-roast-design-1-roast-design-1.md · escalation · "UNVERIFIED external: §7 'Herdr preserves tab/workspace ids across restore and handoff' unsourced for tabs and for live handoff on v0.9.5"
- 2026-11-05-threads-seat-identity-roast-design-1-roast-design-1.md · escalation · "UNVERIFIED external: §9 headless cold restart assumes Herdr v0.9.5 restores bare-shell panes with no client attached"
redesignCycles-design: 1
redesign-design-1: round 1 · triggers shared-cause, defect-type · amended 2. Clone threads seat (write-once), 5. Launching with the continuity key, 6. Location and liveness from threads; delete clone re-matching, 8. Undo and resurrection onto the same seat (D2), 9. Resume driver after a headless cold restart (G4), Key abstractions, Unchanged behavior · lastGood 4795ea116f946a9551a7b59dd1037854bbcbe200 · designRoast - · state specs-committed
stepBackDesign-round-1: redesign — applied: implicit retire/restore protocol inferred from (clone active, seat Retired) → persisted SeatIntent (Restore/Finalize) on CloneRecord driving unretire, succession, resume and finalize (dissolves 6)
