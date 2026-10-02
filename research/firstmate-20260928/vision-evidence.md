# Firstmate versus the two Herdr product visions

Static documentation and selected implementation inspection, 28 September 2026. Firstmate pinned to `2d833ff147cd26a5c461e914e06854e0eb2707ce`. No upstream code was executed. Local comparison: `HERDR-GRAPH-VISION.md`, `SOFTWARE-FACTORY-VISION.md`, `DESIGN-NOTES.md`. Firstmate has implementation evidence; graph and our factory remain intended products. Do not compare its implementation limitations with our untested promises as if both were measured products.

## Strongest case against building graph

Firstmate substantially addresses the same immediate customer problem: one person's ambitions exceed their attention, agents need continuity, and meaningful outcomes should survive conversation loss. It already uses Herdr, has persistent scoped secondmates with their own homes and memory, accepts multiple domains over the same projects, and can use Beads. Durable roles, a foreman, tokenless mechanics, project knowledge, per-project delivery practice and restart recovery are not a defensible novelty claim. A renamed firstmate as foreman, secondmates as department heads, and scouts as researchers could satisfy much of the proposed factory. ([Secondmates](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/docs/architecture.md#L322-L364), [Beads](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/docs/configuration.md#L334-L376), [vision](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/VISION.md#L3-L19))

The attractive alternative is not necessarily a fork: configure a Herdr home and scoped secondmates, supply project practices, and operate it. Our proposed office/academy/secretary functions could start as scopes, charters and instructions. Firstmate explicitly expects deep personal customization. Its dispatch profiles already choose harness/model/effort by natural-language rules, although these are dispatch policies rather than general team templates. ([Customization](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/VISION.md#L56-L78), [dispatch profiles](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/docs/configuration.md#L984-L1020))

This could deliver useful software sooner than implementing a generic organization substrate. A graph cannot eliminate the hard operational work evidenced in Firstmate: missing replies, stale bindings, wake delivery, liveness uncertainty, unlanded work and delivery authority. An organization registry on top of poorly maintained supervision does not produce a reliable factory.

## What actually differs

| Dimension | Firstmate evidence | Our intended product | Consequence |
|---|---|---|---|
| Main user experience | One liaison owns human attention; workers normally report upward | Human can address a designer, researcher or other seat directly and keep side conversations | Real product preference, but not a binary technical capability gap |
| Durable responsibility | Secondmate is an enduring supervisor with a scoped home and children | Any profession can be a durable seat; execution and discussion are peers in the model | More than renaming when enduring specialists must themselves do work |
| Concurrent presence | Another live home supervisor is refused by session lock | Several active clones of one seat, with shared responsibility and separate attribution | Genuine state/concurrency contract requiring implementation |
| Communication | Parent status channels, correlated requests and outcomes | Discoverable threads; seat-wide intent plus actual per-clone participation | Needs a different relationship and communication model |
| Herdr meaning | Backend and visual projection; recorded endpoint metadata is authority | Workspace/teamspace, tab/seat, pane/clone are organizational identities; manual changes enter reconciliation | Native Herdr affinity changes product semantics, not just launcher choice |
| Retirement | Explicit guarded retirement removes secondmate home and route | Archive responsibility, preserve history, permit resurrection | Different lifecycle promise |
| Generality | Opinionated personal command distro | Reusable organizational substrate plus a separate opinionated factory | Potentially useful boundary, not proven demand |

The concurrency distinction is supported by actual `fm-lock.sh` refusal logic, not merely an absent feature keyword. A second read-only chat or separate home remains possible; it is not the same as two authorized concurrent occupants of a shared seat. ([Lock code](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-lock.sh#L182-L201))

The interface distinction needs fairness: Firstmate explicitly treats direct human typing into a secondmate pane as authoritative, conversational intervention. It is inaccurate to say users cannot speak to specialists. The difference is whether this is the normal product model with durable same-seat side conversations, or an intervention reconciled by a supervisor. ([Charter code](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-brief.sh#L410-L421))

A secondmate's charter instructs it to delegate project work, not become an arbitrary enduring engineer or designer. Role-specific briefs can make disposable workers specialists; turning them into durable non-supervisor seats changes supplied lifecycle conventions. ([Generated charter](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-brief.sh#L383-L403))

Retirement and Herdr projection are also grounded: teardown removes homes/routes, while presentation explicitly grants no lifecycle authority. Graph would intentionally use a distinct archive and organizational reconciliation model. ([Teardown](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-teardown.sh#L3717-L3734), [projection](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/docs/herdr-backend.md#L247-L260))

## Differences not to overstate

“Trust peers” versus “workers are supervised, not trusted” is an operating philosophy difference, but graph itself deliberately leaves acceptance and authority policy to instances. Our own factory still expects evidence, review and accountable acceptance. This is not a reason to omit validation or rebuild infrastructure. Firstmate also allows scoped autonomy and captain overrides; characterizing it as uniformly rigid would be unfair. ([Policies](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/VISION.md#L21-L54))

Firstmate's restart-proof language should be read with implementation limits: its architecture asks for `/stow` before reset for information still only in conversation, and explicitly says the stow pass is not enduring reconciliation of open work against repository/PR reality. These are honest qualifications, not proof that Firstmate cannot recover durable fleet state. Graph's narrow scope similarly does not guarantee preservation of all unwritten project knowledge. ([Stow limits](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/docs/architecture.md#L470-L479), [recovery](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/docs/architecture.md#L501-L506))

Dormancy needs precision: secondmate homes survive no live process and idle is healthy, but registered persistent secondmate endpoints are automatically recovered when confirmed dead. Graph intends an explicit dormant responsibility without a running agent; preserving a home is not by itself proof of that exact lifecycle. Neither should be characterized as spending model tokens constantly while idle.

## Adopt, extend, or build

**Adopt Firstmate** if the desired outcome is “give one agent work across projects and receive supervised results.” Most proposed factory practices can be tested there now. Cost: accept the liaison-centered operating model and its existing dependency/toolchain practices.

**Extend Firstmate** for charters, project modes, role briefs, additional memory conventions, and scoped domain routing. These match its customization philosophy. A thin bridge that exposes Firstmate task outcomes to Herdr threads could also be useful, provided lifecycle ownership is explicit. Cost: keeping customizations compatible as tracked contracts evolve.

**Build a narrow graph** if direct access to enduring specialist responsibilities, concurrent clones and independently attributed thread participation are essential to how the user actually works. These are coherent needs outside Firstmate's core command model. Cost: significant identity/reconciliation engineering before factory throughput improves. Do not rebuild the entire dispatch, validation and forge stack in graph.

**Do not call a deep Firstmate rewrite an inexpensive extension.** Replacing its sole-liaison convention, supervisor home lock, parent routing and retirement semantics together crosses its architectural grain. Conversely, do not use this argument to justify every graph feature: many factory-level features remain ordinary Firstmate configuration.

## Recommendation and a falsifiable decision

Conditional yes to graph, no to building a competing broad factory merely because our role names differ. Firstmate materially weakens the case for rebuilding delivery orchestration, while sharpening the case for an organizational product centered on how the human collaborates.

Before a large build, compare one real project in Firstmate with a narrow graph prototype against the same three situations: (1) continue implementation while the human opens a second conversation with the same designer responsibility; (2) replace a specialist conversation and recover identity, thread obligations and attribution; (3) retire then resurrect that responsibility, including a manual Herdr rename or closure. Measure human reconstruction/routing effort, lost decisions and repair cost. Firstmate needs fair configuration and a knowledgeable operator. Graph does not win by merely completing operations Firstmate never aimed to support; it wins only if these interactions deliver enough user value to justify their maintenance cost.

If scoped secondmates plus ordinary human intervention feel sufficient, defer graph. If same-seat concurrent collaboration and visible organization are central daily activities, build that thin foundation and keep evaluating Firstmate or its patterns as a delivery subsystem. A second, differently organized instance remains necessary before claiming that graph generalizes beyond our preferred factory.
