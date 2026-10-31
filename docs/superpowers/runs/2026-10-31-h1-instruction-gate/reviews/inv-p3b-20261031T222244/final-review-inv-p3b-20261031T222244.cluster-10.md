# Final review: cluster 10 (src-store), epic hg-45e

Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify`). Review-source binding for inv-p3b-20261031T222244 succeeded (exit 0) before this artifact was written.

Files in scope: src/store/instruction_source.rs (new), src/store/layout.rs (RECORD_FILES, graph_id, locate for IdKind::Graph), src/store/mod.rs (module export), src/store/tests.rs (graph_id and instruction_source test modules). Slices 000067-000070.

Spec checked against: docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md, sections 2.1, 2.2a, 2.3, 4.2 and the InstructionSource key abstraction.

## Evidence

- Built a review-owned detached worktree at the head commit (same filesystem as the caller, target seeded by reflink copy, submodule initialised from the local checkout), ran `cargo test --offline --lib store::tests`: 46 passed, 0 failed. Worktree removed afterwards.
- Caller checkout observed after the review: HEAD still bce6589afead4c031ac9ecfa3a094c9a5c4be161, `git status --short` empty.
- A first attempt to create the worktree in the session scratchpad failed on the reflink copy (`Invalid cross-device link`, /tmp is a separate filesystem); that worktree was removed before retrying under .worktrees/.

## Spec conformance

- 2.1 scopes and closed file sets: `InstructionScope {Global, Teamspace, Seat}` and `InstructionFile {Rule, AgentsMd}` match. `InstructionFile::parse` allows AGENTS.md only for Seat, and rule names follow `[a-z0-9][a-z0-9_-]*` directly under `rules/`, with no nesting. `..`, uppercase, `.txt`, empty name and dot names are rejected (tested).
- 2.1 retired rejection: `resolve_scope` returns `Retired` (displayed with the `retired_object:` prefix) for retired seats and teamspaces, by name and by id. When a name has one live match and some retired matches, the live one wins. Several live matches give `Ambiguous` listing the ids.
- 2.2a name clause: `check_name` is the one name checker and `list_files` attaches its result to every listed file. Content clause: `check_content` matches the spec exactly. It rejects invalid UTF-8 and Cc controls other than `\t`, `\n` and CRLF, reports a bare CR, and rejects every listed invisible/format range. Each range is tested at its endpoints, plus the code points just outside them. The reason text gives the code point and the 1-based line.
- 2.2a dotfiles: they are ignored both in `classify` and in `walk`, and the fresh-instance `rules/.gitkeep` is neither listed nor data (tested).
- 2.2a "anything under rules/ or a scope AGENTS.md is never data": holds for Root/Teamspace/Seat folders, including nested and badly named rule files. Clone-folder `AGENTS.md`/`rules/` are data by design (tested).
- 2.3 GraphId: `layout::graph_id` derives the id from graph.toml and maps a non-conforming `instance_id` to `StoreError::Corrupt` naming graph.toml. `locate` of the instance's own `gr_` id resolves to the root folder with record graph.toml. Another instance's id returns None, and a corrupt instance id makes `locate` error (all tested, including through `GitStore::locate`'s cache).
- 4.2 shared record list: `layout::RECORD_FILES` is the single list, and its only consumer is `classify`. No duplicate list elsewhere in src.
- Key abstraction "Must not": the module builds no plans or diffs and never orders the render. It takes no caller-supplied path, because paths come only from `scope_folder` and `InstructionFile::rel`. It depends only on layout, tree, ids and ports.

## Observations (non-blocking)

- `classify` checks RECORD_FILES before the instruction-location check, so `rules/seat.toml` classifies as Record rather than Instruction. Every production caller only tests `== Data`, so nothing changes in practice. `list_files` still lists it and `check_name` rejects it, so the renderer and doctor treat it as inadmissible. This was already deferred in task 6.
- Record classification matches on the basename at any depth: `notes/seat.toml` in a seat folder is Record and is left out of `paths.data`. That is the safe direction, since nothing is promoted to an instruction, but it is not stated in the spec.
- `walk` skips dot-named files by basename but descends into dot-directories. `rules/.d/x.md` is listed, gets an inadmissible name and becomes a placeholder plus a doctor warning, and `classify` agrees (Instruction). The two predicates stay consistent; this was already deferred in task 6.
- `walk` pushes Symlink entries and gitlinks (EntryKind::File) as files. A symlink under `rules/` with a valid name would render its link-target text as rule content. Reaching that state takes a direct commit (an accepted residual vector, spec 3.5).
- `check_name` has a branch for a non-Seat `AGENTS.md`, but `list_files` never feeds it one, because only the Seat scope probes AGENTS.md. Meanwhile `classify(Teamspace|Root, "AGENTS.md")` returns Instruction. So a teamspace or root AGENTS.md is neither rendered, nor warned by doctor, nor data. Before this epic `/seat` never handed out a teamspace AGENTS.md, so this is no regression. `FolderKind::Root` and `FolderKind::Teamspace` are only used by tests.
- `resolve_scope` reimplements name/id resolution instead of reusing `plan::grammar::resolve_*`. The spec says InstructionSource depends only on layout and ids, so this was intended. The Ambiguous message lacks grammar's "use an id" hint and the teamspace context.
- `impl PartialEq/Eq/Clone for StoreError` lives in this module and compares by Display text. Adding a StoreError variant breaks compilation here, which is a safe failure, but the impls belong in src/ports/store.rs (already deferred in task 6).
- Name resolution accepts a seat that is live inside a retired teamspace, while Global `affected_seats` excludes such seats. `teamspace retire` cascades to its seats (src/plan/core_kinds.rs), so the state cannot be reached through plans.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- walk/list_files with a dot-directory under rules/ (e.g. rules/.d/x.md) and with Symlink or gitlink entries under rules/
- list_files on a scope folder that has no rules/ directory (teamspace or seat without rules/), which relies on list_dir returning empty for a missing path
- classify precedence for a record-named file under rules/ (rules/seat.toml yields Record, not Instruction)
- resolve_scope picking the single live match when a retired object shares the name
- affected_seats tie-break ordering by id when two affected seats share a name
- layout::locate of a gr_ id when graph.toml is absent (returns None)
- parse_words with a missing teamspace/seat target followed by a file word (e.g. "teamspace rules/x.md" reports "a file is required" instead of a missing target)

## Deferred OK
- classify checks RECORD_FILES before the rules/ and AGENTS.md instruction locations; no production caller distinguishes Record from Instruction
- dot-directories under rules/ are descended and listed as inadmissible names, not ignored
- manual PartialEq/Eq/Clone impls for StoreError sit in instruction_source.rs rather than src/ports/store.rs
- FolderKind::Root and FolderKind::Teamspace and the non-Seat AGENTS.md branch of check_name are unused in production paths
- resolve_scope duplicates plan::grammar name resolution, and its Ambiguous message lacks the "use an id" hint
- basename-level record matching hides nested files named like records (e.g. notes/seat.toml) from paths.data
