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
C16 · r2 · UNOWNED-SEAM · stack clones[].loaded slot · applied — hg-blw.2 owns InstructionsStack view struct with reserved slot; hg-blw.4 consumes it
C17 · r2 · GAP · B2 · applied — hg-blw.4: roster JSON instructions carries loaded part labels, text shows count
C18 · r2 · GAP · R5 per-clone cwd · rejected — spec §1.2 and src/model/launch.rs resolve_cwd: cwd is per seat (override, project_repo, .graph-local/cwd/<seat>); clones share it
C19 · r2 · GAP · R12 loaded write · applied — hg-blw.4 acceptance: git status --porcelain empty after injection
C20 · r2 · GAP · R14 · applied — hg-blw.11 asserts current and stale in both roster --json and instructions stack --json
C21 · r2 · UNOWNED-SEAM · occupant/runtime source · applied — hg-blw.7 Inventory owns runtime binding and current occupant; hg-blw.1 acceptance uses them
C22 · r2 · UNOWNED-SEAM · ops requester resolution · applied — hg-blw.8 owns ids-in-request, daemon-side match incl. retired seats
C23 · r2 · GAP · R13 docs timing · applied — hg-blw.10 now depends on hg-blw.1–.6, .8, .9
C24 · r2 · GAP · B5 text output · applied — hg-blw.4 acceptance: text shows status and stale parts
C25 · r2 · UNOWNED-SEAM · occupant.since · applied — hg-blw.4 consumes Inventory current occupant/since (upstream via hg-blw.1)
C26 · r2 · GAP · R5 inspector env · applied — spec §2.3 and hg-blw.2: user-level entries marked env: inspector (graph never reads agent env back)
C27 · r2 · GAP · R5 per-harness · applied — spec §2.3 and hg-blw.2: harness claude|codex per file, own harness marked
C28 · r2 · UNOWNED-SEAM · instructions stack registration · applied — hg-blw.2 owns it, every_spec_command_parses
C29 · r2 · GAP · B3 latest per scope · applied — hg-blw.10 documents log <scope> --limit 1
C30 · r2 · GAP · B3 non-committed activity · rejected — spec §2.1/§2.4/§2.6: log is committed history, journal via ops, sessions separate; §3 rejects heartbeats; hg-blw.10 documents it
C31 · r2 · GAP · R8 ops head · applied — hg-blw.8: envelope head is the daemon reply's head
