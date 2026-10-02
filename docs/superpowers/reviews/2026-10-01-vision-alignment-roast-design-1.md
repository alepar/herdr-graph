super-roast verdict: Should-fix (21 confirmed)
mode: design        iteration: 1 of 3
profile (assumed): Internal single-human-led design-stage tool. No graph implementation, production deployment, or regulatory obligations are asserted. Intended operations affect real native agent sessions and linked project work; continuity and recovery matter, while general model scheduling, project tracking, and behavioral authorization remain external or instance-owned.
inputs: CURRENT-DESIGN.md; DESIGN-NOTES.md; HERDR-GRAPH-VISION.md; SOFTWARE-FACTORY-VISION.md
coverage: premortem, completeness, yagni, failure-mode, feasibility, distributed, integration, competitors (8/8 scouts) · 38 raw → 30 deduped → 26 full panels (25 initial + 1 promoted), 4 remaining spot checks · judge completion 100% (83/83 calls; 82 retained votes) · remainder-capped: 0
independence: same-family (OpenAI GPT-6) — seat-differentiated panel
seat-agreement: panels 26 · rr 0.85 · rg 0.81 · fg 0.88 · unanimous 0.77 · ground-loo 0.91 (n=22) · reproduce 21/5/0 · refute 25/1/0 · ground 24/2/0

The current design is largely aligned with the visions. Later explicit choices supersede historical proposals: live templates, multiple clones, agent-led /seat loading and mechanical transcript processing are intentional developments. Graph preserves organization; the factory supplies working practices; existing work systems retain project tasks and knowledge.

Most findings specify acknowledged open contracts or interactions between approved choices. None establishes current data loss, an exploitable security defect, or a proven violation of the artifact’s core purpose; no Blocking severity floor is triggered. No severity was demoted because of the profile. This is a report-only design review, not implementation qualification.

## Confirmed findings

- [Should-fix] **F01 — Concurrent edits need a stale-write rule.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats cite [CURRENT-DESIGN.md:14](/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md:14) and [DESIGN-NOTES.md:160](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:160): two clones can submit replacements derived from the same revision; serialization alone does not preserve both contributions. Expected revisions are proposed, not specified.
  fix-shape hint: Define mutation preconditions, conflict responses and patch/replacement semantics.

- [Should-fix] **F02 — Closure detection and descendant retirement need an explicit scope rule.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats find closure versus unavailable observation and last-pane behavior explicitly unresolved at [DESIGN-NOTES.md:147](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:147). The ground seat finds directional ambiguity safeguards, but no adopted evidence threshold or cascade mapping; accepted missed observations do not settle observed ambiguity.
  fix-shape hint: Specify which observations justify retirement, which descendants follow, and how uncertainty remains visible.

- [Should-fix] **F04 — Bootstrap can bind the recovery pane before undo adopts it.** *New interaction gap*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: The panel combines automatic /seat at [DESIGN-NOTES.md:238](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:238), possible creation at [DESIGN-NOTES.md:193](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:193), and mandatory current-pane adoption at [DESIGN-NOTES.md:265](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:265). If startup creates a fresh identity, the undo contract does not say what happens to that identity, its history or memberships.
  fix-shape hint: Include existing recovery-pane bindings and their disposition in the concrete restoration plan.

- [Should-fix] **F05 — Out-of-order transcript results need a current-result selection rule.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats cite [DESIGN-NOTES.md:256](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:256)–259: an older result through position 100 can arrive after one through 150. The panel confirms missing selection/combination semantics, not an established last-arrival overwrite algorithm.
  fix-shape hint: Define how completed results remain discoverable and how recovery selects usable coverage without regression.

- [Should-fix] **F07 — A durable processing request needs an unavailable-source outcome.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: The panel finds durable requests and positive completion at [DESIGN-NOTES.md:255](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:255), while transcript unavailability is contemplated at [HERDR-GRAPH-VISION.md:37](/Users/alepar/AleCode/herdr-graph/HERDR-GRAPH-VISION.md:37). No source-validity or unavailable-input disposition connects these contracts. No actual harness retention failure is asserted.
  fix-shape hint: Define readable-source identity/validity checks and truthful incomplete or unavailable outcomes; graph need not archive transcripts.

- [Should-fix] **F08 — Automatic processing needs a finite eligibility boundary for the summarizer’s own sessions.** *New interaction gap*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: The ground seat narrows this to the ordinary summarizer’s native session at [CURRENT-DESIGN.md:32](/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md:32). If its closure creates more processing, each retirement can reactivate it again. Eligibility of helper transcripts is unestablished and is excluded from this finding.
  fix-shape hint: Choose source eligibility or another finite stopping rule before wiring closure to activation.

- [Should-fix] **F09 — Repeated template composition needs application-scoped correspondence.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats cite [DESIGN-NOTES.md:293](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:293)–302: independent seat IDs and stable member IDs do not determine which application an exclusion or template switch addresses when the same group appears twice in one team.
  fix-shape hint: Define correspondence and exclusion scope for repeated applications without deriving seat IDs from hydration or requiring a new group node.

- [Should-fix] **F10 — Live model/harness defaults need an occupied-session transition rule.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: The panel combines the model-default example at [DESIGN-NOTES.md:203](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:203) with live reconciliation at [DESIGN-NOTES.md:209](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:209). Instruction rereads do not specify when a running session adopts a different runtime configuration; session-replacement authority is also open at [CURRENT-DESIGN.md:41](/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md:41).
  fix-shape hint: Separate effective defaults, current runtime and pending authorized transition; decide whether changes apply at next activation or through another explicit path.

- [Should-fix] **F11 — Undo’s unconditional pane-adoption wording has a zero-target case.** *New interaction gap*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats compare [CURRENT-DESIGN.md:46](/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md:46)–49: undoing a template edit may restore no pane, while hydration undo may retire the caller’s own scope. The required adoption destination cannot always exist.
  fix-shape hint: Make adoption conditional on restoration and define where preview/completion are delivered when undo retires the caller’s scope.

- [Should-fix] **F12 — Shared Beads persistence needs a boundary with the sole Git committer.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats cite shared-repository Beads storage at [DESIGN-NOTES.md:27](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:27), the sole committer at [DESIGN-NOTES.md:150](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:150), and the open coordination question at [DESIGN-NOTES.md:134](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:134). Ownership of project work is settled; committing its representation in the shared checkout is not.
  fix-shape hint: Specify the Beads-owned write/export and graph-owned commit interface, including which files each coordinates.

- [Should-fix] **F13 — Collective group operations need current membership semantics.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: The panel cites named-group list/retire/resurrect at [DESIGN-NOTES.md:39](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:39) and historical action provenance at [DESIGN-NOTES.md:302](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:302). Neither stable IDs nor the original hydration list determines later membership after additions, exclusions, reuse or template switches.
  fix-shape hint: Define the scope of collective operations, or narrow that promise; a separate runtime group identity is optional.

- [Should-fix] **F14 — Retirement handoff needs a default commitment lookup and transfer convention.** *Acknowledged open contract; factory instance*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: The promoted panel unanimously distinguishes graph-operation reassignment from project commitments. [SOFTWARE-FACTORY-VISION.md:35](/Users/alepar/AleCode/herdr-graph/SOFTWARE-FACTORY-VISION.md:35) promises owned remaining commitments, but [SOFTWARE-FACTORY-VISION.md:157](/Users/alepar/AleCode/herdr-graph/SOFTWARE-FACTORY-VISION.md:157) leaves handoff practice open; current principles do not select how successors find and verify transfers across work systems.
  fix-shape hint: Put a minimal authoritative-record, lookup and owner-transfer convention in factory templates/recipes, keeping project work external to graph.

- [Should-fix] **F15 — Cancellation must say what happens to already committed desired intent.** *New interaction gap within an open cancellation contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats combine commit-first reconciliation at [DESIGN-NOTES.md:152](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:152), latest intent at [DESIGN-NOTES.md:179](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:179), and cancellation at [DESIGN-NOTES.md:189](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:189). Cancelling a pending activation request during an outage leaves its effect still desired unless cancellation supersedes or suppresses that intent.
  fix-shape hint: Distinguish cancelling admission/reminders from cancelling outstanding effects, and specify the required desired-state transition.

- [Should-fix] **F16 — Summarizer activation needs precedence against explicit retirement.** *New interaction gap*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: The panel finds live-tab closure retires the seat ([CURRENT-DESIGN.md:18](/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md:18)) while transcript work activates an unoccupied summarizer ([CURRENT-DESIGN.md:32](/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md:32)). Latest-intent reconciliation does not decide whether the workflow may create new activation intent after deliberate retirement.
  fix-shape hint: Define lifecycle eligibility and pending-request disposition after explicit summarizer retirement.

- [Should-fix] **F17 — Delayed whole-seat leave instructions need a supersession check.** *New interaction gap*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats identify a leave notice delivered after a newer whole-seat join: executing it as an individual leave can create a durable clone opt-out ([DESIGN-NOTES.md:13](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:13), [DESIGN-NOTES.md:57](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:57)). Reconciler checks do not identify that later agent submission as an obsolete instruction.
  fix-shape hint: Carry the originating intent/revision into the instruction and validate it before recording the leave; distinguish it from a fresh voluntary opt-out.

- [Should-fix] **F18 — Hydration undo must classify members introduced by later live-template edits.** *Acknowledged open contract*
  verdict: confirmed (reproduce REJECT / refute ✓ / ground ✓). The narrow contract gap survives the dissent; no unconditional automatic inverse is assumed.
  evidence: Refute and ground show that a later member B is absent from hydration H’s original creation set ([DESIGN-NOTES.md:209](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:209), [DESIGN-NOTES.md:285](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:285), [DESIGN-NOTES.md:302](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:302)). Reproduce correctly notes that preview and agent repair prevent assuming an unsafe automatic inverse. That fallback does not decide whether B survives, retires or is a conflict, or how H’s live contribution is withdrawn.
  fix-shape hint: Define that dependency classification and desired-state withdrawal so preview can identify the affected scope.

- [Should-fix] **F19 — The journal and Git writer need a crash boundary.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats cite admission/completion separation at [DESIGN-NOTES.md:150](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:150) and latest committed intent at [CURRENT-DESIGN.md:16](/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md:16). A crash before commit leaves changed checkout files; one after commit but before completion leaves ambiguous operation status. External-effect retry rules do not resolve either local case.
  fix-shape hint: Associate admitted operations with commits and define restart handling for uncommitted changes and committed-but-unreported operations.

- [Should-fix] **F20 — An existing summarizer occupant needs a defined incoming-request receive/resume path.** *Acknowledged open integration contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats narrow the claim to the missing bridge at [DESIGN-NOTES.md:247](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:247)–259: absent-occupant activation and durable posting do not specify how a busy or turn-completed occupant acts on a later request. This is a contract omission, not a finding that a harness cannot do it or that an unbuilt design must already have passed a trial.
  fix-shape hint: Name the ordinary thread-to-occupant receive/resume behavior, then qualify busy, idle and replacement scenarios when integrating it.

- [Should-fix] **F24 — Direct readers need one coherent committed view.** *New interaction gap within the mutation contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: The panel distinguishes the existing latest-committed-intent requirement ([CURRENT-DESIGN.md:16](/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md:16)) from its missing read boundary. /seat reads applicable files directly ([CURRENT-DESIGN.md:30](/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md:30)) while a shared-checkout worker changes related files. Serialization of writers does not specify what concurrent readers may observe.
  fix-shape hint: Define a committed snapshot or equivalent wait/retry rule for /seat and reconciliation across multi-file writes and renames.

- [Should-fix] **F25 — Later reuse must count in hydration-undo dependency checks.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats demonstrate A creating a seat/relationship and B later reusing it without editing a field. A’s literal inverse removes B’s dependency ([DESIGN-NOTES.md:285](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:285)–289). Provenance can reveal the reuse; the unresolved issue is whether it retains the object or requires repair.
  fix-shape hint: Classify later object and relationship reuse in preview/inversion, including dependencies with no field conflict.

- [Should-fix] **F27 — Processed-through coverage must distinguish a prefix from disjoint completed ranges.** *Acknowledged open contract*
  verdict: confirmed (reproduce ✓ / refute ✓ / ground ✓).
  evidence: All seats use [DESIGN-NOTES.md:256](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:256)–259: processing appended positions 101–200 can finish while 1–100 remains pending. A processed-through value of 200 alone cannot establish a complete prefix. This differs from F05’s older-result selection problem.
  fix-shape hint: Define request bounds, completed ranges and the usable contiguous frontier without hiding pending gaps.

## Not verified (beyond panel cap)

- none

## Beyond remainder cap (count only)

- none

## Rejected (with reason)

- **F03 — Summary paths after folder renames.** Reporter override of two CONFIRMs. Ground establishes that placing summaries under renamed seat folders appears only in a pending proposal ([DESIGN-NOTES.md:229](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:229)); both confirming seats concede that conditional premise. The approved result contract can reference an artifact elsewhere. A placement-dependent hazard is not an established current design gap. If that placement is selected, preserve reference validity as part of it.
- **F23 — Cross-seat reload allegedly assumes clean semantic adoption.** Rejected (reproduce REJECT / refute CONFIRM / ground REJECT). Reproduce and ground cite the explicit preservation of old context and disclaimer of automatic forgetting ([DESIGN-NOTES.md:148](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:148)). Refute identifies an open source-task handoff detail but does not establish the alleged clean-adoption assumption or an inevitable misattribution. Preserve the approved reload choice; work handoff can be refined in instance practice.
- **F28 — General project-work supervision allegedly has no owner/wake contract.** Rejected (reproduce REJECT / refute REJECT / ground CONFIRM). Refute shows an owner exists: the foreman accounts for outcomes ([SOFTWARE-FACTORY-VISION.md:43](/Users/alepar/AleCode/herdr-graph/SOFTWARE-FACTORY-VISION.md:43)). Ground concedes this and relies on a stronger continuous-liveness reading. Reproduce/refute cite human-led operation, normal dormancy and explicit activation choices; the vision does not promise unattended stall detection. [Firstmate’s event-driven watcher](https://github.com/kunchenguid/firstmate/blob/main/docs/architecture.md) is a verified competitor concept, not a parity obligation.

## Unverified nits (spot-checked)

These four candidates received one refute seat each, all REJECT/FYI; none is panel-confirmed. They remain recorded here to preserve the complete candidate inventory.

- **F21 — Missing empirical proof of occupant-replacement continuity.** The seat finds a recovery practice already described and continuity explicitly framed as a future demonstration ([HERDR-GRAPH-VISION.md:139](/Users/alepar/AleCode/herdr-graph/HERDR-GRAPH-VISION.md:139), [SOFTWARE-FACTORY-VISION.md:75](/Users/alepar/AleCode/herdr-graph/SOFTWARE-FACTORY-VISION.md:75)). A trial is useful evaluation, not a missing requirement in this draft.
- **F22 — Allegedly missing cross-clone decision adoption.** The seat cites the existing record/notify/assess/adopt-or-defer practice ([SOFTWARE-FACTORY-VISION.md:107](/Users/alepar/AleCode/herdr-graph/SOFTWARE-FACTORY-VISION.md:107)) and the explicit rejection of instantaneous agreement ([HERDR-GRAPH-VISION.md:29](/Users/alepar/AleCode/herdr-graph/HERDR-GRAPH-VISION.md:29)). The claim requires a stronger timing guarantee than the documents make.
- **F29 — No dedicated digest/decision inbox.** The seat finds the foreman/secretary conversational front door already responsible for explaining unresolved work ([SOFTWARE-FACTORY-VISION.md:43](/Users/alepar/AleCode/herdr-graph/SOFTWARE-FACTORY-VISION.md:43)). [Firstmate’s /ahoy and /bearings](https://github.com/kunchenguid/firstmate) offer a useful optional interaction, not a required interface here.
- **F30 — No dedicated project-environment onboarding.** The seat finds adaptable recipes already own context and verification ([SOFTWARE-FACTORY-VISION.md:69](/Users/alepar/AleCode/herdr-graph/SOFTWARE-FACTORY-VISION.md:69)), with no promise that every engineer receives a preconfigured environment. [Devin’s environment setup](https://docs.devin.ai/onboard-devin/environment) is a useful factory recipe idea, not a graph responsibility or demonstrated contradiction.

## Escalations (need human)

- **F06 — Material dissent over stalled transcript-work recovery: reproduce finds no promised deadline or automatic liveness guarantee; refute/ground find the present-occupant recovery path necessary. Decide the intended recovery responsibility and trigger before classifying this as a missing contract.** Evidence: reproduce accepts that work can remain visibly pending but cites role-owned dispatch and no automatic liveness promise; refute/ground cite the absence of recovery for a present but stalled occupant at [DESIGN-NOTES.md:247](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:247)–259. Unlike F20’s receipt bridge, this concerns already-dispatched work. An ordinary role-owned repair rule may suffice; this review does not prescribe timeouts, periodic wakes or a worker pool.
- **F26 — Material dissent over historical transcript attribution: reproduce finds per-contribution attribution sufficient; refute/ground require a scope boundary within a transcript spanning a move. Decide the promised granularity of transcript-derived history before requiring a cutover representation.** Evidence: reproduce cites directly tagged work records at [DESIGN-NOTES.md:16](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:16) and the absence of any requirement to infer history from a current-seat lookup. Ground narrows the concern to transcript-derived history and finds no move/commit/reload cutover at [DESIGN-NOTES.md:148](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:148), [DESIGN-NOTES.md:255](/Users/alepar/AleCode/herdr-graph/DESIGN-NOTES.md:255). Neither per-contribution records nor an assumed current-seat lookup settles what scope a whole-transcript result must report. No particular segment schema is justified yet.

## Useful next decisions

1. **Mutation and observation contract:** F01, F02, F12, F15, F17, F19 and F24 share admission, committed reads, intent changes and recovery boundaries. Settle those together before designing storage details independently.
2. **Composition and undo:** F04, F09–F11, F13, F18 and F25 need one coherent explanation of application scope, current dependencies and pane adoption. Preserve live references and independent seat identity.
3. **Transcript service:** F05, F07, F08, F16, F20 and F27 fit one request/result lifecycle, with F06 and F26 as explicit scope decisions. Separate delivery, execution recovery, source availability and coverage.
4. **Factory practice:** F14 belongs in the initial retirement/recovery recipe. Optional competitor features can be evaluated there without moving project-work machinery into graph.

The [competitor companion](/Users/alepar/AleCode/herdr-graph/docs/superpowers/reviews/2026-10-01-vision-alignment-competitors.md) compares Devin (the assumed meaning of “delvin”), Wheelhouse and Firstmate, with primary links and evidence limits. Capability comparisons are not additional confirmed defects.

Full [seat evidence](/Users/alepar/AleCode/herdr-graph/docs/superpowers/reviews/2026-10-01-vision-alignment-roast-design-1-evidence/packets.json) and [coverage/provenance](/Users/alepar/AleCode/herdr-graph/docs/superpowers/reviews/2026-10-01-vision-alignment-roast-design-1-coverage.json) preserve all 30 candidates. The initial F14 spot check was replaced by a fresh three-seat panel, explaining 83 completed calls versus 82 retained votes. No scouts, judges or deduplication stages failed; neither cap dropped candidates. Same-family seat diversity is not independent-model agreement.

## Vision-to-design traceability

“Covered” below means direction is present in the design, not implemented or empirically validated. Current design takes precedence over the earlier visions (both visions, line 5).

| Vision promise | Design coverage | Remaining boundary |
|---|---|---|
| Durable responsibility and several native conversations per seat | Teamspace/seat/clone/session identities; independent seat IDs; shared and clone-local files | Binding recovery, stale edits, and attribution across moves still need exact contracts |
| Human can return directly to a specialist | Seat-to-tab relationship and `/seat` discovery/loading | Minimal shipped skills and useful orientation experience remain to be selected and tested |
| Organization can change without losing history | Live templates; durable overrides, additions, exclusions; named copies; stable member correspondence | Repeated application, member removal/re-addition, and runtime-default transitions |
| Recoverable organizational actions | Durable serialized writer; separate reconciliation; latest intent; inspect uncertain outcomes; reassignment/cancellation | Commit/read/replay boundaries, cancellation semantics, and dependency-aware compensation |
| Durable collaboration without a coordinator relay | Whole-seat/clone participation and public system channels; threads owns acceptance and receipts | Recipient-side instruction freshness, missed lessons, and real decision adoption are separate from message delivery |
| Resume retired responsibility and authorized recurring work | Archive/resurrect; native session references; dedicated undo; factory vision specifies checking scheduler state | Resume selection, cascade/adoption semantics, and a factory-owned scheduler adapter/recovery recipe |
| Useful memory without a graph-owned work database | Agent-led retrieval; Beads/wiki links; mechanical transcript requests; semantic work in ordinary roles | Artifact/source availability, coverage accounting, eligibility, and instance-owned commitment handoff |
| Factory delivers accepted software outcomes | Factory vision proposes roles, task recipes, evidence and acceptance responsibilities | No concrete factory package yet; project onboarding, work supervision, release and integration practices belong there or in adapters |
| Control coordination cost and human attention | Factory vision explicitly proposes review stopping, spend/WIP practices, and outcome measures | No selected measurement or operational digest; these are proposed factory work, not hidden graph guarantees |

## Apparent inconsistencies already resolved by precedence

- Earlier load-only template wording is superseded for structural changes. Live model understanding still requires rereading; structural propagation does not imply semantic refresh (DESIGN-NOTES.md:207–213).
- `/seat` supersedes `/load`, and direct agent reading supersedes a plugin-generated semantic brief (DESIGN-NOTES.md:236–241). A factory-owned orientation practice remains compatible.
- Mechanical transcript indexing and request tracking deliberately extend the older reference-only boundary; graph still does not semantically summarize or own general project work (DESIGN-NOTES.md:253–259).
- Deferred seats have independent records before any tab exists; absence is not automatically retirement (DESIGN-NOTES.md:191–198).
- A dedicated, broader undo flow supersedes the early last-action and `/seat` recovery proposals (DESIGN-NOTES.md:261–289).

These are documentation drift, not reasons to reverse approved decisions. The meaningful review target is whether the resulting contracts support the intended experience.
