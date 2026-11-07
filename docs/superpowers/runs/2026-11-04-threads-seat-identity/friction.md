# friction log — 2026-11-04-threads-seat-identity
- [2026-11-05] super-design coverage: round 1 and 2 reviews saved as condensed orchestrator copies (requirements block + findings by type/subject), not verbatim; full texts were returned as subagent hand-backs only.
- [2026-11-05] super-design coverage: NEEDS-SPEC requests named leaf beads (hg-66u.12, hg-66u.13), not subepics; not honoured, answered by spec amendments in the disposition commit.
- [2026-11-05] super-roast design round 1: rung Workflow.
- [2026-11-05] report format repair rejected: docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-05-threads-seat-identity-roast-design-2.md (repair-check (iv) line count differs; original stands; parser error: none marker has finding text in Escalations (need human))
- [2026-11-05] super-code pre-flight: repo has no CI config; mergeCheck covers default and --all-features; no CI matrix to compare.
Profile: what-if estimate — none saves more than 1% of the modelled 1h05m; first dispatch to last landing, planning retained
Profile: context — agents 308 (measured 308) · peak≥80% 0 · tasks>1 1 · compacted 0 agents, 0 compactions · coordinator 569K/1000K (57%), 0 compactions · max coordinator 57%
Profile: context agent — coordinator peak 569K/1000K (57%) · first 96K · compactions 0
Profile: unmeasured — 4 dispatched bead(s) did not land (not in the model): hg-66u.4, hg-66u.7, hg-66u.6, hg-66u.8; Claude context windows from the model table: claude-opus-5-5 1000K, claude-sonnet-5-5 1000K
- [2026-11-05] super-code: host has no C toolchain after the 2026-11-03 reboot; all 6 first-round merges failed mergeCheck; unplanned coordinator stop (in-flight work lost), relaunched with toolbox-wrapped commands (ht-build).
- [2026-11-05] super-code: two invocations ended ready-drained/stalled while a review bead closed just after the round's readiness query (hg-66u.28 became ready after Finish began); re-entered each time.
- [2026-11-05] super-code final review: cluster 4 over the 200K window, never written; collection gap recorded, re-entry proceeds (the review is not an input to the next invocation).
- [2026-11-05] roast-code r1: assembler review input selection: provenance unavailable; captured source available (collection gap inv-4aa37d70 cluster-4); bounded direct review of captured base/head, round unavailable for acceptance.
- [2026-11-05] report format check: docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-05-threads-seat-identity-roast-design-3.md exit 4 (none marker has finding text in Escalations — trailing ---); repair not re-dispatched: the same repair failed repair-check (iv) in round 2; original stands.
- [2026-11-05] roast-code r2: same assembler provenance gap (inv-4aa37d70 cluster-4); bounded direct review.
- [2026-11-06] roast-code r2: reporter failed (account usage limit); report is the engine fallback (raw seat severities, default routes); degraded round.
- [2026-11-06] fix loop r2: finding-route records not written per occurrence (manual routing to beads in run.md/bead text); projection routes may show unrouted.
- [2026-11-06] phase-6 sweep: private-herdr tier hit a rustc ICE (failed to spawn work thread, EAGAIN); 51 stale private test herdr servers (HOME=/tmp/hg-*, 29-31h old) are still running; terminating them was refused by the permission layer — human action: stop them (pgrep -u $USER -f 'herdr server' with HOME under /tmp/hg-*) and remove their /tmp/hg-* dirs.
- [2026-11-06] build environment: per the user (relayed by the herdr-threads coordinator, mnSGFz51FTYi), herdr-graph moved from the shared ht-build toolbox to its own hg-build (fedora-toolbox:44 + gcc); leaked herdr-graph test rigs in ht-build await the user's approval to stop.
- [2026-11-07] gated re-entry: hg-66u.44 recorded its tests in toolbox ht-build and hg-66u.21 in distrobox durak-dev despite the hg-build constraint (planner briefs predated it or implementers ignored it).
- [2026-11-07] main drifted by a concurrent super-auto run (seat-failure-manager-seats, ~12.5k code lines); trial merge conflicted in 13 files; the run had no point that integrated main before finish.
- [2026-11-07] session scratchpad wiped again (coordinator.js, code-args.json, settle.sh lost); re-copied coordinator.js from the skill cache and rebuilt args.
