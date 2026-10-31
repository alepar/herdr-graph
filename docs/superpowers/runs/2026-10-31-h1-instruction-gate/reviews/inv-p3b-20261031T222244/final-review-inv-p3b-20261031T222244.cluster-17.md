# Final review: cluster 17 (tests), epic hg-45e

Invocation: inv-p3b-20261031T222244. Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify`; source bound to the ledger via `bind-review-source`, exit 0).

Inputs: `clusters.json` cluster `tests` = `tests/daemon_composed.rs`, `tests/integration_sweep.rs` (slices 000086, 000087), `digest.md`, the epic spec (`bd show hg-45e`, design doc `docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md` at head), and the open beads hg-45e.12 and hg-45e.16, read to check the untested-scope risk.

## What the cluster changes

- `tests/daemon_composed.rs::every_kind_and_command_registered`: `ORG_KINDS` gains `instructions_edit` (22 to 23). `content.write` and `content_write` leave the expected lists. Two negative assertions check that neither the daemon command nor any `content_write*` mutation is registered (spec §1.5, AC1).
- `tests/integration_sweep.rs` golden path: the old two-step `content write` + `request complete --output` is now one `request complete --summary-from <file> --covered 0-N`. The test asserts the bytes land at the derived path `summaries/<tr>-0-N.md` in the archived seat folder, and that the summary moves back with the folder on undo (spec §1.2, AC2).
- `wiring::sweep_every_request_kind_has_a_mutation`: adds `K::InstructionsEdit` to `all` and the exhaustive match. Maps `K::ContentWrite => continue` as a legacy kind with no mutation (spec §1.5).
- `cli_sweep::sweep_every_cli_command_reaches_a_handler`: keeps the `content write` argv in the handler sweep. Adds an explicit check that the stub exits 2 and that its guidance names `--summary-from` and `plan instructions` (spec §1.5, AC1).

## Verification run (review-owned detached worktree at head, removed afterwards)

- `cargo test --locked --offline --test daemon_composed every_kind_and_command_registered`: 1 passed.
- `cargo test --locked --offline --features test-support --test integration_sweep`: 8 passed (wiring and cli_sweep).
- `cargo test --locked --offline --features private-herdr --test integration_sweep golden`, run five times: 5/5 passed.
- Committed evidence `evidence/hg-45e.9/features.log` records one golden failure at `tests/integration_sweep.rs:621` ("the summary went back with the folder") followed by a passing run in the same commit. The five reruns did not reproduce it. Cause not attributed.
- Caller checkout observed before and after: HEAD bce6589, `git status` clean.

## Assessment

The edits are correct and match the spec. The legacy `ContentWrite` handling is right: the variant stays, nothing registers it, and the exhaustive match still forces a decision for new variants. The golden step now checks the daemon-derived path and the exact bytes, which is stronger than the old `exists()` check.

What the cluster does not deliver is the H1 test scope the spec assigns to integration tests:

- Spec §6 (configuration smoke) is owned by hg-45e.12, which is still OPEN. There is no `tests/instruction_smoke.rs` and no equivalent test in `integration_sweep.rs`.
- The end-to-end H1 sweep is owned by hg-45e.16, also still OPEN. Its required test that `is_instruction_source` is false for the derived summary path is absent, and so is the writer inventory.

Nothing in `tests/` exercises any of these at binary level:

- `plan instructions set|clear` → `apply` → `seat --instructions` / `--hook-prompt` for the global, teamspace and seat scopes;
- `setup claude|codex` → `doctor harness_gate` → uninstall;
- the dirty-working-tree non-effect on the render.

The cli_sweep handler surface also does not list the new commands (`plan instructions set/clear`, `instructions show`, `seat --instructions`, `request complete --summary-from`), so a handler regression in those would not be caught there. `src/cli/mod.rs` covers parsing for `instructions show global` only.

## Verdict
not ready (spec §6 configuration smoke and the H1 end-to-end integration tests, hg-45e.12 and hg-45e.16, are still open and absent from tests/)

## Must fix before landing
- Land hg-45e.12: one default-tier integration test that runs plan instructions set -> apply (relay) -> seat --instructions for each of global, teamspace and seat, and setup -> doctor harness_gate ok -> uninstall -> doctor skips for claude and codex in isolated config dirs (spec §6).
- Land hg-45e.16: exercise the main H1 flows end to end on the merged tree, including the hook render containing applied text and excluding an uncommitted working-tree edit (AC5/AC6), and assert is_instruction_source is false for the derived summaries/<tr>-<a>-<b>.md path.

## Untested scope
- cli_sweep handler surface omits plan instructions set/clear, instructions show, seat --instructions and request complete --summary-from, so binary-level handler regressions in the new commands are not caught.
- Golden path does not exercise the identical-retry no-op or summary_conflict at integration level (covered only in src/transcripts/tests.rs unit tests).
- Golden path does not assert that the summary write and request completion form a single commit (AC2 "in one commit").
- The content write stub check says "nothing written" but does not assert that notes/a.md is absent from the seat folder or that the graph head did not move.
- No integration test applies an instructions_edit and then undoes it through the composed daemon (undo is covered only in src/undo/tests.rs).

## Deferred OK
- The single golden failure at integration_sweep.rs:621 in evidence/hg-45e.9/features.log did not reproduce in 5 reruns at head; watch for flakiness in the summary-follows-folder-on-undo assertion.
- The wiring sweep's `K::ContentWrite => continue` keeps the legacy variant out of `all` by design; acceptable as long as the legacy unit tests in src/writer/tests.rs and src/model/tests.rs stay.
