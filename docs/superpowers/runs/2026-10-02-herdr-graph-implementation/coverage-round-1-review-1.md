requirements:
R1 → hg-zmi.1, hg-zmi.4, hg-zmi.14, hg-zmi.11
R2 → hg-zmi.2, hg-zmi.1
R3 → hg-zmi.3
R4 → hg-zmi.2
R5 → hg-zmi.3
R6 → hg-zmi.17
R7 → hg-zmi.6
R8 → hg-zmi.6, hg-zmi.7, hg-zmi.8, hg-zmi.13, hg-zmi.12
R9 → hg-zmi.6, hg-zmi.17
R10 → hg-zmi.5
R11 → hg-zmi.7
R12 → hg-zmi.8
R13 → hg-zmi.9
R14 → hg-zmi.9, hg-zmi.10
R15 → hg-zmi.10
R16 → hg-zmi.11
R17 → hg-zmi.8, hg-zmi.12
R18 → hg-zmi.12, hg-zmi.6
R19 → hg-zmi.12, hg-zmi.13, hg-zmi.18
R20 → hg-zmi.13
R21 → hg-zmi.13
R22 → hg-zmi.3, hg-zmi.5, hg-zmi.11, hg-zmi.15, hg-zmi.16, hg-zmi.19, hg-zmi.14
R23 → hg-zmi.5
R-new: Daemon startup convergence (journal recovery → fresh snapshot → re-correlate in-flight effects → reconcile latest intent) → hg-zmi.18, hg-zmi.3, hg-zmi.7
R-new: Agent harness launch/resume profiles (shell/claude/codex) shared by templates, reconciler, smoke tests → (unmapped)

findings:
- GAP · R23 memory-observer isolation unmapped
- GAP · R6 supersession not named
- GAP · reminder scheduler not started (hg-zmi.17 ∉ hg-zmi.18 deps)
- GAP · mutation kinds of hg-zmi.9/hg-zmi.17 not registered in daemon
- GAP · tier-3 flows exclude hg-zmi.17 ops and hg-zmi.13 bootstrap
- GAP · /seat pending invitations without threads dep
- GAP · README matrix written before verification
- GAP · resurrection not an executable kind
- GAP · harness launch/resume profiles unowned
- UNOWNED-SEAM · reconciler relaunch hook (hg-zmi.7 → hg-zmi.12)
- UNOWNED-SEAM · threads effect extension point (hg-zmi.7 → hg-zmi.11)
- UNOWNED-SEAM · effect record persistence (hg-zmi.3 ↔ hg-zmi.6 ↔ hg-zmi.7)
- UNOWNED-SEAM · template file format/semantics (hg-zmi.13 ↔ hg-zmi.9)
- UNOWNED-SEAM · HERDR_GRAPH/caller-pane env contract (hg-zmi.5/7 → hg-zmi.13, hg-zmi.10)
- UNOWNED-SEAM · agent-relay confirmation surface (hg-zmi.6 → hg-zmi.13)
- UNOWNED-SEAM · undoable action record schema (hg-zmi.8, hg-zmi.9 → hg-zmi.10)
- UNOWNED-SEAM · summaries config key (hg-zmi.6 → hg-zmi.12)
- UNOWNED-SEAM · pending ops/invitations query (hg-zmi.17/hg-zmi.11 → hg-zmi.13)
No ORPHAN.
