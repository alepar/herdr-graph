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
- Human terminal (stdin is a TTY and `--yes` absent): print plan, prompt `[y/n]`, on `y` submit apply.
- Agent relay: agent runs `plan --json`, shows the plan to the user, and after an explicit user yes runs `herdr-graph apply <pl_id> --confirm <plan_hash> --confirmed-by user-relay`. Op record stores `confirmation {mode: tty|relay, plan_hash, at}`.
- At apply, the writer recomputes the plan against the then-committed revision: if relied-on revisions changed AND the recomputed effects differ from the confirmed effects → `stale_plan` rejection with the new plan id (user must confirm again). Unrelated commits do not invalidate (contract: unrelated activity does not invalidate approval).

### 3.4 Journal state machine
`admitted → applying → committed | rejected`, plus `cancelled` (only from `admitted`), `superseded` (committed op whose remaining effects were replaced). Admission = durable SQLite insert (WAL, `synchronous=FULL`) returning `op_id`; admission validates against committed state + queued projection where practical (best effort; the writer rechecks).

### 3.5 Writer
Single tokio task, strictly FIFO. Per op: load committed tree at `refs/heads/main` → recheck preconditions → apply mutation in-memory → write blobs/trees → commit with message `<kind>: <summary>` and trailers `Graph-Op: <op_id>` and `Graph-Action: <act_id>` (if grouped) → CAS update `refs/heads/main` (expected old oid) → mark `committed` with commit oid → fast-forward the working tree (`checkout` with `force` only for files the writer owns; if the working tree has uncommitted edits to a file the commit touches, leave that file, emit a `worktree_dirty` warning; never auto-merge). The writer is the only process that moves `refs/heads/main`.

### 3.6 Crash recovery
On daemon start: walk commits from `refs/heads/main` back to the journal's `checkpoint_commit`; collect `Graph-Op` trailers; ops in `applying`/`admitted` with a trailer → `committed`; without → re-queued (recheck again). Then advance `checkpoint_commit`. Crash-injection points (test-support failpoints): after admission, after tree build, after CAS before journal update, after journal update before worktree FF, mid effect execution.

### 3.7 Failures, cancellation, supersession, reminders
- Precondition/semantic failure → `rejected {reason, explanation, current_revs}`; notify requester through its seat system channel (threads Notify) and record in op. If the requester never submits a replacement (`supersedes` link), daemon re-notifies at 1h, 6h, 24h then stops; replacement or explicit `cancel` stops reminders.
- `herdr-graph cancel <op>`: only `admitted` ops; post-commit cancellation requires a superseding request specifying replacement desired state (e.g. `seat set-lifecycle dormant`), which marks the old op `superseded`. No hidden suppression flags.
- `herdr-graph reassign <op> --to <seat>` changes responsible requester; ops of retired requesters stay discoverable via `herdr-graph ops --unresolved`.
- Delayed instructions (e.g. whole-seat leave to other clones) carry `{op_id, object_id, rev}`; the recipient's skill calls `herdr-graph check-instruction <op> <object> <rev>` which answers `current|obsolete` before acting.

## 4. Herdr runtime: binding, observation, reconciliation

### 4.1 Herdr client
NDJSON over `$HERDR_SOCKET_PATH`: methods used — `session.snapshot`, `events.subscribe`, `workspace.create/rename/close`, `tab.create/rename/close`, `pane.split/close/process_info/get`, `agent.start`, `pane.report_metadata` (optional labels). Env for launched agents is passed via `--env` on tab/pane creation (Herdr `agent.start` has no env): `HERDR_GRAPH=1`, `HERDR_GRAPH_INSTANCE`, `HERDR_GRAPH_SEAT=<st_id>`, `HERDR_GRAPH_CLONE=<cl_id>`. Agents are started with `agent.start {name, kind, pane_id, args}`; resume via native args (`--resume <id>` for Claude, `resume <id>` for Codex) when the plan chose resume.

### 4.2 Binding
Clone ↔ pane binding stores `{pane_id, terminal_id, tab_id, workspace_id}`; seat ↔ tab, teamspace ↔ workspace. A binding is *verified* when snapshot shows the pane with the same terminal_id, or (after Herdr restart, terminal ids change) when the pane's process env/argv carries `HERDR_GRAPH_CLONE=<cl_id>` (read via `pane.process_info` cmdline/env where available) or the `agent_session` matches the clone's current native session. Otherwise the binding is `unknown` → availability `unknown`, never retirement; `/seat` in such a pane proposes a rebind plan.

### 4.3 Observation
- On daemon start and on (re)subscribe: full snapshot reconcile. Then `events.subscribe` to `workspace.*`, `tab.*`, `pane.created|closed|moved|exited|agent_detected`, `pane.agent_status_changed`. Periodic snapshot every 60s (configurable) because the event hub drops overflow silently.
- Mapping to observed mutations (submitted through the writer, mechanism recorded):
  - `pane_closed` of a bound clone → retire clone (cascade unit size 1). If the same snapshot/event batch shows its tab closed → tab closure rule instead.
  - `tab_closed` of a bound seat → one cascade action retiring the seat + its remaining active clones.
  - `workspace_closed` → one cascade action retiring teamspace + all non-retired seats (incl. dormant) + clones.
  - Cascade action records `retired: [ids]` and `already_retired: [ids]` (for undo).
  - `pane_exited` / agent gone with pane present → end occupancy (session `ended`, `end_reason`), triggers transcript finalization (§8).
  - `workspace_renamed`/`tab_renamed` → rename teamspace/seat (path moves; seat-channel notification §7).
  - `pane_moved` across tabs → clone `reload_required = {to_tab}`; across workspaces likewise; binding updated to new pane id.
  - Closure *without* event but missing from a *complete* snapshot while the Herdr connection is healthy and the object was bound+present at the previous snapshot → treated as closure. Missing during connection loss or from a failed snapshot → `unknown`.
- Correlation: every effect the reconciler issues is recorded with its expected observation (e.g. `close tab w1:t3 for op_X`); a matching observation within 30s is attributed to that op and does **not** create a second retirement.

### 4.4 Reconciliation
- Triggers: op committed, observation committed, periodic tick (60s), backoff timers.
- Diff desired (committed revision) vs live (last snapshot) → effects: `create_workspace`, `create_tab`, `split_pane`, `start_agent`, `rename_*`, `close_*`, threads effects (§7), summarizer request delivery (§8).
- Effect identity `ef_ = hash(op_id, object_id, effect_kind, object_rev)`. Before executing, the reconciler re-reads the committed revision: if the object's `rev` advanced and the effect is no longer implied → mark effect `obsolete` (history kept). This enforces latest-intent and prevents old creations reactivating retired seats.
- Transient errors (socket unavailable, timeouts) → retry with exponential backoff 1s→5min, jittered. Unknown outcome of a non-idempotent effect (e.g. create_tab timeout) → inspect snapshot for the expected object (by label + `HERDR_GRAPH_*` env/agent session) before retrying.
- Failures requiring revision (name conflict, invalid binding) → op/effect `needs_revision`, requester notified (§3.7).
- Ordering: dependent effects wait for prerequisites (tab before pane before agent); independent effects proceed.

### 4.5 Session replacements
When an effective harness/model/args change applies to an active clone with an occupant, the plan lists `replace_session {clone, from, to, resume: yes|no}`; applying: send the native exit (`agent.send-keys` `ctrl+c` twice / `/exit`), wait for `pane_exited`/idle shell (timeout 30s), start the new agent with resume args if the harness supports resume with a changed model (Claude: `--resume <id> --model <m>`; Codex: `resume <id> -m <m>`), else fresh. History preserved in `sessions`.

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
- Lifecycle cleanup: retiring a clone → `ReleaseRequirement` for its required memberships (threads itself retires participants on verified pane closure); messages preserved.
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
- On `pane_agent_detected`/status change, graph reads `agent_session` from the snapshot (Claude: id; derive transcript path `~/.claude/projects/<cwd-slug>/<id>.jsonl`, slug = cwd with `/` and `.` → `-`; verify existence) or `pane.process_info` argv (`resume <id>`). Records `ns_` on the clone with harness, native id, transcript path, start.
- Transcript record `tr_`: `{transcript path, native_session, seat, clone, source_seat_summaries_enabled_at_capture, coverage: [ranges], gaps: [ranges], unresolved?: reason}`. Range unit = byte offsets into the JSONL file, half-open `[start,end)`, end aligned to a newline.

### 8.2 Requests
- When a session ends (occupancy ended, pane closed, `/clear` session change, or resume appending after the last boundary), and the source seat's effective `summaries = true`, graph creates `rq_` = `{transcript, range: [covered_end, current_size), status: pending, created_by_op, delivery: {message_id?, dispatched_at?}, result?: {output_ref, covered_range, reported_by, at}}`. Dedup key = `(tr, range)`; overlapping pending requests are merged before delivery.
- If the source seat has `summaries = false` → no request (system/cron/dispatcher). Pending requests created while enabled stay pending after disablement (decision: disabling affects new captures only).
- Missing/unreadable transcript at request time → request `unresolved {reason: missing_input}` (visible to the scan, never "done").
- Destination: the teamspace's summarizer seat (seat whose template member role is `summarizer`) else the instance-level summarizer (`graph.toml summarizer_seat`). Retired summarizer → requests stay pending, no activation (retirement precedence). Active summarizer with no occupant → reconciler relaunches its single clone's occupant (authority §3.2), then delivers.
- Results: `herdr-graph request complete <rq> --output <path-or-url> --covered <start>-<end>` (bookkeeping op). Coverage merge is order-independent: covered ranges union; gaps = requested ranges not covered; a late result for an older range never shrinks newer coverage. `herdr-graph request list --pending|--unresolved|--undispatched` for the leftover scan.

### 8.3 Shipped summarizer template
Template `system-summarizer`: one member `summarizer` (startup active, `summaries=false`, harness claude). Its AGENTS.md: wait for thread requests; for each, ACK (dispatch) then dispatch a subagent that reads the transcript range, writes a summary under the source seat's folder via `herdr-graph content write`, then reports `request complete`; run `/loop 1h` to call `request list --pending --undispatched` and `--unresolved` and re-dispatch leftovers.

## 9. Bootstrap, skills, Beads convention
- Session-start: graph-launched panes carry `HERDR_GRAPH=1`. Shipped Claude SessionStart hook snippet (installed by `herdr-graph setup claude`, owned-group style like threads) injects "run `/seat`" when `HERDR_GRAPH=1`.
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
herdr-graph content write <path> --from <file> [--expect <blob>]
herdr-graph ops [--unresolved] | op <id> | cancel <op> | reassign <op> --to <seat> | check-instruction <op> <obj> <rev>
herdr-graph undo [--list|--json]
herdr-graph request list|ack|complete …        # transcript requests
herdr-graph who <pane>
```
Human-direct commands (`plan` without `--json` on a TTY) prompt `[y/n]` and apply on `y`.

## 11. Testing and validation
Tiers, each test labelled by tier in its name/module so the report can separate verified integration from mocks:
1. **Unit/property** (fakes): schemas round-trip; slugging/collisions; effective config precedence; coverage/gap merging under arbitrary result orders (proptest); template live propagation, exclusivity withdrawal, re-addition identity; plan hashing & staleness rules; undo candidate/preview computation incl. `already_retired`.
2. **Concurrency/crash** (fakes + real git): concurrent stale mutations (two renames same rev → one rejected; disjoint seats → both commit); readers during writes always see complete revisions (reader thread asserting invariants across many commits); failpoint crash at each writer boundary → restart → no lost/duplicate ops; cancellation vs commit race; obsolete effect fencing (retire after create queued → no tab created).
3. **Private-Herdr integration**: spawn a private `herdr server` (isolated HOME/XDG/HERDR_SOCKET_PATH/HERDR_CONFIG_PATH under `/private/tmp/hg-<rand>`, never `herdr server stop`, SIGTERM own pid only — the herdr-threads `private_host.py` pattern ported to Rust test support) with plain shell panes only: Herdr's agent detection needs real harness binaries, so tier 3 configures seats with harness `shell` (a graph-internal harness that runs no agent; `start_agent` is a no-op recorded effect) and asserts workspace/tab/pane lifecycle, env and observation; `agent.start` itself is exercised in tier 4. Flows: create teamspace+seats → tabs/panes appear with env; rename tab → seat renamed; close pane/tab/workspace → cascades; undo tab close → restored; template edit adds member → new tab; reconnect after event-buffer loss → snapshot reconcile.
4. **Real-agent smoke** (opt-in env `HG_REAL_AGENTS=1`): real Claude in private server: `/seat` resolves identity; summarizer request delivered (fallback path) → `request ack` → `request complete`.
5. **Threads**: fake for all logic; real-daemon integration test (opt-in `HG_REAL_THREADS=1`) against current threads protocol for EnsureThread/Invite(Required)/Membership/Notify/ReleaseRequirement; service ACK path only when the amendment is present.
Never touch the user's live Herdr session or memory observer in tests.

## 12. Packaging and delivery
- `herdr-plugin.toml`: id `herdr-graph`, `min_herdr_version = "0.9.1"`, `platforms = ["macos"]`, `[[build]] scripts/build.sh` (cargo build --release, copy binary into plugin `bin/`), `[[startup]] ["bin/herdr-graph","daemon","--ensure"]`, `[[actions]] status, doctor`.
- README: install (`herdr plugin link`), init instance, setup, `/seat`, plan/confirm, undo, threads amendment status, verified-vs-assumed matrix.
- `just`/`cargo` commands: `cargo test` (tiers 1–2 default), `cargo test --features private-herdr` (tier 3), env-gated tiers 4–5.
- Repository pushed to private GitHub `alepar/herdr-graph` (design docs, research and run artifacts included). This is post-merge delivery done at the run's finish step, not a task in the tree (a tree stops at merge-ready).

## Post-Implementation Notes

*As this design is implemented and iterated on — bug fixes, adjustments, anything that diverged from the assumptions above — append a dated note here, whether or not a formal debugging skill was used.*
