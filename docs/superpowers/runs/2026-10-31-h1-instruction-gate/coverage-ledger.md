# coverage ledger — hg-45e
cl-1 · r1 · flag-sweep · hg-45e.6 · kept leaf — promotion-review PROMOTE overruled: split into hg-45e.6 (renderer) and hg-45e.14 (seat --json contract); spec §4 fully specifies both
cl-2 · r1 · flag-sweep · hg-45e.10 · kept leaf — promotion-review PROMOTE overruled: split into hg-45e.10 (setup gate) and hg-45e.15 (doctor); spec §3 fully specifies both
cl-3 · r1 · GAP · R15 · applied — root Integration sweep (after the loop) carries the R15 command matrix in its acceptance
cl-4 · r1 · GAP · R18 · applied — hg-45e.7 acceptance: Claude hook matcher covers all SessionStart sources (test); Codex coverage verified or documented
cl-5 · r1 · GAP · R17 · rejected — plans are content-addressed: apply requires --confirm <plan hash> (shown in the harness prompt) and recomputes effects (stale_plan); spec §2.4, §3.3
cl-6 · r1 · GAP · R19 · rejected — renderer and instructions show read the committed tree only (hg-45e.6/.13 acceptance: dirty working tree does not change output); graph folders are not the agent cwd (cwd = external project repo), so harnesses do not auto-load them
cl-7 · r1 · GAP · R16 · applied — Integration sweep acceptance: audit of every writer of InstructionSource paths (only plan kinds committed by apply)
cl-8 · r1 · GAP · undo bypass · rejected — UndoKind is an OrgKind; plan undo is committed only by apply (clarified in hg-45e.5)
cl-9 · r1 · GAP · legacy ContentWrite undo · rejected — content writes never wrote action records (Applied.action None); graph-change deserialization already in hg-45e.9
cl-10 · r1 · UNOWNED-SEAM · InstructionsEdit action record · applied — hg-45e.4 owns the record schema; hg-45e.5 consumes it
cl-11 · r1 · UNOWNED-SEAM · seat --json contract and hook header · applied — hg-45e.14 owns the seat --json contract, hg-45e.7 owns the header text
cl-12 · r1 · UNOWNED-SEAM · CLI surface for docs · applied — hg-45e.11 now depends on .4 .7 .8 .9 .10 .13 .14 .15 and its acceptance lists every surface
cl-13 · r1 · UNOWNED-SEAM · summary path vs instruction set · applied — hg-45e.8 acceptance asserts the summaries/ path; sweep asserts disjointness via is_instruction_source (no edge, by construction)
cl-14 · r1 · UNOWNED-SEAM · affected-seats resolution · rejected — owned by InstructionSource (hg-45e.3 description and digest)
