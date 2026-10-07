# Task 1 — reusable seat templates and system duties

Implementation base: `bf0e46f`, branch `agreed-fixes-2026-10-07`.

Status: DONE. Commit: `32bb1e97d2e50124ccef33acee8bb450de58de3e` — `feat(templates): add reusable seat definitions and system duties`.

Final validation: 608 library tests and 21 affected integration tests passed; no unresolved implementation concern found in self-review. Compatibility limitations below are intentional and should be included in the Task 3 docs/release handoff.

## Implementation

- Kept the existing `TemplateId` and `templates/<slug>/template.toml` namespace. `TemplateKind` is `team` or `seat`; omitted kind reads as `team`. Seat definitions reject members/relationships; applications require a team template; members carry an optional typed `seat_template` and `responsibility`. Missing or wrong-kind referenced definitions fail validation/resolution.
- Added reusable root `AGENTS.md` instructions to template create/edit/copy documents. Copy preserves kind/instructions and existing stable member correspondence. Template kinds cannot change during edit.
- Centralized runtime resolution with precedence: seat overrides > team member > seat template > team template > graph defaults > built-ins. Added optional graph `args` and `summaries` defaults so all runtime keys obey that ordering. Explicit empty argument lists still override inherited values.
- Shared-definition changes produce existing `session.replace` effects for every affected occupied clone, once per clone. Runtime ownership follows the instantiated seat's canonical template reference when explicitly reused. Live references continue to work after the original application, or even every application, retires while leaving a reused seat alive. No native session/transcript records are rewritten by this code.
- Preserved application IDs, independent instantiated seat IDs, exclusions, overrides, mapped/reused seats, copied member IDs, relationship contributions and existing withdrawal semantics. Definition edits update affected application records using the existing transaction/action machinery.
- Plans carry referenced-definition revisions. Because the existing application layer intentionally checks effect equivalence rather than rejecting every revision bump, these references are also included in template/application effect details. Instruction-only edits therefore change the reviewed effects. Template edits now display exact before/after field values, including prose, rather than only changed field names.
- `/seat` returns reusable template records/instructions, member specialization for every participating application, application records for member attribution, seat record/context, and existing global/team/seat rules. These are read references; there is no interpolation or synthesized semantic brief. New members with a reusable definition do not copy specialization into instance context; legacy inline templates preserve their initial member-AGENTS seeding behavior. Existing instance files are never removed by that compatibility choice.
- Replaced operational `Role` terminology with `SystemDuty` / `system_duty`; legacy `role` fields remain serde aliases on seat records, template members/documents, effective config and seat-create args. `--system-duty` is canonical and `--role` is accepted. Existing action compensation documents use the same backward-compatible `TemplateDocument` parser. The unused profession `role_ref` is ignored when reading and is no longer written. Summarizer discovery and source-summary eligibility still use the operational duty.
- Undo reads current instruction files, detects conflicting later prose edits/clears, restores prior prose/runtime values, and removes files introduced by an undone edit. An explicit empty `agents_md` clears the instruction file; omission leaves it unchanged.
- Guarded direct `content write` for root template AGENTS and actual member AGENTS paths: the rejection directs users to `plan template edit ... --from ...` and reviewed apply. Other opaque files (including `notes/AGENTS.md`) remain writable. This closes a revision/approval/undo bypass and was explicitly accepted by the controller.
- Shipped engineer, researcher, reviewer and designer seat definitions. Project and feature teams reference the same engineer definition and instantiate independent seats; system summarizer uses the new duty spelling.

## Red/green evidence

All Cargo commands used `--offline` and required sandbox escalation because temporary Git/Unix-socket tests are restricted in the normal sandbox. No live service, native instance, external threads checkout or shared server was touched.

1. `cargo test --offline --lib templates::tests::seat_template` — initial RED, 0 passed / 3 failed, 598 filtered. `/private/tmp/hg-task1-red.log`: missing kind (`Null` versus `seat`), nonexistent reference incorrectly accepted, and effective model `team-model` instead of `seat-model`.
2. Same focused command — GREEN 3 passed / 0 failed. `/private/tmp/hg-task1-green1.log`.
3. `cargo test --offline --lib templates::tests::seat_template_copy_and_undo` — RED 1 failed: copied reusable AGENTS missing. `/private/tmp/hg-task1-red-copy-undo.log`.
4. `cargo test --offline --lib templates::tests::seat_template_runtime_precedence` — RED 1 failed: graph summaries default was ignored (`true` instead of `false`). `/private/tmp/hg-task1-red-precedence.log`.
5. `cargo test --offline --lib templates::tests` — GREEN 27 passed / 0 failed. `/private/tmp/hg-task1-green2.log`.
6. `cargo test --offline --lib bootstrap::tests::seat_paths_load_live` — RED 1 failed: instance seat-record reference missing. `/private/tmp/hg-task1-red-paths.log`.
7. `cargo test --offline --lib templates::tests::seat_template_live_update_follows` — RED 1 failed: zero replacements rather than one after origin application withdrawal. `/private/tmp/hg-task1-red-reuse.log`.
8. `cargo test --offline --lib instruction` — RED 6 passed / 2 failed: unreviewed content write committed; earlier instruction undo did not detect later clear. `/private/tmp/hg-task1-red-instruction-guards.log`. GREEN 8 passed / 0 failed in `/private/tmp/hg-task1-green-instruction-guards.log`.
9. `cargo test --offline --lib` — first broad pass 603 passed / 2 failed, 121.52s. `/private/tmp/hg-task1-lib-first.log`. Both failures were shipped-example fixtures: missing new project engineer specialization and the old three-template list. Corrected both and updated example count.
10. `cargo test --offline --lib` — next broad pass 607 passed / 1 failed, 118.07s. `/private/tmp/hg-task1-lib-final.log`. The new instruction-only stale-plan regression exposed the effect-set-versus-revisions gap described above; corrected by making definition references part of reviewed effects, preserving the global effect-equivalence contract.
11. `cargo test --offline --lib seat_paths_load_live` — RED missing a second application's member AGENTS reference (`/private/tmp/hg-task1-red-reused-instructions.log`), then RED stale specialization copied into instance context (`/private/tmp/hg-task1-red-copied-specialization.log`). Both corrected.
12. `cargo test --offline --lib templates::tests::seat_template_live_update_follows` — RED effective structure used the borrower's model (`None` rather than `old`), `/private/tmp/hg-task1-red-reuse-config.log`.
13. `cargo test --offline --lib seat_template_live_update_follows` — RED zero replacements for an independently retained reused seat after every application retired. `/private/tmp/hg-task1-red-retained-seat.log`. Shared-definition propagation now enumerates seats by canonical live reference and attributes them to current applications.
14. `cargo test --offline --lib seat_template` — final focused GREEN, 8 passed / 0 failed, 600 filtered, 4.04s. `/private/tmp/hg-task1-green-seat-final.log`.

The first intermediate compile also caught a wrong internal revision type name, corrected before behavioral green; `/private/tmp/hg-task1-green-build.log`. Native transcript JSON role fields and the threads cooperative-role flag were checked during self-review and preserved; operational duty renaming does not change either external format.

## Final verification

- `cargo test --offline --lib`: **608 passed, 0 failed, 0 ignored**, 129.37s. Full output: `/private/tmp/hg-task1-lib-verified.log`.
- `cargo test --offline --test integration_sweep --test daemon_composed`: **14 daemon_composed + 7 integration_sweep passed, 0 failed, 0 ignored** (5.79s and 5.51s). Full output: `/private/tmp/hg-task1-integration.log`. These exercise composed daemon Unix IPC, real temporary Git state, summary-duty eligibility, config consumption, harness profiles/start/resume, action/effect serialization, capability switches and registry wiring.
- Both final commands completed with exit code 0 in serialized session 46327. No concurrent Cargo build was started.
- `git diff --check`: exit 0; task-owned Rust files formatted with `rustfmt --edition 2024 --config skip_children=true`.

## Self-review

Reviewed schema defaults/aliases, reference validation, every resolution call site, canonical configuration ownership on reuse, fanout/deduplication to running clones, instruction provenance through plan effects, existing action compensation parsing, clear/add undo, app exclusion/retirement and copied member correspondence, content-write bypasses, shared shipped examples, and scope of Git staging. `git diff --check` passed before final verification. No subagents or reviewers were dispatched.

## Compatibility and limitations

- No automatic migration or rewrite of existing graph instance records is needed: omitted kind means team and old duty fields deserialize. Unused profession role_ref is discarded on future serialization. Existing IDs, histories, live state and undo records remain available.
- Template instruction edits now require the template plan/edit path; direct content writes to those precise instruction locations are rejected with actionable guidance. Empty `agents_md` clears the file. Existing instruction files remain readable.
- Pending plans generated by older code can require re-planning because reviewed effect details now include definition revisions and exact changed values. No prior plan is silently re-approved.
- The Rust internal public structs now expose `SystemDuty`/`system_duty`, not `Role`/`role`; serialized compatibility is maintained. Graph is not published as a library package.
- Native harness replacement/resume uses existing reconciler capabilities and preserves their limits. Tests use temporary instances, fake Herdr/threads and real Unix IPC; no live native harness or live service restart is claimed.
- Initial approved design docs, Cargo.lock, the controller's plan/status artifacts, and all preexisting untracked artifacts are excluded from this task's commit. Task 3 owns consolidated docs/release compatibility notes.

## Changed task-owned files

- `src/bootstrap/content.rs`
- `src/bootstrap/examples.rs`
- `src/bootstrap/mod.rs`
- `src/bootstrap/tests.rs`
- `src/cli/template.rs`
- `src/model/common.rs`
- `src/model/effective.rs`
- `src/model/graph.rs`
- `src/model/seat.rs`
- `src/model/template.rs`
- `src/model/tests.rs`
- `src/plan/core_kinds.rs`
- `src/plan/tests.rs`
- `src/store/tests.rs`
- `src/templates/document.rs`
- `src/templates/kinds.rs`
- `src/templates/seat_definitions.rs`
- `src/templates/structure.rs`
- `src/templates/tests.rs`
- `src/transcripts/liveness.rs`
- `src/transcripts/requests.rs`
- `src/transcripts/tests.rs`
- `src/undo/preview.rs`
- `src/writer/tests.rs`
- `templates/designer/AGENTS.md`
- `templates/designer/template.toml`
- `templates/engineer/AGENTS.md`
- `templates/engineer/template.toml`
- `templates/feature-team/template.toml`
- `templates/project-team/members/engineer/AGENTS.md`
- `templates/project-team/template.toml`
- `templates/researcher/AGENTS.md`
- `templates/researcher/template.toml`
- `templates/reviewer/AGENTS.md`
- `templates/reviewer/template.toml`
- `templates/system-summarizer/template.toml`
- `tests/daemon_composed.rs`
- `tests/e2e_private_herdr.rs`
- `tests/integration_sweep.rs`
- `tests/writer_concurrency.rs`

## Scoped review round 1 fixes (base 32bb1e9)

The independent review in `task-1-review.md` identified two important issues. Both were reproduced before production changes; no review agents were dispatched by this task.

1. **Retained-seat team reference changes.** Team edits previously enumerated only active applications of the owner template. Added `compute_team_replacements`, which enumerates every surviving seat with a canonical reference to the edited team, excludes actual withdrawal retirements and already-retired seats/teamspaces, records dependent revisions and produces one replacement per occupied clone. Current consuming applications are attributed separately, including borrowers using another team template. Seats retained after every application retires still receive a reviewed replacement. The same path is used for inverse/undo edits.
2. **Omitted member instructions in undo.** Persisted compensation omission means no write, whereas loaded missing instruction files are represented as an empty string. Undo now normalizes a separate expected-state comparison document: an omitted after-value retains the before text, or missing/empty for a newly added member. Explicit clears remain explicit. It computes the original changed fields from the untouched action documents and builds the inverse from the record, so only instruction fields actually restored by the action become writes. Historical action documents without kind/agents fields and with legacy role/role_ref deserialize through the same normalization. Real later instruction edits/clears continue to conflict.

### Round 1 regressions and red/green evidence

- Added `review_retained_seat_reference_switch_and_undo_preview_replacements`: both original-application retirement and every-application retirement; definition A→B replacement preview and B→A undo preview; application attribution; runtime config restoration.
- Added `review_added_reusable_member_without_instructions_undoes_immediately`: add active reusable member with omitted agents, immediately undo, retire its occupied seat and ensure no session replacement is emitted for the retiring seat.
- Added `review_legacy_added_member_compensation_without_agents_undoes`: explicit historical before/after TOML fixture lacking kind/agents, with legacy `role` and unused `role_ref`; deserialize inverse and apply it through the real template-edit path.
- RED: `cargo test --offline --lib review_` → **3 passed, 3 failed, 605 filtered**, 2.21s; `/private/tmp/hg-task1-review-red.log`. Exact failures: expected one replacement but got zero; current member-add inverse was repair-required; historical action inverse returned `Conflict("template field members... has changed since this edit")`.
- First GREEN: `cargo test --offline --lib review_retained_seat_reference` → **1 passed, 0 failed, 610 filtered**, 1.93s; `/private/tmp/hg-task1-review-green-reference.log`.
- Combined GREEN: `cargo test --offline --lib review_` → **6 passed, 0 failed, 605 filtered**, 3.55s; `/private/tmp/hg-task1-review-green.log`. The filter also selects three existing preview tests.
- Covering GREEN (run once after formatting): `cargo test --offline --lib` → **611 passed, 0 failed, 0 ignored**, 122.53s; `/private/tmp/hg-task1-review-lib.log`.
- Covering GREEN: `cargo test --offline --test integration_sweep --test daemon_composed` → **14 daemon_composed + 7 integration_sweep passed, 0 failed, 0 ignored**, 5.68s and 5.41s; `/private/tmp/hg-task1-review-integration.log`.
- Serialized session 47321 completed with exit code 0. Existing instruction later-clear conflict regression passed in the full library suite. No concurrent Cargo builds or live services were used.

Round 1 scope is five files only: `src/templates/kinds.rs`, `src/templates/seat_definitions.rs`, `src/templates/tests.rs`, `src/undo/preview.rs`, `src/undo/tests.rs`. Self-review checked canonical-reference traversal independently of application lifecycle, deduplication and borrower attribution, actual retirement exclusion, dependency revision inclusion, inverse no-write semantics, historical compensation parsing and the existing later-clear conflict case. `git diff --check` passed. No initial docs, Cargo.lock, controller artifacts or untracked review evidence are staged.

Round 1 result: DONE. Fix commit: `2ad05272b832396ae8cb7a8a8d114fd23b66d97b` — `fix(templates): preview retained-seat changes and preserve undo omission semantics`. Both reported findings are addressed; no unresolved concern found during scoped self-review. Latest verification is 611 library + 21 integration tests passed.
