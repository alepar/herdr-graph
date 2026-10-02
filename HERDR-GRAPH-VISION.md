# herdr-graph: an organization that survives its conversations

Product vision · Draft for discussion · 28 September 2026

> Subsequent design decisions through 1 October are consolidated in [Current design](CURRENT-DESIGN.md); that summary takes precedence where this earlier vision differs.

## The promise

**herdr-graph gives a human and their agents a durable organization inside Herdr.** Organizational identity and relationships survive the conversations currently carrying them. Linked project systems preserve the knowledge and commitments those responsibilities depend on. People can find the right collaborator, resume a responsibility, reshape a team, and understand what is happening without reconstructing the organization from terminal history.

The first substantial application is a [software factory](SOFTWARE-FACTORY-VISION.md). The foundation should also serve a research group, an operating office, or a mixed organization spanning several projects. Its reusable value is continuity and coordination. The organization’s purpose, professions, standards, and operating philosophy belong to the product built on it.

**The organization is explicit; its shape is configurable.** Roles, relationships, instruction scopes, and participation are durable objects the system can discover and maintain. An instance can choose a foreman-led team, collaborating specialist peers, or shared services across projects. The foundation gives those choices consistent identity and lifecycle semantics without prescribing one chain of command.

This document expresses intended product behavior. It draws on the [settled design](DESIGN-NOTES.md), the [designer consultation](research/software-factories-20260928/designer-consultation.md), and the [landscape research](research/software-factories-20260928/RESEARCH.md). It is not an implementation specification or a claim that the capabilities exist today. Proposed extensions are identified below.

## Who it is for

The initial user is someone already working with several native coding-agent conversations in Herdr. Some conversations execute work; others explore designs, research questions, review outcomes, or coordinate a project. The user wants these collaborators to accumulate useful context and take responsibility, while remaining approachable and easy to redirect.

A second user is the author of an organization: someone who wants to express how their own factory works through role templates, relationships, and instructions. They should be able to improve their operating model without maintaining another terminal manager, identity registry, or recovery system.

The pain appears between sessions. Who owns the question now? What did the previous occupant promise? Is this tab a continuing responsibility or a disposable worker? Did a request fail, or is it still being applied? Which conversation should receive a decision? A terminal layout and a collection of prompts leave the human doing this bookkeeping. Graph makes that organizational context explicit and persistent.

## The experience

Imagine returning to a project after a week. The teamspace is still recognizable. Its foreman seat has a compact account of the project’s purpose, current commitments, recent decisions, and unresolved work. A fresh native conversation can occupy that responsibility without pretending to be its predecessor. The record shows who contributed and where the evidence lives.

You open another clone of the same seat to discuss a change while the first continues working. Both share the responsibility, but their conversations and individual working state remain distinct. A decision made in one has a durable place to be recorded and communicated to the other; shared identity does not imply shared model context or instantaneous agreement.

A new initiative needs a team. An agent adapts a template, resolves names and existing relationships, and submits a concrete plan. Graph records the intended organization and brings Herdr and threads into agreement with it. Some seats activate immediately; others are available to activate later. If part of the operation fails, the useful progress remains visible and the missing part can be repaired.

Later, you rename a tab in Herdr. The organization follows the change while preserving identity and name history. When a responsibility retires, its state is archived. When it returns, a new occupant can learn what happened before. Ordinary interaction with Herdr is part of the product experience, including the need to distinguish deliberate changes from incomplete observations.

A smaller example is a PR-review teamspace with two seats: one monitors a repository, the other monitors review requests in a Slack channel. While working with one seat, you discover that its recurring check-ins expire and develop a remedy. The intended experience is that the seat records the lesson in the appropriate shared knowledge or instructions and notifies relevant peers through their shared thread. The other seat assesses its applicability, applies the remedy within its existing authority, and records the outcome without requiring you to relay the discovery. Graph supplies discoverable relationships and participation; the instance supplies the practice of sharing and adopting relevant lessons.

Closing a seat’s tab archives its organizational state and retains references to its native sessions and recovery context. Explicit resurrection should prefer resuming the selected prior native session where the harness supports it and its transcript remains available. Otherwise, a fresh session should recover the responsibility from durable instructions and linked records. The returning occupant checks what changed while it was away and restores authorized recurring work. Restoring a transcript alone does not establish that scheduled jobs still exist or are running; recovery must inspect the scheduling system and avoid duplicate jobs. These are intended recovery requirements; harness-specific session retention and resume contracts remain to be designed.

## A vocabulary that matches the workspace

The organizational model uses Herdr’s existing workspace, tab, and pane structure. Herdr documents those as native, addressable interaction surfaces; the organizational meanings below are herdr-graph’s adopted design. [Herdr concepts](https://herdr.dev/docs/concepts/)

| Concept | Meaning | Herdr presence |
|---|---|---|
| Teamspace | A team or domain with persistent purpose and state | Workspace |
| Seat | A durable responsibility with context, scope, and history | Tab |
| Clone | One concurrent working presence of a seat | Pane |
| Agentic session | The replaceable native conversation doing the work | Agent occupying the pane |

Teamspaces are flat. Relationships express collaboration, reporting, stewardship, and routing across them. A group template can populate an existing teamspace without inventing a nested workspace. Several clones can serve one seat; a seat does not contain other seats.

The workspace is how the human encounters this organization. A designer’s tab represents an addressable responsibility that can span many tasks. Its panes support concurrent conversations within that responsibility. A coordinator can help route work, while the human and other seats can also maintain direct relationships with the designer. Collaboration and reporting relationships need not be identical.

Stable identities preserve attribution through names and occupants changing. Native terminal addresses remain useful locations, with the limitations documented in the [identity qualification](HERDR-IDENTITY-CHECK.md). Graph should make uncertain bindings visible rather than manufacture continuity from a familiar name.

## What the foundation owns

### Organizational continuity

Graph maintains the registry of teams, seats, clones, relationships, and their lifecycle. It makes responsibilities discoverable and provides a way to load their context. A responsibility can be dormant without being abandoned. Creating it does not commit the user to an always-running model or periodic model wakeups.

State should be readable by people, reviewable in Git, and accessible through meaningful operations. The adopted direction is one graph-instance repository and one shared checkout, separate from project source repositories. The plugin validates, serializes, writes, and commits graph-state changes. Agents submit changes, including text updates, through that surface. Readable files and coordinated mutation should reinforce one another.

Graph state covers teamspaces, seats, clones, thread relationships, templates, rulebooks, and the minimal operational journal needed to manage them. Project tasks remain in Beads or other work systems; knowledge remains in project wikis and native conversations. Seat context can reference those records without making graph a second work tracker or knowledge base.

### Composition and instructions

Templates describe initial seats, teams, groups, relationships, and intended thread participation. They are material for agent judgment: an agent tailors a concrete organization to the situation. A template is neither a permanent staffing limit nor a claim that every invocation has identical meaning.

Composition should produce an inspectable organization: named responsibilities, resolved relationships, applicable rulebooks, and intended channels. For example, a research template might establish an investigator, a domain specialist, and a shared discussion; a product template might establish design, engineering, and stewardship responsibilities. Instances define the professions. Graph maintains the resulting structure as occupants and staffing change.

Rulebooks provide global, team, and seat instructions. The foundation supplies their scope and availability. The instance decides how to resolve conflicts, who can arbitrate, which roles can retire others, and when to involve the human. Structural validity and truthful operation status are foundation concerns; choosing a universal management hierarchy is not.

This separates maintained structure from agent judgment. Identity, lifecycle, bindings, and recorded participation need consistent mechanical handling. A rulebook’s advice requires interpretation by its occupants; storing it does not guarantee obedience or establish an authorization boundary. Task recipes, review procedures, and release decisions belong to the instance and its work systems.

Template changes become available on load, and agents can announce changes to current occupants through appropriate channels. Existing conversations do not silently receive new understanding because a file changed. An organization must be able to experiment with a template and retain its accumulated history as its practices evolve.

### Communication with organizational context

Herdr-threads is the intended conversation layer. Graph adds organizational participation: a whole seat can subscribe, or an individual clone can participate. New clones inherit seat-wide intent, while individual membership and receipt remain attributable to their actual participants.

Threads can gather collaborators around a topic across seats and teamspaces. A design question can include a researcher, engineer, and human without requiring each exchange to pass through a coordinator. Reporting and escalation still follow the instance’s rules; participation in a discussion does not itself transfer responsibility or authority.

Organizational learning should travel through these relationships. A lesson learned in one conversation can become a durable shared reference and an attributed notification to affected seats. Active occupants can assess and adopt it; returning occupants can discover it during load. Notification, receipt, and application remain distinct facts. Graph does not infer or synchronize model knowledge, and automatic adoption depends on the instance’s instructions and the recipient’s authority.

Public, discoverable team and seat system channels make lifecycle changes and operational results visible. Required participation still requires initial acceptance; neither an invitation nor a system event invents an agent’s receipt. The designed system-author extension has been handed to the threads project and must not be described as proven runtime capability. [Threads design status](THREADS-IDENTITY-DESIGN-RUN.md)

Messages, work records, and organizational state each retain their own meaning. A received message is not an accepted assignment. An accepted assignment is not completed work. A closed bead is a status with evidence pointers, not independent proof of an accepted outcome.

### Recoverable operation

The organization should remain understandable when a tool disconnects or an operation only partly succeeds. Recorded intent comes first; a separate reconciliation mechanism applies external effects and records what actually happened. It runs after graph changes, on observed Herdr events, and periodically. An unavailable external service should not prevent unrelated state changes from being committed.

Observed manual changes can feed recorded state, and recorded state can drive Herdr and threads. Reconciliation is therefore more than recreating whatever disappeared. It needs to recognize uncertainty, preserve provenance, and distinguish pending creation from deliberate closure. The product promise is visible, repairable progress; exact matching and conflict policies remain design work.

Transient external failures retry with backoff. A request requiring revision returns to its caller with a linked repair path. An unknown outcome calls for inspection before a potentially duplicate effect. These distinctions keep routine outages from becoming repeated agent busywork.

Reconciliation follows the latest committed intent: obsolete effects stop retrying, and effects already performed are reconciled toward the newer state. Observation remains best effort; a manual action that was never observed may be overwritten by recorded intent. Unresolved graph requests survive their requester’s retirement with attribution intact. They remain discoverable and can be explicitly reassigned or cancelled under instance policy. Cancellation ends obsolete requests and reminders; it does not claim external effects were completed or undone.

## What makes this a product

The foundation should be useful before anyone builds an elaborate factory. One human, one teamspace, and a few durable seats should already reduce repeated orientation and lost commitments. Its basic operational skills are part of the product: loading a responsibility, discovering ownership, composing an organization, and understanding or repairing its state must be practical for agents as well as humans.

Its distinctive bet is that an organization can be stable while its models, conversations, and practices change. Gas Town offers durable work and a specialized coordination organization. Wheelhouse’s author argues for application-specific factories and describes persistent seats. Graph borrows the continuity question while leaving the organization’s operating method configurable. These are inspirations, not proof that a universal factory already exists. [Gas Town](https://github.com/gastownhall/gastown), [Wheelhouse account](https://yegge.ai/essays/the-shape-of-things-to-come/), [Seats and Sunsets](https://yegge.ai/essays/seats-and-sunsets/)

Firstmate is a particularly close alternative. It already combines Herdr integration, persistent scoped supervisors, durable messaging, recovery, and software-delivery practices. Its organizing vision centers on one principal liaison and delegated supervisors, with substantial customization within that model. Graph’s proposed distinction is a configurable organization of directly addressable professions, multiple concurrent clones of one responsibility, and topical participation across teams. Persistence and Herdr support alone do not distinguish graph. Firstmate has implemented mechanisms; graph must still demonstrate that its different organizational model is useful. [Firstmate vision](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/VISION.md), [detailed comparison](research/firstmate-20260928/COMPARISON.md)

Other explored alternatives clarify the boundary. Symphony emphasizes issue-driven execution and recovery; Attractor describes workflow execution; OpenHands supplies an agent substrate. Devin’s reusable playbooks and resumable background workflows overlap with parts of the desired working experience. Graph focuses on the continuing responsibilities and relationships across such work: who the human returns to, which conversations share a seat, how collaborators discover one another, and how the organization survives replacement or retirement. These are differences in emphasis and proposed contracts, not claims that the alternatives cannot be extended. The [landscape research](research/software-factories-20260928/RESEARCH.md) records the sources and limits of these comparisons.

The value should be visible in ordinary use. A human can develop a design with a specialist while another clone executes, teach one monitoring seat a lesson that reaches its peers, or resurrect a retired responsibility with an explicit recovery path. Templates make these relationships repeatable; rulebooks make their practices adaptable. Those experiences are the test of whether a separate organizational foundation earns its place alongside existing agent and delivery systems.

“Generalize to other factories” means supporting their organizational patterns where those patterns fit, not promising drop-in execution of their engines. A Gas Town-inspired instance can have supervisors and integration seats. A scenario-driven instance can organize evaluators around acceptance evidence. A research organization can omit software delivery entirely. Scheduling engines, repository-specific tests, deployment infrastructure, and model capabilities remain separate components.

## Product boundaries

| Foundation responsibility | Instance responsibility |
|---|---|
| Durable identity and discoverable relationships | Professions and reporting structure |
| Scoped instruction storage and loading | Values, authority, escalation, and rule precedence |
| Template composition and lifecycle operations | Team recipes and when to instantiate them |
| Recorded participation and threads integration | Discussion habits and communication policy |
| Serialized state changes and reconciliation | Work priorities, budgets, review, and release practice |
| Links to attributed work and evidence | What constitutes success and who may accept it |

Herdr remains the native place where humans meet their agents. Threads remains the conversation system. Beads remains the adopted shared work tracker. Graph connects them organizationally, preserving their separate sources of truth. It does not need to become a code-review engine, a universal workflow interpreter, or a replacement agent harness to deliver its core value.

## Principles to preserve

**Trust with an honest record.** Agents should exercise judgment within their responsibilities, investigate mistakes, repair them, and maintain trust. Corrections should explain what changed instead of erasing inconvenient history.

**Human participation belongs in the ordinary flow.** A user can visit a seat, discuss an issue in another clone, or reshape the team. The organization should support these interactions without requiring the user to operate an invisible control system.

**Dormancy is a useful state.** Persistent responsibility should be cheap to retain. Graph’s ordinary bookkeeping should not require a reasoning model to keep checking whether there is something to do.

**Organization should earn its complexity.** More seats, threads, and rules should make ownership clearer or work better. The foundation should make small organizations comfortable and growth reversible.

## What success would look like

The first convincing demonstration is continuity: replace an occupant and recover its responsibility, outstanding work, and evidence without a human rebuilding the story. A second demonstration is concurrency: two clones can contribute to one seat without losing attribution or silently overwriting its shared state. A third is recovery: a partial team creation or manual Herdr change produces an intelligible state and a workable repair path.

A fourth demonstration is organizational variety: compose two useful teams with different professions and relationships using the same foundation. Their templates and rulebooks should account for their different practices. The human should be able to find and address a specialist in either team, and the system should preserve those relationships across session replacement.

The PR-review example adds a practical continuity check: teach one monitoring seat a relevant operational fix, observe the peer apply it without human relay, then retire and resurrect a seat. Its returning occupant should recover the latest practice and verify or restore its authorized check-ins without duplicating them. A retired seat need not keep executing; continuity here means recoverable responsibility, with any independent scheduler’s behavior governed by the instance.

Proposed evaluation measures include time to orient a replacement occupant, human effort to resolve ownership, unresolved reconciliation discrepancies, and coordination cost per accepted outcome. These are product hypotheses, not measured improvements. A smaller, clearer organization can be a better result than a larger fleet.

## Questions that remain open

The vision does not settle storage formats, mutation preconditions, binding recovery, Beads representation, or every retirement edge case. Latest-intent reconciliation, preservation of requests after requester retirement, and explicit reassignment and cancellation are now approved directions. Their detailed contracts still need specification. These graph operations concern organizational mutations and reconciliation, rather than tracking all of a project’s work.

The larger product question is where the reusable boundary proves useful in practice. The first factory should exercise it, but a second, differently organized instance should challenge it before its conventions become universal primitives. The intended destination is a foundation on which different organizations can grow while remaining legible to the people they serve.
