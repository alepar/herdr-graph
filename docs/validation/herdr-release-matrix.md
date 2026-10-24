# Herdr release qualification

Run `scripts/herdr-release-smoke` from this checkout on macOS or Linux. It reuses the real
private fixtures in `tests/e2e_private_herdr.rs`, `tests/herdr_client.rs`, and
`tests/threads_real.rs`: **30 + 5 + 2 = 37 cases per release**. It does not create
a second graph/Herdr test harness. The exact expected names live in `SUITES` in
the driver; an added, removed, ignored, filtered, duplicated, missing, skipped,
or failing case fails qualification until the inventory is deliberately reviewed.

```sh
scripts/herdr-release-smoke --list
scripts/herdr-release-smoke --versions 0.9.5 --jobs 1 --report /tmp/graph-herdr-one
scripts/herdr-release-smoke --jobs 2 --cache /tmp/graph-herdr-cache --report /tmp/graph-herdr-all
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -v
```

The optional `--versions` accepts one or more pinned versions; unknown/duplicate
versions fail before building. `--jobs` controls concurrent releases; each
release runs its three suites sequentially, with one Rust test thread. `--timeout`
is the deadline per suite (900 seconds by default). Run with permission to create
private Unix sockets and inspect only owned processes with `ps`; a sandbox that
blocks those operations fails qualification. The behavioral unit tests include
real detached temporary processes, so their cleanup tests need that permission too.

Before dispatching release workers, the driver executes two sequential Cargo
commands: one builds the three graph integration executables and the graph binary,
and one builds the actual threads binary. Both use `--locked --offline`. It
validates each executable's exact test inventory, then executes those binaries
directly for every version. `HG_TEST_HERDR_BINARY` and `HG_TEST_THREADS_BINARY`
are validated absolute executable overrides; matrix mode requires prebuilt threads
and prohibits the ordinary nested-build fallback. A runtime PATH guard rejects
Cargo and native `claude`, `codex`, `gemini`, and `opencode` commands; even an
ignored guard error fails the suite. The threads fixture's deliberate local
`claude --version` stub is private and precedes this guard.

The plugin lifecycle case copies the original `herdr-plugin.toml`, its declared
build script, and the prebuilt graph binary into its own fixture root. It links
that private package and exercises the manifest's original relative startup and
action commands. Source `bin/herdr-graph` is untouched. This runtime cell does
**not** qualify release packaging via `scripts/build.sh`; compilation is already
done before the cell, and an unexpected build-hook invocation fails the guard.
Normal standalone test behavior retains its existing build step.

## Pins and future releases

`tests/herdr-releases.tsv` pins the four official macOS/Linux architecture assets
for every stable Herdr version at or above **0.9.1**, initially **0.9.1–0.9.5**.
All 20 initial SHA-256 values were checked against the official GitHub release
asset `digest` fields on 2026-10-17, independently of the reference sweep in
herdr-threads. Sources: [official releases](https://github.com/herdrdev/herdr/releases)
and [GitHub release metadata](https://api.github.com/repos/herdrdev/herdr/releases).
Each selected host binary is hashed again on every run and must report exactly
`herdr VERSION` with exit zero within ten seconds. A mismatch fails and preserves
the existing cache entry or failed download for inspection.

The default graph cache is `TMP/herdr-graph-releases-UID`, where `TMP` is `/private/tmp` on macOS and `/tmp` on Linux; `--cache`
selects another graph-owned cache. An existing matching threads cache entry at
`~/.cache/herdr-threads/herdr-releases/VERSION/PLATFORM/herdr` may be copied into
the graph cache. The source cache is read-only: nothing there is deleted,
replaced, or chmodded. Missing entries are downloaded from the official release
URL and verified before publication. A missing selected asset fails explicitly.

```sh
scripts/herdr-release-smoke --check-upstream
scripts/herdr-release-smoke --update
```

Both commands fetch all GitHub release pages. They consider stable semantic
versions at or above the explicit floor, excluding drafts and prereleases. A
release missing any required platform asset or SHA-256 digest is an error.
`--check-upstream` fails for new/missing versions or changed digests.
`--update` rewrites pins from that official metadata; it does not confer runtime
support. Review the change, run driver regressions, then rerun the complete matrix
before expanding a support claim. The driver selects the asset for the host
(`herdr-{macos,linux}-{aarch64,x86_64}`) and refuses other operating systems.
Checksums for an asset are provenance only; runtime qualification for a
platform comes from a matrix run on it.

## Dependency/build prerequisites

Use the ordinary Rust toolchain and the repository lockfiles. The dependency is
an official Git submodule, currently pinned to herdr-threads **v2.3.1**, exact
commit `bdb0c5a5ac4623dc3a3187d53f6b20f814bd4144`, wire protocol **9**.
It supersedes v2.0.4 `b83d032a64cb9f665f49806b744f7cc730626819` (see the
2026-10-23 entry in `docs/verification-matrix.md`; this sweep has passed at v2.3.1
on Linux x86_64 only, not yet on macOS). v2.0.4 in turn superseded the earlier official v2.0.3 pin `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`:
v2.0.4 became latest stable during replacement qualification, and its tag and
unchanged consumed public API were reverified before updating the gitlink/lock.

```sh
git submodule update --init --recursive
git -C third_party/herdr-threads rev-parse HEAD
cargo fetch --locked
cargo fetch --locked --manifest-path third_party/herdr-threads/Cargo.toml
```

The reported submodule HEAD must equal the exact official commit above. Populate
both locked dependency caches before running the sweep; all sweep builds use
`--locked --offline`. In a prepopulated environment no fetch is needed. A fresh
clone can use `git clone --recurse-submodules <graph-repository-url>`. Source
archives must explicitly include the pinned submodule contents. This setup never
requires editing a separate external threads checkout. The old absolute-symlink
and post-tag account remains only in the dated Task 3 report.

## Runtime coverage

All cases below run against the selected official binary, private Herdr servers,
and real graph CLI/daemon/Git state. The API suite exercises the real Herdr API.
Only the two `threads_real` cases use an actual private threads daemon; graph E2E
participation fixtures do not substitute for the real service cases.

### `e2e_private_herdr` (30 cases)

| Exact case | Outcome asserted |
| --- | --- |
| `e2e_create_teamspace_seats_tabs_panes_env` | Create teamspace, seats, tabs/panes and shell environment; plan/apply and read graph records. |
| `e2e_rename_tab_renames_seat_moves_path_history` | Observe tab rename; move seat path and preserve identity/history. |
| `e2e_close_pane_retires_clone` | Observe pane closure; retire its clone. |
| `e2e_close_tab_retires_seat_and_clones` | Observe tab closure; retire seat and clones. |
| `e2e_close_workspace_retires_all_incl_dormant` | Observe workspace closure; retire all seats including dormant ones. |
| `e2e_undo_tab_close_restores_in_new_tab` | Undo tab closure; restore graph state into a new tab. |
| `e2e_undo_from_bound_pane_adopts_and_keeps_it` | Undo from bound pane; adopt the caller and retain it. |
| `e2e_undo_from_pane_adopts_caller_pane` | Undo from an unbound pane; adopt the caller. |
| `e2e_undo_tab_close_from_pane_adopts_caller_pane` | Undo tab close from pane; adopt the caller. |
| `e2e_template_edit_adds_member_new_tab` | Edit a template/application; materialize the new member in a new tab. |
| `e2e_application_retire_closes_exclusive_seats` | Retire application; close its exclusive seats. |
| `e2e_retire_last_clone_induced_seat_retirement_once` | Retire final clone; induce exactly one seat retirement. |
| `e2e_seat_deactivate_closes_tab_no_retirement` | Deactivate seat; close tab without retiring it. |
| `e2e_move_only_pane_moved_out_no_recreate` | Move the only pane out; do not recreate it. |
| `e2e_move_pane_into_other_seat_tab_keeps_that_seats_name` | Move pane into another seat tab; keep the destination seat name. |
| `e2e_close_while_daemon_down_unknown_not_recreated` | Persist/reopen after daemon downtime; classify closed pane unknown without recreation. |
| `e2e_rename_while_daemon_down_recorded` | Persist/reopen after daemon downtime; record external rename. |
| `e2e_daemon_kill_mid_op_recovers_no_duplicates` | Kill daemon mid-operation; recover without duplicate graph/Herdr objects. |
| `e2e_event_loss_reconnect_converges` | Reconnect event stream; converge after event loss. |
| `e2e_dropped_rename_recovered` | Recover a dropped rename through reconciliation. |
| `e2e_herdr_restart_rebind_no_mass_retirement_no_duplicates` | Restart Herdr; rebind without mass retirement or duplicate clones. |
| `e2e_rebind_restamps_token_after_herdr_restart` | Restart Herdr; restamp graph identity tokens. |
| `e2e_seat_resolution_in_bound_pane` | Resolve a graph seat from a bound pane. |
| `e2e_restored_panes_resolve_without_env` | Resolve restored panes without inherited graph environment. |
| `e2e_summary_lands_in_archived_seat_folder` | Synthetic session report; route transcript request and manually ACK/write/complete summary in archived folder. |
| `e2e_seat_override_model_change_recorded_shell_not_replaced` | Record model override while retaining the shell; no native model launch. |
| `e2e_participation_join_leave_scopes` | Graph participation join/leave scope behavior. |
| `e2e_cancel_and_reassign_rejected_op` | Cancel/reassign rejected operation while retaining its history. |
| `e2e_resurrect_retired_seat` | Resurrect an archived seat with the same graph identity. |
| `e2e_plugin_link_status_action_single_daemon` | Link private plugin package; startup/action lifecycle; exactly one private daemon. |

### `herdr_client` (5 cases)

| Exact case | Outcome asserted |
| --- | --- |
| `private_create_tab_panes_with_env_snapshot_matches` | Create workspace/tab/split, metadata and snapshot, rename, and verify environment reaches actual shells. |
| `private_events_created_renamed_closed` | Subscribe and observe create/rename/close events. |
| `private_snapshot_after_reconnect` | Reconnect snapshot client; persistent server state remains readable. |
| `private_process_info_is_shell` | Inspect pane process information; verify shell execution. |
| `private_agent_absent_and_rejections` | Confirm absent native agent and invalid API-operation rejection. |

### `threads_real` (2 cases)

| Exact case | Outcome asserted |
| --- | --- |
| `real_threads_channel_invite_membership_notify_release` | Actual threads service channel, qualified two-host pane ownership, synthetic public session enrollment, CLI acceptance/membership/read/exact ACK, release/busy and restart behavior. |
| `production_discovery_reaches_real_threads_daemon` | Production default discovery; preserve old-node data, guarded fresh-node reconnect, new channel/idempotence, and normal background connection status. |

## Result and cleanup contract

`--report STEM` writes `STEM.json`, `STEM.md`, and a fresh `STEM.logs/` directory
(the logs path must not already exist). Without it, the driver allocates a private
temporary report directory. Each version has a result JSON; each suite has its
captured output, exact per-case results/expected inventory, runner PID/PGID,
timeout/cancellation flags, and cleanup JSON. Exit zero requires all selected
versions and all cleanup gates to pass; child exit zero alone is insufficient.
The JSON/Markdown reports persist even when a selected release's download,
checksum, version check, test run, or teardown fails. A preflight/build failure
exits nonzero with its diagnostic/build log before runtime qualification begins.

Fixture roots are registered with a unique suite token before anything starts;
server PIDs and start identities are appended after spawning. Every fixture child
inherits its exact `HG_TEST_ROOT`. The driver periodically captures owned
process identities and their descendant closure while parents are alive. This
also records macOS protected shells whose environment `ps` cannot expose, and
retains that proven identity if a child later becomes orphaned. Timeout or
SIGINT/SIGTERM first terminates the runner group; guarded cleanup then handles
private servers/daemons in separate process groups. Every signal rechecks PID
start identity; no generic process-name match, live-host command, or broad `pkill`
is used. Matrix mode disables the ordinary cross-run orphan reaper.

A surviving process, failed inspection, invalid ownership registry, isolation
violation, teardown error, or emergency reaping after the suite returns makes
qualification fail. After process cleanup, the driver inspects known owned
runtime destinations and retains exact raw bytes, lengths and SHA-256 values in
`diagnostics/` plus `diagnostics.json`. It requires `herdr.log` and `config/herdr/herdr-server.log` in every root,
inspects the known `herdr.log`/`herdr-client.log`/`herdr-server.log` tracing files
and numeric `.N` rotations under that config directory when present, and requires
`graph/.graph-local/daemon.log` in every graph E2E root, one private plugin's
captured startup/action stdout and stderr (`plugin-diagnostics.log`) per graph
suite, and all three threads nodes exercised by the threads suite. Threads
inspection includes `daemon.log`, optional `daemon.log.1`, and the existing
`logs/startup-<pid>-<nonce>.log` files in the direct state node, default HOME node,
and preserved `saved-old-node`. Existing nodes require a daemon log and startup
log. The plugin daemon itself uses the graph daemon log.

Every path component is opened relative to the owned root with symlink refusal;
only those destinations are inspected, with no recursive content/Git/transcript
scan. Missing/unreadable required logs, inventory failures, preservation errors,
or Rust panic diagnostics make qualification fail and retain roots. Optional
absences and deliberate content exclusions are recorded. Successful runs remove
only their token-verified fixture roots after raw diagnostics have been preserved.
Failure roots, logs, and PID evidence remain for inspection. An external
SIGKILL of the driver cannot execute cleanup; use retained root/token/PID evidence
for explicit guarded recovery. A protected process that escapes both observed
ancestry and root-marker visibility cannot be safely identified by guessing;
this is not a claim of kernel-level process containment.

The driver removes API credentials from itself and strips inherited live
Herdr/threads/Claude seat routing from children. Fixture HOME, config, sockets,
state and Claude configuration are private. The private Herdr config disables
both supported background version and manifest update checks so network updater
children do not outlive fixture teardown. The strict emergency-reaping gate remains unchanged. All graph seats are shell or explicit
stand-ins. Native session hooks/transcripts are synthetic; summary completion is
played through the CLI. This matrix does not validate native model inference,
model authentication, remote threads continuity, real restore holds, a live user
session, or model/summarizer quality. Reconnect/epoch unit and hybrid tests remain
separate evidence, rather than being relabeled as real-service matrix cases.

## Linux x86_64 run at threads v2.3.1 — 2026-10-23

First Linux run of the driver (Fedora 44, `--jobs 2`), graph `7bdb4af` plus the
driver port, official threads v2.3.1 `bdb0c5a5ac4623dc3a3187d53f6b20f814bd4144`.
Herdr 0.9.1, 0.9.3, 0.9.4 and 0.9.5 passed 37/37. **0.9.2 failed 1/37**:
`e2e_daemon_kill_mid_op_recovers_no_duplicates` found a duplicate workspace
after the mid-operation SIGKILL. The aggregate result is FAIL. The case passed
5/5 when rerun alone on 0.9.2 and on 0.9.5.

Herdr's log and the retained journal show a real at-most-once race in graph's
lost-response recovery, not a 0.9.2 regression. The killed daemon's
`workspace.create` was still in its socket buffer. The restarted daemon locked
and snapshotted at about 10.302 s. Herdr applied the buffered create at 10.312 s,
with the effect's nonce label. Recovery then created a second workspace at
10.489 s, because its earlier snapshot lacked the nonce-labelled one. The fake
Herdr could not express this window, because it applied requests synchronously.

Commit `7110552` fixes it. The planner now detects late copies: unstamped
workspaces or tabs that carry the nonce of a completed create while the
object's live counterpart is elsewhere. It closes a copy that is still as Herdr
created it, and reports one that is in use. The fake's new `Fault::AppliedLate`
reproduces the race in unit tests. A late duplicate pane from a lost split is
not covered: a split carries no nonce, because no Herdr release accepts a pane
label at split time, so a late pane cannot be distinguished from a user's split.

The rerun at `7110552` (`--jobs 2`) **passed 37/37 on every release, 185 total**,
with all 15 cleanup and 15 diagnostic gates clean. Separately, the daemon-kill
case passed 40/40 runs, four at a time across all five releases. Those passing
runs do not show whether any of them hit the race window; the unit tests
exercise the cleanup path itself.

## Replacement qualification — 2026-10-18

The stronger replacement matrix uses frozen graph `1005d04657a260dc89a55d3b1d37ce6b6f7cdb7e` and official threads
v2.0.4 `b83d032a64cb9f665f49806b744f7cc730626819`, protocol 9, on macOS aarch64.
All five Herdr releases **0.9.1–0.9.5 passed 37/37 cases each** (30 graph + 5 API + 2 actual threads): **185 total**. All **15 cleanup gates and 15 diagnostic gates passed**, with **190 owned roots removed**, zero emergency reaping/survivors, and **570 raw diagnostic files (1,686,907 bytes)** retained. Actual binary versions and SHA-256 values matched the official pins. There were no skipped/ignored/filtered matrix cases, Rust panic diagnostics in the inspected streams, timeouts or cancellations.

[The integration handoff](../releases/2026-10-17-threads2-herdr-qualification.md)
records exact source, commands, native exclusions, migration limits and rulings.
[The replacement evidence bundle](2026-10-18-threads2-log-qualification/README.md)
retains raw inspected diagnostics as well as every case/cleanup/log gate result.
Runtime support is limited to this tested macOS architecture; Linux/Intel digests
alone do not imply qualification. Scoped final rereview/integration remain
controller gates; this is not a merge or publication claim.

## Historical qualification and its limit — 2026-10-17

At graph `d9a50590c6ca3dccc558664d4ed608eb2153605d` with official threads v2.0.3
`ded34fa971c18cef81ea1b597097a8ff7cd91eb3`, all five releases produced 37 successful
case results each (185 total), exact inventories and 15 clean cleanup records.
The earlier panic gate inspected harness output only. Private daemon logs were
removed with the 190 roots, so that evidence cannot establish absence of daemon
panics. The original [Task 4 bundle](2026-10-17-threads2-qualification/README.md)
and retained clone remain unchanged historical evidence; the stronger result
above replaces the runtime qualification claim.

Historical Task 3 evidence qualified only 0.9.5 against the clean post-tag
external checkout. Its failed/cancelled trials and
[implementation report](../superpowers/reviews/2026-10-17-threads2-task-3-implementation.md)
remain preserved.
