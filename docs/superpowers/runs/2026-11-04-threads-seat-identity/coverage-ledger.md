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
