# Final review (bounded): epic hg-45e, branch super-auto/h1-instruction-gate

Captured source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head aedf07d87d717e01a6db1fca6d1dcef3284323a9 (both verified with `git rev-parse --verify <sha>^{commit}`). The caller checkout was at aedf07d with a clean status when I started. I did not switch or restore it.

## Scope of this review

This is a bounded captured-source review, not a whole-range review. The review-inputs sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, 235 selected paths, 0 omitted, and a base/head that match. It retains seven gaps, all unresolved and unreviewed here:

- run.md:82 inv-p3b-20261031T222244: incomplete claimed collection
- run.md:87 inv-p3c-20261031T232503: incomplete claimed collection
- run.md:87 inv-p3c-20261031T232503: invalid failed assembly archive (bounded report identity or verdict mismatch)
- run.md:87 inv-p3c-20261031T232503: incomplete assembled bundle: digest.md
- run.md:91 inv-p5r1-20261101T001157: incomplete claimed collection
- run.md:91 inv-p5r1-20261101T001157: invalid failed assembly archive (bounded report identity or verdict mismatch)
- run.md:91 inv-p5r1-20261101T001157: incomplete assembled bundle: digest.md

I read the full changed-path inventory: 235 paths, 41272 insertions, 1085 deletions, of which 170 are run artifacts under docs/superpowers/runs/. I did not build or load a whole-range package. I focused on two things:

- The delta since the previous bounded review (inv-p5r1, head c2a9a306): 13 commits, which land hg-45e.43, .44, .46, .47 and .48.
- Product and doc paths that earlier bounded reviews left uninspected.

From captured Git I inspected 66 hunks across 28 paths, listed in bounded-coverage.json. Those are every hunk of 27 paths, plus the install_for hunk of src/bootstrap/setup_claude.rs.

Partly read but not claimed: the production half of src/cli/instructions.rs, head regions of src/bootstrap/instructions.rs (placeholder and part collection), src/store/instruction_source.rs (`walk`, `list_files`), src/observe/mutations.rs (`move_dir` callers), the old-record test in src/templates/tests.rs, the head of evidence/hg-45e.46/*.log, the design doc, run.md, findings-ledger.md, and the inv-p5r1 report.

The inv-p5r1 bounded review inspected the core abstractions at c2a9a306: InstructionSource, InstructionsKind, the renderer and hook fit, doctor, summaries, the `[y/n]` guard, and ids/layout. Of those paths, only setup_claude.rs, templates/kinds.rs, undo/preview.rs, kinds_extra.rs and line_diff.rs changed since, and I re-inspected the changed parts. That earlier coverage is context only. This review does not claim it.

No full-suite measurement of head aedf07d exists. The caller runs that sweep after this invocation, so the branch is untested at head.

## Findings (own view, against the spec)

- **Regression in the fix for the lossy template undo (Important).** Commit 4fd2948 (hg-45e.44) fixes the `[lossy-undo-gap]` must-fix. A template-edit action now records `before_raw` (base64 of every overwritten or deleted instruction file), and the inverse writes those bytes through `agents_md_raw`. The fix also makes `template_inverse` (src/undo/preview.rs) return `Conflict` with `NO_RAW_BYTES` for every record without `before_raw` whose changed instruction field had non-empty before text. That covers every TemplateEdit action written by v0.2.0 or earlier that touched a root or member AGENTS.md, including plain UTF-8 text that round-trips exactly. The test `template_edit_undo_old_record_without_raw_bytes_is_repair_required` pins this with the admissible bytes `old\n`. The only lossy case is invalid UTF-8: `doc_with_agents` reads with `String::from_utf8_lossy`, and other control bytes survive as text. So only recorded text containing U+FFFD is ambiguous. Today, an undo that worked in v0.2.0 is refused after upgrade, and CHANGELOG.md does not say so. Fix: refuse only when the recorded before text contains U+FFFD (or fails `check_content`), or add a CHANGELOG Migration note that pre-upgrade template edits touching instructions can no longer be undone.
- **Lossy-undo must-fix otherwise resolved.** The new path holds up in what I read:
  - `raw_before_files` keys entries by the same change path that `restored_instruction` looks up, including removed members (`members.<k>` → `members.<k>.agents_md`).
  - The inverse sends raw bytes only when they differ from the recorded text.
  - The mutation maps keys through `raw_field` on the after document and writes over `write_agents`' text.
  - `EditArgs.agents_md_raw` is documented as never set by `parse`, so it joins InstructionsKind's `content_b64` as an undo-only, check-skipping field (see Deferred OK).
  - Plan display vets the restored side (`Side::Restored`), so raw ESC or invalid UTF-8 never reaches a rendered plan.
- **Cap-warning projection (hg-45e.47) matches §4.4 better than before.**
  - `edit_cap_warnings` now lays the edit over the committed tree through the same `agents_placement` that `write_agents` uses, so a member rename carries its file. It also projects seats the edit creates (stand-in records), resurrected seats and application map withdrawals.
  - SeatOverride sizes a cleared `instructions_sections` too.
  - The stand-in slug comes from `seat_slug_for(cx.tree, …)` rather than the patched tree, so two creates in one edit could project onto one slug. That only drops a warning. Deferred.
- **hook_preexisting (hg-45e.43) is consistent.** Any run whose hook change is not `Unchanged` clears `hook_preexisting`, so an updated owned group becomes ours and uninstall removes it. CHANGELOG and the clap help now say "removes what setup added; pre-existing artifacts stay". The CHANGELOG wording was changed inside the released v0.2.0 section, but `hook_preexisting` does not exist at base c77d9ad, so the note describes unreleased behaviour and belongs under Unreleased. Minor doc drift.
- **Stray committed evidence logs at the repo root.** The hg-45e.46 merge commits evidence/hg-45e.46/cargo-test.log (933 lines), cargo-test-ts.log (951 lines) and covered-gaps.log (109 lines) at the top level, and cargo-test.log contains a local home-directory path. This repeats the `[committed-evidence-file]` minor (hg-45e.7) and the stray top-level evidence/ must-fix from the inv-p3b review. Build logs do not belong on main. Remove them, or move them under the run directory, before landing.
- **Placeholders echo raw names (minor, residual-only).** The renderer and `instructions show` put inadmissible file names raw into their output. `Collector::listed` (src/bootstrap/instructions.rs) uses `f.rel` in the part label and `f.path` in the placeholder and header. `cli/instructions.rs` `placeholder` echoes `f.path` too. `RepoPath::new` rejects only `/`-absolute paths, `\`, `.` and `..`, so a rules file name with ESC or bidi/zero-width code points reaches the rendered instructions and the terminal. That contradicts §2.2a's "the renderer never emits inadmissible text". Only a direct git commit can create such a name, which §3.5 already accepts as a residual, and `check_name`'s reason uses `{:?}` escaping. Deferred.
- **Writer inventory blind spot.** tests/instruction_writers.rs now inventories `std::fs` writers as well as `put_file`/`delete_file`, but it does not see `TreeEdit::move_dir`. I checked the unconfirmed callers in src/observe/mutations.rs: retire to the archive, and observed teamspace and seat renames. They move whole object folders with their identity intact, so no instruction file changes owner or content. This is not a gate bypass today. It goes under Untested scope.
- **Docs.**
  - The skill, README, CURRENT-DESIGN, GRAPH_STATE and the new README section carry the relay copy, the hash-match check and the cooperative-strength list.
  - Two stale lines remain. CREATING_NEW_GRAPH.md:68 still says `/seat` "exposes live references" and that "legacy inline members retain initial member-AGENTS copying into seat context", which contradicts the CHANGELOG Hydration change and §1.5. README.md:50 still says "`/seat` exposes live instruction references". Minor doc drift.
- **Small integration seams look correct.**
  - `daemon/compose.rs` drops `bootstrap::register_mutations`.
  - `mutation_key` no longer subkeys ContentWrite, so a legacy journal row fails through "no mutation registered" as §1.5 intends.
  - `RequestKind`/`ActionKind` serialize by snake_case name, so inserting `InstructionsEdit` mid-enum is safe.
  - Plan rendering prints the `diff` detail as an indented block and filters it out of the effect line.
  - The new `Env` fields feed doctor's harness paths.
  - `line_diff`'s hunk grouping and ranges are standard unified-diff arithmetic, and the vetted sides keep raw bytes out of plans.

## Ledger triage

- Recurring minor ×3 [missing-evidence] (hg-45e.8, hg-45e.3, hg-45e.4, process class): deferred, together with the other evidence-only minors. Those are [missing-evidence] on hg-45e.4/.11/.16/.46, [missing-red-evidence] and [stale-test-evidence] on hg-45e.44, [tdd-red-skipped] on hg-45e.36, [suite-not-run] on hg-45e.14/.41, and [committed-evidence-file] on hg-45e.7. I regraded [committed-evidence-file] to must-fix in its hg-45e.46 recurrence: build logs with a home path land on main.
- [lossy-undo-gap] (hg-45e.37), regraded to must-fix by inv-p5r1: fixed by 4fd2948, but the fix adds the pre-upgrade undo regression above, which I grade must-fix (cheap).
- [unreproduced-run-truncation] (hg-45e.16): a `--features test-support` run exited 101 mid daemon_ipc with no failure text, and the rerun passed. This may be a flaky test or a product race. Untested scope until the caller's full sweep shows it does not recur.
- Resolved in captured source, according to the commit subjects and the hunks I inspected:
  - hg-45e.18 cap sizing: cleared sections and created seats (3b14be5).
  - hg-45e.38 `hook_preexisting` re-install (e468e54).
  - hg-45e.40 grep-only inventory: `std::fs` patterns are now inventoried. `move_dir` and git2 paths are still not seen.
  - hg-45e.4 and hg-45e.14 missing edge tests: per covered-gaps.log, `crlf_content_is_admissible_for_set`, `from_path_resolves_against_caller_cwd`, `cap_warning_names_the_cut_seat_agents_md` and `dotfiles_are_not_data` pass. I did not inspect those test hunks.
- [missing-edge-case-test] (hg-45e.47): the resurrected-seat projection in `edit_cap_warnings` has no dedicated test. Untested scope.
- [check-bypass-field] (hg-45e.37): still bounded. The new `agents_md_raw` field has the same shape. Deferred.
- [daemon-not-enforcing] (hg-45e.10): accepted residual (§3.5). Deferred.
- The ledger has no `parked` completion, `parked finding`, `Recurring blocker:` or BLOCKED-AUTH lines.
- The Metrics show `landed≠completed 29 vs 5` and the ledger transport was truncated, so the ledger is incomplete and missing lines are not evidence of a clean run.
- run.md still parks the two roast-1 Codex escalations with no human disposition. findings-ledger.md:42-43 keeps them, and the Codex compaction gap, open.

## Unreviewed

- All seven provenance gaps listed under Scope (inv-p3b at run.md:82, inv-p3c at run.md:87, inv-p5r1 at run.md:91).
- Remaining hunks of src/bootstrap/setup_claude.rs (everything except the install_for hunk).
- Product paths not inspected by this review:
  - src/bootstrap/instructions.rs, src/bootstrap/mod.rs, src/bootstrap/content.rs (deletion)
  - src/cli/instructions.rs, src/cli/content.rs, src/cli/plan.rs, src/cli/request.rs, src/cli/seat.rs, src/cli/undo.rs
  - src/daemon/doctor.rs
  - src/model/b64.rs, src/model/ids.rs
  - src/plan/instructions.rs
  - src/store/instruction_source.rs, src/store/layout.rs
  - src/templates/document.rs
  - src/threads/effects.rs
  - src/transcripts/mod.rs, src/transcripts/mutations.rs
  - src/undo/candidates.rs, src/undo/compensate.rs, src/undo/mod.rs
  - templates/system-summarizer/members/summarizer/AGENTS.md, skills/seat/SKILL.md
- Test sources not inspected: src/bootstrap/tests.rs, src/plan/tests.rs, src/store/tests.rs, src/templates/tests.rs, src/transcripts/tests.rs, src/undo/tests.rs, src/writer/tests.rs, src/model/tests.rs, tests/daemon_composed.rs, tests/integration_sweep.rs.
- evidence/hg-45e.46/ log contents beyond their heads.
- Run artifacts under docs/superpowers/runs/2026-10-31-h1-instruction-gate/ (170 paths), apart from the partial reads noted in Scope.

## Verdict
not ready (bounded review only: 28 of 235 paths; provenance gaps, parked Codex decisions, pre-upgrade template-undo regression, stray evidence logs and no full-suite run at aedf07d)

## Must fix before landing
- Complete a whole-range final review: this bounded review inspected 66 hunks across 28 of 235 changed paths, and the seven review-input provenance gaps (run.md:82 inv-p3b, run.md:87 inv-p3c, run.md:91 inv-p5r1) remain unresolved
- Run the caller-owned full suite (cargo test, cargo test --features test-support, cargo check --locked --offline --all-targets --all-features) at aedf07d and record the result; no full-suite measurement of this head exists
- Record a human disposition for the two parked roast-1 Codex escalations (run.md parked list; findings-ledger.md:42): whether a malformed or unsupported .rules file aborts Codex startup, and whether Codex's ~2,500-token additionalContext limit truncates a 9000-byte HOOK_RENDER_CAP render plus its JSON envelope
- Close or explicitly accept the Codex compaction gap (findings-ledger.md:43): Codex 0.163.0 fires no SessionStart on compaction (src/cli/seat.rs:74), so AC6 inline re-injection after compaction depends on the agent obeying a skills/seat/SKILL.md line
- template_inverse (src/undo/preview.rs) refuses undo with repair_required/NO_RAW_BYTES for every pre-upgrade TemplateEdit record that changed a non-empty root or member AGENTS.md, even exact UTF-8 text, so v0.2.0-era undos regress silently; refuse only when the recorded before text contains U+FFFD (the sole lossy case, since doc_with_agents uses from_utf8_lossy), or add a CHANGELOG Migration note
- Remove evidence/hg-45e.46/ (three cargo logs, about 2000 lines, one with a local home-directory path) from the repo root, or move it under the run directory, before landing; it repeats the [committed-evidence-file] minor and the earlier stray evidence/ must-fix

## Untested scope
- whole-epic review incomplete; bounded inspected hunks only; all remaining hunks and provenance gaps remain unreviewed
- No full-suite run at head aedf07d; the targeted per-task runs in the ledger do not measure the integrated branch, and hg-45e.44's test-support run predates its final rebase
- [unreproduced-run-truncation] hg-45e.16: a --features test-support run exited 101 mid daemon_ipc with no failure text; the rerun passed and the cause is unproven (possibly a flaky test or a product race)
- No live Codex check that the rules file prompts on apply and that additionalContext carries a near-cap render untruncated
- Resurrected-seat projection in edit_cap_warnings has no dedicated test (hg-45e.47)
- tests/instruction_writers.rs does not inventory TreeEdit::move_dir (src/observe/mutations.rs, src/plan/core_kinds.rs) or git2-based writers; the observed callers were checked by inspection only
- Commit 762cff4 claims tests for the binary relay leg, harness_gate env resolution and the std::fs inventory; only tests/instruction_writers.rs was inspected here, not the integration_sweep or bootstrap test hunks
- Everything listed under Unreviewed above: 207 changed paths, the uninspected setup_claude.rs hunks and seven provenance gaps

## Deferred OK
- Recurring minor ×3 [missing-evidence] (hg-45e.8, hg-45e.3, hg-45e.4) and the other evidence-process minors (hg-45e.4/.11/.14/.16/.36/.41/.44/.46)
- [scope-file-extra] hg-45e.43: clap doc wording in src/cli/setup.rs edited outside the bead's file list; wording-only
- [check-bypass-field] hg-45e.37: content_b64 and now TemplateEdit agents_md_raw skip check_content, but parse never sets them, so only the undo inverse does, and the renderer puts a placeholder in place of inadmissible bytes
- [daemon-not-enforcing] hg-45e.10: the daemon trusts the client-reported tty and does not check agent markers; accepted residual per spec §3.5
- [repeat-complete-not-guarded] hg-45e.8: re-completing an --output-completed request with --summary-from writes the file and replaces output_ref; no data lost
- [spec-deviation-documented] hg-45e.41: an identical summary retry makes an audit-only commit, consistent with §1.3
- [loose-presence-check] hg-45e.10: gate_installed RulesFile uses a substring match
- [classification-precedence] hg-45e.3: rules/seat.toml classifies as Record rather than Instruction; fails closed
- [dotdir-handling] hg-45e.3: files under dot-directories in rules/ are listed and shown as placeholders instead of ignored (fail-closed spec deviation)
- [impl-placement] hg-45e.3: the manual Clone/PartialEq/Eq for StoreError live in instruction_source.rs
- [fallback-quality] hg-45e.2: line_diff degrades to a full delete/insert above 4M LCS cells
- [spec-gap] hg-45e.6: cap_warning joins the omitted and cut clauses with '; '
- [behavior-drift] hg-45e.6: bound_paths skips an unreadable template_ref record (unreachable in practice)
- [possibly-stale-lint-allow] hg-45e.17: #[allow(clippy::too_many_arguments)] on create_member_seat
- [test-mechanism-deviation] hg-45e.9: fixtures use a test-only bookkeeping.test_put mutation
- [weak-source-evidence] hg-45e.7: Codex trigger coverage taken from binary strings of codex 0.163.0 (substantive gap tracked under Must fix)
- [doc-drift] hg-45e.11: README has a prose section but no explicit command-list entry for the new commands
- [doc-drift] hg-45e.38: over-long module doc sentence in src/bootstrap/setup_claude.rs
- [doc-drift] hg-45e.16: the setup claude → doctor ok smoke relies on the hg-45e.12 test
- [cleanup-inspection] hg-45e.11: stop-run-processes --check exit 4 on foreign pids (environment)
- [lossy-utf8-read] hg-45e.5: a non-UTF-8 current file compares unequal in instructions_inverse and yields a conflict (fail-safe)
- [approximate-emoji-table] hg-45e.36: EMOJI_BASE broad ranges allow FE0F after some non-emoji symbols
- Latent seam: writer::current_version resolves Blob preconditions through args["path"], which instruction args lack; harmless while admit_apply sends an empty relied_on
- hook_text renders for env-resolved Bound callers, not only pane bindings as §4.4's "same rule as wants_seat_prompt" says
- fit_hook can exceed HOOK_RENDER_CAP when the omitted-parts marker alone outgrows the budget (hundreds of parts; not realistic)
- The worktree_dirty notification (src/threads/effects.rs:572) points data-file edits at plan instructions set, which cannot commit data files
- Seeded seat AGENTS.md copies in existing instances render member text twice until the documented CHANGELOG migration runs
- Two distinct uncompleted requests with the same transcript and range plus identical bytes are rejected summary_conflict rather than treated as a no-op
- summary_b64 rides in the journal and bookkeeping request as well as the seat file (storage duplication)
- Renderer and instructions show echo inadmissible file names raw in the placeholder and part header (src/bootstrap/instructions.rs Collector::listed, src/cli/instructions.rs placeholder); only a direct git commit can create such a name, which is an accepted residual
- edit_cap_warnings computes stand-in seat slugs against the committed tree, not the patched one, so two creates in one edit can project onto one slug; affects warnings only
- CHANGELOG v0.2.0 section was edited to describe the unreleased hook_preexisting uninstall behaviour; move that wording under Unreleased
- CREATING_NEW_GRAPH.md:68 and README.md:50 still describe /seat as live references and inline members as seeding a seat AGENTS.md copy, contradicting the CHANGELOG Hydration change
- instructions show uses a different placeholder wording from the renderer's
