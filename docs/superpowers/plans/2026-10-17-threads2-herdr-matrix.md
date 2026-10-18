# Threads 2 compatibility and Herdr release matrix implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Restore local graph integration with current herdr-threads and provide a repeatable release sweep across recent supported Herdr versions.

**Architecture:** Adapt the existing public threads service client and discovery boundary to node identity/protocol9, retain local-only graph-managed channels, and fence persistent state at namespace changes. Reuse isolated private-server graph tests in a checksum-pinned version driver that reports missing/skipped cells as failures. Preserve graph Git history, native transcripts and recoverable local journals.

**Tech Stack:** Rust, Python3 standard library, official Herdr binaries, isolated Unix sockets, Git/SQLite.

**Spec:** none — user approved local compatibility update and requested a matrix equivalent to threads; this plan records exact scope.

## Global Constraints

- Target official herdr-threads v2.0.4 commit b83d032a64cb9f665f49806b744f7cc730626819 (or explicitly reverify a newer stable release before changing target); protocol9, node-level storage and NodeUuid. External checkout is read-only. The official 2.0.4 tag was reverified on October 18 after publication during qualification; consumed public service/protocol/node APIs are unchanged from 2.0.3.
- Keep graph-managed service channels local; cross-node graph orchestration is outside this scope. Remote seats must not be mistaken for local panes.
- Preserve persistent graph data, native transcript bytes, historical channel/receipt references and undo history. Never replay an old-node intent against a new node, silently drop unresolved work, purge legacy threads data or guess recipient identity.
- Matrix initial official Herdr versions0.9.1,0.9.2,0.9.3,0.9.4,0.9.5; support maintainable upstream check/update. Verify exact checksum and reported version on every selected cached binary.
- Every matrix cell uses private Herdr/threads/graph roots with scrubbed environment; no live server/plugin restarts, peer linking, model credentials or upstream edits.
- A missing test, skip, panic, subprocess timeout or cleanup failure must not yield PASS. Report logs/results per version/case; build binaries once before parallel release runs.
- Work on threads2-herdr-matrix from6c9db8f, preserve existing local artifacts; no main push/publication without a concrete reviewed result.

### Task 1: Upgrade threads adapter and persistent namespace safety

**Files:** Modify src/threads/{adapter,mapping,service_tests,tests}.rs, src/daemon/compose.rs, existing isolation support/tests and tests/threads_real.rs, Cargo.lock; add focused namespace module only if needed. Update relevant README compatibility/recovery instructions.

**Interfaces:** discover returns socket plus node identity from NodePaths/read_node_uuid_hint/read_descriptor. PersistentServiceClient/LocalSocketClient accept NodeUuid; graph's public explicit identity override must parse canonical node IDs and reject malformed configured pairs visibly. Existing service author/ensure key/membership/ACK semantics stay intact. Bind journal/cache reuse to exact node identity and prevent stale pending intents/references from being treated as valid after fresh-store transition.

- [ ] Reproduce existing compile failure, then adapt discovery and all client constructors/fixtures to node identity. Update Cargo.lock offline; pin/document actual upstream commit and do not modify external checkout.
- [ ] Add meaningful regressions before behavior changes, using real temporary roots and fake service peers as appropriate:
```rust
// Target outcomes (construct using existing service_tests/threads fixtures):
assert_ne!(old_node, new_node);
// A cached endpoint/new registration must not replay old-node envelopes.
// Old graph references remain recorded; no receipt becomes dispatched without current exact ACK.
// Remote continuity rows cannot resolve a local pane, and two hosts sharing pane text cannot alias.
```
- [ ] Implement host-qualified pane lookup where current protocol requires it. Preserve service v2 capability, new send-time receipt prerequisite, unsupported-only Notify fallback and busy/disconnect errors. Update fake service fixtures for any capability round trip rather than deleting assertions.
- [ ] Handle changed namespace through a safe explicit recovery boundary: diagnose old graph threads references/pending work, preserve old artifacts, and provide an actionable recoverable way to reconnect/recreate required local channels if existing graph machinery cannot. Do not auto-clear history or replay across node identities. Document exact automatic versus operator steps; ask controller about a genuinely new product command design before expanding CLI.
- [ ] Update isolated real-daemon fixture for current public enrollment/session identity and node layout. Run actual discovery, local pane mapping, ensure/topic, required invite/accept/membership, Notify, exact-ID service ACK/release, busy registration and restart/recovery tests without native models. Threads no longer enrolls a seat merely from check-in; resolve explicitly.
- [ ] Run cargo check --offline --all-targets --all-features; default library plus relevant integration tests; no-default adapter checks and real threads target with HG_REAL_THREADS=1. Record commands/results/skips/red-green, self-review and commit. Return report and concerns.

### Task 2: Reviewed fresh-node reconnect for persistent graphs

**Files:** Add focused threads reconnect planning/mutation/recovery module, register with src/daemon/compose.rs and CLI/plan/undo registry; modify affected threads/reconciliation/transcript models/tests only as needed. Update README recovery instructions.

**Interfaces:** Consume Task1 exact namespace binding/discovery fence (consult task-1-report.md for its final functions/paths). Expose `threads reconnect --from <old-node|legacy> --to <new-node>` through existing reviewable plan/apply machinery; canonical CLI spelling may follow repo grammar if documented precisely. This explicitly changes an external namespace boundary, so ordinary undo must refuse with an actionable reverse-reconnect explanation. No live transition is performed by tests or implementation.

- [ ] Add regressions for legacy populated graph and old-node graph reconnect, strict from/to/head validation, fresh graph auto-bind, same-node restart continuation, interruption/retry, and preserved snapshots/history. Use real Git/journal/writer fixtures.
- [ ] Plan reviews exact old/new binding and changes. Writer preserves a durable migration artifact with old graph channel references, invitations, participation/opt-outs, pending request delivery and exact old operation/effect provenance. Completed requests/native transcripts/history remain untouched.
- [ ] Reconnect recreates desired managed local channels under namespace-salted ensure/effect keys, invalidates old active membership links, and resets incomplete request delivery state with fresh generation while retaining ID/range/history. Old external participation references/opt-outs remain explicit unresolved historical data; never auto-invite or dispatch to a legacy external ID. Do not silently reconstruct channels not owned by graph.
- [ ] Retain old pending service intents under the old identity; never move/rewrite/replay them as new-node work. Reconciliation must not consume old completed/pending effect keys as new-node results. Order local binding and committed graph transition recoverably: crash before/after either step must either resume the exact transition or fail closed, never partly dispatch.
- [ ] Provide exact diagnostics/recovery commands, safety limits and ordinary-undo refusal. Include a fresh-node end-to-end recovery regression when current private fixture supports it; no hidden test-only bypass of production fence.
- [ ] Run focused red/green tests and covering library/threads/transcript/plan integration suites, self-review and commit. Report commands, results and crash semantics. Do not repeat unrelated feature matrices.

### Task 3: Implement checksum-pinned Herdr matrix sweep

**Files:** Create scripts/herdr-release-smoke, tests/herdr-releases.tsv, scripts/tests/test_herdr_release_smoke.py, docs/validation/herdr-release-matrix.md; add/modify tests/herdr_release_smoke.rs and tests/support/{mod,private_herdr,isolated}.rs only as needed. A .github workflow may be added if it actually runs this repo's established build prerequisites; no unsupported-platform promises.

**Interfaces:** Driver uses official herdrdev/herdr release metadata and the threads project's scripts/herdr-release-smoke/tests/herdr-releases.tsv as references, independently runnable from graph. Support --list, --versions, --cache, --report, --jobs, --check-upstream, --update. Cache may read the existing verified threads binary cache but must not mutate external repo. Build graph test executable(s) and threads fixture binary once, then dispatch version tests with exact selected binary (PATH isolation or explicit override) and no concurrent Cargo builds.

- [ ] Inspect existing graph private e2e, Herdr client and real-threads cases; define concrete coverage cells for core graph create/plan/apply/read/undo/templates/persistent reopen, Herdr observation/reconciliation/rename/move/retirement/session shell lifecycle, and realthreads discovery/membership/ACK. Reuse existing tests where they assert those outcomes; avoid copying a full test harness.
- [ ] Add driver behavioral tests with unittest and temporary fake binaries/test executables. Assert bad checksum, wrong reported version, unknown version, missing result, ignored/skipped result, timeout and nonzero child fail even if a fake output says ok. Example parser requirement:
```python
self.assertNotEqual(classify_case(returncode=1, output='test core ... ok', expected='core'), 'PASS')
```
Use actual driver function names/signatures defined by implementation rather than a mirror test.
- [ ] Pin official checksums for all initial releases/platform assets and verify source provenance. Implement explicit stable-semver floor discovery/update/check; no silent omission of unsupported/missing assets or versions. Per-version logs + machine-readable/Markdown reports and nonzero aggregate failure status.
- [ ] Ensure opt-in real tests actually run and missing Herdr cannot silently skip. Require exact result/case counts. Handle child process groups/deadlines/teardown, retain failure evidence, and verify cleanup. Model launches remain shell/stand-in and explicitly distinct from native model validation.
- [ ] Run script tests and one selected private release to verify end-to-end driver before the full matrix. Update docs with exact coverage and exclusions. Self-review and commit; full sweep belongs Task4.

### Task 4: Full release qualification and final docs

**Files:** README.md, docs/verification-matrix.md, docs/validation/herdr-release-matrix.md, current compatibility docs/release notes, test fixes limited to discovered compatibility defects.

**Interfaces:** Consume Tasks1/2 node-compatible adapter/recovery and Task3 driver; record exact graph tree, upstream threads tag/SHA/protocol, each Herdr binary version/checksum, tests actually run, skips and native-runtime limits. Do not replace historical evidence with current claims.

- [ ] Reverify latest official stable threads release/tag/local source and exact source changes; rerun all-consumer compile.
- [ ] Run default, no-default and test-support suites sequentially; run remaining private-only config/isolation checks with native agents disabled; the matrix supplies the full private graph/API/threads suites across every pinned recent Herdr release with no credential requirements. Use appropriate escalation for isolated sockets. Do not duplicate graph/API/real-daemon suites already covered by the matrix on exactly the same tree without cause. Qualify the exact official threads tag through a disposable clone or export; do not alter the user’s external dependency checkout or local plugin binary.
- [ ] Resolve real compatibility failures with focused reproductions and rerun covering cells. Any changed production behavior requires independent review before declaring matrix PASS.
- [ ] Record evidence report, exact coverage/exclusions, migration/recovery commands, dependency rebuild/restart requirements (no live restart performed), checksum provenance and how to detect a new upstream Herdr release. Run fmt/whitespace checks, self-review and commit.

### Controller gates

- [ ] Fresh task-scoped spec/quality review after each task; fix material findings with scoped re-review.
- [ ] Whole-branch review after all tasks, at most one combined final fix wave plus scoped re-review.
- [ ] Verify tested tree and provide concrete integration result with exact branch/SHA, per-version matrix and remaining limitations. Preserve local artifacts and leave live services untouched.
