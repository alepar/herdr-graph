# Resume the design discussion

## Completed scoped threads design

User requested `$super-design` for graph identity in threads, then approved its split/details, requested an adversarial roast and authorized autonomous continuation/coordination. Skills installed. Design and evidence committed through cb7b150 in the threads implementation worktree at `docs/superpowers/runs/2026-09-28-graph-system-identity/`. Existing epic ht-4is gained .29–32 (nine leaves, three subepics), all open for main-agent scheduling and feeding existing .12 sweep. Two coverage rounds finished clean; one full roast confirmed no defects and its wording dissent was resolved by targeted reviewers. Final handoff submitted to herdr-threads/main w4:p1. No implementation code changed; no native proof or work completion inferred. See `THREADS-IDENTITY-DESIGN-RUN.md` for details.


As of 2026-09-28. User wants interactive design only. No implementation authorized. Read these files in order:

1. `HANDOFF.md` — original seed verbatim and research map. Historical context, not the latest decisions.
2. `DESIGN-NOTES.md` — accumulated decisions and explicit supersessions.
3. `HERDR-IDENTITY-CHECK.md` — installed Herdr 0.9.1 evidence and limitations.
4. `DESIGN-ROAST-2026-09-27.md` — historical quick review; subsequent decisions resolve some findings, not an implementation clearance.

## Latest settled direction

Scope clarification: graph tracks teamspaces/seats/clones and their thread relationships, plus templates/rules and necessary lifecycle/reconciliation metadata. It is not another project-work or knowledge tracker. Work remains in native sessions, Beads and project wikis; unresolved graph requests concern graph mutations, not all project commitments.

Latest-intent reconciliation is approved: stop obsolete retries, retain history, and reconcile completed effects toward current committed intent. Missing some manual Herdr actions is explicitly acceptable; no complete observation guarantee. Explicit reassignment/cancellation of unresolved graph requests is approved; retiring a requester preserves unresolved operations and attribution, with takeover policy in instance rules.

Latest continuation: manual Herdr closure retires/archive corresponding objects with provenance. Plugin alone writes AND commits graph state through a durable serialized queue; later failures notify/remind caller with linked resubmissions. Graph desired state is persisted first, then core durable reconciliation drives Herdr/threads; this supersedes optional-only reconcile-skill scope. Reconciliation is separate from the serialized writer and runs on state changes, Herdr events and periodic checks in both directions; external failures do not block unrelated commits. Separate shared-seat and clone files. Cross-tab moves initially require destination-seat reload, preserving transcript but replacing applicable graph state; eventual transfer preserves work history. See the final section of DESIGN-NOTES.md for details and unresolved consequences.

- Flat teamspace = workspace; seat = tab; clone = pane; native agentic session occupies clone. Several clones can serve one seat concurrently. Work attribution includes all three identities.
- Human-readable graph state in one shared Git checkout, separate from project code repos; shared cross-project Beads tracking. All graph-state writes now go through a validating, serializing plugin API/CLI, including templates/rules/notes. No validation Git hook needed. Do not revive the earlier direct-edit proposal.
- Best-effort event observer plus periodic snapshots follows Herdr renames; workspace/tab renames change graph names/state paths with history. Durable IDs remain stable. Path lookup and plugin-mediated writes avoid relying on stale filenames.
- Templates compose teams/seats/groups and thread relationships; agent compiles concrete plans. Plugin validates and applies best effort. Agent creates smaller repair plans from outcomes. No automatic model wake scheduler.
- Rules are global/team/seat instruction text; interpretation, escalation, retirement authority are instance policy, not hard-coded plugin rules.
- Graph owns whole-seat versus individual-clone subscriptions. New clones inherit whole-seat subscriptions only. Whole-seat leave updates intent, requester leaves, system-channel notification tells other clones to leave individually. Completion may be pending.
- Public/discoverable system seat/team channels; system channels are unleaveable. Earlier private channel idea is superseded.
- Threads needs programmatic system author and required-membership support. Approved auth direction: graph registers over a persistent UDS connection; only that connection has system authority; one active system participant. Competing registration rejected; disconnect releases authority; reconnect registers again. Durable author history persists. Cooperative trust: registration is a claim, not proof of executable identity. No reusable service credential selected.
- Current threads now uses a cooperative caller amendment. Earlier claims that native adversarial proof is required were corrected. Existing participant identity is not absent; the missing capability is background service authorship.

## Next useful questions

- Required membership now explicitly needs initial native acceptance, then prevents voluntary leave. Acceptance is historical activity evidence, never a fabricated ACK or ongoing liveness proof.
- Narrow threads integration design is complete and handed off; follow its linked specs/evidence rather than the historical local draft.
- Concrete mutation API/revision checks, Git commit coordination and Beads representation; file format still open.
- Cross-tab pane moves now cross seat boundaries. Detect discrepancy; transfer policy unresolved. Herdr 0.9.1 `tab.move` only reorders within a workspace; `pane.move` can cross tabs/workspaces.
- Clone-local versus seat-shared working state, retired-owner routing, plan unknown-outcome recovery, minimal shipped skills.

Keep research read-only, preserve seed and experiments, and do not restart the claude-mem worker. Its previously reported quota outage is not fixed by restarting. Reverify inherited Herdr context in a non-login shell if live control is needed; do not assume old w8 IDs establish current continuity.
