# herdr-graph

A Herdr plugin that keeps a durable, Git-backed organization graph of human and agent collaborators and applies it to Herdr: **teamspace = workspace, seat = tab, clone = pane**.

- The instance is a Git repository of records (teamspaces, seats, clones, templates, requests, transcripts).
- Changes go through plan / confirm / apply: a plan is computed and hashed, and only a confirmed hash is applied. Every applied plan can be undone.
- Seats talk through herdr-threads channels; transcript requests are recorded in the graph.

## Install

```sh
scripts/build.sh                 # cargo build --release, copies to bin/herdr-graph
herdr plugin link <path-to-this-repo>
```

Never run tests against your live Herdr session: all tests use private servers and isolated state. Linking the plugin into your real Herdr is a manual, deliberate step.

## Create an instance

```sh
herdr-graph init <path> [--with-examples] [--no-user-config]
```

The instance is located by, in order: `HERDR_GRAPH_INSTANCE`, the plugin config dir, then `~/.config/herdr-graph/config.toml`. `init` prints where it wrote the user config (or that it left one alone); `--no-user-config` writes nothing under `$HOME`, so probes can create an instance and point at it with `HERDR_GRAPH_INSTANCE`.

## Claude and Codex setup

```sh
herdr-graph setup claude [--uninstall]   # ${CLAUDE_CONFIG_DIR:-~/.claude}/settings.json
herdr-graph setup codex [--uninstall]    # ${CODEX_HOME:-~/.codex}/hooks.json
```

Each installs a SessionStart hook and the `seat`, `graph`, `adopt` and `template-capture` skills. Herdr manifests do not declare harness skills or hooks, so these commands do it. In a graph pane the hook tells the session to run the seat skill. Both harnesses fire SessionStart on startup, resume, clear and compaction, so a session whose context was reset or compacted reloads its seat role. Codex runs a new or changed hook only after you trust it in an interactive session (the startup review or `/hooks`); setup never writes that trust.

## /seat

`/seat` (in a Claude or Codex session started by herdr-graph) shows or sets the session's seat identity. The `graph` skill documents the CLI for agents.

In a pane graph does not manage yet, `/seat` proposes adopting the caller's own tab (`seat adopt <tab>`) instead of creating a second one; when the tab's workspace is not a teamspace it proposes `teamspace adopt <workspace>` first, and you run `/seat` again after applying it. See [Adopting existing Herdr spaces](#adopting-existing-herdr-spaces).

## Plan, confirm, relay

```sh
herdr-graph plan ... --json
herdr-graph apply <pl> --confirm <hash> --confirmed-by user-relay
```

An agent proposes a plan and relays the hash to the user; the user confirms. There is no bypass, and a non-TTY invocation never applies.

## Manager seats

```sh
herdr-graph plan manager set <seat|teamspace> <manager-seat>
herdr-graph plan manager clear <seat|teamspace>
herdr-graph plan node-manager set <seat>
```

Every seat has a manager it escalates lifecycle issues to: its own `manager`, else its teamspace's default, else the node manager. The node manager is the first seat created on a graph; it is always kept open, the role moves only with `plan node-manager set`, and when its pane is moved in Herdr into a tab no other seat uses, the seat follows it there (with a warning). While no node manager is assigned, every graph command prints a warning on stderr pointing at `plan node-manager set`.

## Adopting existing Herdr spaces

Graph can take over live Herdr workspaces and tabs without relaunching their agents. The unit of adoption is a tab, inside an adopted workspace; one space is migrated one tab at a time, and tabs you do not adopt stay unmanaged and untouched.

```sh
herdr-graph plan teamspace adopt <workspace-id|label> [--name <name>] [--project-repo <path>]
herdr-graph plan seat adopt <tab-id|label> [--teamspace <ts>] [--name <name>] [--harness <h>]
herdr-graph plan seat release <seat>
herdr-graph plan teamspace release <teamspace>
herdr-graph adopt inspect <tab|pane>
```

### Migration path for one space

1. `herdr-graph roster --live --unmanaged` lists the Herdr workspaces, tabs and panes graph does not manage (ids and labels only).
2. Pick one session and run the `adopt` skill in an agent pane. It reads `adopt inspect`, the native transcript, the pane's threads history and the existing graph, and proposes the following as ordinary plans, each shown to you and applied only after your yes.
3. `teamspace adopt` when the session's workspace is not a teamspace yet. It writes a teamspace bound to that workspace and adopts no tabs.
4. `seat adopt` for the tab. It records the observed harness, model and args as a seat override, takes the pane's existing threads seat, and relaunches nothing. A tab with a shell pane is adopted with the `shell` harness; a pane without an agent in an agent tab stays unmanaged inside the seat's tab (the plan says so).
5. Instructions: `plan instructions set seat <seat> AGENTS.md --from <draft>` and teamspace `rules/<name>.md`, drafted from the transcript under `<instance>/.graph-local/adopt/`. Repo `AGENTS.md` and `CLAUDE.md` load natively and are not copied.
6. Hierarchy: the first adopted seat of a graph becomes the node manager, as the first created seat does. Later seats get `manager set` proposals with the evidence quoted.
7. The adopted agent runs `/seat` (prompted by its seat-channel invitation or its next SessionStart) and picks up its graph instructions.

The adoption plan prints what changes once a space is managed: closing the tab or a pane retires the seat or clone (undoable); renaming the tab in Herdr renames the seat and renaming the seat relabels the tab; if the agent exits with the pane kept, graph starts one in it, resuming the last captured session; the node manager is reopened when closed.

### Keyless seats

Adopted clones are keyless: graph reserves no herdr-threads continuity key for them, links none and puts none in the launch environment. Threads' own continuity keeps the seat: a new agent or `/clear` in the same pane, a live `herdr update --handoff`, and a resumed session after a Herdr restart all reattach to the same threads seat, with its memberships (psa-global, pair channels, topic threads). Graph adds only its own seat and team channel invitations and does not import existing memberships into participation intent. Closing a keyless seat's tab retires its threads seat reversibly for 7 days (herdr-threads 3.5.0 and later). Undoing the closure brings the original seat back onto the reopened pane with `ResurrectSeat`, and `herdr-graph undo` names the window for each seat in its preview:

- Within 24 h of the close: everything returns, memberships, history, pending ACK receipts and invitations.
- From 24 h to 7 d: memberships and history return. ACK receipts and ordinary invitations owed to the seat were settled by threads at 24 h. Graph re-issues its own seat-wide invitations; invitations and receipts from other parties are not restored, and those parties have to send them again.
- After 7 d the close is final: the reopened pane gets a fresh threads seat, graph re-invites its own channels, and memberships graph does not own are lost.

`clone purge` of a closed keyless clone finalizes its seat at once, ending the window.

A Herdr restart needs no operator action for any graph-managed seat. Graph rebinds every seat threads holds unresolved on a pane that still exists (`ResurrectSeat` on the seat's own pane), and retries until the seat is placed. A keyless relaunch, fresh or resumed, waits until the seat is placed or the 90 s grace after the restart ends, then starts; an agent started before a late rebind lands enrolls on the original seat at its next check-in. Keyed relaunches never wait. Graph-created shell seats are placed and nothing is launched; an agent graph launched but never saw check in gets a fresh start after the rebind.

Two status rows (`herdr-graph status`, under `clones`) and their `doctor` findings report a seat graph could not bring back. Both name the seat, pane and reason, and the remedy `herdr-threads seat rebind SEAT --pane PANE --operator`; both clear on their own once the seat is back:

- `clones.restart_held` (doctor `restart held`): the seat still reads unresolved 90 s after a Herdr restart, because threads refused the rebind (the reason names it) or the rebind did not complete (threads unreachable or still erroring). The agent has been started anyway; once the seat is placed it enrolls on it.
- `clones.resurrect_held` (doctor `resurrect held`): threads refused graph's resurrect of a closed seat on undo, for example because another seat owns the reopened pane, and the seat is still reversibly retired 90 s after the first refusal. The restore launch waits for the seat; after the operator rebind, graph acknowledges the seat and launches the restored agent.

### Way back out

- `plan seat release <seat>` and `plan teamspace release <teamspace>` mark the records retired with the mechanism `released` (history and transcripts kept) and drop their bindings. They have no Herdr effect: the tab, panes and agents keep running, the threads seat keeps its memberships, and only graph's own channel invitations are released. Graph ignores a released object's token left on the panes.
- Undo of an adoption is a release; it never closes a tab.
- The node manager can be released, or its adoption undone, only while it is the graph's only live seat: the release then clears the node manager back to unassigned (a plan effect), and the next seat adopted or created becomes the node manager. With any other live seat the release is refused; move the role first with `plan node-manager set <seat>`. `teamspace release` of the teamspace holding the node manager follows the same rule: allowed only if no live seat exists outside what it releases.
- A release removes the released objects' adopt actions from the `undo` list.
- Only adopted objects can be released. A seat graph created, or one with a clone graph added, is refused ("was created by graph, not adopted; use `seat retire`, which closes its tab"); graph-created seats leave by `plan seat retire`.

### Other adoption tools

- `herdr-graph adopt inspect <tab|pane>` prints read-only JSON for the skill: ids, labels, cwd, git toplevel, agent kind and argv, native session and transcript path, threads seat, status and memberships. It plans and changes nothing.
- `plan seat override <seat> --cwd <path>` sets the directory a seat's agent is launched in, so per-worker worktrees can be expressed. A relative path resolves against the caller's cwd, and a missing directory draws a plan warning. It does not move a running agent.
- The `template-capture` skill reviews one or two adopted teamspaces and drafts a team template, seat templates, member `AGENTS.md` files and `manager set` plans (a template cannot declare member managers) under `<instance>/.graph-local/template-capture/<name>/`. Apply the result with `plan template create <name> --from <file.toml>`; attaching the source seats with `application apply --reuse` is a separate, optional plan.

Both skills are installed by `setup claude|codex`, beside `seat` and `graph`; rerun setup after upgrading to get them.

## Reusable templates and state paths

Team templates reference reusable seat definitions through member `seat_template` IDs and `responsibility`. `kind = "team"` is the legacy default; `kind = "seat"` supplies reusable instructions/runtime defaults. Independent applications create independent seats; explicit mappings reuse an existing seat. Runtime precedence is seat override → team member → seat template → team template → graph defaults → built-ins. `/seat` exposes live instruction references, application specialization, seat context and scoped rules.

Operational duties use `system_duty`; old `role` fields and `--role` still read, and unused `role_ref` is ignored. Edit reusable/member instructions with reviewed `plan template edit … --from …`, whose `agents_md` fields retain omitted text and clear on empty strings. Live changes preview immediate runtime effects and support undo.

New transcript indexes use `transcripts/<team-name>-<team-id>/<seat-name>-<seat-id>/<transcript-id>.toml`. Registration names and capture attribution remain historical through rename/move/retirement/undo. New bookkeeping records use `mutations/graph-changes/`, `mutations/undoable-actions/` and `mutations/transcript-processing/`. The upgraded binary reads legacy paths and updates old records in place; there is no eager migration or native transcript rewrite. Older binaries cannot discover new-layout records. See [GRAPH_STATE.md](GRAPH_STATE.md) for compatibility and [the release handoff](docs/releases/2026-10-07-agreed-fixes-handoff.md) for validation and release ownership.

## Summaries, instructions and the apply gate

- `herdr-graph request complete <rq> --summary-from <file> --covered <a-b>` stores a summary in the source seat's `summaries/` and completes the request in one commit. `content write` was removed; its stub exits 2 with this guidance.
- `herdr-graph plan instructions set|clear <global|teamspace|seat> [<target>] <file> [--from <path>]` edits global and teamspace `rules/<name>.md` and seat `AGENTS.md`/`rules/<name>.md` through the ordinary plan, confirm and apply flow, with a unified diff. `plan undo <act>` reverts the edit; if the file drifted, undo reports `repair_required`. `herdr-graph instructions show <scope> [<target>] [<file>]` reads the committed files without a daemon. Files must be UTF-8 with no control or invisible code points; names match `[a-z0-9][a-z0-9_-]*`; dotfiles are ignored; `doctor` (`instruction_files`) warns about files that fail and the renderer shows a placeholder instead.
- `herdr-graph seat --instructions` prints the instruction stack rendered from the committed revision; `seat --json` carries it as `instructions`, and `paths.data` / `paths.clone_record` replace `paths.clone_files`. In a bound pane the SessionStart hook injects the same render. Output is capped at 9000 bytes: the most specific parts are kept, general parts are omitted whole, and a marker line names them. A live or resumed session may hold earlier instruction text until its context is reset.
- `setup claude` and `setup codex` install an ask rule so the harness prompts before `herdr-graph apply` runs (`Bash(herdr-graph apply:*)` in Claude `permissions.ask`; `rules/herdr-graph.rules` for Codex). `herdr-graph doctor` (`harness_gate`) warns when it is missing. When the harness asks you to approve an apply, check that the plan hash in the prompt matches the hash printed above the diff.
- This gate is cooperative, at the trust level of `herdr-threads human`. Accepted residual vectors: prompt-skipping harness modes; rewrapped invocations (`sh -c`, `env`, aliases); direct daemon-socket calls; agents editing harness settings; direct `git commit`s to the graph repository; clearing the harness marker variables and faking a terminal to answer a `[y/n]` prompt; and harness-native project instruction files (`CLAUDE.md`, `AGENTS.md`, `AGENTS.override.md` in a seat's cwd and ancestors, including the fallback cwd under `.graph-local/`). The `[y/n]` prompts refuse under `CLAUDE_CODE_SESSION_ID`, `CODEX_SESSION_ID`, `CODEX_THREAD_ID` or `HERDR_GRAPH_SEAT`.

## Undo

```sh
herdr-graph undo
```

## Ops

`herdr-graph ops` lists operations; `cancel`, `reassign`, `check-instruction` and `--supersedes` manage them.

## Inspection

Read-only views of graph state. None of them writes to the graph repository.

**Scope grammar.** `roster`, `log` and `sessions` take an optional scope: `global`, `teamspace <t>` or `seat <s>` (name or id; retired targets resolve too). Omitted means the whole instance. `global` means the global paths only: `graph.toml`, `rules/`, `templates/`, `applications/` and `threads-namespace.json`. `clone <c>` is accepted by `sessions` alone; elsewhere it is a usage error.

**Daemon needs.** `roster`, `instructions stack`, `log` and `sessions` read the committed head and the local `.graph-local/` files without a daemon. `roster --live` and `ops` go through the daemon and exit 5 when it is down; they never start one.

**`--json`.** Every view takes it. stdout carries exactly one JSON document with `schema: 1` and the `head` commit it was computed from; each invocation pins HEAD once, so the document describes that one commit. A notice with no JSON field (the `log` pointer to `ops`, a warning about a skipped record) goes to stderr. In text mode the same notices are part of the output. A value that can be absent is `null`; a degraded state has its own field (`live_unavailable`, `scan`, `tombstones`, `imports_followed`, a per-row `error`). One bad object (a seat whose instruction stack does not resolve, a commit whose record does not parse) becomes that row's error state, not a failed command.

### roster

`herdr-graph roster [<scope>] [--all] [--live [--unmanaged]] [--json]` lists every non-retired teamspace, seat and clone with lifecycle, pane, occupant (harness, native session id, transcript path or `unresolved`), threads seat and state, manager, last change and instruction status. `--all` adds retired objects. Each seat's last change comes from one walk of the history and is `null` when none is found within the scan cap (`last_change_scanned: false` means older history was not searched). If the search for a seat reaches a commit whose tree has a record that does not parse, it stops there: `last_change` is `null`, `last_change_scanned` is `false`, `last_change_unreadable` names that commit (`null` otherwise), and the text shows `last change unknown (unreadable tree at <id>)` and a `warning:` line.

`--live` makes one daemon call and adds, per clone, the Herdr agent kind and status and whether the committed pane binding still matches the live snapshot, plus the threads refusals and lost-seat state from the daemon's own once-per-step read of the seated clones. With no seated clone that read is empty and healthy (`seat_refusals` is `{}`, `seat_lost` is `[]`). The threads half is unavailable only before the daemon's first step, or when its last read failed (threads not running or not located, or it refused the read); then the refusals and lost-seat fields are `null` and `live_unavailable.threads` gives the reason. `--unmanaged` lists Herdr workspaces, tabs and panes graph does not manage (ids and labels only).

- A partial failure exits 0 and the committed roster is still printed. A failed Herdr snapshot nulls every clone's `live` and sets `live_unavailable.herdr`. Threads-only unavailability nulls `threads_live.seat_refusals` and `seat_lost` and sets `live_unavailable.threads`, while every clone's `live` stays populated. The text starts with `live data unavailable: <reason>`.
- A pane binding recorded under an earlier Herdr server is never matched against the new server's ids, even when they coincide. It is reported as `binding from earlier herdr server`.
- Only a down daemon is an error (exit 5).

### instructions stack

`herdr-graph instructions stack <seat> [--clone <c>] [--json]` works for any seat, not only the caller's. It prints the instruction manifest at HEAD: each part's label, path, bytes, digest and the commit that last touched its path, which parts the 9000-byte hook would keep, and, per clone, the loaded status that `roster` also shows. The commit column is `path last touched`, not "changed": a seat section lives in a whole record file, so it can name an unrelated edit to that record. Staleness is decided by digest and never by that column.

Harness files (`CLAUDE.md`, `AGENTS.md`, their `.local`, override and `.claude/rules` variants, and the user-level files) are listed with path, size and mtime, marked not graph-managed. Their content is never read. They are **present, not necessarily loaded**: a harness setting can switch a file off, Codex caps project docs by size and Claude loads subdirectory files on demand. User-level paths are resolved from the inspecting process's environment, which may differ from the agent's. Because content is not read, `@`-imports are not followed (`imports_followed: false`).

**What the loaded record can and cannot say.** The SessionStart hook writes `.graph-local/loaded/<clone>.json`: the parts it injected into one native session, whole or cut, with the digest of the delivered bytes, and the labels the cap omitted. `seat --instructions` and `seat --json` never write it. The status is:

| Status | Meaning |
| --- | --- |
| `current` | the record's native session id equals the clone's occupant and the hook would deliver the same parts now |
| `stale` | same session, different delivery; `stale_parts` lists each part as `modified`, `added` or `removed` |
| `unknown` | no record, or it cannot be tied to the occupant (below) |
| `error` | the seat's stack does not resolve at HEAD; `message` says why |

`truncated` is a flag beside the status, set when the record has omitted or cut parts. The record says what graph injected, not what the model retained.

Limits, all reading `unknown`:

- Until the next injection after an upgrade there is no record.
- Codex seats read `unknown` when the hook payload carries no `session_id`; the record is still shown (`session not identified`). Graph does not guess one from timestamps. If Codex's id form differs from the occupant's, the note is `codex session id form differs`.
- A hook installed before `--stdin-payload` records `native_session_id: null` and reads `unknown` with `hook predates --stdin-payload; rerun setup`.
- Rerun `herdr-graph setup claude` or `setup codex` to rewrite the owner-marked hooks. Codex records trust against the hook's hash, so after the command changes Codex skips the hook until you trust it again in `/hooks`; `setup codex` prints that notice when it changes the command.

### sessions

`herdr-graph sessions [<scope> | clone <c>] [--since <time>] [--limit N] [--json]` lists every native session of the scoped clones, newest `started` first, with the transcript path (`unresolved` when none is recorded, always so for Codex today). Retired clones, seats and teamspaces are included. `--since` takes RFC 3339, `YYYY-MM-DD` or a duration such as `2d`; `--limit` defaults to 50. JSON: `{schema, head, tombstones, total, rows}`, `total` counting sessions after `--since`. A purged clone leaves only a tombstone with no sessions; the count is `tombstones`, and the text footer reports it.

### log

`herdr-graph log [<scope>] [--since <time>] [--limit N] [--by <seat>] [--all] [--max-scan N] [--json]` lists committed history only, newest first: who asked for what and which objects it touched. Operations that never committed (rejected, failed, in flight) are in the journal; use `ops` for them, and `sessions` for sessions. The text ends with a pointer to `ops`.

- The latest modification to global, a teamspace or a seat is `log <scope> --limit 1` (`log global --limit 1`, `log teamspace proj --limit 1`, `log seat foreman --limit 1`).
- `--by <seat>` is an AND filter with the scope: `log seat a --by b` is the changes to `a` that `b` requested. Commits with no requester (observed, bookkeeping, reconciler) never match it.
- Bookkeeping commits are hidden unless `--all`.
- A scoped query scans up to `--max-scan` commits (default 5000). `scan.stopped_early` and the text line `scanned N commits, older history not searched` tell "nothing found" from "cap reached". `--limit` defaults to 30.
- A commit whose tree has a record that does not parse cannot have its scope resolved. It is listed with `malformed: ["tree"]` and a warning, even though it may be outside the scope, and it counts toward `--limit`.
- A row's `undone` state is read from HEAD, not from the commit that created the action.

### ops filters

`herdr-graph ops [--unresolved] [--seat <s>] [--teamspace <t>] [--limit N] [--json]` is the journal listing. `--seat` and `--teamspace` filter on the requester (retired seats included) and combine with AND. It goes through the daemon (exit 5 when down), so it also lists rejected, failed and in-flight operations that `log` cannot show.

## Exit codes

| Code | Meaning |
| ---- | ------- |
| 0 | success |
| 1 | error, rejection or failure |
| 2 | no instance configured, or not inside a Herdr pane (also usage errors of `request complete`) |
| 3 | still running: the change was admitted and finishes in the background (`herdr-graph op <id>`) |
| 5 | graph daemon unavailable (not running, still starting, no reply); retry later |
| 6 | graph integrity error; not transient, report it to the node manager |

The daemon answers every CLI request within `CALL_TIMEOUT − REPLY_MARGIN` (27 s), so a slow writer yields exit code 3 and the op id instead of a transport timeout.

## Threads integration and amendment status

herdr-graph consumes herdr-threads only through its public client API (`third_party/herdr-threads`). Summarizer requests use service-ACK delivery (herdr-threads epic `ht-5nb`, on herdr-threads main since 84de563d): graph registers `service_session_v4` (service ACK, continuity keys, un-retirement, finalization, `ResurrectSeat` and the held-seat rebind), posts an ACK-required request on the summarizer seat channel, and reads its receipts; an ACK means the summarizer received (dispatched) the request, never that it was processed. ACK delivery also requires `messages.send_time_receipts_v1`. If a compatible daemon definitively lacks either capability, graph falls back to Notify plus `herdr-graph request ack`. The cargo feature `threads-service-ack` is on by default (`--no-default-features` forces the fallback).

The dependency target is herdr-threads **3.6.0**, release commit `7c3c09082e8894ebceec9c060b769b07e359a2ad` (`v3.6.0`), dependency submodule pinned to that exact official release commit, using wire protocol **9**. [Official release](https://github.com/alepar/herdr-threads/releases/tag/v3.6.0). The threads daemon must be 3.6.0 or later before this graph connects. Threads 2.1.0–3.6.0 upgrade the threads node store one way (up to schema 19: 3.3.0 moved it to 18, 3.5.0 to 19); back up the threads state directory before upgrading the daemon if you might downgrade. Rebuild graph when upgrading the threads checkout, and restart the graph daemon to load that binary; a binary built against an older wire protocol cannot communicate with the upgraded threads daemon. Pane resolution uses the public host-qualified `ResolveSeat` operation for graph-managed panes. It may allocate a bare local seat before an agent enrolls; it never checks in, accepts invitations or ACKs. Each call uses a fresh operation key so a later pane rebind cannot replay an old resolution. A lost response may leave a bare seat; repeating resolution returns the current unique owner for that host and pane. Restore holds remain authoritative. A `not_required` receipt represents a waived ACK obligation and never marks a summarizer request as dispatched. Graph-managed channels are exempt from threads' automatic quiet-channel archival.

### herdr-threads discovery

The daemon finds the herdr-threads state directory on every call (threads may be installed or started after graph), using the same rules herdr-threads itself uses. First hit wins:

1. `HERDR_GRAPH_THREADS_STATE_DIR` (absolute path).
2. `threads_state_dir = "/abs/path"` in `~/.config/herdr-graph/config.toml` (or the `config.toml` of the plugin config dir; the plugin config dir is read first).
3. The `herdr-threads` sibling of herdr-graph's own plugin state dir (`HERDR_PLUGIN_STATE_DIR` ending in `herdr-graph`, e.g. `<root>/herdr/plugins/herdr-threads`).
4. `$XDG_STATE_HOME/herdr/plugins/herdr-threads`, if it exists.
5. `~/.local/state/herdr/plugins/herdr-threads`, if it exists.

If both 4 and 5 exist and differ, discovery refuses to guess: set `threads_state_dir`. Relative paths are ignored. Discovery reads the node identity hint and published socket descriptor under `<state>/node`; one node can serve several Herdr hosts. Pane resolution always names graph’s configured Herdr socket explicitly. To skip discovery entirely, set `HERDR_GRAPH_THREADS_SOCKET` and `HERDR_GRAPH_THREADS_INSTANCE` (an absolute daemon socket path and its canonical 32-lowercase-hex node identity). Incomplete or malformed pairs are reported as errors instead of silently falling back.

`herdr-graph doctor` prints a `threads` line: `state dir <path> (<where it was found>)` or `not found (<reason>)`, followed by whether the running daemon is connected to threads (its capability, or the error). A missing herdr-threads install is reported as `[WARN]` and does not fail doctor: graph works without threads, degraded (no channels, notifications or summarizer delivery). The check fails when the state dir is found but the running daemon reports it is not connected, or when discovery is ambiguous.

A herdr-threads daemon older than 3.6.0 refuses graph's `service_session_v4` registration; graph does not fall back to v3. Graph reports the named error `herdr-threads daemon refused service_session_v4 (<detail>); upgrade the herdr-threads daemon to >= 3.6.0`, threads stays disconnected, and the doctor `threads` line fails with the upgrade step: install herdr-threads 3.6.0 or later and restart the herdr-threads daemon. Graph retries the registration on its next threads call.

### Threads node identity and recovery

Production setup attempts discovery and binding before the writer starts; it never registers a service just to bind. The first connection binds an empty graph and service journal to the exact node in `.graph-local/threads-intents.node.json`. Graph rechecks discovery and this binding before every service or pane-resolution call, including calls using a cached client. Restarting the same threads node preserves the binding and replays its unresolved service envelopes under their original operation IDs. A different node, a nonempty legacy journal, a populated unbound graph, or a malformed binding stops threads operations with a recovery diagnostic. Graph history, channel references, processing requests, and old intent files remain intact; old receipts are never reinterpreted against the new store.

For a bound graph, restoring the exact original node/store resumes that namespace. An unbound legacy graph requires explicit reconnect even if its current tree is empty: graph scans committed ancestry before first binding. Preserve the graph repository, `.graph-local` and old threads state; never remove a binding or pending intents to bypass this check.

To move to a different node, obtain its canonical 32-character lowercase node ID from threads discovery and the graph repository's current `refs/heads/main` commit. Review and apply the existing plan workflow:

```sh
herdr-graph plan --json threads reconnect --from <old-node-or-legacy> --to <new-node> --head <40-character-main-commit>
herdr-graph apply <plan-id> --confirm <plan-hash> --confirmed-by user-relay
```

Use `legacy` only when the graph has no local binding. The plan rejects a mismatched old binding, a same-node target, malformed IDs, and any head change before apply. The selected endpoint must advertise the exact target node before any work can resume; planning does not start or replace a threads daemon.

The commit records `threads-namespace.json` and `mutations/threads-reconnect/<operation>.json`. The artifact retains old channels, invitations, external participation (including template relationships and application contributions), opt-outs, request delivery attempts, and exact graph journal operation/effect/meta provenance. Managed channels are recreated using a fresh namespace epoch. Incomplete requests retain their IDs, ranges and attempt history while their active delivery state is reset; completed requests, native transcript files and Git history remain untouched. External threads stay unresolved in the artifact and are never automatically recreated or invited. Rejoin an external thread only after resolving its identity on the target node and reviewing a new participation plan.

Publication is commit-first: the next threads call atomically updates the local binding only when it matches that committed transition's exact previous node. A crash before commit leaves the old namespace active; writer recovery retries the same confirmed transition. A crash after commit leaves the committed target authoritative and the next call completes local publication. Corrupt or unexpected bindings fail closed. Before publishing the target binding, graph durably seals the previous intent directory with the exact committed transition. Interrupted publication retries that seal; corrupt or conflicting seals fail closed. Direct low-level clients also refuse sealed directories, whose original intent bytes remain untouched. New work uses `.graph-local/threads-intents-<epoch>`. Cached snapshots and queued writebacks from older epochs are refused. Same-node restarts continue the current epoch without reconnecting.

Every plan reviewed before reconnect must be planned and reviewed again, including otherwise equivalent or unrelated organizational changes. Stored plans remain readable, but applying one across a namespace boundary is rejected even if it was already queued. This conservative rule prevents old external IDs in participation, template or application plans from becoming target-node intent.

Ordinary undo refuses reconnect actions and actions predating the current namespace boundary. Reversing the namespace requires another reviewed reconnect with current `--from`, the previous canonical node as `--to`, and current `--head`; `legacy` is never a target. Reconnect does not restore the old node's memberships or replay its dormant intents. Restore desired organization through new explicit plans after assessing the preserved artifact. If threads is unavailable until after a new graph has committed records, use the explicit `legacy` reconnect workflow; graph does not infer those records are safe. Historical first-binding checks scan all main-branch ancestry, so first connection to a long-lived unbound repository can take longer.

### Initializing the pinned threads dependency

The repository pins `third_party/herdr-threads` as a Git submodule at the exact official v3.6.0 release commit. Initialize it before building; this does not change any separate local threads checkout:

```sh
git submodule update --init --recursive
cargo check --locked --offline --all-targets --all-features
```

The Cargo command requires its locked registry dependencies to be cached. A source clone can initialize submodules during cloning with `git clone --recurse-submodules <graph-repository-url>`. GitHub source archives omit submodule contents; a buildable source bundle must include the pinned dependency tree explicitly. Dependency upgrades require an explicit reviewed gitlink and lockfile update, followed by qualification; a moving external checkout is no longer the build input.

## Test tiers

| Tier | Command |
| --- | --- |
| 1-2 unit and integration | `cargo test` |
| crash-injection | `cargo test --features test-support` |
| 3 private Herdr server | `cargo test --features private-herdr -- --test-threads=1` |
| 4 real agents | `HG_REAL_AGENTS=1 cargo test --features private-herdr --test config_smoke --test real_agent_smoke -- --test-threads=1 --nocapture` |
| 5 real threads | `HG_REAL_THREADS=1 cargo test --features private-herdr --test threads_real -- --test-threads=1 --nocapture` |

To build and test on a host without a C toolchain (Fedora Atomic), use this project's toolbox, `hg-build`. The Rust toolchain comes from `~/.rustup` on the host:

```sh
toolbox create -c hg-build --distro fedora --release 44
podman exec -u root hg-build dnf install -y gcc make pkgconf-pkg-config zlib-ng-compat-devel git
toolbox run -c hg-build cargo test
```

Adoption adds the tier-3 tests in `tests/adopt_private_herdr.rs` (a live tab with a checked-in agent keeps its threads seat and memberships across a daemon restart, release leaves the tab and agent running, unmanaged tabs in an adopted workspace are left alone: `cargo test --features private-herdr --test adopt_private_herdr -- --test-threads=1`) and the tier-5 test `adopt_resolves_the_checked_in_seat` in `tests/threads_real.rs` (run it with the tier-5 command above).

The inspection views add two tier-3 tests: `tests/inspect_live.rs` (`roster --live` against a private Herdr server: `cargo test --features private-herdr --test inspect_live -- --test-threads=1`) and `tests/inspect_sweep.rs` (the end-to-end read-only sweep, requirement R14, through the setup hook: `cargo test --features private-herdr --test inspect_sweep -- --test-threads=1`). `tests/inspect_loaded.rs` (the real hook chain writing the loaded record) runs in tier 1 through `TestRoot`.

Tier 5 against the installed threads daemon instead of the pinned build: add `HG_TEST_THREADS_BINARY=~/.local/share/herdr-threads/bin/herdr-threads`.

Tiers 3 and 5 build the pinned `third_party/herdr-threads` into `target/threads-bin` (a no-op when current) and fail if the herdr-threads binary, `HG_TEST_THREADS_BINARY` included, does not report the submodule's version; matrix mode requires the override and never runs a nested Cargo.

All tiers use private servers and isolated state. The release-build packaging test is ignored by default: `cargo test --test packaging -- --ignored build_script_places_binary`.

Test isolation: every test that spawns a process (Herdr, the daemon, the CLI) goes through `tests/support/isolated.rs` (`TestRoot`) or `PrivateHerdr`. Each gets its own temp root with HOME, the XDG dirs, `CLAUDE_CONFIG_DIR`, the Herdr plugin dirs, the Herdr socket and `HERDR_GRAPH_INSTANCE` all inside it, and `HG_TEST_ROOT=<root>` as a marker. A child that resolves a path outside its root exits 97 and fails the test; an in-process test that resolves the real `~/.config/herdr-graph`, `~/.claude` or the live Herdr socket panics. When a test ends (pass, fail, panic, timeout) its children are signalled and every process still carrying the marker, with its descendants, is swept. A hard-killed test process (SIGKILL, a cargo timeout) leaves nothing behind either: each root holds an exclusive flock on `<root>/owner.lock` for its owner's lifetime (the kernel frees it however the owner dies), and a watchdog process (the test binary re-executed in its own process group, without the marker) waits on that lock and sweeps and removes the root the moment it is free. As a backstop, every `TestRoot` and `PrivateHerdr` start first sweeps the roots of dead owners: roots whose directory is gone or whose lock is free, plus lock-less roots from older checkouts once they are two hours old (a fresh lock-less root is never touched). `tests/test_isolation.rs` pins all of this, the hard-kill guarantee in `sigkilled_test_leaves_no_rig_behind`, `sigkilled_test_leaves_no_private_herdr_behind`, `start_of_run_sweep_reaps_a_dead_owners_rig` and `orphan_rule_spares_a_fresh_lockless_root`.

## Verification matrix

[docs/verification-matrix.md](docs/verification-matrix.md) records the threads 3.6.0 pin and its 3.1.0 to 3.6.0 contract audit (November 26), the November 7 v0.3.0 qualification against threads 3.1.0 on Linux x86_64 with Herdr 0.9.5 only (tiers 1–3 and 5; the Herdr release matrix did not run, hg-aka), the October 29 threads 2.6.4 qualification and the October 23 threads 2.3.1 qualification on Linux x86_64 with Herdr 0.9.5 (all tiers 1–3 and 5 pass), and the October 18 threads 2.0.4 qualification on macOS aarch64: Herdr **0.9.1–0.9.5 each passed all 37 private graph/API/threads cases** (185 total), with clean teardown and retained private-daemon/plugin diagnostics. The [qualification handoff](docs/releases/2026-10-17-threads2-herdr-qualification.md) records exact source revisions, feature-suite counts, checksums, preserved failures, recovery limits and review/integration gates. October 5/7 evidence remains historical and unchanged.

`verified-real` means exercised against private Herdr or an isolated herdr-threads daemon. `verified-fake` means covered with fakes or a real Git store and journal. `assumed` marks behavior that remains unexercised or skipped.

Run tier 3 serially: `cargo test --features private-herdr -- --test-threads=1`. The seat-rename and undo-adoption regressions wait for the derived view and pane token respectively. Historical defects D1–D3 have been fixed; their regressions run in the normal suite.

Real-agent launch/resume, SessionStart bootstrap, and the agent summarizer flow still require the opt-in tier and credentials. Service-ACK delivery is exercised against an isolated 2.6.4 threads daemon with synthetic public-hook registration and cooperative CLI acceptance/ACK; real summarizer-agent delivery is separate. Herdr live handoff remains untested.

Run feature tiers sequentially: another Cargo feature build can replace `target/debug/herdr-graph` while subprocess tests are using it.

## Design documents

Design and product exploration for a durable organization of human and agent collaborators inside Herdr.

## Two products

1. [herdr-graph vision](HERDR-GRAPH-VISION.md): the foundation—persistent responsibilities, native Herdr presence, relationships, templates, instructions, and recoverable organizational state.
2. [Software factory vision](SOFTWARE-FACTORY-VISION.md): a concrete product built on that foundation—project teams, temporary feature collaborators, rulebooks, and evidence-based delivery. Its name is still open.

Both are draft product visions, informed by the existing design and external research. They distinguish settled direction from new proposals and do not assert implementation readiness.

## Supporting material

- [Firstmate comparison](research/firstmate-20260928/COMPARISON.md): pinned documentation/code audit, overlap with both visions, and the case for adopting, extending, or building. [HTML](research/firstmate-20260928/COMPARISON.html) · [PDF](research/firstmate-20260928/COMPARISON.pdf)
- [Firstmate operator sentiment](research/firstmate-20260928/SENTIMENT.md): firsthand praise, operational complaints, fixes, and policy disagreements, with a deduplicated sample. [HTML](research/firstmate-20260928/SENTIMENT.html) · [PDF](research/firstmate-20260928/SENTIMENT.pdf)
- [Research synthesis](research/software-factories-20260928/RESEARCH.md): factory designs, operator experience, implications, and 21 cited sources. [HTML](research/software-factories-20260928/RESEARCH.html) · [PDF](research/software-factories-20260928/RESEARCH.pdf)
- [Landscape evidence](research/software-factories-20260928/landscape-evidence.md) and [operator sentiment](research/software-factories-20260928/sentiment-evidence.md).
- [Designer consultation](research/software-factories-20260928/designer-consultation.md) and [review notes](research/software-factories-20260928/validation-review.md).
- [Design notes](DESIGN-NOTES.md): decisions and explicit supersessions from the adjacent designer session.
- [Current design and continuation](CURRENT-DESIGN.md). Historical [resume](archive/RESUME.md), [original seed](archive/HANDOFF.md), and [Herdr identity qualification](archive/HERDR-IDENTITY-CHECK.md).

The deep-research skill is present in the default Codex profile and all three AISW Codex profiles. [Installation record](research/software-factories-20260928/skill-installation.json) includes destinations and matching file hashes. Research sources, evidence, claims, and validation logs are kept alongside the synthesis.

## License

MIT OR Apache-2.0, at your option: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).
