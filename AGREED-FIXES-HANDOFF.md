# Agreed fixes and release handoff — 2026-10-07

The user explicitly requests sending all agreed fixes to the existing `implementor` tab, getting them merged to main, and having the existing `mod` tab cut a new release. This resumes implementation for this scope and authorizes merge/push and release publication. Keep those owners sequential: implementor owns implementation/integration, mod owns the release after verified integration.

## Approved changes

1. **Reusable seat templates instead of profession roles.** Researcher/designer/engineer/reviewer definitions are reusable seat templates shared across team templates. Team members reference them and add their stable member ID/name, responsibility, startup choice, runtime overrides and participation. Independent instantiations have independent seats; sharing definitions is not sharing an actual seat. Actual reuse remains explicit in application mappings.
   - Runtime precedence: seat instance > team member > seat template > team template > graph defaults > applicable built-ins.
   - `/seat` exposes reusable instructions, team specialization, instance context and scoped rules for the agent to read. No assumed string-interpolation engine or generated semantic brief.
   - Remove profession-role machinery/unused `role_ref`. Replace confusing role terminology as needed, preserving system-duty behavior, designated summarizer discovery and source-summary eligibility; no separate profession registry.
   - Preserve live references, overrides/exclusions/member correspondence, reviewable immediate runtime changes and undo.
   - Current decisions/rationale: final section of DESIGN-NOTES.md, Templates section of CURRENT-DESIGN.md, and CREATING_NEW_GRAPH.md.

2. **Human-browsable transcript index grouping.** Replace `transcripts/<seat-id>/<transcript-id>.toml` with:
   `transcripts/<team-name>-<team-id>/<seat-name>-<seat-id>/<transcript-id>.toml`.
   Use safe name slugs and full stable IDs. Names can change without changing identity; keep enumeration, lookup, rename/move/retirement/undo behavior consistent. Historical attribution in transcript records must survive organizational changes. These are indexes of external native transcripts, not copies of their conversation content. Choose consistent mechanics for historical versus current folder names, document them and retain discovery of existing records.

3. **Purpose-specific operational record paths.** Group the previously generic top-level folders under `mutations/` using the recommended names accepted by the user's request for all agreed fixes:
   - `mutations/graph-changes/<yyyy-mm>/<op-id>.toml` — formerly operations; committed mutation records.
   - `mutations/undoable-actions/<yyyy-mm>/<action-id>.toml` — formerly actions; grouped changes/compensation provenance.
   - `mutations/transcript-processing/<request-id>.toml` — formerly requests; processing delivery/dispatch/results, whose status changes via mutations.
   Preserve stable IDs and semantics; update path readers/writers, CLI/skills/examples and tests. Requests remain transcript work, not generic mutation requests. Live/rejected/failed/cancelled operation lifecycle remains in the local SQLite journal, not reconstructed solely from Git summaries.

4. **Documentation and review artifacts.** Include the current uncommitted CURRENT-DESIGN.md, DESIGN-NOTES.md, GRAPH_STATE.md, CREATING_NEW_GRAPH.md and the October 5 graph-state roast/report evidence. GRAPH_STATE.md presently describes the old implementation; update it to match the final implemented paths/model. Its two supported corrections are already applied: pending request ranges can merge before delivery, and long instance paths use `/private/tmp/herdr-graph-<uid>/<hash16>.sock`.
   - Historical review: docs/superpowers/reviews/2026-10-05-graph-state-roast-design-1.md. No significant findings confirmed; preserve the report as historical evidence, not a demand to implement rejected candidates.
   - Include this handoff so the release owner can check scope and provenance.

5. **herdr-threads 0.2.9 compatibility.** User additionally requests syncing graph with herdr-threads, reported current at 0.2.9. Verify the actual tag/version and current public API in `/Users/alepar/AleCode/herdr-threads` and its release artifacts; update graph's dependency/integration, lockfile and documentation as appropriate. Check service registration/ACK delivery, memberships/invitations, notifications, discovery and existing fallback semantics against the new version. Run isolated real-daemon integration tests, distinguish tested behavior from assumptions, and include the exact threads version/SHA in the integration and release handoff. Coordinate with the existing threads session only as needed for this explicitly requested sync; do not disturb its work or restart live services.

## Execution and compatibility

Inspect current code/main and repository guidance first. Preserve existing local design edits/untracked files; bring these authorized artifacts into any implementation worktree rather than dropping them. Use the appropriate planning/execution/review workflow and meaningful tests. No need to reconfirm settled choices or ask whether merge/release is authorized.

Existing graph instances contain persistent real state: use explicit recoverable compatibility/migration mechanics for changed schemas and paths, preserve native transcripts/history/undo records and avoid silently stranding old records or invalidating old compensation paths. Document any limitations. Implementor should finish tests/review and merge/push verified changes to main. Do not deploy into live sessions or restart memory observers/shared servers to validate this work.

After integration, deliver the exact main SHA, tests/review results, migration notes and release-relevant changes to `mod` through Herdr (install/read `herdr --skill`; identify its current pane from the tab). Explicit user authorization covers this internal handoff. Coordinate with mod to avoid simultaneous changes to the shared checkout.

## Release owner (`mod`)

The user authorizes cutting a new herdr-graph release after this scope is merged. You may prepare by reading current release conventions, but do not publish an earlier main while implementor is working. Verify the handoff SHA and that main contains all scoped changes, then perform the established version/tag/changelog/build/publish process. Determine the version according to repository policy and compatibility impact; do not guess a requested version. Report version, tag/SHA, release URL and verification evidence. Coordinate publication blockers directly with implementor or user as appropriate. Publishing a release does not authorize unrelated live plugin/server restarts.
