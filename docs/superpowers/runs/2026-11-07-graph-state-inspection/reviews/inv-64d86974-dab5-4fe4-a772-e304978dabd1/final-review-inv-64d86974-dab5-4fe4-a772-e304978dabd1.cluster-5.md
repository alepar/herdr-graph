# Final review: cluster 5 (inspect), epic hg-blw

## Source and method

- Captured source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298. I verified both with `git rev-parse --verify <sha>^{commit}`. The bind-review-source command exited 0.
- Caller checkout: at HEAD b9caa7f with a clean status at start and at finish. Nothing was switched or restored.
- Inputs: the clusters.json entry `inspect` (src/inspect/{fixture,harness_files,inventory,loaded,log,mod,output,roster,scope,sessions,stack}.rs, slices 000065-000075), digest.md, `bd show hg-blw --json`, and the design doc docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md (Requirement baseline, sections 2.1-2.6).
- Other files, read only to check named risks: src/cli/{seat,roster,log,sessions,instructions}.rs (entry points and the hook writer), src/bootstrap/instructions.rs (`fit_hook`), src/observe/sessions.rs and src/model/harness.rs (where the Codex occupant id comes from), src/store/layout.rs (`locate`), src/model/seat.rs (serde defaults), and the cluster-5 reviews from invocations inv-4b650ab2 and inv-b9c53a9d.
- `git diff --stat 2dfa656..b9caa7f -- src/inspect` is empty. This cluster has not changed since the inv-b9c53a9d review. The commits since then are docs (hg-blw.10) and the tier-3 sweep tests (hg-blw.11).
- Dynamic check: I made a review-owned detached worktree at b9caa7f, seeded its build dir by reflink, and added two probe tests in that worktree only. Then I ran `toolbox run -c hg-build cargo test --lib inspect:: -- --nocapture`. Result: 91 passed, 0 failed (89 existing tests plus the 2 probes). The leak check printed 0. I removed the worktree afterwards.

## Requirement baseline trace

The spec has a `## Requirement baseline` section with rows B1-B8. It has one human scope change, S1, which affects B8 only.

- B1 "which seats are active": delivered.
  - Path: `herdr-graph roster`, then src/cli/roster.rs, then `inspect::roster::build` (roster.rs:236), then `Inventory::load`.
  - Each clone shows lifecycle, availability, pane, occupant (with native id and transcript) and threads state. Retired seats and clones are filtered out unless `--all` is given.
- B2 "which files are loaded into context per each - rules, roles, etc": delivered for this cluster's part.
  - roster.rs:321-330 and stack.rs:151-164 fill each clone's `instructions` from `loaded::clone_view`. The view carries the record's `parts` and `omitted` labels.
  - `instructions stack <seat>` (src/cli/instructions.rs:110 calls `stack::build`) lists the HEAD manifest with path, bytes, digest and `path last touched`, plus the hook-cap kept/omitted/cut split.
  - The record's only producer is `seat --hook-prompt [--codex] --stdin-payload` in src/cli/seat.rs (`hook_prompt` calls `loaded::record`). The hook command that setup installs is the seam reviewer's to trace.
- B3 "state activity, latest modifications: global, space, seat": delivered.
  - `herdr-graph log [global|teamspace t|seat s]` runs src/cli/log.rs, then `LogQuery::run`, then `HistoryWalker`.
  - Scope membership is decided at each commit's own tree (`Scope::paths_at`). If that misses, the action's affected and retired objects are checked, falling back to the parent tree.
  - The roster's per-seat `last_change` (`fill_last_changes`) uses the same `LogQuery::matches`.
  - `ops --seat/--teamspace` is outside this cluster.
- B4 "list all sessions, by space/seat/clone, with timestamps and most recent first": delivered.
  - `herdr-graph sessions [scope|clone c]` runs src/cli/sessions.rs, then `Inventory::load`, then `sessions::build`.
  - Rows are sorted by `started` descending, with ties broken by ns id. Retired and archived objects are included. A missing transcript path falls back to the transcript record. Purge tombstones are counted, and the view has a `total`/`--limit` footer.
- B5 "Staleness is shown, not notified": delivered for Claude, partly degraded for Codex.
  - `loaded::status` (loaded.rs:196-279) assigns `current` or `stale` by comparing the record against `fit_hook` at HEAD. The roster and the stack show the result. Nothing notifies.
  - For a Codex clone whose occupant id came from a Herdr rollout path, the status is always `unknown`, and the spec's `codex session id form differs` note never appears (first Must fix item below). The spec allows `unknown` here but requires the note, so I do not count this as a missing baseline row.
- B6 "`log` hides `bookkeeping` by default": delivered. `LogQuery::matches` (log.rs:57-63) drops `Category::Bookkeeping` unless `all` is set, and the roster's `last_change` passes `all: false`.
- B7 "Harness-native files: presence, size and mtime … never reads their content": delivered.
  - `harness_files::discover` uses only `fs::metadata`, `symlink_metadata` and `read_dir`. It never opens a file.
  - `stack` carries `harness_files`, `harness_files_note` and `imports_followed: false`.
  - The `origin`/`env` labelling has a defect (second Must fix item below), but presence, size and mtime are correct.
- B8 "`roster --live` is in scope now", with S1: the overlay logic in this cluster is correct.
  - `live_pane_matches` requires `Incarnation::same_server` and the workspace/tab/pane nesting.
  - `binding_from_earlier_server` is typed.
  - If Herdr or threads is degraded, `live_unavailable` is filled and every clone's `live` is null.
  - `--unmanaged` lists ids and labels only, as S1 asks.
  - The daemon handler (src/daemon/roster_live.rs) is the seam reviewer's to trace.

## Code analysis

- Key abstractions hold:
  - Scope owns the grammar, the resolution and the membership at a given tree.
  - Inventory owns the join.
  - LoadedInstructions owns the record, the status rule and the swallowed write errors.
  - LogQuery owns matching. Roster and log both call `matches`.
  - The envelope is built only in output.rs.
  - HEAD is pinned once and passed as a `CommitId` everywhere I checked.
- `loaded::delivered` and `stack::manifest` both assume that `omitted` is a prefix in render order and that the cut part is the last part. I checked this against `fit_hook` (bootstrap/instructions.rs:451-518): in the cut branch, `omitted = labels[..last]` and `cut = labels[last]`. In the omit branch, `omitted = labels[..start]` and there is no cut. Both assumptions hold. The cut prefix digest uses `cut_body_bytes`, which excludes the block header and lands on a char boundary.
- The record path matches on both sides: the writer passes the instance root from `with_head_at`, and the reader passes `store.root()`. The tier-3 tests/inspect_loaded.rs exercise that round trip.
- Harness-file origin collision, confirmed by probe:
  - Setup: cwd `<home>/proj` (a git root), `UserDirs.claude = <home>/.claude`, and `<home>/.claude/CLAUDE.md` present.
  - Result: `origin=Ancestor env=None`.
  - Cause: `discover_claude` walks every ancestor and inserts `<dir>/.claude/CLAUDE.md` (harness_files.rs:228) before the user-level add (harness_files.rs:242-248). `Acc::add` uses `or_insert`, so the first origin wins and `env` stays None.
  - Impact: this is the default layout (cwd under $HOME, `CLAUDE_CONFIG_DIR` unset). The user's global CLAUDE.md is therefore reported as an ancestor project file without `env: inspector`, which contradicts spec 2.3. It also sorts among ancestor files instead of user files.
- Codex form note, confirmed by probe:
  - Setup: a `hook-codex` record with payload id `0199a213-…` (a bare uuid), and an occupant id `rollout-2026-11-08T10-29-59-0199a213-…`. src/observe/sessions.rs:38-45 stems a Herdr `AgentSession::Path`, so the occupant id has this form when Herdr reports a Codex rollout file.
  - Result: `status=Unknown note=None`.
  - Cause: `stem(loaded) == stem(occupant)` (loaded.rs:229-235) cannot hold for this form difference.
  - The test that should cover it (`codex_form_differs_note`, loaded.rs:709) uses a full-path occupant `/home/u/.codex/sessions/019a-bc.jsonl`, which production never stores because observe stems it first.
- History robustness (carried over, unchanged):
  - `LogQuery::matches`, through `Scope::paths_at` and `layout::locate`, parses every teamspace/seat/clone record of each historical tree it inspects.
  - If any historical tree has a record the current deserializer rejects, the error aborts the whole `roster` (roster.rs:568-581) and `log seat|teamspace` (log.rs:104-117). It does not just drop that seat's `last_change`.
  - Most record fields are `#[serde(default)]` (src/model/seat.rs), so this is unlikely today but not impossible.
- Small doc drift: the stack.rs module header (lines 4-6) still says the per-clone instructions are "`unknown` here", though `clone_view` is wired in. output.rs comments still say "hg-blw.12 fills it".

## Concerns

- src/inspect/roster.rs:553-581 and src/inspect/log.rs:96-117: a historical tree whose records fail to parse aborts the whole daemon-free `roster` and `log seat|teamspace`. Spec 2.1 says "one bad object never blanks a view"; degrading only the affected seat's `last_change` (or marking the row malformed) would keep that rule.
- src/inspect/scope.rs:209-236 with src/store/layout.rs:346-360: for each scanned commit and each pending seat, `paths_at` enumerates and parses all seat records of that tree. A dormant seat whose last change is old costs up to 5000 x (records) parses on every plain `roster` call. GitStore's `locate_cache` may soften this, but nobody has measured it.

## Verdict
not ready (user-level ~/.claude/CLAUDE.md is labelled origin ancestor without env: inspector in the default layout; the Codex "session id form differs" note never fires for Herdr's rollout-path occupant id)

## Must fix before landing
- src/inspect/harness_files.rs:217-248 (spec 2.3 `origin` and `env: inspector`): with cwd under $HOME and CLAUDE_CONFIG_DIR unset, the ancestor walk inserts $HOME/.claude/CLAUDE.md as origin ancestor first, and the user-level add then hits the existing entry; the review probe printed `origin=Ancestor env=None`. Fix: run the user-level adds before the ancestor walk, or let a user-level add override origin and env, and add a test with the cwd under the user dir's parent.
- src/inspect/loaded.rs:229-236 (spec 2.3 Codex `note: "codex session id form differs"`, hg-blw.12 acceptance): the production occupant id for a Herdr-reported Codex session is the rollout file stem `rollout-<ts>-<uuid>`, while the SessionStart payload carries the bare uuid, so `stem(loaded) == stem(occupant)` never holds; the review probe printed `status=Unknown note=None`. The clone silently reads unknown, which spec 2.1 forbids. Fix: detect this form difference for the note only (for example, the occupant id ends with `-<payload id>`), without treating the ids as matching, and replace the full-path occupant in `codex_form_differs_note` with a rollout-stem occupant.

## Untested scope
- No test pairs a Codex occupant id in the rollout-stem form (what observe/sessions.rs stores from a Herdr path) with a bare-uuid payload id; `codex_form_differs_note` uses a full path that production never stores.
- No test runs harness-file discovery with the cwd under the directory that holds the user-level Claude dir, which is the default layout and the one that triggers the origin/env collision.
- `instructions stack` has no test where `HistoryWalker::last_touches` fails; stack.rs:199 propagates that error and ends the whole command, while roster's `clone_view` swallows it.
- Roster and stack instruction status for retired seats and clones under `--all` (rendering against an archived seat) is not exercised.
- No test covers roster or log when a historical commit's tree holds an unparseable record.
- Nobody has measured roster `last_change` cost near the 5000-commit cap with many seats and one dormant seat.

## Deferred OK
- Roster text shows status, a truncated flag and a note, but not spec 2.2's "count of parts loaded"; the JSON `parts` array carries it.
- Stack text lists a clone record's omitted labels but not its delivered part labels; the JSON `instructions.parts` carries them.
- stack.rs `manifest()` fails the whole command on a `last_touches` error while roster's `clone_view` drops the column; inconsistent but rare.
- `loaded::clone_view` builds a fresh HistoryWalker for every stale clone; acceptable at current instance sizes.
- The stack.rs module header (lines 4-6) and the output.rs field comments still describe hg-blw.12 as pending; doc drift only.
- Earlier deferred minors for this cluster still apply unchanged: dangling-parent check over the whole instance, swallowed manager error, noisy error rows for retired seats, brittle source-scan test in sessions.rs, log `paths_at` cost, `log` text naming requesters by id, roster text unable to show `retired(reversible)`, an empty active teamspace omitted from the instance-scope roster, and `--since` relying on monotonic commit times.
