C1 · r1 · flag-sweep · hg-blw.5 · kept-leaf — reviewer PROMOTE overruled: split into leaves hg-blw.5 (log), hg-blw.8 (ops filters), hg-blw.9 (roster last_change); §2.4 fully specifies each
C2 · r1 · GAP · R5 · applied — spec §2.3 widened (CLAUDE.local.md, .claude/CLAUDE.md, .claude/rules, user-level Claude/Codex files, origin field, imports not followed); hg-blw.2 updated
C3 · r1 · GAP · R12 · applied — Integration sweep hg-blw.11 asserts HEAD/index/work tree unchanged across every view; per-view acceptance lines added to hg-blw.1/.2/.3/.5/.6/.8
C4 · r1 · GAP · R4 · applied — hg-blw.2 now depends on hg-blw.5 and consumes HistoryWalker last-touch; hg-blw.4 names it; hg-blw.9 drops the stack switch and its hg-blw.2 edge
C5 · r1 · UNOWNED-SEAM · Inventory sessions history · applied — hg-blw.7 Inventory owns full sessions[], one transcript resolver and tombstone count
C6 · r1 · GAP · R10 · applied — hg-blw.3 acceptance: rows carry started/ended/end_reason and teamspace/seat/clone labels
C7 · r1 · UNOWNED-SEAM · roster.live RPC and CloneLive · applied — hg-blw.6 owns roster.live, CloneLive, threads_live, unmanaged
C8 · r1 · GAP · R1 · applied — hg-blw.1 acceptance: manager, is_node_manager, reload_required, runtime fields
C9 · r1 · GAP · R13 · applied — new leaf hg-blw.10 (README and CLI help)
C10 · r1 · UNOWNED-SEAM · ops.list request fields · applied — hg-blw.8 owns the extended request fields (daemon-side filter, backward compatible)
C11 · r1 · GAP · R6 · rejected — spec §2.3 Honest limits already makes unknown a real state; hg-blw.10 documents it
C12 · r1 · UNOWNED-SEAM · Roster reserved fields · applied — hg-blw.1 reserves instructions/last_change/live/threads_live/unmanaged slots
C13 · r1 · GAP · B3 · applied — hg-blw.5 acceptance tests omitted (instance) vs global scope and --help states it
C14 · r1 · GAP · R14 · applied — hg-blw.11 drives hook → current → edit → stale → log
C15 · r1 · sweep · hg-blw.11 · applied — root Integration sweep created over every leaf
