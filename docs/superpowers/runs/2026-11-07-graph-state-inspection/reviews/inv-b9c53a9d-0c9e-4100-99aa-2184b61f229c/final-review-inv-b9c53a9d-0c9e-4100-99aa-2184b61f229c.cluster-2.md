# Final review, cluster 2 (cli), epic hg-blw

Invocation inv-b9c53a9d-0c9e-4100-99aa-2184b61f229c. Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 2dfa656c7f98715fe66ca53c253de19f369a857d (both verified with `git rev-parse --verify`). Caller checkout observed at HEAD 2dfa656, status clean; it was not switched or modified. Review is static (slices 000034-000041 plus `git show` at head); no build or test run was made, to avoid writing a build tree into the caller worktree or a fresh target dir.

Files: src/cli/instructions.rs, src/cli/log.rs, src/cli/mod.rs, src/cli/ops.rs, src/cli/roster.rs, src/cli/seat.rs, src/cli/sessions.rs, src/cli/setup.rs.

## Baseline trace from CLI entry points

- B1 (which seats are active): `herdr-graph roster [<scope>] [--all] [--json]` is registered in `cli::Command::Roster` and dispatched to `cli::roster::run`, which pins HEAD once, resolves the scope (clone refused, AllowClone::No) and calls `inspect::roster::build`, emitting through the shared envelope with `live:false`. Delivered at the CLI layer; row content is the inspect cluster's.
- B2 (files loaded per seat): `instructions stack <seat> [--clone c] [--json]` resolves seat and clone at one pinned HEAD and calls `inspect::stack::build` with `UserDirs::from_env()` (inspector env, as spec 2.3 says). The loaded record has a real producer: `seat --hook-prompt [--codex] --stdin-payload` prints first, flushes, then reads stdin (64 KiB cap, 2 s timeout on a helper thread) and calls the single `loaded::record` entry point; the owner-marked hook commands that setup installs (src/bootstrap/setup_claude.rs:211,219) carry `--stdin-payload`, so the path is reachable from production. `seat --instructions` and `seat --json` do not call `record`. Delivered.
- B3 (activity global/space/seat): `log [<scope>] [--since] [--limit 30] [--by] [--all] [--max-scan 5000] [--json]` resolves scope and `--by` at one HEAD, runs `LogQuery` over `HistoryWalker`, writes malformed-record warnings to stderr and the ops pointer as the last text line or on stderr under `--json`. `ops` gains `--seat/--teamspace/--limit/--json`; filters resolve to ids at HEAD and are sent to the daemon's `ops.list`, which now filters (src/plan/ops.rs) and returns `head`. Delivered.
- B4 (sessions by space/seat/clone, newest first): `sessions [<scope> | clone <c>] [--since] [--limit 50] [--json]` with AllowClone::Yes, Inventory load at the pinned HEAD, and `inspect::sessions::build`. Delivered at the CLI layer; ordering and transcript fallback are the inspect cluster's.
- B5 (staleness shown, not notified): the hook path adds no notice; output bytes are unchanged with and without `--stdin-payload` (the record is written after printing). Delivered.
- B6 (log hides bookkeeping by default): `--all` defaults to false and is passed to `LogQuery.all`. Delivered.
- B7 (harness files never read): the CLI only passes `UserDirs::from_env()`; enforcement is in inspect/harness_files.rs, which is the inspect cluster's to judge.
- B8 and S1 (`roster --live`, `--unmanaged`): `roster --live` makes one `roster.live` call with `CallMode::NoEnsure`, a down daemon goes through `daemon_error::report` (exit 5, covered in tests/daemon_free_reads.rs), and the reply's `roster` deserializes into the same `Roster` type (Deserialize keeps `live_unavailable`, `scan`, `threads_live` and the absent-vs-null `unmanaged`). `--unmanaged` requires `--live` at the clap layer. The daemon-side overlay and unmanaged enumeration are the seam reviewer's and daemon cluster's to trace.

## Analysis notes

- Unchanged behavior holds in this cluster: plain `ops` text output is byte-identical (same `render_op`, same empty-list line, emit only appends a newline when missing) and the daemon's `ops.list` default limit was already 200 at base; the old Claude hook command without `--stdin-payload` never reads stdin, so `session-report` still gets the payload; `setup codex --uninstall` still prints no trust note, as at base.
- `ops --json` uses the daemon's reply `head`, documented in `envelope_from_reply`; a reply without `head` is an error rather than a silently unfiltered listing, which also stops an old daemon from ignoring `--seat` silently.
- `instructions stack --clone <c>` resolves the clone name instance-wide (`Scope::resolve` over `layout::all_clones`), not within the named seat; `stack::build` then rejects a clone of another seat.

## Concerns

- src/cli/instructions.rs `stack`: `--clone <name>` is resolved across every clone in the instance, not among the given seat's clones, so `instructions stack <seat> --clone <name>` exits 1 as ambiguous when another seat has a live clone of the same name (the default clone name is the seat name, src/plan/core_kinds.rs:945), and picks the wrong clone (then fails "not a clone of seat") when the seat's own match is retired and another seat's is live. A clone id works; a fix would filter matches to `entry.clones` before `pick`.
- src/cli/ops.rs `envelope_from_reply`: plain `ops` (text, no new flags) now fails with "daemon reply has no head" against a daemon still running a pre-upgrade binary, since IPC_VERSION stayed 1; the error is safe but is a new failure mode for an otherwise unchanged command during upgrade skew.
- src/cli/roster.rs `live`: scope words are sent raw to the daemon with no client-side parse, so `roster --live clone x` with the daemon down exits 5, not as a usage error; the daemon-free roster reports the same input as usage.
- src/cli/seat.rs `read_stdin_payload`: a harness that keeps the hook's stdin open delays hook exit by up to 2 s after output; within the 10 s budget and disclosed in the doc comment, but it is added SessionStart latency on Codex if Codex does not close stdin.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- src/cli/instructions.rs `instructions stack --clone <c>`: only clap parsing is tested; no test resolves a clone through the CLI, including the same-name-in-another-seat case named in Concerns.
- src/cli/ops.rs `ops --seat/--teamspace` against a running daemon through the CLI binary: filter resolution and the daemon reply envelope are tested separately (unit and list_ops level), never end to end.
- src/cli/setup.rs `codex_trust_note`: the Installed/Updated vs Unchanged notice has no test (also in the task 9 deferred minors).
- src/cli/seat.rs `read_stdin_payload` timeout path (a stdin that stays open) has no test; only the closed-pipe path is exercised by tests/inspect_loaded.rs.

## Deferred OK
- src/cli/instructions.rs: resolve `--clone` among the named seat's clones instead of instance-wide (id form works today).
- src/cli/ops.rs: plain `ops` errors against a pre-upgrade daemon whose `ops.list` reply has no `head`; consider an IPC version bump or tolerating a missing head in text mode.
- src/cli/roster.rs: parse `roster --live` scope words client-side so usage errors are reported before the daemon call.
- src/cli/ops.rs: `--seat/--teamspace` resolve before the daemon call, so with the daemon down an unknown name exits 1 rather than 5 (already recorded as a task 5 deferred minor).
- src/cli/instructions.rs `run`: early return for Stack plus an `unreachable!` arm (already recorded as a task 6 deferred minor).
- src/cli/instructions.rs `stack_is_read_only`: stats the real HOME/CLAUDE_CONFIG_DIR through `UserDirs::from_env()` (already recorded as a task 6 deferred minor).
