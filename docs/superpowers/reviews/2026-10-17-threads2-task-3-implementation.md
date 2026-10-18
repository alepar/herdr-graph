# Task 3 implementation report

Task 3 adds the repeatable graph-owned Herdr release matrix. Base is
`7c38e63f2a7b4c5db860f77afaa8920f848f9227` on `threads2-herdr-matrix`.
There are no production changes, upstream edits, live-host commands, real model
launches, live-seat writes, merges, pushes, or external-cache deletions.
The complete five-version sweep remains Task 4.

## Implementation and coverage

- `scripts/herdr-release-smoke` supports `--list`, `--versions`, `--cache`,
  `--report`, `--jobs`, `--check-upstream`, `--update`, and a per-suite `--timeout`.
  It pins all four supported official asset checksums for each stable release at
  or above 0.9.1, initially 0.9.1–0.9.5. All 20 values match independently saved
  official GitHub release metadata in `/private/tmp/hg-herdr-official-assets.json`.
  An actual online `--check-upstream` returned exit zero and exactly those five
  stable releases. Discovery is paginated, validates complete platform assets and
  SHA-256 metadata, checks stable semver/floor, and compares complete inventories.
- It builds the three graph integration executables and graph binary in one
  Cargo command, then builds the actual threads binary once in a second sequential
  command. Both are locked/offline. There is no Cargo fallback after dispatch.
  Release jobs execute prebuilt binaries directly, with explicit validated binary
  overrides. A PATH/CARGO guard rejects unexpected Cargo/native-model commands;
  even an ignored guard error makes the suite fail.
- Each release reuses exactly **30 graph private E2E + 5 Herdr API + 2 real threads
  = 37 existing cases**. No duplicate E2E harness or metadata-only qualification
  module was added. Test executable inventory is checked before dispatch; runtime
  results require exact names/counts, all `ok`, one exact successful summary,
  no ignored/measured/filtered/skipped cases, exit zero, and clean teardown.
  Fixture-root counts (30/5/3) additionally catch silent early returns.
- The existing graph E2E cases assert create/plan/apply/read, environment, templates,
  application retirement, observation/reconciliation, rename/move/retirement,
  undo and resurrection, persistent daemon reopen/restart/rebind, session/transcript
  shell lifecycle, participation and rejected-op handling, and private plugin
  startup/actions. The five API cases exercise actual snapshots/events/shell process
  state and invalid operations. The two real threads cases exercise production
  discovery, qualified two-host pane ownership, synthetic public session hooks,
  actual CLI membership/read/exact ACK, service capabilities/restart, and reviewed
  reconnect/background status. Every exact name and assertion is mapped in
  `docs/validation/herdr-release-matrix.md`.
- Plugin lifecycle matrix mode copies the original manifest, declared build script,
  and prebuilt graph binary to the test's owned private package. Its manifest's
  startup/action relative paths are unchanged. Normal standalone test behavior
  retains its build step. This cell qualifies plugin runtime hooks/actions, not
  release packaging via `scripts/build.sh`.
- JSON/Markdown aggregate reports and per-version/per-suite logs/result/cleanup
  JSON preserve failure evidence. Selected binary checksum/version errors fail,
  including nonzero or hung `--version`; corrupt cache entries are preserved.
  The threads cache is only read/reverified/copied into graph-owned temporary cache.
- The driver removes API credentials from itself and strips inherited live routing
  from runtime children. Private HOME/state/socket/config remain fixture-owned.
  Real native models never run: shells, a private version stub, synthetic hooks,
  transcripts and manually completed summary requests are explicit stand-ins.

## Cleanup and meaningful fixture fixes

Each root gets a unique suite ownership marker and registry entry **before spawn**;
server PID/start identities are appended after spawn. Roots remain short under
`/private/tmp`. `HG_TEST_ROOT` markers provide spawn-before-PID-registration fallback
for actual Rust servers/daemons. The driver periodically records proven descendants
before parents exit and retains PID/start identities through reparenting. macOS
hides protected system-shell environments, so exact root markers alone are
insufficient; ancestry is anchored in owned processes. Each signal rechecks start
identity, and no broad argv/name kill is used. Root ownership is checked again
before deletion. Matrix mode disables the cross-run orphan reaper.

Timeout/cancellation terminates the runner group and then performs guarded owned
cleanup across private server/daemon groups. Failed inspection, invalid ownership,
isolation violations, survivors, emergency reaping after fixture return, or root
removal failure makes qualification FAIL. Failure roots and evidence are retained;
success roots are removed only after clean teardown. Leak evidence includes private
program basename, PID/PPID/PGID/start/state, without dumping environments.

Two existing fixture defects were exposed and fixed:

1. Plugin startup's short-lived `daemon --ensure` launcher was counted as a second
   daemon by the old substring assertion. Captured private argv showed exactly one
   launcher and one serving daemon. The assertion now counts the exact serving
   command with argument boundaries and still requires exactly one. It logs only
   matching daemon lines, avoiding an unrelated system process-list dump.
2. The Rust fixture reaper could kill a marked Rust parent and miss a protected
   TERM/HUP-ignoring shell descendant. A focused regression failed against that
   original behavior, then passed after retaining owned descendant start identities
   across parent termination. It excludes already-dead zombies from signaling;
   owning Child/wait still reaps direct children. Existing isolation assertions and
   scanner allowlists were not weakened.

An externally SIGKILLed driver cannot execute cleanup. A protected process that
escapes both recorded ancestry and visible root markers cannot safely be identified
by guessing. This is guarded test-process cleanup, not kernel-level containment.

## Red/green and verification evidence

- Initial driver behavioral tests were written before the driver and failed in
  `/private/tmp/hg-task3-driver-red.log` (16 failures: missing implementation).
  Added targeted reproductions exposed protected-orphan tracking, SIGTERM handler
  setup, and root-deletion error reporting before their respective fixes.
- Final driver behavioral suite: **22 passed**, zero skipped/errors,
  `/private/tmp/hg-task3-driver-final.log` (8.543 seconds). Tests use temporary fake
  executables for bad checksum, wrong reported version, nonzero version command,
  unknown version, missing/ignored/duplicate/unexpected result, bad summary, skipped
  case, timeout, nonzero child with fake successful output, incomplete release
  metadata, upstream drift, no credentials/routing, runtime early return, nested
  Cargo ignored by the fixture, protected descendants/orphans, SIGTERM cancellation,
  invalid ownership, and cleanup/root-removal failure.
- Rust protected descendant regression red:
  `/private/tmp/hg-task3-reaper-red.log`, failed with a surviving protected child.
  Green: `/private/tmp/hg-task3-reaper-green.log`, **1 passed** (3.65 seconds).
- Full `cargo test --locked --offline --test test_isolation -- --test-threads=1`:
  **10 passed**, zero failed/ignored, `/private/tmp/hg-task3-isolation-final.log`
  (11.24 seconds), including the existing source isolation scanner and detached
  daemon/panic teardown contracts. No scanner allowlist broadening.
- `scripts/herdr-release-smoke --versions 0.9.5 --jobs 1 --cache /private/tmp/hg-task3-herdr-cache --report /private/tmp/hg-task3-095-qualified`: **PASS, exit 0** on macOS aarch64.
  Exact results: **30 graph + 5 API + 2 actual threads = 37 passed**, no failures,
  ignored, filtered or skipped cases. Runner times: 131.598 / 2.964 / 7.166 seconds.
  Each cleanup gate was clean; root counts were exactly 30 / 5 / 3;
  **no emergency reaping and no survivors**. Aggregate JSON/Markdown and all suite
  output/result/cleanup evidence are at `/private/tmp/hg-task3-095-qualified.*`.
  This is the final source/test fixture tree; only documentation was written after
  the run began. Driver Cargo builds completed sequentially before runtime dispatch.
- `cargo check --locked --offline --all-targets --all-features`: **passed**,
  `/private/tmp/hg-task3-check-final.log` (3.50 seconds).
- `cargo fmt --all --check`: **passed**, empty `/private/tmp/hg-task3-fmt-final.log`.
  `git diff --check`: **passed**. External threads checkout remains clean.
  Source `bin/herdr-graph` still has exactly the timestamp/size/SHA-256 below.
- Self-review checked every Task 3 brief requirement, exact result/root counts,
  safe binary selection, no runtime Cargo/model invocation, and guarded cleanup.
  No additional production or upstream changes were necessary.

## Preserved trial evidence and local artifact safety

The first 0.9.5 trial (`/private/tmp/hg-task3-095-first.*`) reached a legacy plugin
case's nested release build. It was intentionally cancelled before the build could
copy over source `bin/herdr-graph`. The exact matrix driver received SIGTERM;
subsequent inspection found **zero** matching runner/Cargo/rustc processes.
The result stayed FAIL/cancelled; its emergency cleanup reaped the remaining
registered private server and reported no survivors. Source `bin/herdr-graph`
retained its Oct 7 timestamp, 13,624,672-byte size, and SHA-256
`bbc9a7bf8de817cb3dcccfae2e5300de5cb7daac8700b40622ddc64c5971621c`.
It was not replaced or silently restored. Interrupted private build artifacts
were preserved, rather than deleting unrelated local files.

The second 0.9.5 trial (`/private/tmp/hg-task3-095-final.*`) reached all 37 case
assertions: 29 graph passed / plugin count failed, 5 API passed, and both threads
cases passed. The strict threads cleanup gate found/reaped PID 35231 and failed
that suite, with no survivors. That historical failure remains FAIL. Its old
argv/state were not captured, so no exact identity is invented retroactively.
Subsequent focused service diagnostics were clean and useful only for investigation;
they do not replace the final aggregate qualification. The protected-child fixture
regression above independently proved a concrete teardown defect and its fix.

## Scope and remaining qualification limits

- Actual source dependency remains the clean existing external threads checkout
  `b4d5f9628d17f869ab5b139b7f4ae3c933d02841` (`v2.0.3-2-gb4d5f9628`), protocol 9.
  The official 2.0.3 tag is `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`.
  No exact-tag runtime claim or upstream modification is made.
- Current dependency is an absolute local symlink; no misleading workflow was
  added. Documentation describes materializing the exact official source and
  locked dependencies in a fresh disposable checkout before offline builds.
- Runtime qualification is macOS only. Linux digests do not imply Linux support.
  Native models, real summarizer inference, remote continuity, actual restore holds,
  and the live user session remain outside these shell/synthetic fixtures.
- No production compatibility failure was found; no controller production review
  was required. Independent matrix review/full 0.9.1–0.9.5 execution remains later.
