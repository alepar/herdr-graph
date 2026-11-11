# Final seam review: epic hg-blw, integration branch super-auto/graph-state-inspection

Invocation inv-64d86974-dab5-4fe4-a772-e304978dabd1. Captured source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298, both verified with `git rev-parse --verify <sha>^{commit}`. bind-review-source exited 0 before this file was written, and the file did not exist before then. Caller checkout observed at HEAD b9caa7f with an empty `git status --short` at start and before writing; it was not switched, restored or edited. This review is static: reads used `git show b9caa7f:<path>`, no worktree was made, no build or test was run, no rig was started.

## Inputs

- Cluster reviews 1-12 of this invocation (bootstrap, cli, daemon, herdr, inspect, lib, plan, readme, store, tests, writer, docs-reviews).
- clusters.json of the assembled bundle; cross-cluster hunks traced at head: src/daemon/roster_live.rs and src/daemon/compose.rs LazyMap::inspect (daemon to inspect overlay), src/bootstrap/setup_claude.rs hook commands against src/cli/seat.rs `--stdin-payload` and src/cli/session.rs (base) stdin read, src/cli/log.rs warning output against spec 2.1.
- Spec: docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md at head, which has a `## Requirement baseline` (B1-B8, human scope change S1 affecting B8); `bd show hg-blw --json` (epic open, all children closed).
- Cluster 13 (docs-top: the design doc, run.md, ledgers, roast and coverage records) has no review file; it is over the window and is listed as unreviewed below.

## Requirement baseline trace, whole tree

- B1 (which seats are active): `roster [<scope>] [--all] [--json]` runs cli::roster, pin_head, Scope::resolve, inspect::roster::build over Inventory::load. Delivered (clusters 2, 5, 10 traced and ran it).
- B2 (files loaded into context per seat): production chain at head is `setup claude|codex` writing the owner-marked hook (bootstrap) -> harness runs `p=$(cat); printf '%s' "$p" | <bin> seat --hook-prompt --stdin-payload; printf '%s' "$p" | <bin> session-report --from-hook claude` (Claude) or `<bin> seat --hook-prompt --codex --stdin-payload` (Codex) -> cli::seat::hook_prompt prints and flushes `fit_hook`, then for a bound caller reads stdin (64 KiB, 2 s) and calls inspect::loaded::record with per-part digest/bytes and `cut_body_bytes` from bootstrap -> roster `instructions` and `instructions stack` read the record and compare against `fit_hook` at the pinned HEAD. The interface between bootstrap's `HookFit.cut_body_bytes`/`Part.digest` and loaded::delivered agrees (both clusters checked prefix order and char boundary). Cluster 10 ran tests/inspect_sweep.rs `r14_chain_through_the_setup_hook`, which reads the hook command back from the settings `setup claude` wrote and runs it through `sh -c` against the real daemon, reaching `current`, then `stale`, then `current`. Delivered for Claude. For Codex the record is written, but the form-differs note cannot fire (cluster 5 item, carried).
- B3 (activity: global, space, seat): `log [<scope>] ... [--by] [--all]` over HistoryWalker (store) and LogQuery (inspect); journal side through `ops --seat/--teamspace` -> daemon `ops.list` with OpsFilter (plan) -> envelope with the daemon's `head` (cli). The plan/cli interface agrees on `{seat?, teamspace?, unresolved, limit}` and the `head` key. Delivered.
- B4 (sessions by space/seat/clone, newest first): `sessions [<scope> | clone <c>]` -> Inventory::load -> sessions::build, sorted by `started` descending. Delivered; binary-level row ordering is not asserted (see untested items).
- B5 (staleness shown, not notified): loaded::status is display-only; no cluster adds a notice or a Git write. Delivered.
- B6 (log hides bookkeeping by default): `--all` false by default into LogQuery.all; roster last_change passes `all: false`. Delivered.
- B7 (harness files: presence, size, mtime; never read): harness_files::discover uses metadata and read_dir only. Delivered; the user-level origin/env label is wrong in the default layout (cluster 5 item, carried), which does not touch presence, size or mtime.
- B8 (`roster --live` in scope) with S1: cli::roster::live -> daemon `roster.live` registered in the production composition -> one Herdr snapshot, incarnation-aware binding match, `--unmanaged` ids and labels only. Delivered on the happy path and for a Herdr snapshot failure. Under the threads-service-down condition that spec 2.2 names, the reply claims a complete threads read (see next section). This is a defect inside a delivered row, carried as a Must fix item.

## Cross-cluster composition

- Threads-degraded overlay (daemon, inspect, tests, readme compose badly). Confirmed at head: src/daemon/compose.rs:1113-1119 updates `seat_view_error` only for Rejected/Unsupported and leaves it untouched on `Err(_)`; src/daemon/roster_live.rs:66-82 maps a None slot to `ThreadsRead::Ok` with the last remembered refusals and lost. The inspect overlay renders that faithfully, the tier-3 tests in tests/inspect_live.rs and tests/inspect_sweep.rs (rigs with no herdr-threads service) assert `live_unavailable == null`, and the README states a different degraded contract again (clone `live` nulled for threads-only). Three clusters each describe a different threads-down behavior and the spec describes a fourth. The fix has to land across daemon, tests and README together, or the corrected daemon will fail the tier-3 tests that currently certify the defect.
- Hook stdin, bootstrap against cli and the Unchanged-behavior list. The base hook command was `<bin> seat --hook-prompt; <bin> session-report --from-hook claude`, and base src/cli/session.rs:50 already did an unbounded `read_to_string` on stdin inside the same hook process. So a held-open stdin already kept the base hook from completing until the 10 s timeout; the new `p=$(cat)` moves the wait before the prompt prints but does not introduce a new way to block, and the command text is exactly the one spec 2.3 prescribes. My assessment is that this item from the archived inv-b9c53a9d seam review is not a regression; it is carried forward below because cluster 12 carries it, with the narrower disposition of recording that rationale in the design (or bounding the read) rather than treating it as a code defect.
- Text-mode notices: spec 2.1 says notices without a JSON field are part of the text rendering; src/cli/log.rs:114-117 writes HistoryWalker skipped-record warnings to stderr in both modes, and the README follows the spec. Visible on a terminal either way; recorded under Deferred OK.
- Daemon `roster.live` reply deserialized by cli::roster::live into the same `Roster` type with `present_or_null`: the shapes agree, and the tier-3 tests run that round trip against a real daemon.
- doctor's `harness_hook` check consumes bootstrap's `owned_hook_current` predicates, which require the `p=$(cat); ` prefix and the `--stdin-payload` tokens that bootstrap emits; consistent.
- Prior invocations (inv-4b650ab2, inv-b9c53a9d) ended not ready on the same open items. `git log 2dfa656..b9caa7f` touches none of the defect files; the items have no bead, `parked:` entry or recorded disposition (cluster 12).

## Verdict
not ready (roster --live reports threads read with the service down and tier-3 tests lock that in; Codex form-differs note never fires; user-level CLAUDE.md mislabelled ancestor; README misstates tier-3 tests and the threads-degraded contract; cluster 13 unreviewed)

## Must fix before landing
- src/daemon/roster_live.rs:64-82 with src/daemon/compose.rs LazyMap::inspect (1113-1119), baseline B8 under spec 2.2: when the threads seat read fails for any reason other than Rejected/Unsupported (Disconnected from Endpoint::locate, transport, discovery) or has not yet completed, roster.live must return seat_refusals/seat_lost null and live_unavailable.threads naming the reason; record the last seat-read outcome in a slot the handler reads, and add a test that drives a Disconnected seat read instead of writing seat_view_error by hand (cluster 3).
- tests/inspect_live.rs and tests/inspect_sweep.rs `spec_keys_and_flags_reach_their_views` assert `live_unavailable == null` and an object-valued `threads_live` in a rig with no herdr-threads service, locking in the B8 threads-degraded violation; assert the degraded form there or start a real threads service (support::threads_bin, as tests/e2e_private_herdr.rs does) before asserting null, landing together with the daemon fix (cluster 10).
- src/inspect/harness_files.rs:217-248 (spec 2.3 `origin` and `env: inspector`, B7 labelling): with cwd under $HOME and CLAUDE_CONFIG_DIR unset, the ancestor walk inserts $HOME/.claude/CLAUDE.md as origin ancestor first and the first-wins `Acc::add` keeps it, so it prints `origin=Ancestor env=None`; run the user-level adds first or let them override origin and env, and add a test with the cwd under the user dir's parent (cluster 5).
- src/inspect/loaded.rs:229-236 (spec 2.3 Codex `note: "codex session id form differs"`, B2/B5 for Codex): the production occupant id is the Herdr rollout stem `rollout-<ts>-<uuid>` while the payload carries the bare uuid, so the note never fires and the clone reads a silent `unknown`; detect that form difference for the note only, without treating the ids as matching, and replace the full-path occupant in `codex_form_differs_note` with a rollout-stem occupant (cluster 5).
- README.md:231 says inspection adds "one tier-3 test, `tests/inspect_live.rs`"; list tests/inspect_sweep.rs (private-herdr tier-3 sweep added in 1163f89 after README commit 111b9ae) with its run command (cluster 8).
- README roster `--live` degraded-state bullet claims every clone's `live` is null when the threads view is unavailable; per inspect::roster::overlay and spec 2.2 only a Herdr snapshot failure nulls clone `live`, while threads unavailability nulls threads_live.seat_refusals/seat_lost and sets live_unavailable.threads; correct the JSON contract text (cluster 8).
- The four Must fix items of the archived inv-b9c53a9d seam review are open at b9caa7f with no fix, bead or recorded disposition (compose.rs/roster_live.rs threads state, loaded.rs Codex note, harness_files.rs user origin, setup_claude.rs:211 `p=$(cat)` against the design's never-blocks guarantee); fix or record a disposition for each before landing (cluster 12). Seam note on the last one: the base hook already blocked on held-open stdin through session-report's unbounded read_to_string and spec 2.3 prescribes this exact command, so recording that rationale in the design's Unchanged-behavior list is a sufficient disposition.
- Cluster 13 (docs-top) is unreviewed: over the window. Its 19 slices (the design doc, run.md, coverage and findings ledgers, roast records, friction.md) had no cluster review in this invocation.

## Untested scope
- No test covers roster.live while the threads service is unreachable (Disconnected) or before the first seat read completes; the only threads-degraded test sets seat_view_error directly.
- No test pairs a Codex occupant id in rollout-stem form with a bare-uuid payload id at unit or integration level.
- No test runs harness-file discovery with the cwd under the directory that holds the user-level Claude dir.
- The installed Claude hook command is never run with a held-open stdin; only the bare binary with `--stdin-payload` is.
- No binary-level test asserts populated `sessions` rows (newest-first order, timestamps, `clone <c>` rows, footer).
- No integration test drives a truncated injection end to end (`cut` part, `omitted` labels, a cap-boundary move turning the record stale).
- `ops --seat/--teamspace --json` against a live daemon checks only the `ops` key, not the requester filtering.
- No test covers an integrity StoreError during roster.live scope resolution, or roster.live with a scope, `all: true` or a loaded record at the handler level.
- No test covers roster or log when a historical commit's tree holds an unparseable record (spec 2.1 one-bad-object rule).
- `instructions stack --clone <name>` with a clone name shared by two seats is not tested.
- `codex_trust_note` in setup and the harness_hook_checks error arm (malformed settings) have no test.
- `fit_hook` with a cut part that delivers zero body bytes, and `last_touches` on a deleted path, are not tested.
- This seam review ran no build or tests; pass status at b9caa7f is taken from the cluster 2, 5, 9 and 10 runs in review-owned worktrees.

## Deferred OK
- src/cli/log.rs:114-117 writes skipped-record warnings to stderr in text mode, while spec 2.1 and the README put text-mode notices in the rendering; align one side.
- src/cli/seat.rs `seat --json` help line claims a schema-1 document, but the command prints the raw `seat.resolve` reply; reword it.
- src/daemon/roster_live.rs:50-52: route a store failure in scope resolution through CommandError::from_store so an integrity failure exits 6 under --live.
- src/daemon/roster_live.rs:36-41: non-string scope words are dropped silently.
- src/daemon/doctor.rs:267-270: the Codex re-trust warning is permanent once the current hook is installed; doctor.rs:262 reports a malformed settings file twice.
- src/cli/instructions.rs: resolve `--clone` among the named seat's clones instead of instance-wide; drop the early return plus `unreachable!` arm.
- src/cli/ops.rs: tolerate a missing `head` from a pre-upgrade daemon in text mode or bump the IPC version; unknown `--seat/--teamspace` with the daemon down exits 1 rather than 5.
- src/cli/roster.rs: parse `roster --live` scope words client-side so usage errors precede the daemon call.
- src/bootstrap/instructions.rs: derive the cut header length from `block()` and return `cut_body_bytes: None` when not even the cut header fits.
- src/inspect/roster.rs and src/inspect/log.rs: a historical tree whose records fail to parse aborts the whole roster or log seat|teamspace instead of degrading one row (cluster 5 concern); roster last_change cost near the 5000-commit cap is unmeasured.
- src/inspect: roster text omits the count of parts loaded and stack text omits delivered labels (JSON carries both); stack `manifest()` fails on a last_touches error while roster swallows it; module header and output.rs comments still describe hg-blw.12 as pending.
- src/store/history.rs: `last_touches` could return its ScanReport so beyond-the-cap differs from never-touched.
- src/plan/ops.rs: `ops.list` reads head and scans the journal on the async executor and head is not a journal snapshot; IPC rejection of an unparseable `teamspace` is unasserted.
- src/writer/recovery.rs:51: fully qualified `parse_trailers` path instead of a `use`.
- tests: the sweep's `git status --porcelain --ignored` collapses `.graph-local/`; R14 test does not assert an empty occupant first; several reader and re-injection exit codes are unchecked; git spawned with the real HOME gitconfig; inspect_live commit-count race; rustfmt churn in tests/daemon_composed.rs.
- README: degraded first line written as `live data unavailable: <reason>` where code prints the `herdr:` source key; `ops --limit` default 200 not stated.
- Run records: inv-4b650ab2 cluster-8 repeats its terminal sections (fix run.md's double count, not the archive); archived reviews quote the host account name, redact before publishing outside the repo.
