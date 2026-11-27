# super-auto run — 2026-11-26-resurrect-seat-frictionless-restart

flags: planOneShot=f skipPlanRoast=f skipCodeRoast=f autonomous=t
phase: roast-design
codeMechanism: ordinary-subagents
runTag: resurrect-seat-frictionless-restart-20261126T231316
context: Context: claude · planner=200K/167K · implementer=200K/167K · reviewer=200K/167K · finalReview=200K/167K · assembler=200K/167K · warn: planner, finalReview on a 200K model → set ANTHROPIC_DEFAULT_OPUS_MODEL=<model>[1m] or config.models.<role> to a 1M model · warn: implementer, reviewer, assembler on a 200K model → set ANTHROPIC_DEFAULT_SONNET_MODEL=<model>[1m] or config.models.<role> to a 1M model

idea: hg-lnw: Undo of a closed adopted (keyless) tab/pane resurrects the original herdr-threads seat via threads 3.5.0 ResurrectSeat/FinalizeRetirement (service_session_v4); bump threads pin 3.1.0 -> 3.5.0 with contract audit of every minor; Herdr restart frictionless for every graph-managed tab (no operator rebind). Full brief in herdr-threads message mnSGHEfT9jck on thread dn6PT7Q1PAd from the main-tab coordinator; hand back branch, report path, tier 1-3 and 5 results. Coordinator merges.
branch: super-auto/resurrect-seat-frictionless-restart
base: main
spec: 2026-11-26-resurrect-seat-frictionless-restart-design.md
epic: hg-lnw
roastDesignRound: 2
roast-design: 2026-11-26-resurrect-seat-frictionless-restart-roast-design-1.md, 2026-11-26-resurrect-seat-frictionless-restart-roast-design-2.md
findingsLedgerDesign: findings-ledger-design.md
redesignCycles-design: 1
redesign-design-1: round 1 · triggers shared-cause, defect-type · amended §3, §4 · lastGood 723b39a51ec3cd10342deb957ed4de70d88320cc · designRoast r2 fixed · state applied
stepBackDesign-round-1: redesign — applied: keyless relaunch gate becomes level-triggered (seat fact Resolved on its pane, or 90 s grace), rebind re-planned while Unresolved

approvals:
- top-split · auto · hg-lnw.1 LEAF, hg-lnw.2 LEAF, hg-lnw.3 LEAF, hg-lnw.4 LEAF, hg-lnw.5 LEAF (PROMOTE overruled, sp:demoted-by-session), hg-lnw.6 LEAF, hg-lnw.7 LEAF, hg-lnw.8 LEAF, hg-lnw.9 LEAF, hg-lnw.10 LEAF, hg-lnw.11 LEAF
- coverage-round-1 · L1 flag-sweep kept auto; L2-L10 applied auto; L11 rejected auto · R1..R12 in coverage-round-1-requirements.md · specsAt f1085a469e7441a8509e384b51ed8f96ebd683d5 · requirements: 11 · mapped: 11 · unmapped: 0
- coverage-round-2 · L12, L13, L15-L22 applied auto; L14 rejected auto; reviewer 2 NEEDS-SPEC hg-lnw.5 not re-run (answered from spec §4.2 into the bead) · divergence: findings 10 → 9 · novel 5/9 (56%) · widening: no · sweep hg-lnw.12

parked:
- 2026-11-26-resurrect-seat-frictionless-restart-roast-design-1.md · beyond-cap · "30 severe candidates beyond config.panelCap=10 left unjudged (Not verified (beyond panel cap)); unattended answer: no re-roast with a raised cap"
- 2026-11-26-resurrect-seat-frictionless-restart-roast-design-1.md · degraded-verdict · "verdict qualifier [panel-capped: 30 unverified]"
- coverage-amend-design-1 · L23-L29, L31, L32 applied auto; L30 rejected auto · R1..R12 (coverage-round-1-requirements.md)
- 2026-11-26-resurrect-seat-frictionless-restart-roast-design-2.md · escalation · "§4.2 late rebind: does an already-running fresh keyless agent enroll at its next turn or only at a lifecycle event (UNVERIFIED external; asked herdr-threads graph-asks; tier-3 check added to hg-lnw.9)"
