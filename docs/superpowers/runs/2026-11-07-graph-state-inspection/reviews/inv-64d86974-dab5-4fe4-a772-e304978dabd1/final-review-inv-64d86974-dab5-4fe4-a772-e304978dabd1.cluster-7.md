# Final review, cluster 7 (plan), epic hg-blw

## Scope reviewed

- Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298, both verified with `git rev-parse --verify <sha>^{commit}`. Review source bound through `dispatch-text.mjs bind-review-source` (exit 0).
- Cluster files: `src/plan/ops.rs` (slice 000077.diff, +268/-27). Read at head with `git show b9caa7f0:src/plan/ops.rs`.
- Caller checkout observed at HEAD b9caa7f07c309421854fc87241b76fc40cfda298 with a clean status; nothing in it was modified by this review. No review worktree was created; no build or test was run by this review (the change is a pure in-process filter over the journal; task-5-report.md records `cargo test --lib -- plan::ops cli::ops: 23 passed` including the eight new tests).

## What the change does

- `OpsFilter { seat, teamspace, clone, unresolved }` replaces the `unresolved: bool` argument of `list_ops`. Requester ids are compared as stored in `request.requester`, so ops of retired seats and teamspaces still match (spec §3.7 and design §2.4 "The filter is on requester").
- `limit` still applies after filtering: the loop breaks on `out.len() >= limit` before testing the next row, so `limit = 0` returns empty and filtered results are not truncated by unmatched rows.
- `pending_for` (used by `/seat`) is now `list_ops` with `OpsFilter { seat, clone, unresolved: true }` and `ALL`, as the design's Key abstractions entry for `plan::ops::list_ops` requires. Semantics are identical to the removed loop: same `journal.list(&[], ALL)` source, same requester predicate, same `is_unresolved` gate.
- `list_json` now takes the store and returns `{"head": store.head(), "ops": [...]}`; the CLI's `envelope_from_reply` builds the schema-1 envelope from that `head`. The `ops.list` handler parses optional `seat`/`teamspace` ids through the new `opt_id_arg` (absent or null means no filter; a present but unparseable value is a `bad_request` naming the key). `clone` is not exposed over IPC, which matches the design (`ops --seat/--teamspace/--limit/--json` only).
- The stale doc comment the design lists as a follow-up (`src/plan/ops.rs:446`, "the CLI's daemon-less read") is gone; the new comment describes the daemon-only payload. task-5-report.md says it was not touched, but the head source shows it replaced as a side effect of the `list_json` rewrite.

## Baseline trace

- B3 ("state activity, latest modifications: global, space, seat"), journal side: production entry `herdr-graph ops --seat <s> | --teamspace <t> [--unresolved] [--limit N] [--json]` (`src/cli/ops.rs` `list`) resolves names or ids to ids at a pinned HEAD, sends `{"unresolved","limit","seat"?,"teamspace"?}` to the daemon's `ops.list`, which this cluster implements; the handler builds the `OpsFilter` and returns the filtered rows plus `head`. The plan-side half is delivered. The CLI-side name resolution, exit codes and envelope belong to the cli cluster and the seam reviewer.
- Requester teamspace is populated at admission from the requesting seat's record (`src/plan/commands.rs` `requester_for`), and there is no seat-move kind in `RequestKind`, so filtering on the stored teamspace cannot drift from the seat's actual teamspace. Ops with no seat in the caller environment (human at a terminal, observed, bookkeeping, reconciler) carry no teamspace and are excluded from a teamspace filter; that is the design's stated "filter is on requester" contract, and the git-side `log` covers scope by path.
- B1, B2, B4 to B8 have no code in this cluster.

## Analysis notes

- Tests in the cluster cover: seat-only, teamspace-only, unknown teamspace, seat plus teamspace, contradictory seat plus teamspace, unresolved combined with seat, limit 1 and 0 after filtering, retired requester still matching, `list_json` reporting the head and adding no commit, and the IPC handler end to end (no args, seat, seat plus limit, teamspace plus unresolved, head equality, unparseable seat error).
- `list_json` reads `store.head()` before the journal listing. An op can commit between the two reads, so a listed op's state may reflect a commit newer than the reported `head`. The journal is not in Git, so `head` here is context rather than a consistency point; harmless, noted only so nobody reads `head` as a snapshot of the journal.
- The `ops.list` handler calls `store.head()` (a Git read) and the SQLite scan directly on the async executor, while `ops.reassign` and `ops.check_instruction` wrap store access in `spawn_blocking`. `ops.get` already did the same before this change, and the cost is one ref read, so this is a style inconsistency rather than a defect.
- No new Git writes: `list_json_reports_head_and_adds_no_commit` asserts head is unchanged after the call.

## Findings

- none

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- The IPC handler rejecting an unparseable `teamspace` value is not asserted (only `seat: "nonsense"` is); it shares `opt_id_arg` with `seat`, so the risk is low.

## Deferred OK
- `ops.list` performs `store.head()` and the journal scan on the async executor instead of `spawn_blocking`, unlike `ops.reassign`/`ops.check_instruction`; same pattern as the existing `ops.get`, cost is one ref read.
- `head` in the `ops.list` reply is read before the journal scan, so it is not a consistent snapshot of journal state; acceptable because the journal is outside Git.
