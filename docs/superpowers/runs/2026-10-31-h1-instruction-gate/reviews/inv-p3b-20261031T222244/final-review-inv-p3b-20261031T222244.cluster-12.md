# Final review: cluster 12 (src-threads), epic hg-45e

Invocation: inv-p3b-20261031T222244. Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify <sha>^{commit}`; review source bound via dispatch-text.mjs bind-review-source, exit 0). Caller checkout observed on branch super-auto/h1-instruction-gate, clean status; no caller files changed by this review apart from the authorized ledger binding and this artifact.

## Scope

Cluster files: `src/threads/effects.rs` (slice `slices/000074.diff`). The whole cluster is one changed line: the body of the `worktree_dirty` Notify effect in `ThreadsSource::dirty_wants`.

Before: "... Commit your change through herdr-graph content write or discard it."
After: "... Commit your change through a plan (herdr-graph plan instructions set …, then herdr-graph apply) or discard it."

## Spec check

Spec §1.5 (docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md, line 37) lists "`src/threads/effects.rs:572` message text" among the places to update when `content write` is removed. The change meets that: the notification no longer points agents at a removed command, and it routes them through the reviewed path (plan, then `herdr-graph apply`, which the harness ask rule gates per §3.3). The wording matches the CLI stub guidance in `src/cli/content.rs` `STUB_MESSAGE`. No other text in `src/threads/` references `content write` at head (`git grep` over src shows only comments, tests and the stub).

Notification identity (`key: notify:worktree_dirty:{op}:{path}`) is unchanged, so dedup across restarts and the journal known-id check behave as before. Severity, purpose, fencing and ordering are untouched.

## Observations

- The guidance names one plan kind for every dirty file in a seat folder. `dirty_wants` fires for any file under an active seat's folder, including non-instruction files: the existing test `worktree_dirty_notifies_owning_seat_once_per_file_per_op` (src/threads/tests.rs:1134) uses `notes/x.md`, and `seat.toml` or `summaries/*.md` can also land there. `plan instructions set` accepts only `AGENTS.md` and `rules/<n>.md` (`InstructionFile::parse` / `check_name` in src/store/instruction_source.rs), so for those paths the suggested command rejects with a clear error. The "…" and "a plan" wording reads as an example, and the agent gets a precise rejection, so this is low impact. A conditional wording keyed on `instruction_source::is_instruction_source` would be more precise.
- With `content write` gone, free-form data files in a seat folder (clone notes) have no graph write path at all, so for those "commit your change" has no route; "discard it" is the only working option. This follows from the epic's design (§1.5, data listed as `data` only) rather than from this cluster, and composition is another reviewer's remit.
- Tests: existing worktree_dirty tests filter on "left in place" and on path/op, so they still pass and still cover the effect. No test asserts the new guidance text or that the body no longer mentions `content write`.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No assertion that the worktree_dirty notification body carries the new plan/apply guidance or omits `content write` (src/threads/tests.rs worktree_dirty tests match only "left in place", path and op).

## Deferred OK
- worktree_dirty guidance in src/threads/effects.rs:572 suggests `plan instructions set` for every dirty seat-folder file, but that command rejects non-instruction paths such as notes/x.md, seat.toml or summaries/*.md; consider wording conditioned on `is_instruction_source`.
- Seat-folder data files (clone notes) have no commit path after `content write` removal, so the "commit your change" half of the message has no route for them; a design-level follow-up, not this cluster's change.
