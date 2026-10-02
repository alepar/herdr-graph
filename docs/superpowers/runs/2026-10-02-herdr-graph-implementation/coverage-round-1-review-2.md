requirements:
R1 → hg-zmi.1, hg-zmi.4, hg-zmi.14, hg-zmi.11
R2 → hg-zmi.1, hg-zmi.2
R3 → hg-zmi.3
R4 → hg-zmi.2, hg-zmi.3
R5 → hg-zmi.3
R6 → hg-zmi.17
R7 → hg-zmi.6, hg-zmi.13
R8 → hg-zmi.6, hg-zmi.7, hg-zmi.8, hg-zmi.12, hg-zmi.13
R9 → hg-zmi.6, hg-zmi.17, hg-zmi.13
R10 → hg-zmi.5
R11 → hg-zmi.7, hg-zmi.15
R12 → hg-zmi.8
R13 → hg-zmi.9
R14 → hg-zmi.9, hg-zmi.10
R15 → hg-zmi.10
R16 → hg-zmi.11, hg-zmi.17
R17 → hg-zmi.8, hg-zmi.12
R18 → hg-zmi.12
R19 → hg-zmi.12, hg-zmi.13, hg-zmi.7, hg-zmi.11
R20 → hg-zmi.13
R21 → hg-zmi.13
R22 → hg-zmi.3, hg-zmi.5, hg-zmi.15, hg-zmi.16, hg-zmi.19, hg-zmi.11, hg-zmi.14
R23 → hg-zmi.5
R-new: Every background loop (reconciler, observer snapshot, threads connection, transcript, reminder scheduler) is started by the daemon composition root → hg-zmi.18
R-new: Integration test against a real herdr-threads service, not only FakeThreads → (unmapped)
R-new: Undo covers the tree's own plan-applied organizational actions (create/retire/rename via plan), not only cascades and template/application actions → (partially unmapped)

findings:
- GAP · R23 · no test-isolation guard for memory observer / private HOME / threads endpoint → extend hg-zmi.5 fixture
- GAP · reminder scheduler never started (hg-zmi.17 not in hg-zmi.18 deps)
- GAP · R22 real threads integration test unmapped; hg-zmi.19 does not cover .11/.17
- GAP · R14 template copy kind not named
- GAP · R6 supersession not named
- GAP · R15 plan-applied kinds produce no action records
- GAP · R2 name history persistence unowned
- UNOWNED-SEAM · reconciler relaunch hook (hg-zmi.7 ↔ hg-zmi.12, hg-zmi.15)
- UNOWNED-SEAM · reconciler effect-kind registration API (hg-zmi.7 ↔ hg-zmi.11)
- UNOWNED-SEAM · undoable action-record schema (hg-zmi.8, hg-zmi.9 → hg-zmi.10)
- UNOWNED-SEAM · HERDR_GRAPH env contract (hg-zmi.5/7 ↔ hg-zmi.13)
- UNOWNED-SEAM · per-seat summaries key + summarizer role marker (hg-zmi.6/13 ↔ hg-zmi.12)
- UNOWNED-SEAM · transcript identity/byte-range record (hg-zmi.8 → hg-zmi.12)
- UNOWNED-SEAM · service-ACK capability/fallback switch (hg-zmi.11 ↔ hg-zmi.12)
No ORPHAN.
