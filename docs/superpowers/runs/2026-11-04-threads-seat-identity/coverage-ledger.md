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
cov-47 · amend-design-1 · GAP · amended beads' acceptance copied from superseded beads · applied — new acceptance on .27 .28 .30 .31 .34
cov-48 · amend-design-1 · GAP · .31 acceptance contradicted intent-gated unretire · applied — line replaced
cov-49 · amend-design-1 · GAP · .34 acceptance named the impossible argv prefix · applied — rewritten around shell export, status on rejection, unset on kept pane
cov-50 · amend-design-1 · GAP · matcher/rebind deletion before unseated key path · applied — .14 .15 .18 depend on .33; .18 config added
cov-51 · amend-design-1 · GAP · key relink on succession · applied — .29 owns, .32 acceptance; spec §13; threads asked (mnSG57YUhprU)
cov-52 · amend-design-1 · GAP · key release at purge · applied — .13 acceptance; spec §13
cov-53 · amend-design-1 · UNOWNED-SEAM · Restore clear on re-retirement · applied — .27 owns; spec §13
cov-54 · amend-design-1 · UNOWNED-SEAM · key-conflict alert storage · applied — .28 owns the level, .13 production + status text; .21 restated on KeyConflict status
cov-55 · amend-design-1 · GAP · .32 missing dep on .29, stale ref · applied
cov-56 · amend-design-1 · ORPHAN · superseded beads still open · rejected — evidence does not hold: hg-66u.9/.10/.12/.16/.19/.20/.22 are closed (superseded); the dump included closed beads
cov-57 · amend-design-1 · GAP · re-issued ReserveContinuityKey returns no seat (unconfirmed shape) · rejected — evidence does not hold: shapes doc ContinuityKeyReserved{key, owner, seat?, reserved_at} "seat? gives the binding once an agent has checked in"
cov-58 · amend-design-1 · GAP · pane keeps key after graph retire · applied — .34 acceptance
cov-59 · amend-design-1 · GAP · shell rejecting export is silent · applied — .34 acceptance; spec §13
cov-60 · amend-design-1 · GAP · R19 restore trigger unbounded, unseated arm in steady state · applied — .30 acceptance; spec §13
cov-61 · amend-design-1 · UNOWNED-SEAM · first launch of a new post-D1 clone · applied — .30 owns; spec §13
cov-62 · amend-design-1 · UNOWNED-SEAM · tombstone shape and reader exclusion · applied — .27 owns shape, .31 owns exclusion
cov-63 · amend-design-1 · UNOWNED-SEAM · .31/.29 owns lines lost on supersession · applied — literal owns: lines added
cov-64 · amend-design-1 · GAP · .29 cannot assert the restore launch without .30 · applied — .29 depends on .30
cov-65 · amend-design-1 · GAP · early resolve during restore · applied — .7 .29 .32 acceptance; spec §13
cov-66 · graph · GRAPH-EDGE · hg-66u.13 <- hg-66u.30 · parked — narrow to .34 + .28, safe no (shared planner.rs, acceptance test needs .30)
cov-67 · graph · GRAPH-EDGE · nine other critical edges · kept — each carries a real artifact (see graph-pass judge output)
cov-68 · graph · PROPOSAL · seat_fact seam contract .28/.30 · parked
cov-69 · graph · PROPOSAL · split src/reconcile/planner.rs · parked
cov-70 · amend-code-1 · GAP · migration re-invite permanent/unbounded (.59) · applied — one-shot migration marker, active Required only (bead text)
cov-71 · amend-code-1 · GAP · pre-existing Finalize tombstones on disk · applied — .57 deletes on load
cov-72 · amend-code-1 · GAP · purged clone with live seat pre-D2 · rejected — evidence does not hold: §14.10 a graph retire always closes the pane (hg-66u.8 verified), so the seat is retired threads-side
cov-73 · amend-code-1 · GAP · not-launchable restore after D1 · applied — .21 acceptance + dep on .58
cov-74 · amend-code-1 · GAP · undo beads not waiting for key-link want · applied — .32 .18 depend on .68
cov-75 · amend-code-1 · UNOWNED-SEAM · key-link want vs .43/.44 · applied — .68 owns want + bound-seat field, Restore.seat during restore; .43/.44 consume
cov-76 · amend-code-1 · GAP · permanent per-seat refusal · applied — .60 seat-lost alert after staleness window
cov-77 · amend-code-1 · UNOWNED-SEAM · last threads refusal storage · applied — .58 owns, on the Restore intent
cov-78 · amend-code-1 · GAP · .31 tombstone audit sites · applied — named sites + dep on .57
cov-79 · amend-code-1 · ORPHAN · hg-66u.67 · applied — kept as process bead serving the final whole-epic review
cov-80 · amend-code-1 · GAP · mark-failed retry semantics · applied — .58 acceptance
cov-81 · amend-code-1 · GAP · export-rejecting shells after key-link · applied — .32: C1 with resumable session; stuck_restores remedy otherwise
cov-82 · amend-code-1 · GAP · purge in a D1-to-D2 window · rejected — evidence does not hold: D1 and D2 ship in one threads release (mnSGq1eyidhP), no window
cov-83 · amend-code-1 · GAP · R6/R11/R13 without open owner · rejected — delivered by closed beads hg-66u.37 (env delivery), hg-66u.5 (/seat), hg-66u.11 (preserved ids), each with clean review
