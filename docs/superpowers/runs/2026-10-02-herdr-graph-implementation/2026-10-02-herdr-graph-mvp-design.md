# herdr-graph MVP — implementation design

Date: 2026-10-02 · Run: `docs/superpowers/runs/2026-10-02-herdr-graph-implementation/` · Status: approved (root brainstorm, Mode A Q1–Q5 + sections; remainder decided under `/goal` autonomy)

Product contract (authoritative, not restated): [CURRENT-DESIGN.md](../../../../CURRENT-DESIGN.md), decision history [DESIGN-NOTES.md](../../../../DESIGN-NOTES.md), kickoff [IMPLEMENTATION-HANDOFF.md](../../../../IMPLEMENTATION-HANDOFF.md). Where this spec and the contract differ, the contract wins and the difference is a defect in this spec.

## Goal

A working herdr-graph MVP: a Rust Herdr plugin (`herdr-graph` binary + manifest + skills) that persists teamspace/seat/clone organization in a Git-backed graph-instance repository through one serialized, crash-recoverable writer; drives and observes Herdr (workspaces/tabs/panes/agents) to keep live runtime aligned with committed intent; supports plan → confirm → apply for organizational changes, templates with live applications, closure cascades and undo; integrates herdr-threads system channels and participation; tracks native sessions/transcripts and delivers transcript-processing requests to an ordinary summarizer seat — verified by unit, crash-injection and private-Herdr-server integration tests, and pushed to the private GitHub repo `alepar/herdr-graph`.

## Scope

**In the MVP tree** — every pillar of the contract, at the depth listed in each section below.

**Deferred (follow-on, outside this tree; recorded so nothing silently disappears):**
- Cross-tab clone *transfer* (MVP: detect move, mark `reload_required`, reload on `/seat`; no automatic scope transfer).
- Explicit application template *switch* between named copies (member-ID correspondence is stored and preserved on copy; the switch operation itself is deferred).
- Undo of template-content edits that conflict with later edits of the same field: MVP detects the conflict and emits a repair-required result (that *is* the contract's behaviour); no assisted repair-plan generator.
- Multi-pane restoration placement beyond: adopt caller pane for one clone + open remaining clones as new panes in the restored tab.
- Reminder cadence tuning; MVP reminds on a fixed schedule (see §3.7).
- Codex transcript discovery when Herdr reports no `agent_session` (MVP: Claude fully; Codex via `agent_session` or `pane process-info` argv `resume <id>` only; otherwise the transcript is recorded `unresolved`).
- Real herdr-threads service ACK-required delivery: depends on the requested threads amendment (§7.4). MVP ships the adapter contract + fake + a capability-detected fallback.

## 1. Architecture

**Two repositories.** This repo holds the plugin code. A *graph instance* is a separate Git repo (state, templates, rules, seat folders). `herdr-graph init <path>` creates one; plugin config (`$HERDR_PLUGIN_CONFIG_DIR/config.toml`, key `instance = "<abs path>"`, overridable by `HERDR_GRAPH_INSTANCE`) points at it.

**One crate, one binary `herdr-graph`** (Rust 2024, edition matching herdr-threads), three roles:
- `herdr-graph daemon` — resident process started idempotently by the manifest `[[startup]]` command (`herdr-graph daemon --ensure`). One daemon per (Herdr socket, instance) guarded by an `flock` owner lock in `<instance>/.graph-local/daemon.lock`. Hosts writer, reconciler, observer, transcript watcher, threads service connection.
- CLI subcommands (§10) — talk to the daemon over `<instance>/.graph-local/daemon.sock` (fallback `/private/tmp/herdr-graph-<uid>/<hash16>.sock` when the path ≥100 bytes), framing = 4-byte big-endian length + JSON body `{version:1, request_id, command:{kind, args}}` → `{version, request_id, result}`. Read-only commands may read the committed revision directly without the daemon.
- Plugin actions (`status`, `doctor`) via the manifest.

**Locate and ensure (every CLI entry point).** The instance is located by the first hit of: `HERDR_GRAPH_INSTANCE` → `instance` in `$("$HERDR_BIN_PATH" plugin config-dir herdr-graph)/config.toml` (works from any Herdr pane, graph-created or not) → `~/.config/herdr-graph/config.toml` (written by `init`). No instance ⇒ read commands print "no instance configured — run herdr-graph init" and exit 2. Mutating commands, `seat`, `undo` and `request …` connect to the daemon socket; if it is dead they run `daemon --ensure` (bounded 10s) and retry once, else fail with a clear error and the op is not admitted. `daemon --ensure` double-forks with `setsid` and returns once the socket answers (so it survives a one-shot `[[startup]]` hook and works when invoked by the CLI); with no instance configured it exits 0 without starting anything. The daemon **refuses to start without a reachable Herdr socket** (`HERDR_SOCKET_PATH` or the default session socket), and its lock/socket are keyed on (instance, Herdr socket path), so a CLI run outside Herdr can never leave a Herdr-less daemon holding the instance; outside Herdr, mutating commands fail with "daemon unavailable: not inside Herdr". `session-report` is a no-op when neither `HERDR_GRAPH_CLONE` nor a bound `HERDR_PANE_ID` resolves. A Herdr restart re-runs `[[startup]]`.

**Modules** (each external boundary behind a trait with a fake):

| Module | Owns |
|---|---|
| `model` | IDs, record schemas, (de)serialization, validation |
| `store` | committed-revision reads (git2), path layout, name→path slugging |
| `journal` | SQLite op/effect journal, op state machine |
| `writer` | single serialized worker: precondition recheck → tree build → CAS commit → working-tree fast-forward |
| `plan` | change requests → concrete plans, plan hashing, plan store, staleness |
| `herdr` | `HerdrApi` trait; NDJSON socket client (requests, `events.subscribe`, `session.snapshot`, `pane.process_info`) |
| `observe` | event stream + periodic snapshot → observations → observed mutations |
| `reconcile` | desired vs live diff → effects; effect journal; retries/backoff; fencing |
| `threads` | `ThreadsPort` trait; real impl over `herdr_threads::client::service::PersistentServiceClient` + `LocalSocketClient`; fake |
| `transcripts` | native-session tracking, transcript identity/boundaries, requests, coverage |
| `templates` | template/application model, effective desired state |
| `undo` | action log, candidates, previews, compensating mutations |
| `cli` | clap commands, TTY confirm, agent `--confirm` relay |

**Dependency on herdr-threads.** herdr-threads has no Git remote; the crate depends on `herdr-threads = { path = "third_party/herdr-threads" }` where `third_party/herdr-threads` is a committed symlink to `/Users/alepar/AleCode/herdr-threads`. README documents re-pointing it. Use only its public client API (`client::service`, `client::local`, `protocol::*`).

## 2. State model and files

### 2.1 IDs and names
- Durable IDs: prefixed ULIDs — `ts_` teamspace, `st_` seat, `cl_` clone, `ns_` native session record, `tpl_` template, `mem_` template member, `app_` application, `op_` operation, `ef_` effect, `act_` action, `pl_` plan, `tr_` transcript, `rq_` processing request.
- Display names are editable; renames record `{old, new, observed_at, event_at?, source}` in `name_history` (observed time always; event time only when Herdr supplied one — never invented).
- Folder slugs: lowercase, `[a-z0-9-]`, others → `-`, collapsed, max 48 chars; collision within the parent → append `-<last 6 of id>`. Paths are never cached as authority: `herdr-graph path` resolves the current path from the committed revision.

### 2.2 Lifecycle
- Teamspace, seat: `lifecycle = dormant | active | retired`. `dormant` = record exists, no runtime presence desired (never-activated seats start here). `active` = runtime presence desired (tab with ≥1 clone). `retired` = archived.
- Clone: `lifecycle = active | retired`.
- Runtime observation is separate: `runtime = { availability: present | absent | unknown, bound: {herdr_id, terminal_id?}, observed_at }`. Unknown/disconnected observations never change lifecycle.
- Occupancy: clone has `occupant: { native_session: ns_…, harness, since } | null`. Native-agent exit with pane retained clears occupancy only.
- Retired records stay in place, `lifecycle = retired`, `retired = {op, action, at, mechanism}` where `mechanism ∈ {user_cli, agent_request, observed_pane_close, observed_tab_close, observed_workspace_close, application_withdrawal, undo}`. Retired seats' folders move under `archive/` (teamspace-relative) so names are free for reuse; IDs keep working.

### 2.3 Repository layout (graph instance)

```
graph.toml                                  # instance id, schema_version, defaults
teamspaces/<ts-slug>/teamspace.toml
teamspaces/<ts-slug>/rules/*.md             # team rules (opaque)
teamspaces/<ts-slug>/seats/<seat-slug>/seat.toml
teamspaces/<ts-slug>/seats/<seat-slug>/AGENTS.md, notes/…      # opaque seat content
teamspaces/<ts-slug>/seats/<seat-slug>/clones/<clone-slug>/clone.toml (+ opaque clone files)
teamspaces/<ts-slug>/archive/seats/<seat-slug>-<id6>/…         # retired seats
archive/teamspaces/<ts-slug>-<id6>/…                           # retired teamspaces
templates/<tpl-slug>/template.toml, templates/<tpl-slug>/members/<member-slug>/AGENTS.md
applications/<app-id>.toml
rules/*.md                                  # global rules (opaque)
transcripts/<seat-id>/<tr-id>.toml          # transcript records + coverage
requests/<rq-id>.toml                       # processing requests (durable request/result)
actions/<yyyy-mm>/<act-id>.toml             # grouped action records (undo provenance)
operations/<yyyy-mm>/<op-id>.toml           # committed op summaries (history; attribution)
.graph-local/                               # gitignored: journal.sqlite3, daemon.sock/lock, plans/, threads-intents/
```

All records are TOML with `schema = <n>`, `id`, `rev` (u64, writer-incremented on every change to that file). Opaque files (Markdown, notes) carry no `rev`; preconditions use their blob hash.

### 2.4 Key record fields (normative minimum)
- `seat.toml`: `id, rev, name, name_history, teamspace, lifecycle, template_ref? {template, member}, applications [app ids], overrides {harness?, model?, args?, summaries?, instructions_sections?}, participation {seat_wide: [thread refs], }, summaries (bool, effective default true for ordinary seats; templates for system/cron/dispatcher roles set false), activation {last_op?}, runtime, reload_required?`.
- `clone.toml`: `id, rev, seat, name, lifecycle, runtime, occupant?, sessions [ns history: {ns, harness, native_session_id, transcript?, started, ended?, end_reason}], opt_outs [thread refs]`.
- `teamspace.toml`: `id, rev, name, name_history, lifecycle, runtime, project_repo?, channel {thread_id?}`.
- `template.toml`: `id, rev, name, defaults {harness, model, args, summaries}, members [{id: mem_…, name, role_ref, startup: active|deferred, defaults…}], relationships [{kind: thread_participation, thread: <name-pattern>, members: [mem ids]|all}], copied_from? {template, at}`.
- `applications/<id>.toml`: `id, rev, name, template, teamspace, lifecycle (active|retired), member_map {mem_id → seat_id}, additions [seat ids], exclusions [mem ids], reused [{seat, from}], contributions {relationships: [...]}, created_by {op, action}`.

Effective seat config = template member defaults ← template defaults ← seat overrides (overrides win; `instructions_sections` are named sections appended, never merged prose).

## 3. Mutation pipeline

### 3.1 Requests
Every write is a `ChangeRequest { kind, args, relied_on: [(object_id, rev | blob_hash)], requester: {teamspace?, seat?, clone?, native_session?, human?: bool}, supersedes?: op_id }`. Kinds are the organizational change set (§10) plus `content_write` (opaque file patch) and `observed` (from observer) and `bookkeeping` (transcript/request/runtime fields).

### 3.2 Which requests need confirmation
- **Organizational** kinds (create/activate/retire/resurrect/rename/clone add/remove, template edit, application hydrate/retire, override set, participation change, undo): `plan` → user confirmation → `apply`.
- **No confirmation** (approved baseline Q3): `observed` mutations (attributed, undoable via undo where eligible), `bookkeeping`, reconciliation/retries of a confirmed op's effects, `content_write` with preconditions, and summarizer occupant relaunch for an `active` summarizer seat (authority = the confirmed op that made it active; retired seats never relaunch).

### 3.3 Plan → confirm → apply
- `herdr-graph plan <kind> …` computes a `Plan {id: pl_…, request, committed_rev (commit oid), relied_on, effects: [concrete effects incl. per-instance template propagation, session replacements, threads changes, closures (incl. caller pane closure)], warnings}` and stores it in `.graph-local/plans/`. `plan_hash` = sha256 of canonical JSON of `{request, committed_rev-independent relied_on, effects}`.
- Human terminal (stdin is a TTY, no `--json`): print plan, prompt `[y/n]`, on `y` submit apply. There is no bypass flag. Non-TTY without `--json`: print the plan and exit without applying.
- Agent relay: agent runs `plan --json`, shows the plan to the user, and after an explicit user yes runs `herdr-graph apply <pl_id> --confirm <plan_hash> --confirmed-by user-relay`. Op record stores `confirmation {mode: tty|relay, plan_hash, at}`.
- At apply, the writer recomputes the plan's effects against the then-committed revision (including set-shaped reads such as "all applications of template T" and caller-pane bindings). **Apply executes only the confirmed effect set**: if the recomputed effects differ in any way from the confirmed effects → `stale_plan` rejection with a new plan id (user must confirm again), regardless of `relied_on`. If they are identical, unrelated commits in between do not invalidate (contract: unrelated activity does not invalidate approval). Plans predict induced container closures and show them as effects: retiring the last active clone of an active seat closes its tab and therefore (contract: a last-pane closure that removes the tab retires the seat) includes an induced **seat retirement** in the same plan, with a warning suggesting `seat deactivate` instead; closing a teamspace's last tab may close its workspace. **IDs minted at plan time** (`st_`, `cl_`, `app_`, `act_`, effect nonces) are reserved in the stored plan and reused at apply; op-scoped ids (`op_`, `ef_` attempt counters) are excluded from `plan_hash` and from the effect equality check.

### 3.4 Journal state machine
`admitted → applying → committed | rejected`, plus `cancelled` (only from `admitted`), `superseded` (committed op whose remaining effects were replaced). Admission = durable SQLite insert (WAL, `synchronous=FULL`) returning `op_id`; admission validates against committed state + queued projection where practical (best effort; the writer rechecks).

### 3.5 Writer
Single tokio task, strictly FIFO. Per op: load committed tree at `refs/heads/main` → recheck preconditions → apply mutation in-memory → write blobs/trees → commit with message `<kind>: <summary>` and trailers `Graph-Op: <op_id>` and `Graph-Action: <act_id>` (if grouped) → CAS update `refs/heads/main` (expected old oid) → mark `committed` with commit oid → fast-forward the working tree. The writer is the only process that moves `refs/heads/main`.

**The working tree is a derived, human-browsable view of `refs/heads/main`, never an authority.** The FF updates writer-owned files; a file with uncommitted local edits is left in place and recorded in `.graph-local/worktree_dirty` (surfaced by `doctor` and by a Notify to the owning seat's channel); never auto-merge. On folder moves (rename, archive) the writer moves tracked files; untracked/dirty leftovers under the old path are moved to `.graph-local/orphans/<op>/` so a reused slug never inherits them. The FF is re-run on daemon start after recovery (§3.6) and is idempotent. Agents read graph state through `herdr-graph show`/`path` (committed revision) or by reading working-tree files only after `herdr-graph seat`/`path` reports the view current (it reports `view_rev` and `worktree_dirty`). Writes never go through the working tree: `herdr-graph content write --object <id> --rel <relative path> --from <file> [--expect <blob>]` resolves the object's current folder at write time (archived folder included) and rejects targets that do not resolve to a graph object; writes into a **retired** object's folder are allowed only under `summaries/` (so summaries of a seat retired by the very closure that ended its session still land, in its archive folder).

### 3.6 Crash recovery
On daemon start: walk commits from `refs/heads/main` back to the journal's `checkpoint_commit`; collect `Graph-Op` trailers; ops in `applying`/`admitted` with a trailer → `committed`; without → re-queued (recheck again). Then advance `checkpoint_commit`, then re-run the working-tree fast-forward (§3.5).
- **Poison ops.** Each journal row has an `attempts` counter incremented when the writer starts applying it. An op that fails with a deterministic error (panic caught per op, validation bug) or reaches 3 attempts across restarts moves to terminal `failed {reason}` (surfaced by `doctor`, requester notified) and the FIFO proceeds; `cancel` is allowed on `admitted` and `failed` ops. Infrastructure errors (disk, git) are retried with bounded backoff; persistent ones halt the writer with a `doctor`-visible `writer_halted` state instead of spinning.
- **Git locks.** On daemon start, while holding the daemon flock (so no other graph writer exists), stale `refs/heads/main.lock` and `index.lock` in the instance repo are removed. Lock errors during a CAS/FF are retried (bounded, 5 × 200ms); exhaustion → op stays `applying` and the writer halts as above. Tier-2 failpoint inside the ref transaction. Crash-injection points (test-support failpoints): after admission, after tree build, after CAS before journal update, after journal update before worktree FF, mid effect execution.

### 3.7 Failures, cancellation, supersession, reminders
- Precondition/semantic failure → `rejected {reason, explanation, current_revs}`; notify requester through its seat system channel (threads Notify) and record in op. If the requester never submits a replacement (`supersedes` link), daemon re-notifies at 1h, 6h, 24h then stops; replacement or explicit `cancel` stops reminders.
- `herdr-graph cancel <op>`: only `admitted` ops; post-commit cancellation requires a superseding request specifying replacement desired state (e.g. `seat set-lifecycle dormant`), which marks the old op `superseded`. No hidden suppression flags.
- `herdr-graph reassign <op> --to <seat>` changes responsible requester; ops of retired requesters stay discoverable via `herdr-graph ops --unresolved`.
- Delayed instructions (e.g. whole-seat leave to other clones) carry `{op_id, object_id, rev}`; the recipient's skill calls `herdr-graph check-instruction <op> <object> <rev>` which answers `current|obsolete` before acting.

## 4. Herdr runtime: binding, observation, reconciliation

### 4.1 Herdr client and harness launch profiles
NDJSON over `$HERDR_SOCKET_PATH`: methods used — `session.snapshot`, `events.subscribe`, `workspace.create/rename/close`, `tab.create/rename/close`, `pane.split/close/process_info/get/current`, `pane.report_metadata`, `agent.start`, `agent.get`, `agent.send_keys` (exact names verified against `herdr api schema --json` of the installed 0.9.1). Env for launched panes is passed via `env` on workspace/tab/pane creation (Herdr `agent.start` has no env): `HERDR_GRAPH=1`, `HERDR_GRAPH_INSTANCE`, `HERDR_GRAPH_SEAT=<st_id>`, `HERDR_GRAPH_CLONE=<cl_id>`. Graph never relies on reading env back (Herdr exposes no pane env); identity comes from the token in §4.2.

**cwd.** Every graph-created workspace/tab/pane is created with an explicit `cwd`: the seat's effective `cwd` override, else the teamspace `project_repo` (+ worktree) when set, else `<instance>/.graph-local/cwd/<seat-id>` (a stable, id-named directory that never moves on rename). The cwd used is recorded on the `ns_` record; resume and transcript lookup use the recorded cwd, never the seat folder.

**Harness launch profile table** (owned by the seam contract hg-zmi.1; §4.5 and §8.1 refer to it and restate nothing):

| harness | agent.start kind | launch args | resume args | model arg | exit sequence | session-id source | transcript locator |
|---|---|---|---|---|---|---|---|
| `shell` | — (no agent; tests) | — | — | — | — | — | — |
| `claude` | `claude` | `[]` | `--resume <id>` | `--model <m>` | `agent.send_keys` `ctrl+c`, `ctrl+c`; if still running, submit `/exit` | graph SessionStart hook report (§8.1), else Herdr `agent_session` (kind id\|path) | path-kind `agent_session` if present; else `${CLAUDE_CONFIG_DIR:-~/.claude}/projects/<slug(cwd)>/<id>.jsonl` with `slug` = every char not `[A-Za-z0-9]` → `-`; else glob `projects/*/<id>.jsonl` |
| `codex` | `codex` | `--no-daemon` | `resume <id>` | `-m <m>` | `agent.send_keys` `ctrl+c`, `ctrl+c` | Herdr `agent_session`, else `pane.process_info` argv `resume <id>` | hook-reported `transcript_path` if any, else `unresolved` (deferral) |

**Readiness and start outcomes.** `start_agent` runs only in a pane whose snapshot shows no agent and whose `pane.process_info` foreground process is the shell. `agent.start` outcomes: success → occupant recorded; `agent_not_ready` (blocked during startup, e.g. trust/auth dialog) → the blocked agent is adopted as occupant and the effect becomes `blocked_needs_human` with a Notify to the seat channel; timeout → `unknown` outcome (inspect snapshot before any retry); a non-shell foreground → `needs_revision`. `start_agent` is never blindly retried.

### 4.2 Binding and identity
- **Panes** (clones) are stamped right after creation with a graph-owned token `hg=<cl_id>` via `pane.report_metadata`; **workspaces** (teamspaces) via `workspace.report_metadata` (`hg=<ts_id>`). Herdr 0.9.1 has no tab metadata, so **tabs have no token**: a seat's tab is identified by `tab_id` within an incarnation and, across incarnations, as the tab that contains the seat's token-matched panes. A seat tab containing no token-bearing panes after a rebind is `unknown`.
- **Creation-atomic nonce.** Because create calls return the id needed for stamping, every create carries a creation-atomic nonce: the effect id is embedded in the create's `cwd`-independent label (`<name> ·<ef6>`, the 6-char effect-id suffix, renamed to the plain name once stamped and bound). A create whose response was lost (timeout, crash between create and stamp — a tier-2 failpoint) is recovered by looking for that nonce label in the next snapshot before any retry; found ⇒ adopt + stamp, not found ⇒ retry.
- Binding stores `{token, pane_id, terminal_id, tab_id, workspace_id, incarnation}`. Matching order: token → terminal_id (any incarnation, accepted only when the pane's cwd and, if present, agent session also match the record; this keeps bindings across a Herdr live handoff) → the clone's current native session id. A binding that cannot be matched is `unknown` → availability `unknown`, never retirement; `/seat` in such a pane proposes a rebind plan.
- Herdr session restore does not restore creation-time env or (possibly — spike) tokens, so nothing downstream depends on `HERDR_GRAPH*` env being present in a restored pane: the SessionStart hook and `session-report` resolve the clone through `HERDR_PANE_ID` → binding when `HERDR_GRAPH_CLONE` is absent, and graph re-stamps tokens on rebind.

### 4.3 Observation — level-triggered structural differ
1. **Triggers and baseline.** Herdr events (`events.subscribe` to `workspace.*`, `tab.*`, `pane.*`, `pane.agent_status_changed`), the 60s tick (configurable) and reconnects only *trigger* a fresh complete `session.snapshot`; no event is mapped to a mutation directly. The last complete snapshot is persisted in `.graph-local/baseline.json` with its **incarnation** (subscribe connection generation + Herdr server pid and process start time from `ps`, where obtainable). A diff runs only between two complete snapshots of the same incarnation.
2. **Rebind pass.** The first complete snapshot after daemon start, reconnect, or an incarnation change is a rebind pass: identity matching (§4.2), then, for every matched object, a diff of label and occupant against **committed state** (not the baseline), emitting observed renames and occupancy ends/starts with `observed_at` — so a rename or agent exit during a gap is recorded, not reverted, and §7.1 cleanup and §8.2 requests still fire. Agents that Herdr itself resumes after a server restart (`resume_agents_on_restore`) are observed as occupancy starts and adopted; graph waits a 90s grace after an incarnation change before any relaunch `start_agent`, and re-checks the pane is still an idle shell. Matched objects update bindings (bookkeeping). Unmatched bound objects become `unknown`. Objects closed while the daemon was down therefore surface as `unknown` (visible in `doctor` and `/seat`), never as silent retirement and never as recreation: the reconciler emits **no create effects for `unknown` objects** until a rebind or an explicit plan (`seat activate`, `undo`, `rebind`).
3. **Diff semantics** (same-incarnation baseline → new snapshot, matched by token/terminal_id): a known token under a new pane_id/tab/workspace ⇒ **move**; a changed label ⇒ **rename** (name_history records observed time only); agent gone with pane present, or native session id changed ⇒ **occupancy end** (+ new occupancy if a new session appears); a token not present ⇒ **disappearance**. Dropped events are harmless because everything comes from the diff.
4. **Containment grouping.** Disappearances in one diff are grouped: workspace gone ⇒ workspace rule (retire teamspace + all non-retired seats incl. dormant + clones); tab gone (workspace present) ⇒ tab rule (retire seat + remaining clones), absorbing its panes; pane gone with its tab present ⇒ clone rule (retire clone). A tab emptied because its last pane moved elsewhere ⇒ the moved clone is handled as a move (`reload_required` on the clone); the seat keeps `lifecycle = active` with `runtime.availability = absent` and flag `moved_out`, and the reconciler emits **no create_tab for a seat whose active clones are all bound elsewhere** (the user moved them deliberately; `/seat` in the moved pane offers a rebind or move-back plan). A move into a tab/workspace graph does not know leaves the clone `unbound` with a rebind proposal. Each group is one cascade action recording `retired` and `already_retired` (for undo).
5. **Classification against intent, not time.** Before emitting an observed mutation, each diff element is checked against committed desired state plus journaled effects with their predicted end state (including induced container closures predicted by plans, §3.3). An element explained by intent (e.g. the reconciler closed the tab of a seat being deactivated or retired; Herdr removed the tab after a confirmed plan that retired the seat's last clone — which, per the contract's last-pane rule, that plan shows as an induced **seat retirement** effect) produces only bookkeeping (binding/runtime update), never a second retirement. **Predicted end states are single-use and short-lived**: a prediction is consumed by the first diff that shows it, and is discarded if the first complete snapshot after its effect completes still shows the container present — so a prediction Herdr never performed cannot later absorb a real user closure. Seat `deactivate` leaves clones `active` with runtime absent — explained, neither a create nor a retirement.
7. **Baseline advance.** `baseline.json` advances only after that diff's observed mutations are committed. `observed` requests carry no rev preconditions on the objects they retire/rename (they record facts); if one is nevertheless rejected (e.g. the object was retired meanwhile), the next diff re-derives it against the fresh revision rather than losing it.
6. **Ordering.** Observer and reconciler share one loop step: snapshot → diff → commit observed mutations (writer) → reconcile against the new committed revision and the same snapshot. The reconciler never acts on a snapshot whose observations are uncommitted.

### 4.4 Reconciliation
- Runs as the second half of the loop step (§4.3.6); additionally on op committed (which triggers a fresh snapshot first) and backoff timers.
- Diff desired (committed revision) vs live (that step's snapshot) → effects: `create_workspace`, `create_tab`, `split_pane`, `start_agent`, `rename_*`, `close_*`, threads effects (§7), summarizer request delivery (§8). `unknown` objects get no create effects (§4.3.2).
- Effect identity `ef_ = hash(op_id, object_id, effect_kind, object_rev)` with predicted end state. Before executing, the reconciler re-reads the committed revision: if the object's `rev` advanced and the effect is no longer implied → mark effect `obsolete` (history kept). This enforces latest-intent and prevents old creations reactivating retired seats.
- Transient errors (socket unavailable, timeouts) → retry with exponential backoff 1s→5min, jittered. Unknown outcome of a non-idempotent effect (e.g. create_tab timeout) → take a snapshot and look for the expected object **by its token** before retrying; adopt it if found.
- Failures requiring revision (name conflict, invalid binding, start precondition) → op/effect `needs_revision`, requester notified (§3.7).
- Ordering: dependent effects wait for prerequisites (tab before pane before agent); independent effects proceed.

### 4.5 Session replacements
When an effective harness/model/args change applies to an active clone with an occupant, the plan lists `replace_session {clone, from, to, resume: yes|no}`. Applying: wait for agent status `idle`/`done` (not `working`/`blocked`; up to 10 min, else `needs_revision` "occupant busy", requester notified — no force), send the harness's exit sequence (§4.1 table), wait for the pane's foreground to return to the shell (timeout 30s → `needs_revision` "occupant did not exit", **no new agent started**, requester notified), then start the new agent per the profile table with resume args when the harness supports resume, else fresh. History preserved in `sessions`.

## 5. Templates and applications
- Templates are live: an application references its template by id; effective desired structure = template members (minus exclusions) mapped through `member_map`, plus additions. Editing a template (`herdr-graph template edit` → plan) lists every affected application/instance and every resulting seat creation/session replacement; apply propagates immediately via reconciliation.
- Hydrate (`herdr-graph apply-template <tpl> --teamspace <ts> --name <app-name> [--reuse mem=seat]…`): creates the application record, creates seats (dormant or active per member `startup`), records reuse in each participating application's `reused`, creates relationship contributions (thread participation), and an action record `{kind: hydrate, created: [...], reused: [...], relationships_added: [...]}`.
- Member added to template later → new seat created for each active application not excluding it (recorded as contribution of that application). Member removed from template → for each application: seats still *exclusive* to that application (not in another application's map/reuse, not added independently) → retired with mechanism `application_withdrawal`; shared seats kept, contribution removed. Member re-added with same `mem_` id → existing seat (if retired by withdrawal, not by local exclusion) is resurrected and remapped; excluded members stay excluded.
- Retiring a seat that maps a template member records `exclusions += mem` for that seat's application(s).
- Copy (`template copy <tpl> <new-name>`) preserves member ids.
- Retire application: marks `lifecycle=retired`, withdraws contributions; seats exclusive to it (including those created by later template edits) are retired; reused/pre-existing/independently-added seats and relationships still contributed elsewhere are kept; preview lists both.
- Ambiguity (e.g. a seat exclusive to the app has been given overrides referencing another application, or is bound to an active occupant working on relationships of another application) → plan carries `repair_required` and refuses to apply until a narrower request is submitted.

## 6. Undo
- `herdr-graph undo` (TTY): lists recent undoable actions newest first (`act_` records: closure cascades, retirements, hydrations, template edits, application retirements), with already-undone ones marked. Select by number → concrete preview → `[y/n]`.
- Closure/retirement undo: resurrect exactly `retired` ids (not `already_retired`); previously dormant seats stay dormant; restored active seats/clones get new tabs/panes. If the restore includes runtime presence and the command runs in a Herdr pane, the caller pane is adopted as one restored clone (preview names it); if the caller pane has a graph binding, preview shows it and its replacement; the displaced clone is retired (`mechanism: undo`) with history archived; if displacement affects another clone with an active occupant → `repair_required`. Remaining clones open as new panes in the restored tab; occupants relaunch (resume where the recorded harness/session supports it).
- Hydration undo = retire that application (§5 rule) using current membership.
- Template edit undo = inverse patch of the edit's changed fields; if any changed field has a later edit or conflicting override → `repair_required` explanation (no guess).
- If the undo retires the caller's own pane: preview says so; op is admitted and its id printed before the pane closes.
- Undo creates new compensating ops (`kind: undo`, `undoes: act_…`); original history untouched.

## 7. Threads integration
### 7.1 Channels
- Each active seat gets a managed public seat channel, each teamspace a managed public teamspace channel: `EnsureThread` with stable `OperationId` derived from object id; thread topic = `<teamspace>/<seat>` (renames → `SetTopic`). Thread IDs stored on the record.
- Every clone with an occupant (a threads native seat bound to its pane) receives `Invite{constraint: Required}` to its seat channel and teamspace channel. The agent must explicitly `herdr-threads accept-required …` (never fabricated); graph records invitation state `pending|accepted|released|retired` from `Membership` queries.
- Participation intent from templates/requests: seat-wide thread participation stored on seat; clone opt-outs on clone. New clones are invited to seat-wide threads only (Ordinary invites for non-system threads). Whole-seat leave: remove intent, Notify the seat channel with a delayed instruction `{op, seat, rev}` telling other clones to leave themselves; completion tracked via Membership. System channels reject leave (threads enforces).
- Lifecycle cleanup is keyed to occupancy, not only retirement: when the differ (§4.3) records an **occupancy end** (agent exited, pane kept), graph issues `ReleaseRequirement` for that occupant's required memberships; on **occupant changed/started**, graph re-invites the new occupant's threads seat (Required for system channels, Ordinary for seat-wide participation); retiring a clone releases as well (threads itself retires participants on verified pane closure). Messages and attribution are preserved.
- Service connection: one `PersistentServiceClient` per daemon, intents dir `.graph-local/threads-intents/`; on `ServiceBusy` or disconnect → effects retry with backoff; never take over another registration.

### 7.2 Notifications
Renames, path changes, op rejections/reminders, reload-required → `Notify{severity: Info|Warn}` on the affected seat channel(s) (teamspace rename → every seat channel in it).

### 7.3 Threads mapping for attribution
Graph exposes `herdr-graph who <pane|threads-seat>` mapping threads native seats (pane-bound) to teamspace/seat/clone/native session (graph owns the mapping).

### 7.4 Summarizer delivery capability
`ThreadsPort::send_request(thread, recipients, body, op_key) -> MessageId` and `receipt_state(message_ids)`. Implementations:
1. `ServiceAckDelivery` — the threads amendment, **accepted 2026-10-02** as herdr-threads epic `ht-5nb` (.1 contract, .2 store send, .3 service reads + capability routing, .4 e2e), queued behind threads epic `ht-p03`; no schema change. Agreed shapes (`src/protocol/service.rs`): register capability `service_session_v2` (a v1 session asking new ops gets `Unsupported`, connection kept); `ServiceOperation::Send(ServiceSend{thread, body 1..=64KiB, recipients: Vec<SeatId> ≤100 no dups, deadline_millis: Option<u64>, operation: OperationId})` journaled exactly-once → `ServiceResult::MessageSent(ServiceMessageSent{summary: MessageSummary, author, recipient_count, receipt_duration_millis})`, obligation key `(message_id, seat_id)`; `ServiceOperation::History(ServiceHistoryQuery{thread, page, initial})` → `Page<MessageSummary>`; `ServiceOperation::Receipts(ServiceReceiptsQuery{message, page})` → `DeliveryInspection` with per-recipient `Pending | Acknowledged(decided_at) | Retired`. Semantics: every *joined* member of the thread gets an obligation plus explicit recipients (a pending required invitee is valid) → graph sends summarizer requests on the summarizer seat's managed seat channel, whose joined set is exactly that seat's clone(s); send only to service-managed threads (`IncompatibleOwnership`); zero obligations → `InvalidRequest`; `Conflict` → nothing published, resubmit with a new key; key reuse with different payload → `OperationPayloadMismatch`. Service never ACKs and is never a recipient. Interim: History/DeliveryInspect/Recipients/Message are claimless reads via `LocalSocketClient` today. Compiled behind cargo feature `threads-service-ack` (default off) until ht-5nb lands; runtime capability probe = successful `service_session_v2` registration. Full note: threads scratchpad `graph-amendment-design.md` (copied to the run dir as `threads-amendment-reply.md`).
2. `NotifyFallback` (default until the amendment lands): the request record in graph is the durable queue; delivery = `Notify{Warn}` on the summarizer seat channel with the request id; the summarizer ACKs dispatch through `herdr-graph request ack <rq>` (records `dispatched_at`, never success). Labeled in docs/status as *fallback: ACK lives in graph, not threads*.
3. Fake (tests).

## 8. Transcripts and the summarizer
### 8.1 Native sessions and transcripts
- Session identity comes, in order, from: (a) graph's own Claude SessionStart hook (installed by `herdr-graph setup claude`, §9), which runs `herdr-graph session-report` with the hook payload's `session_id`, `transcript_path`, `source` and `cwd` plus `HERDR_PANE_ID`/`HERDR_GRAPH_CLONE` — this works whether or not Herdr's own Claude integration is installed; (b) Herdr `agent_session` from the snapshot (kind id|path); (c) `pane.process_info` argv per the §4.1 profile table. The transcript path is resolved by the profile table's locator; existence is verified. Records `ns_` on the clone with harness, native id, transcript path, recorded cwd, start. A new session id on the same clone (e.g. `/clear`) ends the previous `ns_`.
- Transcript record `tr_`: `{transcript path, native_session, seat, clone, source_seat_summaries_enabled_at_capture, coverage: [ranges], gaps: [ranges], unresolved?: reason}`. Range unit = byte offsets into the JSONL file, half-open `[start,end)`, end aligned to a newline.

### 8.2 Requests
- When a session ends (occupancy ended, pane closed, `/clear` session change, or resume appending after the last boundary), and the source seat's effective `summaries = true`, graph creates `rq_` = `{transcript, range: [covered_end, current_size), status: pending, created_by_op, delivery: {message_id?, dispatched_at?}, result?: {output_ref, covered_range, reported_by, at}}`. Dedup key = `(tr, range)`; overlapping pending requests are merged before delivery.
- If the source seat has `summaries = false` → no request (system/cron/dispatcher). Pending requests created while enabled stay pending after disablement (decision: disabling affects new captures only).
- Missing/unreadable transcript at request time → request `unresolved {reason: missing_input}` (visible to the scan, never "done").
- Destination (deterministic): the teamspace's oldest non-retired seat with effective `role = summarizer` (ties by id), else `graph.toml summarizer_seat`. Retired summarizer → requests stay pending, no activation (retirement precedence). Active summarizer with an active clone but no occupant → reconciler relaunches that clone's occupant (authority §3.2), then delivers. Active summarizer with zero active clones, or no destination at all → request stays pending with `undeliverable: <reason>`, flagged by `doctor` and Notified once to the teamspace channel.
- **Daemon-owned liveness.** The daemon (not the summarizer's session) scans requests every 10 min: pending + undelivered → deliver; delivered but not ACKed after 30 min → a reminder `Notify{Warn}` referencing the original message id / request id (the service-ACK Send is exactly-once, so it is never re-sent under the same key); ACKed (dispatched) but not completed after 6 h → a new delivery marked `retry` under a **fresh op key**, recording the new message id on the request; `unresolved` stays visible. The summarizer role's `/loop 1h` scan is a convenience, not the recovery mechanism.
- Results: `herdr-graph request complete <rq> --output <path-or-url> --covered <start>-<end>` (bookkeeping op). Coverage merge is order-independent: covered ranges union; gaps = requested ranges not covered; a late result for an older range never shrinks newer coverage. `herdr-graph request list --pending|--unresolved|--undispatched` for the leftover scan.

### 8.3 Shipped summarizer template
Template `system-summarizer`: one member `summarizer` (startup active, `summaries=false`, harness claude). Its AGENTS.md: wait for thread requests; for each, ACK (dispatch) then dispatch a subagent that reads the transcript range, writes a summary under the source seat's folder via `herdr-graph content write --object <source seat id> --rel summaries/<tr>-<start>-<end>.md` (resolved at write time), then reports `request complete`; optionally `/loop 1h` to call `request list --pending --undispatched` and `--unresolved` and re-dispatch leftovers (the daemon's liveness scan, §8.2, is the actual recovery).

## 9. Bootstrap, skills, Beads convention
- Session-start: graph-launched panes carry `HERDR_GRAPH=1`. Shipped Claude SessionStart hook snippet (installed by `herdr-graph setup claude`, owned-group style like threads) injects "run `/seat`" when `HERDR_GRAPH=1` **or** `HERDR_PANE_ID` resolves to a bound clone (restored panes lose env, §4.2); it also runs `session-report`.
- `/seat` skill (`skills/seat/SKILL.md`): runs `herdr-graph seat --json` which resolves the caller pane → clone/seat/teamspace (or returns `unbound`/`ambiguous` with candidates and a proposed create/resurrect/rebind plan); prints paths of applicable templates, rules (global/team/seat), seat AGENTS.md, clone files, pending graph ops, pending invitations with the exact `herdr-threads accept-required` commands, `reload_required`. The agent reads those files itself (no semantic brief).
- `graph` skill (`skills/graph/SKILL.md`): how to request organizational changes (plan → show user → relay confirm), content writes, ops/cancel/reassign, check-instruction, Beads label convention.
- `undo` usage documented in README + skill.
- Beads convention: labels `hg-ts:<ts_id>`, `hg-seat:<st_id>`, `hg-clone:<cl_id>`, optional `hg-ns:<ns_id>`; `herdr-graph seat --json` prints the `bd list --label …` query for inherited work. Graph never calls `bd`.
- Example templates shipped (configurable examples, not hardcoded hierarchy): `system-summarizer`, `project-team` (foreman active + researcher deferred), `feature-team` (engineer + reviewer deferred).

## 10. CLI surface (MVP)
```
herdr-graph init <path> | daemon [--ensure] | status | doctor | setup claude
herdr-graph seat [--json]                      # bootstrap/resolve caller
herdr-graph show <id|path> | list [teamspaces|seats|clones|applications|templates] | path <id>
herdr-graph plan <change…> [--json]  /  apply <pl_id> [--confirm <hash> --confirmed-by user-relay]
  changes: teamspace create|rename|retire|resurrect, seat create|activate|deactivate|rename|retire|resurrect|override,
           clone add|retire, participation join|leave (--scope seat|clone),
           template create|edit|copy, application apply|retire
herdr-graph content write --object <id> --rel <relative path> --from <file> [--expect <blob>]
herdr-graph session-report --from-hook claude            # called by graph's SessionStart hook (stdin = hook payload)
herdr-graph rebind <clone> --pane <pane>                  # organizational: plan → confirm
herdr-graph ops [--unresolved] | op <id> | cancel <op> | reassign <op> --to <seat> | check-instruction <op> <obj> <rev>
herdr-graph undo [--list|--json]
herdr-graph request list|ack|complete …        # transcript requests
herdr-graph who <pane>
```
Human-direct commands (`plan` without `--json` on a TTY) prompt `[y/n]` and apply on `y`.

## 11. Testing and validation
Tiers, each test labelled by tier in its name/module so the report can separate verified integration from mocks:
1. **Unit/property** (fakes): schemas round-trip; slugging/collisions; effective config precedence; coverage/gap merging under arbitrary result orders (proptest); template live propagation, exclusivity withdrawal, re-addition identity; plan hashing & staleness rules; undo candidate/preview computation incl. `already_retired`.
2. **Concurrency/crash** (fakes + real git): concurrent stale mutations (two renames same rev → one rejected; disjoint seats → both commit); readers during writes always see complete revisions (reader thread asserting invariants across many commits); failpoint crash at each writer boundary → restart → no lost/duplicate ops; cancellation vs commit race; obsolete effect fencing (retire after create queued → no tab created); crash between create and token stamp → nonce adoption, no duplicate; poison op → `failed` after 3 attempts, FIFO continues; stale git lock files removed on start, lock contention retried; observed retirement racing a confirmed op on the same seat → re-derived, not lost; predicted workspace close not performed → a later user close still retires the teamspace; plan-time ids reused at apply (create plan not perpetually stale).
3. **Private-Herdr integration**: spawn a private `herdr server` (isolated HOME/XDG/HERDR_SOCKET_PATH/HERDR_CONFIG_PATH under `/private/tmp/hg-<rand>`, never `herdr server stop`, SIGTERM own pid only — the herdr-threads `private_host.py` pattern ported to Rust test support) with plain shell panes only: Herdr's agent detection needs real harness binaries, so tier 3 configures seats with harness `shell` (a graph-internal harness that runs no agent; `start_agent` is a no-op recorded effect) and asserts workspace/tab/pane lifecycle, env and observation; `agent.start` itself is exercised in tier 4. Flows: create teamspace+seats → tabs/panes appear with env; rename tab → seat renamed; close pane/tab/workspace → cascades; undo tab close → restored; template edit adds member → new tab; reconnect after event-buffer loss → snapshot reconcile; Herdr private-server restart → rebind pass, no mass retirement, no duplicate tabs; close a tab while the daemon is down → `unknown` on restart, no recreation; retire a seat's last clone via plan → plan shows induced seat retirement; after apply the tab is gone, seat retired exactly once; seat deactivate → tab closed, seat dormant, no retirement; move a seat's only pane to another tab → clone reload_required, seat active+absent+moved_out, no tab recreated; dropped rename event → rename recovered from snapshot diff; rename a tab while the daemon is down → recorded as observed rename on restart, not reverted; close a tab whose seat has summaries=true → summary lands in the archived seat folder; restored panes after Herdr restart (no HERDR_GRAPH env) still resolve via `/seat` and `session-report`.
4. **Real-agent smoke** (opt-in env `HG_REAL_AGENTS=1`): real Claude in private server: `/seat` resolves identity; summarizer request delivered (fallback path) → `request ack` → `request complete`.
5. **Threads**: fake for all logic; real-daemon integration test (opt-in `HG_REAL_THREADS=1`) against current threads protocol for EnsureThread/Invite(Required)/Membership/Notify/ReleaseRequirement; service ACK path only when the amendment is present.
Never touch the user's live Herdr session or memory observer in tests.

## 12. Packaging and delivery
- `herdr-plugin.toml`: id `herdr-graph`, `min_herdr_version = "0.9.1"`, `platforms = ["macos"]`, `[[build]] scripts/build.sh` (cargo build --release, copy binary into plugin `bin/`), `[[startup]] ["bin/herdr-graph","daemon","--ensure"]`, `[[actions]] status, doctor`.
- README: install (`herdr plugin link`), init instance, setup, `/seat`, plan/confirm, undo, threads amendment status, verified-vs-assumed matrix.
- `just`/`cargo` commands: `cargo test` (tiers 1–2 default), `cargo test --features private-herdr` (tier 3), env-gated tiers 4–5.
- Repository pushed to private GitHub `alepar/herdr-graph` (design docs, research and run artifacts included). This is post-merge delivery done at the run's finish step, not a task in the tree (a tree stops at merge-ready).

## Post-Implementation Notes

**2026-10-03 — Changes vs. original design.** Attention notices prefer the requester's channel when available; for seatless requesters or missing requester channels, use the affected seat's channel (a clone resolves to its owning seat), or the affected teamspace channel for teamspace effects. The user selected durable pending delivery while channels are unavailable.

Failed operation finalization halts the writer and preserves that halt in memory if its journal write also fails; operator resume/restart recovery requeues the applying operation. Working-tree dirty markers represent unresolved edits and retain their per-op identities; each fast-forward prunes resolved/orphaned entries and fails without overwriting an unreadable or malformed marker file.

Integration assertions wait for the derived working-tree revision and restored pane token, since committed state becomes visible before those side effects finish. Rust sources and tests were formatted separately from behavioral fixes.

The recovery sweep also found that a workspace created before the event subscription could retain a Done creation effect and an unstamped nonce label while its temporary ID belonged to the old generation. `LiveIndex` now correlates durable creation nonces across generations before considering another create, restricted to the same graph object and unique unclaimed workspace. This also covers daemon crash between creation and stamping. Regression tests reject reused IDs without the nonce, foreign graph tokens, and ambiguous matches.

*As this design is implemented and iterated on — bug fixes, adjustments, anything that diverged from the assumptions above — append a dated note here, whether or not a formal debugging skill was used.*
