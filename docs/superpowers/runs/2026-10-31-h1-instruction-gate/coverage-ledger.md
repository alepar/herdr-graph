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
cl-15 · r2 · GAP · R15 · applied — root Integration sweep bead filed after the loop with the R15 matrix
cl-16 · r2 · GAP · R16 · applied — Integration sweep acceptance inventories every writer of instruction-source paths and states how each is gated; small gaps fixed inline, larger ones filed
cl-17 · r2 · GAP · out-of-apply commits · rejected (documented) — direct git commits to the graph repo are inside the accepted cooperative trust model (spec §3.5 residual vectors, updated); the writer builds commits from the base tree via git2 and never stages working-tree files (src/store/git.rs)
cl-18 · r2 · GAP · gate bypass · rejected (documented) — rewrapped invocations, socket calls, settings edits and prompt-skipping modes are accepted residual vectors (spec §3.5); hg-45e.11 must list them
cl-19 · r2 · GAP · R17 · applied — render already prints the plan hash above the effects (src/plan/commands.rs render); hg-45e.11 tells users to match the prompt hash to it
cl-20 · r2 · GAP · templates as sources · rejected — template instructions are edited only by plan template edit (an apply-committed plan kind; spec §2.1); seat seeding from templates is the application apply plan
cl-21 · r2 · GAP · R3 · rejected — hg-45e.9 description names templates/system-summarizer/members/summarizer/AGENTS.md (the inputs summarized it as "as listed")
cl-22 · r2 · GAP · R9 · rejected — hg-45e.10 owns the relay hint in src/cli/plan.rs with acceptance "relay hint text updated"
cl-23 · r2 · GAP · R18 · applied — hg-45e.7 adds the Codex fallback line in the seat skill (seat --instructions after clear/compaction) when Codex lacks the triggers
cl-24 · r2 · GAP · harness-native nested loading · rejected — the closed instruction set has no CLAUDE.md; Codex loads AGENTS.md only along the cwd path (external project repo), not from files the agent reads
cl-25 · r2 · GAP · e2e slice · applied — hg-45e.12 acceptance adds set -> apply -> hook render, working-tree edit excluded
cl-26 · r2 · GAP · R14 · applied — hg-45e.11 depends on hg-45e.5 and documents undo
cl-27 · r2 · UNOWNED-SEAM · instruction-vs-data predicate · applied — hg-45e.8 keeps only the path-shape assertion; the sweep asserts !is_instruction_source(summary path)
cl-28 · r2 · UNOWNED-SEAM · committed-revision reader · rejected — every reader goes through TreeRead at a CommitView (existing store abstraction), and InstructionSource maps paths at a revision; no new boundary
cl-29 · r2 · UNOWNED-SEAM · undo preview diff · applied — hg-45e.5 acceptance: undo plan prints the reverse diff via InstructionsKind delegation (already transitively after hg-45e.2)
cl-30 · r2 · UNOWNED-SEAM · truncation contract · applied — hg-45e.7 owns the cap and marker; seat --json/--instructions never truncate
cl-31 · r2 · UNOWNED-SEAM · bound_paths helper · applied — hg-45e.6 owns the helper shape; hg-45e.14 changes output keys only
