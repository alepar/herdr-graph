# Final review (bounded): epic hg-45e, branch super-auto/h1-instruction-gate

Invocation: inv-p3c-20261031T232503. Captured source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head 7195fd631dc85daa6a0651f20a47a76cdf2217d8 (both verified with `git rev-parse --verify`). The caller checkout was clean and at 7195fd6 when the review started; I did not switch or modify it.

## Scope of this review

This is a bounded captured-source review, not a whole-range review. The review-inputs sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, 199 selected paths, 0 omitted, and one retained gap: `7195fd6:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:63 inv-p3b-20261031T222244: incomplete claimed collection`. I did not build or load a whole-range package. I read the full changed-path inventory (199 paths, 29779 insertions, 944 deletions) and inspected 16 paths from captured Git, listed in bounded-coverage.json: CHANGELOG.md, CREATING_NEW_GRAPH.md, CURRENT-DESIGN.md, GRAPH_STATE.md, README.md, skills/graph/SKILL.md, skills/seat/SKILL.md, templates/system-summarizer/members/summarizer/AGENTS.md, src/cli/content.rs, src/cli/mod.rs, src/cli/plan.rs, src/cli/request.rs, src/plan/instructions.rs, src/transcripts/mod.rs, src/transcripts/mutations.rs, tests/integration_sweep.rs. I also read two head-revision regions of src/bootstrap/setup_claude.rs (lines 380-400 and 558-575) without inspecting its diff hunks, so that path is not claimed as covered.

I picked the inspected set because of the prior final review (inv-p3b, verdict not ready). Its must-fix items fell on docs, the graph skill, the summary path and the sweep tests, and three of the four post-p3b beads (hg-45e.11, .12, .16) changed exactly these files. Nobody has run a full-suite measurement of head 7195fd6. The caller runs that sweep after this invocation, so the branch is not tested at head.

## Findings (own view, against the spec)

- Prior must-fix items, now resolved in captured source: the graph skill no longer instructs `content write` and carries the relay copy, the hash-match check and the cooperative-strength note (skills/graph/SKILL.md); the needle test asserts the new text and the absence of "content write" (463ee0f, src/bootstrap/tests.rs); CHANGELOG.md has the removal, added commands, `paths.clone_files` → `paths.data`/`paths.clone_record`, the ask rule and the Migration section (summarizer template edit plus `plan instructions clear seat <name> AGENTS.md` for seeded seats); README.md, CREATING_NEW_GRAPH.md, GRAPH_STATE.md and CURRENT-DESIGN.md are updated; the configuration smoke over the three scopes and both harness gates landed (91ef36a); the sweep tests for summary-path classification and summary-into-retired-seat plus instruction flow landed (7a2f0c6, 89933f8); the top-level evidence/ directory is gone (be43cf8; `git ls-tree 7195fd6 evidence` is empty).
- Still open from the prior must-fix list: run.md:18-19 at head still lists both roast-1 Codex escalations as parked with no human disposition: (a) whether a malformed or unsupported `.rules` file aborts Codex startup, (b) whether Codex's ~2,500-token `additionalContext` limit truncates a 9000-byte HOOK_RENDER_CAP render plus its JSON envelope. These are plan-level questions that need the human's decision. The only evidence on (a) is a code comment citing `codex execpolicy check` on codex-cli 0.163.0 (setup_claude.rs:73-74), and nothing addresses (b).
- Data-loss path in `setup codex` (src/bootstrap/setup_claude.rs:386-397, 565-568). An existing `$CODEX_HOME/rules/herdr-graph.rules` with different content is overwritten (`HookChange::Updated`) without a backup. Uninstall later deletes the file, because the manifest marks it as gate_file. That contradicts CHANGELOG.md's "Uninstall removes only what setup added". The ledger graded it minor ([overwrites-preexisting-file], hg-45e.10). It is a data-loss path, so I triage it as must-fix. The fix is small: keep the original bytes in the manifest and restore them on uninstall, or refuse and warn on foreign content.
- The sweep's instruction flows use the daemon-level `d.committed(...)` plan+apply helper, not the `herdr-graph plan` binary and then `herdr-graph apply --confirmed-by user-relay`. Rendering is checked end to end through the binary (`seat --instructions`, `seat --hook-prompt`), and the uncommitted-edit exclusion is asserted. The binary plan/apply leg and the relay-hint output under an agent marker are not exercised together. I record this as untested scope, not a defect.
- `request complete --summary-from` sends the summary bytes as `summary_b64` inside the bookkeeping request payload (src/transcripts/mod.rs:674-680), so every summary is also carried in the journal and request record as well as the seat file. It is correct, but it doubles storage for large summaries. Deferred.
- `write_summary` (src/transcripts/mutations.rs) re-completes a request that was already completed with `--output` for the same range: it writes the file and replaces output_ref instead of rejecting. No data is lost, since output_ref was bookkeeping only. This matches the ledger's deferred [repeat-complete-not-guarded].
- The documented command forms resolve. `plan undo <act>` works because plan forwards raw words and the undo kind matches `("undo","")`. `summary_conflict` is raised for both a different range and different bytes, and the `content write` stub exits 2 with guidance that the cli_sweep asserts.

## Ledger triage

- Recurring minor ×3 [missing-evidence] (hg-45e.8, hg-45e.3, hg-45e.4), process class: deferred, along with the other evidence-only minors ([missing-evidence] on hg-45e.11/.16, [no-red-run], [thin-evidence], [suite-not-run]). [committed-evidence-file] on hg-45e.7 is resolved by be43cf8.
- [overwrites-preexisting-file] (hg-45e.10): regraded to must-fix as a data-loss path (above).
- [unreproduced-run-truncation] (hg-45e.16): the first `--features test-support` run ended mid daemon_ipc with exit 101 and no failure text, and the rerun passed. This could be a test or product flake. It goes to untested scope, and the caller's full sweep must confirm it does not recur.
- [daemon-not-enforcing] (hg-45e.10): the daemon does not enforce the agent marker. That is an accepted residual vector, documented in README and the graph skill. Deferred.
- No `parked` completions, `parked finding` lines or BLOCKED-AUTH lines appear in the ledger. Metrics are UNAVAILABLE (Finish ledger re-read truncated), so missing ledger counts are not evidence of a clean run.
- Every other deferred minor is listed under Deferred OK.

## Verdict
not ready (bounded review of 16/199 paths; parked Codex escalations lack a human disposition; setup codex overwrites and later deletes a pre-existing herdr-graph.rules; no full-suite run at head)

## Must fix before landing
- Record a human disposition in run.md for the two parked roast-1 Codex escalations: whether a malformed or unsupported .rules file aborts Codex startup, and whether Codex's ~2,500-token additionalContext limit truncates a HOOK_RENDER_CAP=9000 render plus its JSON envelope (run.md:18-19, plan-level decision)
- setup codex overwrites a pre-existing $CODEX_HOME/rules/herdr-graph.rules with different content and uninstall deletes it (src/bootstrap/setup_claude.rs:386-397, 565-568), contradicting CHANGELOG "Uninstall removes only what setup added"; keep and restore the original bytes or refuse on foreign content (ledger minor [overwrites-preexisting-file] regraded as a data-loss path)
- Complete a whole-range final review: this bounded review covered 16 of 199 changed paths and the run.md:63 inv-p3b collection gap is unresolved
- Run the caller-owned full suite (cargo test, cargo test --features test-support, cargo check --locked --offline --all-targets --all-features) at 7195fd6 and record the result; no full-suite measurement of this head exists

## Untested scope
- No full-suite measurement of head 7195fd6; the sweep is deferred to the caller
- Unreviewed in this bounded pass: src/bootstrap/instructions.rs, src/bootstrap/mod.rs, src/bootstrap/setup_claude.rs (only head lines 380-400 and 558-575 read), src/bootstrap/tests.rs, src/bootstrap/content.rs, src/store/instruction_source.rs, src/store/layout.rs, src/store/mod.rs, src/store/tests.rs, src/daemon/doctor.rs, src/daemon/compose.rs, src/cli/instructions.rs, src/cli/seat.rs, src/cli/setup.rs, src/cli/undo.rs, src/config.rs, src/model/*, src/plan/line_diff.rs, src/plan/kinds_extra.rs, src/plan/types.rs, src/plan/tests.rs, src/plan/commands.rs, src/plan/core_kinds.rs, src/plan/mod.rs, src/templates/*, src/threads/effects.rs, src/transcripts/tests.rs, src/undo/*, src/writer/*, tests/daemon_composed.rs, docs/superpowers/specs/INDEX.md and all docs/superpowers/runs/** paths including the archived inv-p3b review package
- Provenance gap retained: 7195fd6:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:63 inv-p3b-20261031T222244 incomplete claimed collection
- Prior degraded verdict retained: inv-p3b cluster-1, cluster-6 and cluster-13 reports were not strictly extractable (run.md parked degraded-verdict line)
- hg-45e.16's first test-support run ended mid daemon_ipc with exit 101 and no failure text; the rerun passed and the cause is unproven (possible flake)
- The R16 writer inventory exists only in the uncommitted task-18-report.md; no committed test or artifact enforces it
- The sweep drives plan/apply through the daemon helper d.committed, not the herdr-graph plan and apply --confirmed-by user-relay binary path, so the relay-hint output under an agent marker is not exercised end to end
- setup claude/codex -> doctor harness_gate is covered only by hg-45e.12's sweep_harness_gate_install_doctor_uninstall; the hg-45e.16 sweep relies on it
- No committed evidence that a Codex SessionStart payload near 9000 bytes plus JSON escaping reaches the model intact
- Upgrade path for an existing instance with seeded seat AGENTS.md copies and the pre-H1 summarizer template has no test or doctor check (migration is documentation only)
- No integration test applies an instructions_edit and then undoes it through the composed daemon; undo of global and teamspace scopes has no committed test
- Summary write and request completion in one commit, the identical-retry no-op and summary_conflict are unit-tested only, not asserted at integration level
- Re-completing a request completed with --output by --summary-from for the same range writes and re-completes; no test
- instruction_files doctor check covers only the global rules/ scope; teamspace, seat rules, seat AGENTS.md and retired-scope skipping are untested
- set/clear against a committed non-UTF-8 or inadmissible current file (plan OK, apply Failed) and CRLF-only edits giving an empty diff are untested
- Undo marker path in src/cli/undo.rs has no undo-specific test (hg-45e.10)
- src/cli/instructions.rs no-instance exit path and error exit codes are not tested end to end (hg-45e.13)
- No test for the seat AGENTS.md will-be-cut cap-warning branch, CRLF content or --from resolved relative to caller cwd (hg-45e.4)
- No test that an inadmissible member AGENTS.md or responsibility renders the placeholder (hg-45e.6)
- No seat-reply test that dotfiles like rules/.gitkeep are absent from paths.data (hg-45e.14)
- Seat skill content (Codex seat --instructions fallback line, new step 3) has no needle test
- Hook bound rule for a caller resolved only via HERDR_GRAPH_CLONE (env) is untested

## Deferred OK
- summary_b64 is stored in the bookkeeping request payload as well as the seat file, doubling storage for large summaries (src/transcripts/mod.rs:674-680)
- [repeat-complete-not-guarded] write_summary re-completes a request already completed via --output for the same range (hg-45e.8); no data lost
- [daemon-not-enforcing] caller.tty reflects the raw stdin tty and the daemon does not enforce the agent marker; documented residual vector (hg-45e.10)
- [loose-presence-check] gate_installed RulesFile uses a substring match (hg-45e.10)
- [classification-precedence] classify checks RECORD_FILES before instruction locations (hg-45e.3)
- [impl-placement] manual Clone/PartialEq/Eq for StoreError in instruction_source.rs (hg-45e.3)
- [dotdir-handling] walk skips dotfiles by basename only; files under dot-directories inside rules/ are listed (hg-45e.3)
- [deferred-wiring] doctor ignoring rules/.gitkeep was wired later (hg-45e.3)
- [fallback-quality] line_diff degrades to full delete/insert above n*m > 4M (hg-45e.2)
- [spec-gap] cap_warning joins omitted and cut clauses with '; ' (hg-45e.6)
- [behavior-drift] bound_paths skips a seat template_ref whose template record is unreadable (hg-45e.6)
- [test-mechanism-deviation] bootstrap fixtures use bookkeeping.test_put instead of put_files (hg-45e.9)
- [weak-source-evidence] Codex SessionStart/PostCompact coverage claim derived from codex 0.163.0 binary strings (hg-45e.7); folds into the parked escalation disposition
- [doc-drift] edit_cap_warnings sits under a stale doc comment in src/templates/kinds.rs (hg-45e.18)
- [missing-edge-case-test] SeatOverride clearing sections is not sized, and seats created by a template edit are not sized (hg-45e.18)
- [lossy-utf8-read] undo preview instructions_inverse reads with from_utf8_lossy and fails closed as a conflict (hg-45e.5)
- [possibly-stale-lint-allow] too_many_arguments allow on create_member_seat (hg-45e.17)
- [doc-drift] README has a prose section but no command-list entry for the new commands (hg-45e.11)
- [doc-drift] setup claude -> doctor ok is covered by the hg-45e.12 test rather than a new sweep test (hg-45e.16)
- Recurring minor x3 [missing-evidence] process class (hg-45e.8, hg-45e.3, hg-45e.4) and the other evidence-only minors ([missing-evidence] hg-45e.11/.16, [no-red-run] hg-45e.10, [thin-evidence] hg-45e.6, [suite-not-run] hg-45e.14)
- [committed-evidence-file] hg-45e.7: resolved by be43cf8 (evidence/ removed)
- [cleanup-inspection] stop-run-processes --check exit 4 on foreign pids (hg-45e.11), environment issue
- Carried forward from inv-p3b Deferred OK: renderer placeholder points to doctor scopes; hook_text accepts env-resolved Bound; /seat proposal relay copy still says "after an explicit yes"; data files have no commit route after content write removal; template/instruction undo refused when other text is inadmissible; no doctor check for over-cap seat renders; Codex compaction relies on the seat skill line; Codex JSON envelope can exceed HOOK_RENDER_CAP; setup_claude install_for restorable flag; PatchedTree duplicates Overlay; undo summary shows raw gr_ id; identical pre-existing summary for an uncompleted request gets an inaccurate message; summarizer template range-placeholder wording; CLAUDE_CONFIG_DIR empty-value filter in src/daemon/mod.rs:80
