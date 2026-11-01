# Final review (bounded) — epic hg-45e, branch super-auto/h1-instruction-gate

Invocation: inv-p6fr-20261101T020246. Captured source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head 05d731054d7541d3eee97a1ef3d26008cc7c8d6f (both verified with `git rev-parse --verify`; caller checkout at 05d7310, clean, unchanged during review). Spec: docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md at head.

This is a bounded captured-source review, not a whole-range review. The review-inputs sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, 242 selected paths, 0 omitted, 10 provenance gaps (all in run.md at 05d7310). The failed assembly outputs and the original sidecar were preserved. No whole-range package was built or loaded. A whole-range review remains outstanding.

## Scope inspected

17 of 242 changed paths, 39 hunks, read from captured Git (`git diff base..head`, `git show head:<path>`): the undo arm (src/undo/{preview,compensate,candidates,mod}.rs), the instructions plan kind (src/plan/instructions.rs), InstructionSource (src/store/instruction_source.rs), the renderer and hook fit (src/bootstrap/instructions.rs), the summary API (src/transcripts/{mutations,mod}.rs, src/model/b64.rs), the prompt gate and hook CLI (src/cli/{plan,undo,seat}.rs), README.md, CHANGELOG.md, skills/seat/SKILL.md and the design doc. Head-file spot reads outside those hunks (not counted as hunk coverage): src/bootstrap/setup_claude.rs gate install/uninstall, src/plan/commands.rs confirm-mode and plan.create arg handling, src/daemon/client.rs caller_info_from_env, src/store/tree.rs list_dir.

## Step 1 — own findings against the spec

- Spec gap, Codex compaction: the design goal and acceptance criterion 6 require the instruction stack inline at start, clear and compaction. src/cli/seat.rs (hook_output_text doc) records that Codex 0.163.0 has no SessionStart compaction source (only `PostCompact`), and the only mitigation is one line in skills/seat/SKILL.md telling Codex agents to run `seat --instructions` — the "depend on the agent obeying a prompt" mode the design set out to remove. The divergence is not recorded in the design's Post-Implementation Notes (empty at head), and unchanged README.md:33 still says both harnesses fire SessionStart on compaction. Needs a human decision: install an owned Codex `PostCompact` hook, or accept the divergence and record it (design note, README:33 correction).
- src/transcripts/mutations.rs write_summary: a request already Completed via `--output` with the same range, then re-completed with `--summary-from`, writes the file and replaces `output_ref` instead of being treated as a conflict (same as ledger Task 3 minor). Spec §1.3 does not name this case; low impact.
- src/model/b64.rs b64_decode is lenient: it strips trailing `=` and silently drops leftover bits (a length ≡ 1 mod 4 input decodes without error). Only internally produced payloads reach it today; low impact.
- src/bootstrap/instructions.rs fit_hook: the cap is measured on the plain text; for Codex the text is then JSON-escaped into `additionalContext`, so the emitted bytes can exceed HOOK_RENDER_CAP. Acceptable while the cap remains a conservative assumption.
- src/bootstrap/instructions.rs PatchedTree::list_dir keeps a directory entry whose only file a patch deletes (WorkTree::list_dir drops it). Harmless for current callers, which walk into the empty dir.
- src/plan/commands.rs accepts `mode: "tty"` whenever the client-reported `caller.tty` is true; daemon/client.rs fills it from raw stdin without the agent-marker check. Only reachable through direct socket calls, a documented residual vector (§3.5).
- Verified correct in inspected hunks: InstructionsKind staleness rides `before_sha` in the effect (same_effects); undo inverse delegates to the instructions kind with raw bytes via `content_b64` only from undo (plan.create accepts only `words`, commands.rs:323); the renderer order matches §4.1 (shared seat template, team root, member + responsibility, sections, seat rules, seat AGENTS.md); fit_hook budget arithmetic matches the emitted layout (stop at first overflow, single-part cut at a line boundary); check_content implements the §2.2a clause including CRLF and keycap/emoji selector rules; the `[y/n]` prompts in plan and undo honour the four agent markers, and `CLAUDE_CODE_SESSION_ID` is set in a live Claude Code shell.

## Step 2 — ledger triage (progress.md)

No `parked`, `parked finding`, `Recurring blocker:` or `BLOCKED-AUTH` lines. One `Recurring minor:` cluster: missing-evidence ×3 across hg-45e.8, hg-45e.3, hg-45e.4 (process class; reports gave test results as prose). Reassessed as process-only, no product impact; individual repeats also at hg-45e.11, hg-45e.16, hg-45e.46.

- Task 4 (hg-45e.10) [overwrites-preexisting-file], named a data-loss path: verified fixed at head — setup_claude.rs install bails on a pre-existing rules file with different content, and an identical pre-existing file is not marked owned (`gate_file`) so uninstall leaves it.
- Task 20 (hg-45e.37) [lossy-undo-gap], named a data-corruption path: addressed in inspected src/undo/preview.rs (template_inverse uses `before_raw` and refuses lossy or inadmissible legacy records with NO_RAW_BYTES); the recording side in src/templates/kinds.rs (raw_before_files, ~1218-1319) was not inspected here.
- Task 18 (hg-45e.16) [unreproduced-run-truncation]: a `--features test-support` run aborted mid daemon_ipc with exit 101 and no failure text; rerun passed. Unexplained abort; needs the full-suite sweep to show whether it recurs.
- Task 12 (hg-45e.14), Task 24 (hg-45e.41) [suite-not-run], Task 27 (hg-45e.44) [stale-test-evidence]: no full-suite run at head.
- Task 20 [check-bypass-field]: verified only undo sets content_b64; Deferred OK.
- Task 4 [daemon-not-enforcing]: documented residual; Deferred OK.
- Task 4 [loose-presence-check]: substring match can report a hand-edited rules file as present; Deferred OK.
- Task 6 [classification-precedence], [dotdir-handling]: such files are Record or fail the name check and render as placeholders; never data. Deferred OK.
- Task 19 [approximate-emoji-table]: VS16 after some non-emoji symbols is visible-harmless; Deferred OK.
- Task 11 [weak-source-evidence]: feeds the Codex compaction finding above.
- Remaining minors (lint allow, doc drift, missing edge-case tests, evidence-format items, grep-based writer inventory, cap-warning wording, oversize line-diff fallback): Deferred OK.

## Verdict
not ready (bounded review only: whole-range review outstanding, 10 provenance gaps, no full-suite run, Codex compaction delivery needs a human decision)

## Must fix before landing
- Decide the Codex compaction gap: either install an owned Codex PostCompact hook that injects the render, or record the accepted divergence in the design Post-Implementation Notes and correct README.md:33, which still claims Codex fires SessionStart on compaction (src/cli/seat.rs hook_output_text, skills/seat/SKILL.md line 8).
- Complete a whole-range final review of c77d9ad..05d7310; this bounded review covered 17 of 242 paths.
- Run the full suite at head (cargo test, cargo test --features test-support, cargo check --locked --offline --all-targets --all-features) and confirm the hg-45e.16 daemon_ipc exit-101 abort does not recur.

## Untested scope
- No full-suite measurement of this branch exists; the sweep is deferred to the caller.
- Ledger Metrics are UNAVAILABLE (merges, completions, fix-pass, ledger-check); ledger counts are not evidence of a clean run.
- Provenance gap: run.md:84 inv-p3b-20261031T222244 incomplete claimed collection.
- Provenance gap: run.md:89 inv-p3c-20261031T232503 incomplete claimed collection.
- Provenance gap: run.md:89 inv-p3c-20261031T232503 invalid failed assembly archive (bounded report identity or verdict mismatch).
- Provenance gap: run.md:89 inv-p3c-20261031T232503 incomplete assembled bundle (digest.md).
- Provenance gap: run.md:93 inv-p5r1-20261101T001157 incomplete claimed collection.
- Provenance gap: run.md:93 inv-p5r1-20261101T001157 invalid failed assembly archive (bounded report identity or verdict mismatch).
- Provenance gap: run.md:93 inv-p5r1-20261101T001157 incomplete assembled bundle (digest.md).
- Provenance gap: run.md:97 inv-p5r2-20261101T010931 incomplete claimed collection.
- Provenance gap: run.md:97 inv-p5r2-20261101T010931 invalid failed assembly archive (bounded report identity or verdict mismatch).
- Provenance gap: run.md:97 inv-p5r2-20261101T010931 incomplete assembled bundle (digest.md).
- Unreviewed product paths (all hunks): CREATING_NEW_GRAPH.md, CURRENT-DESIGN.md, GRAPH_STATE.md, docs/superpowers/specs/INDEX.md, skills/graph/SKILL.md, templates/system-summarizer/members/summarizer/AGENTS.md.
- Unreviewed product paths (all hunks): src/bootstrap/content.rs, src/bootstrap/mod.rs, src/bootstrap/setup_claude.rs (head spot-read only), src/cli/content.rs, src/cli/instructions.rs, src/cli/mod.rs, src/cli/request.rs, src/cli/setup.rs, src/config.rs.
- Unreviewed product paths (all hunks): src/daemon/compose.rs, src/daemon/doctor.rs (instruction_files and harness_gate checks), src/model/action.rs, src/model/change.rs, src/model/common.rs, src/model/graph.rs, src/model/ids.rs (IdKind::Graph mapping), src/model/mod.rs.
- Unreviewed product paths (all hunks): src/plan/commands.rs (spot-read only), src/plan/core_kinds.rs, src/plan/kinds_extra.rs (seat override cap check), src/plan/line_diff.rs, src/plan/mod.rs, src/plan/types.rs, src/store/layout.rs, src/store/mod.rs.
- Unreviewed product paths (all hunks): src/templates/document.rs, src/templates/kinds.rs (before_raw recording, edit_cap_warnings, hydration seeding change), src/threads/effects.rs, src/writer/mutation.rs.
- Unreviewed test paths (all hunks): src/bootstrap/tests.rs, src/model/tests.rs, src/plan/tests.rs, src/store/tests.rs, src/templates/tests.rs, src/transcripts/tests.rs, src/undo/tests.rs, src/writer/tests.rs, tests/daemon_composed.rs, tests/instruction_writers.rs, tests/integration_sweep.rs.
- Unreviewed run docs: 179 paths under docs/superpowers/runs/2026-10-31-h1-instruction-gate/ other than the design doc (roast reports, coverage rounds, findings ledgers, profiles, run.md, and the reviews/ archives for inv-p3b, inv-p3c, inv-p5r1, inv-p5r2).
- Task 20 lossy-undo fix: recording side in src/templates/kinds.rs not inspected; only the undo consumer in src/undo/preview.rs was.

## Deferred OK
- write_summary re-completes a request already completed via --output for the same range (ledger Task 3); low impact, spec §1.3 silent.
- b64_decode accepts non-canonical input (trailing bits dropped); only internal payloads reach it.
- Codex hook output can exceed HOOK_RENDER_CAP after JSON escaping; cap is a conservative assumption.
- PatchedTree::list_dir keeps emptied directories; harmless for current callers.
- Daemon trusts client-reported tty for mode tty (ledger Task 4); documented direct-socket residual.
- setup codex pre-existing rules file (ledger Task 4): verified fixed at head.
- content_b64 bypass of check_content reachable only from undo (ledger Task 20); verified plan.create takes only words.
- Loose Codex rules presence check (ledger Task 4).
- classify precedence and dot-directories under rules/ (ledger Task 6); never data, rendered as placeholders.
- Approximate emoji base table (ledger Task 19).
- Recurring minor missing-evidence cluster (hg-45e.8, hg-45e.3, hg-45e.4, plus hg-45e.11, hg-45e.16, hg-45e.46); process only.
- Remaining ledger minors: possibly stale clippy allow, doc drift items, missing edge-case tests, oversize line-diff fallback, cap-warning wording, grep-based writer inventory, committed evidence log, scope-file wording edit.
