# Final review: cluster 14 (src-undo), epic hg-45e

Invocation: inv-p3b-20261031T222244. Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify`; review source bound in the ledger before this write).

Inputs: slices 000078-000082 (src/undo/candidates.rs, compensate.rs, mod.rs, preview.rs, tests.rs) and digest.md. Spec: design doc §2.5 (Undo), the UndoKind/ActionKind component entry, and acceptance item 3 ("Applied edits are undoable, and undo of a drifted file reports a conflict"). src/plan/instructions.rs and src/store/instruction_source.rs were opened only to check the inverse-args contract and target resolution.

## Spec conformance

- `ActionKind::InstructionsEdit` is added to `is_undoable`, and `summarize` gets an arm ("edited instructions <file> of <object>"). Meets §2.5.
- `UndoKind` holds an `instructions: Arc<dyn OrgKind>` field, wired in `register_kinds` from `RequestKind::InstructionsEdit`. This matches the component entry ("UndoKind holds an Arc to it").
- `Inverse::Ready` now carries the inverse kind's args as a `serde_json::Value`, so the template and instruction inverses share one enum. This is what the component entry allows. The template path still builds the same args through `template_edit_args`, and the legacy test was adapted to match.
- `instructions_inverse` reads `{file, before, after}` from the compensation and `affected[0]` as the target object. It maps Graph/Teamspace/Seat to a scope and resolves the path at the current tree. When the current file equals `after`, it emits `set before` (or `clear` when `before` is absent). Otherwise it emits `Conflict` (`repair_required`), the same way `template_inverse` does. This meets both §2.5 bullets.
- The args match InstructionsKind's `Args` contract: `action`, `scope` word, `target` (an id string, which `resolve_scope` accepts), `file`, `content`. Plan and mutate both delegate to the instructions kind. They share the reserved `act` id, and `finalize` turns the record into `Undo`, as the template-edit path already does.
- Plan-time and apply-time drift are both covered. Plan returns `repair_required`. Mutate recomputes the inverse and rejects `repair_required`, and the generic `same_effects` staleness check runs first anyway.

## Verification performed

- I created a review-owned detached worktree at head (seeded from the integration target, submodule initialised from the local copy) and ran `cargo test --offline --locked --features test-support --lib undo::`: 24 passed, including the four new instruction-undo tests (removing a new file, restoring the previous text, restoring after clear, drifted file returns repair_required with apply refused and the file untouched).
- I ran a throwaway probe test in that worktree (not committed; the worktree is removed). Undo of a global-scope set and of a teamspace-scope set both plan and commit cleanly. Undo of an edit on a seat that was retired afterwards fails at plan time with `retired_object: seat ... is retired` rather than `repair_required`. The candidate summary for a global edit shows the raw graph id (`... of gr_...`), because `Names` does not map GraphId to a name.
- Caller checkout: HEAD stayed at bce6589 and `git status` was clean before and after. No caller files were edited.

## Observations (non-blocking)

- Undo on a retired seat or teamspace is a plan error, not a conflict. That is correct behavior, but it is not tested and the spec does not mention it.
- Undo of an instruction edit goes through `InstructionsKind::plan`, which re-runs `check_content` on the restored `before` text. Any `before` content that is inadmissible today (for example a pre-gate legacy file) makes the edit un-undoable with an Invalid error instead of a conflict. This is defensible because the gate should not restore inadmissible content, but it is undocumented.
- `instructions_inverse` compares against `from_utf8_lossy` of the current file. This was already noted as a deferred minor: a non-UTF-8 file compares unequal and returns a conflict, which is the safe direction.
- The candidate summary for global-scope edits prints the graph id instead of a label such as "global". This is cosmetic.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No committed test covers undo of a global-scope or teamspace-scope instruction edit (probe showed both work; only seat scope is in src/undo/tests.rs).
- No test asserts the `summarize` text for InstructionsEdit candidates ("edited instructions <file> of <object>").
- No test covers undo of an instruction edit whose target seat or teamspace was retired afterwards (plan fails with retired_object, not repair_required).
- No test covers drift between undo plan and apply (mutate-side `Inverse::Conflict` -> reject repair_required path); only plan-time drift is tested.
- No test covers undo when the recorded `before` text is no longer admissible under check_content.

## Deferred OK
- Candidate summary for global-scope edits shows the raw graph id instead of a "global" label (src/undo/candidates.rs summarize InstructionsEdit arm).
- instructions_inverse uses from_utf8_lossy for the current file; non-UTF-8 content yields a conflict (safe direction, already listed in deferred minors).
- Undo of an edit whose before text is now inadmissible fails as Invalid rather than repair_required; acceptable but undocumented.
