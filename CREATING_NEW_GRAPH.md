# Creating a software-factory graph

This is a high-level guide to defining a graph instance for the organization described in [SOFTWARE-FACTORY-VISION.md](SOFTWARE-FACTORY-VISION.md). It distinguishes recommended factory conventions from current graph capabilities. [CURRENT-DESIGN.md](CURRENT-DESIGN.md) records the approved foundation contract; [GRAPH_STATE.md](GRAPH_STATE.md) describes its on-disk state.

Define team compositions, profession instructions, a factory rulebook, and project-specific bindings. Graph creates the identities and runtime bookkeeping when you instantiate them.

## Team compositions

Start with these compositions:

| Team template | Seats | Initial activation |
|---|---|---|
| Project team | Foreman, secretary, lead researcher | Foreman active; others dormant |
| Feature group | Designer, engineer, reviewer; researcher as needed | Activate according to the work |
| Investigation group | Lead investigator plus selected specialists | Lead active |
| Maintenance group | Steward; incident/reliability specialists as needed | Steward active when needed |
| System services | Summarizer | Activated for processing work; its own summaries disabled |

Office and academy can follow once several projects need shared coordination or knowledge. Feature and investigation groups are applications inside a project teamspace, rather than nested workspaces.

For each team template, define:

- Composition: stable member IDs, names, which seats are reused versus created, and initial activation. Record concrete reuse choices when instantiating the application.
- Communication: shared discussion threads and which members participate.
- Defaults: harness, model, launch arguments, and transcript-summary eligibility.
- Operating practice: purpose, coordination owner, escalation destination, staffing criteria, and conditions for retiring the group or handing off its commitments.

Operating practice belongs in instructions/rules. Current graph template relationships encode thread participation; foreman authority and escalation policy need explicit instructions.

## Reusable seat templates

Define reusable seat templates for professions such as researcher, designer and engineer. Team-template members reference these definitions and specialize them. There is no separate profession-role object layer. Sharing a definition creates independent seats on instantiation; sharing an actual seat requires explicit reuse.

For each reusable seat template, define instructions and runtime defaults:

| Profession | Essential instructions |
|---|---|
| Foreman | Own direction, assemble collaborators, route decisions, track commitments, identify acceptance/release owners |
| Secretary | Preserve decisions, triage incoming information, route messages, maintain discoverable handoffs |
| Lead researcher / investigator | Maintain evidence and uncertainty, investigate questions, publish supported conclusions |
| Designer | Explore intent with the human, record decisions and rationale, produce implementable designs |
| Engineer | Execute owned work, use isolated worktrees, verify behavior, deliver reviewable changes |
| Reviewer | Evaluate against intent and evidence, distinguish defects from preferences, report unresolved concerns |
| Steward | Maintain system knowledge, investigate incidents, own authorized monitoring and recovery |
| Summarizer | Dispatch processing, track successful coverage, scan leftovers, handle missing inputs |

Each instruction set should also specify authority boundaries, expected outputs, collaboration practices, context-loading instructions, and recovery/handoff behavior.

The reusable-seat-template model is implemented. Both kinds share `TemplateId` and the `templates/<name>/` namespace: `kind = "seat"` supplies reusable instructions/defaults; `kind = "team"` contains members/relationships and is the only kind accepted by application apply. Omitted kind reads as team for existing records. Each team member can use a typed `seat_template = "tpl_…"` reference and `responsibility` text; unknown references and references to team definitions are rejected. Template kind cannot change during an edit.

Operational markers use `system_duty` (`system`, `cron`, `dispatcher`, `summarizer`). Existing `role` fields and CLI `--role` remain read aliases; `--system-duty` is canonical. The unused profession `role_ref` is ignored. Designated summarizer discovery and source-summary eligibility retain their behavior.

## Instructions for an instantiated seat

Recommended instruction structure:

```text
Factory-wide rules
  + project rules and knowledge references
  + reusable seat-template instructions
  + this template member's responsibility
  + this instance's scope and bindings
  + explicit seat overrides
```

This is a recommended composition convention, not an existing automatic instruction renderer. The instance-specific part answers: which project/feature, who coordinates it, what is authorized, which threads to use, and where its work/evidence live. Keep task progress in Beads and knowledge in the project's existing knowledge system.

`/seat` exposes live references to reusable template records/instructions, every participating application's member specialization/responsibility, application records, the seat record/context and scoped rules. It does not interpolate project names or produce a semantic brief. New reusable members keep specialization in the template; legacy inline members retain initial member-AGENTS copying into seat context. Existing context files are preserved.

Use `herdr-graph plan template create|edit <name-or-id> --from <file.toml>` followed by the ordinary confirmation/apply flow. The create/edit document accepts root and member `agents_md` strings. Omission keeps an existing file; an empty string clears it. Direct `content write` to a reusable template's or actual member's instruction file is rejected: instruction changes need template revisions, a reviewed effect preview and undo. Other opaque content remains writable under the ordinary content rules.

Approved runtime precedence for reusable seat templates:

```text
seat instance override → team member override → seat template default
  → team template default → graph default
```

This order applies to harness, model, arguments and summary eligibility. Explicit empty arguments override inherited arguments. When an existing seat is explicitly reused, its canonical template reference controls runtime configuration; the additional application contributes instruction context and membership.

Applicable fields fall back to built-in defaults when none is configured. This runtime precedence is separate from the recommended instruction structure above. Live definition/reference changes and instruction edits appear in reviewed plans and apply immediately through reconciliation, including required session replacements for surviving reused seats. Explicit overrides remain in force. Undo restores prior configuration/instructions and detects conflicting later edits; native resume remains limited by the harness.

## Other state and materials

Beyond templates, define:

| State/material | Contents |
|---|---|
| Factory rules | Delegation, approval, acceptance/release, escalation, attribution, review effort and retirement policy |
| Project teamspace | Project name, repository association, project rules, knowledge/work-system references |
| Application bindings | Application name, selected template, member-to-seat mapping, reused seats, additions and exclusions |
| Threads | Automatically managed team/seat channels; explicit feature, investigation, announcements or intake discussions |
| Task recipes and skills | Investigation, feature delivery, defect repair, maintenance; expected evidence and handoff conditions |
| Recovery references | Monitoring jobs, cadence, authorization, last processed position and renewal practice |
| Instance defaults | Default harness/model and designated summarizer seat |

Some of these are graph records; others are instructions or references to external systems. Graph supplies organizational identity, template applications and participation intent. Beads owns project work, herdr-threads owns actual memberships/messages/receipts, and the chosen knowledge and scheduling systems own their respective records. Linking those materials does not move their ownership into graph.

You generally do not hand-author clone/session history, transcript indexes, processing requests, or undo/operation records. Graph produces those.

The shipped `project-team` and `feature-team` templates are starting examples: they contain foreman/researcher/engineer and engineer/reviewer respectively. Both teams reference the same reusable engineer definition and create independent seats. Engineer, researcher, reviewer and designer seat definitions are supplied. The factory described in the vision needs richer compositions and operating instructions.
