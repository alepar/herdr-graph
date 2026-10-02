# herdr-graph implementation handoff — 2026-10-02

The user has approved the design and explicitly authorized implementation using `super-auto` in a new Herdr tab named `implementor`, running Claude Opus 5.5. This supersedes historical statements that implementation is not authorized.

## Kickoff

Invoke the installed `super-auto` skill and implement the approved herdr-graph design. Treat the settled design as the input to implementation planning, rather than restarting open-ended brainstorming. Follow the skill's required checks and approval checkpoints; the user did not request skipping reviews or select extra flags. Resolve routine implementation choices autonomously and surface material conflicts with the contract. Do not ask the user to reconfirm previously approved design decisions.

Read in this order:

1. `CURRENT-DESIGN.md` — consolidated current contract.
2. `DESIGN-NOTES.md` — decision history; later explicit corrections supersede older proposals, especially the final October 2 sections.
3. `HERDR-GRAPH-VISION.md` and `SOFTWARE-FACTORY-VISION.md` — intent and examples; current decisions take precedence.
4. `docs/superpowers/reviews/2026-10-01-vision-alignment-roast-design-1.md` and `docs/superpowers/reviews/2026-10-01-vision-alignment-competitors.md` — historical adversarial review and comparative evidence. Many findings were subsequently addressed in the approved design; map them to current decisions instead of treating all findings as still open.

Use the Herdr skill installed at `.claude/skills/herdr/SKILL.md`, sourced from `herdr --skill`. Inspect current Herdr and `/Users/alepar/AleCode/herdr-threads` APIs/artifacts before asserting integration support. Old worktree paths and archived run evidence are not current implementation proof.

## Contract to preserve

- Independent persistent teamspace/seat/clone identities, flat Herdr workspace/tab/pane mapping, replaceable native occupants, and dormant seats.
- Graph owns organizational state and operational bookkeeping. Opaque role instructions own semantic work. Beads Dolt state is orthogonal to graph Git state; only agents explicitly mutate beads. Full teamspace/seat/clone labels support successor discovery.
- One graph-instance repository, one serialized plugin writer, conditional recoverable transactions, complete committed reads, durable operation identity and crash recovery; external reconciliation follows committed intent.
- Requested changes show a detailed plan and require user confirmation of exactly those effects. Progress/retries within approval are automatic. Changed relevant assumptions or effects invalidate approval; unrelated activity does not. Product confirmation semantics do not prohibit writing the implementation already requested here.
- Live templates apply immediately, including runtime defaults. Show session replacements in the plan; preserve history and resume where supported. Retain overrides, additions, exclusions and stable member correspondence. Durable application records own membership and relationship contributions, not seat identity.
- Retiring an application withdraws its contributions; preserve pre-existing/reused objects and independent relationships. Respect explicit containment-based cascade retirement on native closure. Archive history. Re-add corresponding template members with retained identities and respect exclusions.
- Undo uses recent-action selection, concrete preview and confirmation, then compensating mutations. Preserve later independent work, use current dependencies, and conditionally adopt the caller's pane only when restoring runtime presence. Conflicts go to an agent repair plan.
- Threads carry durable summarizer requests; ACK means dispatch to a subagent, not successful processing. The ordinary summarizer role tracks success and periodically scans leftovers. Seat configuration enables/disables source transcript summaries; system/cron/dispatcher seats disable them.
- Processing identifies transcript plus fixed input boundary, records successful coverage and output references, preserves gaps/newer coverage under out-of-order results, and marks unavailable input unresolved. Pending work may activate an unoccupied active seat under the approval contract; explicit retirement requires explicit resurrection intent.
- Graph owns participation intent; threads owns actual memberships and receipts. Never fabricate acceptance or ACKs. Use the approved cooperative service identity contract and validate transport capabilities.

## Implementation elaboration and scope

Choose concrete schemas, language/build layout, storage/journal/read implementation, integration APIs, deduplication, recovery timing, source parameter defaults, label spelling, member-removal transitions and test strategy within the contract. Explicitly account for how automatic activation, observed changes and bookkeeping fit the approved plan boundary. Raise a focused question only if a material product choice cannot be resolved from the approved decisions.

Implement the generic graph mechanisms with the minimal supplied skills/templates and ordinary summarizer role needed to demonstrate them. Software-factory arrangements are configurable examples, not a mandatory hardcoded hierarchy. Optional missing practices were sent to the separate `brainstormer` session; do not silently expand implementation to all competitor features.

Do not restart the memory observer, stop shared Herdr servers, close existing user tabs, or use live user sessions as destructive test fixtures. Use isolated test instances/workspaces and explicit fixtures. No deployment or final merge is implied by this handoff; follow the skill's integration checkpoint.

## Validation expectations

Plan meaningful checks for concurrent stale mutations and consistent reads; crash recovery around journal/commit boundaries; cancellation and obsolete messages; cascade retirement and undo provenance/adoption; repeated templates, shared members, withdrawal and re-addition; immediate runtime replacement plans; exact approval scope/staleness; dispatch ACK versus coverage, gaps, missing input and role recovery; retired summarizer precedence; and Beads/graph persistence independence. Separate verified integration behavior from mocks and assumptions.

The project began as documentation only, without Git or a Beads database. Check `git status`, `bd --version` and `bd list --limit 1` before invoking the pipeline. Use its normal planning, review and execution artifacts. Preserve all design/research history and report progress and any actual blockers in the `implementor` tab.
