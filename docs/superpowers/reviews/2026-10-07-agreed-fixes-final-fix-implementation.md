# Final-review correction implementation — agreed fixes, 2026-10-07

## Scope and provenance

- Sole implementation worker on coordinated branch `agreed-fixes-2026-10-07` from `b1a35d6ec1d72949dfee01a0bf33073b652c4b09`; binding scope is `AGREED-FIXES-HANDOFF.md` and the complete whole-branch review, whose only finding is Important F1.
- Preserved the original `.superpowers/sdd/2026-10-07-agreed-fixes/final-review.md` byte-for-byte as `docs/superpowers/reviews/2026-10-07-agreed-fixes-final-review.md`. Earlier task and October 5 review artifacts remain untouched.
- Followed systematic debugging, test-driven development (including writing-good-tests), and verification-before-completion skills. No subagents/reviewers, checkout changes, merge/push, release publication, live service/instance access or upstream changes.
- Controller-approved collision boundary: reject ambiguous template create/edit documents; do not add a rejection to application hydration of existing stored templates. Exact ruling is retained in the release handoff.

## Root cause and correction

The existing mutation wrote member instructions only when `agents_md` was present, using the incoming display-name slug. A same-ID rename with omission left its instructions at the old path while `/seat` followed the new path. Undo correctly treated name and instruction edits as different fields, but its inverse omitted instructions; applying it exposed stale old-path text after a later specialization edit.

`src/templates/kinds.rs::write_agents` now captures all current member instruction bytes keyed by stable member ID before any path mutation. It removes prior member instruction paths, then writes the resulting members: explicit text/empty clears win, while omission carries the captured bytes of that ID. Missing instructions and new IDs cannot inherit a destination's former owner's text. The edit caller supplies the resolved `after` document, which includes IDs assigned by existing name matching even when input omitted them. Snapshotting all sources before writing any destination makes swaps independent of iteration order. Only instruction files are changed; opaque sibling files are untouched.

Forward edits and undo both use this existing mutation path. No change was made to inverse field restoration, drift checks, historical compensation parsing, action/operation layouts, application mappings, native transcripts/history, reusable-template identity, profession registries or generated briefs. The carried bytes are `Vec<u8>` read directly from the tree, with no decode/re-encode step. Existing compensation strings and their historical semantics are retained.

`src/templates/document.rs::validate` rejects distinct member names that normalize to the same instruction path. The error names both members and the path and asks for a rename. This covers create/edit plan documents, including inverse edits; the schema, stored-record readers and application hydration are unchanged. The field documentation now states ID-based omission and explicit-empty clear semantics.

## Meaningful regressions and red/green

All tests exercise real isolated Git-backed plans/mutations, without mocking the code under test. Bootstrap fixtures now register the real undo kind.

1. `bootstrap::tests::member_rename_omission_keeps_live_specialization_and_immediate_undo`: both canonical and participating-application members; preview identifies only the name field, resulting `/seat` references and exact committed/working-tree bytes follow the name, old path is removed, immediate undo restores the correct path. Stable member/application/seat/clone correspondence is asserted.
2. `bootstrap::tests::member_rename_undo_preserves_later_instruction_edits_and_clears`: canonical and participating members crossed with later nonempty text and explicit clear; undo only the rename, preserving current specialization, unrelated root instructions and responsibility. Exact fixture bytes include CRLF, Unicode and trailing whitespace.
3. `bootstrap::tests::member_rename_explicit_clear_undo_restores_original_specialization`: rename plus explicit clear removes instructions, including the old path; undo restores original bytes/reference.
4. `templates::tests::member_rename_swap_preserves_each_identity_and_undo`: two IDs swap display names; each destination receives the other path's correct source bytes, stable mappings remain unchanged, and undo swaps back.
5. `templates::tests::member_rename_into_departing_path_does_not_inherit_other_identity_text`: destination formerly belongs to a departing member with text; test both retaining another ID with no instructions and replacing with a new ID. Neither inherits stale text; undo restores the departed member's original text.
6. `templates::tests::member_rename_rejects_colliding_instruction_paths_before_writing`: case and punctuation normalization collisions in edit/create fail before any committed state change with actionable diagnostics.

First test execution caught a fixture typo (`startup = dormant`, where the document enum uses `deferred`) in two tests. That setup was corrected before implementation. The subsequent pre-fix execution compiled and all six tests failed at the expected assertions: missing new live reference, stale old bytes after undo, lingering old path after clear, destination inheritance, collision plan accepted, and swapped bytes still belonging to previous path owners. This corrected run is the red evidence. Production implementation then made all six pass.

| Command | Result | Evidence |
|---|---|---|
| `cargo test --offline --lib member_rename -- --test-threads=1` (corrected pre-fix) | exit 101; 0 passed / 6 failed / 617 filtered | `/private/tmp/hg-final-fix-red.log` |
| same command after correction | exit 0; 6 passed / 0 failed / 617 filtered | `/private/tmp/hg-final-fix-green.log` |
| `cargo test --offline --lib` | exit 0; 623 passed / 0 failed / 0 ignored; 127.98s | `/private/tmp/hg-final-fix-lib.log` |
| `cargo test --offline --test daemon_composed --test daemon_ipc --test integration_sweep` | exit 0; 33 passed / 0 failed / 0 ignored (14 + 12 + 7); 5.60s / 1.46s / 5.41s | `/private/tmp/hg-final-fix-integrations.log` |
| `cargo fmt --all -- --check` | exit 0 | final checks |
| `git diff --check`; staged whitespace check excluding byte-preserved review | exit 0; exception detailed below | final checks |

The first full staged `git diff --cached --check` exited 2 and stopped the chained commit before it ran: the only finding was the original two trailing spaces on the Base line of the byte-preserved review, an intentional Markdown hard break. The artifact must remain byte-identical, so it was not normalized. `git diff --cached --check -- . ':(exclude)docs/superpowers/reviews/2026-10-07-agreed-fixes-final-review.md'` passes for every new source/test/report/handoff change, and `cmp` independently verifies the preserved review. This is a documented preservation exception, not a blanket clean full-diff claim.

All Cargo invocations were sequential. Test runs used `require_escalated` because the authorized isolated tests need temporary Git repositories and sockets. Log files are local validation evidence, not committed artifacts. The complete earlier feature matrix and real-threads tests were not repeated: this correction changes no conditional-feature/runtime adapter path. No native-agent or live-runtime validation is claimed.

## Compatibility, self-review and remaining ownership

The actual migration is a reviewed template edit's current instruction-path update, not a scan or rewrite of history. Existing root omissions, explicit clears, retained names/IDs and unrelated later field edits keep their semantics. Removing a member removes its active instruction path; the existing before-document compensation retains restoration text. A destination is populated only from its current stable identity or explicitly supplied text, never an orphan file. Existing stored colliding templates remain readable and application hydration unchanged, but any create/edit/inverse document with colliding names must resolve the ambiguity before it can apply. Previously lost information cannot be reconstructed by this correction; no claim of retroactive recovery from already-broken old-name paths is made.

Self-review checked all write_agents callers, resolved-ID handling, plan diff/compensation omissions, swap read-before-write ordering, clear/absence/new-ID behavior, canonical/participating bootstrap lookup, conflict preservation, exact bytes, and stable mappings. Tests would fail if omitted instructions stayed at the old path, inverse used historical text, explicit clears resurrected a destination, swaps read a previously overwritten source, or collisions were accepted. The 623-test library run passed, including `review_legacy_added_member_compensation_without_agents_undoes`, `layout_compat_legacy_delivery_and_old_action_undo`, and other historical omission/operational compatibility tests. The three integration targets contributed 33 additional passing tests. All 656 covering tests passed without failures or ignored tests. Production change is limited to instruction persistence and document validation; no unrelated refactoring.

Final source/tests/release handoff and proper evidence copies are committed together with subject `fix(templates): preserve member instructions across renames`. The controller receives the resulting exact SHA rather than embedding a self-referential commit ID here. This file's committed copy is `docs/superpowers/reviews/2026-10-07-agreed-fixes-final-fix-implementation.md`; scratch remains ignored. Scoped final re-review, final review acceptance, integration/push and release ownership transfer remain controller-owned and pending. Self-review does not replace that independent review.
