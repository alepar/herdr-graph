---
name: seat
description: Resolve this pane's graph identity (teamspace, seat, clone) and load the files that apply to it. Use when a session starts in a herdr-graph pane, when a hook says "run /seat", or when you need to know which seat you are.
---

# /seat

Find out who you are in the graph, then read what applies to you.

## Steps

1. Run `herdr-graph seat --json` and read the JSON it prints.

2. Look at `resolution.status`.
   - `bound`: you are the clone `resolution.clone` of seat `resolution.seat` in teamspace
     `resolution.teamspace`. Continue with step 3.
   - `unbound` or `ambiguous`: the graph cannot tell which seat this pane is. Do not guess.
     Show the user `resolution.candidates` and, when present, `resolution.proposal.rendered`
     exactly as printed. If they agree to the proposal, relay their answer:
     `herdr-graph apply <plan_id> --confirm <hash> --confirmed-by user-relay`
     (use the proposal's own `plan_id` and `hash`). Never apply a proposal without the user's
     explicit yes. If they decline, stop and tell them what is unresolved.
   - A `bound` resolution can carry a `proposal` too: the pane binding is stale and a rebind is
     proposed. Treat it the same way.

3. Read the files yourself. Everything is listed under `paths`:
   - `templates`: live reusable/team template records and instructions, member specialization
     for all participating applications, application records, and the seat record
   - `rules.global`, `rules.team`, `rules.seat`: rules, most general first
   - `seat_agents_md`: your seat's own instructions
   - `clone_files`: files kept for this clone

   Read member `responsibility` alongside its instruction references and your instance context.
   These references are live; no generated brief or string interpolation is implied. Explicit reuse
   keeps canonical runtime configuration while exposing all participating contexts.
   Do not summarize them to the user unless asked; follow them.

4. Check freshness. If `worktree_dirty` is not empty, those files have local edits that the graph
   did not overwrite; the committed version may differ. `view_rev` is the revision the working
   tree mirrors; compare it with `head`. If they differ, the files on disk are behind.

5. Handle `pending_invitations`. Each entry has an exact `accept_command`. Run the commands for
   `constraint: required` invitations now, one by one, exactly as printed.

6. If `reload_required` is true, the graph changed something that needs a fresh session to take
   effect. Tell the user and continue with what you have.

7. Look at `pending_ops`: graph changes requested by your seat that are not resolved. Mention
   rejected ones to the user. For the details use `herdr-graph op <op>`.

8. Inherited work lives in Beads. Run the query in `beads_query`
   (`bd list --label hg-seat:<st>`) and pick up what is assigned to your seat.

The graph never calls `bd` itself, and `/seat` never changes the graph on its own.
