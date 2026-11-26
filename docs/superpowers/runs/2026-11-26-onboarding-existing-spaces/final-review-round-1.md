# Final review: epic hg-ihi (onboarding existing Herdr/threads spaces)

Invocation inv-ebcde2e9-712c-467c-8e00-8d78d30c2383. Integration branch epic-hg-ihi-integration.
Captured source: base 9d70b6f4fcac29a1f668dfd57b6a5eb6433cf1db, head 2319b4b685af6c183521f2359f82afaf58efae8f (both verified with `git rev-parse --verify`).

## Inputs and route

- review-inputs.json: base/head match, sourceAvailable true, status complete, 51 selected paths, no omissions, no gaps; wholeRangeReady false with 58 out-of-range informational entries only (historical run archives at the base revision). This is the normal route: the digest and all 51 slices were the inputs, and source at the head was opened only to trace named risks.
- Spec: `bd show hg-ihi` points to docs/superpowers/specs/2026-11-07-onboarding-existing-spaces-design.md (approved 2026-11-25). It has no `## Requirement baseline` section, so the design body (Phases 1 to 3, G1 to G6, Decisions 1 to 5) is the acceptance standard.
- Sweep at 2319b4b: 4816 passed, 0 failed, 8 skipped, leaked-rigs 0. The sweep runs tiers 1 to 3 only; it has no tier-5 (real threads) step.
- The caller checkout was not switched; no build or test was run by this review.

## Coverage against the design

- Phase 1. `teamspace adopt` (src/plan/adopt.rs), `seat adopt` (src/plan/seat_adopt.rs), `seat release` / `teamspace release` and undo-of-adopt-as-release (src/plan/release.rs, src/undo/compensate.rs) are registered in register_core_kinds and reachable from `plan.create` through `register_commands_live` in src/daemon/compose.rs. The threads seat is resolved with ResolveSeat at plan time (gather_live) and again at admission (admission_seats into `extra.threads_seats`); the mutation rejects a changed or unresolved seat. A seat already held by a live clone is refused. Adopted clones are keyless: no reserve-key want (threads/effects.rs `reservable`), no key link (`key_link_target`), no key in the launch env (model/launch.rs `agent_env` adds the key only when a reservation is recorded), and the planner start and restore gates accept `keyless`. Observed harness, model, args and cwd are written as seat overrides (harness.rs `observed_run` strips resume and model tokens). The G5 warnings are printed. `/seat` in an unmanaged pane proposes `seat adopt <tab>` or `teamspace adopt <ws>` (bootstrap/resolve.rs `proposal_words`). Tier-3 tests in tests/adopt_private_herdr.rs cover a checked-in seat surviving a daemon restart, release leaving the layout intact, and unmanaged tabs left alone.
- Phase 2. The adopt skill ships and is installed by setup (setup_claude.rs SKILLS). `herdr-graph adopt inspect` exists (cli/adopt.rs, daemon/adopt_inspect.rs). `seat override --cwd` exists (kinds_extra.rs). The memberships field of `adopt inspect` is wrong; see the first item under Must fix.
- Phase 3. The template-capture skill ships with a parse-checked example.
- G4 way back out. Release works for adopted seats that are not the node manager. On a fresh graph the first adopted seat becomes the node manager (design section 3, Hierarchy), and from then on release, teamspace release and undo of that adoption are all refused. See the second item under Must fix.

## Cross-task seams examined

- Released objects and the reconciler (tasks hg-ihi.2, hg-ihi.4): plan_closes skips a released teamspace's workspace and drops released clones from a tab's graph panes, so a stale clone token left on a pane closes nothing. Re-adoption of a released tab works because SeatAdopt::check, adopt.inspect and the held-seat check all ignore retired clones, and the reconciler restamps the pane with the new clone's token.
- Plan-time live facts (hg-ihi.3, hg-ihi.5, hg-ihi.7): `needs_live` and `bind_live_facts` fold the resolved ids into the args, so the writer recomputes the plan without Herdr. The seat.resolve handler computes the proposal words, gathers facts, then calls seat_reply_live, which computes the words again. Any mismatch fails safe at apply, because the seat is rechecked there.
- Undo and resurrection (hg-ihi.4): resurrecting a released seat or teamspace is refused, and the user is pointed to re-adoption. `restamp` leaves released retirements alone. Undo of an adoption plans `seat.release` / `teamspace.release` and never a `runtime.close_*` effect.
- Node-manager guard (hg-k4l.8 rule, extended here): `seat.release` and `teamspace.release` are refused for the live node manager. Combined with first_seat::assignment in SeatAdopt, this traps the first adopted seat on a fresh graph. The testkit hides this, because `fx_with` opts out of the node manager. Only `fx_fresh_with` reaches it, and release_of_the_node_manager_is_refused asserts the refusal as intended behaviour.
- adopt inspect and threads 3.1.0 (hg-ihi.7, hg-ihi.8): `memberships_with` runs `herdr-threads --json thread list --seat <seat> --all`. In the pinned third_party/herdr-threads (v3.1.0), `--all` maps to DirectoryMembership::All, and store/queries.rs `directory` sets membership_match to true for every thread under that filter. The output is therefore the first page of the whole instance directory, not the seat's threads. The rows carry no per-seat membership state, so a reader cannot filter them afterwards. The flag-free default with `--seat` (joined plus invited) is the seat's memberships. The adopt skill tells the agent that the memberships come from `adopt inspect`, and also suggests `herdr-threads thread list --joined` with no `--seat`, which lists the skill runner's own threads rather than the adopted seat's. Only fake scripts test this path.
- `seat adopt --harness` on a tab where Herdr detects no agent: every pane is adopted with that harness, and its seat comes from seat_for (which may allocate a bare seat). The clone is keyless, has no occupant and no launch mark, so `start_needed` is true and graph starts an agent in the pane on the next pass. The plan prints only the generic "if the agent exits ... graph starts one" warning. The adopt skill always passes `--harness`. When Herdr has detected the agent this is harmless: a contradicting flag is refused and an agreeing one changes nothing. It matters only when Herdr's detection misses a running agent.

## Ledger triage

- Recurring missing-evidence cluster (x3: hg-ihi.2, hg-ihi.7, hg-ihi.1): a process gap only. The tip sweep is the branch's measurement and is green. Acceptable.
- hg-ihi.4 stale-undo-candidate: after a plain `seat release`, the original adopt action is still listed by `undo` and fails with "already retired". This is a known product defect, which the ledger graded minor; it is moved to Must fix. The adopt action is never marked undone_by, and `is_undoable(Adopt)` lists it regardless.
- hg-ihi.7 flaky-test-unverified (integration_sweep failing 1 to 2 times under load): no test is skipped or ignored, and the tip sweep shows 0 failures. Acceptable, to be tracked if it recurs.
- hg-ihi.10 unverified-doc-claim: checked. release.rs `adoption()` matches a live Adopt action with an empty undone_by, so the CURRENT-DESIGN wording holds.
- hg-ihi.8 weak-command-check: overlaps the memberships item, because the untested `herdr-threads` spans include the misleading `thread list --joined`. The rest is acceptable.
- hg-ihi.8 suite-not-run: superseded by the tip sweep.
- Missing-edge-case-test lines (hg-ihi.2, hg-ihi.1, hg-ihi.5, hg-ihi.4, hg-ihi.9) and the tier-3 gap (hg-ihi.3): listed under Untested scope.
- No parked, parked-finding, Recurring blocker or BLOCKED-AUTH lines. The metrics show no fix passes and ledger-check ok.

## Verdict

not ready (adopt inspect reports the whole thread directory as the seat's memberships; the first adopted seat on a fresh graph can never be released or have its adoption undone; a released seat's adopt action stays listed as undoable)

## Must fix before landing

- src/cli/adopt.rs memberships_with: `thread list --seat <seat> --all` returns the unfiltered instance directory (third_party/herdr-threads src/store/queries.rs: DirectoryMembership::All gives membership_match = true), so `adopt inspect` prints every thread as the seat's memberships and the adopt skill relays them to the user. Fix: drop `--all` (the default with `--seat` is joined plus invited), or pass `--joined` and `--invited` explicitly. Assert the argv in the test. In skills/adopt/SKILL.md step 2, change `herdr-threads thread list --joined` to `herdr-threads thread list --seat <seat>`.
- src/plan/release.rs with src/plan/managers/retire.rs and first_seat::assignment in src/plan/seat_adopt.rs: on a fresh graph the first adopted seat becomes the node manager, and `seat release`, `teamspace release` and `undo <adopt>` are then all refused ("move the role first with node-manager set"). The design's G4 way back out is unavailable for the first trial tab unless the user creates a graph seat (which launches an agent) or retires the seat (which closes its tab). The adoption plan, the README and the adopt skill do not mention this. This needs a human decision. Options: allow releasing the node manager when it is the graph's only live seat, clearing node_manager_seat back to the pristine state; or keep the refusal and state it in the seat adopt plan warnings, the README "Way back out" section and the skill. Neither option weakens the open-node-manager invariant for a graph that has other seats.
- src/plan/release.rs adoption() and src/undo/candidates.rs (ledger hg-ihi.4, misgraded minor): after a plain `seat release`, the adopt action stays in `undo` as undoable and its undo fails with "already retired". Mark the adopt action undone_by (or superseded) when it is released, or filter it out of the candidates.

## Untested scope

- The tier-5 test adopt_resolves_the_checked_in_seat (tests/threads_real.rs), the one real-threads check of the seat match, was not run: the sweep has no tier-5 step, and the hg-ihi.10 report says it was not run. The 8 skipped sweep tests are not identified.
- tests/adopt_private_herdr.rs does not wait for graph's seat-channel invitation, so acceptance of graph's channels by an adopted agent is unexercised. It also restarts the daemon by SIGKILL rather than `daemon stop` (ledger hg-ihi.3).
- No reconcile-layer test covers a keyless pane-close cascade and its undo; this is covered only at the threads layer (ledger hg-ihi.2).
- plan_closes with a tab that mixes released-clone panes and live graph panes is untested (ledger hg-ihi.4).
- The seat.resolve handler path through gather_live, and the plan created for the teamspace-adopt-first proposal, are untested; only seat_reply_live with hand-built facts is covered (ledger hg-ihi.5).
- The workspace relabel path (`teamspace adopt --name` differing from the label) is not run against a live Herdr (ledger hg-ihi.1).
- adopt inspect memberships are tested only against fake shell scripts, never against the pinned herdr-threads CLI.
- `seat adopt --harness <agent>` on a pane with no detected agent is covered at the plan level only; the reconcile start it triggers is untested.
- The template-capture example parse test does not exercise a member seat_template reference (ledger hg-ihi.9).

## Deferred OK

- Recurring missing-evidence (hg-ihi.2, hg-ihi.7, hg-ihi.1; plus hg-ihi.4 and the hg-ihi.5 evidence-tree mismatch): process only; the tip sweep is the measurement.
- integration_sweep failing under load (hg-ihi.7): the tip sweep is green and no test is ignored; track it if it recurs.
- Proposal words computed twice in seat.resolve (hg-ihi.5): a mismatch fails safe at apply through the admission seat recheck.
- seat_note and seat_error placed inside the threads object (hg-ihi.7): the brief left the schema open and the docs match the code.
- Threads rig helpers duplicated in tests/adopt_private_herdr.rs (hg-ihi.3): test-only maintenance.
- Out-of-scope edits to kinds_extra testkit and integration_sweep (hg-ihi.1): small and needed.
- stop-run-processes tooling noise (hg-ihi.6): the leak count was 0.
- The CURRENT-DESIGN adoption eligibility claim (hg-ihi.10): verified against release.rs adoption().
- Follow-up: when `seat adopt --harness` overrides a tab with no detected agent, warn that graph will start an agent in the pane, or refuse unless the user confirms.
- Follow-up: in observed_run, a bare Claude `--resume` with no value consumes the next flag, and `--session-id <id>` is kept in the seat args and replayed on every later launch.
- Follow-up: `/seat` from an unmanaged pane inside a tab that is already a seat proposes `seat adopt <tab>`, which is refused ("already seat"); the proposal error gives no further guidance.
- Follow-up: README calls adopt inspect read-only, but its seat_for can allocate a bare threads seat for an agent pane that has none. Note this in the docs.
