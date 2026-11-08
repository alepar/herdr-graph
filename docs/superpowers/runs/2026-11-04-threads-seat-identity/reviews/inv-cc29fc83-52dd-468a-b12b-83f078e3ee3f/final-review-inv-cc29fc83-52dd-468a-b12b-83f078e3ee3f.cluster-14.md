# Final review, cluster 14 (src-reconcile), epic hg-66u

Invocation inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f. Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61,
head e4984a492e2d3ce8344b9be83d56a3471049fd60 (both verified with `git rev-parse --verify`). Review source
binding ran first and returned the same invocation/base/head triple.

Files: src/reconcile/herdr_exec.rs, src/reconcile/planner.rs, src/reconcile/session.rs, src/reconcile/tests.rs
(slices 000050-000053). Spec: docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md
at head, sections 5, 7, 13, 14 and the Requirement baseline (B1-B11, no human scope change rows).

Caller checkout observed before and after: branch super-auto/threads-seat-identity, HEAD e4984a4, clean. A
review-owned detached worktree at the head (`.worktrees/review-c14-cc29fc83`) was used for reproductions and
removed afterwards.

## What the cluster changes

- Launch env (spec §5, pre-D1 part). `StartAgent` and `Relaunch` gain `env`. The executor's `start` and
  `replace_session` paths pass `agent_env(&clone, false)` (HERDR_GRAPH_CLONE only; the continuity key is
  withheld because no reservation exists before D1, matching "no key is emitted before D1"). A new journal meta
  `launch_env:<clone>` records the pane and the exported variable names after a Done/BlockedNeedsHuman start,
  after a lost-response start that turned out to have run, and after a replacement that started an agent.
  The status side that reads it (hg-66u.38) is in another cluster.
- Tabs and workspaces across incarnations (spec §7). `LiveIndex::workspace` and `tab_for_seat` now match a
  bound Herdr id in any incarnation; a bound id from an older incarnation that is absent now returns None
  instead of falling through to token/clone-pane fallbacks. `tab_owner` drops its incarnation filter.
- Tests: env assertions on four existing start/replace tests, a new launch_env record test, and two
  LiveIndex tests (bound ids survive a cold restart without tokens; absent ids are not re-found by tokens).

## Tracing against the spec and baseline

- §5 / B6 / B9: env reaches Herdr only through the port; the shell export lives in the Herdr adapter
  (src/herdr/client.rs, another cluster), so the executor's existing guard still runs before the export.
  `HERDR_GRAPH_CLONE` is set on every graph start (Unchanged behavior row). The continuity key is hard-coded
  off at three sites with a pointer to hg-66u.13; turning it on is gated D1 work (hg-66u.44), consistent with
  the global constraint not to fake D1. B9 delivery therefore remains with the gated subtree, not this cluster.
- §7 / B4 / B5: across a live handoff or restore Herdr keeps tab and workspace ids and drops tokens; before
  this change the planner filtered bindings by incarnation and so lost the seat tab and teamspace workspace
  after any incarnation change. The new any-incarnation match is what keeps the reconciler from creating a
  duplicate tab/workspace after a handoff. `live_index_matches_bound_tab_and_workspace_across_incarnations`
  covers it with `FakeHerdr::restart(false, false)` (ids kept, metadata dropped). Clone pane placement after
  handoff still requires a same-incarnation binding; re-binding clones comes from SeatObservation/the
  observe matcher, which is the seam reviewer's path to trace.
- Herdr id reuse: the old code distrusted reused Herdr ids across incarnations. The project's Herdr skill
  notes closed tab and pane ids are not reused, and §7 asserts ids are preserved across restore and handoff,
  so dropping the filter follows the spec. A Herdr start without session restore (ids possibly restarting) is
  not exercised anywhere in this cluster.
- Reconcile unit tests at the head: `cargo test --lib reconcile::` in the ht-build toolbox, 58 passed.

## Reproduction of the workspace early return

`LiveIndex::workspace` returns None as soon as the bound id is from an older incarnation and absent. That
return sits ahead of the same-incarnation evidence the function already trusts: the journal live ref and
the create-nonce match, both written by the reconciler's own CreateWorkspace in the current incarnation.
`create_workspace` records only a live ref (the binding is written later by StampToken), and `stamp` looks
the workspace up through `idx.workspace`, so the follow-up StampToken for a workspace the reconciler just
recreated can never find it.

I reproduced this in the review worktree with a temporary test in src/reconcile/tests.rs: activate, step,
`FakeHerdr::restart(false, false)`, then close the workspace (or `renumber_tabs_and_workspaces`), then four
steps. At the head there is one CreateWorkspace, zero ReportWorkspaceMetadata calls, and the teamspace
StampToken row stays Pending with "workspace is not present". The same test with only the
`if b.incarnation != self.snap.incarnation { return None; }` lines removed stamps the new workspace (StampToken
Done, one metadata report). The temporary edits were reverted.

Production reachability: the reconciler runs only after an observe commit (src/observe/step.rs:597), and
every incarnation change forces a rebind pass that marks an unmatched teamspace Unknown, which the planner
skips. So I could find no production path that reaches this state today: the teamspace has to be non-Unknown
while it holds an older-incarnation binding to an id that is absent. It is a latent defect in code this
cluster changed. The fix is local: return None after the live-ref and nonce checks, skipping only the
clone-pane fallback. `tab_for_seat` has the same ordering, but `create_tab` writes the binding with the new
incarnation on success, so tabs recover.

## Observations outside this cluster (for the seam and root reviewers)

- The head still pins third_party/herdr-threads at cdc8423 (v2.6.4) and nothing in the range touches the
  submodule or Cargo.toml, while spec §1 / hg-66u.4 call for v3.0.0 (95e9ebac). This cluster does not depend
  on the pin, so I have not judged it here.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test exercises the reconciler after `FakeHerdr::live_handoff` itself (ids and agents kept, tokens dropped); the cross-incarnation LiveIndex test uses `restart(false, false)`, which also changes pane ids.
- No test covers a Herdr start without session restore, where ids could restart and an older-incarnation bound tab/workspace id could name an unrelated live object, which the any-incarnation match would then adopt.
- The lost-response branch of `start` (effect Unknown, agent already running) records `launch_env` but no test asserts it.
- `record_launch_env` after `replace_session` is asserted only through the StartAgent env, not through the recorded `launch_env` meta.

## Deferred OK
- src/reconcile/planner.rs `LiveIndex::workspace`: the older-incarnation early return runs before the same-incarnation live-ref and create-nonce checks, so a workspace the reconciler recreates in a new incarnation is never found by its StampToken (reproduced in the reconcile fixture: StampToken stuck Pending "workspace is not present"); latent because the observe rebind pass marks such a teamspace Unknown before the reconciler runs; fix by moving the return below the live-ref and nonce checks.
- src/reconcile/herdr_exec.rs: `agent_env(&clone, false)` is hard-coded at three sites (start, lost-response start, replace_session); hg-66u.44 must switch all three to the recorded reservation together so `launch_env` and the actual export agree.
- src/reconcile/planner.rs `tab_for_seat`: an older-incarnation bound id that is present but owned by another seat still falls through to the live-ref and token fallbacks (already disclosed as a Task 4 deferred minor).
- Pin still at herdr-threads v2.6.4 at the head versus spec §1 v3.0.0; outside cluster 14 and handed to the seam/root reviewer, not judged here.
