# Quick whole-design roast — 2026-09-27

## Scope and method

Design discussion review, not implementation clearance. Internal cooperative local tooling. Inputs: DESIGN-NOTES.md, HANDOFF.md, HERDR-IDENTITY-CHECK.md, and the current herdr-threads design/protocol. Three fresh focused scouts (state, messaging, model) and one fresh refutation/verification reviewer; root checked supporting source. Same-family reviewers, not independent model families. This deliberately reduced pass follows the user's request for a quick roast; it is not a completed formal super-roast panel workflow. No implementation or design fixes applied.

Outcome: four concrete Should-fix gaps, two existing open decisions worth prioritizing, and documentation drift. No reason found to discard the overall architecture. That is a limited review conclusion, not a guarantee of completeness.

## Should-fix gaps

### 1. Background graph operations lack a herdr-threads authority path

Graph promises plugin-created notifications, channels and invitations (DESIGN-NOTES.md:13,17,35). Threads currently permits ordinary create/invite/send only with native caller authority. Its operator commands cover rebind, fresh participant allocation and orphan invitation, not general publishing or invitations into populated threads.

Example: user renames a workspace while all agents are unavailable. Graph can update files, but has no specified supported caller for sending the resulting notification.

Evidence: herdr-threads canonical design lines 17,31,33,57,71; `src/protocol/commands.rs` operator conversion and PermitMutation enum, lines 287–344. Both under `/Users/alepar/AleCode/herdr-threads/.worktrees/herdr-native-mailbox-thread-plugin/`.

Resolution options: a narrow programmatic service-author integration, or pending operations completed by an eligible agent. No need to introduce graph seats/clones into the transport model. Agent-driven hydration already has a possible native caller; unattended work is the gap.

### 2. Whole-seat leave is intent, not immediate transport completion

DESIGN-NOTES.md:11 promises all current clones leave. Threads `Leave` takes thread, operation and caller claim, with no target participant; it requires a native permit (`src/protocol/commands.rs:195`, PermitMutation). Clone A cannot use the existing operation to immediately remove clone B.

Resolution: record collective intent and report pending per-clone leaves until each executes, or request a narrow transport extension. Until then, B can still receive messages. Outstanding receipt obligations also survive leave under the threads contract.

### 3. A pane moved between tabs crosses a seat boundary

DESIGN-NOTES.md:9 binds seat to tab. Existing move guidance at :98 and HERDR-IDENTITY-CHECK.md:25 checks teamspace changes but does not expressly cover another tab in the same workspace.

Example: an engineering clone is dragged into the designer's tab. Its terminal ID remains the same, but its placement conflicts with seat identity, role and subscriptions.

Resolution: flag cross-tab movement as a seat-binding discrepancy. Let a plan resolve movement versus reassignment; terminal continuity does not authorize role transfer.

### 4. Path lookup plus pre-commit validation does not protect concurrent writes

DESIGN-NOTES.md:15–16 requires name-derived paths and current-path lookup, :27 shares a worktree, and the observer updates bindings/labels in the background.

Example: clone resolves its state path, observer renames the directory, clone writes the old path and recreates stale state. Two clones editing shared seat state can also lose each other's updates.

Resolution: choose a small write/rename coordination contract, with durable-ID addressing and revision checks where appropriate. The agreed staged-state validator remains useful but checks structure, not lost updates or write ownership. Shared Git staging/commit coordination must be covered too.

## Existing open decisions to settle

### 5. Unknown external outcomes during plan recovery

DESIGN-NOTES.md:42–48 already disallows blind retries and identifies unknown outcomes as open. An external create can succeed before its result is journaled; the repair agent then needs enough evidence to find it rather than duplicate it. Preserve operation intent before submission, record results, and inspect unknown effects before retrying. This is an acknowledged gap, not a new contradiction.

### 6. Retirement and responsibility discovery

DESIGN-NOTES.md:8,20–21,82 leaves what routing returns for a retired owner undecided. A later regression may still match that seat's fence. Define whether discovery shows an archived owner, no active owner, or a configured successor; instance instructions decide the response. Outstanding commitments and original attribution must remain visible.

## Documentation drift

- The conversation accepted direct edits for instructions/templates/notes, structural mutations through plans, and a pre-commit validator over the staged graph. These decisions are missing from the notes. Do not misreport them as undecided product behavior.
- Historical single-seat-per-pane review statements are expressly superseded; they are not current contradictions, but a consolidated specification should remove their ambiguity.
- Some examples still say all team seats join channels; operationally this now means graph seat intent resolved to clone participants.

## Rejected or accepted tradeoffs

- Template updates temporarily reaching clones at different times is accepted: latest template on load, optional PSA for active sessions. It is not a new defect.
- Best-effort events plus snapshots need not guarantee every rename before a crash. This was explicitly accepted; finding 4 concerns write races, not demanding perfect observation.
- Public channels are compatible with the current threads discovery design. Privacy machinery is no longer needed.
- Instance-specific hierarchy, retirement permissions and rule conflict resolution belong in the rulebook. Their flexibility is intentional, not missing plugin enforcement.
- Graph-owned seat-wide subscriptions do not require transport-native groups. Per-clone actions and eventual completion must be explicit.

## Recommended next discussion

Resolve the graph-to-threads programmatic authority contract first; it affects generated notifications, invitations and collective leave. Then settle shared state mutation/rename coordination. The remaining items fit the existing plan-and-recovery design rather than requiring a new architecture.
