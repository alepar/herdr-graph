### Spec Compliance

- ✅ Task 1 is compliant with the controller's amended scope: discovery/NodeUuid conversion, strict overrides, persistent journal/cache fence, host-qualified resolution, capability checks, and updated isolated fixtures are present. No blocking Task 1 defect found. Reviewed base `151ee59` through head `0d44ce66e9bdfea90388505cce07b2311f631d8d` using the supplied diff package.
- ✅ File coverage matches the brief: `src/threads/{adapter,mapping,service_tests,tests}.rs`, `src/daemon/compose.rs`, `tests/support/private_herdr.rs`, `tests/threads_real.rs`, `Cargo.lock`, and README all have corresponding changes; the focused `src/threads/namespace.rs` module is within the allowed extension.
- ✅ Discovery uses the upstream node hint and checked descriptor (`src/threads/adapter.rs:131`); configured override errors remain visible (`src/daemon/compose.rs:625`). Both lazy consumers validate before returning cached objects (`src/daemon/compose.rs:658`, `src/daemon/compose.rs:780`), and journal fencing happens before opening the persistent client (`src/threads/adapter.rs:169`).
- ✅ Host resolution uses an explicit host and fresh operation ID, propagating errors without claimless fallback (`src/threads/mapping.rs:56`). The production caller resolves a bound graph clone pane (`src/threads/effects.rs:1135`), consistent with the controller-approved bare-seat allocation contract.
- ✅ ACK capability requires service registration and send-time receipts; only Unsupported or a successfully returned list without the capability selects fallback (`src/threads/adapter.rs:557`). Fake peers handle the added public capability exchange (`src/threads/service_tests.rs:184`), retaining the existing receipt tests.
- ⚠️ Cross-task requirement, not a Task 1 rejection: migration, writer coordination, stale queued effects, namespace-salted identifiers, and undo boundaries are not implemented here. `src/threads/namespace.rs:54` checks a committed tree before publishing separately at `src/threads/namespace.rs:102`; it is not a transaction with graph mutation. Task 2 must close the acknowledged first-binding race and prove interruption/retry behavior before product-wide compatibility approval (`docs/superpowers/plans/2026-10-17-threads2-herdr-matrix.md:47`).
- ⚠️ Cross-task preservation check: the legacy classification inspects the current main tree, not all undo/history states (`src/threads/namespace.rs:61`). Task 2 should explicitly cover a currently empty legacy graph with historical channel references reachable through history/undo, in addition to a presently populated graph. No history deletion occurs in this diff; safe namespace handling when those records become active remains to be established.
- ⚠️ Qualification limit: actual remote continuity rows and an actual restore-induced hold are not exercised by these tests. The fake proves the chosen command/host and refusal propagation (`src/threads/service_tests.rs:716`); the real test proves two local hosts with identical pane text do not alias (`tests/threads_real.rs:277`). The report appropriately limits the claim; full upstream remote-continuity/restore behavior is not independently qualified here.

### Strengths

- Namespace publication uses a private file, file fsync, no-clobber publication, and parent fsync; mismatches and corrupt bindings preserve existing bytes (`src/threads/namespace.rs:29`, `src/threads/namespace.rs:90`). Tests exercise competing identities, unchanged pending envelopes, and committed historical records rather than simply asserting an error string (`src/threads/namespace.rs:117`, `src/threads/namespace.rs:170`, `src/threads/service_tests.rs:683`).
- Cached service and pane-map reuse is covered together against a changed identity at the same socket (`src/daemon/compose.rs:836`). This targets the dangerous path where initial discovery alone would be insufficient.
- The real fixture demonstrates bare resolution without an open binding, host separation, actual membership/ACK operations, and same-node restart continuity (`tests/threads_real.rs:259`, `tests/threads_real.rs:277`, `tests/threads_real.rs:466`). Credential removal is applied before private server children start (`tests/support/private_herdr.rs:115`).
- README records the exact qualification source revision and clearly states that reconnect is follow-on work (`README.md:80`, `README.md:100`). The change does not mutate native transcripts, purge legacy artifacts, or add cross-node graph orchestration.

### Issues

#### Critical (Must Fix)

- None.

#### Important (Should Fix)

- None within the amended Task 1 scope. Cross-task requirements above remain gates for final product qualification.

#### Minor (Nice to Have)

1. `src/threads/mapping.rs:18`: the trait documentation still says `None` means the agent has not registered, immediately before stating that the real implementation may allocate a bare seat. The implementation returns `Some` for a resolvable unregistered pane. Update the contract to describe `None` as an implementation-specific absence and explicitly say that `Some` does not establish enrollment; this avoids future callers mistaking resolution for agent presence.
2. `README.md:100` and `src/threads/namespace.rs:22`: the general instruction to restore the original node/store can imply that this alone unblocks an unbound populated legacy graph. The branch always rejects such a graph regardless of which node is restored (`src/threads/namespace.rs:76`). Distinguish an existing matching binding, which restoration can resume, from unbound legacy data, which currently requires the deferred reviewed recovery procedure. This is documentation precision, not a reason to weaken the fence.

### Checks and Evidence

- Read the supplied diff in sequential portions; did not run git commands, change source/index/HEAD, start agents, run Herdr/threads commands, or rerun completed test suites. Only this requested review report was written.
- Named risk: changing lookup into allocation could operate on an arbitrary pane. Focused caller check at `src/threads/effects.rs:1135` confirms an existing graph clone, occupant, and runtime pane binding precede `seat_for`.
- Named risk: host-qualified allocation might discard the host or bypass upstream guard handling. Focused upstream API-path check found dispatch attaches `request.host` before identity resolution (`third_party/herdr-threads/src/service/dispatch.rs:1558`), and the store uses observed guard admission/host state for application (`third_party/herdr-threads/src/store/seats.rs:3270`). `RuntimeContext::explicit` canonicalizes the host parent and `claim_host` parses it (`third_party/herdr-threads/src/daemon/paths.rs:26`, `third_party/herdr-threads/src/daemon/paths.rs:108`). This supports reliance on the approved public contract; it is not a replacement for remote-row qualification.
- Named risk: the new credential-free fixture might leave another allowlisted credential source. Focused check of `src/herdr/isolation.rs:155` and `src/herdr/isolation.rs:158` confirms its credential allowlist consists of the same two removed keys, with private HOME/XDG/Claude/plugin/socket paths; the child factory uses `env_clear` (`tests/support/private_herdr.rs:210`).
- TDD evidence inspected: `/private/tmp/hg-task1-regressions-red.log:7` shows the host-command and node-journal regressions fail for the intended behaviors, plus the missing capability fixture exchange; `/private/tmp/hg-task1-regressions-green.log:17` records 10 passing. `/private/tmp/hg-task1-capability-red.log:6` records ServiceAck versus expected NotifyFallback before the capability fix. These are behavior-level regressions, not compilation-only red evidence.
- Verification evidence inspected: `/private/tmp/hg-task1-default-final.log:637`, `:657`, `:670`, and `:684` record 630 + 14 + 7 + 8 passes (659 total); `/private/tmp/hg-task1-no-default.log:59` records 52 passes; `/private/tmp/hg-task1-real-final.log:11` records 2 passes; `/private/tmp/hg-task1-check-final.log:2` records successful all-consumer check. All completed runs report zero failures and ignored cases, and the inspected final logs contain no warning/error/skip diagnostics. The baseline's build warnings are expected consequences of its five deliberate compile failures, not final-run noise.

### Assessment

**Task quality:** Approved for amended Task 1 scope, with 0 Critical, 0 Important, and 2 Minor findings.

**Reasoning:** The implementation establishes a conservative node boundary and uses the public host-aware resolution contract without inventing recipient identity or weakening receipt semantics. The evidence supports this adapter/fixture step; recovery, writer coordination, historical reactivation, and final matrix qualification remain explicit cross-task gates.
