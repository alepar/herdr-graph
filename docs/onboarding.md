# Onboarding: move an existing Herdr space into herdr-graph

> Status: draft, 2026-11-26. Sections marked *pending* wait for the seat-resurrect and restart work (hg-lnw, herdr-threads 3.6.0).
> Needs a build from `main`: adoption is not in a tagged release yet (v0.3.0 lacks it).

You already run agents in Herdr tabs and they talk over herdr-threads. This guide brings those tabs under herdr-graph one at a time, without restarting anything.

## What you get

- **Nothing is relaunched.** Adopting a tab records it as a seat. The agent in it keeps running, with its session and its threads memberships.
- **One tab at a time.** Unadopted tabs are never touched.
- **You can back out.** `release` returns a tab or a whole space to unmanaged, and nothing closes.
- **You confirm every change.** The agent proposes a plan; it is applied only after your yes.
- **An agent does the work.** You point an agent at this guide, name a tab, and answer its questions.

One rule to remember: to leave, use **release**. `seat retire` closes the tab, and `teamspace retire` closes the whole workspace, unadopted tabs included.

## Quick start for agents

If the user pointed you at this guide, do this in order. Stop and report on any failure.

1. Install, from a checkout of this repo:

   ```sh
   scripts/build.sh                      # builds and copies to bin/herdr-graph
   herdr plugin link <path-to-this-repo>
   herdr-graph init <instance-path>      # a new Git repo for the graph's records
   herdr-graph setup claude              # and/or: herdr-graph setup codex
   herdr-graph daemon --ensure
   herdr-graph doctor
   ```

   Rebuild after every pull: an old `bin/herdr-graph` has no `adopt` commands.
2. List what is unmanaged: `herdr-graph roster --live --unmanaged`.
3. Ask the user which tab to adopt first. Adopt one tab per run; never batch.
4. Run the `adopt` skill on that tab. It proposes each change as a plan. Show each plan to the user verbatim and apply it only after their explicit yes:

   ```sh
   herdr-graph apply <plan-id> --confirm <hash> --confirmed-by user-relay
   ```

5. Never run `seat retire` or `teamspace retire` on adopted objects to back out. Use `plan seat release` or `plan teamspace release`.

## Adopt your first space

The example is one project workspace, `myproject`, with four tabs:

| Tab | What runs there | In this example |
|---|---|---|
| `main` | Claude Code, the coordinator | adopted first |
| `impl` | Codex, implements tasks | adopted second |
| `review` | Claude Code, reviews | adopted later, or never |
| `shell` | a plain shell | left unmanaged |

**1. Install.** Run the install block from the quick start, once per machine. For Codex, open an interactive session afterwards and trust the new hook (the startup review or `/hooks`).

**2. Adopt the coordinator tab.** In any agent pane, say:

> Read docs/onboarding.md in herdr-graph and adopt the tab `main` in workspace `myproject`.

The agent reads the tab, its transcript and its threads history, then shows you up to three kinds of plan:

1. `teamspace adopt`: the workspace becomes a teamspace. No tab is adopted by this step.
2. `seat adopt`: the tab becomes a seat. Graph records the harness, model and arguments it observes.
3. Instructions: a draft role (`AGENTS.md`) and standing rules for the seat, taken from what you told that agent in its transcript. Edit the drafts before you say yes. Your repo's `AGENTS.md` and `CLAUDE.md` are not copied; they keep loading as before.

The first seat of a graph becomes the **node manager**: the seat that others escalate to, and that graph keeps open. Start with the tab you would want in that role.

**3. Let the adopted agent pick up its seat.** It runs `/seat` at its next session start, or when it reads its seat-channel invitation. Nothing else changes for it.

**4. Adopt the next tab.** Same sentence, with `impl`. This time the agent also proposes who manages the seat (`manager set impl main`), and quotes the messages that show it.

**5. Check.** `herdr-graph roster --live --unmanaged` now lists only `review` and `shell`. `herdr-graph roster` shows the two seats.

Stop here for as long as you like. A half-adopted space is a supported state.

## What changes once a tab is managed

- Closing the tab retires its seat. `herdr-graph undo` in a terminal lists recent actions and lets you pick one to reverse; an agent uses `herdr-graph plan undo <action>`.
- Renaming the tab in Herdr renames the seat, and the other way round.
- If the agent exits and the pane stays, graph starts an agent in it again, resuming the last session. Shell seats are left alone.
- The node manager's tab is reopened if you close it.
- *Pending (hg-lnw):* what undo of a closed tab restores on the threads side, and what happens to a seat across a Herdr restart.

## Back out

```sh
herdr-graph plan seat release <seat>            # one tab back to unmanaged
herdr-graph plan teamspace release <teamspace>  # the whole space
```

Both are plans you confirm like any other. The tab, its panes and its agents keep running, and the threads memberships stay. Graph keeps the history and transcripts of the released seat.

The node manager can be released only while it is the graph's only live seat. Otherwise move the role first: `herdr-graph plan node-manager set <seat>`.

## Recipes

Common team shapes, and how each maps to seats. Adopt in the order given.

| Your setup | Adopt | Leave unmanaged |
|---|---|---|
| **Independent agent tabs, you route between them** | Your longest-lived tab first (it becomes node manager), then the others as you go. | Throwaway tabs. |
| **Implementer plus reviewer** | Both tabs. The instruction drafts capture who writes and who only reads. | — |
| **Claude Code plus Codex** | Both; run `setup claude` and `setup codex`. Each seat records its own harness. Shared rules go on the teamspace (`rules/<name>.md`), once. | — |
| **Long-lived coordinator, short-lived workers** | The coordinator. Adopt a worker only if it will outlive its task. | Per-task workers: they end before adoption pays off. |
| **Utility panes** (dev server, logs, git UI) | Optional: a shell tab is adopted with the `shell` harness. | The default. A shell pane inside an agent's tab stays unmanaged in that tab. |

Workers in their own worktrees: after adoption, `herdr-graph plan seat override <seat> --cwd <path>` records the directory the seat's agent starts in.

Two or more spaces with the same shape: the `template-capture` skill drafts a reusable team template from one or two adopted teamspaces.

## Troubleshooting

- **`herdr-graph` does not know `adopt`**: the binary is old. Run `scripts/build.sh` again.
- **A command exits with status 5**: the daemon is not running. Run `herdr-graph daemon --ensure`.
- **`/seat` in an unmanaged pane** proposes adopting that tab. That is the same flow, started from inside the tab.
- *Pending (hg-lnw):* seat held after a Herdr restart; undo of a closed adopted tab.

Reference: README, [Adopting existing Herdr spaces](../README.md#adopting-existing-herdr-spaces).
