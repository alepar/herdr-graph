# Final review: cluster 7 (plan), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 2dfa656c7f98715fe66ca53c253de19f369a857d (both verified with `git rev-parse --verify`). Review source bound in the ledger (bind-review-source exit 0). Caller checkout observed at HEAD 2dfa656c, status clean, before and during the review; it was not switched or modified.

Cluster input: `src/plan/ops.rs` (slice 000059.diff), one commit f5a349b (task 5, hg-blw.8).

## Scope of the change

- `OpsFilter { seat, teamspace, clone, unresolved }` with requester matching by stored id; `list_ops(journal, &OpsFilter, limit)` replaces the `unresolved: bool` signature, limit applied after filtering, newest first as before.
- `pending_for` (used by `/seat` in `src/bootstrap/mod.rs:354`) now builds an `OpsFilter { seat, clone, unresolved: true }` and calls `list_ops(.., ALL)`: same iteration order, same predicate as the removed loop, so `/seat`'s pending list is unchanged.
- `list_json` now returns `{head, ops}`, reading `store.head()` first, and maps errors through `OpsError` (Journal and Store both convert via `#[from]`).
- `ops.list` handler reads optional `seat` / `teamspace` ids (`opt_id_arg`: absent or null means no filter; an unparseable id is `bad_request` naming the key), keeps `unresolved` and the default limit of 200.
- The stale doc comment the design called out (§3, `src/plan/ops.rs:446`, "the CLI's daemon-less read") is gone; the new comment describes the daemon reply only.

## Spec trace

- Design §2.4 "Journal side": `ops` gains `--seat`, `--teamspace`, `--limit`, `--json`, through the daemon, filter on requester. The daemon half is here: filters on `Requester.seat` / `Requester.teamspace`, limit after filter, `head` in the reply for the envelope. Key abstractions entry for `plan::ops::list_ops` ("takes a requester filter (seat, teamspace, unresolved) and a limit; `pending_for` becomes this function with a filter instead of a second copy of the loop") is met exactly.
- Requirement baseline B3 ("latest modifications: global, space, seat"): the journal-side part of seat/teamspace scoping is delivered by this handler. Production producer of the matched field checked: `requester_for` (`src/plan/commands.rs:99-126`) fills `seat`/`clone` from `HERDR_GRAPH_*` and `teamspace` from the seat record at HEAD; `reassign` (`src/plan/ops.rs:295`) sets both seat and teamspace. So a teamspace filter matches real agent-requested ops, not only test fixtures. Ops with no requester (observed, bookkeeping, reconciler, a human terminal without `HERDR_GRAPH_SEAT`) match only the unfiltered listing, which is what "the filter is on requester" means.
- The CLI entry point (`src/cli/ops.rs`: name resolution through Scope at a pinned HEAD, then `ops.list` with `seat`/`teamspace` ids, envelope head from the reply) is in the cli cluster; the CLI-to-handler argument contract (keys `seat`, `teamspace`, `limit`, `unresolved`, ids as strings) matches what this handler parses. That seam is the seam reviewer's to confirm end to end.
- §2.1 "No new Git writes": `list_json` only reads `store.head()`; `list_json_reports_head_and_adds_no_commit` asserts head is unchanged.
- §3.7 retired requesters stay listed: matching is by stored id with no lifecycle lookup; `retired_requester_still_matches` covers a seat retired after its op.

## Test evidence

- New unit tests in the cluster: `filter_by_seat_and_teamspace`, `filter_combines_with_unresolved_and_limit` (includes limit 0 and limit after filter), `retired_requester_still_matches`, `list_json_reports_head_and_adds_no_commit`, `ipc_list_filters_by_seat_teamspace_and_limit` (drives the registered `ops.list` handler: unfiltered, seat, seat+limit, teamspace+unresolved, `head` in reply, bad seat id is an error naming `seat`). Existing tests were mechanically moved to the new signature.
- Task 5 report: `cargo test --lib -- plan::ops cli::ops` 23 passed on pre-commit tree c2eb13a; f5a349b is the only commit touching this file in the range. Not re-run in this review, to avoid a build in the caller worktree or a fresh target dir.

## Findings

- `list_json` doc comment says `head` is "the commit the daemon read the listing against", but the journal is not versioned by commit: `store.head()` is read before `journal.list`, so a writer commit landing between the two can show an op as `committed` whose commit is newer than the reported `head`. The value is a useful stamp, not a consistency guarantee; the comment overstates it (src/plan/ops.rs, `list_json`).
- `ops.list` still parses `limit` and `unresolved` leniently (`as_u64` / `as_bool` with defaults), so a wrong-typed `limit` silently becomes 200 and a wrong-typed `unresolved` becomes false, while a bad `seat`/`teamspace` id is now a `bad_request`. Pre-existing for `limit`/`unresolved`; the CLI always sends well-typed values, so no production path hits it.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No end-to-end test runs `herdr-graph ops --seat/--teamspace --json` against a live daemon with several requester-attributed ops; the handler is tested through the Registry and the CLI separately (task 5 noted it as not added). Seam reviewer's to weigh.
- Tests were not re-run at head 2dfa656c in this review; the passing run cited is task 5's on pre-commit tree c2eb13a.

## Deferred OK
- Reword the `list_json` doc comment so `head` is described as the store head read just before the journal listing, not a commit the listing is consistent with.
- Make `ops.list` reject a wrong-typed `limit` or `unresolved` with `bad_request`, matching the new strictness for `seat`/`teamspace`.
