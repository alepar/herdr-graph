# Onboarding: move an existing Herdr space into herdr-graph

> Needs herdr-graph v0.4.0 or later and a herdr-threads daemon of 3.6.0 or later.

You already run agents in Herdr tabs and they talk over herdr-threads. This guide brings those tabs under herdr-graph one at a time, without restarting anything.

## What you get

- **Adoption relaunches nothing.** Adopting a tab records it as a seat. The agent in it keeps running, with its session and its threads memberships.
- **One tab at a time.** Adoption and release never touch unadopted tabs.
- **You can back out.** `release` returns a tab or a whole space to unmanaged, and nothing closes.
- **You confirm every change.** The agent proposes a plan; it is applied only after your yes.
- **An agent does the work.** You point an agent at this guide, name a tab, and answer its questions.

Terms: a **teamspace** is a Herdr workspace that graph manages, a **seat** is a managed tab, and the **harness** is the agent program in it (Claude Code, Codex, or a plain shell).

One rule to remember: to leave, use **release**. `seat retire` closes the tab, and `teamspace retire` closes the whole workspace, unadopted tabs included.

## Quick start for agents

If the user pointed you at this guide, do this in order. Stop and report on any failure.

0. Check `herdr-threads --version`: graph refuses a daemon older than 3.6.0. If it is older, tell the user to upgrade herdr-threads and restart its daemon first.

1. Install, from a checkout of this repo:

   ```sh
   scripts/build.sh                      # builds and copies to bin/herdr-graph
   herdr plugin link <path-to-this-repo>
   herdr-graph init <instance-path>      # a new Git repo for the graph's records; ask the user where
   herdr-graph setup claude              # and/or: herdr-graph setup codex; installs the skills, `adopt` included
   herdr-graph daemon --ensure
   herdr-graph doctor
   ```

   Rebuild after every pull: an old `bin/herdr-graph` has no `adopt` commands. After `setup codex`, the user must trust the new hook in an interactive Codex session.
2. List what is unmanaged: `herdr-graph roster --live --unmanaged`.
3. If the user has not named a tab, ask which one to adopt first. Adopt one tab at a time; never batch.
4. Run the `adopt` skill on that tab. It proposes each change as a plan. Show each plan to the user verbatim and apply it only after their explicit yes. The plan output gives the id and hash:

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

**4. Adopt the next tab.** Same sentence, with `impl`. This time the agent also proposes who manages the seat (`herdr-graph plan manager set impl main`), and quotes the messages that show it.

**5. Check.** `herdr-graph roster --live --unmanaged` now lists only `review` and `shell`. `herdr-graph roster` shows the two seats.

Stop here for as long as you like. A half-adopted space is a supported state.

## What changes once a tab is managed

- Closing the tab retires its seat. Undo reopens the tab and restarts its agent: `herdr-graph undo` in a terminal lists recent actions and lets you pick one to reverse; an agent uses `herdr-graph plan undo <action>`.
- Renaming the tab in Herdr renames the seat, and the other way round.
- If the agent exits and the pane stays, graph starts an agent in it again, resuming the last session. Shell seats are left alone.
- The node manager's tab is reopened if you close it.
- Undo of a closed tab brings back the same threads seat, and the undo preview says how much comes back with it:

  | Time since the close | What returns |
  |---|---|
  | under 24 h | everything: memberships, history, pending receipts and invitations |
  | 24 h to 7 d | memberships and history; other parties must resend receipts and invitations that were still pending |
  | over 7 d | a fresh threads seat; graph re-invites its own channels, other memberships are lost |

- A Herdr restart needs nothing from you. Graph puts every managed seat back on its pane and restarts its agent; allow about 90 s.

## Back out

```sh
herdr-graph plan seat release <seat>            # one tab back to unmanaged
herdr-graph plan teamspace release <teamspace>  # the whole space
```

Both are plans you confirm like any other. The tab, its panes and its agents keep running, and the threads memberships stay. Graph keeps the history and transcripts of the released seat.

The node manager can be released only while it is the graph's only live seat. Otherwise move the role first: `herdr-graph plan node-manager set <seat>`. Releasing a teamspace that holds the node manager works only when no live seat exists outside that teamspace.

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
- **Graph cannot connect to threads**: `doctor` fails on its `threads` line when the herdr-threads daemon is older than 3.6.0. Upgrade herdr-threads and restart its daemon.
- **`restart held` or `resurrect held` in `herdr-graph status` or `doctor`**: graph could not put a seat back after a restart or an undo. The row names the seat, the pane, the reason and the one command that fixes it. It clears once the seat is back.

Reference: README, [Adopting existing Herdr spaces](../README.md#adopting-existing-herdr-spaces).
