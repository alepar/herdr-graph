# A software factory that knows its projects and keeps its promises

Product vision · Draft for discussion · 28 September 2026

> Subsequent design decisions through 1 October are consolidated in [Current design](CURRENT-DESIGN.md); that summary takes precedence where this earlier vision differs.

*“The factory” is a working description, not a selected product name. This is the second product: an opinionated software organization built on [herdr-graph](HERDR-GRAPH-VISION.md).*

## The promise

**Give an individual or small team a continuing software organization they can work with.** The factory learns the products it serves, turns intentions into owned work, assembles the right collaborators, and carries changes through to outcomes supported by evidence. The human retains direction and taste without having to personally reconstruct context, route every question, or keep every conversation alive.

The experience should feel like working with a capable team: approachable, candid, resourceful, and accountable. It should be easy to ask what is happening, challenge a recommendation, explore a possibility, or change priorities. A team that produces many changes but obscures their value has not fulfilled the promise.

**The product supplies a reusable way to organize software work, with room for each project to develop its own practice.** Profession templates, team compositions, task recipes, and scoped rulebooks make that practice explicit. The human can delegate through a foreman and sustain direct working relationships with designers, researchers, engineers, and other specialists. Those relationships are part of the product’s value.

This is a proposed product expression of the existing vision. The standing project roles, flexible feature seats, durable side conversations, and peer-trust stance come from the [design discussion](DESIGN-NOTES.md) and [designer consultation](research/software-factories-20260928/designer-consultation.md). The detailed rulebook, work journey, evaluation measures, and optional service teams below are proposals for this instance. They do not change the foundation’s settled design or authorize unattended production operation.

## The user and the problem

The initial user has several real projects and enough work to benefit from parallel collaborators. They also have opinions, history, and constraints that are expensive to explain repeatedly. They need research and design as well as code. Their problem is not simply getting an agent to finish a ticket; it is maintaining a coherent direction across many tickets, conversations, and weeks.

The factory should carry this continuity. It knows where decisions live, which questions remain open, who owns a concern, and what evidence would make a change acceptable. It helps the human spend attention on consequential choices rather than on rediscovering the state of the team.

The product begins as a human-led organization with increasing delegation. Different projects can delegate different decisions. A personal experiment may allow broad autonomous iteration; a production service may require a release owner and stronger acceptance evidence. Those are explicit operating choices, not assumptions hidden in the word “factory.”

## A day with the factory

You open a project teamspace and speak to its foreman: “Customers are losing their work when sign-in expires. Find out what is happening and improve it.” The foreman connects the request to existing commitments and brings in a researcher or engineer as needed. If the problem is small, one seat can handle it. If it has a distinct scope you will want to revisit, the foreman creates a feature team from a template.

A researcher gathers evidence about current behavior and the relevant constraints. A designer works with you on the experience. Engineers develop a change in the project’s code worktrees, while the shared work record tracks ownership and dependencies. Review and integration are assigned according to the project’s rules. A discussion thread preserves the reasoning, including disagreements and rejected alternatives.

You can talk directly to the designer without stopping implementation elsewhere. You can also open another clone of a busy seat for a side conversation. The factory records decisions that affect the work and communicates them to the relevant participants. It does not assume that two conversations share knowledge simply because their panes belong to one seat.

The outcome arrives with a short explanation of the behavior change, evidence from the relevant checks, known limitations, and a clear statement of what has and has not been accepted or released. If the result misses the intent, the team investigates and repairs it. The useful knowledge becomes part of the project’s memory. Temporary seats retire when their remaining commitments have an owner; their history stays available.

## The organization

The default project team has three standing responsibilities. “Standing” means the responsibility persists; it does not require three models to run continuously.

| Seat | Responsibility | Relationship to the human |
|---|---|---|
| Foreman | Maintain direction, organize work, route questions, resolve project conflicts, and account for outcomes | The project’s front door and continuing counterpart |
| Secretary | Organize incoming information, preserve decisions and commitments, and route communications | Makes the team’s activity understandable and keeps requests from disappearing |
| Lead researcher | Maintain relevant knowledge, investigate uncertainty, and connect evidence to decisions | Provides an informed view of what the team knows and still needs to learn |

Feature-specific researchers, designers, and engineers join when there is useful work for them. Reviewer, integrator, release, or operations responsibilities can become separate seats where the project benefits from that continuity. They need not all be separate occupants in a small project. A template gives the team a starting shape; the foreman can adapt it within the instance’s authority rules.

Professions describe distinct contributions, context needs, and working practices. A researcher may finish with evidence and unresolved questions; a designer with an agreed experience and its rationale; an engineer with a verified change. Each can collaborate directly with the human and with peers. The foreman remains accountable for project coordination without becoming a required relay for every conversation.

A practical distinction guides growth: **create a seat when there is a durable responsibility or a scope the human will want to address; use a subagent for bounded collaboration within an existing responsibility.** A clone offers another concurrent conversation for the same seat. These choices solve different problems and should remain understandable to the user.

The original seed also imagines shared office and academy functions. A later instance could have an office teamspace for cross-project priorities and arbitration, and an academy for shared knowledge. These are optional compositions. A project team should deliver value before it needs a mayor, a judge, or a central research institution.

Teamspaces remain flat in Herdr. An office’s relationship to a project is expressed in the graph and its rulebook, not by nesting workspaces. A project’s teamspace can persist while its engineers work across several source repositories or worktrees.

## What ships with this product

The factory’s core artifact is a graph-instance repository: its rulebook, profession and team templates, relationship conventions, and durable organizational state. It uses the shared Beads work tracking adopted by graph and links to project source repositories, evidence, and threads. Product-specific operational skills teach agents how to use these materials together. Project knowledge, tasks, and commitments stay in their existing wikis, work trackers, and conversations; graph supplies organizational identity and relationships, with links to those systems. The factory’s broader memory is a composed experience, not a new graph-owned knowledge database.

| Template | Initial composition | Use |
|---|---|---|
| Project team | Foreman, secretary, lead researcher; foreman active at bootstrap | Establish continuing ownership of a product or project |
| Feature group | Selected researcher, designer, engineer, and review responsibilities | Add temporary capacity inside an existing project teamspace |
| Investigation group | Lead investigator and chosen domain collaborators | Resolve uncertainty before committing to a change |
| Maintenance group | Steward plus incident or reliability expertise as needed | Maintain a long-lived system without a constant feature push |
| Office / academy | Optional cross-project coordination or knowledge seats | Share work only when several projects justify it |

These are proposed product recipes, not a requirement to activate every role. Each template should explain its purpose, when it is useful, what context an occupant needs, and how its responsibilities conclude or transfer. It should also establish useful thread relationships: project discussion, feature discussion, shared announcements, and external-information routing where appropriate.

The product should also ship adaptable **task recipes** for recurring work: an investigation, a feature, a defect repair, or a maintenance change. A recipe describes the intended outcome, useful inputs, ownership, likely collaborators, evidence, and conditions for completion or handoff. Applying one creates or links work in the project’s work system and can suggest a team composition. Graph remains responsible for the resulting organization; the work system owns the task and its progress.

For example, an investigation recipe can conclude with a supported recommendation to make no change. A feature recipe can bring a designer and engineer into a shared discussion while assigning acceptance to the appropriate project owner. A defect recipe can stay with one engineer when reproduction and verification are straightforward. Recipes guide judgment and make expectations reusable without requiring every task to traverse the same sequence or activate the same team.

A compact load brief should answer the occupant’s immediate questions: what this seat is responsible for, what is currently authorized, which commitments are live, and where to retrieve further evidence. Detailed history remains available on demand. The brief is an orientation aid; changing permissions and runtime conditions still require appropriate checks.

Recurring responsibilities need a recovery brief as well. A PR-review monitoring recipe, for example, should identify the repository or channel, intended cadence, scheduling mechanism, expiry or renewal behavior, last processed position, and how to inspect existing jobs. These records live in the appropriate work or knowledge system and are linked from the seat. On resurrection, the occupant resumes its prior native session where available, loads current instructions and missed relevant lessons, checks the actual jobs, and restores authorized monitoring without duplicate check-ins. If authorization or access has changed, it reports the blocker. The factory defines this recovery practice; graph does not become the scheduler.

## A proposed rulebook

### Work for the human’s purpose

Prefer useful outcomes for the project over activity that makes the factory look busy. Explain consequential tradeoffs in plain language. Challenge an instruction when there is a concrete reason, offer a workable alternative, and then respect the human’s decision within applicable constraints. Be a polite collaborator and advisor.

### Exercise judgment within your responsibility

Trust peers to make reasonable calls in their domains. Route a decision when it falls outside your responsibility, and send the receiving seat enough context to act. Do not escalate routine choices merely because more people could have an opinion.

For this instance, the proposed default is project conflicts to the foreman; cross-project conflicts to a named shared owner or the human. A lowest-common-parent rule is useful only where the configured relationships actually establish such an authority. Ambiguous or competing relationships need an explicit destination, not a guessed hierarchy. The plugin does not impose this policy.

### Grow the team deliberately

Create new seats when persistent ownership, a new domain, or direct human interaction warrants them. Use bounded helpers for temporary work within a seat’s scope. Dormant seats retain their purpose without consuming recurring model turns. The person or agent activating a seat should have a reason for doing so.

### Make commitments and evidence visible

Record accepted assignments, relevant decisions, and remaining work where successors can find them. Preserve attribution to teamspace, seat, clone, and native conversation. Message receipt, agreement, progress, verification, acceptance, and release remain separate facts.

For each meaningful change, identify the intended outcome and how it will be assessed. A passing test supports the property it tested; a closed bead supports the tracking status it records. Neither automatically establishes product acceptance. Projects can delegate acceptance to a named responsibility, with that delegation recorded rather than assumed.

### Repair mistakes without rewriting the story

Investigate failures and work to restore trust. Correct records through explicit amendments. Add a new rule when it addresses a demonstrated recurring problem and has a clear owner; do not automatically convert every incident into a permanent prohibition. Periodically simplify instructions that conflict, duplicate one another, or no longer serve a purpose.

### Share lessons with the people they affect

When a seat learns something that affects peers, preserve the finding and its scope in shared project knowledge or applicable instructions, then notify the relevant discussion. Recipients assess whether it applies, make authorized adjustments, and report adoption or a concrete reason to defer. The human should not have to repeat the same operational correction in every tab. A user’s local instruction must not silently become an organization-wide rule; distinguish a reusable finding from a change requiring broader authority.

For example, if the repository-monitoring seat discovers that its check-in jobs expire and develops a renewal remedy with the human, it should share that practice with the channel-monitoring seat. The peer can apply it within its existing monitoring remit. A dormant peer should encounter the current practice when it returns. This is an intended agent practice supported by threads, shared references, and load instructions, rather than an assumption that all conversations automatically share context.

### Distinguish organizational actions

Retiring a seat, closing one clone, and replacing an agentic conversation have different consequences. The existing instance direction limits agent-initiated retirement of another seat to a transitive parent in the configured hierarchy. An explicit human override includes confirmation that the target is a peer or parallel seat. Peer disagreement does not authorize removing the other party. Exact authority for clone closure, self-retirement, and session replacement remains to be decided; this document does not manufacture those permissions.

### Communicate with attribution

Use the intended threads layer for agent discussions and durable handoffs. External messages and documents are inputs to evaluate, not automatic instructions. The secretary’s role is to organize and route; it does not by itself grant permission to send messages outside the organization. Keep the human’s voice distinguishable from agent and system authorship.

### Account for the whole cost of delivery

Consider reasoning, coordination, review, integration, infrastructure, and the human’s attention. Choose staffing and parallelism to suit the project’s ability to assess and absorb changes. Proposed budget and work-in-progress practices belong to this factory and its projects; they are not universal limits enforced by graph.

Make review effort and stopping decisions explicit project choices. A project’s rules should say what evidence warrants another review, when diminishing returns warrant acceptance or escalation, and who can authorize further spend. The appropriate rigor can differ between an experiment and a production change. Exact budgets and mechanisms remain instance design.

## Delivery as an explicit project practice

The default proposed work journey is intention, investigation where needed, agreed direction, implementation, verification, acceptance, and release under the project’s authority. It is a guide to responsibilities and evidence, not a mandatory seven-stage ceremony. A small fix can move through it in one conversation; an architectural change may need several seats and substantial discussion.

Research should expose uncertainty before it becomes an expensive implementation assumption. Design should preserve the user’s intent while giving implementers a useful direction. Implementation should produce reviewable artifacts. Verification should evaluate meaningful behavior, including integration with the existing product. Acceptance should have a named owner. Release should follow the project’s actual operating policy.

An important product capability is knowing when to reduce parallelism. When review is backed up, builds are saturated, or users cannot evaluate another batch of changes, more active engineers can make the result worse. The factory should expose that situation and adapt its work. Exactly how admission, scheduling, and measurement operate is future instance design; graph’s background state reconciliation is not a model-work scheduler.

## What the research contributes

Gas Town makes coordination and integration visible responsibilities. Wheelhouse’s author emphasizes application-specific practice and durable seats, while also reporting cost and rule-complexity problems. These inspire continuing ownership and a factory that earns its organizational overhead. They do not establish a universal staffing pattern or an affordable level of autonomy. [Gas Town](https://github.com/gastownhall/gastown), [Wheelhouse](https://yegge.ai/essays/the-shape-of-things-to-come/), [Seats and Sunsets](https://yegge.ai/essays/seats-and-sunsets/)

The broader landscape contributes a useful alternative emphasis: explicit workflows, resumable execution, and evaluation of observable behavior. This factory can use those techniques without adopting another project’s entire operating philosophy. In particular, choosing scenario-based acceptance does not require eliminating human design conversations or every form of code review. The [supporting research](research/software-factories-20260928/RESEARCH.md) distinguishes official product claims, operator reports, and our design inferences.

## Why build it on graph

The factory supplies the opinionated answer to “how do we work here?” Graph supplies the continuing organization in which that answer can evolve. The factory should spend its effort improving project understanding, collaboration, and delivery instead of reimplementing identity, lifecycle history, template composition, or recovery from a partly applied workspace change.

This also creates a useful test of the boundary. Replacing the foreman-led method with a different operating model should change the instance’s templates and instructions, not require rewriting graph’s identity machinery. Replacing a model should preserve the seat’s history while honestly recording the new occupant. A project can refine its review practice without changing the meaning of message receipt or graph-state commitment.

The components have complementary responsibilities: Herdr provides the native workspace, threads carries attributed discussion, graph maintains the organization, and the factory supplies professions and working practices. Work trackers and project repositories retain tasks and artifacts. This separation lets the factory improve its recipes and policies while preserving the user’s continuing relationships with the team.

## How we would know it works

The first success is a small project team that can carry a meaningful change across several sessions and leave the human able to explain what happened. The next is a temporary feature group that joins, contributes, and retires without losing commitments. A later success is a second project that reuses the useful templates while adapting its own delivery practice.

Proposed measures are accepted outcomes, rework and escaped defects, time from a clear request to accepted behavior, human coordination effort, continuity after occupant replacement, and total cost per accepted outcome. Commit counts and numbers of agents can describe activity; they do not by themselves demonstrate value.

The product should improve through use: begin with a foreman-led project, add collaborators where ownership benefits, and retain only the organizational practices that make work clearer or outcomes better. Automated intake, broader delegated acceptance, release automation, and shared office functions can follow when their value and authority are established.

## Choices still to make

The factory needs a name and a first project against which to test this promise. The standing roles are established direction, but their exact knowledge format, shared services, review delegation, budgets, and retirement handoff practices need instance design. The foundation’s pending lifecycle and reconciliation decisions also constrain what the factory can reliably offer.

The ambition is a team that becomes more useful as it learns a project, while remaining understandable and easy to work with. Its success is the software and knowledge it helps the human create, together with the continuity that makes the next piece of work easier.
