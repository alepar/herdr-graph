# Final review: cluster 6 (src-daemon), epic hg-45e

Invocation: inv-p3b-20261031T222244
Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify <sha>^{commit}`; review source bound in the ledger before this write).
Files: src/daemon/compose.rs, src/daemon/doctor.rs (slices 000049.diff, 000050.diff).
Spec: docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md, sections 1.5, 2.2a and 3.4; child bead hg-45e.15.

## Scope and method

- Read both slices, the digest, the epic and the design sections that name this cluster.
- Opened other files only to check named risks: src/bootstrap/setup_claude.rs (`gate_installed`, `owned_hook_installed`, `config_dir`, `codex_dir`), src/store/instruction_source.rs (`list_files`, `walk`), src/bootstrap/instructions.rs (placeholder use), src/herdr/isolation.rs (tripwire and scrubbed env), src/config.rs (new `Env` fields), src/store/layout.rs (`all_seats`).
- Ran `cargo test --offline --lib daemon::` in a review-owned detached worktree at head (seeded target by reflink copy, submodule contents copied without .git): 46 passed, 0 failed. This includes `harness_gate_skips_without_owned_hook`, `harness_gate_ok_when_installed`, `harness_gate_warns_when_gate_missing` and `instruction_files_check_flags_inadmissible_files`. The worktree was removed afterwards.
- Observed caller state: HEAD stayed at bce6589, `git status --short` clean before and after.

## Findings

### compose.rs: removal of `crate::bootstrap::register_mutations` (spec 1.5)

Correct. At base, `bootstrap::register_mutations` registered only `content::ContentWrite`; nothing else is lost. The function no longer exists at head. `tests/daemon_composed.rs:393-398` asserts neither `content.write` nor any `content_write*` mutation is registered, and `src/writer/tests.rs:444` covers a legacy journaled ContentWrite row failing via "no mutation registered", which matches 1.5's upgrade story.

### doctor.rs: `harness_gate_checks` (spec 3.4)

Meets the spec: skips harnesses without the owned SessionStart hook, uses the same `config_dir`/`codex_dir` as setup, delegates rule syntax to `setup_claude::gate_installed` (ApprovalGate keeps ownership of syntax), and WARNs with exactly `run herdr-graph setup claude|codex`. Unparseable settings surface as a WARN instead of a failure, which fits doctor's role. `Env` gained `claude_config_dir`/`codex_home` from `CLAUDE_CONFIG_DIR`/`CODEX_HOME`. `config_dir`/`codex_dir` call the isolation tripwire; `scrubbed_env` already pins both variables under the test root, so doctor run by integration tests stays inside isolation.

Ordering: harness checks now come before the `instance` check in `doctor()`. The existing no-instance test (`r.checks[0].name == "instance"`) passes only because its temp home has no owned hook. Nothing I found reads `checks[0]` positionally outside tests, so this is a test-fragility note, not a defect.

### doctor.rs: `instruction_files_check` (spec 2.2a)

Meets the bead's acceptance (hg-45e.15): reads the committed head through `CommitView`, walks global plus non-retired teamspace and seat scopes via `InstructionSource::list_files` (no hand-built paths), WARNs once per file (a bad name short-circuits the content check), names the path and reason, ignores dotfiles such as `rules/.gitkeep`, and reports one OK line when everything is admissible. Graph read errors degrade to one WARN.

Seam gap (renderer to doctor): `src/bootstrap/instructions.rs` emits the placeholder `(...: skipped, not an admissible instruction file — see herdr-graph doctor)` for template `AGENTS.md`, member `AGENTS.md`, `responsibility` and seat `instructions_sections` as well (lines 163-165, 193, 262-264, 288-290). Doctor only checks `InstructionSource` scopes, so a template part committed by hand with, say, an ESC byte renders a placeholder that sends the user to doctor, and doctor then reports "all instruction files admissible". Spec 2.2a's doctor sentence is scoped to "a file in an instruction location", and plan-time checks cover the template and section paths, so this is reachable only through a direct git commit (an accepted residual vector in 3.5). I classify it as deferred, not blocking, but the placeholder's pointer is misleading in that case.

Smaller observations: seats under a retired teamspace are still checked when the seat itself is not retired (harmless, at worst an extra WARN). Files under dot-directories inside `rules/` are listed and WARNed as bad names (already recorded as a deferred minor from hg-45e.3).

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- `doctor()` wiring of `harness_gate_checks` through `Env` (`CLAUDE_CONFIG_DIR`/`CODEX_HOME`/`HOME` fallback) has no end-to-end test; the setup install, doctor, uninstall cycle in isolated config dirs (spec section 6) belongs to hg-45e.12 and hg-45e.16, both still open at head
- `instruction_files_check` tests cover only the global `rules/` scope; no test covers teamspace rules, seat rules, seat `AGENTS.md`, or skipping retired teamspaces and seats
- `harness_gate_checks` error branches (unparseable `settings.json`/`hooks.json`) have no test for the WARN mapping
- no test pins the new check order in the full `doctor()` report when an owned hook is installed (harness_gate rows ahead of `instance`)

## Deferred OK
- the renderer placeholder for template `AGENTS.md`, member `AGENTS.md`, `responsibility` and `instructions_sections` points to doctor, but doctor checks only InstructionSource scopes; reachable only via direct git commits, so a follow-up should extend `instruction_files` to those render parts or reword their placeholder
- seats inside a retired teamspace are still scanned when the seat itself is not retired; harmless extra coverage
- dot-directories inside `rules/` are walked and their files WARNed as bad names (existing hg-45e.3 deferred minor)
- the Codex gate presence check uses a substring match, which doctor inherits through `gate_installed` (existing hg-45e.10 deferred minor)
