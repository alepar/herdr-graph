# Final review: cluster 8 (readme), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298 (both verified with `git rev-parse --verify <sha>^{commit}`). Cluster input: `slices/000001.diff` (README.md, +64 lines: the new "Inspection" section and one paragraph under "Test tiers"). Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md at head, which has a `## Requirement baseline` section (B1-B8, S1).

Caller checkout observed at head b9caa7f, working tree clean (`git status --short` empty). No review worktree was needed; all reads used `git show <head>:<path>` and `git grep <head>`.

## Method

Each README claim was checked against the code at head: the clap definitions in src/cli/{roster,log,sessions,ops,instructions,setup}.rs, the emit path in src/inspect/output.rs, the status rule in src/inspect/loaded.rs, the live overlay in src/inspect/roster.rs and src/daemon/roster_live.rs, `parse_since` in src/inspect/mod.rs, the sessions view in src/inspect/sessions.rs, and the test files and Cargo features at head.

## Claims confirmed against head

- Scope grammar: `roster`, `log` and `sessions` take 0..=2 scope words. Only `sessions` passes `AllowClone::Yes`. `roster clone x` is exit 1 with a `herdr-graph:` usage line (tests/inspect_roster.rs `roster_clone_scope_is_usage_error`), which matches the README's "usage error" and the exit-code table.
- Daemon needs: `roster --live` calls `roster.live` with `CallMode::NoEnsure` and reports a down daemon through `daemon_error::report` (exit 5). `ops` goes through the daemon. The other views are daemon-free.
- Flags and defaults: `--unmanaged` has `requires = "live"`. `log --limit` defaults to 30 and `--max-scan` to 5000. `sessions --limit` defaults to 50. `--since` accepts RFC 3339, `YYYY-MM-DD` and `<n>{s,m,h,d,w}`. `instructions stack <seat> [--clone] [--json]` exists. `ops` has `--seat`, `--teamspace`, `--limit` and `--json`, with seat and teamspace resolved by name or id, retired included.
- Text strings appear verbatim in code: `binding from earlier herdr server`, `session not identified`, `codex session id form differs`, `hook predates --stdin-payload; rerun setup`, `scanned N commits, older history not searched`, `present, not necessarily loaded`, and `imports_followed: false`.
- Loaded-status table: it matches `loaded::status`. `unknown` covers a missing or unreadable record, a null id, no occupant and an id mismatch. `current` and `stale` compare against `fit_hook` at HOOK_RENDER_CAP. `truncated` is set from `omitted` or a `cut` part. The flagless note comes from `stdin_payload: false`.
- `setup codex` prints a trust notice when the hook is installed or updated (src/cli/setup.rs `codex_trust_note`).
- Sessions JSON `{tombstones, total, rows}` is inside the envelope. The text footer `tombstones in scope: N` and the transcript fallback `unresolved` are both present.
- Requirement baseline: B1 (roster), B2 (instructions stack and the loaded record), B3 (log with scopes and `--limit 1` recipes), B4 (sessions by scope or clone, newest first), B5 (staleness shown as status, no notification), B6 (bookkeeping hidden unless `--all`), B7 (harness files listed by path, size and mtime; content never read) and B8 with S1 (`roster --live`, and `--unmanaged` with ids and labels only) are each documented. Each has a registered production entry point at head: clap subcommands in src/cli/mod.rs, and `roster.live` registered in src/daemon/compose.rs. Behavioral delivery past those entry points belongs to the code clusters and the seam reviewer.

## Findings

- README "Test tiers" paragraph (README.md:231 at head) says "The inspection views add one tier-3 test, `tests/inspect_live.rs`". But `tests/inspect_sweep.rs` (818 lines, `#![cfg(feature = "private-herdr")]`, its own run line `cargo test --features private-herdr --test inspect_sweep -- --test-threads=1`) was added in 1163f89/ba17a28, after the README commit 111b9ae. It is a second tier-3 test, and it covers R12 (read-only), the `--json` contract, the pinned HEAD under a concurrent writer and R14 through `setup claude`. Anyone who runs only the documented per-file command skips the sweep.
- README roster bullet "When the Herdr snapshot or the threads view is unavailable, the committed roster is still printed, every clone's `live` is `null`, and `live_unavailable` names each reason" misstates the JSON contract for the threads-only case. `inspect::roster::overlay` sets each clone's `live` to null only on a Herdr snapshot failure. When only the threads seat view is unavailable, every clone's `live` is still filled in. Only `threads_live.seat_refusals` and `threads_live.seat_lost` are null, with `live_unavailable.threads` set, as spec §2.2 says. A JSON consumer that follows the README and treats `live == null` as "threads down" will misread the output.
- README `--json` paragraph says notices with no JSON field, naming "a warning about a skipped record" as one, go to stderr under `--json` and "In text mode the same notices are part of the output". At head, `cli::log::execute` writes HistoryWalker warnings to stderr in both modes, before `emit`. Only the ops pointer goes through `emit` and lands on stdout in text mode. The README follows spec §2.1, so the code is what deviates. Which side should change is for the code-cluster or seam reviewer to decide. As written, the README describes behavior that does not happen for skipped-record warnings.
- Minor wording: the README gives the degraded text first line as `live data unavailable: <reason>`. The code prints `live data unavailable: herdr: <reason>` (and `; threads: <reason>`), as spec §2.2 shows. The README's `<reason>` placeholder drops the source key.
- Minor omission: `ops --limit` defaults to 200 at head (src/cli/ops.rs), and the README `ops filters` usage line does not say so. The README gives the defaults for `log` and `sessions`.

## Verdict

not ready (README states one tier-3 inspection test though inspect_sweep.rs is a second, and misstates clone `live` nulling under threads-only degradation)

## Must fix before landing

- README.md:231 says inspection adds "one tier-3 test, `tests/inspect_live.rs`"; tests/inspect_sweep.rs (private-herdr tier-3 sweep added in 1163f89 after README commit 111b9ae) must be listed with its run command.
- README roster `--live` degraded-state bullet claims every clone's `live` is null when the threads view is unavailable; per `inspect::roster::overlay` and spec §2.2 only a Herdr snapshot failure nulls clone `live`, while threads unavailability nulls `threads_live.seat_refusals`/`seat_lost` and sets `live_unavailable.threads`; correct the JSON contract text.

## Untested scope

- The README's test-tier commands (`--test inspect_live`, the tier-1 `inspect_loaded` claim) were checked by reading the cfg gates and file headers at head, not by running them.
- The README's example recipes (`log global --limit 1`, `log teamspace proj --limit 1`, `log seat a --by b`) were checked against the clap definitions and LogQuery wiring only, not run against an instance.

## Deferred OK

- The README says skipped-record warnings are part of text output, but `cli::log::execute` sends them to stderr in both modes; the README matches spec §2.1, so the code-cluster or seam reviewer decides whether code or README changes.
- The README writes the degraded first line as `live data unavailable: <reason>` where the code prints `live data unavailable: herdr: <reason>`; a wording nit.
- The README does not give the `ops --limit` default (200); the omission is harmless.
