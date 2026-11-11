# Final review, cluster 1 (bootstrap), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298 (both verified with `git rev-parse --verify`). Caller checkout observed at HEAD b9caa7f, clean status. Review source bound in the ledger before this file was written (bind-review-source exit 0).

Files: src/bootstrap/instructions.rs, src/bootstrap/setup_claude.rs, src/bootstrap/tests.rs (slices 000049-000051). Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md at head, sections 1.2, 2.3, the InstructionManifest and LoadedInstructions component entries, and the Requirement baseline (B1-B8, S1).

## What the cluster delivers

- InstructionManifest (spec component entry): `Part` gains `bytes` and `digest`, both set only in `Collector::add` from the newline-terminated body; `part_digest` is `sha256:` plus lowercase hex. The rendered text is unchanged: `block()` still emits label, path and body, and the digest is never printed by the renderer. A unit test pins the SHA-256 test vector for "abc", the byte count of a multi-byte body and the drop of a blank part.
- HookFit gains `cut_body_bytes`: the number of bytes of the cut part's body that the hook delivered, header excluded. The header length is recomputed with the same format `fit_hook` uses for its blocks (`"\n" + "## {label} — {path}\n"`). Because `kept` always ends at a `\n` and the header is a whole prefix, the value lands on a char boundary of `body`. `cut_body_bytes_is_prefix` proves that the prefix appears in the hook text and the next byte does not. `inspect::loaded::delivered` (inspect cluster) consumes it to digest the delivered prefix, as spec 2.3 requires ("the delivered prefix for the cut part").
- Hook commands (spec 2.3 "Where the hook gets the native session id"): the Claude command is exactly the spec's `p=$(cat); printf '%s' "$p" | <bin> seat --hook-prompt --stdin-payload; printf '%s' "$p" | <bin> session-report --from-hook claude # herdr-graph-owned`, and Codex is `<bin> seat --hook-prompt --codex --stdin-payload # herdr-graph-owned`. I ran the Claude command under `sh -c` against a stub binary whose path contains a space: both subcommands got the same payload byte for byte (only the trailing newline is stripped, which JSON parsing tolerates), and the exit status was 0.
- Setup rewrite of an older owned hook: `install_for` already replaces any owned group that differs from the desired one (`HookChange::Updated`). The new test `rerun_setup_replaces_outdated_hook_in_place` covers both harnesses with an old command beside a user group: the user group survives, there is exactly one owned group, and it carries `--stdin-payload`.
- `owned_hook_current` plus per-target `current` predicates feed doctor's `harness_hook` check (daemon cluster, tested in doctor.rs around line 1075-1110). The predicates ignore the binary path, as documented.

## Requirement baseline trace (cluster share)

- B2 (which files are loaded into context per seat): this cluster supplies the production pieces at the hook entry point: the per-part digest and bytes, the delivered-prefix length, and the setup-installed hook commands that carry `--stdin-payload` so the hook can bind the record to a session. The record writer, the status rule and the views live in cli/seat.rs, inspect/loaded.rs, inspect/roster.rs and inspect/stack.rs. That path leaves this cluster and is the seam reviewer's to trace. Within the cluster, nothing blocks it.
- B5 (staleness shown, not notified): the cluster adds no notice and no Git write. The setup hook rewrite touches only harness settings, which setup already owned.
- B1, B3, B4, B6, B7, B8 and S1 have no code in this cluster.

## Edge cases examined

- When the marker naming every omitted label is long enough that the budget for the cut part is smaller than its header line, `kept` becomes "" or "\n" and `cut_body_bytes` saturates to `Some(0)`. The loaded record then lists that part as `cut` with 0 bytes and the digest of the empty string, although the agent never saw its header. HEAD-side comparison uses the same `fit_hook`, so status stays consistent (no false stale). This needs a stack with enough parts that the omitted-label marker approaches 9000 bytes. Recorded under Deferred OK.
- The header length in `fit_hook` duplicates `block()`'s format string. If someone changes one without the other, `cut_body_bytes` drifts. `cut_body_bytes_is_prefix` would catch that drift, so this is a maintenance note, not a defect.
- The Codex predicate `contains(" seat --hook-prompt --codex --stdin-payload ")` relies on the owner marker following a space. The emitted command always has that space.

## Findings

- none

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test drives `fit_hook` into the degenerate cut where the omitted-label marker leaves less budget than the cut part's header (cut_body_bytes saturates to 0); behavior there is reasoned, not exercised.

## Deferred OK
- A cut part whose header line was not delivered (budget below header length) is still recorded as `cut` with 0 bytes and an empty-body digest instead of being treated as omitted; consistent on both sides of the comparison, so no false staleness, and reachable only with a marker near the 9000-byte cap.
- `fit_hook` recomputes the cut part's header length with a format string duplicated from `block()`; a shared helper would remove the drift risk that `cut_body_bytes_is_prefix` currently guards.
