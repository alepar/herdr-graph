---
name: graph
description: Change the herdr-graph organization (teamspaces, seats, clones, templates) through the plan, confirm, apply flow, store summaries, edit instruction files through reviewed plans, and track work with Beads labels. Use when you need to create, rename, retire or activate graph objects, to complete a summary request, or to change instructions.
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
2. Show the plan to the user verbatim: the `rendered` text, including warnings and any diff.
   Do not paraphrase it and do not shorten it.
3. Show this plan to the user, then run
   `herdr-graph apply <plan_id> --confirm <hash> --confirmed-by user-relay`
   with the `plan_id` and `hash` from the plan you showed; the harness asks the user to approve it
   in its own prompt. Tell the user to check that the plan hash in that prompt matches the hash
   printed above the diff. If the graph
   changed in the meantime the apply is rejected as stale: build a new plan and show it again.
   Never apply without showing the plan first, and never look for a way around the prompt.
4. `herdr-graph undo` reverts the last action. The user runs it, not you. `[y/n]` prompts
   refuse to run when `CLAUDE_CODE_SESSION_ID`, `CODEX_SESSION_ID`, `CODEX_THREAD_ID` or
   `HERDR_GRAPH_SEAT` is set: they print the plan and the apply hint instead.

### How strong the approval is

The daemon cannot tell whether the harness asked. `setup claude` adds a `Bash(herdr-graph
apply:*)` ask rule to `settings.json` and `setup codex` writes `rules/herdr-graph.rules`;
`herdr-graph doctor` (`harness_gate`) warns when a harness with the hook lacks the rule. This
is a cooperative control, the same trust level as `herdr-threads human`. Accepted residual
vectors: prompt-skipping harness modes; rewrapped invocations (`sh -c`, `env`, aliases) the
rule does not match; direct calls to the daemon socket; an agent editing its harness settings
to drop the rule; direct `git commit`s to the graph repository, which the renderer serves
because it reads the committed head; clearing the harness marker variables and faking a
terminal to answer a `[y/n]` prompt; and harness-native project instruction files
(`CLAUDE.md`, `AGENTS.md`, `AGENTS.override.md` in a seat's cwd and ancestors, including the
fallback cwd under `.graph-local/`), which are outside the graph's instruction sources. Do not
use any of them.

## Operations

- `herdr-graph ops` lists operations; `ops --unresolved` lists the ones needing attention.
- `herdr-graph op <op>` shows one operation.
- `herdr-graph cancel <op>` cancels an operation that has not committed.
- `herdr-graph reassign <op> --to <seat>` hands a rejected operation to another seat.
- To replace an operation with a corrected one, add `--supersedes <op>` to the new plan.
- When you receive an instruction that refers to an operation and some time has passed, run
  `herdr-graph check-instruction <op> <object> <rev>` before acting. `obsolete` means the
  instruction no longer applies: do not act on it.

## Summaries and instructions

Store a transcript summary by completing its request:
`herdr-graph request complete <rq> --summary-from <file> --covered <a-b>`. The daemon writes
`summaries/<transcript-id>-<a>-<b>.md` into the source seat's folder (live or archived) and
completes the request in one commit. The file must be UTF-8 Markdown. An identical retry is a
no-op; different bytes or a different range are rejected as `summary_conflict`. `--output`
remains for bookkeeping only and cannot be combined with `--summary-from`. The old direct file-write command
was removed; its stub exits 2 and points here.

Change instruction files through a reviewed plan, then the apply flow above:

- `herdr-graph plan instructions set <scope> [<target>] <file> --from <path>` and
  `herdr-graph plan instructions clear <scope> [<target>] <file>`.
- Scopes and their closed file sets: `global` (`rules/<name>.md`), `teamspace <name-or-id>`
  (`rules/<name>.md`), `seat <name-or-id>` (`AGENTS.md` or `rules/<name>.md`). `<name>` matches
  `[a-z0-9][a-z0-9_-]*`; there is no nesting. Retired objects are rejected.
- The plan prints a unified diff and names the seats affected. Content must be valid UTF-8
  without control or invisible code points (CRLF is fine); the plan names the first offender.
  `set` with identical bytes and `clear` of an absent file are plan errors.
- `herdr-graph instructions show <scope> [<target>] [<file>]` prints the committed file, or
  lists the scope's files; it needs no daemon.
- `plan template edit` (`agents_md`, `responsibility`) and `plan seat override --section`
  remain the way to change template and section instructions. Template edits preview live
  dependencies and session replacements.
- `plan undo <act>` reverts an instruction edit (it goes through the same confirmation). If the
  file changed since the edit, undo reports `repair_required` and changes nothing.

A change reaches a seat at its next `/seat` or session start (start, clear, compaction); a live
or resumed session may hold the previous text until its context is reset. `herdr-graph seat
--instructions` prints the full committed render; the SessionStart hook may truncate it.

Team members use typed `seat_template` IDs and `responsibility` to specialize reusable
`kind = "seat"` definitions. Applications require `kind = "team"` (the legacy default). Shared
definitions create independent seats; explicit member mappings select actual seat reuse.
Operational duties use `--system-duty`; legacy `--role` remains accepted.

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
