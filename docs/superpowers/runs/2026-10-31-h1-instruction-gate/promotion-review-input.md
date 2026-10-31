### hg-45e.12 — Configuration smoke: three instruction scopes and two harness gates
Spec §6 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). One integration test (tests/instruction_smoke.rs or in tests/integration_sweep.rs default tier) that, on an isolated instance, runs for each scope (global, teamspace, seat): plan instructions set -> apply (relay) -> seat --instructions (or render_instructions) shows the new text in the right section; and for each harness (claude, codex) in isolated config dirs (tests/support/isolated.rs pattern): setup -> doctor harness_gate ok -> uninstall -> doctor skips. consumes: InstructionsKind, seat --instructions, ApprovalGate. Files: tests/instruction_smoke.rs (new), tests/support/*.
blocked-by hg-45e.4: consumes InstructionsKind
blocked-by hg-45e.7: consumes seat --instructions
blocked-by hg-45e.10: consumes ApprovalGate
Acceptance:
- [ ] each of the three scopes is exercised end to end and asserted
- [ ] each harness gate install/doctor/uninstall cycle is exercised and asserted
- [ ] runs in the default cargo test tier (no private-herdr feature)
- [ ] cargo test green
Deps: hg-45e.7, hg-45e.10, hg-45e.4

### hg-45e.11 — Docs and graph skill for summaries, instruction plans and the apply gate
Spec §3.3, §3.5, §5 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). Update skills/graph/SKILL.md: description line (no 'write seat content'), replace 'Writing seat content' with 'Summaries and instructions' (request complete --summary-from; plan instructions set|clear + instructions show; template edit and seat override --section remain for template/section instructions), confirmation copy per §3.3 and the cooperative-strength note per §3.5; update the graph-skill needle test in src/bootstrap/tests.rs (~1058: replace 'content write' with the new needles). Update CURRENT-DESIGN.md (Bootstrap + Templates bullets: verbatim committed render, instruction plans, apply ask rule; supersede 'does not synthesize prose'), GRAPH_STATE.md:63, CREATING_NEW_GRAPH.md:70, README.md (:52 and the command list), CHANGELOG.md (content write removal, new commands, migration note to update the instance summarizer template via plan template edit). No edits to docs/superpowers/runs/2026-10-02-*, docs/releases/*. Files: skills/graph/SKILL.md, src/bootstrap/tests.rs (needle test only), CURRENT-DESIGN.md, GRAPH_STATE.md, CREATING_NEW_GRAPH.md, README.md, CHANGELOG.md.
Acceptance:
- [ ] graph skill describes summaries, instruction plans, the apply ask rule and its cooperative strength; no 'content write' instructions remain in it
- [ ] skill needle test updated and passing
- [ ] CURRENT-DESIGN, GRAPH_STATE, CREATING_NEW_GRAPH, README and CHANGELOG reflect the new commands and the removal
- [ ] cargo test green
Deps: 

### hg-45e.10 — ApprovalGate: harness ask rule on apply in setup and doctor
Spec §3.1-3.4 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). New closed enum ApprovalGate on setup_claude::Target: SettingsAsk (Claude: add 'Bash(herdr-graph apply:*)' plus 'Bash(<absolute bin> apply:*)' when the hook command uses an absolute binary path to settings.json permissions.ask) and RulesFile (Codex: owned $CODEX_HOME/rules/herdr-graph.rules with prefix_rule(pattern=["herdr-graph", "apply"], decision="prompt") plus the absolute-path variant). Manifest records exactly the entries/files added; install idempotent and never duplicates an existing entry or claims it; uninstall removes only recorded entries (existing restore-original-bytes path still applies). First, read-only: check the installed Codex (codex --help / docs if present locally) for the rules-file syntax; if it cannot be confirmed, keep the spec syntax, and make 'setup codex' print a note that the user must confirm the rule in Codex. doctor (src/daemon/doctor.rs): harness_gate checks per harness with the owned SessionStart hook installed; missing rule -> WARN 'run herdr-graph setup claude|codex'; harnesses without the owned hook skipped. Relay hint in src/cli/plan.rs:89 changes to 'show this plan to the user, then run herdr-graph apply <pl> --confirm <hash> --confirmed-by user-relay; the harness asks the user to approve it'. owns: ApprovalGate. Files: src/bootstrap/setup_claude.rs, src/cli/setup.rs, src/cli/plan.rs, src/daemon/doctor.rs, src/bootstrap/tests.rs (setup tests), src/daemon/ tests.
Acceptance:
- [ ] setup claude adds the ask entries; second run Unchanged; pre-existing identical entry neither duplicated nor removed by uninstall
- [ ] uninstall claude removes only recorded entries (and restores original bytes when the sha matches)
- [ ] setup codex writes the owned rules file; uninstall removes it
- [ ] doctor WARNs when the owned hook is installed but the gate is missing; ok when present; skipped when no owned hook (tests with isolated config dirs)
- [ ] relay hint text updated
- [ ] cargo test green
Deps: 

### hg-45e.9 — Remove content write
Spec §1.5 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). Delete the content.write daemon command registration (src/bootstrap/mod.rs), the ContentWrite mutation, admit_write and src/bootstrap/content.rs (b64 already moved by the summary-API bead), and the content_write mutation registration. Keep RequestKind::ContentWrite and Category::Content variants (legacy, deserialization only; doc comment saying so). CLI 'content write' becomes a stub with the same arguments that exits 2 printing the guidance text from §1.5. Update templates/system-summarizer/members/summarizer/AGENTS.md to 'request complete <rq> --summary-from <tmp file> --covered <start>-<end>' (drop the separate content write step), src/threads/effects.rs:572 message text, and every test using content write: src/bootstrap/tests.rs (content-write tests replaced by stub/legacy tests; summarizer template needle at ~1362), tests/daemon_composed.rs, tests/integration_sweep.rs golden step (~561-593, use --summary-from; keep the archived-folder and folder-move assertions) and cli_sweep argv (~1909), src/cli/mod.rs argv tests, src/writer/tests.rs, src/model/tests.rs, src/model/common.rs doc comment. consumes: request complete --summary-from (summary API). Files: as listed.
blocked-by hg-45e.8: consumes request complete --summary-from
Acceptance:
- [ ] herdr-graph content write ... exits 2 with the guidance text (test)
- [ ] no content.write daemon command and no content_write mutation are registered; a journaled legacy ContentWrite op fails through the writer's 'no mutation registered' path (test)
- [ ] legacy ContentWrite journal/graph-change records still deserialize (test)
- [ ] summarizer template uses --summary-from; golden sweep step updated and compiles under --features private-herdr
- [ ] repo search for 'content write', 'content.write', 'content_write', 'ContentWrite' and 'admit_write' (source and non-source, excluding docs/superpowers/runs/2026-10-02-*, docs/releases/*, docs/reviews/* and this run's directory) shows only the stub, the legacy variants, the docs bead's files, and CHANGELOG
- [ ] cargo test; cargo test --features test-support; cargo check --locked --offline --all-targets --all-features green
Deps: hg-45e.8

### hg-45e.8 — Summary API: request complete --summary-from
Spec §1.1-1.4 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). Move b64_encode/b64_decode from src/bootstrap/content.rs into a shared module (e.g. src/model/b64.rs) and re-point content.rs's uses. CLI request complete gains --summary-from <file> (mutually exclusive with --output), sending summary_b64 over request.complete. Daemon handler passes it through. bookkeeping.request_complete with summary: at apply resolve request -> transcript -> seat -> cx.tree.locate (live or archived), write <seat folder>/summaries/<transcript-id>-<start>-<end>.md, set output_ref to summaries/<...>.md; identical retry (completed, same range, same bytes) is a no-op success; different bytes or a completed request with a different range reject summary_conflict; non-UTF-8 rejected; unresolved still rejects. --output path unchanged. owns: (extends RequestComplete mutation). Files: src/model/b64.rs (new), src/model/mod.rs, src/bootstrap/content.rs (import only), src/cli/request.rs, src/transcripts/mod.rs, src/transcripts/mutations.rs, src/transcripts/tests.rs.
Acceptance:
- [ ] request complete --summary-from writes the summary into a live seat's folder and an archived (retired) seat's folder and completes the request in one commit
- [ ] identical retry is a no-op; different bytes -> summary_conflict; different range on a completed request -> summary_conflict
- [ ] --output behavior unchanged; --summary-from and --output together rejected by the CLI
- [ ] b64 helpers live in the shared module and content.rs still compiles against them
- [ ] tests in src/transcripts/tests.rs; cargo test green
Deps: 

### hg-45e.7 — SessionStart hook injects instructions; seat --instructions
Spec §4.3, §4.4 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). In src/cli/seat.rs: new 'seat --instructions' flag printing only the render, daemon-less (open committed head, resolve_caller(view, caller, None)), exit 1 with 'not a bound graph seat' when unresolved. --hook-prompt [--codex]: when the pane resolves Bound (same rule as wants_seat_prompt), output the header line from §4.4 plus the render text, capped at HOOK_RENDER_CAP (default 9000 bytes) at a line boundary with '[truncated: run herdr-graph seat --instructions for the full text]'; otherwise today's prompt; any error falls back to today's prompt; Codex keeps the additionalContext JSON shape. First, read-only: check the installed Claude Code documentation/help for a documented hook output size limit; if found, set HOOK_RENDER_CAP below it and note the source in a code comment; if not found, keep 9000 and say so in the comment. consumes: InstructionRenderer. Files: src/cli/seat.rs, src/bootstrap/mod.rs (wants_seat_prompt helper if shared), src/bootstrap/tests.rs or tests/.
blocked-by hg-45e.6: consumes InstructionRenderer
Acceptance:
- [ ] bound pane: hook output (Claude text and Codex JSON) = header + render; unbound: today's prompt unchanged
- [ ] output over the cap is cut at a line boundary and ends with the truncation marker (test with a large fixture)
- [ ] a store error or unreadable instance yields today's prompt, never a non-zero exit
- [ ] seat --instructions prints the render without a daemon and exits 1 when unbound
- [ ] cargo test green
Deps: hg-45e.6

### hg-45e.6 — InstructionRenderer and seat --json instructions/data contract
Spec §4.1, §4.2, §4.5 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). Extract TemplateParticipation from bound_paths (ordered, de-duplicated participating templates with optional member + responsibility); bound_paths and the renderer both use it. New src/bootstrap/instructions.rs render_instructions(tree, at, seat) -> RenderedInstructions {rev, parts[{label, path, body}], text} reading only the committed tree, in the six-step order (global rules, teamspace rules, per template: shared seat template AGENTS.md, team template root AGENTS.md, member AGENTS.md + 'responsibility: <text>'; seat instructions_sections; seat rules; seat AGENTS.md), leading revision line, '## <label> — <repo-relative path>' headers, empty parts omitted, non-UTF-8 placeholder. seat_reply adds instructions {rev, parts[{label,path}], text} (null when not bound); paths.clone_files replaced by paths.data (files under seat and clone folders that are neither instruction sources nor record files, via InstructionSource) plus paths.clone_record; render_seat prints instructions.text. skills/seat/SKILL.md step 3 and step 4 per §4.5 (minimal edit). owns: InstructionRenderer, TemplateParticipation. consumes: InstructionSource. Files: src/bootstrap/instructions.rs, src/bootstrap/mod.rs, src/bootstrap/tests.rs, skills/seat/SKILL.md.
blocked-by hg-45e.3: consumes InstructionSource
Acceptance:
- [ ] render order and headers match spec §4.1 on a fixture with all six part kinds; empty parts omitted
- [ ] a dirty working-tree instruction file does not change the render (test)
- [ ] seat --json carries instructions and paths.data/clone_record; clone_files gone; notes/n.md appears under data, clone.toml does not
- [ ] bound_paths and the renderer share TemplateParticipation (no second template walk)
- [ ] seat skill text updated per §4.5; existing setup skill-content tests still pass
- [ ] cargo test green
Deps: hg-45e.3

### hg-45e.5 — Undo for instruction edits
Spec §2.5 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). Extend UndoKind: InstructionsEdit added to is_undoable; UndoKind holds an instructions Arc<dyn OrgKind>; instructions_inverse mirrors template_inverse: if the current file equals the action's after, the inverse is set(before) or clear when before was absent, delegated to InstructionsKind plan/mutate; if drifted, repair_required conflict. finalize stamps undone_by as for other kinds. consumes: InstructionsKind. Files: src/undo/compensate.rs, src/undo/candidates.rs, src/undo/preview.rs, src/undo/mod.rs, src/undo/tests.rs.
blocked-by hg-45e.4: consumes InstructionsKind
Acceptance:
- [ ] plan undo <act> for an instruction edit restores the previous text (and removes a file that did not exist before)
- [ ] a file changed after the edit yields a repair_required conflict instead of an undo
- [ ] the undo action stamps undone_by; the edit no longer lists as undoable afterwards
- [ ] tests in src/undo/tests.rs; cargo test green
Deps: hg-45e.4

### hg-45e.4 — InstructionsKind: plan instructions set|clear and instructions show
Spec §2.2, §2.4 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). New OrgKind (src/plan/instructions.rs) with RequestKind::InstructionsEdit and ActionKind::InstructionsEdit, verbs (instructions,set) and (instructions,clear); parse '<scope> [<target>] <file> [--from <path>]' via InstructionSource (--from relative to caller cwd); plan: UTF-8 check, 'no change' for identical bytes, 'no such instruction file' for clear of absent; effect instructions.set|instructions.clear on the target object with detail {file, before_sha, after_sha, diff} (diff from LineDiff); warning 'takes effect at each affected seat's next /seat (start, clear, compaction): <seat names>'; mutate writes/deletes the file and writes an InstructionsEdit action record (write_action) with compensation {file, before, after} (full text or absent). Staleness is left to OrgMutation same_effects. Register in daemon compose and test fixtures' kind registries. Read-only CLI 'herdr-graph instructions show <scope> [<target>] [<file>]' from the committed head without a daemon. owns: InstructionsKind. consumes: InstructionSource, LineDiff. Files: src/plan/instructions.rs, src/plan/mod.rs, src/model/change.rs, src/model/action.rs, src/daemon/compose.rs, src/cli/instructions.rs, src/cli/mod.rs, src/plan/tests.rs.
blocked-by hg-45e.3: consumes InstructionSource
blocked-by hg-45e.2: consumes LineDiff
Acceptance:
- [ ] plan instructions set/clear produce plans with the effect detail, rendered diff and affected-seats warning for global, teamspace and seat scopes
- [ ] apply commits the file change and an InstructionsEdit action record with before/after
- [ ] editing the file after planning makes apply return stale_plan
- [ ] no change / no such instruction file / non-UTF-8 / retired target are rejected at plan time
- [ ] instructions show prints the committed file(s) without a daemon
- [ ] tests cover the above; cargo test green
Deps: hg-45e.2, hg-45e.3

### hg-45e.3 — InstructionSource: instruction scopes, files and paths
Spec §2.1 and Key abstractions (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). New module (src/store/instruction_source.rs or src/model/): closed enums InstructionScope {Global, Teamspace(TeamspaceId), Seat(SeatId)} and InstructionFile {Rule(name), AgentsMd}; parse '<scope> [<target>] <file>' (target by name or id, resolved at a revision); name rule [a-z0-9][a-z0-9_-]*; allowed files per scope (global: rules/<n>.md; teamspace: rules/<n>.md; seat: AGENTS.md and rules/<n>.md); repo path for (scope,file); effect object id (GraphId for Global); affected live seats (global: all; teamspace: its; seat: itself); retired targets rejected; predicate is_instruction_source(path) over seat/clone folders and the shared record-file list (moved from bootstrap/content.rs RECORD_FILES into layout, without editing content.rs, which another bead deletes). Never accepts a caller path. owns: InstructionSource. consumes: GraphId. Files: src/store/instruction_source.rs, src/store/mod.rs, src/store/layout.rs.
blocked-by hg-45e.1: consumes GraphId
Acceptance:
- [ ] enums, parser and path mapping implemented; invalid names, disallowed files (e.g. global AGENTS.md), unknown targets and retired targets produce distinct errors (unit tests)
- [ ] affected-seat sets correct for each scope (unit tests on a fixture graph)
- [ ] is_instruction_source classifies seat AGENTS.md, seat rules/*.md, record files and other files correctly
- [ ] record-file list lives in layout (or this module) as the single list
- [ ] cargo test green
Deps: hg-45e.1

### hg-45e.2 — LineDiff and plan diff rendering
Spec §2.4 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). New small module (src/plan/line_diff.rs): unified line diff via LCS with 3 lines of context, no new crate. plan::commands::render prints an effect's detail.diff (when present) as an indented block after that effect's line instead of inlining it in the JSON detail. owns: LineDiff. Files: src/plan/line_diff.rs, src/plan/mod.rs, src/plan/commands.rs, src/plan/types.rs (describe).
Acceptance:
- [ ] line_diff produces unified-diff hunks (@@ headers, +/-/space lines, 3 context lines) for add/remove/change/empty-before/empty-after cases (unit tests)
- [ ] render() shows the diff block indented under the effect and omits the diff from the inline detail; effects without diff render exactly as before (existing render tests unchanged)
- [ ] no new dependency in Cargo.toml
- [ ] cargo test green
Deps: 

### hg-45e.1 — Seam contract: GraphId (IdKind::Graph)
Spec §2.3 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md). Add IdKind::Graph with prefix gr_ and one deterministic id per instance derived from graph.toml instance_id (must round-trip AnyId::parse; an instance_id that does not fit is reported as corrupt). layout::locate(gr_ id) resolves to the instance root folder with record_path graph.toml. owns: GraphId. Files: src/model/ids.rs, src/model/graph.rs, src/store/layout.rs.
Acceptance:
- [ ] IdKind::Graph exists with prefix gr_ and is in IdKind::ALL; no prefix is a prefix of another
- [ ] a helper returns the instance GraphId from graph.toml; round-trips through AnyId::parse
- [ ] layout::locate of the GraphId returns folder = repo root, record_path = graph.toml
- [ ] unit tests cover derivation, parse round-trip, locate, and the corrupt case
- [ ] cargo test green
Deps: 

