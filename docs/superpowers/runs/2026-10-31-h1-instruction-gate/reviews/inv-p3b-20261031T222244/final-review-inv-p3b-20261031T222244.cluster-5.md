# Final review — cluster 5 (src-cli), epic hg-45e

Invocation: inv-p3b-20261031T222244. Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify`). Review-source binding recorded in progress.md via `dispatch-text.mjs bind-review-source` (exit 0) before this file was written.

Scope: src/cli/{content,instructions,mod,plan,request,seat,setup,undo}.rs (slices 000040–000047), judged against the design doc docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md at head. Other files were opened only to check named risks: src/bootstrap/mod.rs `wants_seat_prompt`, src/bootstrap/resolve.rs `resolve_caller`, src/bootstrap/instructions.rs `fit_hook`, src/daemon/client.rs `caller_info_from_env`, src/bootstrap/setup_claude.rs (Codex rule verification comment), skills/seat/SKILL.md, plus a grep of tests for `hook_text` and `--summary-from`.

## Observed evidence

- Caller checkout: HEAD bce6589 and `git status --short` clean at start and again before writing. No caller files edited.
- A review-owned detached worktree at .worktrees/super-auto-h1-instruction-gate/.worktrees/review-inv-p3b-cluster5 (head bce6589), target seeded with a reflink copy, submodule contents copied from the integration worktree.
- `cargo test --offline --locked --lib -- cli:: bootstrap::tests::hook bootstrap::tests::seat_instr`: 36 passed, 0 failed (covers every new cli test plus bootstrap::tests::hook_instructions::{bound_pane_gets_header_and_render, unbound_pane_gets_todays_prompt, large_global_rule_is_omitted_whole}).
- `cargo build --offline --locked` succeeded. Binary probes with an empty HOME and no instance: `content write --object x --rel a --from f` prints the exact §1.5 guidance to stderr and exits 2; `seat --instructions` prints `herdr-graph: not a bound graph seat` and exits 1; `instructions show global` prints `no instance configured` and exits 2 (EXIT_NO_INSTANCE); `seat --hook-prompt` prints nothing and exits 0.
- bd: hg-45e.11 (docs), hg-45e.12 (configuration smoke) and hg-45e.16 (integration sweep) are still open; their branches are not on the integration head.

## Spec conformance

- §1.5 content write stub: same arguments kept (`--object/--rel/--from/--expect`), exits 2, message matches the spec text byte for byte (asserted in a test). Meets AC1's CLI part.
- §1.1/§1.4 `request complete --summary-from`: `--output` and `--summary-from` are mutually exclusive and one is required (clap test). The CLI reads the file relative to its own cwd and sends `summary_b64` through the shared `model::b64`. Unreadable file exits 2 with the path. UTF-8 validation is left to the daemon, which matches §1.4 (validation happens in the mutation, another cluster).
- §2.2 `instructions show`: read-only, opens the committed head through `CommitView`, no daemon, 1–3 words. A test shows a dirty and an untracked working-tree file do not change the output. Inadmissible names and content print a placeholder rather than the text. It uses its own placeholder wording, not the renderer's §2.2a string; §2.2a only fixes the renderer's text, so this is allowed, but the two differ.
- §3.3 agent markers: `AGENT_MARKERS` is the exact four-variable set, an empty value counts as unset, and `prompt_terminal` covers both `ConfirmMode::Tty` commit paths: `plan_flow` (shared by rebind) and interactive `undo`. A grep at head confirms `"mode": "tty"` is sent only from those two places. The relay hint text matches §3.3.
- §4.3 `seat --instructions`: conflicts with `--json`/`--hook-prompt`, no daemon, `resolve_caller(view, caller, None)`, exits 1 with `not a bound graph seat`.
- §4.4 hook: header and render through `fit_hook(HOOK_RENDER_CAP)` when Bound, today's prompt otherwise, any failure falls back, and the Codex `additionalContext` shape is unchanged (test). The cap research is recorded in a doc comment, as the spec asked.
- §3.1/§3.2 setup output: prints the `apply gate:` state on install and uninstall. §3.2's "print a note when the Codex syntax cannot be confirmed" does not apply: setup_claude.rs records that the syntax was verified with `codex execpolicy check` (codex-cli 0.163.0).

## Findings

1. Hook bound rule is wider than "same rule as wants_seat_prompt" (§4.4). `hook_text` accepts any `Resolution::Bound`, which includes `via == "env"` (a live clone named by `HERDR_GRAPH_CLONE`). `wants_seat_prompt` accepts only `via == "binding"`. When a process inherits `HERDR_GRAPH_CLONE` but its pane is not that clone's binding (for example a nested or relaunched session), the hook now injects that seat's full instruction stack instead of the generic prompt. This matches what `seat --instructions` does under §4.3 ("binding or env"), and graph launches set the env deliberately, so the effect is benign. Still, it departs from the spec's wording, and no test covers the env-only case. Minor.
2. Codex compaction is not covered by an inline render. The doc comment on `hook_output_text` records that Codex 0.163.0 has no SessionStart source for compaction, so Codex seats depend on the skill line "after a clear or compaction, run `herdr-graph seat --instructions`". The epic goal and AC6 promise inline delivery at compaction without depending on the agent obeying a prompt. For Codex this is a harness limitation, documented only in a code comment and the skill. The design's Post-Implementation Notes section is still empty. Minor spec deviation: it should be written down there (hg-45e.11 is still open). A PostCompact hook is a possible follow-up if Codex's PostCompact supports `additionalContext`.
3. The Codex hook output can exceed `HOOK_RENDER_CAP`. `fit_hook` caps the text at 9000 bytes, but `hook_output_text(true, …)` then wraps it in JSON: about 70 bytes of envelope, and each newline, quote and tab escaped to two bytes. A near-cap render therefore emits several hundred bytes over 9000 on Codex. `println!` also adds one byte on Claude. §4.4 says "Output is capped". The cap is a conservative assumption and no Codex limit is documented, so this is minor.
4. `seat --instructions` prints `not a bound graph seat` for every failure: no instance located, store open error, and a render `StoreError` on a seat that is bound. That is accurate only for the unresolved case. Minor diagnostic issue.
5. Interactive `undo` under an agent marker on a real TTY prints the old non-terminal message, "run `herdr-graph undo` in a terminal, or `herdr-graph plan undo <act>` and relay the plan to the user". The behaviour is right (no prompt, and it matches the non-terminal path, as §3.3 requires), but the first clause is misleading for an agent that is already in a terminal. Cosmetic.
6. The `setup claude|codex` subcommand help still reads "Install the … SessionStart hook and the seat and graph skills" and does not mention the apply gate it now installs. Cosmetic.

None of these break an acceptance criterion this cluster owns. Each new path has unit coverage, and the binary probes behave as specified.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- `seat --hook-prompt` and `seat --instructions` binary wiring (`with_head`, `caller_info_from_env`, `print!` and exit codes) against a real bound instance; only `hook_text`/`instructions_text` are unit-tested, and the end-to-end smoke is hg-45e.12, which is still open
- `hook_text` for a caller resolved only via `HERDR_GRAPH_CLONE` (env) and not by pane binding, which differs from `wants_seat_prompt`'s binding-only rule
- Interactive `undo` refusing to prompt under an agent marker (src/cli/undo.rs:72); covered only through the `prompt_terminal`/`decide` unit tests (already a deferred minor)
- `instructions show` `run()` exit paths (no instance, error exit code) and the retired-target case; only `show_text` is tested
- `request complete --summary-from` with an unreadable file (exit 2) in the CLI; the happy path is exercised by the integration sweep step
- `setup` CLI `apply gate:` output lines on install, update and uninstall

## Deferred OK
- Hook bound rule accepts env-clone resolution as well as pane binding; consistent with §4.3 and benign, but worth a one-line Post-Implementation Note or a narrowing to `via == "binding"`
- Codex compaction depends on the skill telling agents to run `seat --instructions` (no SessionStart source for compaction in Codex 0.163.0); record it in the design's Post-Implementation Notes and consider a PostCompact hook as a follow-up
- Codex JSON envelope and escaping (and the trailing newline from println) can push hook output a few hundred bytes past HOOK_RENDER_CAP; the cap is a conservative assumption
- `seat --instructions` reports `not a bound graph seat` for instance, store and render failures too
- `undo` agent-marker message still says "run `herdr-graph undo` in a terminal"
- `instructions show` placeholder wording differs from the renderer's §2.2a placeholder
- `setup claude|codex` help text does not mention the apply gate
