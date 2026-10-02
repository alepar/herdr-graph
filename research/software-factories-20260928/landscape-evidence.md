# Software-factory landscape: primary-source evidence

Research date: 2026-09-28. This is a research input for two product visions, not an implementation specification or adoption recommendation. Source and evidence ledgers accompany this file. Local alignment: `DESIGN-NOTES.md`, including its later superseding decisions on the sole graph writer and reconciliation.

## Main distinction

The useful comparison is between layers, not a league table of interchangeable products. StrongDM describes an operating model; Attractor describes an execution engine; Symphony describes issue dispatch and execution; OpenHands provides a coding-agent substrate with application surfaces; Devin packages agents, reusable procedures, and workflow orchestration. The same factory can contain several of these layers. This classification is our synthesis of the sources below, not a taxonomy claimed by their authors.

| System | Documented center of gravity | Principle to consider | Boundary to preserve |
|---|---|---|---|
| StrongDM factory | Seed, validation, feedback | Evaluate user-visible outcomes | Its review policy is one factory's choice |
| Attractor | Declarative, resumable task graphs | Separate execution mechanisms from model backends | Workflow graph is not organizational identity |
| Symphony | Issue-driven scheduler/runner | Version policy; reconcile operational state | Per-issue execution does not define permanent seats |
| OpenHands | Coding-agent SDK and applications | Composable core; explicit state ownership | Agent conversation persistence is a different layer |
| Devin | Productized engineering agents and workflows | Reusable procedures and evidence-producing handoffs | Vendor product behavior does not establish portability |
| SWE-agent / mini-SWE-agent | Coding-agent harness | Avoid adding orchestration complexity without need | Harness quality does not establish factory quality |

## StrongDM: outcome validation is the operating model

StrongDM's first-party account describes non-interactive development driven by specifications and scenarios, with agents writing code and running validation. It treats scenarios as end-to-end stories, often held outside the implementation repository to reduce reward hacking. It also reports behavioral twins of external services for high-volume validation. The account explicitly rejects human code writing and review; that is its organizational policy, not evidence that every successful factory must do so. [L01](https://factory.strongdm.ai/)

Its principles emphasize seeds, realistic validation, and feedback until holdout scenarios continue to pass. For herdr's concrete factory, the transferable idea is an explicit acceptance practice: work is complete when agreed outcomes have evidence. Whether that means human review, automated scenarios, or both belongs in the factory ruleset. StrongDM's published claims are a team's experience, not an independent reliability or economic study. [L02](https://factory.strongdm.ai/principles)

## Attractor: execution graphs without a prescribed agent backend

Attractor specifies DOT-defined pipelines, pluggable handlers, checkpoints, human decision nodes, and conditional routing. It permits different LLM/agent backends while separating its headless engine from presentation. The human gates are a useful corrective to equating the authors' no-review operating policy with an engine limitation. [L03](https://github.com/strongdm/attractor/blob/main/attractor-spec.md)

For herdr-graph, this supports making mechanisms reusable without fixing the factory's rules. It does not establish that herdr should adopt DOT or make each seat a workflow node. A persistent organization and a bounded execution pipeline have different lifecycles. A future integration could connect them, but compatibility has not been demonstrated.

## Symphony: policy and coordination are different responsibilities

Symphony's README presents autonomous implementation runs with CI, review feedback, complexity analysis, and walkthrough evidence. It explicitly describes the project as an engineering preview for trusted environments. [L04](https://github.com/openai/symphony)

The specification separates repository-owned workflow policy from coordination, execution, integration, and observability. It defines bounded concurrency, per-issue workspaces, retry backoff, stopping ineligible runs, and reconciliation. A successful run can stop at a handoff state; it need not mean the issue is done. The spec disclaims being a general-purpose workflow engine. [L05](https://github.com/openai/symphony/blob/main/SPEC.md)

This is especially relevant to the two-product vision: reliable coordination belongs beneath the organization's chosen definition of done. It also supports keeping admission, execution, acceptance, and completion legible to users. Symphony's tracker/filesystem recovery should not be described as equivalent to herdr's proposed authoritative graph files, serialized mutation journal, and independent external-effect reconciliation.

## OpenHands: a replaceable execution substrate

OpenHands documents Python and REST APIs for coding agents, used beneath its CLI and cloud experiences. This makes it useful as a contrast: an SDK can enable a factory without supplying its enduring organization. [L06](https://docs.openhands.dev/sdk)

Its V1 design discusses problems caused by mutable state, configuration sprawl, and application logic entering the agent core. It responds with one mutable conversation state, immutable components, separated layers, and composable agent capabilities. These are documented architectural lessons, not a guarantee that all state divergence is eliminated. For herdr, borrow explicit ownership and separation; do not turn one agent's conversation state into the authority for the whole organization. [L07](https://docs.openhands.dev/sdk/arch/design)

## Devin: reusable practice and structured orchestration

Devin's guidance stresses clear success criteria, sufficient context, focused sessions, and independent parallel tasks. It describes inspecting session outcomes and improving future context. These are vendor recommendations rather than independently measured success rates. [L08](https://docs.devin.ai/essential-guidelines/when-to-use-devin)

Playbooks package repeatable procedures and postconditions, distinguish task instructions from broader organizational Knowledge, and maintain version history. This supports a concrete factory product that ships useful operating practice while allowing teams to revise it. Prompt documents still require judgment; they are not equivalent to an enforced state machine. [L09](https://docs.devin.ai/product-guides/creating-playbooks)

Dynamic Workflows add deterministic Python orchestration, structured outputs, recorded calls, resumption, and background runs that leave the coordinating conversation available. That last affordance resembles the user's wish to discuss work while another clone executes it. The resemblance concerns user experience, not identical identity or state semantics. Recorded agent results do not by themselves establish exactly-once behavior for arbitrary external effects. [L10](https://docs.devin.ai/work-with-devin/dynamic-workflows)

## A restraint from SWE-agent

The SWE-agent repository recommends mini-SWE-agent as its simpler successor. Both are best understood here as agent harnesses. Their inclusion cautions against measuring a foundation by how much orchestration it adds: product value should come from continuity and useful coordination, not mandatory machinery around every task. This is our design inference; benchmark comparisons were not evaluated. [L11](https://github.com/SWE-agent/SWE-agent)

## Implications for the two product visions

The foundation can promise that responsibility, organizational memory, relationships, and operational intent survive replaceable sessions. Its native Herdr mapping supplies an intelligible place for people to see and join that organization. This proposition comes from the local design direction, informed rather than dictated by the external examples.

The factory above it can promise a coherent way of producing software: standing responsibilities, temporary feature teams, scoped rules, a discovery-to-delivery practice, and evidence for acceptance. Different factories should be able to choose different review policies, escalation patterns, and degrees of autonomy without replacing the foundation. Attractor, Symphony, and OpenHands independently reinforce the narrower principle of separating core mechanisms from application or workflow policy; they do not independently validate herdr's exact architecture.

Avoid claiming universal compatibility with every factory. The supportable vision is that the foundation offers durable organizational primitives on which several factory styles can be expressed. Concrete adapters, semantic mismatches, and lifecycle integration remain future design and engineering work.

## Method and limitations

Read the deep-research methodology, obtained the system date, and attempted the mandated search CLI first. Initial sandbox network requests failed; the approved escalated retry succeeded. Retrieved official pages using the web tool, followed documentation links, and stored short quotations and atomic supported claims in `landscape-evidence.jsonl`. Eleven sources span five independently authored project/vendor groups, but multiple pages within a group are not independent corroboration. Exact quotations are under 25 words per source. Repositories were read at their current default branches; no commit pins or implementation tests were collected.

This packet covers documented technical principles only. It contains no representative user-sentiment sample, cost comparison, security evaluation, or verified production performance claim. Source-side marketing superlatives and benchmark assertions are intentionally excluded. Product capabilities and documentation may change after the retrieval date. Several guessed documentation URLs failed; claims use only successfully retrieved pages.
