# Final review, cluster 2 (cli): epic hg-blw, integration branch super-auto/graph-state-inspection

Invocation inv-64d86974-dab5-4fe4-a772-e304978dabd1. Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298, both verified with `git rev-parse --verify`. The review source binding to progress.md succeeded before this file was written.

## Scope and method

- Files: src/cli/instructions.rs, log.rs, mod.rs, ops.rs, roster.rs, seat.rs, sessions.rs, setup.rs (slices 000052 to 000059 of the assembled bundle).
- Acceptance standard: the design doc's `## Requirement baseline` (B1 to B8, S1), plus §2 and Key abstractions. I opened inspect/output.rs, inspect/mod.rs, inspect/roster.rs (the `Roster` serde shape), inspect/stack.rs (the `--clone` membership check), inspect/loaded.rs (`record`), plan/ops.rs (`list_json` head), bootstrap/setup_claude.rs (hook commands) and tests/inspect_sweep.rs, but only to check specific risks.
- Since the previous final review head (2dfa656), only help-line doc comments and the `help_lines_present` test changed in this cluster. The rest of the change was tests/inspect_sweep.rs.
- I ran `toolbox run -c hg-build cargo test --lib cli::` in a review-owned detached worktree at the head (target seeded by reflink). Result: 70 passed, 0 failed. The leaked-rig check printed 0, and I then removed the worktree. The caller checkout stayed at b9caa7f, clean, before and after.

## Baseline trace from CLI entry points

- B1 (which seats are active): `herdr-graph roster [<scope>] [--all] [--json]` goes cli::roster::run, then execute (pin_head once, then ScopeArg::parse with AllowClone::No), then Scope::resolve, then inspect::roster::build, then the Envelope `with_live(false)`. This is daemon-free. Delivered.
- B2 (files loaded per seat): there are three CLI entry points.
  - `instructions stack <seat> [--clone] [--json]`: cli::instructions::stack pins HEAD once, resolves the seat (retired allowed) and the optional clone, then calls inspect::stack::build with `UserDirs::from_env()`.
  - The per-clone roster `instructions` object.
  - The only writer, `seat --hook-prompt [--codex] --stdin-payload`. hook_prompt prints and flushes the unchanged `fit_hook` text first. Then, for a `Resolution::Bound` caller only, it reads stdin (capped at 64 KiB and 2 s, on a helper thread) and calls `loaded::record`, which swallows every error. `seat --instructions` and `seat --json` never reach `record`. Without `--stdin-payload`, stdin is never read, so an older installed hook still feeds `session-report`. Delivered in this cluster. The installed Claude command's own `p=$(cat)` read lives in bootstrap/setup_claude.rs; that is the seam reviewer's.
- B3 (activity: global, space, seat): `log [<scope>] [--since] [--limit 30] [--by] [--all] [--max-scan 5000] [--json]` goes cli::log::execute, then pin_head, Scope and `--by` resolution at that head, then LogQuery::run over HistoryWalker, then emit with OPS_POINTER as the trailing notice. That notice goes to stdout in text mode and to stderr under `--json`. Warnings go to stderr. `ops --seat/--teamspace/--limit/--json` resolves names to ids at one pinned HEAD, then calls `ops.list`, then builds an envelope whose `head` comes from the daemon reply (plan/ops.rs list_json sets it). Delivered.
- B4 (sessions by space/seat/clone, newest first): `sessions [<scope> | clone <c>] [--since] [--limit 50] [--json]` goes cli::sessions::prepare (AllowClone::Yes), then Inventory::load, then inspect::sessions::build. Delivered.
- B5 (staleness shown, not notified): no CLI path in this cluster sends a notice. Delivered.
- B6 (log hides bookkeeping by default): `--all` defaults to false and passes straight into `LogQuery.all`. The sweep test checks the default view against the `--all` view. Delivered.
- B7 (harness files: presence, size, mtime only): stack passes the inspector's UserDirs and reads no content in the CLI layer. Discovery correctness belongs to the inspect cluster. Delivered as far as this cluster goes.
- B8 and S1 (`roster --live`, `--unmanaged` enumerate only): with `--live`, cli::roster::live makes one `roster.live` call with CallMode::NoEnsure. A down daemon goes through `daemon_error::report` (exit 5). The reply's `roster` is deserialized into the same `Roster` type: `live_unavailable` always present, `scan`, `threads_live` and `unmanaged` with absent-versus-null kept by `present_or_null`. It is printed with `with_live(true)`. clap enforces `--unmanaged requires --live`. Delivered at the CLI boundary. The overlay content (the threads-degraded state, incarnation-aware binding) belongs to the daemon cluster and the seam.

## Analysis of risks

- Single HEAD rule: every daemon-free command in this cluster calls `pin_head` once and passes the same CommitId to Scope, the builders and the Envelope. `ops` and `roster --live` take `head` from the daemon reply rather than reading HEAD again. This is consistent with §2.1.
- JSON contract: every view emits through `inspect::output::emit` or `print`, so `--json` writes exactly one document. Notices with no field go to stderr, and text mode appends them. No command builds the envelope by hand.
- Exit codes: no instance gives EXIT_NO_INSTANCE. Store and scope errors go through `inspect::fail`, which gives 6 for an integrity failure (bare or wrapped in InstructionError::Store) and 1 otherwise. A down daemon for `--live` and `ops` goes through daemon_error (exit 5).
- Hook never fails: hook_prompt still returns `ExitCode::SUCCESS` on every path. The record runs only after the output is flushed. The stdin helper thread is abandoned on timeout, and process exit ends it.
- `instructions stack --clone <c>` resolves the clone across the whole instance and only then checks membership (stack.rs bails with "clone … is not a clone of seat …"). A clone name that repeats across seats (for example `main`) is therefore ambiguous by name, and the id form is needed. The output is still correct, so the prior review's deferral stands.
- The new `seat --json` help line, "Print one JSON document (schema 1) on stdout; notices go to stderr.", is wrong. `seat --json` pretty-prints the raw `seat.resolve` reply, which has no `schema` or `head` envelope (cli/seat.rs, `serde_json::to_string_pretty(&reply)`). It is a doc-only defect on a command that already existed.
- setup codex: `codex_trust_note` prints the re-trust notice on Installed or Updated, and the older generic notice on Unchanged. It never prints on uninstall, which matches the old behavior and §2.3 Honest limits.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- src/cli/ops.rs: the only end-to-end `ops --seat foreman --json` check, in tests/inspect_sweep.rs, asserts the `ops` key exists; no test runs the binary against requester-attributed journal ops to check the seat or teamspace filtering.
- src/cli/setup.rs `codex_trust_note`: no test covers the Installed/Updated versus Unchanged notice.
- src/cli/instructions.rs `instructions stack --clone <name>`: no test covers a clone name shared by two seats (the ambiguity path).
- src/cli/roster.rs `live()`: no CLI-level test covers a daemon reply with no `head`, or a daemon-side scope error and its exit code.
- src/cli/seat.rs: the hook record path for an unbound caller with `--stdin-payload` has no explicit test that stdin stays unread.

## Deferred OK
- src/cli/seat.rs: the `seat --json` help line claims a schema-1 document, but the command prints the raw pretty-printed `seat.resolve` reply; reword it, for example to "Print the daemon's seat reply as JSON".
- src/cli/instructions.rs: resolve `--clone` among the named seat's clones instead of instance-wide, so a clone name repeated across seats works without its id.
- src/cli/ops.rs: plain `ops` in text mode errors when a pre-upgrade daemon's `ops.list` reply has no `head`; tolerate a missing head in text mode or bump the IPC version.
- src/cli/ops.rs: `--seat/--teamspace` resolve before the daemon call, so with the daemon down an unknown name exits 1 rather than 5.
- src/cli/roster.rs: parse the `roster --live` scope words client-side, so usage errors are reported before the daemon call.
- src/cli/instructions.rs `run`: the early return for Stack plus an `unreachable!` arm, and `stack_is_read_only` stats the real HOME through `UserDirs::from_env()`.
- Seam (not this cluster): the installed Claude hook command `p=$(cat); …` in src/bootstrap/setup_claude.rs reads stdin without a bound before the prompt prints. The binary side in src/cli/seat.rs bounds its own read (64 KiB, 2 s, after printing). The previous final review's must-fix item on that command is for the bootstrap or seam reviewer to confirm.
