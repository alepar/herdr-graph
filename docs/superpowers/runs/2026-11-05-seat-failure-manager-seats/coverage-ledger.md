# coverage ledger — 2026-11-05-seat-failure-manager-seats
C1 · r1 · flag-sweep · hg-k4l.7 · kept as leaf — PROMOTE resolved by splitting templates into hg-k4l.14; spec §4.8 answers the design
C2 · r1 · flag-sweep · hg-k4l.8 · kept as leaf — PROMOTE resolved by splitting reparenting into hg-k4l.15; spec §4.9/§4.11 answer the design
C3 · r1 · flag-sweep · hg-k4l.11 · kept as leaf — PROMOTE resolved by splitting who/show into hg-k4l.16; spec §4.3/§4.12 answer the design
C4 · r1 · GAP · B11 placement · applied — hg-k4l.8 acceptance: refuse placement changes; spec §4.9
C5 · r1 · GAP · R18 · applied — new leaf hg-k4l.17; spec §4.11
C6 · r1 · GAP · B7 observed cascades · rejected — observed closure retirement is the existing sanctioned non-plan mutation; reparent rides along; documented in spec §4.11
C7 · r1 · GAP · B10 paths · applied — hg-k4l.7 acceptance: one shared helper on every create/resurrect path; spec §4.8
C8 · r1 · GAP · B10 reading · rejected — human approved "the first seat created" ("lgtm"); recorded as baseline S1
C9 · r1 · GAP · B2 · applied — hg-k4l.2 generic retry sentence; new leaf hg-k4l.18; spec §4.4
C10 · r1 · UNOWNED-SEAM · manager-pointer effect kinds · applied — hg-k4l.5 owns effect constructors; .6/.7/.13/.15 consume
C11 · r1 · UNOWNED-SEAM · integrity message format · applied — hg-k4l.12 owns builder with suffix slot; hg-k4l.11 fills; end-to-end text test in .11
C12 · r1 · UNOWNED-SEAM · who reply path · applied — edge hg-k4l.16 -> hg-k4l.3
C13 · r1 · UNOWNED-SEAM · node-manager set spelling · applied — hg-k4l.5 owns NODE_MANAGER_SET_USAGE; .6/.8/.10 consume; edge hg-k4l.10 -> hg-k4l.5
C14 · r1 · GAP · hg-k4l.10 read path · applied — acceptance states direct HEAD read (spec §4.10) and dangling-pointer warning
C15 · r1 · GAP · R2 outside Herdr · applied — hg-k4l.2: distinct variant, exit 2; spec §4.4
C16 · r1 · GAP · documentation · applied — new leaf hg-k4l.20
C17 · r1 · GAP · R20 · applied — new leaf hg-k4l.19; spec §4.12
C18 · r1 · GAP · R15 text output · applied — hg-k4l.11 acceptance
C19 · r1 · GAP · R9 undo-driven retirement · applied — hg-k4l.15 acceptance
C20 · r1 · GAP · R3 dangling node manager · applied — hg-k4l.11 acceptance
C21 · r1 · GAP · R11 workspace reopen · applied — hg-k4l.9 acceptance
C22 · r1 · GAP · show integrity exit · applied — hg-k4l.16 acceptance
C23 · r2 · GAP · R18 action-record format · applied — hg-k4l.15 owns {holder, field, old, new} on every route; hg-k4l.17 consumes
C24 · r2 · GAP · cycles from other plans · applied — new leaf hg-k4l.21; spec §4.9
C25 · r2 · GAP · retired-target pointers · applied — hg-k4l.4 transparency rule; spec §4.6
C26 · r2 · GAP · dangling node_manager_seat · applied — hg-k4l.4 (treated as unassigned), hg-k4l.6 (assign semantics); spec §4.6/§4.7
C27 · r2 · GAP · B11 observed move · applied — hg-k4l.9 acceptance; spec §4.9
C28 · r2 · GAP · B11 deactivation · applied — hg-k4l.8 acceptance; spec §4.9
C29 · r2 · GAP · B7 observed cascades (C6 re-raised) · parked — needs human: approve observed-cascade reparenting outside plans or require a reparent plan
C30 · r2 · UNOWNED-SEAM · seat.resolve reply struct · applied — hg-k4l.11 owns SeatRef and text layout; edge hg-k4l.11 -> hg-k4l.2; hg-k4l.19 consumes
C31 · r2 · UNOWNED-SEAM · live node manager · applied — hg-k4l.4 owns live_node_manager; .8/.9/.10/.11 consume
C32 · r2 · UNOWNED-SEAM · show exit-6 rendering · applied — hg-k4l.2 owns local StoreError::Integrity rendering; hg-k4l.16 consumes
C33 · r2 · GAP · B4 silent-drop scope · rejected — the approved change named the seat.resolve silent drops (review H3: mod.rs:178,193,200); no other readers were in scope
C34 · r2 · GAP · B12 warning exclusions · parked — accepted design narrowing (init/setup/daemon/hook-prompt have no graph or must keep output clean); surfaced for human confirmation
C35 · r2 · GAP · R8 undo of first create · applied — hg-k4l.8 refusal names the remedy
C36 · r2 · GAP · B13 duties after move · applied — hg-k4l.19 acceptance; spec §4.12
C37 · r2 · UNOWNED-SEAM · CURRENT-DESIGN ordering · applied — hg-k4l.20 depends on .9/.6/.17/.18
C38 · r2 · GAP · R19 entry points · applied — hg-k4l.18 acceptance enumerates client entry points
C39 · graph · GRAPH-EDGE · hg-k4l.20 <- hg-k4l.17 · parked — drop is safe no (doc bead describes landed undo behaviour); would cut depth 7→6
C39k · graph · GRAPH-EDGE · hg-k4l.17 <- hg-k4l.15 · kept — carries a real artifact
C39k · graph · GRAPH-EDGE · hg-k4l.15 <- hg-k4l.5 · kept — carries a real artifact
C39k · graph · GRAPH-EDGE · hg-k4l.5 <- hg-k4l.4 · kept — carries a real artifact
C39k · graph · GRAPH-EDGE · hg-k4l.4 <- hg-k4l.1 · kept — carries a real artifact
C40 · amend-code-1 · GAP · B11 reopen with moved_out set (tab path, partial closure, foreign container) · applied — hg-k4l.62 acceptance tests (a)-(c); moved_out never gates node-manager reopen
C41 · amend-code-1 · GAP · superseded classify-side tests/docs from hg-k4l.9/.57/.58/.61 · applied — hg-k4l.62 acceptance (rewrite tests, CURRENT-DESIGN and module docs)
C42 · amend-code-1 · UNOWNED-SEAM · LiveIndex::workspace placement · rejected — hg-k4l.62 description owns it (reviewers saw a truncated line)
C43 · amend-code-1 · UNOWNED-SEAM · CascadeMutation applied ends -> commit_ops SessionEnded · rejected — hg-k4l.62 description owns it
C44 · amend-code-1 · UNOWNED-SEAM · per-clone survival in two places (hg-k4l.61) · rejected — hg-k4l.61 is closed; hg-k4l.62 owns the single survival fact and replaces its code
C45 · amend-code-1 · GAP · node manager teamspace stays Active under the new path · applied — hg-k4l.62 acceptance test
C46 · amend-code-1 · GAP · reopened tab keeps recorded identity · applied — hg-k4l.62 acceptance test
C47 · amend-code-1 · GAP · lazy reparent target preview/apply parity · applied — hg-k4l.63 acceptance (one function, preview + apply + observed cascade test)
C48 · amend-code-1 · GAP · no end-to-end after the amend · applied — new leaf hg-k4l.67 (integration sweep, deps .62/.63/.64)
C49 · amend-code-1 · GAP · hg-k4l.64 ordering · rejected — hg-k4l.54/.55/.56 are already merged and closed
C50 · amend-code-1 · ORPHAN · hg-k4l.65 · kept — outcome named (mergeable without stray artifacts; findings r1 #7 and two final reviews)
C51 · amend-code-1 · ORPHAN · hg-k4l.66 · kept — outcome named (migration notice for R1, R5, exit codes)
C52 · amend-code-1 · GAP · B7 observed pointer writes · parked — still C29; amendment adds no pointer-writing route beyond do_retire_seat, but CascadeMutation's kept branch now also writes the re-read teamspace record
C53 · amend-code-1 · precheck · flag-sweep 8 / unstated 25 · kept — flags are session demotions/coordinator splits already disposed; unstated edges were added by the coordinator at execution time and carry their reasons on the ledger's `Edge add:` lines
