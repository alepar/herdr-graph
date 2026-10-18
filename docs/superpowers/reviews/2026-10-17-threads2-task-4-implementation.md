# Task 4 implementation report

Implementation base: `c1b3af1963bc04734f11686cf8c27d1f27edc89a`, branch `threads2-herdr-matrix`. Final frozen runtime source: `d9a50590c6ca3dccc558664d4ed608eb2153605d`. Threads dependency: exact official v2.0.3 commit `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`, wire protocol 9. Platform: macOS aarch64. This report is an implementation/qualification handoff for fresh independent review, not integration or release approval.

## Changes and reviewed boundaries

- `456dd9c`: replace moving absolute dependency symlink with an official-URL Git submodule at exact v2.0.3; root Cargo.lock unchanged. The external checkout advanced to clean unreleased `b83d032a64cb9f665f49806b744f7cc730626819`; the old symlink reproduced a locked-metadata failure. The external checkout was never edited. Controller reviewed/approved the pin before qualification resumed.
- `33be69e`: the existing packaging README contract still required `ln -sfn`; replace only that obsolete assertion with `git submodule update --init --recursive`. Preserve every other packaging check. Focused test green 1/0; full default rerun green 694/0/1. The failed original default 693/1/1 remains FAIL, exit 101.
- `d9a5059`: fix the newly exposed parallel test-support race without changing production semantics or the failpoint API. The reconnect CAS crash fixture armed global writer failpoints in the shared library process; three sibling writer tests panicked. Reuse existing TestRoot through narrowly test-only/test-support-gated self-alias/imports, spawn the exact current-test case in a private scrubbed process/TMPDIR with bounded ChildGuard cleanup, and retain the complete original commit/journal/binding/crash recovery body. Verify exactly one selected successful child case/summary. The new sentinel regression itself always runs in a dedicated child and proves the crash fixture leaves its parent's failpoint registry intact.
- Final docs update README, current design, graph state layout, verification/matrix status and [integration handoff](../../releases/2026-10-17-threads2-herdr-qualification.md). The handoff copies all 7 current ledger Ruling lines verbatim, gives exact reconnect/rebuild/restart commands, old-plan/undo/external participation limits, checksum provenance and new-upstream detection workflow.
- Copy Task 1 review, Task 2 review/rereview1 and Task 3 review/rereview1 byte-identically into proper docs; existing implementation reports remain unchanged. Task 4's ignored workspace report and this proper report are synchronized. No scratch workspace is force-added.

## Failure diagnosis and red/green evidence

All logs remain under `/private/tmp/hg-task4-qualification-534cd4f_/logs/` and in the durable raw evidence archive. No failed run is relabeled or deleted.

- Pre-pin all-consumer build deliberately interrupted before runtime when controller required a committed immutable dependency pin; exact owned runner/build quiescence checked. Retained prepin-all-consumers-cancelled.log/prepin-interruption.json.
- Moving-dependency metadata red: locked/offline metadata exit 101 because unreleased external package requires lock update. Pinned host-filtered metadata green exit 0 resolves 2.0.3. The unrestricted metadata attempt separately hit uncached Android package `android_system_properties v0.1.6` under offline mode, exit 101; it is not a green result despite the early log filename. Correct official Herdr metadata replaced an initial wrong-repository404 lookup; all outcomes documented in trial-notes.md.
- Packaging red: pin456-default-failed.log,693 passed/1 failed/1 ignored exit 101. Green: packaging-submodule-green.log,1 passed; final default.log,694 passed/0 failed/1 ignored exit 0.
- Parallel test-support red: test-support-parallel-failed.log,706 passed/3 failed/1 ignored exit 101. Source inspection identified the process-global failpoint registry. Serial focused plan group 72/0 was diagnosis only, never qualification.
- Deterministic isolation red: crash-isolation-red.log, sentinel child catches original inline fixture hitting its parent's CAS failpoint, exit 101. First green attempt failed because its helper demanded zero filtered siblings under --exact; that trial remains crash-isolation-first-green-attempt-failed.log. Corrected helper requires one selected pass and one successful summary while allowing intentionally filtered sibling cases; the matrix's no-filter gate is unchanged.
- Focused parallel green: `cargo test --locked --offline --features test-support --lib reconnect_ -- --test-threads=8`,16 passed/0 failed, exit 0,6.02s, crash-isolation-green.log. Final ordinary parallel full test-support covers the committed fix. No broad serial substitute, production behavior change, source scanner allowlist expansion or matrix gate relaxation occurred.

## Final sequential checks

| Exact command | Source HEAD | Result | Exit | Seconds |
|---|---|---|---:|---:|
| `cargo check --locked --offline --all-targets --all-features` | `d9a5059` | all consumers compiled | 0 | 1.829 |
| `cargo test --locked --offline --no-fail-fast` | `33be69e` | 694 passed, 0 failed, 1 ignored | 0 | 172.441 |
| `cargo test --locked --offline --no-default-features --no-fail-fast` | `33be69e` | 691 passed, 0 failed, 1 ignored | 0 | 205.015 |
| `cargo test --locked --offline --features test-support --no-fail-fast` | `d9a5059` | 710 passed, 0 failed, 1 ignored | 0 | 209.505 |
| `HG_REAL_AGENTS=0 cargo test --locked --offline --features private-herdr --test config_smoke --test test_isolation -- --test-threads=1 --nocapture` | `d9a5059` | 16 passed, 0 failed, 0 ignored | 0 | 28.147 |
| `python3 -m unittest discover -s scripts/tests -v` | `d9a5059` | 24 passed, 0 failed/errors/skips | 0 | 9.563 |
| `scripts/herdr-release-smoke --versions 0.9.1 0.9.2 0.9.3 0.9.4 0.9.5 --jobs 2 --cache /private/tmp/hg-task4-qualification-534cd4f_/herdr-cache --report /private/tmp/hg-task4-qualification-534cd4f_/matrix` | `d9a5059` | 185 passed; all 15 cleanup gates clean | 0 | 496.352 |

Default/no-default evidence is at `33be69e`, retained by controller because the final delta is gated by `cfg(test)` and `test-support`. Final all-consumer, ordinary parallel test-support, config/isolation, driver and matrix use d9a5059. Each feature build/test command is locked/offline and Cargo invocations were sequential. Matrix builds graph executables/bin once, then actual threads once before dispatch. Fmt and whitespace checks pass. The ignored release-build packaging test is intentionally unrun; runtime plugin startup/actions use a private package and prebuilt graph binary.

Config/isolation 16 is a harness count: 12 isolation cases, shell/summary checks, plus two explicit disabled native Claude/Codex early returns. ConfigJSON says shell VERIFIED and both native configs SKIPPED, with HG_REAL_AGENTS=0. No actual native bootstrap/launch/resume/root-agent/summarizer flow was qualified. Driver 24 contains negative tests for strict failure/cleanup gates; matrix has no skips/ignored/filtered cases. No redundant standalone37-case private sweep was run.

## Runtime matrix and provenance

| Herdr actual version | Graph | API | Threads | Cleanup | Official macOS aarch64 SHA-256 |
|---|---:|---:|---:|---|---|
| `herdr 0.9.1` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `5fc7a7e7adfaca56fa80aa89dcb025693357268dab8285b9ce2d08a2313c89de` |
| `herdr 0.9.2` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `e5dae9c7d93010968897de10c6e9ea2aacb14965f7b9e3b3d061dba82d2a1944` |
| `herdr 0.9.3` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `5173a3e0ae42d5d1ab7ebfa5d5e6329f7c3d23f8e1a3677c7ce3231da2884157` |
| `herdr 0.9.4` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `ea20bfff9cb8c83a996081c926e6ff9370de8e543baa859bca48d19175d895ff` |
| `herdr 0.9.5` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `fb77c2232a05a8035421382c894392f4174e4e38a18e48af3dd0290157c3a7e2` |

Total 185 runtime cases; all 15 suite gates clean, 190 owned roots removed, 0 emergency reaped/0 survivors, no timeouts/cancellation/panic diagnostics. Official metadata still identifies latest stable threads 2.0.3 and Herdr 0.9.5; peeled tagded34fa9 is recorded. The --check-upstream maintenance gate exits0 for all five versions/all20 pinned digests. Linux/Intel checksums are provenance only; actual runtime qualification is macOS aarch64.

Private graph clone `/private/tmp/hg-task4-qualification-534cd4f_/graph` is clean/detached at d9a5059; nested submodule clean/detached at exact ded. The initial pin456 source comparison proved all427 tracked entries byte-equal. Later frozen-source/final comparisons allow only concurrent Markdown changes and assert all executable source/tests/scripts/manifests/lock/gitlink bytes equal. [The evidence bundle](../../validation/2026-10-17-threads2-qualification/README.md) includes exact source/binary hashes, command heads/counts/exits, all per-case/cleanup identity evidence, official provenance and raw final/failed/build logs. Private clones/cache remain retained for review.

## Preservation, recovery and limits

Root source `bin/herdr-graph` remains the original Oct 7 artifact, 13624672 bytes, SHA-256 bbc9a7bf8de817cb3dcccfae2e5300de5cb7daac8700b40622ddc64c5971621c, unchanged mtime/size. The clone has no source bin artifact. External threads checkout stays clean/unchanged. No user daemon, live session, native credential store, upstream source, peer link or read-only release cache was modified. No push/merge/version bump/install/release/archive publication occurred.

The handoff documents commit-first node/epoch markers, preserved mutations/threads-reconnect artifacts, per-epoch journals and old-journal seals; explicit from/to/head-fenced plan/apply; mandatory pre-boundary re-planning; undo refusal; preserving native/completed history and explicit external participation reassessment. Build setup initializes submodules; buildable source archives must include pinned dependency contents. Protocol 9 requires graph rebuild/operator restart, neither installed nor performed on the live system here.

Shell/synthetic fixtures cover actual local private Herdr/threads, cooperative public-hook enrollment/CLI acceptance/read/exact ACK, and reviewed graph recovery. Native inference/auth/bootstrap/root-agent/summarizer quality, remote continuity, actual restore-induced holds and live-session handoff remain excluded. Fake/hybrid restore/epoch/race evidence is named separately. Guarded cleanup is not a kernel-containment claim.

Self-review checked each Task 4 brief/preflight requirement, exact source/command provenance, unchanged production semantics, strict gates, review-copy equality, preserved failed evidence and all current controller rulings. Task 4 independent review and whole-branch review/integration remain required controller work.
