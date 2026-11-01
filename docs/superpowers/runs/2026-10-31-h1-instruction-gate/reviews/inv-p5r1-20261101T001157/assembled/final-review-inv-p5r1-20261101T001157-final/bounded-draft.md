# Final review (bounded): epic hg-45e, branch super-auto/h1-instruction-gate

Invocation: inv-p5r1-20261101T001157. Captured source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head c2a9a306f16f5f952d7a88313873866a14b6e85a. I verified both with `git rev-parse --verify <sha>^{commit}`. The caller checkout was clean and at c2a9a306 when the review started. I did not switch, restore or modify it. `bind-review-source` succeeded (exit 0) before any artifact write.

## Scope of this review

This is a bounded captured-source review, not a whole-range review. The review-inputs sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, 217 selected paths, 0 omitted and base/head match. It retains four gaps, all unresolved and unreviewed here:

- run.md:78 inv-p3b-20261031T222244: incomplete claimed collection
- run.md:83 inv-p3c-20261031T232503: incomplete claimed collection
- run.md:83 inv-p3c-20261031T232503: invalid failed assembly archive (bounded report identity or verdict mismatch)
- run.md:83 inv-p3c-20261031T232503: incomplete assembled bundle: digest.md

I read the full changed-path inventory (217 paths, 34183 insertions, 954 deletions). I did not build or load a whole-range package. From captured Git I inspected every hunk of 32 product, doc and template paths, 110 hunks in all, listed in bounded-coverage.json. They cover the spec's six key abstractions and every commit and approval path the epic changes: InstructionSource, InstructionsKind, the renderer and hook fit, TemplateParticipation, ApprovalGate/setup/doctor, the summary mutation, undo, the `[y/n]` marker guard, ids/layout and the content-write stub.

I also read some regions without inspecting their hunks, so those paths are not claimed as covered: the non-test half of src/cli/instructions.rs, the first 110 lines of src/plan/line_diff.rs, parts of src/plan/commands.rs and src/writer/mod.rs at head, the p3c bounded-draft.md and run.md lines 1-40 and 75-90.

No full-suite measurement of head c2a9a306 exists. The caller runs that sweep after this invocation, so the branch is untested at head.

## Findings (own view, against the spec)

- Gate integrity holds in what I inspected. `plan.create` builds args only through `OrgKind::parse` (src/plan/commands.rs `create_plan`), so the check-skipping `content_b64` field of InstructionsKind args can only come from the undo inverse (src/undo/preview.rs `instructions_inverse`). The deferred [check-bypass-field] concern is therefore bounded. The renderer still replaces any inadmissible restored bytes with the placeholder.
- The only `ConfirmMode::Tty` commit paths are `plan_flow` (shared by `rebind`, src/cli/ops.rs:52) and interactive `undo`. Both go through `prompt_terminal`, which honours the four agent markers. No other client sends `plan.apply` with mode tty. The daemon still trusts the client-reported `caller.tty`, which is an accepted residual (§3.5).
- The writer never consults the instructions plan's `relied_on` blob precondition: `admit_apply` sends `relied_on: vec![]`, and staleness rides `same_effects` as the spec intends. The Blob `ReliedOn` the kind builds only feeds the plan hash. This is correct, but `writer::current_version` resolves Blob versions through `req.args["path"]`, which instruction args do not carry. If anyone later threads plan `relied_on` into the request, every instruction plan would fail `precondition_failed`. Deferred, as a latent seam.
- Codex compaction coverage is a spec gap. The goal and AC6 require the inline render at session start, clear and compaction. src/cli/seat.rs records that Codex 0.163.0 has no `SessionStart` source for compaction, only a separate `PostCompact` event. The epic covers this with one line in skills/seat/SKILL.md telling Codex agents to run `herdr-graph seat --instructions` after compaction. That relies on the agent obeying a prompt, which is exactly what the spec set out to stop depending on (Main challenges, §4.4). Either setup codex installs a PostCompact hook (if Codex accepts context from it), or the human accepts the gap explicitly. This sits next to the two still-parked Codex escalations (run.md:18-19): whether a malformed `.rules` file aborts Codex startup, and whether Codex's ~2,500-token `additionalContext` limit truncates a 9000-byte HOOK_RENDER_CAP render plus its JSON envelope. Neither has a human disposition at head.
- The prior bounded review's data-loss must-fix is resolved. `install_for` now bails when `$CODEX_HOME/rules/herdr-graph.rules` exists with different content and the manifest does not mark it as ours (src/bootstrap/setup_claude.rs, RulesFile arm). A byte-identical pre-existing file is not recorded as `gate_file`, so uninstall leaves it.
- Hook binding rule divergence (minor). §4.4 says the hook renders "when the pane resolves Bound (same rule as `wants_seat_prompt`)". `wants_seat_prompt` requires `via == "binding"`, but `hook_text` renders for any `resolve_caller` Bound result, env-resolved `HERDR_GRAPH_SEAT`/`HERDR_GRAPH_CLONE` included. That is plausibly intended for graph-launched panes, but a nested session that inherits those variables gets that seat's instructions. Deferred.
- `fit_hook` overshoots in a pathological case (minor). In the single-part cut path, `budget = cap.saturating_sub(head + marker)`. When the marker naming every omitted part is longer than the cap allows (hundreds of parts), the output exceeds HOOK_RENDER_CAP. Not reachable with realistic instruction stacks. Deferred.
- Dot-directories inside `rules/` are walked (src/store/instruction_source.rs `walk` skips dotfiles by basename only). A file under `rules/.x/` is listed, fails the name clause and renders as a placeholder with a doctor WARN. §2.2a says dotfiles are ignored entirely. This matches the deferred [dotdir-handling] finding. It is harmless (fail-closed) but deviates from the spec. Deferred.
- The worktree_dirty notification (src/threads/effects.rs:572) now tells users to commit any locally edited file through `plan instructions set`. For data files (summaries, clone notes) no commit path exists any more, since `content write` was removed and the spec deliberately added no replacement for clone data. The message misleads for those paths. Deferred, as a wording follow-up.
- Existing instances with a seeded seat `AGENTS.md` copy now render member instructions twice: live at step 3 and as the seat copy at step 6. That doubles the bytes and can push hooks over the cap until the CHANGELOG migration (`plan instructions clear seat <name> AGENTS.md`) is run. It is documented. Deferred.
- The summary path and conflicts match §1.2-1.3. The file is derived at apply time from request → transcript → seat → locate, never overwritten, and `summary_conflict` fires on a different range or different bytes. One edge: two distinct requests for the same transcript and range, where the second has identical bytes but is not yet completed, are rejected `summary_conflict` rather than a no-op. Deferred.

## Ledger triage

- Recurring minor ×3 [missing-evidence] (hg-45e.8, hg-45e.3, hg-45e.4, process class): deferred, along with the other evidence-only minors ([missing-evidence] on hg-45e.4/.11/.16, [no-red-run], [tdd-red-skipped], [thin-evidence], [suite-not-run] on hg-45e.14/.41, [committed-evidence-file]).
- [lossy-undo-gap] (hg-45e.37): a template-edit undo restores a non-UTF-8 member AGENTS.md from the recorded document text, so the bytes come back lossy. This is a data-loss path on undo, so I regrade it to must-fix, or to explicit human acceptance. Impact is bounded: such a file is inadmissible and already renders as a placeholder, and git history keeps the original bytes.
- [unreproduced-run-truncation] (hg-45e.16): the first `--features test-support` run ended mid daemon_ipc with exit 101 and no failure text, and the rerun passed. This is possibly a flaky test or product race. It goes to untested scope, and the caller's full sweep must show it does not recur.
- [overwrites-preexisting-file] (hg-45e.10): resolved in captured source (install now refuses foreign content).
- [check-bypass-field] (hg-45e.37): verified bounded (see Findings). Deferred.
- [daemon-not-enforcing] (hg-45e.10): accepted residual (§3.5). Deferred.
- [repeat-complete-not-guarded] (hg-45e.8): no data loss, since output_ref was bookkeeping only. Deferred.
- [spec-deviation-documented] (hg-45e.41): an identical summary retry creates an audit-only commit (mutations/ touched). This matches §1.3's "no graph change" meaning. Deferred.
- No `parked` completion, `parked finding`, `Recurring blocker:` or BLOCKED-AUTH lines appear in the ledger. Metrics are UNAVAILABLE (the Finish ledger re-read was truncated), so missing counts are not evidence of a clean run.
- Every other deferred minor is listed under Deferred OK.

## Unreviewed

- All four provenance gaps listed under Scope (inv-p3b and inv-p3c collection/assembly gaps at run.md:78 and run.md:83).
- Test sources, not inspected: src/bootstrap/tests.rs, src/plan/tests.rs, src/store/tests.rs, src/templates/tests.rs, src/transcripts/tests.rs, src/undo/tests.rs, src/writer/tests.rs, src/model/tests.rs, tests/daemon_composed.rs, tests/instruction_writers.rs, tests/integration_sweep.rs.
- Product paths not inspected or only partly read: src/plan/line_diff.rs, src/cli/instructions.rs, src/plan/commands.rs, src/plan/core_kinds.rs, src/plan/mod.rs, src/cli/mod.rs, src/cli/setup.rs, src/daemon/compose.rs, src/model/common.rs, src/model/graph.rs, src/model/mod.rs, src/store/mod.rs, src/bootstrap/content.rs (deletion).
- Docs not inspected: skills/graph/SKILL.md, README.md, CURRENT-DESIGN.md, GRAPH_STATE.md, CREATING_NEW_GRAPH.md, docs/superpowers/specs/INDEX.md.
- Run artifacts under docs/superpowers/runs/2026-10-31-h1-instruction-gate/ (inv-p3b assembled slices, cluster reports, inv-p3c bounded bundle, run.md), apart from the partial reads noted in Scope.

## Verdict
not ready (bounded review of 32/217 paths with four provenance gaps; parked Codex escalations and the Codex compaction gap lack a human disposition; lossy template-undo data path; no full-suite run at head)

## Must fix before landing
- Complete a whole-range final review: this bounded review inspected 32 of 217 changed paths, and the four review-input provenance gaps (run.md:78 inv-p3b, run.md:83 inv-p3c) remain unresolved
- Run the caller-owned full suite (cargo test, cargo test --features test-support, cargo check --locked --offline --all-targets --all-features) at c2a9a306 and record the result; no full-suite measurement of this head exists
- Record a human disposition for the two parked roast-1 Codex escalations (run.md:18-19): whether a malformed or unsupported .rules file aborts Codex startup, and whether Codex's ~2,500-token additionalContext limit truncates a 9000-byte HOOK_RENDER_CAP render plus its JSON envelope
- Close or explicitly accept the Codex compaction gap: Codex 0.163.0 fires no SessionStart on compaction (src/cli/seat.rs doc comment), so AC6 inline re-injection after compaction depends on the agent obeying the skills/seat/SKILL.md line; install a PostCompact hook if Codex supports context there, or get a human sign-off
- Template-edit undo restores a non-UTF-8 member AGENTS.md lossily from recorded document text (ledger [lossy-undo-gap], hg-45e.37, regraded from minor as a data-loss path): restore the raw bytes, or refuse undo with repair_required, or record explicit human acceptance

## Untested scope
- No full-suite run at head c2a9a306; the targeted per-task runs recorded in the ledger do not measure the integrated branch
- [unreproduced-run-truncation] hg-45e.16: a --features test-support run exited 101 mid daemon_ipc with no failure text; the rerun passed, the cause is unproven, and it could be a flaky test or a product race
- The binary-level `herdr-graph plan` → relay hint under an agent marker → `herdr-graph apply --confirmed-by user-relay` leg is not exercised end to end (prior bounded review); the sweep uses the daemon-level plan+apply helper
- No live Codex check that the rules file prompts on apply and that additionalContext carries a near-cap render untruncated
- doctor harness_gate is not driven from CLAUDE_CONFIG_DIR/CODEX_HOME through Env::from_process (unit tests pass explicit paths)
- No test that an inadmissible member AGENTS.md or responsibility renders the placeholder (hg-45e.6)
- No test for the "seat AGENTS.md will be cut" cap-warning branch, for CRLF content through plan instructions set, or for --from resolved relative to the caller cwd (hg-45e.4)
- Cap sizing skips seats created by a template edit itself and cleared seat sections (hg-45e.18)
- No test for re-install with an existing hook_preexisting manifest or for the non-exact uninstall path with a pre-existing hook group (hg-45e.38)
- tests/instruction_writers.rs inventories writers by grepping .put_file(/.delete_file( only; writers using std::fs or other APIs are not detected (hg-45e.40)
- src/cli/instructions.rs run(): the no-instance exit and error exit codes are not tested end to end (hg-45e.13)
- No seat-reply test that rules/.gitkeep and other dotfiles are absent from paths.data (hg-45e.14)
- Everything listed under Unreviewed above: 185 changed paths and four provenance gaps not inspected by this review

## Deferred OK
- Recurring minor ×3 [missing-evidence] (hg-45e.8, hg-45e.3, hg-45e.4) and the other evidence-process minors (hg-45e.4/.6/.7/.10/.11/.14/.16/.36/.41)
- [repeat-complete-not-guarded] hg-45e.8: re-completing an --output-completed request with --summary-from writes the file and replaces output_ref; no data lost
- [spec-deviation-documented] hg-45e.41: an identical summary retry makes an audit-only commit, consistent with §1.3 "no graph change"
- [check-bypass-field] hg-45e.37: content_b64 skips check_content, but plan.create only accepts parse()-built args, so only undo sets it, and the renderer placeholders inadmissible bytes
- [daemon-not-enforcing] hg-45e.10: the daemon trusts the client-reported tty and does not check agent markers; accepted residual per spec §3.5
- [loose-presence-check] hg-45e.10: gate_installed RulesFile uses a substring match
- [classification-precedence] hg-45e.3: rules/seat.toml classifies as Record rather than Instruction; the renderer still placeholders it, so it fails closed
- [dotdir-handling] hg-45e.3: files under dot-directories in rules/ are listed and placeholdered instead of ignored (fail-closed spec deviation)
- [impl-placement] hg-45e.3: the manual Clone/PartialEq/Eq for StoreError live in instruction_source.rs rather than ports/store.rs
- [deferred-wiring] hg-45e.3: doctor ignoring rules/.gitkeep was wired later; fresh-init doctor test passes in source
- [fallback-quality] hg-45e.2: line_diff degrades to a full delete/insert above 4M LCS cells
- [spec-gap] hg-45e.6: cap_warning joins the omitted and cut clauses with '; '
- [behavior-drift] hg-45e.6: bound_paths skips an unreadable template_ref record (unreachable in practice)
- [possibly-stale-lint-allow] hg-45e.17: #[allow(clippy::too_many_arguments)] on create_member_seat
- [test-mechanism-deviation] hg-45e.9: fixtures use a test-only bookkeeping.test_put mutation
- [weak-source-evidence] hg-45e.7: Codex trigger coverage claim taken from binary strings of codex 0.163.0 (the substantive gap is tracked under Must fix)
- [doc-drift] hg-45e.18: the merged_after doc comment now sits above edit_cap_warnings in src/templates/kinds.rs
- [doc-drift] hg-45e.11: README has a prose section but no explicit command-list entry for the new commands
- [doc-drift] hg-45e.38: over-long module doc sentence in src/bootstrap/setup_claude.rs:10
- [doc-drift] hg-45e.16: the setup claude → doctor ok smoke relies on the hg-45e.12 test
- [cleanup-inspection] hg-45e.11: stop-run-processes --check exit 4 on foreign pids (environment)
- [lossy-utf8-read] hg-45e.5: a non-UTF-8 current file compares unequal in instructions_inverse and yields a conflict (fail-safe)
- [approximate-emoji-table] hg-45e.36: EMOJI_BASE broad ranges allow FE0F after some non-emoji symbols
- Latent seam: writer::current_version resolves Blob preconditions through args["path"], which instruction args lack; harmless while admit_apply sends empty relied_on
- hook_text renders for env-resolved Bound callers, not only pane bindings as §4.4's "same rule as wants_seat_prompt" says
- fit_hook can exceed HOOK_RENDER_CAP when the omitted-parts marker alone outgrows the budget (hundreds of parts; not realistic)
- The worktree_dirty notification (src/threads/effects.rs:572) points data-file edits at plan instructions set, which cannot commit data files
- Seeded seat AGENTS.md copies in existing instances render member text twice until the documented CHANGELOG migration runs
- Two distinct uncompleted requests with the same transcript and range plus identical bytes are rejected summary_conflict rather than a no-op
- summary_b64 rides in the journal and bookkeeping request as well as the seat file (storage duplication)
