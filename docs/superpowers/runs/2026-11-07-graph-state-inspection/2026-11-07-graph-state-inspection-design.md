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

- none

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
| instructions | §2.3: `current`, `stale (N parts)`, `unknown`, plus a `truncated` flag |
| `reload_required` | the flag, when set |

**With `--live`** it makes one daemon call, `roster.live`. The daemon takes one Herdr snapshot and adds per clone:
- `agent: {kind, status}` from `pane.agent`;
- `live_pane_matches`: whether the snapshot still shows the committed binding.

It also adds the threads `seat_refusals` and `seat_lost` already held in memory (`src/daemon/compose.rs:441-511`). This is the only way to answer "is an agent running right now" rather than "did the observer last commit an occupant".

JSON:

```json
{"schema":1,"head":"<40-hex>","live":false,
 "teamspaces":[{"id":"ts_…","name":"proj","lifecycle":"active",
   "seats":[{"id":"st_…","name":"foreman","lifecycle":"active","manager":{"id":"st_…","name":"node-manager"},
     "is_node_manager":false,"reload_required":false,
     "clones":[{"id":"cl_…","name":"main","lifecycle":"active",
       "runtime":{"availability":"present","pane":"p7","tab":"t2","workspace":"w3","observed_at":"…"},
       "occupant":{"harness":"claude","native_session":"ns_…","since":"…"},
       "threads":{"seat":"sn…","state":"resolved","read_at":"…"},
       "instructions":{"status":"current","loaded_rev":"…","loaded_at":"…","source":"hook","truncated":false,"stale_parts":[]},
       "live":null}]}]}]}
```

**Cost:**
- Daemon-free mode reads every seat and clone record at HEAD: O(records), the same as `list`.
- The instruction status column needs one render per active seat (§2.3). That is the dominant cost. It is still only tree reads, roughly the cost of `seat --instructions` per seat.
- `--live` adds one Herdr snapshot.

### 2.3 Loaded instructions and staleness

**New local record:** `.graph-local/loaded/<clone-id>.json`. It is written atomically (temp file plus rename), best-effort, and never fails the caller.

```json
{"schema":1,"clone":"cl_…","seat":"st_…","rev":"<40-hex>","at":"…","source":"hook|hook-codex|seat-instructions|seat-json",
 "parts":[{"label":"global rules/style.md","path":"rules/style.md","digest":"sha256:…","bytes":812}],
 "omitted":["global rules/style.md"],"cut":null,"bytes":8740}
```

**Who writes it.** Every path that hands the render to an agent:
- `seat --hook-prompt` (both harnesses), after it prints;
- `seat --instructions`;
- the daemon's `seat.resolve` for `seat --json`.

The caller is resolved exactly as those paths already resolve it (`resolve_caller`). An unbound caller writes nothing.

**Digest choice.** `digest` is the SHA-256 of the rendered part body, not the Git blob id:
- Seat sections and `responsibility` are not files of their own; their `path` is `seat.toml` or `template.toml`.
- A blob id would mark them stale on any unrelated record edit.

**Status:**

| Status | Condition |
|---|---|
| `current` | every part at HEAD has the same label and digest as the record |
| `stale` | some part differs. `stale_parts` lists `{label, change: modified\|added\|removed}`, and the text shows the commit that last touched each changed path (one `git log -1` per changed path) |
| `unknown` | no record exists, or the record predates `occupant.since` — the record then belongs to an earlier session, so it says nothing about the current one |
| `truncated` (flag) | the hook omitted or cut parts. Reported separately, because "loaded" then means "loaded the specific half" |

**Honest limits.** The record says what graph **injected**, not what the model retained:
- A session that compacts gets a fresh hook injection, and the record updates.
- A session that ignored the hook, or whose hook failed, shows the last successful injection.
- Codex runs the hook only after the user trusts it.
- So `unknown` is a real state, not an error.

**New reader:** `herdr-graph instructions stack <seat> [--clone <c>] [--json]`. It is daemon-free and works for **any** seat, not just the caller's. It prints the manifest at HEAD:
- each part: label, path, bytes, digest, and the commit that last changed it;
- which parts the 9000-byte hook would keep;
- per clone, the loaded status from the record;
- **harness-native files present** in the seat's cwd and its ancestors (`CLAUDE.md`, `AGENTS.md`, `AGENTS.override.md`), listed by path with size and mtime, marked `not graph-managed`, and never read for content. JSON: `harness_files:[{path,bytes,mtime}]`.

Bodies are left out; `seat --instructions` and `show <path>` still print text.

```text
seat foreman (st_01…) at 1e01e11c4a2b — 6 parts, 7.9 KB, hook cap 9000: all kept
  1 global rules/style.md                     812 B  sha256:3f…  changed 5ad9cea 2026-11-06
  …
  6 seat AGENTS.md — teamspaces/proj/seats/foreman/AGENTS.md  2.1 KB  sha256:9a…  changed f6d32a8
harness files (not graph-managed):
  /home/u/src/proj/CLAUDE.md   3.4 KB  modified 2026-11-05 14:02
  /home/u/src/proj/AGENTS.md   1.1 KB  modified 2026-10-30 09:15
clones:
  main (cl_01…)  loaded 1e01e11c4a2b via hook at 10:42Z  current
  alt  (cl_02…)  loaded 5278a66f…    via hook at 09:10Z  stale: modified seat AGENTS.md (f6d32a8)
```

Implementation note: this refactors `seat_parts` into a manifest (label, path, body, digest) that both the renderer and the stack view consume. The text the agents receive does not change.

### 2.4 `herdr-graph log [<scope>] [--since <date|dur>] [--limit N] [--by <seat>] [--all] [--json]`

**Daemon-free.** It walks `main` from HEAD, newest first, reading only committed objects. That is safe while the writer runs, because Git objects are immutable.

**What it reads per commit:**
- the `Graph-Op`/`Graph-Action` trailers;
- the `OperationRecord` the commit added (requester, confirmation mode, kind);
- the `ActionRecord`, when present (`affected`);
- the changed paths (`diff-tree`).

**Scope membership.** A commit is in scope when any of these holds:
- a changed path lies under the scope's folder, resolved to IDs against **that commit's own tree**, so renames and moves are attributed correctly. The `mutations/graph-changes` file every commit adds is ignored for this purpose. The scopes are:
  - `global`: `graph.toml`, `rules/`, `templates/`, `applications/`, `threads-namespace.json`
  - `teamspace`: the teamspace folder, including its seats
  - `seat`: the seat folder, including its clones, plus transcripts whose path carries the seat ID
- `action.affected`/`retired` names an object in scope;
- `--by <seat>`: the requester is that seat. This is the "changes made by" view, as opposed to the default "changes to".

**Noise.** `bookkeeping` kinds (seat facts, transcript processing) are hidden unless `--all`. Observed changes (renames, moves, closures, occupancy) stay, because they are exactly the activity a user wants to see.

```text
2026-11-07 10:41Z  op_01…  seat_rename        foreman → lead                     by foreman/main (relay)
2026-11-07 09:58Z  op_01…  observed           session ended on main (AgentExited) observed
2026-11-06 22:10Z  op_01…  instructions_edit  seat AGENTS.md                     by node-manager (tty)  act_01… undone
```

JSON row: `{commit, at, op, kind, summary, requester:{teamspace,seat,clone,human}|null, confirmation:{mode,plan_hash}|null, action:{id,kind,undoable,undone}|null, objects:[{id,kind,name,change}], paths:[…]}`.

**Cost.** One commit read and one tree diff per scanned commit. `--limit` defaults to 30. A scoped query scans until it finds `limit` matches, or until `--max-scan`, default 5000 commits. When it stops early, it says so (`scanned 5000 commits, older history not searched`). No index is proposed. If long histories make it slow, a derived cache in `.graph-local/` can come later without changing the output.

**Journal side**, which is not in Git: `ops` gains `--seat <s>`, `--teamspace <t>`, `--limit N` and `--json`, and keeps going through the daemon. The filter is on requester. Rejected, failed and in-flight ops exist only there. `log` prints a one-line pointer to `ops --seat …` instead of reading the journal itself.

**Last change in the roster.** `roster` can carry each seat's `last_change {at, op, summary}` from the same walker in one pass, with the same scan cap. Seats not found within the cap show `last_change: older than scan`.

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

`current` is true when this session is the clone's `occupant`. The document carries `schema: 1` and `head`, like the other readers.

**Limit — purge discards session history at HEAD.** `clone purge` replaces the record with a tombstone that has `sessions: []` (`src/threads/assignment.rs:89-118`), or removes it entirely. After that:
- the sessions survive only in Git history and in the transcript index under `transcripts/`;
- `sessions` reads HEAD, so a purged clone's sessions are not listed;
- the output footer says how many tombstones are in scope.

Recovering them from history (`--history`, a Git walk like `log`) is possible but not proposed until someone needs it.

**Cost.** O(clone records in scope), the same as `list clones`, plus one transcript-record lookup per session that lacks a path. `--limit` defaults to 50 and `--since` filters on `started`.

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
2. **Loaded record and staleness.** The `.graph-local/loaded/` writer in the hook, `seat --instructions` and `seat.resolve`; `current`/`stale`/`unknown`/`truncated` in roster and stack.
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
