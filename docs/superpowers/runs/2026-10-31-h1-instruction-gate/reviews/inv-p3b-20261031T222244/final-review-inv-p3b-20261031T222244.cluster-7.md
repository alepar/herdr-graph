# Final review: cluster 7 (src-model), epic hg-45e

Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify`). Review source bound via dispatch-text.mjs bind-review-source (exit 0).

Files: src/model/{action,b64,change,common,graph,ids,mod,tests}.rs (slices 000051-000058).

## Spec checks

- Spec 1.4 / components "b64_encode/b64_decode move to a shared module": src/model/b64.rs is a verbatim move of the base `src/bootstrap/content.rs` lines 235-280 (diffed; only the header comment changed). Callers: src/cli/request.rs (encode `summary_b64`), src/transcripts/mod.rs and src/transcripts/mutations.rs (decode). Round-trip, tail-length and invalid-character tests remain in src/bootstrap/tests.rs (~936-947). Met.
- Spec 1.5 / global constraint "keep RequestKind::ContentWrite and Category::Content as legacy variants": both kept with doc comments; `category()` still maps ContentWrite to Content. New test `legacy_content_write_records_deserialize` covers a JSON ChangeRequest and a TOML OperationRecord round-trip with `kind = "content_write"`. Met.
- Spec 2.3 / GraphId: `IdKind::Graph` with prefix `gr_` added to `IdKind::ALL` (length 14); `GraphId::from_instance_id` = `gr_` + instance_id verbatim, validated by the shared `validate_body` (26 Crockford chars + ULID parse), so a non-ULID instance_id is rejected; `GraphRecord::graph_id()` wraps it. `AnyId::parse` relies on no prefix being a prefix of another; the new test asserts that pairwise over ALL. `store/init.rs` mints instance_id with `ulid::Ulid::new().to_string()` (uppercase), so real instances always form a valid id. Round-trip through `AnyId::parse` and `kind()` is tested; lowercase, empty and "inst" are rejected. `layout::locate` handling (src/store/layout.rs:329-348) is outside this cluster. Met.
- Spec 2.5 / ActionKind::InstructionsEdit: variant added; serde is by name (snake_case), so inserting it mid-enum does not disturb existing records. Undo wiring (candidates/compensate/preview) is in other clusters. Met at model level.
- InstructionsKind's `RequestKind::InstructionsEdit`: added; falls to `Category::Organizational` via the `_` arm, so `requires_confirmation()` is true. That is the behavior the gate needs (organizational kinds go through plan -> confirm -> apply).

## Observations

- `request_kind_categories` does not list `RequestKind::InstructionsEdit` among the confirmation-requiring kinds. The `_` wildcard makes this correct today, but the instruction gate's core property (instruction edits require confirmation) has no model-level assertion; a future explicit arm mapping it elsewhere would not be caught here.
- `b64_decode` is lenient (accepts input whose length is not a multiple of 4, ignores leftover bits, strips any number of trailing `=`). Pre-existing behavior moved verbatim; the only producer is herdr-graph's own CLI, and decoded summaries are then UTF-8/content checked downstream. Not a defect for this epic.
- `RelPath` doc comment updated to drop the `content write --rel` reference; accurate.
- I did not re-run tests: a review-owned worktree under the scratchpad could not reflink-seed target/ (cross-device), and the epic forbids cold builds outside the seeded target. The worktree was removed. Committed evidence evidence/hg-45e.9/full.log and evidence/hg-45e.18/lib-tests.log show the lib suite green (695 passed at the later run). Caller checkout observed unchanged: HEAD bce6589, status clean.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- src/model/tests.rs request_kind_categories does not assert RequestKind::InstructionsEdit.requires_confirmation() (correct today only via the `_` arm of RequestKind::category)
- src/model/b64.rs has no tests in the model module itself; coverage lives in src/bootstrap/tests.rs and does not exercise malformed-length or non-canonical-padding input

## Deferred OK
- src/model/b64.rs b64_decode leniency on malformed length and trailing bits is pre-existing, moved verbatim, and only fed by herdr-graph's own CLI
- Tests in this cluster were not re-run by this reviewer (cross-device reflink seed unavailable); relying on committed lib-suite evidence
