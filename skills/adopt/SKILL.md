---
name: adopt
description: Adopt, onboard or migrate one existing Herdr tab or session into herdr-graph without relaunching it. Reads the live tab, its transcript, its threads history and the graph, then proposes teamspace adoption, seat adoption, instruction drafts and hierarchy changes, each as an ordinary plan. Use when the user asks to adopt, onboard or migrate an existing tab or pane into the graph.
---

# adopt

Bring one live session under the graph. Nothing is relaunched. Every change is a plan the user confirms.

## Steps

1. Pick the session. The user names one tab or pane. Adopt one session at a time and never batch.
   `herdr-graph roster --live --unmanaged` lists what is still unmanaged (ids and labels only).
   Sessions the user does not pick stay unseated.

2. Gather. Read only; change nothing.
   - `herdr-graph adopt inspect <tab|pane>`: ids, labels, cwd, git toplevel, agent kind, argv,
     native session id, transcript path, threads seat, status and memberships.
     If it reports a managed pane, stop: the tab already belongs to the graph.
   - The native transcript at `session.transcript_path`. For Codex the path may be absent: say so
     and continue with threads history only. Find the task brief, the user's standing instructions
     and repeated corrections, who handed this session work, whom it reports to and whom it directs.
   - Threads history of the pane's seat: its memberships come from `adopt inspect`. Read recent
     messages in its pair and handoff threads with `herdr-threads thread list --joined`,
     `herdr-threads thread participants <thread>` and `herdr-threads read <thread>`.
     Reading never ACKs; do not ACK, accept or send anything.
   - The existing graph: `herdr-graph roster`, `herdr-graph list seats`, `herdr-graph show <seat>`,
     `herdr-graph instructions show teamspace <ts>` and `instructions show seat <seat>`, plus the node
     manager and manager pointers the roster shows.
   - The instance path: `herdr-graph status` prints `instance: <path>`. Drafts go under
     `<instance>/.graph-local/adopt/<tab-label>/`.

3. Propose, in this order, one plan at a time. Build each with `herdr-graph plan --json <change...>`,
   show `rendered` verbatim with its warnings, and apply only through the relay flow of the `graph`
   skill: `herdr-graph apply <plan_id> --confirm <hash> --confirmed-by user-relay` after the user's
   explicit yes. Skip a step that does not apply.
   1. `herdr-graph plan --json teamspace adopt <workspace-id> --name <name> --project-repo <repo>`
      when the workspace is not a teamspace yet. The repo is the common git toplevel of the
      workspace's pane cwds. When it is not unique, leave `--project-repo` out and say why.
   2. `herdr-graph plan --json seat adopt <tab-id> --name <name> --harness <harness>`.
      Explain the observed harness, model and args it records and that nothing is relaunched.
   3. Instruction drafts. Write `AGENTS.md` (the role) and `rules/<name>.md` (standing rules) under
      the drafts directory, show them, and let the user edit them. Then
      `herdr-graph plan --json instructions set seat <seat> AGENTS.md --from <draft>`.
      A rule shared by several seats of the space goes to
      `herdr-graph plan --json instructions set teamspace <ts> rules/<name>.md --from <draft>`.
      Never copy a repo `AGENTS.md` or `CLAUDE.md` or the user's global rules: they load natively
      and importing them duplicates them.
   4. Hierarchy. The first adopted seat of a graph is the top: the graph makes it the node manager,
      as with the first created seat. Say so and propose nothing. For each later seat propose
      `herdr-graph plan --json manager set <seat> <manager-seat>` and any change to existing pointers
      its role implies: `manager set <teamspace> <seat>` for the default manager,
      `manager set <earlier-seat> <this-seat>`, `node-manager set <seat>`. Each is its own plan,
      with the evidence quoted (message ids, transcript lines). A `main` or `mod` tab label is
      supporting evidence only. The graph refuses cyclic chains; do not work around a refusal.

4. Finish. Tell the user the adopted agent picks up graph instructions at its next `/seat`
   (prompted by its seat-channel invitation or its next SessionStart). Thread memberships are kept
   as they are. List them for information only; do not record them as graph participation.

## Rules

- Never apply without the user's yes.
- Never edit graph state outside plans.
- Never adopt more than the one session the user chose.
- Infer nothing silently: quote the evidence for every proposal.
