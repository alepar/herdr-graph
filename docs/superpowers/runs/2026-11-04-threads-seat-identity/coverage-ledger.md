# coverage ledger — 2026-11-04-threads-seat-identity

cov-1 · r1 · flag-sweep · hg-66u.10 · kept as leaf — PROMOTE overruled: spec c77dfe3 defines retired mapping and pre-D2 undo (seat succession leaf hg-66u.20); alert display moved to hg-66u.13
cov-2 · r1 · flag-sweep · hg-66u.16 · kept as leaf — PROMOTE overruled: split into threads-side leaf and undo rewrite hg-66u.22; D2 API uncertainty handled by the D2 gate
cov-3 · r1 · GAP · R3 parity · applied — new leaf hg-66u.23 (parity scenario table), hg-66u.14 depends on it
cov-4 · r1 · GAP · G5 limbo · applied — new leaf hg-66u.24 (gated G5, owns G5 pin and mapping); hg-66u.18 depends on it
cov-5 · r1 · GAP · R8 resume trigger · applied — hg-66u.12/.21 acceptance: agent exit with pane open launches nothing
cov-6 · r1 · GAP · R3 unreachable on actuators · applied — acceptance lines on .8 .12 .16 .20 .21
cov-7 · r1 · GAP · R10 succession after D2 · applied — hg-66u.22 restricts succession to final-retired seats
cov-8 · r1 · GAP · R6 env delivery · applied — hg-66u.9 must confirm from Herdr source or use env argv prefix; not closable unmet
cov-9 · r1 · GAP · R5 v3 handoff · applied — hg-66u.4 acceptance cites ht-hsyg inclusion
cov-10 · r1 · GAP · R15 pin bumps · rejected — evidence does not hold: hg-66u.13 and hg-66u.16 descriptions already own their D1/D2 bumps; G5 bump now owned by hg-66u.24; owns lines made explicit
cov-11 · r1 · GAP · R1 thin path · applied — hg-66u.17 depends on hg-66u.7
cov-12 · r1 · GAP · R7 undo acceptance · applied — hg-66u.22 tab reopen/session resume; hg-66u.10 pane close retires
cov-13 · r1 · GAP · R17 pre-upgrade clones · applied — acceptance on hg-66u.20 and hg-66u.22
cov-14 · r1 · GAP · R18 key-conflict resolution · applied — hg-66u.13 acceptance
cov-15 · r1 · GAP · R16 pane closed during downtime · applied — hg-66u.12 acceptance (no launch) + hg-66u.24
cov-16 · r1 · GAP · R9 reservation ordering · applied — key env emitted only once reserved (hg-66u.9, spec §5)
cov-17 · r1 · UNOWNED-SEAM · key-conflict alert · applied — owned by hg-66u.13; guard moved from .12 to .21
cov-18 · r1 · UNOWNED-SEAM · continuity key / reservation · applied — owns line on hg-66u.13
cov-19 · r1 · UNOWNED-SEAM · unretire want · applied — owns line on hg-66u.16
cov-20 · r1 · UNOWNED-SEAM · PaneSeatMap / resolve-seat want · applied — owns line on hg-66u.7
cov-21 · r1 · ORPHAN · hg-66u.20 · applied — kept per reviewer's conditional fix (D2 may slip past this run); interim outcome named in its description
cov-22 · r1 · ORPHAN · hg-66u.3 · applied — wired to hg-66u.24
cov-23 · r2 · GAP · R9 key linkage pre-D1 · applied — hg-66u.13 backfill + seat-bearing reserve requested from threads (mnSGjHOtxksb; coordinator recommends accepting) with keyed-relaunch fallback; spec §5
cov-24 · r2 · GAP · R3 unreachable remaining actuators · applied — acceptance on .7 .13 .22 .24
cov-25 · r2 · GAP · R10 no release on retirement · applied — hg-66u.8 acceptance and want existence condition
cov-26 · r2 · GAP · R20 parity blocking · applied — hg-66u.14 blocked on clean post-D1 parity rerun or human open item
cov-27 · r2 · GAP · R15 D1 pin · applied — owns + acceptance on hg-66u.13; pin acceptance on hg-66u.16 (cov-10 shortfall)
cov-28 · r2 · GAP · R14 Unsupported service · applied — hg-66u.6 acceptance
cov-29 · r2 · GAP · R7 interim undo · applied — hg-66u.20 reopens tab, no caller-pane adoption (moved from .22), one launch; spec §8 interim (cov-21 shortfall)
cov-30 · r2 · GAP · R4 rig-unmet hand-back · applied — clause on .4 .21 .22
cov-31 · r2 · UNOWNED-SEAM · seat succession · applied — hg-66u.20 owns; .22 consumes
cov-32 · r2 · UNOWNED-SEAM · SeatStatus extensions · applied — owns lines on .13 and .16
cov-33 · r2 · GAP · R16 stuck Unknown · applied — hg-66u.12 status remedy; root epic hand-back rule for unreleased gates
cov-34 · r2 · GAP · R10 finalizers / operator exception · applied — hg-66u.22 cites no threads timer (confirmed by coordinator); operator final retire succession kept as documented exception (operator explicitly chose finality; not a scope change)
cov-35 · r2 · GAP · R5 daemon reconnect · applied — new leaf hg-66u.25; hg-66u.17 depends on it
cov-36 · r2 · GAP · R8 harness switch · applied — hg-66u.12 acceptance
cov-37 · r2 · GAP · R9 first launch keyed · applied — hg-66u.13 reserve at creation (holder = graph service, no seat needed; coordinator confirms)
cov-38 · r2 · GAP · R9 fresh launch unreserved · applied — hg-66u.21 acceptance
cov-39 · r2 · GAP · R2 planner naming · applied — hg-66u.14 acceptance
cov-40 · r2 · GAP · R12 graph retire with pane open · applied — hg-66u.8 acceptance (verify + document)
cov-41 · r2 · GAP · gates unreleased · applied — root epic Global Constraints
cov-42 · r2 · UNOWNED-SEAM · resume driver launch owner · applied — hg-66u.12 owns sole launch want; spec §9
cov-43 · r2 · UNOWNED-SEAM · reservation holder · applied — holder is the graph service, no seat dependency; spec §5
cov-44 · r2 · UNOWNED-SEAM · shared matcher code · applied — hg-66u.14 depends on hg-66u.11, ownership split stated
cov-45 · r2 · ORPHAN · hg-66u.5 · applied — kept as baseline-only requirement (B11/R11), no Goal edit
cov-46 · r2 · GAP · R4 attended cold restart · applied — hg-66u.12 acceptance (C1, grace avoids auto-resume race); spec §9 defines C1
