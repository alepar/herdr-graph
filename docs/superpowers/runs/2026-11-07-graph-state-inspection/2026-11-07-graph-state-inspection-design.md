# Graph state inspection — design

Date: 2026-11-07 · Status: approved by the user 2026-11-08 · Beads: epic hg-blw (phases hg-blw.1–.6), follow-ups hg-e58, hg-6nl, hg-la5, hg-87k · Scope: herdr-graph v0.3.0 · Code changes: none

The user's request:

> design for graph state inspection:
> - which seats are active, which files are loaded into context per each - rules, roles, etc
> - state activity, latest modifications: global, space, seat

## Goal

An operator or agent can inspect graph state read-only from the CLI, with `--json` on every view: which seats and clones are active and who occupies them (`roster`, `roster --live`), which instruction files each seat's agent was handed and whether they are stale (`instructions stack` plus the loaded record), every native session newest first (`sessions`), and recent changes scoped to global, a teamspace or a seat (`log`, `ops` filters).

## Requirement baseline

Source: the user's design request (2026-11-07, quoted above), the sessions request (2026-11-08, §2.5), the product decisions in §4 (user, 2026-11-07) and the implementation launch (2026-11-08, relayed by the main-tab coordinator).

### Human request

> design for graph state inspection:
> - which seats are active, which files are loaded into context per each - rules, roles, etc
> - state activity, latest modifications: global, space, seat
>
> ability to list all sessions, by space/seat/clone, with timestamps and most recent first
>
> hg-blw for impl, /superpowers:super-auto fully autonomouth both roasts are on

### Requirements

- B1 · "which seats are active" · source: design request 2026-11-07
- B2 · "which files are loaded into context per each - rules, roles, etc" · source: design request 2026-11-07
- B3 · "state activity, latest modifications: global, space, seat" · source: design request 2026-11-07
- B4 · "ability to list all sessions, by space/seat/clone, with timestamps and most recent first" · source: sessions request 2026-11-08
- B5 · "Staleness is shown, not notified." · source: §4 product decision 1
- B6 · "`log` hides `bookkeeping` by default." · source: §4 product decision 2
- B7 · "Harness-native files: presence, size and mtime. … It never reads their content." · source: §4 product decision 3
- B8 · "`roster --live` is in scope now" · source: §4 product decision 4

### Human scope changes

- S1 · 2026-11-08 · "not managed by graph - just enumerates them, no additional info" · source: user decision on the `roster --live --unmanaged` shape, relayed by the main-tab coordinator (herdr-threads message mnSGMfHi7SMe) · affects: B8

Every claim about today's behavior below is cited to the v0.3.0 source. A seat-bearing instance needs a private Herdr server, so no sample was captured from one. The samples are reconstructed from the format strings at the cited lines; identifiers in them are placeholders. The daemon-free outputs (`list templates`, `instructions show global`) were run against a scratch `init --with-examples` instance.

## 1. Today

### 1.1 Which seats are active

The facts exist, but they are spread over per-record TOML. No single command joins them.

| Fact | Where it lives | Updated by | How to see it today |
|---|---|---|---|
| Seat and teamspace lifecycle `dormant/active/retired` | `SeatRecord.lifecycle` (`src/model/seat.rs:19`, enum `src/model/common.rs:52-57`) | plans and the observed cascade | `list seats`, `show <seat>` |
| Clone lifecycle `active/retired` | `CloneRecord` (`common.rs:62-66`) | plans and the observer | `list clones` (no seat column, `src/bootstrap/show.rs:91-98`), `show <clone>` |
| Pane binding and availability `present/absent/unknown` | `Runtime{availability,bound,observed_at}` (`common.rs:79-84,116-140`) | observer, on change only (`src/observe/step.rs:321-342`) | `show <clone>`; `doctor` lists Unknown objects (`src/daemon/doctor.rs:182-220`) |
| Agent running in the pane | proxy only: `CloneRecord.occupant{native_session,harness,since}` (`common.rs:144-148`) plus `sessions[]` history (`src/model/native_session.rs:9-34`) | observer diff of `pane.agent` (`src/observe/diff.rs:100-150`); Claude `session-report` hook | `show <clone>`; `who <pane>` prints the ns id |
| Herdr agent status `idle/working/blocked/done` | Herdr snapshot `AgentInfo.status` (`src/ports/herdr.rs:10-30`) | — | **never stored or printed**. Read only for delivery gating (`src/reconcile/session.rs:193`) |
| Threads seat state | `threads_seat`, `seat_fact{seat,state,read_at}`, `seat_intent` (`src/model/clone.rs:37-137`) | SeatObservation, on change only (`src/observe/seat_observation.rs:76-89`) | `show <clone>`; `status` exception lists |
| Threads presence (busy/idle, last activity) | herdr-threads | — | not reachable through graph's threads port (`src/ports/threads.rs:94-230`). Use `herdr-threads seat list` |

What each command gives:

- `list [kind]` is daemon-free and reads the committed head. It prints one row per object, `id  name  lifecycle  path` (`src/bootstrap/show.rs:31-41`). The only state column is lifecycle.
  ```text
  st_01…  foreman   active   /…/teamspaces/proj/seats/foreman
  ```
- `show <id|name>` is daemon-free. It prints the record TOML verbatim and appends a `manager:` line for a seat (`show.rs:142-170`). It has every field, but only for one object at a time.
- `status` needs the daemon and never starts one. It prints raw JSON per component (`src/cli/status.rs:59-108`). The `clones` component holds exception lists only: `retired_unpurged`, `key_conflicts`, `stuck_restores` and `resume_blocked` (`src/daemon/clone_status.rs:13-153`). Healthy clones are not listed.
  ```text
  instance: /…/graph
  daemon: running (pid 4242, up 3600s)
  herdr socket: /…/herdr.sock
  clones: {"retired_unpurged":[],"key_conflicts":[],"stuck_restores":[],"resume_blocked":[]}
  ```
- `who <target>` needs the daemon (exit 5 without one), even though the handler only reads HEAD (`src/threads/mod.rs:366-385`). It prints `ts/seat/clone/ns` plus the manager chain. It shows no lifecycle or availability, and it matches threads seats through invitation links only, never `clone.threads_seat` (`mod.rs:324-328`).
- `seat [--json]` covers only the caller's pane. It takes a live Herdr snapshot of that one pane (`src/bootstrap/mod.rs:54-73`).

**Gaps:**
- No roster: you cannot get all seats → clones → pane → occupant in one call.
- No `--json` on `list`, `show`, `status` or `who`.
- Herdr's agent status is never shown.
- No "last seen": `observed_at`, `read_at` and `since` all mark changes, not heartbeats.

### 1.2 Which files a seat's agent has in context

**Render order.** `seat_parts` builds the stack in `src/bootstrap/instructions.rs:272-369`, from the committed HEAD only:

1. global `rules/**/*.md`, recursive and sorted (`src/store/instruction_source.rs:409-432`)
2. teamspace `teamspaces/<ts>/rules/**`
3. for the seat's `template_ref`, then each of its `applications[]`, de-duplicated (`instructions.rs:63-131,289-336`):
   1. the member's seat template `templates/<t>/AGENTS.md`
   2. the template root `AGENTS.md`
   3. the member `members/<m>/AGENTS.md`, followed by a `responsibility:` line
4. seat `instructions_sections` from `seat.toml`
5. seat `rules/**`
6. seat `AGENTS.md`, always last

Empty parts are dropped. An inadmissible file becomes a placeholder line (`instructions.rs:148-150,222-231`).

**Text format.** Line 1 is the revision line: `Instructions for seat <name> (<id>) at graph revision <12-hex>. …` Each part follows as `## <label> — <repo path>`, then its body (`instructions.rs:256-268`). No part carries its own hash.

**Readers:**
- `seat --instructions` is daemon-free and untruncated. It works only for the caller's pane.
- `seat --json` returns `.instructions = {rev, parts:[{label,path}], text}` (`src/bootstrap/mod.rs:370-380`), plus `paths.*`. Two caveats:
  - `paths.rules.*` lists only top-level `*.md` files (`mod.rs:80-93`), while the renderer is recursive.
  - `paths.data` lists context-only files. They are never rendered.
- `instructions show <global|teamspace t|seat s> [file]` is daemon-free. It shows the raw global, teamspace and seat files only. It does not show template, member, responsibility or section parts (`src/cli/instructions.rs:52-91`).

**Hook delivery.** The SessionStart hook is `seat --hook-prompt` for Claude, followed by `session-report`. For Codex it is `seat --hook-prompt --codex` with no session report (`src/bootstrap/setup_claude.rs:168-180`). It prints the render capped at `HOOK_RENDER_CAP = 9000` bytes:
- It keeps whole parts from the most specific end and drops the general ones.
- Omitted parts are named in a `[truncated: omitted …]` marker (`instructions.rs:18-20,418-488`).
- Plans warn at plan time when an edit would push a seat over the cap (`cap_warning`, `instructions.rs:493-524`).

**Loaded versus current — the main gap.** Nothing records what a session actually received:
- `session-report` sends the session id, transcript path and cwd only (`src/transcripts/capture.rs:85-105`).
- `NativeSession` and `Occupant` have no revision field.

So nobody can answer "is this agent running on stale instructions?":
- `plan instructions` only warns that live sessions keep the old text until a reset (`src/plan/instructions.rs:260-263`).
- Template text edits produce no session effect; only harness, model or args changes do (`src/model/effective.rs:158-185`).
- Today the only evidence is the revision line inside the agent's own conversation.

**Finding — `reload_required` is mislabelled.**
- Its only production setter is the pane-move mutation (`src/observe/mutations.rs:745-747`). The other setter is test-only (`src/plan/kinds_extra.rs:865`).
- No production code clears it.
- The notice it triggers says "Your instructions changed on disk. Reload your session…" (`src/threads/effects.rs:818`). That text is wrong for the actual trigger, a pane move.

This is tracked as bead hg-87k. It is not part of this design.

**Outside graph's knowledge.** These are not rendered, not tracked and never named in the Rust source:
- harness-native `CLAUDE.md`, `AGENTS.md` and `AGENTS.override.md` in the seat cwd and its ancestors. The cwd is the seat override, else the teamspace `project_repo`, else `.graph-local/cwd/<seat>` (`src/model/launch.rs:68-80`).
- skills
- harness settings
- the herdr-threads hook text

### 1.3 Recent activity and modifications

| Record | Content | Attribution | Reader today |
|---|---|---|---|
| Git commit on `main` | `<kind>: <summary>` with trailers `Graph-Op:` and optionally `Graph-Action:` (`src/writer/mod.rs:361-366`) | author is always `herdr-graph` (`src/store/git.rs:24-26`) | `git log` by hand. Only recovery parses the trailers (`src/writer/recovery.rs:33-57`) |
| `mutations/graph-changes/<yyyy-mm>/<op>.toml` `OperationRecord` | kind, summary, requester, confirmation `{mode tty\|relay, plan_hash, at}`, plan, action, `admitted_at`/`finished_at` (`src/model/operation.rs:7-60`) | `Requester{teamspace,seat,clone,native_session,human}` (`src/model/change.rs:81-93`), filled from `HERDR_GRAPH_SEAT`/`HERDR_GRAPH_CLONE` (`src/plan/commands.rs:99-126`). Empty for observed, bookkeeping and reconciler ops. `native_session` is never set | `show <path>` only |
| `mutations/undoable-actions/<yyyy-mm>/<act>.toml` `ActionRecord` | `kind`, `at`, `ops`, `affected[{object,before,after}]`, `retired` (`src/model/action.rs:7-31`) | none directly; only through `ops` | `undo --list [--json]`, undoable kinds only, 20 newest (`src/undo/candidates.rs:169-190`) |
| SQLite journal `.graph-local/journal.sqlite3` | tables `ops`, `effects`, `notices`, `meta` (`src/journal/mod.rs:22-37`). The only place rejected, failed and in-flight ops exist | requester inside `request_json` | `ops [--unresolved]` and `op <id>`: daemon only, no `--json`, fixed limit 200, no filters (`src/cli/ops.rs:67-86`; `src/plan/ops.rs:446-476`) |
| Transcript requests `mutations/transcript-processing/<rq>.toml` | status, range, delivery, result (`src/model/request.rs:6-23`) | `created_by_op` | `request list [--pending\|--unresolved\|--undispatched]`: no scope filter, no JSON |

Per-object stamps also exist: `Retirement{op,action,at,mechanism}` and `NameChange{…,source}` (`src/model/common.rs:163-188`).

**Gaps:**
- No `log`, `history` or `activity` command.
- No `--seat`, `--teamspace` or `--since` filter anywhere.
- The only per-seat view is `/seat`'s `pending ops:` list, for the caller only (`plan/ops.rs:116-131`).
- `status` has counts but no recent list.
- No CLI reads the graph-change records or the commit trailers.

## 2. Design

### 2.1 Principles

- **One scope grammar everywhere:** `global | teamspace <t> | seat <s>`. It is the same as `instructions show`. Omitting the scope means the whole instance.
- **Committed state is read daemon-free from HEAD**, like `list`, `show` and `instructions show`. Anything that needs the journal, a live Herdr snapshot or threads goes through the daemon and exits 5 when it is down. This keeps the 2026-11-05 rule: no new daemon-free journal reads.
- **Every new reader takes `--json`.** The JSON is the contract; text is a rendering of it. Each JSON document carries `schema: 1` and the `head` revision it was computed from.
  - Every fact, notice or degraded state the text rendering shows has a named, typed JSON field. A value that can be absent is marked `|null` in the shapes below. Applied in every view: the roster `instructions.parts` labels (§2.2), `last_change: null` plus `last_change_scanned` and `last_change_unreadable` (§2.4), `scan:{scanned,max_scan,stopped_early}` on `log` and on the roster when it walks history (§2.4), `tombstones` on `sessions` (§2.5), `live_unavailable` on `roster --live` (§2.2), `imports_followed: false` on `instructions stack` (§2.3) and the per-row `error` states below.
  - Under `--json`, stdout carries exactly one JSON document. A notice that has no field (the `log` pointer to `ops`, warnings about skipped records) goes to stderr. In text mode notices are part of the text rendering.
- **One HEAD per invocation.** Each daemon-free command resolves HEAD to one `CommitId` at start and passes it to Scope, Inventory, `seat_parts`, the loaded-record comparison, HistoryWalker and the Envelope. No component resolves HEAD itself, so a writer commit landing mid-run cannot tear the view: everything describes the `head` the document names. `roster --live` does the same inside the daemon.
- **One bad object never blanks a view.** A failure confined to one object becomes that row's error state, never a whole-command exit:
  - a seat whose stack does not resolve (a dangling `template_ref` or application, `src/bootstrap/instructions.rs:105-131`) gets `instructions: {"status":"error","message":"…"}` in `roster` and an `error` field on that seat in `stack`; the other seats render normally;
  - a commit whose `OperationRecord` or `ActionRecord` does not parse is listed by HistoryWalker with `op`/`action` null and `malformed: ["operation"|"action"]`, plus one stderr warning per command; the walk continues.
  - a commit whose tree has a record that does not parse cannot have its scope resolved. An unreadable historical tree is never skipped silently: `log` lists it as a row marked `malformed: ["tree"]` with its own stderr warning (it may be outside the scope, and it counts toward `--limit`), and the roster reports every seat still unmatched there as `last_change: null`, `last_change_scanned: false`, `last_change_unreadable: <commit>`, with a warning.
  - Whole-command exits stay for failures that leave nothing to show: no instance, HEAD unreadable, the record enumeration itself failing (exit 6 as today), and usage errors.
- **No new Git writes.** Inspection must not add commits. The one new piece of state, the loaded-instructions record, lives in `.graph-local/`.

### 2.2 `herdr-graph roster [<scope>] [--live] [--json]`

**Without `--live`** it is daemon-free and reads committed HEAD. It shows one block per non-retired seat, with its clones beneath. `--all` includes retired seats.

```text
proj (ts_01…) active
  foreman (st_01…) active  manager: node-manager
    main (cl_01…)  present  w3:t2:p7  claude ns_01… (3f2a…, ~/.claude/projects/…/3f2a….jsonl) since 10:42Z  threads sn…  resolved  instructions: current @1e01e11c4a2b
    alt  (cl_02…)  absent   -          -                          threads sn…  retired(reversible)  instructions: unknown
  researcher (st_02…) dormant
```

Columns and their sources:

| Column | Source |
|---|---|
| lifecycle | `SeatRecord`/`CloneRecord.lifecycle` |
| availability, pane | `runtime.availability`, `runtime.bound` |
| occupant harness, ns id, since | `CloneRecord.occupant` |
| threads seat, state | `threads_seat`, `seat_fact.state` |
| manager | `model::manager` resolution |
| instructions | §2.3: `current`, `stale (N parts)`, `unknown`, `error`, plus a `truncated` flag and the count of parts loaded |
| `reload_required` | the flag, when set |

**With `--live`** it makes one daemon call, `roster.live`. The daemon takes one Herdr snapshot and adds per clone:
- `agent: {kind, status}` from `pane.agent`;
- `live_pane_matches`: whether the snapshot shows the committed binding. It is true only when the clone's `runtime.bound` is of the same Herdr server as the daemon's current incarnation (`Incarnation::same_server`, `src/model/common.rs`, as `src/reconcile/planner.rs` already filters) **and** the snapshot has that pane in that tab and workspace. A binding from an earlier Herdr incarnation is `false` whatever ids the new server reuses, and the text shows `binding from earlier herdr server`.

It also adds the threads `seat_refusals` and `seat_lost` already held in memory (`src/daemon/compose.rs:441-511`). This is the only way to answer "is an agent running right now" rather than "did the observer last commit an occupant".

**Degraded live state.** The overlay is never silently empty:
- daemon down: exit 5 through `cli::daemon_error`, as `ops` does today;
- daemon up but no Herdr client, or the snapshot call fails: the command still prints the committed roster, with `live: true`, every clone's `live: null`, and `live_unavailable: {"herdr": "<reason>"}` in the envelope. The text prints `live data unavailable: herdr: <reason>` as its first line. The handler must not reuse `snapshot().await.ok()?` (`src/bootstrap/mod.rs:65-67`), which turns a failure into "no agent";
- threads seat view not running: `seat_refusals`/`seat_lost` are null and `live_unavailable.threads` names the reason.
- `live_unavailable` is `null` when everything was read. Exit code is 0 in the partial cases, because the committed part is still correct.

JSON (`|null` marks nullable values, §2.1):

```json
{"schema":1,"head":"<40-hex>","live":false,"live_unavailable":null,
 "scan":{"scanned":412,"max_scan":5000,"stopped_early":false},
 "teamspaces":[{"id":"ts_…","name":"proj","lifecycle":"active",
   "seats":[{"id":"st_…","name":"foreman","lifecycle":"active","manager":{"id":"st_…","name":"node-manager"}|null,
     "is_node_manager":false,"reload_required":false,
     "last_change":{"at":"…","op":"op_…","summary":"…"}|null,"last_change_scanned":true,
     "clones":[{"id":"cl_…","name":"main","lifecycle":"active",
       "runtime":{"availability":"present","pane":"p7","tab":"t2","workspace":"w3","observed_at":"…"},
       "occupant":{"harness":"claude","native_session":"ns_…","native_session_id":"3f2a…","since":"…"}|null,
       "threads":{"seat":"sn…","state":"resolved","read_at":"…"}|null,
       "instructions":{"status":"current|stale|unknown|error","loaded_rev":"…"|null,"loaded_at":"…"|null,
         "loaded_session":"3f2a…"|null,"source":"hook|hook-codex"|null,"truncated":false,
         "parts":[{"label":"global rules/style.md","delivered":"whole|cut"}],"omitted":["…"],
         "stale_parts":[{"label":"…","change":"modified|added|removed"}],"message":null},
       "live":null}]}]}]}
```

`instructions.parts` and `omitted` are the labels from the loaded record (§2.3), so the roster answers which files each clone was handed; they are `[]` when there is no record. `stack`'s clones section carries the same object per clone. `message` is set only for `error`. `scan` is present when the command walked history for `last_change`, else absent.

**Cost:**
- Daemon-free mode reads every seat and clone record at HEAD: O(records), the same as `list`.
- The instruction status column needs one render per active seat (§2.3). That is the dominant cost. It is still only tree reads, roughly the cost of `seat --instructions` per seat. A seat whose render fails costs nothing further and shows `error` (§2.1).
- `--live` adds one Herdr snapshot.

### 2.3 Loaded instructions and staleness

**New local record:** `.graph-local/loaded/<clone-id>.json`. It is the last hook injection into one **native session** of that clone. It is written atomically (temp file plus rename), best-effort, and never fails the caller.

```json
{"schema":1,"clone":"cl_…","seat":"st_…","native_session_id":"3f2a…"|null,"rev":"<40-hex>","at":"…","source":"hook|hook-codex",
 "parts":[{"label":"global rules/style.md","path":"rules/style.md","delivered":"whole","digest":"sha256:…","bytes":812},
          {"label":"teamspace rules/a.md","path":"teamspaces/proj/rules/a.md","delivered":"cut","digest":"sha256:…","bytes":120}],
 "omitted":["global rules/old.md"],"bytes":8740}
```

- `parts` lists exactly the parts the injection delivered, in render order. `delivered` is `whole` or `cut`; at most one part is `cut` (the `HookFit` cut part).
- `digest` and `bytes` describe the bytes actually delivered: the full rendered body for a `whole` part, the delivered prefix for the `cut` part.
- `omitted` lists the labels of parts the cap dropped entirely. They were not delivered, so they carry no digest.
- `at` is informational only. No rule compares it with anything.

**Who writes it.** Only the hook, the one path that injects text into a session's context:
- `seat --hook-prompt` (source `hook`) and `seat --hook-prompt --codex` (source `hook-codex`), after they print.
- `seat --instructions` and the daemon's `seat.resolve` for `seat --json` **do not write**. They hand text to whoever ran them, which is often a script reading one field (`seat --json | jq .beads_labels`), not a session's context, so they must not overwrite what the hook delivered.

The clone is resolved as the hook already resolves it (`resolve_caller` with no snapshot pane, `src/cli/seat.rs:133-145`). An unbound caller writes nothing. A record for a different `native_session_id` is replaced; one per clone is kept.

**Where the hook gets the native session id.** The record's tie to a session is the harness's own session id, the same value the occupant's `NativeSession.native_session_id` holds:
- **Claude.** The SessionStart hook's stdin JSON carries `session_id` (and `source`: startup, resume, clear, compact); `session-report` already parses it with `parse_claude_hook` (`src/transcripts/capture.rs:67-81,97`). Today `seat --hook-prompt` does not read stdin so that `session-report` can (`src/bootstrap/setup_claude.rs:168-172`). The hook command changes to read the payload once and feed both:
  `p=$(cat); printf '%s' "$p" | <bin> seat --hook-prompt --stdin-payload; printf '%s' "$p" | <bin> session-report --from-hook claude # herdr-graph-owned`.
  `--stdin-payload` is a new hidden flag: only with it does `seat --hook-prompt` read stdin, so an older hook command still installed in a user's settings keeps feeding `session-report` unchanged. Such a hook writes `native_session_id: null` until `setup` rewrites the owner-marked hook (`setup_claude.rs:195`).
- **Codex.** The hook command becomes `<bin> seat --hook-prompt --codex --stdin-payload`, and the hook reads `session_id` from its stdin JSON when the payload carries one. Nothing else reads Codex hook stdin. Codex's hooks documentation (https://learn.chatgpt.com/docs/hooks, fetched in design roast round 2) says the SessionStart stdin payload carries `session_id`; the repo itself has no capture of one. Graph takes the occupant's Codex id from the Herdr snapshot `agent_session` (a file stem when Herdr reports a path) or `resume <id>` argv (`src/model/harness.rs:89-104`, `src/observe/sessions.rs:38-45`). The status compares the two byte for byte; hg-blw.12 records which form a real or documented Codex payload uses and, if it differs from the occupant's, the clone reads `unknown` with a `note: "codex session id form differs"` rather than guessing a mapping. The hook does **not** take a Herdr snapshot itself: it is daemon-free, runs under a 10 s budget, and at SessionStart Herdr may not have detected the agent yet. So a Codex payload without `session_id`, like an unparseable payload, records `native_session_id: null`, and that clone's status is `unknown` with the record still shown (`loaded … session not identified`). That is the honest answer, not a bug to paper over with timestamps.
- Parsing is lenient: a missing or malformed payload never changes the hook's output or exit code.

**Digest choice.** `digest` is the SHA-256 of the rendered part body, not the Git blob id:
- Seat sections and `responsibility` are not files of their own; their `path` is `seat.toml` or `template.toml`.
- A blob id would mark them stale on any unrelated record edit.

**Status.** The status first decides whether the record describes the session now in the clone, by identity, then compares content. The occupant's native id is the `native_session_id` of the `sessions[]` entry that `occupant.native_session` names.

| Status | Condition |
|---|---|
| `unknown` | no record; a corrupt or older-schema record; the record's `native_session_id` is null; the clone has no occupant; or the record's `native_session_id` differs from the occupant's. When a record exists, `loaded_session`, `loaded_rev` and `loaded_at` are still shown, so "loaded into session 3f2a…, not yet reported as the occupant" is visible during the seconds between the hook and `session-report` |
| `current` | the record's `native_session_id` equals the occupant's, and a hook injection rendered at HEAD now (`fit_hook` over the HEAD manifest, cap 9000) would deliver the same parts: the same labels in the same `delivered` mode with the same digests |
| `stale` | the ids match and the delivered sets differ. `stale_parts` lists `{label, change}`: `modified` (same label, different digest or delivery mode), `added` (delivered at HEAD, not in the record's `parts`), `removed` (in the record's `parts`, not delivered at HEAD). Parts omitted on both sides never count |
| `error` | the seat's stack does not resolve at HEAD (§2.1); `message` says why |
| `truncated` (flag) | the record has `omitted` labels or a `cut` part. Reported beside the status, never instead of it |

Comparing against what the hook would deliver now, rather than against the full manifest, means a truncated seat whose sources are unchanged reads `current`, and an edit to a part the agent never received is not staleness. An edit that moves the cap boundary is, because a reload would deliver different text.

There is no comparison of `at` with `occupant.since`, so the production order (the Claude hook runs before `session-report` stamps `since`; Codex's occupant arrives later through the observer diff) does not matter: once the occupant with that native id is committed, the same record reads `current` or `stale`.

The commit shown beside a stale part, and in `stack`'s part list, is the last commit that touched the part's `path` (one HistoryWalker last-touch lookup per path). For seat sections and `responsibility` the path is a whole record file, so the column is labelled `path last touched`, not "changed": it may name an unrelated edit to that record. Staleness itself is digest-driven and does not depend on it.

**Honest limits.** The record says what graph **injected**, not what the model retained:
- A session that compacts gets a fresh hook injection with the same `session_id`, and the record updates.
- A session that ignored the hook, or whose hook failed, shows the last successful injection for that session, or `unknown` if there was none.
- Codex runs the hook only after the user trusts it, and Codex records trust against the hook's hash, so the `--stdin-payload` rewrite makes every Codex install skip the hook until the user re-trusts it in `/hooks`. `setup codex` prints that notice when it changes the command, and `doctor` reports an untrusted-after-change hook as a warning it cannot fix (graph never writes trust). Codex may also not supply a usable session id (above).
- So `unknown` is a real state, not an error.

**New reader:** `herdr-graph instructions stack <seat> [--clone <c>] [--json]`. It is daemon-free and works for **any** seat, not just the caller's. It prints the manifest at HEAD:
- each part: label, path, bytes, digest, and the commit that last touched its path (`path last touched`, see Status above);
- which parts the 9000-byte hook would keep;
- per clone, the loaded status from the record, as the same `instructions` object the roster carries (§2.2);
- **harness-native files present**, listed by path with size and mtime, marked `not graph-managed`, and never read for content. JSON: `harness_files:[{path,bytes,mtime,origin,harnesses,note}]`, `origin` one of `project | ancestor | user`, `harnesses` a list from `claude | codex` (every harness whose discovery rule below picks that file; the seat's own harness is marked, others are listed, not filtered), `note` null or one of `shadowed` and `claude-only-without-claude-md`.
  - Discovery, one rule per harness, applied to the seat's cwd (the `src/model/launch.rs` rule):

    | Harness | Project files | User-level files |
    |---|---|---|
    | Codex | from the Git root of the cwd (the cwd itself when not in a Git repo) down to the cwd, at most one file per directory: `AGENTS.override.md` if present, else `AGENTS.md`. A shadowed `AGENTS.md` beside an override is listed with `note: shadowed` and without `codex` in `harnesses`. Directories above the Git root are not Codex's | `$CODEX_HOME/AGENTS.override.md` if present, else `$CODEX_HOME/AGENTS.md` (`CODEX_HOME` else `~/.codex`) |
    | Claude | in the cwd and every ancestor: `CLAUDE.md`, `CLAUDE.local.md`, `.claude/CLAUDE.md`, `.claude/rules/**/*.md`. `AGENTS.md` in those directories counts for Claude only when no `CLAUDE.md` or `CLAUDE.local.md` exists in the cwd or above it; it then gets `claude` in `harnesses` with `note: claude-only-without-claude-md` when Codex does not also pick it | `$CLAUDE_CONFIG_DIR/CLAUDE.md` (else `~/.claude/CLAUDE.md`) |

  - User-level paths are resolved from the inspecting process's environment, which may differ from the agent's own: graph never reads the agent's environment back (`src/model/launch.rs`), so these entries carry `env: inspector` in text and JSON.
  - "Present" is not "loaded": a harness setting can switch a file off, Codex caps project docs by size, and Claude loads subdirectory files on demand. The output says so in one line, and JSON carries `harness_files_note: "present, not necessarily loaded"`.
  - Because content is never read, `@`-imports are not followed; the output says so in one line, and JSON carries `imports_followed: false`.

Bodies are left out; `seat --instructions` and `show <path>` still print text.

```text
seat foreman (st_01…) at 1e01e11c4a2b — 6 parts, 7.9 KB, hook cap 9000: all kept
  1 global rules/style.md                     812 B  sha256:3f…  touched 5ad9cea 2026-11-06
  …
  6 seat AGENTS.md — teamspaces/proj/seats/foreman/AGENTS.md  2.1 KB  sha256:9a…  touched f6d32a8
harness files (not graph-managed):
  /home/u/src/proj/CLAUDE.md   3.4 KB  modified 2026-11-05 14:02
  /home/u/src/proj/AGENTS.md   1.1 KB  modified 2026-10-30 09:15
clones:
  main (cl_01…)  loaded 1e01e11c4a2b into 3f2a… via hook at 10:42Z  current
  alt  (cl_02…)  loaded 5278a66f… into 9c1d… via hook at 09:10Z  stale: modified seat AGENTS.md (path last touched f6d32a8)
```

Implementation note: this refactors `seat_parts` into a manifest (label, path, body, digest) that both the renderer and the stack view consume. The text the agents receive does not change.

### 2.4 `herdr-graph log [<scope>] [--since <date|dur>] [--limit N] [--by <seat>] [--all] [--json]`

**Daemon-free.** It walks `main` from the invocation's pinned HEAD (§2.1), newest first, reading only committed objects. That is safe while the writer runs, because Git objects are immutable.

**What it reads per commit:**
- the `Graph-Op`/`Graph-Action` trailers;
- the `OperationRecord` the commit added (requester, confirmation mode, kind);
- the `ActionRecord`, when present (`affected`, `retired`, `kind`), as that commit wrote it;
- the changed paths (`diff-tree`).

**Undo state comes from HEAD, not from the commit.** An undo commit appends its op to the original action's `undone_by` later (`src/undo/compensate.rs:248-274`), so the action as its own commit wrote it always reads "not undone". For each listed row that has an action, `action.undoable` and `action.undone` are read from that action's record at the invocation's pinned HEAD (§2.1), with the same rule as `undo --list` (`undone: !undone_by.is_empty()`, `src/undo/candidates.rs:184`). That is one record read per printed row, not per scanned commit, and it does not depend on the scan cap. An action whose record no longer exists at HEAD shows `undone: null`.

**Scope membership.** A commit is in scope when either of these holds:
- a changed path lies under the scope's folder, resolved to IDs against **that commit's own tree**, so renames and moves are attributed correctly. The `mutations/graph-changes` file every commit adds is ignored for this purpose. The scopes are:
  - `global`: `graph.toml`, `rules/`, `templates/`, `applications/`, `threads-namespace.json`
  - `teamspace`: the teamspace folder, including its seats
  - `seat`: the seat folder, including its clones, plus transcripts whose path carries the seat ID
- `action.affected`/`retired` names an object in scope.

**`--by <seat>` is a separate AND filter**, not a scope alternative. It keeps only commits whose `OperationRecord.requester.seat` equals that seat's id; the seat is resolved once at the pinned HEAD with the same Scope resolver (name or id, retired allowed). Because the requester is stored by id, a later rename of that seat does not matter. So `log --by b` is "everything seat b requested" (instance scope AND requester b), and `log seat a --by b` is "changes to a that b requested". Commits with no requester (observed, bookkeeping, reconciler) never match `--by`.

**Noise.** `bookkeeping` kinds (seat facts, transcript processing) are hidden unless `--all`. Observed changes (renames, moves, closures, occupancy) stay, because they are exactly the activity a user wants to see.

```text
2026-11-07 10:41Z  op_01…  seat_rename        foreman → lead                     by foreman/main (relay)
2026-11-07 09:58Z  op_01…  observed           session ended on main (AgentExited) observed
2026-11-06 22:10Z  op_01…  instructions_edit  seat AGENTS.md                     by node-manager (tty)  act_01… undone
```

JSON document: `{schema, head, scan:{scanned, max_scan, stopped_early}, rows:[…]}`. JSON row: `{commit, at, op|null, kind|null, summary, requester:{teamspace,seat,clone,human}|null, confirmation:{mode,plan_hash}|null, action:{id,kind,undoable,undone|null}|null, objects:[{id,kind,name,change}], paths:[…], malformed:["operation"|"action"|"tree"]…}`. `op` and `kind` are null for an unattributed commit (no `Graph-Op` trailer) or a malformed `OperationRecord` (§2.1).

**Cost.** One commit read and one tree diff per scanned commit. `--limit` defaults to 30. A scoped query scans until it finds `limit` matches, or until `--max-scan`, default 5000 commits. When it stops early, `scan.stopped_early` is true; the text rendering ends with `scanned 5000 commits, older history not searched`. So a `--json` consumer can tell "no matching changes" (`rows: []`, `stopped_early: false`) from "cap hit". No index is proposed. If long histories make it slow, a derived cache in `.graph-local/` can come later without changing the output.

**Journal side**, which is not in Git: `ops` gains `--seat <s>`, `--teamspace <t>`, `--limit N` and `--json`, and keeps going through the daemon. The filter is on requester. Rejected, failed and in-flight ops exist only there. `log` prints a one-line pointer to `ops --seat …` instead of reading the journal itself: as the last text line in text mode, and on stderr under `--json` (§2.1).

**Last change in the roster.** `roster` can carry each seat's `last_change {at, op, summary}` from the same walker in one pass, with the same scan cap. A seat with no in-scope commit within the cap has `last_change: null`; `last_change_scanned` is false when the walk stopped early (so older history might hold one) and true when the walk reached the root commit. The text shows `older than scan` for the first case and `-` for the second. A third case is a seat still unmatched when the walk reaches a commit whose tree has a record that does not parse (§2.1): the seat is not matched against older commits, so `last_change` is `null`, `last_change_scanned` is `false`, and `last_change_unreadable` is that commit (`null` in every other case). The text shows `last change unknown (unreadable tree at <short id>)` for the seat and one `warning:` line for the roster. The roster envelope carries the same `scan` object.

### 2.5 `herdr-graph sessions [<scope> | clone <c>] [--since <date|dur>] [--limit N] [--json]`

Added 2026-11-08 at the user's request, relayed from the artifacts and transcripts survey (`docs/superpowers/specs/2026-11-08-artifacts-beads-transcripts-survey.md`, Part 2). The request: "ability to list all sessions, by space/seat/clone, with timestamps and most recent first".

**Data.** Sessions belong to clones: `CloneRecord.sessions: Vec<NativeSession>` plus `occupant` (`src/model/clone.rs:25,27`). Each `NativeSession` holds `id` (ns_), `harness`, `native_session_id`, `transcript_path?`, `transcript?` (tr_), `cwd`, `started`, `ended?` and `end_reason?` (`src/model/native_session.rs:9-34`). Today only `show <clone>` exposes the native id and path:
- `who` prints the ns_ id;
- `seat --json` carries only `beads_labels.ns`.

**Command.**
- Daemon-free; reads committed HEAD.
- Lists every session of every clone in scope, flat, sorted by `started` descending. Ties are broken by ns_ id.
- Includes retired clones and seats and teamspaces under `archive/`. Their `sessions[]` persist, and `list_clones` already enumerates both `seats/` and `archive/seats/` (`src/store/layout.rs:197-216`).
- The scope grammar gains `clone <c>` for this command only. A clone has no folder of its own for `log` or `roster` scoping to rely on.

```text
STARTED            ENDED              END          TEAMSPACE/SEAT/CLONE   HARNESS  NS        NATIVE ID      TRANSCRIPT
2026-11-08 09:12Z  -                  -            proj/foreman/main      claude   ns_01…    3f2a…-…        ~/.claude/projects/…/3f2a….jsonl
2026-11-07 21:40Z  2026-11-08 09:11Z  session_changed proj/foreman/main   claude   ns_01…    9c1d…-…        ~/.claude/projects/…/9c1d….jsonl
2026-11-07 18:03Z  2026-11-07 19:55Z  pane_closed  proj/reviewer/alt [retired]  codex  ns_01…  019a…        unresolved
```

**Transcript column.** It shows `transcript_path`. When that is absent but `transcript` (tr_) is set, the path comes from the transcript record's `transcript_path` (`src/model/transcript.rs:13`). Otherwise it shows `unresolved`. Codex sessions are always `unresolved` today, because `transcript_path` is never captured for Codex (`src/model/harness.rs:99-103`, `src/observe/sessions.rs:63`). The column says so; it does not guess a path.

**JSON row:**

```json
{"ns":"ns_…","native_session_id":"…","harness":"claude","started":"…","ended":null,"end_reason":null,"current":true,
 "teamspace":{"id":"ts_…","name":"proj","lifecycle":"active"},"seat":{"id":"st_…","name":"foreman","lifecycle":"active"},
 "clone":{"id":"cl_…","name":"main","lifecycle":"active"},
 "transcript":{"id":"tr_…","path":"…","source":"session|transcript_record"}|null,"cwd":"…"}
```

`current` is true when this session is the clone's `occupant`. The document is `{schema: 1, head, tombstones: N, total: M, rows: […]}`, where `tombstones` is the count of purge tombstones in scope (the text footer below).

**Limit — purge discards session history at HEAD.** `clone purge` replaces the record with a tombstone that has `sessions: []` (`src/threads/assignment.rs:89-118`), or removes it entirely. After that:
- the sessions survive only in Git history and in the transcript index under `transcripts/`;
- `sessions` reads HEAD, so a purged clone's sessions are not listed;
- the output footer says how many tombstones are in scope.

Recovering them from history (`--history`, a Git walk like `log`) is possible but not proposed until someone needs it.

**Cost.** O(clone records in scope), the same as `list clones`, plus one transcript-record lookup per session that lacks a path. `--limit` defaults to 50 and `--since` filters on `started`. The document carries `total` (sessions in scope after `--since`) next to `rows`, and the text adds a footer `showing N of total; use --limit` whenever `rows` is shorter than `total`.

**Roster addition.** The occupant column in `roster` also shows the current session's `native_session_id` and transcript path (or `unresolved`). The JSON `occupant` gains `native_session_id` and `transcript: {id, path, source}|null`. Both come from the occupant's own `sessions[]` entry; `Occupant` itself holds only the ns_ id (`src/model/common.rs:144-148`).

### 2.6 Sources and cost summary

| View | Git HEAD | Git history | `.graph-local/loaded` | Journal | Herdr snapshot | Threads | Daemon |
|---|---|---|---|---|---|---|---|
| `roster` | ✓ | (last_change) | ✓ | — | — | — | no |
| `roster --live` | ✓ | (last_change) | ✓ | — | ✓ | daemon's in-memory seat view | yes |
| `instructions stack` | ✓ | per changed path | ✓ | — | — | — | no |
| `log` | ✓ | ✓ | — | — | — | — | no |
| `sessions` | ✓ (clone + transcript records) | — | — | — | — | — | no |
| `ops --seat/--json` | — | — | — | ✓ | — | — | yes |

Threads presence (busy/idle, last message) is out of scope. Graph's threads port has no read for it. The roster prints the threads seat ID so `herdr-threads seat list` can be cross-referenced. A threads-side API would be a separate request to that project.

### 2.7 Herdr UI

Not now. Every view above is a CLI command with JSON output, so a herdr-bar or pane plugin can render them later without new graph APIs. Building a UI first would freeze an unvalidated shape.

## 3. Recommendation, rejected options, phases

**Recommendation:**
- Ship `roster`, `instructions stack` with the loaded record, `sessions`, and scoped `log`, plus filters and `--json` on `ops`.
- They are separate commands because the questions have different sources and costs.
- `sessions` is its own reader rather than a `roster` flag: the roster is a tree of current state, while sessions are a flat, time-sorted history that includes retired objects.

**Rejected:**

| Option | Why not |
|---|---|
| One `inspect` / `state` super-command | Mixes daemon-free and daemon reads, and a cheap HEAD read with a history walk. The cost becomes invisible to the caller. Three commands with one shared scope grammar are clearer. |
| Fold the roster into `status` | `status` is the operator's exception-only diagnostic, with its own daemon-down fallback. A roster there bloats it and blurs that contract. |
| Commit the loaded revision onto `CloneRecord` | A Git commit on every session start, `/clear` and compaction would mean churn in history and writer load. The hook is also daemon-free and must never block or fail. Loaded state is local runtime observation, like the journal, so it belongs in `.graph-local/`. |
| Detect what was loaded by reading native transcripts | Harness-specific and fragile, and transcripts may be large or rotated. Graph already knows exactly what it injected at injection time. |
| Heartbeat / last-seen field | Needs periodic writes for little gain. `roster --live` answers "running now" from one snapshot on demand. |
| Daemon-free journal reads for `log` | Reverses the 2026-11-05 decision that removed daemon-free fallbacks. Committed history covers `log`; `ops` covers the journal. |
| Git blob IDs as part digests | Seat sections and `responsibility` live inside `seat.toml`/`template.toml`, so any edit to those records would show as instruction drift. |
| Per-commit index for `log` | Not justified before real history sizes show a need. The scan cap bounds the cost and the output says when it stopped. |

**Phases:**

1. **Roster and stack, read-only, no new state.**
   - `roster [<scope>] [--all] [--json]`;
   - `instructions stack <seat> [--json]` with the manifest, hook-cap simulation and harness-file presence;
   - `sessions [<scope> | clone <c>] [--since] [--limit] [--json]`, plus native session id and transcript path in the roster occupant column;
   - the `seat_parts` manifest refactor.

   The instruction column shows `unknown` until phase 2.
2. **Loaded record and staleness.** The `.graph-local/loaded/` writer in the hook only (Claude and Codex), keyed by the native session id from the hook's stdin payload; the `--stdin-payload` flag and the rewritten owner-marked hook commands in `setup`; `current`/`stale`/`unknown`/`truncated` in roster and stack (§2.3). `seat --instructions` and `seat.resolve` stay non-writing.
   - Its tests follow the production order. The end-to-end test starts from a clone with **no occupant**, pipes one Claude SessionStart payload through the real hook command (`seat --hook-prompt --stdin-payload`, then `session-report --from-hook claude`), waits for the daemon to commit the occupant, and asserts `current`. It then asserts `unknown` for a record whose `native_session_id` differs from the occupant's, `unknown` for a Codex record with `native_session_id: null`, and that a `seat --json` or `seat --instructions` call leaves the record byte-identical.
3. **History.**
   - `log [<scope>] [--since] [--limit] [--by] [--all] [--json]`;
   - `ops --seat/--teamspace/--limit/--json`;
   - roster `last_change`.
4. **Live.** `roster --live` through the daemon: the Herdr agent kind and status, binding match, threads refusals and lost seats.

**Follow-ups found during the survey.** These are separate fixes, not part of this design:
- Bead hg-87k: the `reload_required` notice text claims an instruction change, but the only trigger is a pane move, and nothing clears the flag (`src/threads/effects.rs:818`, `src/observe/mutations.rs:745-747`). This design does not cover the fix.
- `seat --json` `paths.rules.*` lists only top-level files, while the renderer is recursive (`src/bootstrap/mod.rs:80-93`).
- `who` resolves threads seats only through invitation links, not `clone.threads_seat` (`src/threads/mod.rs:324-328`).
- A stale comment in `src/plan/ops.rs:446` claims a daemon-free `ops` fallback that no longer exists.

## 4. Product decisions (user, 2026-11-07)

1. **Staleness is shown, not notified.** Phase 2 adds no reload notice. Notification will be decided once we have seen how often staleness occurs. The mislabelled `reload_required` notice is tracked separately as bead hg-87k.
2. **`log` hides `bookkeeping` by default.** Observed changes stay visible; `--all` shows everything.
3. **Harness-native files: presence, size and mtime.** `instructions stack` lists each file's path, size and mtime, marked `not graph-managed`. It never reads their content.
4. **`roster --live` is in scope now** (phase 4), not deferred.

## 5. Overlap with onboarding-design

Migration from existing Herdr spaces will want to see unmanaged workspaces, tabs and panes next to managed ones. `roster --live` already takes the snapshot, so an `--unmanaged` section listing snapshot objects with no graph binding would be a small, natural extension. This should be coordinated through the main tab before either design fixes the shape.

**Decided (user, 2026-11-08, S1):** `roster --live --unmanaged` adds a section that only enumerates the Herdr workspaces, tabs and panes in the snapshot that have no graph binding, with their ids and labels as Herdr reports them. No other details (no agent kind, threads seat or suggestions). JSON: `unmanaged: {workspaces:[{id,label|null}], tabs:[{id,label|null,workspace}], panes:[{id,label|null,tab}]}`, absent without `--unmanaged`; `label` is null when Herdr reports none (`src/ports/herdr.rs:36`); further fields may be added later without breaking `schema: 1` readers.

"Has a graph binding" is incarnation-aware, as `live_pane_matches` is (§2.2): an object counts as bound only when some non-retired record's `runtime.bound` names it **and** that binding is `same_server` as the daemon's current Herdr incarnation. The records that can bind are non-retired teamspaces, seats and clones (each has `runtime.bound`, a `Binding` with optional `workspace_id`, `tab_id` and `pane_id`, `src/model/common.rs`); an object is bound when any such binding names its id in the matching field. A binding left from an earlier Herdr server does not hide a new object that reuses its id. If the snapshot is unavailable, `unmanaged` is null and `live_unavailable` says why (§2.2).

## Key abstractions

**InstructionManifest** — extended `bootstrap::instructions::{Part, SeatParts, seat_parts}`: each `Part` gains `bytes` and a `digest` (SHA-256 of the rendered body), computed in one place, `Collector::add`.
- Owns: the ordered part list of one seat at one tree, with identities that can be compared. The renderer (`render_instructions`), the hook cap (`fit_hook`), doctor, `instructions stack` and the loaded record all consume this one list.
- Hides: how a part's identity is computed. Per spec §2.3 it is the body digest, not the blob id.
- Must not: compute staleness or read `.graph-local` (that is LoadedInstructions), or change the rendered text (spec §2.3 implementation note).

**Scope** — new closed enum in a new `inspect` module: `Instance` (scope omitted), `Global`, `Teamspace(id)`, `Seat(id)`, `Clone(id)`, with one parser and one resolver at the invocation's pinned HEAD (§2.1).
- Owns: the shared scope grammar (§2.1) and name-or-id resolution. It resolves retired targets too, because `sessions` and `log` cover retired objects. The rule: one live match resolves; several live matches are ambiguous; with no live match, one retired match resolves and several retired matches are ambiguous (the error lists every id); no match is unknown target. An id always resolves, live or retired. It also owns two membership answers:
  - object membership: does this teamspace, seat or clone fall in scope;
  - path membership at a given commit's tree: the folders of the scope's IDs, located in that tree (§2.4), with `global` mapped to its fixed path set and the `mutations/graph-changes` file excluded.
- Hides: where a scope's objects live in the tree, including `archive/` moves and renames across history.
- Must not: be `InstructionScope`. That enum is about instruction files, and its `Global` means something else. Scope reuses `instruction_source`'s `pick` and error wording rather than redefining them: `pick` gains a retired-allowed mode that Scope uses, applying the retired-only rule above instead of returning `InstructionError::Retired` (`src/store/instruction_source.rs:259-285`). The default mode, and so `instructions show`, is unchanged. It must not filter by kind or requester (LogQuery does that).
- Errors: usage, unknown target and ambiguous name. They follow the `InstructionError` messages and map to the same exit codes as `instructions show`.

**Inventory** — new module: the joined teamspace → seat → clone tree read once from the pinned HEAD it is given (live and `archive/`) and filtered by a Scope.
- Owns: joining clone to seat to teamspace by ID, and attaching each clone's current occupant session (its `sessions[]` entry, plus the transcript path through the transcript record, else `unresolved`). It uses `layout::list_teamspaces`, `all_seats` and `all_clones`.
- Hides: record enumeration, the parent join, and the transcript-path fallback rule of §2.5.
- Must not: decide what a view shows (retired filtering, sorting and limits belong to each view), or read `.graph-local` or Git history.
- Relationships: `roster` (a tree) and `sessions` (a flat list sorted by `started` descending) both consume it. These are the two consumers that justify it.
- Errors: `StoreError` only, and only for the enumeration itself (a tree or record-list read failing), which leaves no view to show. An integrity error stays distinguishable, so it maps to exit 6 as `seat --instructions` already does. A per-seat stack render failure is not Inventory's: the view that renders the manifest turns it into that seat's `error` row (§2.1).

**LoadedInstructions** — new module owning `.graph-local/loaded/<clone>.json`, modelled on `observe::baseline`. It holds the record type (keyed by clone, bound to one `native_session_id: Option<String>`), a closed `InjectionSource` enum (`Hook | HookCodex`), and a closed `InstructionStatus` enum (`Current | Stale(changes) | Unknown`) with a separate `truncated` flag. The roster/stack `error` state is not a variant here: it arises before this module is asked, when the HEAD manifest does not render (§2.1).
- Owns:
  - recording one hook injection: clone, seat, the native session id from the hook payload (or none), the rev, and the delivered parts from the manifest plus its `HookFit` (kept parts `whole`, the cut part `cut` with the digest of its delivered prefix, omitted labels). The write is atomic through `fsutil::write_atomic` and best-effort;
  - reading a record;
  - the single status rule of §2.3: first identity, then content. The record is `Unknown` unless its `native_session_id` is set and equals the occupant's native session id (the `native_session_id` of the occupant's `sessions[]` entry, which Inventory supplies). Only then does it compare the record's delivered parts with `fit_hook` over the HEAD manifest. There is no comparison of `at` with `occupant.since`.
- Hides: the file format and schema, and the comparison rule (label, delivery mode and delivered-bytes digest; `modified`, `added` or `removed`; omitted parts never compared).
- Must not: fail or block its caller. Every write error is swallowed. It must not touch Git. Callers never compare digests themselves; `roster` and `instructions stack` both ask this module for the status.
- Relationships: depends on InstructionManifest and on Inventory's occupant session. One caller writes: `cli::seat::hook_prompt` (via `hook_text`, with the clone from `Resolution::Bound` and the session id parsed from stdin under `--stdin-payload`, both harnesses). `print_instructions` and the daemon's `seat.resolve` (`bootstrap::seat_reply`) never call it. `bootstrap::setup_claude::{hook_command, codex_hook_command}` emit the stdin-sharing commands of §2.3.
- Lifecycle: one file per clone, holding the latest hook injection; there is only one writer kind, so no precedence rule is needed. A hook for a new native session (after `/clear` or a restart) replaces the record; until the occupant catches up, the status is `Unknown` with the record shown. It lives until the instance is deleted. Nothing removes a purged clone's file; that is harmless, and noted as a follow-up.
- Errors: none cross the boundary. A missing, corrupt or older-schema record reads as `Unknown`.

**HistoryWalker** — new module (`store::history`, over `GitStore::with_repo`): walks `main` from the pinned HEAD `CommitId` it is given, newest first, and yields one `Change` per commit: commit id and time; the `Graph-Op` and `Graph-Action` trailers; the added `OperationRecord` and the `ActionRecord`, when present; the changed paths from `diff-tree`. The walk stops at a scan cap and reports whether it stopped early.
- Owns: the revwalk, trailer parsing, per-commit record reads, the scan cap and the "older history not searched" report. It also answers "last commit that touched path P", used by `stack` and `stale_parts`.
- Hides: that the source is a live Git walk. The derived `.graph-local` cache that §2.4 allows later replaces it here without changing any caller.
- Must not: filter by scope, kind or requester (LogQuery does that), or read the journal (§2.1).
- Relationships: `writer::recovery::trailers_since` and the walker share one trailer-parsing function, extracted rather than duplicated. Consumers are `log`, roster `last_change` (one pass with the same cap) and `instructions stack`.
- Lifecycle: one per invocation, borrowing the CLI's `GitStore`. It is not shared, and it is safe while the writer runs because Git objects are immutable.
- Errors: `StoreError::Git` for the walk itself. Hitting the cap is a reported outcome, not an error. A commit without a `Graph-Op` trailer, such as the init commit, comes through as an unattributed `Change`. A commit whose `OperationRecord` or `ActionRecord` fails to parse comes through with that record absent and `malformed` naming it; the walk continues and the view warns once on stderr (§2.1).
- Also owns: reading an action's record at the pinned HEAD for `log`'s `undoable`/`undone` columns (§2.4), once per printed row.

**LogQuery** — new: the `log` filter.
- Owns: deciding which `Change`s match. A change is in scope when Scope's path membership holds at that commit's tree, or when `affected` or `retired` names an in-scope object. `--by` is applied after that as an AND filter: the requester's seat id must equal the resolved `--by` seat (§2.4). Without `--all` it drops `RequestKind::Bookkeeping` (§4 decision 2). It also applies `--since` and `--limit`.
- Must not: walk history itself. Roster `last_change` uses the same predicate with the seat's Scope, so it is not written twice.

**View output** — new small shared piece: an `Envelope` (`schema: 1`, `head`, `live` where applicable, body) and one emit step that chooses JSON or text. Each view renders text from the same struct it serializes (§2.1: the JSON is the contract).
- Relationships: five views exist now (roster, stack, log, sessions, ops), and each supplies its own text rendering. This is a shared interface with one implementation per view.
- Must not: hold any logic beyond output selection.

**Roster** — new view builder: Inventory plus InstructionManifest plus LoadedInstructions (plus HistoryWalker for `last_change`), turned into the roster struct.
- Owns: roster shape, retired filtering (`--all`), and the `reload_required` and manager columns (through `model::manager::Pointers`).
- Relationships: the CLI calls it on HEAD with no daemon. The daemon's new `roster.live` command calls the same builder, then attaches a `live: Option<CloneLive>` per clone from one Herdr snapshot (agent kind and status, binding match) plus `seat_refusals` and `seat_lost`, and, with `--unmanaged`, the unbound-object list (§5, S1). There is one builder, and the live data is an overlay field, not a second roster.
- Errors: daemon down for `--live` is exit 5 through `cli::daemon_error`, as `ops` does today. Daemon up with the Herdr snapshot or threads view unavailable is not an error: the overlay is null and `live_unavailable` names the cause (§2.2). Binding matches use `Incarnation::same_server` against the daemon's current incarnation.

**`plan::ops::list_ops`** — extended: it takes a requester filter (seat, teamspace, unresolved) and a limit. `pending_for` becomes this function with a filter instead of a second copy of the loop. `ops.list` reads the new arguments, and `cli::ops` gains `--json`.

**`instruction_source` (`pick`, `parse_words`)** — extended only to expose name-picking to Scope. `instructions show` behaviour is unchanged.

Variation:
- Scope kinds, injection sources, instruction statuses and part-change kinds are fixed sets. Each is a closed enum with `match`, and each is matched only inside its owning module.
- Views vary by command. Each one renders its own text from its own serialized struct, with JSON-or-text chosen in one emit step.
- Where history comes from (a Git walk now, an optional cache later per §2.4) is decided only inside HistoryWalker.
- The live-only roster data is decided only in the daemon's `roster.live` handler.

Walk-through:
- **Main use case:** `roster seat foreman --json`. The CLI resolves HEAD to one `CommitId`; Scope parses and resolves `seat foreman` at that commit; Inventory loads the scoped teamspace, seat and clone tree, with occupant sessions; per active seat, `seat_parts` gives the manifest (a failed render makes that seat's clones `error`); per clone, LoadedInstructions reads the record, checks its `native_session_id` against the occupant's, and returns `Current`, `Stale` or `Unknown` plus the truncated flag; HistoryWalker makes one capped pass with LogQuery on the seat's Scope to fill `last_change`; the Roster struct is wrapped in the Envelope and emitted as JSON.
- **Likely future change:** the onboarding `--unmanaged` section (§5). It touches one new type, an unmanaged-objects list built from the snapshot, and one registration, a field the `roster.live` handler fills. Inventory, Scope, the CLI builder and the other views are untouched.
- **Second likely change:** a `.graph-local` log cache. It replaces internals of HistoryWalker only.

Per spec §2.5: `clone <c>` is a scope for `sessions` only. Scope's parser therefore takes an "allow clone" switch, and `roster` and `log` reject `Clone`. This is a deliberate mode flag, kept in the parser alone.

Per spec §2.3: only the hook writes the loaded record. `seat --instructions` and the daemon's `seat.resolve` stay pure reads, so a script reading `seat --json` cannot overwrite what a session was handed.

Per spec §2.1: `log` never reads the journal. Rejected and in-flight ops stay in `ops`, and `log` prints a pointer to it instead of merging them.

Digest:
- Scope: owns the scope grammar, name-or-id resolution and object/path membership at any commit's tree; new scope kinds are added as a variant here; callers never map scopes to folders or match names themselves.
- Inventory: owns the HEAD teamspace → seat → clone join with occupant sessions and transcript-path fallback; new per-clone joined facts are added here; views never enumerate records or join parents themselves.
- LoadedInstructions: owns `.graph-local/loaded/`, the session binding of the record and the current/stale/unknown/truncated rule; a new hook-based injection path calls its one record entry point and adds a source to its enum; non-injecting readers never write;
  callers never compare digests or session ids, or let a write failure surface.
- HistoryWalker: owns the newest-first commit walk, trailer parsing, the scan cap and last-touch-per-path; a history cache would go inside it;
  callers never use revwalk or parse trailers directly.
- LogQuery: owns change matching (scope paths at the commit's tree, affected objects, `--by` as an AND filter, the bookkeeping filter); new log filters are added here; `log` and roster `last_change` never filter changes themselves.
- View output: owns the `schema`/`head` envelope and the JSON-or-text choice; a new view supplies one struct and its text rendering; commands never hand-assemble the envelope.
- Roster: owns the roster shape for both the daemon-free and `--live` modes; live-only data is added to the overlay the daemon fills;
  the daemon never builds a second roster.

## Unchanged behavior

The amendment of 2026-11-08 (redesign cycle 1) must not change any of these:

- The text an agent receives from `seat --hook-prompt` (both harnesses), `seat --instructions` and `seat --json` `.instructions.text`: same render, same 9000-byte cap and truncation marker, same exit codes. The hook still never fails or blocks its session. One deliberate exception: on Codex the rewritten hook command needs a one-time re-trust in `/hooks` before Codex runs it again (§2.3 Honest limits), and setup says so.
- A harness that never closes the hook's stdin was already bounded only by the hook `timeout` (10 s) that setup writes: the base hook command (`<bin> seat --hook-prompt; <bin> session-report --from-hook claude`) blocked on it through `session-report`'s unbounded `read_to_string`. The §2.3 command (`p=$(cat); printf '%s' "$p" | <bin> seat --hook-prompt --stdin-payload; printf '%s' "$p" | <bin> session-report --from-hook claude`) moves that wait before the prompt prints but adds no new way to block, and the harness documents deliver the payload with EOF. `installed_hook_with_held_open_stdin_is_bounded_like_the_base_hook` pins both commands to the same behavior.
- `session-report --from-hook claude` still receives the full Claude SessionStart payload on stdin and reports the same `session.report` arguments, including from an older installed hook command that lacks `--stdin-payload`.
- `seat --instructions` and `seat --json` (`seat.resolve`) stay read-only: they write neither Git nor `.graph-local/loaded/`.
- No new Git writes and no new reload notice (§2.1, §4 decision 1). The loaded record stays local in `.graph-local/`, best-effort and daemon-free.
- `instructions show` resolution and errors, including its `Retired` error for retired-only matches.
- `undo --list` output and its undone rule.
- The default `log` hides `bookkeeping`; `--all` shows it (§4 decision 2).
- Harness-native files are listed by presence, size and mtime only; their content is never read (§4 decision 3).
- `roster --live` with the daemon down exits 5; `--unmanaged` only enumerates ids and labels (S1).
- Exit 6 for an integrity failure that leaves nothing to show, as `seat --instructions` maps it today.
