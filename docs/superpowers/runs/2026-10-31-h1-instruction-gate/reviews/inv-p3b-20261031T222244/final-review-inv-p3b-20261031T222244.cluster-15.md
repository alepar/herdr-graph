# Final review: cluster 15 (src-writer), epic hg-45e

Invocation: inv-p3b-20261031T222244
Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify <sha>^{commit}`; review source bound in the ledger before this write).
Inputs: `src/writer/mutation.rs`, `src/writer/tests.rs` (slices 000083.diff, 000084.diff), digest.md, epic design `docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md` (§1.5, acceptance criterion 1).

## Scope of the change

- `mutation_key` no longer treats `RequestKind::ContentWrite` as a sub-keyed kind; only `Observed` and `Bookkeeping` derive `<kind>.<sub>`. Doc comments updated to match.
- New test `legacy_content_write_op_fails_with_no_mutation_registered`: `WriterCore::admit` refuses a new `ContentWrite` request with "no mutation registered", and a row journaled directly via `Journal::admit` (simulating a pre-upgrade row) drains to `StepOutcome::Failed` / `OpState::Failed` with a rejection reason containing "no mutation registered", with store head unchanged.
- `registry_rejects_duplicates_and_keys_use_sub` assertion message updated.

## Spec check

- Spec §1.5: "A journaled ContentWrite op left unapplied at upgrade fails through the writer's existing 'no mutation registered' path". Met. At head, `src/writer/mod.rs:320-325` looks up `mutation_key(req)` and calls `finish_failed` on a miss. The test exercises exactly this path end to end through `drain()`.
- Acceptance criterion 1, "No ... `content_write` mutation is registered". The test shows admission refuses the kind. The registration removal itself lives in another cluster (`src/bootstrap`). It is checked there by `content_write_has_no_daemon_command_and_no_mutation` in `src/bootstrap/tests.rs:899`.
- Dropping `ContentWrite` from `has_sub` is safe. The pre-H1 producer (`src/bootstrap/content.rs` at base, around line 205) never set `sub`: its key was the bare "content_write". With no mutation registered for either form, any legacy row (with or without `sub`) still misses the registry and fails. The change only removes a dead branch, and history deserialization is untouched (`RequestKind::ContentWrite` stays in `src/model/change.rs:36`).

## Verification run

- Review-owned detached worktree at head (scratchpad `rev-c15`, own `CARGO_TARGET_DIR`): `cargo test --offline --lib writer::` gave 46 passed, 0 failed. Targeted: `legacy_content_write_op_fails_with_no_mutation_registered`, `registry_rejects_duplicates_and_keys_use_sub` and `model::tests::legacy_content_write_records_deserialize` all pass.
- Caller checkout observed before and after: HEAD bce6589afead4c031ac9ecfa3a094c9a5c4be161, `git status --short` clean.

## Observations

- A legacy pre-H1 row that carried an `expect` blob precondition goes through `recheck_relied_on` (`src/writer/mod.rs:313`) before the mutation lookup. If that blob drifted, the row ends `Rejected` (stale), not `Failed`. Either way nothing commits and the outcome is terminal. The spec's wording ("fails", "doctor's failed-ops check surfaces it") covers only the non-drifted case. This is harmless, but such a row would not show up in doctor's failed-ops check. The legacy test uses `relied_on: vec![]`, so it does not cover this case.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- Legacy ContentWrite row with a non-empty `relied_on` whose blob has drifted: ends Rejected via `recheck_relied_on` before the no-mutation path, so doctor's failed-ops check would not list it (not covered by the legacy test).
- Doctor's failed-ops check surfacing a legacy ContentWrite Failed row is not exercised in this cluster (belongs to doctor scope).

## Deferred OK
- Once the `content write` CLI stub is removed next release, consider whether `RequestKind::ContentWrite` keeps needing writer-side tests or only deserialization tests.
