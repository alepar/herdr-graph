# Final review, cluster 16 (src-reconcile), epic hg-66u

Invocation inv-4aa37d70-023c-483a-b47c-3dc801a586fe. Captured source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61,
head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify <sha>^{commit}`). The
review source binding (`dispatch-text.mjs bind-review-source`) exited 0 and echoed the same triple before this file
was written.

Files: src/reconcile/herdr_exec.rs, src/reconcile/planner.rs, src/reconcile/session.rs, src/reconcile/tests.rs
(slices 000077-000080). Spec: docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md
at head (Requirement baseline B1-B11, no human scope change rows; sections 5, 7, 9, 10, 13, 14). Bead state read
with `bd show` / `bd list --parent hg-66u` at review time.

Caller checkout observed before and after: HEAD 6576a11, clean. A review-owned detached worktree
(.worktrees/review-c16-4aa37d70, target seeded by reflink, submodule at 95e9ebac v3.0.0) was used for test runs
and probes and removed afterwards.

## What the cluster changes

- Launch env, pre-D1 part of §5 (hg-66u.37/.38). `StartAgent` and `session::Relaunch` gain `env`. The executor's
  `start` (also serving RelaunchOccupant) and `replace_session` pass `agent_env(&clone, false)`, i.e.
  HERDR_GRAPH_CLONE only; the continuity key is withheld because no reservation exists before D1. A journal meta
  `launch_env:<clone>` records the pane and exported variable names after a Done/BlockedNeedsHuman start, after a
  lost-response start found already running, and after a replacement that starts an agent. Its only consumer is
  `transcripts::capture::check_export` (another cluster), which filters by pane, so a stale record from an older
  pane or a later shell-harness switch is harmless.
- Tabs and workspaces across incarnations, §7 (hg-66u.11). `LiveIndex::workspace` and `tab_for_seat` match a
  bound Herdr id in any incarnation; a bound id from an older incarnation that is now absent returns None instead
  of falling through to token/clone-pane fallbacks. `tab_owner` drops its incarnation filter.
- Tests: env assertions on four start/replace tests, a launch_env record test, two LiveIndex cross-incarnation
  tests, and (new since the previous final review) `graph_retire_of_a_clone_with_an_open_pane_closes_it`, which
  pins §14.10 (a graph retire closes the clone's pane).

## Verification run

- `cargo test --lib reconcile::` in the ht-build toolbox at head: 59 passed, 0 failed.
- Probe (temporary test, reverted): `activate`, `step`, `FakeHerdr::live_handoff`, then LiveIndex and one reconcile
  step. The seat's tab (t2) and the workspace (w1) are found by preserved id; the step issues only
  ReportWorkspaceMetadata and ReportPaneMetadata (re-stamping), no CreateWorkspace/CreateTab/SplitPane. So the
  reconciler does not duplicate containers after a handoff, which is the part of B5 this cluster owns.
- Probe (temporary test, reverted): `restart(false, false)`, then `user_close_workspace`, then four reconcile steps.
  Result: one CreateWorkspace (Done), the teamspace StampToken stuck Pending with "workspace is not present", and
  the seat's CreateTab left Pending. Cause: `LiveIndex::workspace` returns None for an older-incarnation absent
  bound id before it consults the same-incarnation live ref and create nonce that `create_workspace` just wrote,
  and `create_tab` also resolves the workspace through `idx.workspace`. This confirms the previous reviewer's
  finding and widens it: the whole teamspace subtree stalls, not only the stamp.
- Reachability of that stall: the observe rebind pass (src/observe/step.rs, `if rebind` over `m.unmatched`) marks
  such a teamspace Unknown before the reconciler runs, `implied` refuses CreateWorkspace for an Unknown teamspace,
  and a matched teamspace has its binding rewritten with the current incarnation. I found no production path that
  gives the planner a non-Unknown teamspace with an older-incarnation absent binding, so it stays latent.

## Tracing the requirement baseline through this cluster

- B6: env reaches Herdr only through the existing port; the shell export lives in the Herdr adapter
  (src/herdr/client.rs, another cluster), after the executor's guard, as §5 requires. Delivered for this cluster.
- B2, B8: no change in this cluster's behavior; B8's seat retention on agent exit is the threads-participation
  cluster's path (seam reviewer).
- B4/B5, containers: tabs and workspaces keep their bound ids across a handoff or restore (probe and
  `live_index_matches_bound_tab_and_workspace_across_incarnations`). Delivered.
- B4/B5, unattended agent recovery: §9's resume driver (restart trigger after a headless cold restart) belongs to
  hg-66u.30, whose file list is src/reconcile/planner.rs, desired.rs, herdr_exec.rs and tests.rs. That bead is
  open, is not gated on any threads release (its session path is "before D1"), and no restart or restore trigger
  exists at head (`git grep -i 'resume driver|restart trigger'` over src finds nothing). After a headless cold
  restart nothing in the reconciler relaunches the agents the clones had. Not delivered.
- B9: every production start path in this cluster (herdr_exec.rs `start` normal and lost-response branches,
  `replace_session`) hard-codes `agent_env(&clone, false)`, so no agent is ever launched with
  HERDR_THREADS_CONTINUITY_KEY. The keyed launch is hg-66u.44 (open, blocked by gate hg-66u.1, D1 unreleased) after
  hg-66u.43 (open). Withholding the key until a reservation exists is the correct safety property per §5 and must
  not be weakened; the row is nonetheless undelivered at head.
- B7/B10: the restore launch (one launch per Restore epoch, §14.7) is the resume driver's restore trigger
  (hg-66u.30, open) and the D2 un-retire path (hg-66u.31/.32, gated on hg-66u.2). The reconcile side does not exist
  at head; the seat-intent and un-retire parts are the threads cluster's and the seam reviewer's to trace.
- B1/B3: clone location from threads (SeatObservation, hg-66u.28) is the observe cluster's path; LiveIndex clone
  re-matching deletion (hg-66u.14) is gated on D1. Seam reviewer's to trace.
- B11: /seat output, another cluster.

## Verdict
not ready (B4/B5 resume driver hg-66u.30 and B9 keyed launches hg-66u.44 are absent from the reconcile launch path at head)

## Must fix before landing
- B4/B5 (unattended resume after a headless cold restart or handoff): the resume driver's restart trigger and launcher (hg-66u.30, ungated, files src/reconcile/planner.rs and herdr_exec.rs) is not on head; no reconciler path relaunches a clone's agent after a headless cold restart. Land hg-66u.30 or hand the row back as open.
- B9 (keyed launches): src/reconcile/herdr_exec.rs `start`, its lost-response branch and `replace_session` all pass `agent_env(&clone, false)`, so no production start exports HERDR_THREADS_CONTINUITY_KEY; delivery is hg-66u.44 after hg-66u.43, blocked by gate hg-66u.1 (D1 unreleased). Do not emit an unreserved key to get around the gate; hand the row back as open until D1 ships and hg-66u.44 lands.
- B7/B10 (undo relaunch onto the same seat): the restore launch trigger (hg-66u.30 restore arm, §14.7) is absent from the reconciler at head, and the D2 un-retire beads hg-66u.31/.32 wait on gate hg-66u.2; the threads-side intent path is the seam reviewer's to trace.

## Untested scope
- No reconcile test runs after `FakeHerdr::live_handoff` itself; the committed cross-incarnation LiveIndex test uses `restart(false, false)`, which also renumbers pane ids (this review's handoff probe passed but was not kept).
- No test covers a Herdr start without session restore, where tab/workspace ids could restart and an older-incarnation bound id could name an unrelated live object that the any-incarnation match would adopt.
- The lost-response branch of `start` (effect Unknown, agent already running) records `launch_env`, but no test asserts it.
- `record_launch_env` after `replace_session` is asserted only through the StartAgent env, not through the recorded `launch_env` meta.
- The brief's guard-before-start ordering (process_info checked before the env export and agent.start) is not asserted; the fake does not record ProcessInfo (Task 7 deferred minor).

## Deferred OK
- src/reconcile/planner.rs `LiveIndex::workspace`: the older-incarnation early return runs before the same-incarnation live-ref and create-nonce checks, so a workspace the reconciler recreates in a new incarnation is never found by its StampToken or by the seat's CreateTab (reproduced: StampToken Pending "workspace is not present", CreateTab Pending); latent because the observe rebind pass marks such a teamspace Unknown first; fix by moving the return below the live-ref and nonce checks.
- src/reconcile/planner.rs `tab_for_seat`: an older-incarnation bound id that is present but owned by another seat still falls through to the live-ref and token fallbacks (Task 4 deferred minor).
- src/reconcile/herdr_exec.rs: `agent_env(&clone, false)` appears at three sites; hg-66u.44 must switch all three to the recorded reservation together so `launch_env` and the actual export agree.
