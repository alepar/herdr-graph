# Final review: cluster 5 (src-inspect), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 6eb6cec786cde8ca070fbf9aed46bb0609ff63ac (both verified with `git rev-parse --verify`). The review-source binding command exited 0. Caller checkout observed at HEAD 6eb6cec with a clean status; nothing was switched or restored.

Files reviewed (head blobs): src/inspect/{fixture,inventory,log,mod,output,roster,scope,sessions}.rs, plus slices 000029-000036. For named risks only, I also read src/store/history.rs, src/store/layout.rs, src/cli/{mod,log,sessions,roster,instructions}.rs and the design doc at docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md.

The review is static only. `toolbox run -c hg-build true` fails on this host with "unable to find user alepar: no matching entries in passwd file" (rc 255), so no tests were run (task 9's report hit the same failure). No test processes were started, so there is nothing to leak.

## Requirement baseline trace

The spec has a `## Requirement baseline` section with rows B1-B8 and one human scope change, S1, which affects B8 only.

- B1 "which seats are active": delivered. The production entry point is `herdr-graph roster`: src/cli/roster.rs, then `inspect::roster::build` (roster.rs:226), then `Inventory::load`. It shows lifecycle, availability, pane, occupant and threads state per clone and filters retired seats unless `--all`. The `--live` overlay (`overlay`, roster.rs:398) is in this cluster. Its daemon producer is src/daemon/roster_live.rs, which is the seam reviewer's to trace.
- B2 "which files are loaded into context per each - rules, roles, etc": NOT delivered.
  - roster.rs:290-295 renders the seat's instructions and then discards the result: every clone's `instructions` is always `InstructionsView::unknown()`, with `parts: []` and `omitted: []`.
  - `git ls-tree -r 6eb6cec src/inspect` has no stack.rs and no loaded.rs. src/cli/instructions.rs has only `show`, so `herdr-graph instructions stack <seat>` does not exist.
  - Nothing writes `.graph-local/loaded/`.
  - hg-blw.2 (instructions stack) and hg-blw.12 (loaded record) are open, and task 9 (hg-blw.12) reported BLOCKED. Task 6 (hg-blw.2) reported passing `inspect::stack` tests, but that work is not on the integration branch at head.
  - No human scope change row covers B2.
- B3 "state activity, latest modifications: global, space, seat": delivered.
  - `herdr-graph log [global|teamspace t|seat s]` runs src/cli/log.rs `execute`, then `LogQuery::run` and `HistoryWalker::walk`. Scope membership is decided at each commit's own tree (`Scope::paths_at`, scope.rs:205), with a fallback to the action's affected and retired objects.
  - The roster's `last_change` per seat (roster.rs:519 `fill_last_changes`) uses the same `LogQuery::matches`.
  - The journal side (`ops --seat/--teamspace`) is outside this cluster.
- B4 "list all sessions, by space/seat/clone, with timestamps and most recent first": delivered.
  - `herdr-graph sessions [scope|clone c]` runs src/cli/sessions.rs, then `Inventory::load` and `sessions::build`. It sorts by `started` descending, breaks ties by ns id, and includes retired and archived objects through `list_teamspaces`/`list_seats`, which read both the live and the `archive/` folders.
  - Transcripts fall back to the transcript record (`resolve_transcript`). The purge tombstone count goes in the footer and the JSON.
- B5 "Staleness is shown, not notified": NOT delivered. No code path produces `current` or `stale`: `InstructionsStatus::Current`/`Stale` are only rendered (roster.rs:586-592), never assigned outside tests. No human scope change row covers B5.
- B6 "`log` hides `bookkeeping` by default": delivered. `--all` defaults to false (src/cli/log.rs), and `LogQuery::matches` (log.rs:56-62) drops `Category::Bookkeeping` unless `all` is set. The roster's `last_change` passes `all: false`.
- B7 "Harness-native files: presence, size and mtime … never reads their content": NOT delivered. The only production surface for it is `instructions stack`'s `harness_files`, and that command is absent at head (see B2). No human scope change row covers B7.
- B8 "`roster --live` is in scope now" (with S1): the overlay logic in this cluster is correct.
  - `live_pane_matches` requires `Incarnation::same_server` plus the workspace/tab/pane nesting (roster.rs:438-450).
  - A binding from an earlier server is reported.
  - Degraded herdr and threads states fill `live_unavailable` and set every clone's `live` to null.
  - `--unmanaged` lists ids and labels only, as S1 asks, and collects bindings from an instance-wide Inventory.
  - Production reachability through the daemon handler is the seam reviewer's to trace.

## Code analysis

- Scope, Inventory, View/Envelope, LogQuery and Roster each keep to their stated responsibilities (see the key-abstractions digest).
- HEAD is pinned once and threaded through every call; `Inventory::load` and `HistoryWalker` take the `CommitId`.
- Malformed operation and action records become `malformed` rows with one warning, and the walk continues.
- `scan.stopped_early` is true only when the cap cut the walk short, not on a caller break. So `--since` and `--limit` stops correctly read as a complete scan, and `last_change_scanned` follows that rule.
- Scope path membership: archived seats live under `<ts_dir>/archive/seats/`, so teamspace scope covers them. Seat scope also matches both transcript layouts (`transcripts/<seat>/…` and `…/<slug>-<seat>/…`). `mutations/graph-changes/` is excluded from every scope.
- History cost and robustness: in `LogQuery::matches`, a non-instance scope calls `Scope::paths_at`, and `layout::locate` behind it parses every teamspace and seat record (for a clone, every clone record) of the historical tree. This runs once per scanned non-bookkeeping commit, and in the roster pass once per seat still pending.
  - Two consequences follow, both listed below. The cost scales with seats × commits × records on every daemon-free `roster`.
  - A single historical tree whose records no longer parse (corrupt, or an older record shape) returns a StoreError. That error fails the whole `roster` and `log seat …` (roster.rs:551-557, log.rs:108-113), even though spec §2.1 says one bad object never blanks a view.
- Text-rendering deviations from the spec samples are cosmetic:
  - `log` prints the requester's ids rather than names.
  - The roster cannot show `retired(reversible)`.
- Outside this cluster, for the cli cluster's reviewer: src/cli/mod.rs:219-226 `inspect_stubs_not_implemented` still runs `sessions` and expects a "not implemented" error. `sessions` is implemented at head, so this test fails: it returns Ok, or trips the real-HOME isolation check that task 6 reported.

## Concerns

- src/inspect/roster.rs:519-560 and src/inspect/log.rs:67-90: a history tree whose records fail to parse makes `paths_at`/`contains_object_at` return Err. That aborts the whole daemon-free `roster` (B1) and `log seat|teamspace`, instead of degrading only the affected seat's `last_change`. This contradicts spec §2.1 "one bad object never blanks a view".
- src/inspect/scope.rs:205-233 together with src/store/layout.rs:346-360: `paths_at` for seat or teamspace scope enumerates and parses every teamspace and seat record per scanned commit. The roster repeats this per pending seat, so a dormant seat whose last change is far back costs up to 5000 × (all records) TOML parses on every `roster` call. The perf-risk deferral in the digest covers `log`; it understates the roster multiplier.

## Verdict
not ready (B2, B5 and B7 are undelivered at head: no instructions stack, no loaded record, roster instructions always unknown)

## Must fix before landing
- B2 undelivered: at head, src/inspect/roster.rs:290-295 always sets `instructions` to `InstructionsView::unknown()` with empty `parts` and `omitted`; src/inspect/ has no stack.rs or loaded.rs; src/cli/instructions.rs has no `stack` subcommand; nothing writes `.graph-local/loaded/` (hg-blw.2 and hg-blw.12 are open, and task 9 is BLOCKED); no human scope change row covers it.
- B5 undelivered: no production path assigns `InstructionsStatus::Current` or `Stale`, so staleness is never shown in `roster` or anywhere else; it needs the hg-blw.12 LoadedInstructions record and status rule wired into roster.rs `build`; no human scope change row covers it.
- B7 undelivered: `herdr-graph instructions stack` with `harness_files` (path, bytes, mtime, never reading content) is absent at head; task 6's hg-blw.2 work is not on the integration branch; no human scope change row covers it.

## Untested scope
- No test of this cluster ran during the final review: the hg-build toolbox is unusable on this host (`toolbox run -c hg-build true` gives "unable to find user alepar", rc 255), so every pass claim rests on the per-task reports.
- Roster and log behavior when a historical commit's tree has an unparseable record (whole-command failure versus a degraded `last_change`) has no test.
- Roster `last_change` cost near the 5000-commit cap with many seats and a dormant seat has no measurement or test.
- Degraded `roster --live` first-line text through the CLI binary is not exercised (only at the overlay level and the in-process daemon level, per task 7).
- A seat-scoped `log` across a commit that deletes the seat folder outright (not archiving it) depends only on the action's affected fallback to the parent tree; no test covers a deletion commit without an action record.

## Deferred OK
- `log` text names the requester by ids (`by st_…/cl_…`) instead of the spec sample's names; the JSON carries the ids, so this is cosmetic.
- Roster text cannot show `retired(reversible)` (no field carries reversibility); already disclosed by task 2.
- In instance scope, an active teamspace with no listed seats is omitted from the roster entirely (roster.rs:363-371); harmless for "which seats are active", but an empty teamspace becomes invisible.
- The `--since` stop in `LogQuery::run` assumes commit times decrease monotonically along first-parent history; clock skew could end the walk early.
- src/cli/mod.rs:219-226 `inspect_stubs_not_implemented` still lists the implemented `sessions` command and will fail; it is outside this cluster, so it is routed to the cli cluster's or the seam reviewer.
