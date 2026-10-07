# Agreed graph fixes implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Deliver the approved reusable seat templates, browsable transcript indexes, operational record grouping, documentation and threads 0.2.9 integration.

**Architecture:** Extend the existing template identity and plan/apply machinery with explicit team/seat template kinds rather than a profession registry. Preserve old record readers and compensation paths while new writes use the approved layout; transcript folders retain attribution at registration rather than following later organization changes. Keep mutations serialized and runtime effects reviewable.

**Tech Stack:** Rust, serde/TOML, SQLite journal, Git tree transactions, Herdr and herdr-threads.

**Spec:** AGREED-FIXES-HANDOFF.md, CURRENT-DESIGN.md, final reusable-template section of DESIGN-NOTES.md.

## Global Constraints

- Runtime precedence: seat instance > team member > seat template > team template > graph defaults > applicable built-ins.
- No profession-role registry, assumed string interpolation or generated semantic brief.
- Independent instantiations have independent seats; actual reuse remains explicit in application mappings.
- Preserve live references, overrides/exclusions/member correspondence, reviewable immediate runtime changes and undo.
- New transcript indexes: `transcripts/<team-name>-<team-id>/<seat-name>-<seat-id>/<transcript-id>.toml`, safe slugs and full stable IDs.
- New records: `mutations/graph-changes/<yyyy-mm>/<op-id>.toml`, `mutations/undoable-actions/<yyyy-mm>/<action-id>.toml`, `mutations/transcript-processing/<request-id>.toml`.
- Existing persistent instances, native transcript contents, history and undo must remain usable; compatibility changes must be recoverable.
- Preserve and include all existing authorized local documentation and October 5 roast artifacts. Do not change external threads checkout, live instances, observers or shared services.
- All implementation occurs on `agreed-fixes-2026-10-07` in the coordinated checkout. Mod is read-only until exact pushed SHA handoff.

### Task 1: Reusable seat templates and system duties

**Files:** Modify `src/model/template.rs`, `src/model/common.rs`, `src/model/seat.rs`, `src/model/effective.rs`, `src/templates/{document,kinds,structure,tests}.rs`, `src/bootstrap/{resolve,show,examples,tests}.rs`, template CLI/planning/undo integration, shipped `templates/`, related tests. Add focused template helpers if needed to avoid enlarging mutation-kind files excessively.

**Interfaces:** Extend `TemplateRecord`/`TemplateDocument` with a `kind` discriminator defaulting to team for old records; seat templates use existing `TemplateId` and `templates/<slug>/template.toml` plus `AGENTS.md`. Team members reference seat templates with a typed optional `seat_template: TemplateId` and optional responsibility. Resolve referenced instructions and defaults through existing `/seat`, planning and application paths. Introduce system-duty terminology with legacy serde aliases for old `role` fields; remove unresolved profession `role_ref` while still reading old documents.

- [ ] Add behavioral regressions for two team templates sharing one definition but producing independent seats; every runtime precedence level; instruction layering; old records; summarizer eligibility; live reference edits with immediate replacement preview; exclusions/reuse/member correspondence and undo.
  Test construction follows existing `src/templates/tests.rs` fixtures and `TemplateDocument::parse`:
  ```rust
  let legacy = TemplateDocument::parse("name = 'legacy'\n[[members]]\nname = 'worker'\nstartup = 'deferred'\nrole = 'dispatcher'\nrole_ref = 'unused'\n").unwrap();
  assert_eq!(legacy.members.len(), 1);
  ```
- [ ] Run focused tests before implementation; record expected failures in task report.
- [ ] Extend records/documents and shared effective-resolution helpers. Validate seat references and kind constraints; reject nonexistent or wrong-kind references rather than silently falling back. Seat template edits propagate through all live consuming team applications, with dependency revisions in plans, immediate session effects and reversible action provenance. Preserve team-template create/edit/copy behavior; make seat-template creation/edit/copy accessible through existing template commands and documents.
- [ ] Expose reusable instructions, responsibility/member instructions, seat context and scoped rules as read references through `/seat`; update shipped examples to demonstrate shared definitions. Keep native transcript/session history intact.
- [ ] Run `cargo test --offline --lib` and targeted affected integration tests. Self-review and commit only task-owned files. Write report with red/green evidence, compatibility mechanism and concerns.

### Task 2: Transcript and mutation record paths with compatibility

**Files:** Modify `src/store/{layout,tests}.rs`, `src/transcripts/{mutations,delivery,tests}.rs`, writer/undo/doctor/readers where path construction occurs, model path comments, path-dependent tests. Add focused layout compatibility helper module if needed.

**Interfaces:** Keep action/operation/request path function arguments stable while returning new destinations. Add transcript registration path helper accepting attribution names/full IDs; enumeration and ID lookup read both legacy and new paths. Existing records update at their located path, so old compensation bytes/paths remain valid. Do not eagerly relocate historical operational records; new records use new layout. Historical transcript grouping is fixed at initial registration; later rename/move/retirement/undo do not move indexes or change attribution.

- [ ] Add tests that create legacy records and new records in the same tree and verify ID lookup/enumeration, processing updates, undo of actions recorded before the upgrade, and no duplicate records on rewriting legacy requests/transcripts. Assert new paths, full IDs and safe name slugs.
  ```rust
  assert!(layout::request_record(&rq).as_str().starts_with("mutations/transcript-processing/"));
  assert!(layout::action_record(at, &act).as_str().starts_with("mutations/undoable-actions/"));
  assert!(layout::operation_record(at, &op).as_str().starts_with("mutations/graph-changes/"));
  ```
- [ ] Run new focused tests and record red evidence.
- [ ] Implement dual-layout reads and location-preserving writes for old records. Reject ambiguous duplicate IDs across layouts instead of selecting arbitrary records. New transcript path derives names from attribution objects at initial registration using `slugify`; retain full stable IDs in folder components. Legacy unknown names must not block record discovery or processing. Preserve local journal lifecycle semantics.
- [ ] Update path callers and tests without wholesale unrelated rewrites. Check rename, move, retirement, late processing and undo maintain historical attribution and discovery.
- [ ] Run `cargo test --offline --lib` plus relevant integration suites; self-review and commit only task-owned files. Report exact compatibility limitations and red/green evidence.

### Task 3: Threads 0.2.9 integration, final docs and validation

**Files:** Modify `Cargo.lock`, threads adapter/test fixtures if required, `README.md`, approved design/state/factory docs, affected `skills/` examples, and validation docs. Include `AGREED-FIXES-HANDOFF.md` and October 5 roast report/evidence unchanged as historical artifacts.

**Interfaces:** Consume the completed template and layout behavior. External `third_party/herdr-threads` is a symlink to a read-only checkout; release commit is `223b61a88625d7f442d22d9b8728dc4b2282b15f`, dereferenced `v0.2.9` tag. Verify this locally and against official release metadata without modifying it. Current path dependency automatically updated the lockfile during baseline; own that update here.

- [ ] Verify release/tag/Cargo version and public API changes since 0.2.6 using local source/history and official GitHub release. Inspect registration/ACK, memberships/invitations, notifications, discovery and fallback; report tested contracts separately from source inspection.
- [ ] Add focused behavioral tests for changed contracts if needed, record red/green and adapt graph integration. Use `cargo check --offline --all-targets --all-features` to check all API consumers.
- [ ] Run default, no-default-features, test-support and private-herdr suites sequentially with test logs. Run isolated real-daemon integration with `HG_REAL_THREADS=1 cargo test --offline --features private-herdr --test threads_real -- --test-threads=1`; use sandbox escalation for private sockets, never existing services. Do not run concurrent Cargo builds against one target tree.
- [ ] Update `GRAPH_STATE.md`, `CREATING_NEW_GRAPH.md`, `CURRENT-DESIGN.md`, relevant README/skills and approved notes to match final behavior and explain dual-layout compatibility and old duty fields. Preserve chronological/historical claims. Record exact version/SHA and validation in a committed release-handoff document; document that mod determines first release metadata because there is no existing release/version/packaging policy.
- [ ] Include approved local artifacts and roast evidence. Run `cargo fmt --check`, isolation audit and whitespace check, self-review and commit. Report precise test counts, commands, API evidence and migration limitations.

### Controller integration gate

- [ ] Fresh task-scoped spec/quality review after each task; fix material findings and obtain scoped re-review.
- [ ] Fresh broad whole-branch review against approved handoff, with recorded tests and compatibility evidence.
- [ ] Merge the reviewed branch into main, confirm merged tree matches tested tree, push main, verify remote SHA.
- [ ] Through Herdr send mod `w8:pC` exact pushed SHA, scope checklist, tests/review evidence, compatibility/migration limitations, threads release/version/SHA/API verification and explicit checkout ownership handoff. No live restarts.
