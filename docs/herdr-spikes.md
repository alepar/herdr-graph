# Herdr 0.9.1 spikes and wire findings

Recorded by hg-zmi.5 on 2026-10-02 against `herdr 0.9.1` (API protocol 22), always in a private server
(`PrivateHerdr`, root `/private/tmp/hg-<ulid>`); the user's default session was never contacted. Consumed by
hg-zmi.8 (reconcile/bootstrap) and hg-zmi.7. Re-run with:

```
cargo test --features private-herdr --test herdr_spikes -- --ignored --nocapture --test-threads=1
```

Task 8's design stays defensive either way; these observations only choose between a metadata token and a
label nonce, and fix which Herdr events may be trusted.

## Decisions at a glance

| Question | Observation | Consequence |
|---|---|---|
| Do metadata tokens survive a server restart? | No. Workspace and pane tokens (`hg=...`) were empty after a restart; labels survived. | Treat the token as an optimisation valid within one incarnation only. After a restart, re-adopt by label nonce and re-stamp. Choose the label nonce as the durable identity. |
| Are terminal ids stable across a restart? | No, every terminal id was regenerated (none shared). Workspace and pane ids happened to be reused. | Never key anything durable on `terminal_id` or on pane ids across incarnations. Terminal ids are stable only within one server process (they survive live handoff per the schema, not a cold restart). |
| Does closing the last tab close the workspace? | Yes. `tab.close` on the last tab returns Ok and the workspace is gone. | Plans that close a seat's last tab also remove the workspace; `FakeHerdr` defaults to this. |
| Event order when closing multi-pane tabs / multi-tab workspaces | Closing a tab or workspace emits only `tab_closed` / `workspace_closed`, with no per-pane `pane_closed`. Closing the only pane of a tab emits only `pane_closed`, not `tab_closed`, yet the tab disappears. | Events are hints only. Always re-snapshot; never infer a tab or pane removal from a missing event. |
| Does a detached daemon started from a `[[startup]]` hook survive the hook's exit? | Yes, both with plain `nohup ... &` and with a new session (perl `setsid`); both were re-parented to pid 1. | A hook may start the daemon with a plain `nohup` detach. Startup hooks run when the server starts, not at `plugin link`. |
| `agent_session` for an `agent.start`ed Claude without Herdr's Claude integration | not verified: needs `HG_REAL_AGENTS=1` and an explicit `ANTHROPIC_API_KEY`; no real agent was launched. | Keep the graph hook report as the primary session id source (`GraphHookReport` before `HerdrAgentSession`, as modelled). |

## Spike 1: close order for multi-pane tabs and multi-tab workspaces

Workspace with two tabs of three panes each, plus a one-pane tab. Events (`name:id`, arrival order, subscribed to
every global topic):

- close one pane of a 3-pane tab: `pane_closed:w2:p6`
- close that tab with 2 panes still in it: `tab_closed:w2:t2` (no `pane_closed` for the two panes)
- close the only pane of a one-pane tab in a multi-tab workspace: `pane_closed:w2:p7` (no `tab_closed`; the tab no longer exists in the next snapshot)
- close the workspace (a 3-pane tab remaining): `workspace_closed:w2` (no `tab_closed`, no `pane_closed`)

Consequence: closing a container does not announce its children, and removing a container's last child does not
announce the container. hg-zmi.8 must rebuild truth from `session.snapshot` on every event and treat event kinds as
wake-ups (spec 4.3.1), never as state transitions.

## Spike 2: closing a workspace's last tab

Closing one of two tabs leaves the workspace. `tab.close` on the last tab returns `Ok(())` and the workspace
disappears; events arrived as `workspace_closed:w2` then `tab_closed:w2:t1` (workspace first). Other workspaces were
untouched. Consequence: closing a seat's last tab is a workspace close; a plan must not expect an empty workspace to
remain, and the event order again must not be relied on.

## Spike 3: restart preserves metadata, labels, terminal ids?

Four panes in two tabs, workspace token `hg=ts_SPIKE`, pane token `hg=cl_SPIKE`, pane label `spike-label`;
private server stopped with SIGTERM and started again on the same root.

- workspace ids reused: yes (`w1`); pane ids reused: yes (`w1:p1..p4`)
- terminal ids reused: no (all four regenerated)
- workspace and tab and pane labels: preserved
- workspace token and pane token: not preserved (empty maps)
- incarnation: server pid and start time changed (probe works against the private socket)

Consequence: a token is not durable across a Herdr restart, so the label nonce must carry identity across
incarnations; pane and workspace ids may coincide across a restart without meaning anything, which is why diffs run
only within one incarnation. Live handoff (`server.live_handoff`) was not exercised: not verified.

## Spike 4: detached daemon from a `[[startup]]` hook

A throwaway plugin (`[[startup]] command = ["./start.sh"]`) was linked into the private server with `herdr plugin
link`. The hook did not run at link time; it ran when the server was restarted. The script started two children and
exited: (a) `nohup sh -c '... exec sleep 300' &`, (b) the same under `perl -MPOSIX -e 'POSIX::setsid(); exec @ARGV'`.
Three seconds after the hook exited both were alive, parent pid 1 (a stayed in the launcher's process group, pgid 365; b led its own group).
Consequence: the plugin's startup hook can launch the graph daemon with a plain detached background start. Whether the
child survives `herdr server` itself stopping was not tested (the fixture terminated the children explicitly).

## Spike 5: `agent_session` without Herdr's Claude integration

not verified: set `HG_REAL_AGENTS=1` with an explicit `ANTHROPIC_API_KEY` and run
`spike5_agent_session_for_agent_started_claude`. Side observation from `pane.report_agent` on a bare server: a
reported agent shows up in `agents[]` and in the pane (`agent`, `agent_status`) but `agent_session` stays absent
unless a session is supplied by the reporter, consistent with the defensive design.

## Wire facts the client depends on (verified with `herdr api schema --json`)

- Envelope: one NDJSON line `{"id","method","params"}`; reply `{"id","result"}` or `{"id","error":{"code","message"}}`.
  Unparsable requests are answered with an empty id.
- `session.snapshot` is flat (`workspaces`, `tabs`, `panes`, `agents`, `layouts`); the client nests it. Graph metadata
  tokens are the `tokens` maps (key regex `^[A-Za-z0-9_-]{1,32}$`, so the graph key is `hg`), written with
  `pane.report_metadata` / `workspace.report_metadata` and a `source` (`herdr-graph`).
- `pane.rename` exists (label), so `HerdrApi::rename_pane` is supported; nothing in the port trait is unsupported in 0.9.1.
- `workspace.create`, `tab.create`, `pane.split` accept `label`, `cwd`, `env` and `focus` (sent as false). Env reaches the
  shell (verified: `$HERDR_GRAPH_CLONE` echoed from each pane). `workspace.create` returns the first tab and pane.
- Subscription request topics are dotted (`tab.created`), delivered event kinds are underscored (`tab_created`) as
  `{"event": kind, "data": {...}}`. `pane.agent_status_changed` requires a `pane_id` (no wildcard: `*` is rejected), so the
  client subscribes it per pane known at subscribe time and relies on `pane.updated` / `pane.agent_detected` plus snapshots
  for panes created later. Added global topics beyond the bead's list: `workspace.metadata_updated`, `pane.updated`, `tab.moved`.
- `tab_created` and `pane_created` payloads nest the object (`data.tab.tab_id`); `*_renamed` and `*_closed` carry ids at top level.
- `agent.send_keys` and `agent.get` target an agent and reject a plain shell pane (`agent_not_found`); `send_keys` in the
  port goes through `pane.send_text` (text) and `pane.send_keys` (key names such as `Enter`), which work on any pane.
- `agent.start` needs a `name`; the port carries none, so the client uses the pane label (the kind when unlabelled).
  An unsupported kind is rejected with `unsupported_agent_kind` (mapped to `NeedsRevision`).
- Error codes seen: `agent_not_found`, `pane_not_found`, `unsupported_agent_kind`, `invalid_request`.
