# Final review, cluster 1 (bootstrap), epic hg-blw

Invocation inv-b9c53a9d-0c9e-4100-99aa-2184b61f229c. Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 2dfa656c7f98715fe66ca53c253de19f369a857d (both verified with `git rev-parse --verify`; caller checkout at head 2dfa656, clean, unchanged during review). The source binding via `dispatch-text.mjs bind-review-source` exited 0 before this file was written.

Files: src/bootstrap/instructions.rs, src/bootstrap/setup_claude.rs, src/bootstrap/tests.rs (slices 000031-000033). Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md at head (§2.3 loaded record, §5 InstructionManifest and LoadedInstructions, Requirement baseline B1-B8, S1).

## What the cluster changes

- InstructionManifest: `Part` gains `bytes` and `digest` (`sha256:<hex>` of the rendered, newline-terminated body), both computed once in `Collector::add` after `newline_terminated`, as spec §5 requires. `part_digest` is public so the loaded record (src/inspect/loaded.rs) hashes a cut prefix the same way. The digest is the body digest, not a Git blob id, matching the spec's "Digest choice".
- `HookFit.cut_body_bytes`: the number of body bytes of the cut part the hook delivered, header excluded. `kept` always ends at a `\n` (or is empty), so the offset is a UTF-8 char boundary and `p.body.get(..n)` in loaded.rs never falls back. The header length is recomputed with the same `\n## {label} — {path}\n` shape that `block` emits; the unit test `cut_body_bytes_is_prefix` pins that the prefix is exactly what reached the hook text and that one more byte is not.
- Hook commands: the Claude command is now exactly the spec §2.3 string, `p=$(cat); printf '%s' "$p" | <bin> seat --hook-prompt --stdin-payload; printf '%s' "$p" | <bin> session-report --from-hook claude # herdr-graph-owned`; Codex is `<bin> seat --hook-prompt --codex --stdin-payload # herdr-graph-owned`. Both use `sh_quote` for the binary path, so a path with spaces stays one word. `printf '%s'` is POSIX, and `$(cat)` dropping trailing newlines does not affect a JSON payload.
- `owned_hook_current` plus `current: fn(&str) -> bool` on `Target`: it reports whether every owner-marked SessionStart command has the current shape, ignoring the binary path. Its one caller is `doctor::harness_hook_checks` (daemon cluster), which turns `Some(false)` into "outdated herdr-graph hook; run herdr-graph setup <name>".

## Requirement tracing for this cluster

- B2 and B5 (loaded files per seat, staleness shown): this cluster supplies the per-part identity (digest, bytes), the cut prefix length and the stdin-sharing hook commands. Production path: `herdr-graph setup claude|codex` → `install_for` → `(target.group)(binary)` → `hook_command`/`codex_hook_command` written into settings.json or hooks.json → the harness runs it at SessionStart → `seat --hook-prompt --stdin-payload` (src/cli/seat.rs, cli cluster) → `inspect::loaded::delivered` uses `fit.cut_body_bytes` and `part_digest` (inspect cluster). The steps after the installed command are the seam reviewer's to trace. Out of cluster, tests/inspect_loaded.rs runs the real `hook_command(bin_path())` string under `sh -c`, so the installed string itself is exercised in a test.
- Upgrade path for existing installs (spec §2.3: an older hook "writes `native_session_id: null` until `setup` rewrites the owner-marked hook"): `install_for` matches a single owned group that differs from `desired` through the `[first, rest @ ..]` arm and replaces it in place, so a re-run of setup does upgrade a pre-`--stdin-payload` hook. `rerun_setup_replaces_outdated_hook_in_place` covers both harnesses beside a user's own group, which survives. The Codex re-trust notice on `Updated` lives in src/cli/setup.rs (cli cluster) and prints for that case.
- B1, B3, B4, B6, B7, B8, S1: this cluster contributes nothing to them, and none of them depends on it.
- The text agents receive does not change (spec §6 invariant): `Collector::add` passes the same newline-terminated body, and `seat --json` builds `.instructions.parts` field by field (src/bootstrap/mod.rs:375), so the new `Part` fields do not leak into that output.

## Edge cases checked

- Multibyte bodies: `part_digest_and_bytes` asserts `bytes` counts UTF-8 bytes ("héllo\n" = 7) and that blank parts are still skipped.
- Cut part with no body line delivered: if the single most specific part's first line does not fit the remaining budget, `kept` is the header only and `cut_body_bytes` is `Some(0)`. The record then lists it as `cut` with 0 bytes and the digest of "", and HEAD-side recomputation goes through the same `fit_hook`, so status stays consistent.
- Command currency matching: `claude_command_current` requires the `p=$(cat); ` prefix and ` seat --hook-prompt --stdin-payload; `, and `codex_command_current` requires ` seat --hook-prompt --codex --stdin-payload ` (the trailing space comes before the owner marker). Both match the generated commands for any quoted binary path, and both reject the pre-change commands used in the doctor and setup tests.

## Findings

- none

## Concerns

- `fit_hook` recomputes the cut block's header length with a format string copied from `block`. If one of them changes and the other does not, `cut_body_bytes` drifts. The unit test `cut_body_bytes_is_prefix` would catch that, so this is a maintenance note, not a defect.
- When the cut part cannot fit even its own header line (only reachable when the omitted-label marker nearly fills the 9000-byte cap), `kept` is just "\n" but `cut_body_bytes` is still `Some(0)`, so the record lists a `cut` part whose header the agent never received. This is consistent on both sides of the status comparison, and in practice it cannot be reached.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- The bootstrap unit and integration tests were not run at head 2dfa656 in this review: I did not build in hg-build, to keep the caller worktree's target/ untouched, and progress.md records an earlier hg-build outage. The correctness claims above come from reading the code and the tests' assertions.
- `owned_hook_current`'s error path (an unreadable or unparseable settings file) has no direct test; only the None, Some(true) and Some(false) paths are covered, through doctor_flags_outdated_hook in src/daemon/doctor.rs.
- `fit_hook` with a cut part that delivers zero body bytes (the header only, or not even the header) has no test.

## Deferred OK
- Derive the cut header length from `block` (e.g. `block(p).len() - p.body.len()`) instead of a second format string, so the two cannot drift.
- Return `cut_body_bytes: None` (or omit the cut part) when not even the cut part's header fits, so the record never lists a part whose header was not delivered.
