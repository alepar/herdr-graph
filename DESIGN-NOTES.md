# herdr-graph — discussion notes

Updated 2026-10-02. The user authorized implementation through a new Claude Opus 5.5 `implementor` session using `super-auto` on 2026-10-02. Earlier discussion-only restrictions are historical. The original user seed remains verbatim in [original seed](archive/HANDOFF.md). These notes distinguish user decisions from proposals.

> Current reading guide: [CURRENT-DESIGN.md](CURRENT-DESIGN.md) consolidates the latest decisions. This file retains discussion history; later explicit corrections supersede earlier proposals.

## User decisions and direction

- Build a generic mechanism to instantiate, track, list, discover, load, retire, and resurrect domains and seats. Particular graph shapes, professions, hierarchies, and escalation rules are configurable. A small set of enforced primitives remains to be designed.
- Spaces represent teams/domains, such as projects. Domains and their seats each retain persistent state. A global registry supports discovery and routing to the responsible fence.
- Revised identity model: teamspace maps to a Herdr workspace; seat maps to a tab; clone maps to a pane; agentic session is the replaceable native conversation occupying that clone. Each preceding identity outlives the next. Several clones of one seat may be active concurrently. This supersedes the earlier one-seat-per-pane and single-active-occupant proposals.
- Clones share the seat's role/context and support persistent side conversations: one may execute work while another brainstorms with the user. Exact shared versus clone-local state and subscription behavior remain to be designed.
- Join and leave scope is explicit in graph API/CLI: whole seat or individual clone. Seat-wide participation persists in graph state beyond any clone's lifetime. New clones inherit only seat-wide subscriptions, not the union of peers' individual memberships. Individual leave preserves peers' memberships and records a clone-specific opt-out from a seat-wide subscription, so load does not immediately undo it. Whole-seat leave removes the seat-wide subscription and leaves the thread for all current clones; future clones do not inherit it. This supersedes both union inheritance and clone-only voluntary join defaults. How a later whole-seat join treats existing opt-outs remains to be settled.
- Graph owns the seat/clone subscription model, resolves required threads during load/hydration, and handles invitations to current clones. Keep clone/group concepts out of herdr-threads for now: its existing pane-bound participants correspond to graph clones, with actual membership and receipts tracked individually there. Graph records intended seat-wide participation; invitation, acceptance and actual membership can temporarily differ and must not be conflated. No native herdr-threads participant-group feature is required by this design direction.
- The system creates a public, discoverable seat channel and automatically invites every clone of that seat. It also creates a public, discoverable teamspace channel. These supersede the prior private/hidden seat-system-thread direction. Thread membership remains explicit under herdr-threads; public visibility does not imply plugin-authenticated authorship. System-channel membership is mandatory/unleaveable. Automatic teamspace-channel invitations and whether these channels admit ordinary discussion remain to be clarified.
- Beads and other work must carry seat, clone, and agentic-session attribution. Claiming and contribution history must distinguish concurrent clones and successive native conversations within each clone.
- User wants Herdr workspace/tab renames reflected in graph teamspace/seat names and corresponding state paths. Retain prior names and rename timestamps in state so history can be traced. Durable IDs stay stable. The timestamp should distinguish plugin observation time from any authoritative event time; do not invent an exact rename time after missed events. Pane names do exist in inspected Herdr 0.9.1 (`pane rename`), but the tab/seat choice is the adopted organizational model, independent of that capability.
- Provide an API/CLI to resolve the caller seat's current state path, used at initial load and whenever the agent consults or updates its state. Do not make cached paths authoritative.
- Seat channels carry programmatically generated notifications of renames, state-path changes, and other relevant plugin changes. Notify every affected seat, including when a teamspace rename changes multiple seat paths. All ordinary and system-created seat/teamspace channels are public and discoverable per the user's revised decision. Exact plugin-authorship support must be checked against herdr-threads; private access control is no longer required for these channels.
- Teams remain flat and map one-to-one to Herdr workspaces ("teamspace"); teams do not contain teams. Cross-team organization uses relationships. Checked installed workspace CLI and [Herdr concepts](https://herdr.dev/docs/concepts/) / [CLI reference](https://herdr.dev/docs/cli-reference/) on 2026-09-27: workspaces are top-level containers for tabs/panes. Herdr additionally groups linked Git-worktree workspaces under a primary workspace; these remain distinct workspaces, not arbitrary nested teams.
- Professions describe a kind of responsibility. The user's example project has standing foreman, secretary, and lead researcher seats, plus feature-specific researchers/designers/engineers that retire on completion. This is an example configuration, not a required topology.
- A seat's responsibility/fence is a plain-text description, interpreted by agents when deciding ownership and routing work.
- Retirement archives seat state. Resurrection loads it and rejoins persistent threads. Thread retirement/rejoining notices are useful to participants. Organizational continuity need not preserve the old mailbox participant ID or rewrite old receipts.
- A load skill uses Herdr space/tab/pane names and context to create, resume, or resurrect a seat. Ambiguous matching behavior is not yet decided.
- Domain templates define initial teams and relationships; profession templates supply rough responsibilities/fences. Agents can edit them and tailor the organization at the user's request.
- Template edits become available to existing seats through the current template; there is no automatic propagation mechanism. `/load` initializes context from the latest template. For live sessions, the editing agent may choose case by case to announce the change in a global or team PSA channel subscribed to by everyone in that scope; recipients reread the template. Template copies support experiments/canaries and later reintegration. Local adjustments and conflicts remain open.
- One separate Git repository is intended to contain graph state, templates, hydrated instances, and a shared Beads database across projects. Project source repositories do not own bead tracking. Database storage, Git representation, and concurrent writes remain design questions.
- Graph state is authoritative in human-readable files checked into the graph instance's Git repository. Templates, rules, domain/seat records, and a compact plan execution journal follow that direction. Beads owns tracked work; herdr-threads owns conversation history, referenced from graph files. Exact file formats/layout and Beads' Git representation remain to be designed; no separate authoritative graph database is planned.
- The graph repository has one shared main worktree used by all seats. Seats carefully select their own files/chunks when committing to minimize conflicts; no per-seat graph worktrees. Shared Git index/commit coordination remains an implementation detail to design, since disjoint files alone do not isolate staging.
- Seats remain in their teamspace while working in external code-repository worktrees. Those worktrees need not be registered as Herdr workspaces. Teamspaces are not inherently bound to a code repository; optionally associate one with a work-project repository and a worktree within it. That association is distinct from the graph state repository and does not relocate bead tracking.
- Bead labels should track the implementing agent, seat, and transcript. Exact representation and multiple-contributor attribution remain open.
- Seat instruction: "we trust our peers to do the right thing and make the right judgements". Preserve the seed's collaborator stance: investigate mistakes, repair them, and maintain trust.
- Do not assume hcom as a dependency. herdr-threads is the intended messaging foundation, under development in its own project.
- The seed's "MCP-only comms" means a strong preference for herdr-threads with attributed seat/agent communication, rather than sending terminal input that appears to come from the user. It is not a blanket requirement that all communication use MCP transport.
- Threads and their membership belong in template composition. Team hydration can reference an existing global PSA thread, create a team-specific thread, and arrange for all team seats to join both. A profession-specific subscription can connect the secretary to an existing global mailman thread for external-message notifications. These are example configurable subscriptions, not mandatory organization-wide channels.
- Graph templates describe intended thread membership; actual invitations, acceptance, and receipt semantics remain owned by herdr-threads. Hydration must not claim an absent occupant has accepted or acknowledged anything. Exact handling of dormant seats and future added seats remains to be designed.
- Each seat's hydrated state contains its resolved list of channels to join, including team-wide channels for seats added later. The graph plugin creates threads as needed and manages references to existing threads, with agent judgment guided by the shipped hydration skill where necessary. Meaningful graph names may map to transport-owned thread IDs; the exact API integration remains to be verified.
- Shipped operational skills are an essential part of the graph plugin. Derive the minimal necessary skill set from the agreed operations; do not assume every operation needs a separate skill.
- A composed hydration can be a named instance whose seats and threads can be listed, retired, or resurrected together. Templates contain agent instructions for deriving names from hydration invocation parameters and context, such as seats `eng.auth` / `prof.auth` and channels `project-a/auth` / `project-a/auth/subteam`. Channel path segments do not introduce nested teamspaces. Exact name scope, collision handling, and instance lifecycle behavior remain open.
- Repeated hydration of the same name has no universal reuse/resume/create rule. The hydrating agent decides case by case from intent and existing state.

## Structured plans and recovery — agreed direction

### Follow-up decisions and proposals — 2026-09-28

Latest decisions supersede earlier alternatives in this section:

- User approved connection-bound programmatic authority in herdr-threads: graph opens a persistent local socket connection and registers as the system participant; only that connection may perform system operations, with at most one active system participant. Prefer this cooperative approach over distributing a reusable service credential. Registration identifies claimed intent, not proof of executable identity. Disconnect releases authority; reconnect requires explicit registration. Reject competing registration/takeover while the incumbent remains active. Explicit recovery of a stuck connection is allowed in principle; its control surface remains to design. Durable system authorship should survive connection replacement, while connection authority does not.

- Identity inspection correction: current herdr-threads root design has a 2026-09-28 superseding cooperative-caller amendment. Participants supply contextual claims checked against instance/binding state; adversarial native main-agent proof is no longer required. The earlier graph explanation overstated that requirement by citing historical clauses. No reusable participant secret/API-key model was found in the inspected authority protocol. Internal mutation permits are request-bound daemon objects, not participant credentials. A separate programmatic-account credential would be a new capability; operator boundaries remain separately limited.

- System channels are unleaveable by agentic participants. Ordinary individual/whole-seat leave must reject system-channel targets. Retirement/cleanup semantics remain distinct from voluntary leave.
- All graph-state mutations, including non-structural content, go through a meaningful plugin API/CLI. Validate mutations there and serialize writes with renames; no state-validation Git hook is required for the normal workflow. Files remain human-readable and Git-tracked. The API decouples callers from storage format. Expected revisions remain proposed protection against stale edits. Out-of-band file changes/Git restores need validation before use.
- Required herdr-threads extension: registered programmatic accounts, programmatically imposed membership that agentic participants cannot leave, and distinct service authorship. Impersonation should be prevented where practical, otherwise deliberately difficult and clearly warned against. Registration uses the exclusive live connection described above, not a reusable service secret; membership acceptance semantics remain to design; imposed membership must never fabricate an explicit message receipt.
- Confirmed against Herdr 0.9.1 source: `tab.move` accepts tab ID and insertion index and reorders within its existing workspace. It has no destination workspace. `pane.move` can cross tabs/workspaces, so individual clones can still cross organizational boundaries.

- User selected notification-driven whole-seat leave: remove seat-wide participation intent, have the requesting clone leave, and publish a seat-system-channel instruction for other clones to leave individually. Track actual completion separately; a received notification is not proof of leave. Preserve pending work for unavailable clones. Publication still depends on the graph-to-threads service-author integration. System channels cannot be leave targets, resolving the earlier system-channel departure edge case.
- User adopted serializing all graph-state writes, including non-structural updates, through the plugin to coordinate with renames. This supersedes the earlier direct-edit preference. Proposed contract: mutations address durable IDs, include expected revisions, and execute through the same writer as observed renames; files remain human-readable and Git-tracked. Templates/rules/notes can still be edited as text via submitted patches. Direct out-of-band writes cannot receive the same concurrency guarantee.
- Historical decision: staged semantic validation through a pre-commit hook was considered, then superseded by mandatory plugin-mediated writes with validation at the API boundary. No validation hook is required.
- Rechecked current herdr-threads root design and protocol on 2026-09-28: participant identity exists and ordinary send/create/invite operations use participant caller context; the cooperative amendment supersedes historical native-proof requirements. Operator commands remain narrowly scoped. The gap is an external graph service author, not absence of thread or participant IDs. No live agent consultation was necessary to establish this documented boundary.

- Use the same structured plan mechanism for graph mutations, including hydration and recovery. The agent compiles concrete operations: create/reuse a workspace, create tabs/panes, bind seats, launch selected harnesses, establish thread references, and deliver initial instructions as appropriate. Other lifecycle operations use the same mechanism within their eventual authority rules.
- The plugin validates the plan against recorded and live state and reports inconsistencies, such as existing objects or invalid references. A clean plan proceeds to execution without an additional routine confirmation step; detected preflight inconsistencies return to the agent for resolution.
- Execution is best effort because live Herdr state can change independently. Preserve results of successful operations and flag failures back to the agent. Treat this as a validated batch with recorded progress, not an atomic transaction across Git, Herdr, threads, and running agents.
- The agent inspects the failures and determines what is still missing. It authors a smaller concrete repair plan and submits it through the same validation/execution mechanism. Do not blindly replay the original plan or assume name collisions authorize adoption.
- Batching primitive operations is expected to reduce model/tool round trips and repeated output. Context savings remain a hypothesis to measure; useful diagnostics and recovery evidence must remain available.

Proposed execution details still to settle: dependency handling (continue independent work and skip dependent operations after a failed prerequisite), compact per-operation outcomes with actual IDs, durable operation identity, and explicit unknown outcomes requiring inspection before retry. Launch or prompt submission alone cannot establish receipt or completed work.

## Activation and composition — user decisions

- Initial activation is decided by the user or another agent. Provide load and discoverable pending work; creating a seat does not automatically launch an occupant or schedule wakes.
- Team creation supports mixed startup choices from its template: some seats can be instantiated and activated immediately, while others are activated on demand. The user's project team needs at least one active front-door session (for example, the foreman), which can activate the rest. This is explicit team bootstrap behavior, not a recurring wake mechanism. Whether deferred seats already have individual records or remain template declarations is still open.
- Seats may create additional seats within their domain authority; template membership does not limit a team's eventual composition. Feature-scoped seats can come and go.
- Templates are composable: individual seat templates can form groups, and a group can be instantiated into an existing team, including one created from another template. Instantiation must match the destination's structural type. A team can contain seats; a seat cannot contain other seats. Other node types and valid containment relationships remain to be settled. A template group need not imply an additional runtime organizational node.
- A possible later reconcile skill would compare recorded graph state with live Herdr state and bring them into agreement. This is future direction, not initial implementation scope; desired-state semantics and handling of differences remain open.

## Rulebooks and scoped rules — user direction

- The graph plugin supplies generic primitives and basic operational skills. A graph instance is a Git repository containing its initial state, templates, and instance-specific rulebook.
- Rulebooks/rules are graph primitives containing agent instructions for reacting to situations. Scope can be global, team, or seat. Each seat independently consults its applicable rules. These are behavioral instructions, not plugin-enforced authorization gates.
- Conflict-resolution policy belongs entirely to the graph instance's global rulebook, not the plugin. Lowest-common-parent escalation is a candidate instruction for the user's instance, not built-in behavior. The plugin must not select an arbitrator, impose scope precedence, or require a particular reporting hierarchy. Instance instructions can define those choices and change them.
- Rule distribution, enforcement through agent behavior, and amendment need further design. Preserve the earlier template PSA/load model as relevant precedent, without assuming it fully settles rule updates. Record corrections honestly and avoid turning every incident into a permanent restriction.
- Example policy for the user's graph: a seat may retire another seat only when it is a transitive parent of the target in the configured hierarchy. Peers and seats in parallel hierarchies cannot retire each other on their own initiative. This addresses conflict resolution by removing the other party. Explicit user direction can override the relationship restriction, with confirmation that the target is a peer/parallel seat. This is instance policy, not a hard-coded plugin restriction. Self-retirement and exact approval/delegation mechanics remain unspecified.
- With the revised seat/clone/session model, closing a clone and retiring the whole seat are distinct operations. The instance rulebook will need to express their authority separately.

## Incoming research: Seats and Sunsets

Read Steve Yegge's [Seats and Sunsets](https://yegge.ai/essays/seats-and-sunsets/) (2026-09-15) on 2026-09-27. The user supplied six takeaways for discussion; they are proposals, not approved architecture:

1. Persist seat responsibility, authority, commitments, and history independently of temporary occupants.
2. Boot with a compact brief and retrieve further context as needed.
3. Treat dormancy as normal; creating a seat need not schedule recurring model wakes.
4. Separate ownership/routing from prohibitions. Proposed gates should identify an owner, rationale, and review date.
5. Amend history explicitly; preserve failures and link completion claims to evidence and acceptance.
6. Account for coordination tokens, wakes, and review effort per accepted outcome.

The essay is operator testimony. Model rankings, psychological explanations, and constant-time trust claims are not established guarantees. Briefs cannot replace checks of changing permissions or actual system state. Incident repair need not create a permanent restriction.

## Implications to discuss next

- Distinguish a dormant seat, which still owns responsibility, from a retired seat, whose state is archived. Whether incoming work activates an occupant is configurable policy to decide.
- Template rereads should update effective instructions while preserving accumulated seat state and historical attribution. The user settled notification as optional global/team PSA messages, with current templates read on load; no automatic refresh mechanism is planned.
- Persistent thread history does not by itself recover all outstanding commitments. Load should connect seat work state with relevant threads; old retirement and receipt records remain historical facts.
- Closed beads are status/evidence pointers, not independent proof that an outcome was accepted. User acceptance and any delegated acceptance authority must remain distinguishable.
- Keep the generic mechanism separate from the user's chosen hierarchy and from any proposed wake or gate defaults.

## Open questions retained

### Targeted addressing review — 2026-09-27

Subsequent user revision: the review below examined the former seat-to-pane mapping. Apply its continuity cautions to clone-to-pane bindings; seats now bind to tabs and permit multiple active clones. The old one-current-activation-per-seat proposal is superseded. herdr-threads' pane-bound participant would correspond to a graph clone; thread membership, receipts and per-seat system-message delivery must be revisited accordingly.

User requested a small subagent roast before settling addressing. Read-only review used the pinned herdr-threads identity audit; no new live continuity tests were run. Findings and the following remedies are proposals, not a sealed contract:

- Public pane IDs locate candidate bindings. Renames/reordering preserve a verified live binding, but socket path/instance name plus pane ID cannot prove continuity across restart and address recycling.
- An explicit graph seat ID at launch expresses intent. Copied/inherited context and duplicate launches require a distinct activation generation and atomic checks of one current activation per seat and one seat per pane.
- Inherited Herdr addresses may be stale after moves. Resolve current placement using supported live evidence; crossing teamspaces is a discrepancy to resolve. Offline move followed by restore can erase continuity evidence, so do not guess from names.
- Git rollback, copied state, and stale plans can revive old binding records. Recorded observations are not proof of current occupancy. Binding-changing plans should compare expected generations again at application.
- Proposed load behavior: discover intended seat, verify placement/continuity, automatically load only a unique current binding, and return uncertain matches for an explicit rebind plan. Resurrection retains organizational seat identity but creates a fresh activation generation.
- A usable Herdr server-incarnation identifier and authoritative current-caller lookup still require qualification. Missing evidence must result in ambiguity, not silent adoption. This protects against accidental confusion under a cooperative local-user model; it is not hostile-process authentication.

### Remaining design questions

User approved best-effort runtime observation: event notifications plus periodic snapshot checks to keep recorded Herdr bindings and labels current. A lightweight non-model observer handles bookkeeping; ambiguous recovery remains agent judgment. Aim to cover ordinary cases without requiring a bulletproof identity system. The user's "99.9%" expresses the desired practical coverage, not a measured reliability claim. Poll interval and implementation details remain open. Missing/contradictory evidence should surface uncertainty rather than silently assign a different seat.

Read-only installed API qualification completed: [Herdr identity check](archive/HERDR-IDENTITY-CHECK.md). Herdr 0.9.1 exposes terminal IDs and process observations, but no boot identifier or authenticated current-caller lookup. Terminal IDs change on restore. Conservative mismatch/ambiguity handling remains necessary; addressing is not yet sealed.

- What are the few enforced primitives?
- How do name-derived state paths handle collisions, unsafe characters, and concurrent edits? Tab renames now rename seats, regardless of clone count.
- Which state is shared by all clones versus clone-specific? How does invitation fanout interact with pending invitations and clone creation? System channels become unleaveable after explicit initial acceptance (settled 2026-09-28); acceptance confirms activity at that moment, not continued liveness.
- How is unattended incoming work surfaced to users or agents deciding on activation?
- How do template instructions interact with instance-specific instructions and local changes?
- How are graph/Beads updates coordinated and represented in Git?
- What generic operations support loading, amending, and distributing scoped rule text? Conflict resolution itself is instance policy.

No code, infrastructure, live agent topology, or external communications were changed as part of these notes.

## Threads system identity amendment — 2026-09-28

User requested super-design and attachment to existing threads epic ht-4is, then notification to its main agent. User approved the top split (.29 contract, .30 connection, .31 store/membership, .32 client/integration) and proposed details. Required invitations need explicit acceptance; upgrades from voluntary membership also require explicit consent. Managed thread lifecycle is service-controlled, and service notifications retain system-event no-ACK semantics. Full current design is in the threads worktree at docs/superpowers/runs/2026-09-28-graph-system-identity/design.md with three linked nested designs. See THREADS-IDENTITY-DESIGN-RUN.md for workflow status. No graph or threads implementation code is being changed in this design run.

Threads amendment handoff completed: design/evidence through cb7b150, ht-4is.29–32 opened for implementation, existing .12 sweep extended. Coverage and roast/resolution evidence in the linked run directory. Final notification submitted to herdr-threads/main; implementation was not performed here. Thread ID, not topic/name, determines identity/reuse; duplicate topic with fresh ID is permitted.

## Lifecycle, serialized commits, and reconciliation — user decisions

- Manual closure retires the corresponding graph object, matching explicit graph retirement; archive state rather than delete it. Record retirement mechanism/provenance (agent request, explicit user request, graph API/CLI, observed tab/pane closure). Graph owns retirement effects including thread cleanup and state updates. Exact last-pane/containing-tab closure behavior and reliable distinction between closure and unavailable observation still need specification.
- Cross-tab pane movement is intended eventually to transfer a clone to the destination seat: inherited template/project/seat scope and system threads change, while prior work history persists. Initial behavior: detect the move and mark reload required. Reload replaces the session's applicable graph state with the destination seat's state; do not carry prior seat state forward. Existing transcript/context remains. This is not a claim that a live model forgets its prior instructions automatically.
- Seat shared state and individual clone state use separate files. Work stays in Beads with teamspace/seat/clone attribution; preserve the previously agreed native-session/transcript attribution as well. Graph mutation API/CLI owns concurrent edits; precise admission/version semantics remain to specify.
- Plugin is the only graph-state writer AND Git committer. All mutation requests enter a durable journaled queue and a single worker processes them strictly serially. This supersedes agents independently selecting/staging graph files/chunks for commits. Callers need not synchronously wait for Git commit; admission and completion must be distinct.
- At journal admission, check compatibility with committed state and preceding queued requests where practical, reducing later failures. Later failure notifies the caller through its system thread. Track failed requests and linked resubmissions; remind agents that have not resubmitted, and stop reminders for the original once a replacement is submitted. Exact retry/supersession, cancellation, unavailable/retired caller handling, reminder cadence and queue failure/dependency behavior remain to specify.
- Graph state is authoritative and is updated first. Example: persist the new seat, then create the Herdr pane. A shared durable workflow/reconciliation mechanism brings Herdr, threads and other driven systems into agreement with graph state. Event observations and periodic checks use the same logic as initial application/recovery. User compared this to Temporal-style workflows; no dependency on Temporal was selected.
- This expands the earlier optional future reconcile-skill idea into core reconciliation for recorded intent and external effects. External effects remain best effort and cannot be made atomic with Git. Desired records and observed/applied effects need distinct status; precise effect identity and uncertain-outcome handling remain design work.

### Consequences proposed for discussion, not yet approved

- An observed deliberate manual close must first become a graph retirement intent, or a desired-state reconciler would recreate the object the user just closed. A missing object in a failed/incomplete snapshot is not sufficient evidence of deliberate closure. Closure during a graph-issued operation needs correlation to avoid double retirement.
- Thread cleanup after a pane is gone must use lifecycle/retirement reconciliation, not depend on that absent agent voluntarily leaving. Releasing a required constraint is not itself removing a native participant. Qualify the existing threads retirement path before adding another service authority.
- A queued request's durable receipt is not completion. Return an operation ID with inspectable pending/applied/failed status. Queue projection is provisional; a predecessor's failure can invalidate later admissions.
- Serial execution prevents simultaneous writes, but does not by itself detect a stale whole-file replacement computed by a caller. Define patch/precondition semantics within the API before claiming lost updates are prevented.

## Reconciliation scheduling — agreed continuation

- User confirmed reconciliation is separate from the serialized journal/state/Git writer. External effects do not hold up unrelated graph-state commits; dependent effects still need their prerequisites.
- Reconciliation runs after graph-state changes as well as from Herdr events and periodic checks. It checks both directions: observed manual Herdr changes that should update recorded state, then graph intent versus live Herdr state to drive outstanding effects. The same mechanism extends to other driven systems such as threads.
- Proposed implementation consequence: observations submit mutations through the same state writer rather than editing files independently. Track pending graph-issued effects and their last observations so graph's own creates/closes/renames are not mistaken for contradictory manual actions.
- Still to settle: when both recorded intent and live state changed since the last successful reconciliation, classify a conflict instead of treating every difference as a new manual command. Preserve the user's best-effort stance; no claim that polling reveals who caused every change.
- Retry policy remains open: distinguish transient external-effect failure that the reconciler can retry from a request requiring agent revision. Earlier user instruction on failure notifications/reminders and linked resubmissions remains in force; its precise boundary with autonomous effect retries needs agreement.

## Retry ownership — approved

- Transient external-effect failures (for example, unavailable Herdr or threads) are retried automatically by reconciliation with backoff. They do not require the caller to resubmit unchanged intent.
- Failures requiring a revised request (for example, name conflicts, stale preconditions or incompatible bindings) notify the requesting agent and track a linked replacement request under the existing reminder model.
- Unknown outcomes remain distinct from known transient failures: inspect/recover the external result before retrying a non-idempotent operation. Automatic retry does not imply blind replay.
- Next proposed detail, not yet approved: reconcile current intent, and fence pending effects by object revision/lifecycle so an old creation retry cannot reactivate a subsequently retired seat. Preserve superseded operations in history while stopping their obsolete effects and reminders.

## Latest-intent reconciliation — approved

- User approved reconciliation following the latest committed intent. Superseded operations retain history but stop retrying obsolete effects. External actions check that their intended state is still current; effects already performed are reconciled toward the newer committed state.
- User explicitly accepts that best-effort observation may miss some manual actions. Do not require a complete manual-action log, reconstruct unobserved intent, or claim that periodic snapshots reveal every intermediate action. An unobserved manual action may consequently be overwritten by reconciliation of recorded intent.
- This supersedes the earlier pending status of latest-intent reconciliation. The separately proposed explicit cancellation of failed requests/reminders was not specifically addressed in this reply and remains to confirm.

## Graph scope and retired requesters — approved clarification

- User approved preserving unresolved graph operations after their requester retires, retaining original attribution and exposing them through discovery. Instance rules decide who takes responsibility; plugin supplies explicit reassignment and cancellation. Do not hard-code escalation to a foreman/secretary or silently treat retirement as successful completion.
- User clarified the narrow scope: graph state tracks teamspaces, seats, clones and thread relationships. Most actual work lives outside graph—in native sessions, Beads, project wikis and other project systems. Do not turn shared seat/clone state into a second work tracker, knowledge base or ledger of all commitments.
- Pending operations/reminders described here concern graph's own structural and relationship mutations/reconciliation. External project tasks and knowledge remain owned by their existing systems and can be referenced by stable IDs/links where useful.
- Templates and rulebooks retain their already-agreed role in defining/operating the organization. Durable identity, lifecycle/binding metadata, desired relationships and the minimal operational journal support that role; they do not imply importing project content into graph.
- Explicit reassignment and cancellation of unresolved graph requests are now approved. Cancellation ends obsolete work/reminders while preserving history; it is not evidence that requested external effects happened or were undone.

## Load and deferred activation — approved

- User approved the proposed `/load` flow: resolve teamspace/seat/clone from graph's Herdr binding; for unbound or ambiguous context inspect candidates and propose a create/resurrect/rebind plan; read latest applicable templates, instance rules and seat/clone configuration; reconcile intended thread memberships and expose invitations for explicit acceptance; return a compact identity brief with relevant links and pending graph operations.
- Project knowledge and work context are retrieved according to instance instructions (for example wiki and Beads references), not collected into graph itself.
- A seat may have a durable record before any Herdr tab exists. Hydration records the template's seats; activation creates tabs/panes for those needed now. Activating a deferred researcher loads an existing seat rather than repeating template expansion.
- This settles the earlier question of whether deferred seats remain only template declarations: hydrated deferred seats have individual records.
- An absent tab for a never-activated seat is intentional, not evidence of retirement. Manual closure of an established live tab still retires/archive its seat per the previous decision.
- Next lifecycle detail to settle: represent a never-activated seat separately from an archived retired seat. Proposal: pending activation / active / retired desired lifecycle, with observed runtime availability tracked separately. No requirement that an active desired seat's agent is currently running or healthy.

## Template defaults and seat overrides — approved

- User approved retaining template references plus explicit instance overrides. `/load` uses current template defaults; explicit seat-level overrides survive any template changes until explicitly changed or removed at the seat level.
- Example: template model X, seat override Y, later template model Z -> that seat still uses Y.
- User agreed to named instruction sections or additive instructions rather than automatic merging of arbitrary prose. Exact representation remains open. This configuration precedence does not impose precedence or conflict resolution on instance behavioral rulebooks.
- Proposed next question: template changes to structural declarations versus refreshed defaults. Existing hydrated identities should not be silently created/retired merely by reading new defaults; determine how an agent applies added/removed seat declarations to existing teams through plans.

## Live template reconciliation — user correction

- Existing instances reference live templates. Template edits change their effective desired state, including structural declarations, and reconciliation applies the resulting changes. The proposed exception requiring an explicit update plan for structural template changes is rejected; do not introduce implicit pinned revisions or separate upgrade machinery.
- Seat-level overrides remain durable and take precedence over changed template defaults. Effective desired state is derived from the current referenced template plus instance-owned configuration/overrides.
- Deliberate template versioning uses distinct template identities/names such as `feature-team:v1` and `feature-team:v2`. Copy a template for experiments or significant changes; the skill should suggest that workflow. Small edits to the existing template propagate to its instances. No automatic migration between differently named templates is implied.
- Runtime agents still learn instruction changes through `/load` or optional PSA rereads; automatic structural reconciliation does not mean a live model has reread changed instructions.
- Remaining design question: preserve stable identity and provenance for hydrated template members, explicit overrides and independently added seats. Need define how a removed template member affects its instantiated seat without erasing independently added seats or recreating intentionally retired members. No removal/retirement policy is newly assumed here.

## Structural instance overrides — approved

- User approved durable instance additions and exclusions alongside value overrides. Retiring a template-declared member records its exclusion for that instance, preventing reconciliation from recreating it while the template still declares it.
- Independently added seats belong to the instance and survive template edits. Preserve their provenance separately from template-derived members.
- Effective desired state combines current referenced templates with explicit instance additions, exclusions and value/instruction overrides. These survive template edits until explicitly amended or removed.
- Next proposal, not yet approved: identify template members with stable keys independent of display names, so renaming preserves instance bindings and exclusions. Define removal of a template member separately from local retirement/exclusion and from merely renaming its display label.

## Incoming user notes — 2026-10-01, discussion pending

User supplied the following additions for fit/scope discussion, not automatic implementation:

- Encourage a new seat for a durable user-facing scope/fence/responsibility; collaborators within a fence can be subagents. Encourage threads.
- New-agent conventions: Codex --no-daemon (user hypothesis: otherwise threads identity is lost), possible Claude equivalent; prefer threads; automated session killing needs approval from another agent up the hierarchy. Harness/daemon identity assertion requires current qualification, not assumed fact. Session killing versus clone/seat retirement scope needs clarification; hierarchy remains instance policy.
- Seat history indexed by Beads epics; /seat boot orientation using Herdr context; attribution plus artifact type/visibility and user-accepted milestone metadata. Closed beads should lead to evidence, not constitute independent completion proof.
- Seat folder with AGENTS.md, optional memory Markdown, transcript index/summaries; possible /ruminate. Discuss whether these are referenced instance-managed knowledge or graph-owned content, preserving narrow graph scope until explicitly revised.
- Threads participants trace back to teamspace/seat/clone. Historical attribution must remain traceable after renames/moves/retirement; mapping is graph-owned in the existing integration direction.
- Extensible hierarchy, easy addition of seats, decisions within fences and handoff outside them largely reaffirm prior instance rules.
- New requested recovery capability: undo accidental pane/tab/workspace closure by opening a fresh tab/agent and asking it to restore the archived identity into its current location as appropriate. Fits existing archive/resurrection model, with recovery semantics still to design (selection, batch closure descendants, runtime bindings, threads acceptance and restoration exclusions).

Proposed discussion framing: /seat can be the user-facing entry to /load rather than a separate boot mechanism; history brief retrieves Beads/wiki/transcript references without importing all work into graph. Distinguish seat (responsibility), clone (parallel durable conversation within same seat), subagent (temporary helper). Restore through new desired-state mutations, not whole-repository rollback; preserve closure history and subsequent unrelated changes. None of these refinements is marked approved by this entry.

## Seat bootstrap and transcript services — 2026-10-01

- User chose one entrypoint, `/seat`, with automatic prompting where possible: e.g. HERDR_GRAPH=1 causes session-start hook instructions to tell the seating agent to run /seat. Marker is an enablement hint, not binding proof. Agent discovers/reads its applicable state directly; no plugin-generated semantic orientation brief is assumed.
- Human-browsable folder hierarchy follows graph composition (teams/seats, clone state within seat). Graph provides folder structure and minimal bootstrap. Other memory/work instructions are opaque agent content; existing serialized write/commit rules still apply to graph repository files.
- /ruminate and similar behavior comes from seat instructions, opaque to graph. No recurring model wake schedule selected.
- User proposes graph tracking native sessions/transcript references across sessions and shipping a minimal template for system-duty agents. Proposed workflow: plugin observes session closure, notifies a summarizer via threads, summarizer processes transcript. Users can extend with rule extraction or ruminate/dream agents. Clarify activation, reliable closure detection, transcript finality/continuation and completion tracking before declaring this designed. This intentionally extends earlier reference-only graph scope with mechanical transcript lifecycle/indexing, not built-in semantic summarization.
- Local threads repository check: current source src/harness/setup.rs builds Codex launch argv starting with --no-daemon (line 677 at inspection). Current harness design specifies Codex --no-daemon and actual non-login inherited Herdr context checks for both harnesses. Validation design says Claude runs natively. Codex launch requirement is confirmed in implementation contract; no equivalent Claude flag inferred. This inspection does not independently reproduce a daemon identity-loss experiment.
- Prior threads worktree path no longer exists. Canonical source is now ~/AleCode/herdr-threads; current designs under docs/design/herdr-threads and archived run evidence referenced by archive/herdr-threads-run-2026-09-26. Do not claim old worktree paths remain current.

## Summarizer dispatch and activation — approved clarification

- User approved automatic activation when transcript processing is needed and the summarizer has no running occupant.
- Summarizer's main occupant acts as a front door: receives thread requests, dispatches transcript processing to subagents, and reports completed work back to graph. It need not finish one transcript before accepting another. Dispatch/concurrency/reply behavior belongs in role-specific AGENTS.md instructions, not special graph scheduling logic.
- User rejected a graph/template-specific `activate on assigned lifecycle work` option. Do not introduce a summarizer-specific activation flag or built-in worker pool. Role instructions tell the agent to wait for incoming thread requests and dispatch processing.
- Mechanical boundary still to specify: instructions govern a running occupant; graph must initiate approved activation when its transcript workflow addresses an unoccupied summarizer. Proposal: use the existing generic load/activate operation before posting durable requests; reuse an existing occupant without requiring it to be idle. This does not yet establish that all arbitrary messages to all retired/unoccupied seats auto-activate/resurrect them.
- Subagent dispatch is distinct from successful processing. Pending transcript requests survive occupant loss; explicit completion should link the source transcript coverage and resulting artifact. Exact retry/deduplication and reporting contract remain to design.

## System-duty request/result contract — approved

- User approved a small shared request/result contract for system-duty seats, with task execution instructions opaque to graph.
- Graph tracks durable requests and explicit completion. Results reference output artifacts and the input coverage processed (for transcript work: transcript identity, processed-through position, and summary path).
- A resumed transcript can generate processing for its newly appended portion. Pending requests remain discoverable after a summarizer/occupant crash; dispatch alone does not mark processing complete.
- Graph activates an unoccupied summarizer through ordinary activation, then posts durable requests without waiting for previous summaries to finish. Role instructions own subagent dispatch and result reporting; no special activation option or plugin worker-pool policy is introduced.
- Still to design: stable request identity/retry deduplication, concurrent or out-of-order result handling, and the generic integration boundary between thread replies and graph mutation API completion reports.

## Dedicated undo CLI — user direction

- User rejected making undo a /seat mode: automatic /seat matching/creation conflicts with intentionally restoring a previous object into a fresh location.
- Provide a dedicated undo CLI command. It displays the last action detected by graph and the proposed undo plan, asks a simple [y/n] confirmation, and applies only after acceptance.
- The terminal pane running the command is adopted into the restored pane/tab/workspace arrangement, avoiding a leftover recovery pane. Exact mapping for multi-object restoration and native session launch/resume remain to specify.
- Restoration uses recorded graph history and reconciliation; do not assume a raw Git revert is an adequate undo of external effects.
- Pending scope question: does "last action" mean latest reversible user/agent organizational action across the graph, or a narrower current-context action? Background observations, effect completions and bookkeeping should not accidentally become the undo target (proposal, not yet approved).
- Proposed execution detail: pin preview to a specific recorded action and relevant state revisions; if intervening changes invalidate the plan, show a refreshed plan rather than undoing a different/newest action after confirmation.

## Undo action selection — approved refinement

- User prefers displaying a sorted list of recent actions and letting the user choose, rather than implicitly targeting only the latest action.
- Dedicated undo CLI flow: list recent undo candidates (newest first), select one, display its concrete undo/restoration plan including where the current terminal pane will be adopted, then [y/n] confirmation.
- Retain recorded action identity throughout selection/preview/application. Exact eligible action types, dependency conflicts and already-undone presentation remain design details; do not imply arbitrary historical actions can always be safely inverted.

## Undo scope expansion — user direction

- Initial undo scope includes template hydration and template updates alongside clone/seat/teamspace closure/retirement. User rationale: these have wide impact and are otherwise difficult to undo.
- Proposed contract for discussion: record hydration/update as a grouped action with before/after configuration and provenance for affected objects/relationships. Undo produces a new compensating graph mutation plus reconciliation; preserve history and external project work.
- Hydration undo must distinguish newly created objects from reused objects, and relationships added by this action from pre-existing ones. Template-update undo must account for later template edits and instance overrides rather than blindly restoring an old snapshot. Conflicts should appear in the preview; exact resolution behavior remains open.
- Live template references mean undoing a template update changes effective desired state for current referencing instances, potentially including teams created after the original edit. Preview must make that scope visible; whether/how to preserve selected later instances remains to discuss.

## Grouped undo contract — approved

- User approved grouping each hydration/template update as one action with enough before/after and provenance information to construct its inverse.
- Undo hydration retires objects created by that action and reverses relationships it added; preserve objects merely reused and external project work.
- Undo template update reverses that edit while preserving unrelated later edits and explicit instance overrides. Because references are live, preview all currently affected instances, including those created after the original update.
- Undo creates new graph mutations followed by reconciliation; preserve original history rather than rewriting it.
- Conflicts such as later edits to the same field are explained by the CLI and handed to an agent for a repair plan. Do not guess. Straightforward cases retain recent-action selection, concrete preview and [y/n] confirmation.

## Template-member and instantiated identity — approved

- Template member ID identifies a declaration and survives display-name changes. Seat ID identifies its instantiated seat and survives retirement/resurrection. Display names remain editable and drive Herdr labels/human-browsable paths.
- Instance exclusions reference template member IDs, so renaming a declaration does not recreate a deliberately retired member.
- Copying a template (for example feature-team:v1 to feature-team:v2) preserves member IDs for corresponding roles. An explicit switch between those templates can retain existing instantiated seats and overrides rather than replacing every role.
- Member IDs identify correspondence, not a global singleton seat: separate team instances still have distinct seat IDs. Exact scoping for multiple applications/composition of the same template remains to design.

## Seat identity clarification — user correction

- Seat ID is the independent identity of the instantiated seat itself, including seats created fresh without any template. It is not derived from a hydration instance ID or template member ID.
- A templated seat references its template/declaration as provenance; an ad hoc seat need not have such a reference. The earlier proposed hydration-instance + member-ID mapping must not be interpreted as identity derivation or a required extra per-seat instance identity.
- Grouped hydration action records can reference the independent seat IDs created/reused by that action for undo and provenance. Whether a composed group needs a separate durable runtime identity beyond existing action/provenance records remains unspecified; do not add one solely to identify seats.
- Preserve previously approved stable member correspondence across template copies/renames without replacing instantiated seat identities.

## Safe graph-state changes — approved after design review

- User approved treating each graph mutation as one conditional, recoverable transaction. This addresses the design contracts raised by review findings F01, F19 and F24; it is not implementation or runtime verification.
- `/seat` and reconciliation read one complete committed revision, including across multi-file changes and renames, while the writer prepares the next change. A partially updated shared checkout is not an authoritative read view.
- Requests identify the state they relied on. The serialized writer rechecks preconditions at application, even when admission already checked compatibility. Unrelated intervening changes may proceed; conflicts return to the requesting agent with an explanation for rereading and resubmission. Graph does not merge competing prose or intentions.
- Example: independent changes to different seats may both succeed; two renames of the same seat from the same starting state cannot silently overwrite one another. No lock is held while an agent thinks.
- Each mutation commits its graph changes together. Durable admission means queued, graph commitment means recorded state changed, and external reconciliation remains separate.
- Durable operation identity supports determining after a crash whether a mutation committed. Resume unfinished work without applying an already committed mutation twice; reconcile external effects from committed intent.
- Exact precondition granularity/representation, how callers obtain a consistent committed read view, and the journal-to-commit recovery mechanism remain to specify. The approved observable contract supersedes the earlier wholly open status of stale-edit protection and committed-read/recovery behavior.

## Cancellation and delayed actions — approved after design review

- User approved cancellation and supersession agreeing with current desired intent, addressing review findings F15 and F17 at the design-contract level.
- Before commitment, cancellation removes an uncommitted request from execution without applying its requested state mutation; retain its operational history.
- After commitment, cancelling remaining effects requires a new superseding graph mutation specifying what should now be true. For example, cancel an unfinished activation by leaving the seat dormant. If the replacement state is ambiguous, return it to the requesting agent for resolution rather than guessing.
- Preserve the history of already-performed effects and reconcile toward the replacement state. Cancellation does not silently undo the entire original operation or establish that effects were reversed. It must not leave a hidden cancellation flag fighting the reconciler's authoritative desired state.
- Delayed instructions carry their originating operation and relevant revision. The recipient checks whether an instruction still applies before acting. An obsolete whole-seat leave instruction arriving after a newer join must not be submitted as a fresh voluntary clone opt-out.
- The tradeoff is that cancelling a partially completed operation can require an explicit replacement plan. Exact cancellation/commit race handling, precondition granularity, and instruction/API representation remain to specify within the approved conditional-mutation contract.

## Closure and cascading retirement — approved after design review

- User approved the full containment-based closure contract addressing F02, explicitly including cascading retirement because undo is available in the design.
- Observed pane closure retires its clone, preserving its seat and other clones. Tab closure retires its seat and remaining clones. Workspace closure retires its teamspace and all remaining seats/clones, including dormant seats without running agents.
- A native-agent exit while its pane remains ends that occupancy without retiring the clone or seat. If last-pane closure also removes the tab, the resulting tab closure retires the seat; follow the resulting containment change rather than assuming the tab survives.
- Disconnection or ambiguous observations mark runtime availability unknown without changing organizational lifecycle. Never-activated absent tabs remain intentional. This preserves the approved best-effort observation scope and does not require reconstruction of missed manual actions.
- Record a cascade as one action, retaining which descendants it actually retired and their provenance. Undo must distinguish those descendants from objects already retired before the cascade. Correlate graph-issued closures with observed effects to avoid duplicate retirement actions.
- Graph owns reconciliation of the required lifecycle thread-membership cleanup even after occupants disappear. Preserve messages and historical attribution; cleanup cannot depend on an absent agent voluntarily leaving.
- State is archived rather than deleted. Exact observation qualification, native cascade correlation and threads cleanup APIs remain integration details to specify; no new runtime capability is claimed.

## Undo terminal adoption — approved after design review

- User approved conditional adoption of the terminal running undo, addressing F04 and F11. This supersedes earlier unconditional current-pane-adoption wording.
- When undo restores a runtime presence after closure, reuse the command's current pane for one restored clone. For larger restorations, the preview identifies that clone and which other panes will reopen. Previously dormant seats remain dormant.
- If the recovery pane already has a graph binding, including one created by automatic `/seat`, show that binding and its proposed replacement. Archive displaced identities and preserve their history. If adoption affects unrelated occupants or work, explain the conflict and request an agent repair plan rather than silently displacing them.
- Undoing a template edit or hydration need not restore a terminal. When there is no runtime restoration target, leave the caller's pane where it is unless the undo itself retires it.
- If undo retires the caller's own pane, the preview explicitly states that the terminal will close. Persist the operation and provide its ID before closure so its completion remains discoverable elsewhere.
- Keep the existing action-selection, concrete-preview and [y/n] confirmation sequence. Native session resume versus fresh launch and exact multi-object placement mechanics remain separate design details.

## Template applications and current group membership — approved after design review

- User approved an explicit durable record for each template application, addressing F09 and F13. It identifies a composition that can be inspected and operated on collectively, not an extra identity for each seat or a nested teamspace.
- Each application record has a stable application ID, editable name, live template reference, mappings from template members to independent seat IDs, and its additions, exclusions and relationship contributions.
- Example: `auth-feature` and `billing-feature` can both use the same engineer declaration while mapping it to different seats. Excluding the engineer in Auth affects only that application. Switching Auth to a copied template preserves its seat through stable member correspondence.
- Explicitly reusing a seat records that reuse in each participating application. This does not derive the seat ID from application identity or replace durable seat-level overrides.
- Collective listing and lifecycle operations use current application membership. The original hydration action remains historical evidence for undo rather than the sole record of current membership.
- This settles the previously open need for application-level runtime identity for repeated composition and collective operations. Shared-member retirement, later reuse dependencies, and removal of live contributions remain the next design decisions; no retirement policy is inferred from this record alone.

## Composition withdrawal and later dependencies — approved after design review

- User approved withdrawing an application's contribution while preserving objects that gained an independent use, addressing F18 and F25. This refines the earlier hydration-undo rule based only on original creation provenance.
- Retiring or undoing a composition marks its application retired so its live template stops generating desired structure. Inspect current membership, including members created later by live-template reconciliation, rather than only the original hydration list.
- Retire only members still belonging exclusively to the withdrawn application. Preserve members that existed before it and those subsequently reused elsewhere. Undo may therefore leave an object created by the original action when that object now serves another purpose.
- Remove only the application's relationship contributions. Preserve relationships still required by another application or independently added.
- If later edits or configuration dependencies make the result ambiguous, show the conflict and send it to an agent for a repair plan. The preview identifies both the objects retained and those retired.
- Example: Auth creates an engineer, Billing later reuses it, and a later live-template edit creates an Auth-only reviewer. Undo Auth preserves the engineer but retires the reviewer and withdraws Auth's live contribution.
- These rules apply to withdrawing a composition. Explicit closure of a seat tab or workspace continues to follow the approved containment-based cascading retirement contract.

## Transcript queue, recovery and source eligibility — user decisions, 2026-10-02

- User selected threads as the reliable request queue with ACKs for both idle and busy summarizer occupants. The summarizer ACKs on dispatch to a subagent. This records the integration direction, not an independently verified runtime guarantee.
- Processing can still fail after dispatch/ACK. The role tracks which transcripts were successfully processed and periodically scans for leftovers. This settles role-owned recovery responsibility; ACKs must remain distinct from successful results and graph completion records.
- Periodic leftover scans are now selected for this role, superseding its earlier unspecified recovery behavior. Their cadence and scheduling mechanism remain open; no plugin-managed model worker pool is introduced.
- Add a seat parameter enabling/disabling transcript summaries. System, cron and dispatcher roles have it disabled, including the summarizer in its dispatcher role. This is a source-eligibility parameter, distinct from the previously rejected special activation flag for destination system-duty roles.
- Parameter spelling, ordinary-seat defaults, pending-work treatment on disablement, exact processed-coverage tracking and unavailable-source outcomes remain to specify. Role-owned tracking must integrate with the already-approved request/result contract without treating dispatch as successful processing.

## Beads persistence and inherited work — user clarification, 2026-10-02

- Beads uses a Dolt database whose state is orthogonal to graph's Git-tracked files. Do not design coordination between Beads database writes and graph Git commits based solely on the earlier shared-repository description; this resolves the ownership premise behind F12.
- Only agents explicitly mutate Beads. Creating, retiring or otherwise changing seats does not implicitly mutate bead records or transfer their ownership/status.
- Beads tracks plans sent into execution, with super-code as an example, plus milestones and published artifacts. It is not a graph-managed mirror of all conversation content.
- Bead labels carry the full teamspace/seat/clone tuple. A successor filters those labels to discover inherited tasks; this is the selected work-discovery convention for F14. Existing native-session/transcript attribution remains useful provenance and is not removed by this clarification.
- A different successor identity or an actual reassignment requires explicit agent action in Beads rather than an automatic graph lifecycle hook. Exact label representation and query conventions remain implementation detail.

## Immediate changes and plan confirmation — user direction, 2026-10-02

- User chose immediate application of live-template changes, including changes affecting existing model/harness defaults. This rejects the assistant's suggestion to defer runtime changes to the next activation by default. Exact transition/restart/resume mechanics still need design.
- User directed a Terraform-like interaction: every requested change first displays a detailed plan of what will happen; the user confirms; exactly that change is applied. If intervening state makes the plan stale, produce a new plan and repeat confirmation rather than silently adapting the approved action.
- Undo is the recovery mechanism for accidental mass changes. The plan should make the actual scope of a requested change reviewable; confirmation is user-directed here, not an inferred universal behavioral authorization policy.
- The interaction between this requested-change approval flow and automatic observations, internal processing bookkeeping, summarizer activation and retries of approved reconciliation remains to be specified. Do not invent exemptions or silently remove previously approved automation.
- The separate proposal for member removal/re-addition and retained correspondence has not yet been approved; immediate application specifies timing, not every resulting lifecycle transition.


## Final convergence — approved recommendations, 2026-10-02

- Approve a requested change together with its downstream effects. Progress and retries within that approved plan proceed automatically. Changed assumptions or different effects require a fresh plan and confirmation; unrelated activity does not invalidate approval. Exact bookkeeping/observation representation and how automatic summarizer activation is included in an approved workflow remain implementation design; do not silently grant unbounded activation authority.
- Identify transcript processing by transcript identity plus a fixed input boundary. Record successful coverage and its output reference separately from dispatch ACKs. Out-of-order results must neither erase newer coverage nor conceal gaps. Missing input remains explicitly unresolved so the role's periodic leftover scan can distinguish it from ordinary pending work.
- Show required native-session replacements in the approved plan for immediate configuration changes. Preserve history and resume sessions where supported; do not invent unsupported harness capabilities.
- Distinguish an active seat lacking an occupant from an explicitly retired seat. Pending work can activate the former under the approval contract; resurrecting the latter requires explicit intent. Pending work must not defeat retirement.
- Re-added template members retain instantiated identity when correspondence is clear, while respecting local exclusions. This settles retained identity on re-addition; exact member-removal transitions must be elaborated consistently with application contribution/dependency rules rather than assuming every derived seat is disposable.

## Implementation handoff — authorized, 2026-10-02

- User considers the design converged and requests updating these notes, installing the Herdr skill from `herdr --skill`, and opening a new `implementor` tab running Claude Opus 5.5 with a prompt to `super-auto` this design.
- Implementation is now authorized. Earlier design-only statements do not block implementation. [CURRENT-DESIGN.md](CURRENT-DESIGN.md) is the consolidated contract; these chronological notes provide rationale and superseding decisions. Original visions and the review are context, not instructions to reopen approved decisions.
- The implementor should elaborate remaining schemas, integration contracts and other mechanical choices through `super-auto`, preserving the approved behavior and surfacing material conflicts. Do not restart the memory observer or disturb existing user sessions to test the product.
- Missing optional practices were handed to the separate `brainstormer` session; they are not automatically additions to this implementation scope. See [IMPLEMENTATION-HANDOFF.md](IMPLEMENTATION-HANDOFF.md) for the durable kickoff and validation expectations.
