# Task 2 — Record-layout compatibility

## Implementation

New records use these exact destinations:

- `mutations/graph-changes/<yyyy-mm>/<op-id>.toml`
- `mutations/undoable-actions/<yyyy-mm>/<action-id>.toml`
- `mutations/transcript-processing/<request-id>.toml`
- `transcripts/<slugified-team-name>-<full-team-id>/<slugified-seat-name>-<full-seat-id>/<transcript-id>.toml`

The three operational path helper signatures remain stable. `transcript_registration_record` accepts initial team/seat display names and full stable IDs; `transcript_record` remains explicitly documented as the legacy path helper, with no production registration caller left using it. Shared `slugify` handles unsafe punctuation, separators, non-ASCII input, empty names, and length limits.

New transcript records persist optional `capture_attribution = { teamspace, teamspace_name, seat_name }` (serialized as a TOML table). This is fixed at initial registration. Existing records without it remain readable and retain unknown historical attribution. Neither registration refresh nor processing backfills current organization into old records. Delivery source labels use the captured seat name, or the stable seat ID plus `(capture name unknown)` for legacy records; destination routing still follows existing current-organizational rules.

Shared enumeration and ID lookup discover both layouts and reject duplicate IDs explicitly with both conflicting paths. Existing request acknowledgement, delivery, unresolved updates, merge keeper/merged-away updates, and completion resolve the existing location. Transcript eligibility/coverage/gap updates likewise resolve the existing record instead of reconstructing its destination. Reconciler delivery now resolves legacy request IDs. Existing undo and operation reassignment already resolve locations; the new layout discovery makes their old records available without rewriting history. Newly committed operation/action records use the new roots. SQLite journal lifecycle behavior is unchanged.

No eager migration, deletion, or relocation of historical records is performed. Native transcript content is neither copied nor changed.

## Regression evidence

Commands were run sequentially with sandbox escalation for isolated Git/socket fixtures. No live instance, external thread state, shared daemon, or real agent was used.

- Initial RED: `cargo test --offline --lib store::tests::` — **27 passed, 3 failed** on expected old request/action/operation path assertions.
- Initial RED after correcting test fixture registration: `cargo test --offline --lib layout_compat_` — **0 passed, 2 failed**: registration still used the legacy seat-only path; merging a legacy request attempted a second record at the new destination and failed duplicate protection.
- GREEN: `cargo test --offline --lib store::tests::` — **30 passed**, including both-layout request lookup, safe transcript slugs/full IDs, and duplicate transcript/request/action/operation rejection.
- RED for historical delivery label: `cargo test --offline --lib layout_compat_` — **3 passed, 1 failed** because legacy delivery invented a capture name from the renamed current seat.
- GREEN: `cargo test --offline --lib layout_compat_` — **4 passed**, including old template-edit compensation undo, original action path preservation, old operation bytes preserved, legacy request delivery, request keeper/merged-away location preservation, missing-request rejection, mixed-layout CLI list view, native transcript bytes unchanged, and attribution fixed through seat/team rename, observed pane movement, teamspace archival retirement, undo, and late completion.
- Independent regression sensitivity check: temporarily restore direct new-path request lookup in the delivery executor, run `cargo test --offline --lib layout_compat_legacy_delivery_and_old_action_undo` — **0 passed, 1 failed** (legacy request receives no delivery). Restored implementation immediately. Log: `/tmp/herdr-graph-task-2-delivery-red.log`.
- Independent regression sensitivity check: temporarily disable the duplicate guard, run `cargo test --offline --lib store::tests::git::locate_each_kind` — **0 passed, 1 failed** (`duplicate IDs must be rejected`). Restored implementation immediately. Log: `/tmp/herdr-graph-task-2-duplicate-red.log`. The intentional temporary removal produced an unused-helper warning; it is absent with the guard restored.

During test construction, corrected fixture-only errors (undo registration needs template kinds; candidate field is `act`; signature helper lives in `store::git`) and replaced a nonexistent `seat move` command with the production `observed.move` mutation. Those setup/compile errors are not counted as feature RED evidence.

## Final validation

- `cargo test --offline --lib` — **614 passed, 0 failed, 0 ignored**, 121.56 seconds. Log: `/tmp/herdr-graph-task-2-lib.log`.
- `cargo test --offline --features test-support --test integration_sweep --test daemon_composed --test writer_concurrency --test writer_crash` — **34 passed, 0 failed, 0 ignored** across four suites: daemon_composed **20** (12.74 seconds), integration_sweep **8** (5.77 seconds), writer_concurrency **3** (14.80 seconds), writer_crash **3** (4.71 seconds). Includes action-envelope parsing at the new root, CLI handler coverage, composed session-end/request processing, concurrent reader atomicity, and crash recovery with queued operations/history. Log: `/tmp/herdr-graph-task-2-integration.log`.
- `cargo test --offline --features private-herdr --test real_agent_smoke --no-run` — compiled successfully, **no agents or test fixture launched**. Log: `/tmp/herdr-graph-task-2-smoke-compile.log`.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- All final commands exited zero; final logs contain no compiler warnings. The full library suite ran once after restoring the deliberate regression-check reversions; no production changes followed it.

Self-review traced all production uses of request/transcript destination helpers: only new request creation uses the request helper, and only initial transcript registration constructs the new transcript destination. Request delivery, merge, ACK, unresolved, completion, transcript refresh/coverage, undo-original updates, and operation reassignment all preserve located paths. Native transcript files and local journal storage semantics are untouched.

## Compatibility boundaries and self-review

- Existing records are intentionally left in their current legacy or historical named folders. Names in new folders reflect registration, not the latest names; registration after a rename captures the names known at that registration time.
- Legacy attribution is unknown and is not inferred from current bindings. A new registration requires resolvable seat/team records, including archived objects; existing legacy discovery and processing do not require captured names.
- Duplicate IDs and corrupt records fail reads explicitly; there is no automatic repair or arbitrary winner. ID lookup uses the shared record enumeration for these four kinds, so it validates supported roots and parses their records; large-history performance was not benchmarked.
- Discovery covers the defined one-level legacy/two-level current transcript grouping and one-month-level legacy/current action/operation roots. Arbitrarily nested user-created layouts are not supported.
- There is no downgrade migration: older binaries that only know old paths cannot discover newly written records. The release documentation must state forward compatibility, historical path retention, and use of the upgraded binary.
- Move coverage exercises the current production observer move behavior (pane binding/moved-out seat). There is no `seat move` organizational CLI in this repository.
- Read-only CLI consumers use shared ID/list helpers; the opt-in real-agent fixture now resolves transcript IDs through the store instead of assuming directory depth. Real agents are not launched by this task.
- Original authorized design/review artifacts and `AGREED-FIXES-HANDOFF.md` remain untouched. Final documentation and the pending herdr-threads 0.2.9 `Cargo.lock` update belong to Task 3.

## Task-owned changed files

- `src/store/layout.rs`, `src/store/tests.rs`
- `src/model/transcript.rs`, `src/model/tests.rs`, `src/model/action.rs`, `src/model/operation.rs`, `src/model/request.rs`
- `src/transcripts/mutations.rs`, `src/transcripts/delivery.rs`, `src/transcripts/tests.rs`
- `src/undo/tests.rs`
- `src/writer/mod.rs`, `src/writer/tests.rs`
- `tests/integration_sweep.rs`, `tests/real_agent_smoke.rs`
- This report
