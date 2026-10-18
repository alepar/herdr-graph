# Herdr release qualification

Run `scripts/herdr-release-smoke` from this checkout on macOS. It reuses the real
private fixtures in `tests/e2e_private_herdr.rs`, `tests/herdr_client.rs`, and
`tests/threads_real.rs`: **30 + 5 + 2 = 37 cases per release**. It does not create
a second graph/Herdr test harness. The exact expected names live in `SUITES` in
the driver; an added, removed, ignored, filtered, duplicated, missing, skipped,
or failing case fails qualification until the inventory is deliberately reviewed.

```sh
scripts/herdr-release-smoke --list
scripts/herdr-release-smoke --versions 0.9.5 --jobs 1 --report /private/tmp/graph-herdr-one
scripts/herdr-release-smoke --jobs 2 --cache /private/tmp/graph-herdr-cache --report /private/tmp/graph-herdr-all
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

The default graph cache is `/private/tmp/herdr-graph-releases-UID`; `--cache`
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
before expanding a support claim. Checksums for Linux assets are provenance,
**not Linux runtime qualification**. The plugin currently declares macOS support,
and the runtime driver refuses other operating systems.

## Dependency/build prerequisites

Use the ordinary Rust toolchain and this repository's lockfile. The currently
qualified threads source is `b4d5f9628d17f869ab5b139b7f4ae3c933d02841` from
[alepar/herdr-threads](https://github.com/alepar/herdr-threads), two commits after
the `v2.0.3` release commit `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`.
This is a checkout qualification, not exact-tag runtime evidence.

The tracked `third_party/herdr-threads` is an absolute local symlink. It is not a
portable CI dependency declaration. No workflow is added that would pretend this
symlink works on another machine. In a **fresh disposable graph checkout**, replace
only that symlink with a clone of the official threads repository, check out the
exact commit above, and verify the threads worktree is clean. Populate both Cargo
dependency caches first (`cargo fetch --locked` against each manifest); the sweep
then builds offline. Do not replace or modify an existing user's upstream checkout.
See also the dependency setup in the graph README. A future CI workflow must
materialize this exact source and its locked dependencies before invoking the sweep.

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
qualification fail. Successful runs remove only their token-verified fixture
roots. Failure roots, logs, and PID evidence remain for inspection. An external
SIGKILL of the driver cannot execute cleanup; use retained root/token/PID evidence
for explicit guarded recovery. A protected process that escapes both observed
ancestry and root-marker visibility cannot be safely identified by guessing;
this is not a claim of kernel-level process containment.

The driver removes API credentials from itself and strips inherited live
Herdr/threads/Claude seat routing from children. Fixture HOME, config, sockets,
state and Claude configuration are private. All graph seats are shell or explicit
stand-ins. Native session hooks/transcripts are synthetic; summary completion is
played through the CLI. This matrix does not validate native model inference,
model authentication, remote threads continuity, real restore holds, a live user
session, or model/summarizer quality. Reconnect/epoch unit and hybrid tests remain
separate evidence, rather than being relabeled as real-service matrix cases.

## Qualification status — 2026-10-17

The complete **0.9.1–0.9.5** matrix passed on **macOS aarch64** at graph `d9a50590c6ca3dccc558664d4ed608eb2153605d` with the initialized official threads v2.0.3 submodule `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`, protocol 9. Each version passed all **30 graph + 5 API + 2 actual threads = 37 cases**: **185 total**. All 15 suite cleanup gates were clean, all 190 owned roots were removed, and no emergency reaping, survivors, ignored/filtered/skipped cases, panic diagnostics, timeouts or cancellations occurred. Actual reported binary versions and SHA-256 values matched the official pins.

[The integration handoff](../releases/2026-10-17-threads2-herdr-qualification.md) records exact commands, source distinctions, checksums, preserved failed trials, native exclusions and migration limits. [The evidence bundle](2026-10-17-threads2-qualification/README.md) retains all case/cleanup observations and raw logs. `--check-upstream` passed against the five official stable releases and all 20 platform digests. Runtime support is limited to this tested macOS architecture; Linux/Intel digests alone do not imply qualification.

Historical Task 3 evidence qualified only 0.9.5 against the clean post-tag external checkout. Its failed/cancelled trials and [implementation report](../superpowers/reviews/2026-10-17-threads2-task-3-implementation.md) remain preserved. The final exact-tag sweep above supersedes its pending full-matrix status. Task 4's independent review and whole-branch integration remain controller work; this is not a merge or publication claim.
