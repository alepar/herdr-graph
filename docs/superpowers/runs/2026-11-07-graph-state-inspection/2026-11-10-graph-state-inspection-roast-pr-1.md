super-roast verdict: Should-fix (13 confirmed) [low coverage] [review readiness incomplete]
mode: PR        iteration: 1 of 3
profile (assumed): herdr-graph 0.3.0 is a local, single-operator Herdr plugin: a Rust CLI plus a long-running per-user daemon over a Git-backed instance repository, reached over a local socket. It has no network-exposed surface and no multi-tenant users, and its README documents the trust gate as cooperative. The data at stake is the operator's own graph repository, which Git history keeps recoverable, and the code under review is read-only inspection views. Blast radius is low, so resilience, observability and cost findings are rated Nit unless they contradict the change's own stated spec contract.
inputs: super-auto/graph-state-inspection@63a57b5 vs main@bfeb8f3 (bounded: product paths only)
coverage: scouts 13/13 (correctness, security, premortem, simplicity-design, hot-path-perf, concurrency-async, api-contract, observability, testing, test-causality-lifecycle, test-isolation-environment, test-suite-cost, hygiene-docs) · raw 107 → deduped 89 → panel 25 · spot 48 · promoted 2 · judge completion 100% · remainder-capped: 14 · review readiness: coverage bounded, provenance unavailable — basis: "review input selection: provenance unavailable; captured source available (run.md:83 inv-64d86974 incomplete claimed collection: final-review cluster 13 never written); inspected: all 43 product paths outside docs/superpowers/runs/ at bfeb8f3104dd3783d67a46570c9f272e935012b5..63a57b57ecf1c0eadff1cb5f438492e1db6df3d5, every hunk; unreviewed: 284 run-record paths under docs/superpowers/runs/ (listed in /var/home/alepar/AleCode/herdr-graph/.worktrees/super-auto/graph-state-inspection/.superpowers/sdd/hg-blw-plan/assembled/roast-code-r1-bounded/unreviewed-paths.txt)"
independence: same-family (Claude) — seat-differentiated panel · rung: Workflow
seat-agreement: panels 27 · rr 0.89 · rg 0.78 · fg 0.81 · unanimous 0.74 · ground-loo 0.83 (n=24) · reproduce 13/14/0 · refute 12/15/0 · ground 17/10/0
lane-yield (found/confirmed/unique/refuted): correctness 8/4/2/3 · security 3/0/0/1 · premortem 7/3/0/4 · simplicity-design 27/2/1/7 · hot-path-perf 5/3/1/2 · concurrency-async 6/1/1/5 · api-contract 11/1/1/9 · observability 8/0/0/7 · testing 10/2/1/4 · test-causality-lifecycle 4/1/1/3 · test-isolation-environment 4/0/0/2 · test-suite-cost 4/0/0/2 · hygiene-docs 10/1/1/3

## Confirmed findings            ← consumed by super-design, one task per finding
- [Should-fix] src/inspect/log.rs:68; src/inspect/roster.rs:580 — `log <scope>` and every `roster` (including `roster --live`) parse each scanned historical commit's records with today's strict parsers, so one unparseable or duplicate-id record anywhere in the scan window aborts the whole command instead of degrading a row or the decorative last_change column. [lanes: correctness, premortem]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: `LogQuery::matches` (src/inspect/log.rs ~68) calls `self.scope.paths_at(&tree)?` and `contains_object_at(...)?` on each commit's own tree.
  evidence: `Scope::paths_at` (src/inspect/scope.rs:209-217) calls `layout::locate`, which reaches `folder_records` and `read_toml::<R>(..)?` (src/store/layout.rs:152-157); `file_records` (layout.rs:167) rejects duplicate ids as `StoreError::Corrupt`.
  evidence: `fill_last_changes` (src/inspect/roster.rs:566-581) does `failed.get_or_insert(e)` then `return Err(e)`; `build_inner` (roster.rs:386) propagates with `?`; the roster always runs this pass with `Scope::Seat`.
  evidence: spec §2.1 says "One bad object never blanks a view" and limits whole-command exits to no instance, HEAD unreadable, HEAD enumeration failure and usage errors. src/inspect/loaded.rs:296-303 already tolerates the same class of failure with `unwrap_or_default()`.
  evidence: not reproduced by any seat; the trigger (a past commit holding a record today's deserializer rejects) is plausible, not observed. That is why all three seats rated Should-fix and not Blocking.
  fix-shape hint: treat a `matches` error on a historical tree as a degraded state (roster: `last_change: null` plus a scan/warning signal; log: a marked or skipped row) instead of propagating it out of the walk.
- [Should-fix] src/daemon/roster_live.rs:64 — `roster.live` reports `ThreadsRead::Ok` with the refusals and lost seats of the last successful threads read when the threads seat view is failing with transport errors (Disconnected/Busy/timeout), because LazyMap only records Rejected/Unsupported in `seat_view_error`; stale shared state is presented as live, against the spec's 'live_unavailable.threads names the reason'. [lanes: concurrency-async, premortem]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/daemon/roster_live.rs:64-81 returns `ThreadsRead::Unavailable` only when `seat_view_error` is `Some`; otherwise `Ok` with the in-memory `refusals` and `lost` slots.
  evidence: src/daemon/compose.rs `LazyMap::inspect` (~1088-1121) sets the slot only for `Rejected`/`Unsupported`, clears it on `Ok`, and has `Err(_) => {}` with the comment "transient transport errors leave it as it was". `seat_refusals` is overwritten only `if read.is_ok()`.
  evidence: design §2.2 line 205: "threads seat view not running: seat_refusals/seat_lost are null and live_unavailable.threads names the reason." A never-started threads service returns `Ok` with empty lists; a service that went down returns the last-read lists with no read age.
  evidence: the handler reads the three slots under three separate locks, so one reply can mix two reads (secondary).
  severity: seats rated Should-fix / Nit / Should-fix. Kept at Should-fix because it contradicts a quoted spec sentence, not only an observability preference.
  fix-shape hint: have `LazyMap::inspect` record the outcome of every read (a last-read-ok flag or timestamp) for `roster.live` to consult, without changing what the existing slot means to the `status` provider; read the slots under one lock.
- [Should-fix] src/inspect/loaded.rs:296 — `loaded::clone_view` hard-codes its own `HistoryWalker::new(..., DEFAULT_MAX_SCAN).last_touches` per stale clone, so a roster or stack with N stale clones repeats the same up-to-5000-commit diff walk N times, ignores the caller's injected max_scan, and contradicts roster's documented single walker pass. [lanes: premortem, hot-path-perf, simplicity-design]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/inspect/loaded.rs:283-306 builds `HistoryWalker::new(store, at, HistoryWalker::DEFAULT_MAX_SCAN).last_touches(paths).unwrap_or_default()` inside the closure passed to `status`; the caller cannot inject a cap or a walker.
  evidence: src/inspect/roster.rs:322 calls `loaded::clone_view` once per clone in the per-seat loop; src/inspect/stack.rs:153 does the same and stack.rs:199 adds a separate manifest walk.
  evidence: src/inspect/roster.rs:33-35 says the module "runs one `HistoryWalker` pass"; the design's roster Cost section says the instruction status column is "still only tree reads". `build_with_cap(max_scan)` reaches only `fill_last_changes`, so tests that set a small cap do not cover the stale-part walks.
  evidence: one template or rules edit makes every clone of the seat stale at once; this also runs on the `roster.live` daemon path.
  severity: seats rated Nit / Should-fix / Should-fix. Kept at Should-fix despite the low-blast-radius profile because it contradicts the module's documented single-pass contract and bypasses the injected cap.
  fix-shape hint: compute the union of stale paths once per roster or stack build and run one batched `last_touches` with the caller's cap, or pass the walker into `clone_view`.
- [Should-fix] tests/inspect_sweep.rs:568 — The only binary-level test of `ops --seat` checks just that an `ops` key exists. No test shows that the CLI's name-to-id resolution (`resolve_filters`) reaches the daemon and filters the listing, and `ops --teamspace` is never run through the CLI. [lanes: testing]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: tests/inspect_sweep.rs:568-569 runs `rig.json(&["ops","--seat","foreman","--limit","5"])` then `assert_keys("ops",&ops,&["ops"])`; no row is asserted although the same test has just applied a seat-requested rename.
  evidence: tests/inspect_sweep.rs:330 is an envelope/read-only check only. tests/daemon_free_reads.rs:71 excludes the filtered forms. A grep of tests/ finds no `ops` invocation with `--teamspace`.
  evidence: `resolve_filters` (src/cli/ops.rs:154) has no test; filtering is tested only with ids at the IPC layer (`ipc_list_filters_by_seat_teamspace_and_limit`). Dropping the `seat` key would still pass every test.
  severity: seats rated Should-fix / Should-fix / Nit. No seat ran the tests.
  fix-shape hint: in the sweep test assert the `ops --seat foreman` rows contain the seat-requested op and that a non-matching filter returns none; add one `ops --teamspace` run through the binary.
- [Nit] src/inspect/roster.rs:554 — The roster's last_change pass calls LogQuery::matches per pending seat per scanned commit, each running an uncached layout::locate that parses every teamspace and seat record in that commit's tree, so one quiet seat costs O(commits x seats x instance size) up to the 5000-commit cap on every `roster` and `roster --live`. [lanes: premortem, hot-path-perf]
  verdict: confirmed (reproduce REJECT-outvoted / refute ✗-survived / ground ✓)
  evidence: src/inspect/roster.rs:554-575 calls `q.matches(store, c)` for each seat whose `last_change` is still `None`; src/inspect/scope.rs:216-231 calls the free `layout::locate(tree, &id)`; src/store/layout.rs:359 `IdKind::Seat => find_by_id(all_seats(tr), id)`; `LOCATE_CACHE` wraps only `GitStore::locate` in src/store/git.rs.
  evidence: the walk stops early only when every listed seat has matched; `DEFAULT_MAX_SCAN` = 5000 (src/store/history.rs:100). Not benchmarked by any seat.
  evidence: the reproduce seat rejected on spec §2.4 line 354 ("No index is proposed"). The refute seat answered that the spec's stated cost model is "one commit read and one tree diff per scanned commit" and does not describe a per-seat, per-commit full-record parse.
  demoted: seats split Should-fix / Nit; the profile is a local single-operator CLI with no latency requirement, the cost is unmeasured and bounded by the 5000 cap, so this cost finding is a Nit here.
  fix-shape hint: resolve seat folders once per commit tree and share the result across seats, or memoize `locate` per tree. The fix for the strict-parse finding touches the same loop.
- [Nit] src/store/history.rs:130 — HistoryWalker::commits sets Sort::TOPOLOGICAL, which makes libgit2 traverse and topologically sort the entire reachable history before it yields the first commit. The max_scan cap (default 5000) therefore does not bound the revwalk cost: every walker pays O(total history), including `log --limit 1` and walks that break after one commit. [lanes: hot-path-perf]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/store/history.rs:127-141 sets `Sort::TOPOLOGICAL`, then `simplify_first_parent`, `push(from)`, and collects ids up to the cap.
  evidence: vendored libgit2-sys-0.18.8+1.9.7 `libgit2/src/libgit2/revwalk.c`: `if (walk->sorting != GIT_SORT_NONE) walk->limited = 1;` (~line 763); `prepare_walk` runs `limit_list` (line 660) and `sort_in_topological_order` (line 663) before the first `next()`.
  evidence: the design says "The scan cap bounds the cost" (line 354 and the alternatives table at line 440); that does not hold for the revwalk. The 50k-commit timing spike was not run, so the size of the penalty is unknown.
  demoted: the ground seat rated Should-fix; two seats rated Nit. The profile has no latency requirement and the spec accepts slow long histories, so an unmeasured cost is a Nit here.
  fix-shape hint: drop the `set_sorting` call (or use `Sort::NONE`) and keep `simplify_first_parent`; on a first-parent linear history the order is the same and the walk is lazy.
- [Nit] src/inspect/log.rs:98 — `log --since` breaks the walk at the first commit whose wall-clock time is older than `since` and still reports a complete scan, although commit times are not monotonic; no test covers commit times that go backwards. [lanes: correctness, testing]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/inspect/log.rs:98 `if kept.len() >= self.limit || self.since.is_some_and(|s| c.at < s) { return ControlFlow::Break(()) }`, doc comment "history is time-ordered".
  evidence: src/store/history.rs:130 walks topologically, not by time; a Break returns `report(scanned, false)`, so `stopped_early` is false. `since_and_limit` (log.rs:621) uses monotonic days only.
  evidence: needs a backwards clock step between writer commits; spec §2.4 does not specify break versus skip. All three seats rated Nit.
  fix-shape hint: skip commits older than `since` instead of breaking, or document the assumption and add a test with out-of-order commit times.
- [Nit] src/daemon/roster_live.rs:52 — `roster --live` maps every scope-resolution error to BadRequest, including a graph integrity StoreError wrapped in InstructionError::Store. A broken graph therefore exits 1 with a generic line under `--live`, while the daemon-free `roster` exits 6 with the integrity line for the same state. [lanes: correctness]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/daemon/roster_live.rs:51-52 `Scope::resolve(...).map_err(|e| CommandError::bad_request(e.to_string()))?`; `Scope::resolve` returns `InstructionError` (src/inspect/scope.rs:117), which has a `Store(#[from] StoreError)` variant.
  evidence: src/cli/daemon_error.rs:92-98 maps `BadRequest` to exit 1; src/inspect/mod.rs:70-78 `integrity_of` unwraps `InstructionError::Store` to exit 6. The builder call two lines below already uses `CommandError::from_store` (src/daemon/registry.rs:68).
  evidence: design line 581: "Exit 6 for an integrity failure that leaves nothing to show."
  severity: seats rated Nit / Should-fix / Nit. Only a scoped `--live` roster on a corrupt graph is affected, and the error text still passes through.
  fix-shape hint: match `InstructionError::Store(s)` and route it through `CommandError::from_store`; keep `bad_request` for the rest.
- [Nit] src/inspect/stack.rs:199 — `instructions stack` fails the whole command when the history last-touch lookup errors, while the roster and stack per-clone status treat the same lookup as optional and drop only the commit column. [lanes: correctness]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/inspect/stack.rs `manifest` does `HistoryWalker::new(store, at, DEFAULT_MAX_SCAN).last_touches(&paths)?`; the caller at stack.rs:139 is `manifest(store, at, r)?`.
  evidence: src/inspect/loaded.rs:296-302 runs the same lookup with `.unwrap_or_default()` and the comment "A history read failure only drops the commit column." `StackPart.changed` is already an `Option`.
  evidence: the trigger is a corrupt or unreadable Git object while HEAD reads fine. All three seats rated Nit.
  fix-shape hint: map the lookup error to an empty map so `changed` reads `null`.
- [Nit] tests/inspect_live.rs:184 — roster_live_matches_the_graph_pane_and_lists_only_unbound_objects samples the commit count with no wait for the daemon to go idle, so reconciler commits still in flight after the seat is created can break the 'roster --live adds no commit' assertion, and the failure would blame roster --live. [lanes: test-causality-lifecycle]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: tests/inspect_live.rs:150-158 waits only for the foreman tab and the clone's pane binding; `commits = rig.commit_count()` is taken at line 184; the assertion is at line 221 after several CLI calls and a `workspace.create`.
  evidence: tests/inspect_sweep.rs:153-166 `settle()` exists for the same setup and is called before that file's R12 check (lines 337-338, 590-591). inspect_live has no equivalent.
  evidence: not reproduced; no seat established that a commit follows the pane binding. All three seats rated Nit.
  fix-shape hint: wait for the daemon to stop committing (a settle step) before sampling `commits`; keep the assertion.
- [Nit] CURRENT-DESIGN.md:96 — CURRENT-DESIGN.md is the repo's consolidated design contract, and the PR does not update it. Its list of daemon-free readers, its SessionStart hook description and its doctor checks are now stale: roster, log, sessions and instructions stack are missing, as are the `--stdin-payload` hook and `.graph-local/loaded/` record, `harness_hook` and the `ops --seat/--teamspace` filters. [lanes: hygiene-docs]
  verdict: confirmed (reproduce ✓ / refute REJECT-outvoted / ground ✓)
  evidence: `git diff --stat bfeb8f3...63a57b5 -- CURRENT-DESIGN.md DESIGN-NOTES.md` is empty. Line 96 lists `show`, `list`, `path`, `instructions show` and `seat --hook-prompt` as the daemon-free readers. A grep for roster, sessions, loaded and stdin-payload finds nothing relevant. README.md:272 links the file as "Current design and continuation".
  evidence: the refute seat rejected because no existing sentence becomes false, the README gained a 64-line Inspection section, and no repo rule requires a per-feature update. The reproduce seat conceded all three points and still rated the drift a Nit.
  fix-shape hint: add the new daemon-free readers, the loaded record and the `harness_hook` check to CURRENT-DESIGN.md, or point it at the README Inspection section.
- [Nit] src/cli/ops.rs:120 — `ops --json` passes the daemon's internal `OpSummary` value straight through as the schema-1 row shape. Its `requester` object (fields left out when unset, `human` omitted when false, an extra `native_session` field) differs from the `requester` object that `log --json` defines under the same schema 1 (all four keys always present, nulls included). [lanes: api-contract]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/cli/ops.rs `OpsView { ops: Vec<Value> }` is filled from `reply["ops"].as_array().cloned()`; rows are `plan::ops::OpSummary` (src/plan/ops.rs:42-51) embedding `model::change::Requester` (src/model/change.rs:85-96) with `skip_serializing_if` on teamspace, seat, clone, native_session and human.
  evidence: `inspect::log::RequesterView` (src/inspect/log.rs:157-162) always emits teamspace, seat, clone and human. README line 92 says "A value that can be absent is `null`"; `ops --json` does not follow that.
  evidence: the spec never defines the `ops` row shape (line 356 only says `ops` gains `--json`).
  demoted: the refute seat rated Should-fix; two seats rated Nit. The `--json` surface is new and unreleased with no existing consumers, so a contract-consistency gap is a Nit here.
  fix-shape hint: add an ops row view type (or reuse `RequesterView`) between the journal model and the JSON output.
- [Nit] src/inspect/roster.rs:588 — The "serde snake_case word of an enum" conversion is reimplemented again in roster.rs and sessions.rs, inlined twice in stack.rs, and log.rs uses `format!("{:?}").to_lowercase()` instead, so text renderings get enum words three different ways. [lanes: simplicity-design]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: src/inspect/roster.rs:588 and src/inspect/sessions.rs:59 each define `word`; src/inspect/stack.rs:336 and :352 inline `serde_json::to_value`; base already has `journal::name_of` (src/journal/mod.rs:97).
  evidence: src/inspect/log.rs:267-268 `kind_word` is `format!("{:?}", id.kind()).to_lowercase()`. `IdKind` (src/model/ids.rs:26-32) has the variant `NativeSession`, so it yields "nativesession" where serde yields "native_session". `kind_word` feeds the log row `kind` field (log.rs:206, :218). log.rs:325 does the same for `ConfirmMode`, which is single-word today.
  severity: seats rated Nit / Should-fix / Nit. The only live effect is the spelling of one id kind in log output.
  fix-shape hint: one shared helper (or `as_str` on the enums) used by all inspect renderers, and replace the Debug-lowercase calls in log.rs.

## Not verified (beyond panel cap)   ← severe candidates the panel cap left unverified — listed, never dropped
- none

## Not verified (dedupe failed or judge lost)   ← findings a pipeline failure left unjudged — listed, never dropped
- none

## Beyond remainder cap (count only)   ← low-severity candidates the dedupe remainder cap dropped; the count survives, the claims do not
- 14 candidates dropped by the remainder cap — raise config.remainderCap and re-run to see them

## Rejected (with reason)        ← so re-roasts don't re-litigate
- src/bootstrap/setup_claude.rs:211; tests/inspect_loaded.rs:487 — The Claude SessionStart hook command starts with an unbounded `p=$(cat)` before the binary runs, so the 2 s STDIN_WAIT guard never protects it.
  reason: rejected 3/3. The Claude Code hooks reference (https://code.claude.com/docs/en/hooks) says "Command hooks receive JSON data via stdin" and its own examples read the payload with `input=$(cat)`, so EOF after the payload is the documented usage. No seat found a harness that holds hook stdin open. The seats note the page implies EOF and does not state it. What remains is a missing held-open test of the shell form.
- src/cli/ops.rs:141; src/cli/ops.rs:201 — `ops.list` gained a required `head` reply field and new `seat`/`teamspace` arguments with no IPC_VERSION bump.
  reason: rejected 2/1. README.md:163 tells operators to restart the graph daemon after a rebuild. src/ipc.rs:45 records the precedent "Added without an IPC_VERSION bump". The failure is loud ("daemon reply has no head"). The silent filter-ignore case needs a future relaxation of the head check. The dissenting ground seat (Nit) cites the same ipc.rs:45 convention.
- src/daemon/roster_live.rs:48; src/store/history.rs:128 — `roster.live` runs its history walks on the daemon's shared GitStore and contends with the writer on the repo Mutex.
  reason: rejected 2/1. The spec sets no contention or latency requirement for `roster --live`. `commits()` returns ids only and `walk()`/`last_touches()` re-take the lock per commit, so the writer can interleave. Hold time is unmeasured. The dissenting ground seat (Nit) cites the spec's HistoryWalker lifecycle line "It is not shared"; the reproduce seat read that line as a safety claim about immutable objects, not a contention promise. The O(history) revwalk itself is confirmed separately at src/store/history.rs:130.
- src/daemon/roster_live.rs:47 — The `roster.live` handler runs its build in spawn_blocking with no deadline, cancellation or concurrency limit.
  reason: rejected 3/3. The same uncancelled spawn_blocking shape is the daemon's convention (plan/commands.rs:345 and :371, undo/commands.rs:87, bootstrap/mod.rs:58). The work is a bounded read-only walk that ends on its own. The design states no deadline or concurrency requirement for this handler.
- src/inspect/scope.rs:228 — Teamspace scope membership leaves out the transcripts of its own seats, so seat scope is not a subset of teamspace scope.
  reason: rejected 3/3. Spec §2.4 defines teamspace as "the teamspace folder, including its seats" and attaches transcripts only to seat. The test `seat_transcripts_included` asserts "Teamspace scope does not take transcripts". The spec states no subset rule.
- src/inspect/log.rs:53 — `log --by <seat>` silently drops commits whose OperationRecord failed to parse, with no malformed warning.
  reason: rejected 3/3. Design §2.4 line 342: "Commits with no requester ... never match `--by`". A commit with an unparseable record has no readable requester. The malformed warning is specified for listed commits.
- src/daemon/roster_live.rs:64 — roster.live spends a fixed 10 s Herdr timeout on the snapshot after an unbounded build, so slow plus slow exceeds the client's CALL_TIMEOUT.
  reason: rejected 3/3. The overrun needs a build longer than about 17 s, which the finding does not show. A slow Herdr alone stays under the 27 s server budget and yields the promised `live_unavailable.herdr` reply. The design gives no end-to-end wall-clock guarantee.
- src/plan/ops.rs:479 — The `ops.list` handler now calls `store.head()` synchronously on a tokio worker without spawn_blocking.
  reason: rejected 2/1. On base, `ops.get` already calls `op_detail(&d.journal, d.store.as_ref(), ...)` inline (bfeb8f3:src/plan/ops.rs:473) and compose.rs:574 calls `store.head()` synchronously. The call is a single ref lookup. Reporting the head in the envelope is the intended new contract. The dissenting ground seat (Nit) calls the pattern an existing convention.
- src/cli/roster.rs:73 — `roster --live` calls a new daemon command without a protocol bump, so an old daemon gives exit 1 "unknown command", not exit 5.
  reason: rejected 3/3. The help text at src/cli/roster.rs:26 promises exit 5 "when it is down"; a running pre-upgrade daemon is not down. The unit test `other_remote_codes_are_exit_1` pins UnknownCommand to exit 1. src/ipc.rs:45 shows commands are added without a bump.
- src/daemon/doctor.rs:268; src/daemon/doctor.rs:269; src/daemon/doctor.rs:266 — The `harness_hook codex` doctor check emits a permanent WARN for every current Codex hook.
  reason: rejected 3/3. Design line 287: "`doctor` reports an untrusted-after-change hook as a warning it cannot fix (graph never writes trust)". Graph cannot read Codex trust, so it cannot tell trusted from untrusted. WARN does not fail doctor.
- tests/inspect_sweep.rs:597 — every_component_reports_the_one_pinned_head cannot force a writer commit to land inside a single view invocation.
  reason: rejected 2/1. src/inspect/inventory.rs:496 `pinned_commit_not_torn` pins a head, commits again, and asserts the load at the pinned commit is not torn, which is the deterministic proof. The dissenting reproduce seat (Nit) reported finding no such seam and did not address that unit test.
- tests/inspect_sweep.rs:154 — `Rig::settle` uses a fixed 1.5 s 'HEAD unchanged' window instead of the daemon's own idle signal.
  reason: rejected 3/3. The cited alternative, `settle_after_step` in tests/daemon_composed.rs:1875-1905, also relies on a quiet window (8 polls x 50 ms). The late-commit case is not reproduced and names no concrete daemon action. The cost is about 9 s across 4 opt-in tier-3 tests.
- src/inspect/loaded.rs:156 — A failed loaded-record write is discarded with no stderr line, doctor check or record field.
  reason: rejected 3/3. Design §2.3: the record is "written atomically, best-effort, and never fails the caller", and "A session that ignored the hook, or whose hook failed, shows the last successful injection for that session, or `unknown` if there was none."
- src/inspect/roster.rs:94 — The roster's `last_change` can come from a commit with an unreadable record, but `LastChange` has no `malformed` field and the roster prints no warning.
  reason: rejected 2/1. The spec fixes the shape as `last_change:{"at","op","summary"}|null` (line 216) and the roster text shows no malformed state. `at` and the trailer `op` stay correct. The dissenting ground seat (Nit) agrees it is "not a contract break".

## Unverified nits (spot-checked)
- [Nit] src/inspect/roster.rs:598 — Roster and stack text byte-slice the unvalidated `loaded_rev` at index 12 with no char-boundary check, so a loaded record whose `rev` has a multibyte character before byte 12 panics the reader. spot: CONFIRM (also roster.rs:605 and stack.rs:235; `short_rev` already uses `get(..12)`).
- [FYI] src/cli/setup.rs:63 — A first-time `setup codex` note says the hook command 'changed' and must be 're-trusted'. spot: REJECT (wording only; the notice fires when design.md:287 says it should).
- [FYI] src/store/history.rs:218 — `--since` compares a whole-second commit time against a sub-second `since` value. spot: REJECT (Git commit times are whole seconds; the design specifies no sub-second precision).
- [Nit] src/inspect/loaded.rs:554 — `stale_modified_added_removed` comments "different delivery mode, is modified" but compares an identical render; no test exercises `|| p.delivered != h.delivered`. spot: CONFIRM.
- [FYI] tests/inspect_loaded.rs:171 — The default-tier loaded-record tests write the occupant straight to the graph and never run the session-report → daemon → occupant step. spot: REJECT (`r14_chain_through_the_setup_hook` runs the real chain in the private-herdr tier; the pieces are unit-tested separately).
- [FYI] tests/inspect_loaded.rs:330 — `codex_hook_records_the_payload_session` asserts `current` against a hand-written Claude-harness occupant. spot: REJECT (design §2.3 accepts the Codex id-form uncertainty with the `unknown` fallback and `CODEX_FORM_NOTE`; the real Codex id form is an external fact no seat verified).
- [FYI] src/inspect/log.rs:309 — The new text renderers write commit subjects and op summaries to the terminal without removing control characters. spot: CONFIRM (seat severity Nit). pre-existing on base: src/cli/ops.rs:72 prints `o["summary"]` unfiltered; the new views widen the surface to arbitrary commit subjects.
- [FYI] src/cli/seat.rs:225 — The SessionStart hook persists `session_id` from its stdin payload unvalidated, and stack and roster print it. spot: REJECT (the payload comes from the user's own harness; no trust boundary is crossed).
- [FYI] src/plan/ops.rs:479 — `ops.list` reads head and the journal with no common snapshot. spot: REJECT (the journal is live SQLite state with no commit pin; the spec does not require the envelope head to match it).
- [FYI] src/bootstrap/setup_claude.rs:217 — `harness_hook` marks every pre-change hook outdated, and rerunning setup drops Codex trust. spot: REJECT (design lines 287 and 572 state the one-time re-trust as a deliberate exception).
- [FYI] src/cli/ops.rs:31 — `ops` truncates at `--limit 200` with no sign that it did. spot: REJECT (the spec gives `ops` a plain limit; base already limited to 200).
- [Nit] src/inspect/roster.rs:284 — The roster drops a manager-chain integrity error (`effective_manager(...).ok().flatten()`), so the seat shows no manager and no error field. spot: CONFIRM.
- [FYI] src/cli/ops.rs:145 — `ops` treats a reply without an `ops` array as an empty list. spot: REJECT (`list_json` always emits an array; a missing `head` still fails).
- [FYI] src/inspect/loaded.rs:179 — `loaded::read` folds every I/O error into the same `unknown` as corrupt JSON. spot: REJECT (design line 511: "A missing, corrupt or older-schema record reads as `Unknown`").
- [FYI] src/cli/seat.rs:221 — A hook that records `native_session_id: null` records no reason. spot: REJECT (design §2.3 maps every such cause to the same `unknown` outcome).
- [FYI] src/store/history.rs:155 — `HistoryWalker::walk` runs `change_at` (diff plus record parse) for every scanned commit. spot: REJECT (design line 354 accepts "One commit read and one tree diff per scanned commit").
- [FYI] src/cli/seat.rs:287 — The loaded record is last-writer-wins and its `at` is taken after the stdin wait. spot: REJECT (design.md:237 defines the record as the last injection; a mismatch degrades to `unknown`).
- [FYI] tests/inspect_sweep.rs:342 — The R12 checks for daemon-backed views compare repo facts with no observation window. spot: REJECT (the handler does not write; `one_call_takes_one_snapshot_and_adds_no_commit` pins that).
- [Nit] src/inspect/harness_files.rs:329; src/inspect/harness_files.rs:217 — `claude_agents_md_fallback` and `codex_bounded_by_git_root` fail if any ancestor of the tempdir holds a CLAUDE.md, because `discover_claude` walks every ancestor. spot: CONFIRM (no collision exists today).
- [FYI] tests/inspect_loaded.rs:42 — The new tests run bare `Command::new("git")` with the ambient environment. spot: REJECT (base tests do the same at tests/integration_sweep.rs:2344; no hook runs `cargo test`).
- [Nit] src/cli/instructions.rs:115 — The in-process test `stack_is_read_only` reaches `UserDirs::from_env()` and stats the developer's real `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md`. spot: CONFIRM (metadata-only; the assertions do not depend on it).
- [Nit] src/inspect/stack.rs:235 — The short-revision rule is reimplemented inline in roster.rs and stack.rs although `bootstrap::instructions::short_rev` exists. spot: CONFIRM.
- [Nit] src/inspect/stack.rs:215 — The "omitted parts are a render-order prefix, cut part is last" rule is recomputed in both `loaded::delivered` and `stack::manifest`. spot: CONFIRM.
- [Nit] src/bootstrap/instructions.rs:505 — `fit_hook` re-formats the block header to compute `cut_body_bytes`, so the header format lives in two places. spot: CONFIRM.
- [Nit] src/bootstrap/setup_claude.rs:168 — Hook currency is decided by substring-matching hand-written copies of the command shape. spot: CONFIRM (no test ties the recognizer to the generator).
- [FYI] src/inspect/output.rs:129 — `InstructionsView.note` is free English prose with no machine code. spot: REJECT (design line 261 specifies the note as a literal string; consumers act on `InstructionsStatus`).
- [Nit] src/inspect/harness_files.rs:387 — `content_never_opened` shows only that a mode-000 file is still listed, not that content is never read; under root the chmod has no effect. spot: CONFIRM.
- [Nit] src/cli/ops.rs:174 — Resolving one name to an id goes through a fabricated `[word, name]` ScopeArg plus `unreachable!` arms at several call sites. spot: CONFIRM (the seat counted six `unreachable!` arms, not five).
- [FYI] src/cli/log.rs:115 — `log` text mode sends skipped-record warnings to stderr outside `emit`. spot: REJECT (design lines 168 and 519 specify a stderr warning).
- [FYI] src/inspect/mod.rs:31 — `inspect::pin_head` is a one-line pass-through that enforces nothing. spot: REJECT (spec §2.1 asks for one resolve per command, not type-level enforcement).
- [FYI] src/inspect/roster.rs:368 — `roster::build_inner` assembles the roster, collects bindings and reloads the Inventory for `--unmanaged`. spot: REJECT (the extra load runs only on the opt-in path; one shared builder is the stated design).
- [FYI] src/daemon/doctor.rs:254 — The `harness_hook` doctor check is not named in the README. spot: REJECT (README lines 125-126 document the same remedy).
- [Nit] src/inspect/roster.rs:1 — New module docs cite 'spec §2.x' for the inspection design while existing code uses the same numbers for the MVP design, with no path to the document. spot: CONFIRM.
- [Nit] src/cli/roster.rs:2 — Module docs still say behaviour 'is implemented by' or 'filled by' bead ids that have landed (also src/inspect/output.rs:108, :128 and src/inspect/roster.rs:109). spot: CONFIRM.
- [FYI] tests/inspect_sweep.rs:42 — `r14_chain_through_the_setup_hook` copies the hard-coded `/usr/bin/sleep` as its fake `claude`. spot: REJECT (AGENTS.md limits tests to the Fedora hg-build toolbox, where it works; the seat did not verify the macOS and multi-call-sleep claims).
- [Nit] tests/inspect_sweep.rs:780 — The `rig.settle()` after the second `set_instruction` pays at least 1.5 s although the following assertions read committed state only. spot: CONFIRM.
- [FYI] tests/inspect_sweep.rs:336 — Three sweep tests each build an identical rig. spot: REJECT (the file documents "One rig per test"; merging would hide the pinned-head result behind a read-only failure).
- [FYI] tests/inspect_loaded.rs:496 — `held_open_stdin_still_prints_and_exits` asserts an 8 s wall-clock bound. spot: REJECT (a coarse hang detector at 4x the 2 s wait; tighter bounds already exist at tests/daemon_ipc.rs:474).
- [Nit] src/inspect/loaded.rs:787 — `write_failure_swallowed` asserts that a path nothing writes to does not exist. spot: CONFIRM (test noise; the real property is covered in tests/inspect_loaded.rs).
- [FYI] src/inspect/stack.rs:43; src/inspect/stack.rs:170 — `instructions stack --json` carries a `rev` that always equals the envelope `head`. spot: REJECT (the text rendering needs the revision in the view; `seat --json` already has the same shape).
- [FYI] src/cli/instructions.rs:28 — The inspection CLI names the same concepts inconsistently (`--by` vs `--seat`, positional forms, default limits). spot: REJECT (design lines 342 and 356 mandate the names and give the reason).
- [Nit] src/daemon/doctor.rs:267 — Codex-specific behaviour is chosen by `name == "codex"` in doctor and setup instead of a `Target` property. spot: CONFIRM.
- [FYI] src/inspect/harness_files.rs:41 — `HarnessName` duplicates `model::harness::Harness` minus Shell. spot: REJECT (the narrower type keeps Shell out of the JSON contract; the mapping is one exhaustive match).
- [Nit] src/inspect/output.rs:118 — `InstructionsView.source` is an `Option<String>` kept in sync with `InjectionSource` by a hand-written `word()`. spot: CONFIRM.
- [FYI] src/inspect/sessions.rs:126 — `ObjRef.lifecycle` is a `String` compared against "retired". spot: REJECT (it is the JSON-facing word; the test at sessions.rs:274 would catch a rename).
- [Nit] src/inspect/sessions.rs:67 — `SessionsQuery.scope` is never read by `sessions::build`. spot: CONFIRM.
- [FYI] src/inspect/roster.rs:545 — `LogQuery` mixes the match predicate with walk-run options, so the roster builds one with filler `limit: 1`. spot: REJECT (one shared predicate is the documented intent of `LogQuery`).
- [Nit] src/inspect/roster.rs:595 — The shared `instructions_text` renderer lives in the roster module and stack imports it from there. spot: CONFIRM.

## Escalations (need human)      ← UNVERIFIED externals, incomplete panels, unsettled panels, material dissent
- none
