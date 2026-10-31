# Final review: cluster 4 (src-bootstrap), epic hg-45e

Invocation: inv-p3b-20261031T222244. Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify`). The review source binding succeeded (`bind-review-source` exit 0).

Files reviewed: src/bootstrap/content.rs (deleted), src/bootstrap/instructions.rs (new), src/bootstrap/mod.rs, src/bootstrap/setup_claude.rs, src/bootstrap/tests.rs (slices 000035-000039). Spec: docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md, sections 1.5, 3.1, 3.2, 3.4 (gate presence API), 4.1, 4.2 and 4.4 (fit and cap warning).

## Evidence gathered

- I created a review-owned detached worktree at head under .worktrees/review-c4-inv-p3b, seeded its target with a reflink copy and ran `cargo test --lib bootstrap::`. All 55 existing bootstrap tests passed.
- In that worktree only, I added one probe test for the setup_claude manifest-lost path, described below. It failed as predicted: `PROBE hook_removed=true owned_after=1`. I then reverted the probe and removed the worktree.
- Caller checkout after the review: HEAD bce6589afead4c031ac9ecfa3a094c9a5c4be161, `git status --short` empty. No caller files were edited.

## Spec conformance

- §1.5: the `content.write` IPC command, the `ContentWrite` mutation, `admit_write` and `register_mutations` are gone from bootstrap. content.rs is deleted and b64 moved to model/b64.rs. A test asserts that no command or mutation is registered. Conforms.
- §4.1 renderer: it reads the committed `TreeRead` only, and the order is global rules, teamspace rules, then per participation the seat template root, the team template root, and the member AGENTS.md plus responsibility. After that come seat sections, seat rules and seat AGENTS.md last. The revision line, the `## <label> — <path>` headers, empty-part omission and the §2.2a placeholder for name and content failures (including the responsibility and section bodies) all match the text. `bound_paths` and the renderer both consume `template_participation`, so the traversal cannot diverge. A dirty-working-tree test is present. Conforms.
- §4.2: `instructions {rev, parts[{label,path}], text}`, `paths.data` (seat folder minus `clones/` plus the bound clone folder, filtered through `classify`), `clone_record`, and null instructions for unbound or ambiguous replies are present. `render_seat` prints the block. Conforms.
- §4.4 `fit_hook`: it stops at the first overflow walking from the most specific part, and reserves the marker inside the cap. It cuts a single oversize part at a line boundary (and at a char boundary first), keeps the kept parts in render order, and ends with the marker naming omitted parts and `<label> (cut)`. The byte accounting checks out: head plus blocks equals `hook_size`, and both truncation paths fit within `cap`. `cap_warning` renders the patched tree exactly rather than estimating, which is stricter than the spec and acceptable. Conforms.
- §3.1/§3.2 ApprovalGate: Claude `permissions.ask` gets the bare entry plus an absolute-path entry. Entries are added idempotently, only added entries are recorded in the manifest, and uninstall removes only the recorded entries. The exact-restore path still applies. Codex gets an owned rules file: write, record, and remove on uninstall, removing the created `rules/` dir only when it is empty. The syntax was verified with `codex execpolicy check` (code comment). `gate_installed` and `owned_hook_installed` are exposed for doctor. Conforms.

## Findings

- (minor, confirmed regression) src/bootstrap/setup_claude.rs `install_for` handles first install with no manifest, an owned hook group already present (installed by hand, or the manifest was lost) and ask entries still missing. `settings_changed` becomes true through the gate, and the fresh manifest is built with `original: before` and `restorable: true`, where `before` already contains the owned hook. A later `uninstall` takes the exact-restore path, rewrites `original` and reports `hook_removed=true`, but the owned SessionStart hook is still in settings.json (the probe reproduced this). Before this diff, the hook-Unchanged first-install branch recorded `restorable: false`, and uninstall removed the owned group. The fix is to record `restorable: false` on first install whenever `owned_at` was non-empty. The same flaw already existed at base for the hook-Updated first-install case. Doctor would surface the leftover hook as a missing gate, so this does not weaken the approval guarantee.
- (minor, simplification) `PatchedTree` in src/bootstrap/instructions.rs re-implements `store::tree::Overlay` (an `EditSet` overlay over a `TreeRead`, which also hides directories emptied by deletes). The plan kinds could size hooks through an `Overlay` instead of a second overlay type. PatchedTree's `list_dir` keeps a directory whose files were all deleted. That is harmless for the renderer, but the two overlays already behave differently.
- (minor) `gate_installed` for RulesFile maps any read error, not just NotFound, to `false`. Install likewise treats any read error on the rules file as absent. A permission or is-a-directory error therefore shows up as "missing gate" or as a later write failure, rather than as the real cause.
- (minor) `is_apply_rule` strips only one pair of single quotes. For an absolute binary path containing a `'`, `sh_quote` produces `'..'\''..'`, which `is_apply_rule` does not parse back. Doctor would then report a missing gate even though setup installed one.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- setup_claude install with no manifest over a hand-installed owned hook, followed by uninstall (the probe shows the hook survives while `hook_removed=true` is reported)
- Upgrade path: re-running `setup claude`/`setup codex` over a manifest written by the previous release (no gate_entries/gate_file/created_rules_dir keys)
- Codex install over a pre-existing user-authored rules/herdr-graph.rules (overwritten as Updated, then deleted on uninstall)
- paths.data excluding dotfiles (rules/.gitkeep) and a sibling clone's files under the seat's clones/ folder
- fit_hook cut path when the single most specific part is one line longer than the budget (only the part heading survives)
- Inadmissible member AGENTS.md or responsibility rendering the placeholder (already noted as a task-7 deferred minor)

## Deferred OK
- Record `restorable: false` in install_for on first install when an owned hook group already existed (confirmed regression in a narrow manifest-lost path; doctor surfaces it)
- Replace PatchedTree with store::tree::Overlay to keep one overlay implementation
- Distinguish NotFound from other IO errors in gate_installed(RulesFile) and in the install-time read of the rules file
- Make is_apply_rule round-trip sh_quote output for paths containing single quotes
- RulesFile presence check is a substring match (already deferred from task 4)
- cap_warning joins omitted and cut clauses with "; " (already deferred from task 7)
- template_participation silently skips a template_ref whose record is unreadable but errors for an application's missing template record (asymmetry, unreachable in practice)
