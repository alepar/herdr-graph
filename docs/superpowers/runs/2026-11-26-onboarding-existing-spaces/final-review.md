# Final review: epic hg-ihi (onboarding existing Herdr/threads spaces)

Invocation inv-7e4115e1-8c83-4f19-8651-2e539741d62f. Integration branch epic-hg-ihi-integration.
Captured source: base 9d70b6f4fcac29a1f668dfd57b6a5eb6433cf1db, head 079d106dac642c2f91a1f2e770332b740d421cf0 (both verified with `git rev-parse --verify <sha>^{commit}`; the review source binding returned rc=0).

## Inputs and route

- review-inputs.json: base and head match the captured source, sourceAvailable true, status complete, 51 selected paths, 0 omitted, 0 gaps. wholeRangeReady is false only because of 58 out-of-range informational entries, all historical run archives at the base revision (docs/superpowers/runs/...). No selection gaps, so this is the normal route: digest.md, the slices and captured Git at the head, with source opened only to trace named risks.
- Spec: `bd show hg-ihi` points to docs/superpowers/specs/2026-11-07-onboarding-existing-spaces-design.md (approved 2026-11-25). It has no `## Requirement baseline` section, so the acceptance standard is the design body (G1 to G6, Phases 1 to 3, Decisions 1 to 5).
- Prior invocation inv-ebcde2e9 reviewed head 2319b4b and returned not ready with three items. hg-ihi.16 (2319b4b..079d106) addresses them; each fix is checked below.
- Sweep at 079d106: 4835 passed, 0 failed, 8 skipped, leaked-rigs 0, across tiers 1 to 3 and the HG_REAL_THREADS=1 tier-5 threads_real step. This is the branch's only full-suite measurement. This review ran no build or test.
- Caller checkout observed at HEAD 079d106 with a clean status. It was not switched. No caller file was edited except this report and the ledger binding append.

## Coverage against the design

- G1 / Phase 1 adoption: `teamspace adopt` (src/plan/adopt.rs) and `seat adopt` (src/plan/seat_adopt.rs) are registered in register_core_kinds and reach production through compose.rs `register_commands_live` with real Herdr and PaneSeatMap sources. Neither emits a create, launch or close effect. The only runtime effect is an optional relabel when `--name` differs from the label. The reconciler stamps tokens on the bound but unstamped objects. Tier 3 asserts the workspace token appears and that unmanaged tabs stay unchanged.
- G2 seat match: ResolveSeat runs at plan time (`gather_live` -> `bind_live_facts`) and again at admission (`admission_seats` -> `extra.threads_seats`). The mutation rejects `threads_seat_changed` or `threads_seat_unresolved`, so it fails safe when no admission facts exist. A seat held by a live, non-tombstone clone is refused. A shell tab is adopted seatless. The tier-5 adopt_resolves_the_checked_in_seat test runs in the sweep.
- G3 memberships: graph does not import them into participation. The tier-3 adopted_seat_keeps_its_threads_seat_and_memberships_across_a_daemon_restart test asserts that the membership, the clone, the seat and the pane survive adoption and a daemon restart, and that no new threads seat appears.
- G4 keyless mode and the way back out: `CloneRecord.keyless` is honoured by `reservable` and `key_link_target` in threads/effects.rs, and by the `start_needed` and `plan_resume` gates in planner.rs. No key enters the launch env without a reservation. `seat release` and `teamspace release` retire with RetireMechanism::Released, drop the bindings and emit no runtime effect. plan_closes skips released teamspaces and released clones. Undo of an Adopt action routes to the release (compensate.rs). Resurrecting a released object is refused, and the refusal points to re-adoption. Release is refused for graph-created seats, for graph-created teamspaces and for seats with clones that graph added.
- G5 warnings: the seat adopt plan prints the containment, rename, restart and keyless warnings, plus a node-manager warning when one applies. The observed harness, model, args and cwd are recorded as seat overrides, using `observed_run` to strip launch args, resume tokens and model tokens. Level-triggered occupancy in observe/diff.rs records OccupancyStarted for the already-running agent, so a later restart can resume it.
- G6 and Phase 2: in an unmanaged pane, `/seat` proposes `seat adopt <tab>`, or `teamspace adopt <ws>` first (bootstrap/resolve.rs `proposal_words`). `seat override --cwd` exists (kinds_extra.rs). `herdr-graph adopt inspect` exists (cli/adopt.rs, daemon/adopt_inspect.rs). `setup claude|codex` installs the adopt and template-capture skills (setup_claude.rs SKILLS).
- Phase 3: the template-capture skill ships with a parse-checked example, and its quoted commands are checked against the real verbs.

## Verification of the prior invocation's items

- adopt inspect memberships: memberships_with now runs `--json thread list --seat <seat>` without `--all`, the unit test asserts that argv, and skills/adopt/SKILL.md step 2 uses `thread list --seat <seat>`. Resolved.
- First adopted seat trapped as node manager: `release_clearing` in src/plan/managers/retire.rs lets a release take the live node manager when no other seat is live. It emits a `graph.node_manager_seat` effect to null. `refuse_node_manager_retirement` accepts `seat.release` and `teamspace.release` only when the plan also has that clearing effect and every other seat is retired or being released. first_seat `qualifies` then lets the next seat take the role. Tests: release_of_the_only_live_seat_clears_the_node_manager, teamspace_release_of_the_node_managers_teamspace_clears_it, undo_of_the_first_adoption_clears_the_node_manager, release_of_the_node_manager_with_another_live_seat_is_refused. The README, the adopt skill and the plan warning describe the rule. Resolved.
- Stale undo candidate after release: `supersede_adoption` pushes the release op into the adopt action's `undone_by`. list_candidates hides an Adopt action that a release superseded but no undo undid. compensate `finalize` no longer double-pushes. Tests released_seat_adoption_is_no_longer_an_undo_candidate and released_teamspace_adoptions_are_no_longer_undo_candidates cover it. Resolved.

## Cross-task seams examined

- Re-adoption after release (hg-ihi.1, hg-ihi.3, hg-ihi.4): SeatAdopt::check, TeamspaceAdopt::check, adopt.inspect and the held-seat check all skip retired clones and teamspaces. A workspace whose stale token names the released teamspace is still placed for the new teamspace through its bound id (planner.rs `workspace` checks the bound id before the foreign token), so it is restamped. Tier 3 release_leaves_the_tab_and_agent_running re-adopts the released tab and undoes that adoption.
- Released objects and tab ownership: released clones stay in desired.clones with their seat, so `others(t)` keeps a released tab from being taken as another seat's tab, while plan_closes ignores them. No close is planned.
- Keyless succession (hg-ihi.2 with hg-ihi.4): `purge_tombstone` carries `keyless`, so a closure undo of an adopted seat stays keyless and check_seat still allows releasing it.
- Node-manager guard and first_seat (hg-ihi.3, hg-ihi.4, hg-ihi.16): the plan effects and the mutation agree. Both compute `release_clearing` against the same tree, and `set_node_manager(None)` runs only when that check holds at mutate time.
- seat.resolve live path (hg-ihi.5 with hg-ihi.3): the words are computed twice, but any mismatch fails safe at apply through the admission seat recheck.
- Teamspace retire or deactivate of an adopted teamspace closes the stamped workspace, unmanaged tabs and agents included (planner.rs plan_closes CloseWorkspace). The adopt plan warns about this. The retire plan itself carries no adoption-specific warning. This is noted under Deferred OK as a follow-up, not a defect against the design.

## Ledger triage

- Recurring minor, missing-evidence (x3: hg-ihi.2, hg-ihi.7, hg-ihi.1, and also hg-ihi.4 and the hg-ihi.5 tree mismatch): a reporting gap only. The tip sweep is the measurement and it is green. Acceptable.
- hg-ihi.4 stale-undo-candidate: a known product defect, misgraded as minor. It is now fixed by hg-ihi.16 (verified above), so nothing remains to land.
- hg-ihi.7 flaky-test-unverified: no test is skipped or ignored for it, and the tip sweep has 0 failures. Track it if it recurs.
- hg-ihi.8 weak-command-check: the misleading memberships command it overlapped is fixed and its argv asserted. The remaining `herdr-threads` spans in the skill are read-only, documented commands (`thread list --seat`, `thread participants`, `read`). Acceptable.
- hg-ihi.8 suite-not-run and hg-ihi.10 unverified-doc-claim: superseded by the tip sweep, and the claim was verified against release.rs `adoption()`.
- The hg-ihi.16 minors (node-manager warning wording not asserted, one over-long skill line) are cosmetic.
- The missing-edge-case-test lines (hg-ihi.1, hg-ihi.2, hg-ihi.4, hg-ihi.5, hg-ihi.9) and the hg-ihi.3 tier-3 gap are listed under Untested scope.
- No parked, parked-finding, Recurring blocker or BLOCKED-AUTH lines. Metrics: ledger-check ok, landed equals completed (11), 0 fix passes.
- Ledger line 102 records that the hg-ihi.16 cleanup was incomplete. The task worktree .worktrees/epic-hg-ihi-integration--task-hg-ihi.16 and /tmp/sp-8bd8a737/hg-ihi.16 were both still present when observed. This is housekeeping, not a code item.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- tests/adopt_private_herdr.rs never waits for graph's seat-channel invitation, so no test has an adopted agent accept graph's channels; the daemon restart uses SIGKILL, not `daemon stop` (ledger hg-ihi.3).
- release_leaves_the_tab_and_agent_running uses a shell tab with no threads seat, so release of an agent tab and the release of graph's channel invitations in real threads are covered only at the unit level.
- No reconcile-layer test covers a keyless pane-close cascade and its undo; only the threads layer covers it (ledger hg-ihi.2).
- plan_closes is untested on a tab that mixes released-clone panes with live graph panes (ledger hg-ihi.4).
- The seat.resolve handler path through gather_live, and the plan it creates for a teamspace-adopt-first proposal, are untested; only seat_reply_live with hand-built facts is covered (ledger hg-ihi.5).
- The workspace relabel (`teamspace adopt --name` differing from the label) never runs against a live Herdr (ledger hg-ihi.1).
- `seat adopt --harness <agent>` on a pane where Herdr detects no agent is tested only at the plan level; the reconcile start it triggers in that pane is untested.
- The template-capture example parse test does not exercise a member seat_template reference (ledger hg-ihi.9).
- The node-manager release warning text in src/plan/seat_adopt.rs is not asserted by any test (ledger hg-ihi.16).
- The sweep does not identify its 8 skipped tests.

## Deferred OK
- Recurring missing-evidence (hg-ihi.1, hg-ihi.2, hg-ihi.4, hg-ihi.7; hg-ihi.5 evidence-tree mismatch): process only; the green tip sweep is the measurement.
- integration_sweep failures under load (hg-ihi.7): the tip sweep is green and no test is ignored; track it if it recurs.
- Proposal words computed twice in seat.resolve (hg-ihi.5): a mismatch fails safe at apply through the admission seat recheck.
- seat_note and seat_error inside the threads object (hg-ihi.7): the brief left the schema open, and the docs match the code.
- Threads rig helpers duplicated in tests/adopt_private_herdr.rs (hg-ihi.3), and the out-of-scope testkit and integration_sweep edits (hg-ihi.1): test-only and small.
- stop-run-processes tooling noise (hg-ihi.6) and the over-long line at skills/adopt/SKILL.md step 3.4 (hg-ihi.16): cosmetic.
- Follow-up: in src/model/harness.rs observed_run, a bare Claude `-r`/`--resume` with no value consumes the next token (`claude -r --model opus` records args ["opus"]), and `--session-id <id>` is kept in the seat args and replayed on later launches; the seat.adopt effect shows the args before confirmation, and `seat override --clear args` repairs them.
- Follow-up: `teamspace retire` or deactivate of an adopted teamspace closes its whole workspace, unmanaged tabs and agents included; only the adopt plan warns, so add the same warning to the retire and deactivate plans for adopted teamspaces.
- Follow-up: when `seat adopt --harness` sets a harness on a tab with no detected agent, warn that graph will start an agent in the pane.
- Follow-up: an adopted seat retired by containment (tab closed) keeps its adopt action in `undo`, and that undo fails with "already retired" until the closure itself is undone; this is a clear error with no data risk.
- Follow-up: `/seat` from an unmanaged pane in a tab that is already a seat proposes `seat adopt <tab>`, which is refused ("already seat") with no further guidance; README should also note that adopt inspect's seat_for can allocate a bare threads seat.
- Housekeeping: remove the leftover hg-ihi.16 task worktree (.worktrees/epic-hg-ihi-integration--task-hg-ihi.16, including its target/) and /tmp/sp-8bd8a737/hg-ihi.16 once no process uses them (ledger line 102).
