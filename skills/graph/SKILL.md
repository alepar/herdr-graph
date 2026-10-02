---
name: graph
description: Change the herdr-graph organization (teamspaces, seats, clones, templates) through the plan, confirm, apply flow, write seat content, and track work with Beads labels. Use when you need to create, rename, retire or activate graph objects, or to write notes into a seat.
---

# graph

Organizational changes are never made directly. They go plan, user confirmation, apply.

## Changing the organization

1. Build a plan: `herdr-graph plan --json <change...>`, for example
   `herdr-graph plan --json seat create reviewer --teamspace alpha --active`.
   Other changes: `teamspace create|rename|retire|resurrect`,
   `seat create|activate|deactivate|rename|retire|resurrect|override`,
   `clone add|retire|rebind`, `participation join|leave`, `template create|edit|copy`,
   `application apply|retire`, `undo`.
2. Show the plan to the user verbatim: the `rendered` text, including warnings. Do not
   paraphrase it and do not shorten it.
3. Ask the user for an explicit yes. Only an explicit yes counts. Silence, a question, or "go
   ahead and do what you think" is not a yes. There is no bypass and you must not look for one.
4. After the yes, relay it:
   `herdr-graph apply <plan_id> --confirm <hash> --confirmed-by user-relay`
   with the `plan_id` and `hash` from the plan you showed. If the graph changed in the
   meantime the apply is rejected as stale: build a new plan and show it again.
5. `herdr-graph undo` reverts the last action. The user runs it, not you.

## Operations

- `herdr-graph ops` lists operations; `ops --unresolved` lists the ones needing attention.
- `herdr-graph op <op>` shows one operation.
- `herdr-graph cancel <op>` cancels an operation that has not committed.
- `herdr-graph reassign <op> --to <seat>` hands a rejected operation to another seat.
- To replace an operation with a corrected one, add `--supersedes <op>` to the new plan.
- When you receive an instruction that refers to an operation and some time has passed, run
  `herdr-graph check-instruction <op> <object> <rev>` before acting. `obsolete` means the
  instruction no longer applies: do not act on it.

## Writing seat content

Write notes and other files into an object's folder with
`herdr-graph content write --object <id> --rel <path inside the folder> --from <file>`.
Add `--expect <blob>` to refuse the write when the file changed since you read it. The folder is
resolved when the write happens, so renames do not matter. You cannot write record files such as
`seat.toml`, and a retired object only accepts files under `summaries/`.

## Finding things

`herdr-graph show <id|path>`, `herdr-graph list [teamspaces|seats|clones|applications|templates]`
and `herdr-graph path <id>` read the committed graph without a daemon.

## Beads labels

Work in Beads is tied to the graph by labels:

- `hg-ts:<ts>` teamspace
- `hg-seat:<st>` seat
- `hg-clone:<cl>` clone
- `hg-ns:<ns>` native session

Use `bd list --label hg-seat:<st>` to find the work a seat inherited from earlier clones. Label
new work with your own `hg-seat:` so the next clone finds it. The graph never calls `bd`; the
labels are a convention you follow.
