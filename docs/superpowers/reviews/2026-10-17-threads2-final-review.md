# Final whole-branch review — threads 2 compatibility and Herdr matrix

Reviewed base `6c9db8f5d81cf1771d0afd3ea8e1d37a9c8027fa` through `fcad880`, using the supplied complete 13-commit diff package. Binding requirements are the final review brief, the implementation plan, and the seven controller rulings. This review covers the final implementation across task boundaries, including the previously fixed findings.

### Strengths

- The production namespace boundary is substantially complete. `src/writer/mod.rs:274` orders reconnect's exclusive external-call lock before the binding lock; ordinary writer applies also hold the binding lock. `src/threads/namespace.rs:128` checks the snapshot epoch under the shared call lock, and production service/map wrappers retain that guard through the external call. Reconnect commits the marker, preservation artifact and graph edits together before local publication; `src/threads/namespace.rs:240` seals the previous journal before publishing the target binding. The source and preservation/crash regressions support fail-closed recovery without rewriting old intent bytes or native transcripts.
- Stale intent is fenced at the important integration points. `src/plan/apply.rs:118` compares the original reviewed commit's epoch with the applying tree, covering stored and already-admitted organizational plans. `src/daemon/compose.rs:696` includes the epoch journal path in service-cache reuse, including A→B→A. Threads and delivery effects carry epoch identities, bookkeeping carries captured epochs, and the writer rejects late channel/invitation/delivery/ACK updates. Connection probes, requester reminders, pending routing, delivery, receipt polling and reconciler notices all have scope propagation in their production paths. Pre-boundary undo cannot restore old references (`src/undo/compensate.rs:72`). No additional production namespace/history defect was identified.
- Node discovery and pane resolution use the official public node/host contracts. `src/threads/mapping.rs:56` supplies the exact host and a fresh resolution operation, without the ambiguous claimless Seats fallback. Real private tests exercise two hosts with colliding pane text, bare-seat resolution without enrollment, actual membership/ACK/release, same-node restart and reviewed fresh-node recovery. Unsupported capability selects fallback; disconnect/busy errors remain errors. Actual remote continuity and restore-induced holds are appropriately excluded from the runtime claim.
- The matrix checks official checksums and exact versions on cache selection, builds before release workers, requires exact case inventories and successful summaries, and rejects timeouts, skips, prohibited runtime tools and dirty cleanup. Owned-root/PID/start tracking and the private plugin package protect the user's services and source binary. The remaining panic gap is specific and described below.
- Qualification evidence is unusually traceable: exact source and dependency revisions, raw successful and failed runs, per-case results, cleanup records, platform restrictions and native exclusions are retained. Independent checks in this review verified all 18 bundle checksums, all 127 archive-member lengths/hashes, the 175 code/build/test/gitlink input entries, all 20 official asset pins, and the original local plugin binary's hash/size/mtime. All seven Ruling lines match the ledger verbatim.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

**I1. Panics in private daemon logs can still produce PASS, and the evidence is then deleted.**

- Primary location: `scripts/herdr-release-smoke:395`; destructive consequence: `scripts/herdr-release-smoke:416`. Related log routing: `tests/support/private_herdr.rs:51`, `src/daemon/ensure.rs:111`, and `tests/e2e_private_herdr.rs:327`.
- `classify_suite` now rejects panic diagnostics in the harness's captured stdout/stderr, but `run_suite` supplies only `output.log`. The private Herdr server writes stdout/stderr to `<root>/herdr.log`, and graph's detached daemon writes to `<instance>/.graph-local/daemon.log`. The E2E fixture prints the graph log when an assertion times out; successful tests do not forward or inspect those logs. Cleanup checks processes and isolation violations, then PASS removes every root without inspecting its daemon logs.
- Therefore a background task in a private daemon can panic, leave the exercised assertions satisfied, and be normally terminated by fixture teardown. Successful libtest results and clean process cleanup then yield PASS despite the binding no-panic requirement. The subsequent root removal loses the only diagnostic. The earlier Task 3 parser fix handles panic text reaching harness output, but does not cover this cross-process integration path.
- Focused reproduction invoked the actual `run_suite` with a zero-exit fake fixture that registered one correctly owned `/private/tmp/hg-<uuid>` root, wrote a standard `thread 'tokio-runtime-worker' panicked at ...` diagnostic to `graph/.graph-local/daemon.log`, and printed exactly one successful case plus its valid summary. The result was `status=PASS`, `cleanup.clean=true`, and `root_removed=true`. The fake fixture spawned no descendants; only `process_snapshot` was replaced with a self-only snapshot to avoid inspecting unrelated processes. Classification, root registration/ownership checks, cleanup decisions, deletion and result persistence were the real driver code. Evidence: `/private/tmp/hg-final-panic-review-zln7ec0l/fixture` and `/private/tmp/hg-final-panic-review-zln7ec0l/run/result.json`.
- Fix: inspect the owned fixtures' actual runtime logs after process quiescence and before PASS/root removal; include graph, Herdr, threads and private plugin daemon destinations. Fail and preserve diagnostics/roots when a panic is found or required log inspection fails. Add a behavioral regression with a panic only in a child log, alongside successful harness output and clean teardown. Use explicit owned-log inventory or similarly bounded traversal that does not follow paths outside the owned roots.
- Qualification impact: the retained 15 harness outputs do pass the existing parser, but the archive contains no `daemon.log` or `herdr.log` members and the 190 fixture roots were removed. This review found no actual panic in the retained harness logs; it cannot establish absence of private-daemon panics from evidence that was never retained. Requalify the affected runtime matrix after fixing this gate, or inspect complete preserved child logs if such independently retained evidence exists. Preserve the original run as historical evidence and distinguish the stronger replacement qualification.

#### Minor (Nice to Have)

**M1. The matrix's current dependency setup instructions still prescribe the removed symlink and post-tag checkout.**

- Location: `docs/validation/herdr-release-matrix.md:80` and `docs/validation/herdr-release-matrix.md:86`.
- The active “Dependency/build prerequisites” section calls `b4d5f9628...` the currently qualified source, says the tracked dependency is an absolute symlink, and instructs a fresh checkout to replace it and use that post-tag commit. The final branch instead tracks the exact official `ded34fa9...` submodule, and the later status section/README correctly document that pin.
- Impact: a maintainer following the matrix's own prerequisites gets contradictory setup instructions and may qualify a different dependency from the reviewed official release.
- Fix: make this operational section use `git submodule update --init --recursive`, the exact official gitlink, and the locked-cache prerequisites. Keep the post-tag/symlink account only in the explicitly historical Task 3 report or a clearly dated historical note.

### Evidence and review limits

- Read the full supplied diff in bounded passes, recovering portions truncated by tool output. Parsed the repetitive JSON evidence structurally for inventories, statuses, cleanup outcomes, provenance and hashes; checked the binary archive against its manifest rather than attempting to interpret binary diff text. Read all proper task reports/reviews and the full plan/ledger.
- Named outside-diff checks covered all production service/map call sites, transcript liveness receipt/reminder routing, requirement-release identity, graph/Herdr child log routing and teardown, and the source/input provenance assertions. No indiscriminate source crawl or diff re-derivation was used.
- Independently reclassified the 15 archived harness outputs with the actual final driver and checked exact inventories, return codes, timeout/cancellation flags and cleanup records: each of 0.9.1–0.9.5 has 30 graph + 5 API + 2 threads case passes, totaling 185, with 190 owned roots and 15 clean cleanup records. This establishes those recorded observations, subject to I1's missing child-log gate.
- Independently totaled raw archived summaries: default 694/0/1, no-default 691/0/1, ordinary parallel test-support 710/0/1, and config/isolation 16/0/0. The two native config early returns and packaging ignore remain disclosed exclusions. Default/no-default at `33be69e` are reasonably retained because the subsequent fixture delta is restricted to `cfg(test, feature = "test-support")`; final test-support/all-consumer/matrix inputs match `d9a5059` and the reviewed branch's executable source.
- Saved official metadata agrees with all 20 pins and the declared official threads tag/gitlink. This is verification of dated evidence, not a new online assertion about the latest release. The original source plugin binary remains unchanged. No current external checkout mutation or live-state inspection was performed.
- No completed test suite was repeated. The only new execution was the focused synthetic child-log panic reproduction. No Herdr/threads/session commands, membership actions, native models, credentials, network calls, subagents, source/index/HEAD/branch changes or integration actions occurred. Writes were limited to the private reproduction artifacts and this requested review report.

### Recommendations

Address I1 and M1 together, retain the existing evidence unchanged, and provide a scoped review of the new log gate and its regression. The new gate needs runtime evidence covering the previously uninspected private logs; repeating unrelated Rust feature suites is unnecessary for a driver/documentation-only fix. Keep all current migration limits, native exclusions and controller rulings in the final handoff.

### Assessment

**Ready to merge? No — fixes required.**

The threads compatibility and preserving namespace migration are well integrated and supported by meaningful tests. Integration is blocked by the reproducible panic false-PASS gap in the release qualification driver; the stale dependency prerequisite section should be corrected in the same fix wave.

**Finding count: 0 Critical, 1 Important, 1 Minor.**
