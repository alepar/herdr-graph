# Threads system identity design run

## Goal

Design the programmatic graph identity extension for herdr-threads, add its scope to the existing implementation epic, and notify herdr-threads/main. This run does not implement code or redesign the rest of the graph.

## Status — 2026-09-28

- User explicitly requested `$super-design`, installation from the legacy ~/.codex profile, attachment to the active threads epic, and a final notification to its implementing agent.
- Installed copies of super-design, brainstorming, using-git-worktrees, super-roast and upstream-feedback into `/Users/alepar/.aisw/profiles/codex/codex-3/skills`; existing files were not overwritten.
- Existing root: `ht-4is`, “Build herdr-threads native mailbox plugin”, open, labelled `sp:ht-4is`. Preserve its existing implementation history and review counters. Add scoped work directly; do not create another implementation epic or take over the full run.
- Active linked worktree: `/Users/alepar/AleCode/herdr-threads/.worktrees/herdr-native-mailbox-thread-plugin`. Worktree was clean at inspection.
- Existing integration sweep: `ht-4is.12`. Reuse it when wiring added work; do not create a duplicate terminal sweep.
- Normative cooperative caller amendment already supersedes adversarial native caller proof. System registration fits that cooperative trust model, with an explicit claim rather than executable attestation.
- Current `src/daemon/transport.rs::serve_connection` and canonical daemon spec use one request/response per connection. Persistent service registration requires an explicit transport extension; it cannot be represented solely by another author label.
- Existing commands have no programmatic registration or required-membership operation. Native participant identity already exists; the gap is a durable service author with connection-bound authority.

## Accepted design inputs

One programmatic system participant active per threads instance. Register over a persistent UDS connection; only that accepted connection may exercise system authority. Reject competitors while active. Disconnect releases authority; reconnect registers anew. Durable identity/history survives disconnect and daemon restart, live authority does not. No reusable service secret selected and no strong same-UID adversarial isolation claimed.

System seat/team channels are public, discoverable and unleaveable by their required participants. Graph owns the seat/clone subscription policy and sends notifications as itself. Threads does not need graph groups or clone inheritance. System authority never fabricates recipients' message acknowledgments.

## Design and review progress

User selected initial explicit acceptance, then preventing leave. They clarified that acceptance confirms the agent is active. This is confirmation at acceptance time, not ongoing liveness. Draft at `THREADS-SYSTEM-IDENTITY-DRAFT.md` includes the state machine and proposed lifecycle, receipt, replay and recovery details.

Fresh promotion review by `/root/identity_promotion_review`: initial ISSUES, corrected and re-reviewed COMPLETE. A is LEAF; B/C/D PROMOTE. Corrections: explicit shared authority guard through commit/rollback, DB-before-authority lock ordering, explicit managed topic update, and runtime dependencies on D's future integration leaves rather than its independently startable client work.

Committed threads draft and index/run entry at `3ec84dd`, path `docs/superpowers/runs/2026-09-28-graph-system-identity/design.md` in the active threads worktree. Created direct children `ht-4is.29` (A, task), `.30` (B, epic), `.31` (C, epic), `.32` (D, epic), all deferred. B/C/D depend on A with explicit consumed-artifact reasons and carry `sp:needs-design`. The additions are not implementation-ready. Human top-split approval pending; coverage rounds 0; roast rounds 0.

## Remaining workflow

Obtain the scoped top-split approval, design/decompose B/C/D, complete coverage review, wire existing integration sweep `ht-4is.12`, finalize the amendment/tasks and notify main. Approval of this addition is not inferred from the old epic's approval. No implementation code has changed and no agent notification has been sent yet. Threads draft preserves user decisions and labels additional policy details as proposals (including joined-voluntary upgrade acceptance, managed lifecycle controls, no-ACK service notifications and generation-targeted operator recovery).

## Environment continuity

Latest non-login shell had `HERDR_ENV=1`, but no values for `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID` or `HERDR_PANE_ID`. Rediscover explicit targets before messaging; do not assume earlier w8 IDs establish current continuity. Do not restart the memory observer.

### Approved and decomposed

User answered “lgtm” to the concrete split and draft details. Approval recorded in the threads run. Nested specs committed at 4eb19c9; nine leaves total (.29, .30.1–2, .31.1–3, .32.1–3), all deferred until handoff. All feed existing integration sweep .12. Fresh promotion reviews complete; .32.2 retained as cohesive integration leaf with explicit sp:demoted-by-session rationale. Coverage round 1 underway, three independent reviewers. Initial packet delivery twice truncated; recovering the identical frozen input in bounded chunks, not treating incomplete reads as clearance. Roast preference question pending; no roast run yet.

### Coverage complete; roast requested before handoff

Final three fresh coverage reviewers read full inputs and reported zero findings; all nine requirements mapped. Two round-1 missing consumed-artifact edges were fixed (.31.3 -> .3.8 materializer; .32.3 -> .11.2 native fixture), and the final round verified both. Blocking graph checked acyclic. Finalized design commit 62c5091, then user answered the pending optional question requesting adversarial roast. This supersedes the earlier coverage-only default. All 12 scoped nodes (nine leaves, three epics) returned to deferred before any handoff message.

Roast iteration 1 recorded in threads run.md. Coordinator /root/identity_roast_coordinator running report-only skill pipeline; artifacts /tmp/identity-roast-1/. No Workflow tool, manual subagent fanout, actual same-family GPT review. Root awaits report and all escalation/qualifier sections before deciding next steps. Do not restart the roast counter.

Live target rediscovered: workspace w4 label herdr-threads, tab w4:t1 label main, pane w4:p1 occupied by Codex. No message sent yet; reverify target before final authorized notification.

### Autonomous continuation

User explicitly switched this run to autonomous mode and authorized chatting with implementing agent as needed. Main target w4:p1 reverified working; coordination message submitted explaining new scope is deferred pending roast, existing work continues, and requesting integration concerns via /tmp/herdr-threads-graph-feedback.md if needed. Submission is not receipt proof. Final scope handoff remains pending.

Roast 1 complete: eight scouts, eight raw findings, two deduped, six judge seats, zero confirmed findings. Release replay candidate unanimously rejected; one material-dissent escalation on name/ID collision wording. Root clarified the ambiguous sentence to existing ID-based semantics and assigned its concrete cases to .31.2. Original report preserved unchanged; original dissenting ground and fresh refute seats checking only this resolution. Same-family GPT only, no cap losses or dead stages. No implementation/code changes.

### Complete design and submitted handoff

Final design/review commit cb7b150. Targeted resolution verified by original ground and fresh refute, no unresolved findings. One roast round, no further full roast because clarification was mechanical. Nine leaves and three intermediate epics reopened for main agent scheduling; existing sweep preserved. Final user-authorized handoff submitted successfully to reverified herdr-threads/main (w4:p1); submission is not proof of receipt or implementation. Root checked .31.2 acceptance text after targeted reviewers used an earlier task snapshot. All actual implementation and native evidence remain future work owned by main. Skill-workflow feedback analysis is being kept local.

Final bookkeeping commit 421440b: committed handoff text/run state and local workflow feedback draft. All required artifacts verified present; threads worktree clean at final check; 12 scoped nodes remain open. Final handoff submission succeeded; no explicit recipient acknowledgement observed before ending this run. No further action required from user to complete this design request.
