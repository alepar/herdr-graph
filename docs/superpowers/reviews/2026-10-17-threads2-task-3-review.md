# Task 3 review — spec compliance and quality

## Spec compliance

❌ One Important issue: the required panic-to-FAIL gate is incomplete. `scripts/herdr-release-smoke:245` accepts successful libtest output that also contains a background-thread panic. The focused reproduction below returned PASS.

The remaining Task 3 requirements are met within the reviewed scope:

- `tests/herdr-releases.tsv:5` pins all 20 assets for 0.9.1–0.9.5. `scripts/herdr-release-smoke:105` validates stable releases, the explicit floor, required assets and official digests; upstream discovery is paginated and explicit check/update operations compare/rewrite the inventory. A read-only comparison against `/private/tmp/hg-herdr-official-assets.json` found no differences for any pin.
- `scripts/herdr-release-smoke:206` verifies SHA-256 before execution and requires the exact reported version, exit zero and deadline compliance on each selection, including cache hits. `scripts/herdr-release-smoke:219` preserves corrupt entries and failed downloads and copies from the external threads cache without changing it.
- `scripts/herdr-release-smoke:33` defines the exact 30 graph / 5 API / 2 real-threads inventory. `scripts/herdr-release-smoke:245` rejects missing, duplicate, unexpected, ignored and failed results, bad summaries and nonzero exits. `scripts/herdr-release-smoke:368` additionally rejects timeout, cancellation, prohibited tool invocations, missing runtime roots and dirty cleanup. `tests/support/mod.rs:25` turns ordinary fixture skips into failures in matrix mode.
- `scripts/herdr-release-smoke:435` completes the two Cargo builds sequentially before dispatch and checks executable inventories. `tests/threads_real.rs:34` requires a validated prebuilt override in matrix mode. `tests/e2e_private_herdr.rs:2167` constructs a private plugin package from the original manifest/build script and prebuilt graph binary, preserving source `bin/herdr-graph` and runtime manifest paths. The packaging exclusion is explicit in `docs/validation/herdr-release-matrix.md:36`.
- `scripts/herdr-release-smoke:64` removes live routing and the supported model credentials before runtime; `src/herdr/isolation.rs:158` assigns private HOME/XDG/Herdr/threads/graph paths and passes only an explicit environment allowlist. `tests/support/isolated.rs:367` registers ownership before launch and PID/start identity afterward. `scripts/herdr-release-smoke:322` checks owned roots, tracks descendants, rechecks process start identity before emergency signals, and fails for leaks, inspection errors, isolation violations or survivors. `scripts/herdr-release-smoke:409` fails root-deletion errors rather than declaring success. Matrix fixtures disable the cross-run orphan reaper at `tests/support/isolated.rs:30`.
- `scripts/herdr-release-smoke:368` persists suite output/result/cleanup records; the aggregate writer at `scripts/herdr-release-smoke:479` provides JSON and Markdown. `docs/validation/herdr-release-matrix.md:100` maps concrete cases and distinguishes actual threads service coverage from graph participation and synthetic/native-model exclusions.
- The diff contains no production changes or upstream edits. Fixture changes are relevant to this task: private plugin packaging, exact daemon counting, binary overrides, ownership registration and protected-descendant cleanup.

⚠️ Task boundary: full 0.9.1–0.9.5 qualification remains Task 4, explicitly stated in `docs/validation/herdr-release-matrix.md:195`. This review approves neither all five releases nor exact-tag threads runtime support. Task 3's existing dependency qualification is for the reported external checkout, not the official 2.0.3 tag.

## Strengths

- `scripts/herdr-release-smoke:245` and `scripts/herdr-release-smoke:368` combine exact inventory, process exit, runtime root counts and cleanup into qualification. These are substantive defenses against the usual zero-test/skip/timeout false passes.
- `scripts/tests/test_herdr_release_smoke.py:50` exercises actual driver functions, and the later timeout/descendant tests launch real temporary child processes. `tests/test_isolation.rs:291` adds a meaningful protected-child regression rather than weakening the isolation scanner.
- `tests/e2e_private_herdr.rs:2167` preserves user build artifacts while still exercising original plugin runtime commands. `tests/e2e_private_herdr.rs:2282` fixes the launcher-versus-serving-daemon assertion without relaxing its exactly-one requirement.
- `docs/superpowers/reviews/2026-10-17-threads2-task-3-implementation.md:123` retains interrupted and failed trial evidence and explains limitations without relabeling those trials as qualification.

## Issues

### Critical

None.

### Important — 1

**I1. Panic output can be classified PASS.** `scripts/herdr-release-smoke:253`; missing regression at `scripts/tests/test_herdr_release_smoke.py:50`.

The rejection expression covers skips, isolation violations and fixture teardown diagnostics, but not a Rust panic. A detached/background Rust thread can panic without causing the enclosing test function or libtest process to fail. Exact successful case lines, the expected successful summary and exit zero therefore do not prove that no panic occurred. If the panicked thread has already exited, process cleanup also has nothing to reject.

One focused check imported the real driver without bytecode writes and invoked `classify_suite(0, output, ['core'])` with this output:

```text
running 1 test
test core ... ok

thread '<unnamed>' panicked at src/worker.rs:12:5:
background worker failed

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Observed result: `{"status": "PASS", "cases": {"core": "PASS"}, "expected": ["core"], "observed_count": 1, "detail": ""}`. This violates the binding requirement that a panic must not yield PASS. Reject standard Rust panic diagnostics in captured runtime output and add a behavioral regression demonstrating that a background panic alongside otherwise successful libtest output fails. Keep the diagnostic in the retained failure evidence. This is a driver-gate defect; no panic was found in the saved final 0.9.5 suite outputs.

### Minor

None.

## Evidence and review limits

- Reviewed the complete `review-7c38e63..52d6147.diff` once in ordered sections, for base `7c38e63f2a7b4c5db860f77afaa8920f848f9227` through head `52d614730a4a91f11ba214280833c5c4366e36f5`. Read the task brief and implementation report. No git commands, suite reruns, source edits, live-seat commands or subagents were used.
- Named cross-file risk: the matrix's sanitized parent environment must remain private when fixture children are spawned. Read only the existing environment construction/allowlist in `src/herdr/isolation.rs:138`; it confirms private roots and credential/routing restrictions for child fixtures.
- The diff cut off the changed `ps_lines` parser mid-function. Read `tests/support/isolated.rs:233` through its adjacent marker parser to verify that the new PPID/state/start columns remain in the returned remainder consumed by descendant tracking.
- Named focused executable check: background-thread panic acceptance, documented under I1. No completed tests were rerun.
- Read `/private/tmp/hg-task3-095-qualified.json` and its three suite outputs: 0.9.5 PASS, exactly 30/5/2 successful cases, exit zero, no timeout/cancellation, 30/5/3 registered roots, clean cleanup, no leaked/reaped/surviving processes, and no panic lines. This supports the reported selected-release validation. Its HEAD field is the precommit base because execution preceded the task commit; source equivalence to the final tree is the implementer's recorded claim, not a separately hashed source attestation.
- Read final driver/isolation/check/fmt evidence: 22 driver tests passed; 10 isolation tests passed; all-target/all-feature check passed; formatting log empty. No warnings or errors appeared in the examined final evidence. The isolation helper's nested `running 1 test` output is expected test execution output.
- Read both historical trial aggregate JSON files: `/private/tmp/hg-task3-095-first.json` and `/private/tmp/hg-task3-095-final.json` remain FAIL.
- Compared pins against the saved official metadata: zero differences. This verifies consistency with the supplied provenance evidence; no additional online discovery was performed during this scoped review.
- The report openly limits protected-process discovery to visible markers or observed ancestry. This review does not establish kernel-level containment, recovery after SIGKILL of the driver, native-model behavior, packaging behavior, Linux runtime support, or Task 4's complete release qualification.

## Assessment

**Task quality: Needs fixes.**

The implementation is otherwise well scoped, uses existing meaningful runtime cases, protects private artifacts and enforces substantial result/cleanup checks. Fix the reproducible panic false-PASS gate before relying on the driver for the full release matrix.

**Finding count: 0 Critical, 1 Important, 0 Minor.**
