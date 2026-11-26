---
name: template-capture
description: Capture, extract or generalise a team template from one or two adopted teamspaces. Reads the seats, their instructions, manager pointers and shared threads, then drafts a team template, seat templates, member AGENTS.md and manager set plans for the user to edit. Use when the user asks to capture, extract or turn an adopted teamspace into a template.
---

# template-capture

Turn what an adopted teamspace has grown into a reusable team template. Capturing never changes the
source spaces. Every change to the graph is a plan the user confirms.

## Steps

1. Pick the source. The user names one adopted teamspace, or two. Two spaces are compared, never merged
   silently.

2. Read. Change nothing.
   - `herdr-graph list seats` and `herdr-graph show <seat>` for each seat of the space: harness, model,
     args, overrides and the `responsibility` when one is set.
   - `herdr-graph instructions show seat <seat>` for each seat and
     `herdr-graph instructions show teamspace <ts>` for the space. The roster shows each seat's manager
     pointer and its place in the hierarchy.
   - Thread participation. Only the seats' graph participation becomes a template relationship;
     threads memberships that exist only in `herdr-threads` do not.
   - Where a seat's instructions are thin, read its transcript or summaries to find its actual role. Quote
     the evidence in your notes.
   - The instance path: `herdr-graph status` prints `instance: <path>`. Drafts go under
     `<instance>/.graph-local/template-capture/<name>/`.

3. Draft, under that directory.
   - `team.toml`: `kind = "team"`, one `[[members]]` per seat with `name`, `responsibility`, `startup`
     (`active` for a seat that is active, else `deferred`), `[members.defaults]` with the seat's harness and
     model, and `agents_md` generalised from the seat's `AGENTS.md` with project specifics (repo names,
     paths, ticket ids, people) removed. The team-level `agents_md` holds the teamspace rules every member
     shares. `[[relationships]]` records shared threads.
   - `seat-<role>.toml` (`kind = "seat"`): one per role that recurs, meaning two or more seats, or the
     same role in both spaces. A seat template holds the role's `agents_md` and `[defaults]`.
   - `managers.txt`: a team template cannot declare member managers (a template has no manager field, and
     the only relationship kind is `thread_participation`). List instead the plans to run after
     `application apply`, one per line, as `manager set <member-seat> <manager-member-seat>`, each with
     the source pointer it mirrors.
   - With two spaces, keep what they share and list everything that differs in `differences.md`: member
     sets, instructions, startup, defaults and pointers. Do not pick a side silently.

4. Show the drafts. The user edits them. Do not apply anything before they say the drafts are final.

5. Apply, each as its own plan through the relay flow of the `graph` skill: build with
   `herdr-graph plan --json <change...>`, show `rendered` verbatim with its warnings, and run
   `herdr-graph apply <plan_id> --confirm <hash> --confirmed-by user-relay` only after the user's explicit yes.
   1. For each seat template: `herdr-graph plan --json template create <seat-role> --from seat-<role>.toml`.
   2. Put the new template ids into `team.toml` as each member's `seat_template`.
   3. `herdr-graph plan --json template create <name> --from team.toml`.

6. Optional and separate, only when the user asks:
   - Attach the source seats to the new template with
     `herdr-graph plan --json application apply <name> --teamspace <ts> --name <app> --reuse <mem>=<seat>`
     (one `--reuse` per member). Warn that live template references then drive those seats, and show the
     session replacements the plan prints.
   - Then the `manager set` plans from `managers.txt`, for the seats of a fresh application.

## Example team.toml

```toml
name = "feature-team"
kind = "team"
agents_md = """
Work in small reviewed changes. Report to your manager when blocked.
"""

[defaults]
harness = "claude"

[[members]]
name = "engineer"
responsibility = "Implement scoped tasks and report results"
startup = "active"
agents_md = """
You implement one task at a time and hand it to the reviewer.
"""

[members.defaults]
model = "opus"

[[members]]
name = "reviewer"
responsibility = "Review finished tasks before merge"
startup = "deferred"

[[relationships]]
kind = "thread_participation"
thread = "team-handoffs"
members = "all"
```

## Rules

- Never apply without the user's yes.
- Never change the source spaces, their seats or their instructions.
- Never copy a repo `AGENTS.md` or `CLAUDE.md` or the user's global rules into a draft: they load natively.
- Infer nothing silently: quote the evidence for every generalisation you make.
