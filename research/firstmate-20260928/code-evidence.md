# Firstmate code audit versus herdr-graph

Pinned Firstmate commit: `2d833ff147cd26a5c461e914e06854e0eb2707ce`. Read-only inspection on 2026-09-28; no upstream code, hooks, or tests executed. Graph comparison uses current `DESIGN-NOTES.md`, especially its latest scope and reconciliation amendments. These are implementation observations versus a proposed graph design, not a feature parity claim.

## Main finding

Firstmate already implements much of the operational machinery a factory needs, including persistent domain supervisors, generation-aware replacement, durable steering, backend recovery, and scoped journals. The strongest graph distinction is the organizational object model and ownership contract: durable teamspace/seat/clone identity, concurrent conversations of one role, graph-owned relationships, and one authoritative organizational writer with separate reconciliation. Building another worker launcher would substantially duplicate Firstmate.

## code-01: Persistent organization already overlaps

Firstmate has durable secondmate homes and identity/parent bindings, not just disposable task workers. Persistent secondmates are explicitly excluded from backlog items.

Evidence: [bin/fm-parent-channel-lib.sh:80-124](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-parent-channel-lib.sh#L80-L124); `bin/fm-backlog-transition-lib.sh:7-27`. Confidence: high. Limitation: Static implementation and explicit lifecycle contract; not live-tested.

## code-02: Different identity cardinality

The inspected task record has one endpoint, one Herdr workspace/tab/pane tuple, and a replacement spawn generation. Registry rejects duplicate home and ID assignments. Graph instead deliberately adds seat identity with several concurrent clones and separate native sessions.

Evidence: [bin/fm-spawn.sh:4843-4917](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-spawn.sh#L4843-L4917); `bin/fm-secondmate-registry-lib.sh:240-269; DESIGN-NOTES.md:9-12`. Confidence: high. Limitation: This establishes the central record shape, not proof no auxiliary feature anywhere supports multiple conversations.

## code-03: Several mutation owners rather than a graph-wide writer

Spawn publishes task metadata under a per-task lock and pairs backlog transition with publication; parent-channel publishers independently append status, and Herdr event handling independently changes markers. This is concrete distributed script ownership, unlike graph’s proposed single queue/writer/Git committer.

Evidence: [bin/fm-spawn.sh:4843-4940](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-spawn.sh#L4843-L4940); `bin/fm-parent-channel-lib.sh:134-152; bin/backends/herdr.sh:3906-3941; DESIGN-NOTES.md:148-164`. Confidence: high. Limitation: Does not imply Firstmate lacks locking or journals: it has both. No global absence claim.

## code-04: Git/state boundary differs

Firstmate ignores state/, data/, config/, and secondmate home/parent markers in its own Git repository. Graph intends authoritative organizational files and its operational journal to be Git tracked through one committer.

Evidence: [.gitignore:1-13](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/.gitignore#L1-L13); `DESIGN-NOTES.md:148-150`. Confidence: high. Limitation: Ignore rules do not prove an operator cannot separately version those directories; claim is repository default.

## code-05: Recovery is substantive

Firstmate preserves task metadata across relaunch, records fresh spawn generations, and has metadata/backlog crash recovery with teardown markers. It should not be described as ephemeral or without recovery.

Evidence: [bin/fm-backlog-transition-lib.sh:7-56](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-backlog-transition-lib.sh#L7-L56); `bin/fm-spawn.sh:4843-4917; docs/agent-control.md:93-139`. Confidence: high. Limitation: Code/contract inspection only; no independent fault-injection test.

## code-06: Herdr projection versus organizational semantics

Firstmate’s projected task workspace is expressly a best-effort presentation, not ownership/lifecycle authority. Existing task operations retain recorded endpoints through label changes. Graph treats UI renames, closures, and cross-tab moves as organizational changes, subject to best-effort observation.

Evidence: [docs/herdr-backend.md:155-161](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/docs/herdr-backend.md#L155-L161); `docs/herdr-backend.md:243-257; DESIGN-NOTES.md:145-164`. Confidence: high. Limitation: Comparison is documented Firstmate behavior versus graph approved design, not two shipping capabilities.

## code-07: Reconciliation has different target

Firstmate secondmate reconciliation is a durable, cooldown-limited instruction to the owning home to repair backlog/metadata drift. Parent explicitly cannot edit child records. Graph reconciliation drives latest organizational intent into Herdr and threads and observes manual changes back into state.

Evidence: [bin/fm-secondmate-reconcile.sh:11-74](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-secondmate-reconcile.sh#L11-L74); `DESIGN-NOTES.md:162-179`. Confidence: high. Limitation: Only this reconciliation subsystem is characterized; Firstmate also has other recovery systems.

## code-08: Messaging is already durable

Firstmate normal steering persists sequenced inbox records and sends a terminal doorbell best-effort; moving to handled acknowledges action. This is more than blind typing. Native commands/explicit targets retain a typed plane.

Evidence: [bin/fm-send.sh:18-83](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-send.sh#L18-L83); `bin/fm-task-inbox-lib.sh:2-57`. Confidence: high. Limitation: Documented code contract corroborated by inbox implementation entry points; not throughput/durability tested.

## code-09: Communication shape differs

Parent-channel routing resolves from durable home identity to parent status file (or remote relay file), with scripts publishing facts. Graph intends public discoverable threads with seat/clone subscriptions and a distinct graph service author.

Evidence: [bin/fm-parent-channel-lib.sh:95-152](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/fm-parent-channel-lib.sh#L95-L152); `DESIGN-NOTES.md:12-15; DESIGN-NOTES.md:139-141`. Confidence: high. Limitation: Scoped routing comparison; not claim Firstmate forbids arbitrary agent conversation.

## code-10: Identity caveats offer useful lessons

Herdr live-discovery explicitly uses fm-prefixed tab labels in a home workspace because stored pane IDs are not guaranteed across server lifecycles. Projection recovery separately checks exact metadata consistency before replacing endpoints. Graph should borrow these uncertainty distinctions without adopting labels as durable seat identity.

Evidence: [bin/backends/herdr.sh:3785-3806](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/bin/backends/herdr.sh#L3785-L3806); `bin/fm-spawn.sh:3370-3425`. Confidence: high. Limitation: Read-only recovery discovery differs from authority for mutation; do not conflate them.

## Reuse and integration implications

Reusable design material: exact endpoint qualification, distinction between absent/unreadable endpoints, spawn-generation fencing, idempotent inbox records, separate message persistence and wake signals, owner-local recovery, and script-published facts that do not depend on a model remembering to report. These are lessons to adapt, not verified drop-in components.

Potential integration: treat a Firstmate home as an execution/factory adapter beneath a graph seat or team, retaining Firstmate ownership of task worktrees and task lifecycle. Use its documented fleet ledger for observational events only: its own contract warns about duplicate delivery, missing history, no sequence/gap detection, and no forced flush. It cannot be graph’s authoritative mutation journal without a new contract.

Main friction: both systems would create/close Herdr objects; Firstmate defaults to per-task presentation workspaces while graph has durable organizational workspaces/tabs. Both can recover missing endpoints with different intent interpretations. Assign one owner to each Herdr object and lifecycle, translate Firstmate task IDs into graph attribution, and decide which changes may flow back before attempting integration. Firstmate’s per-home state and parent routing do not directly implement graph’s global registry, concurrent clones, or threads membership semantics. Those require an adapter or upstream extension, not renaming a few roles.

Evidence boundary: inspection sampled the named core paths, not every script. No assertion of universal feature absence or runtime reliability is made. Graph itself is a design here; its recovery/identity guarantees remain to implement and test.

## Counterevidence and deeper body inspection

Firstmate **does** support multiple simultaneous conversations: [Pi supervision branch](https://github.com/kunchenguid/firstmate/blob/2d833ff147cd26a5c461e914e06854e0eb2707ce/docs/pi-supervision-branch.md#L28-L75) runs supervision beside captain chat, with opt-in non-Pi hosting too. Graph’s distinction is general named seat/clone lifecycle and participation, not inventing parallel conversations.

Function bodies inspected beyond headers: `fm-task-inbox-lib.sh:145-230` temp-writes/atomically renames records, holds a sequence lock, and scans pending plus handled records for remote idempotent enqueue. `fm-secondmate-reconcile.sh:310-340,470-545` publishes per-target request files, checks lifecycle and metadata locks, revalidates identity, and actually invokes `fm-send --fire-and-forget`. Parent-channel body directly appends to the selected status path; spawn body writes the single endpoint tuple. These support the narrower implementation claims above.
