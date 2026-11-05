# Final review (bounded): hg-66u, threads seat as the clone identity

Invocation inv-c9eda770-1c6e-4c3e-8abf-ce9786392bc2 · branch super-auto/threads-seat-identity · captured base 0a84c67554aa3fc8e8caf20db4163683ab30af61 · head 992f07ab49a682b3803f256bffcc32b5e699e48d.

## Scope of this pass

The review-input selection is unavailable (sourceAvailable true, wholeRangeReady false, no omissions; two retained gaps for run.md:72 inv-4aa37d70: incomplete claimed collection; incomplete split bundle, missing cluster report or slices). This is a bounded captured-source review. The changed-path inventory has 310 paths, about 250 of them run documents under docs/superpowers/runs. I inspected the production seams that landed or interact with hg-66u.28, .30, .23 and .29 after the previous final review (inv-4aa37d70), plus the files that review flagged. bounded-coverage.json lists every inspected hunk. Everything else is unreviewed in this pass. No full-suite measurement exists for this head: the caller runs that sweep after this invocation, so this report does not treat the branch as tested.

## Requirement baseline trace

- B1 (1:1, threads authoritative): partly delivered before D1. ThreadsSeatAssignment (src/threads/assignment.rs) gives a write-once threads_seat with check_free on Assign and Succeed. RuntimeLoop::step_once calls observe_seats, then seat_observation::observe (src/observe/step.rs, src/observe/seat_observation.rs), so threads now speaks for seated clones in production through LazyMap's ThreadsSeatView (src/daemon/compose.rs production_ports). Undo no longer adopts the caller's pane (src/undo/adopt.rs deleted; restore_mutate commits runtime absent). The side-by-side matcher stays until D1 by spec design. The whole-batch inspect failure below can switch threads authority off for every clone at once.
- B2 (hard dependency): accepted. A threads outage holds state (ThreadsRead::Unavailable).
- B3 (delegate re-matching to threads): not delivered. Matcher deletion (hg-66u.14) and the rebind removals (hg-66u.15) wait on gate hg-66u.1 (D1), which is open.
- B4 (unattended restarts): delivered for the session path. plan_resume's restart trigger (src/reconcile/planner.rs) plans RelaunchOccupant for a clone whose committed seat_fact is Unresolved after the incarnation change, its bound pane exists and the pane holds no agent. The executor applies the grace and the shell guard and resumes the session. The fresh-agent path (hg-66u.21) waits on D1, so after a headless cold restart a clone with no resumable session is not relaunched.
- B5 (herdr update --handoff): traced statically only. The incarnation change forces rebind mode, SeatObservation's Resolved arm rebinds seated clones, and preserved-id equality matches tabs and workspaces (src/observe/matcher.rs). No composed or real-rig test demonstrates it (see Untested scope). hg-66u.17 is open.
- B6 (no Herdr change): held. Env reaches the agent through a shell export before agent.start.
- B7 (undo reopens tab, resumes session, unretires seat): delivered only as the pre-D2 interim. Both undo (src/undo/compensate.rs) and resurrection (do_resurrect_seat, src/plan/core_kinds.rs) write a Restore intent. ThreadsSource has succession, acknowledge and clear-on-reattach wants (src/threads/effects.rs), and plan_resume has a restore launch arm. Un-retire of the same seat (hg-66u.31/.32) waits on gate hg-66u.2 (D2), which is open. Interim undo yields a new seat and a one-time re-acceptance.
- B8 (agent exit keeps seat and memberships): delivered. clone_wants keys on membership_seat and releases only when the seat changes or the want goes away.
- B9 (continuity key): not delivered. Every start site passes agent_env(&clone, false) (src/reconcile/herdr_exec.rs), and there is no reservation port, field or reserve want. hg-66u.43/.44/.33 wait on D1.
- B10 (hands-off undo with membership restore): not delivered. It waits on D2, like the un-retire part of B7.
- B11 (/seat Option D): delivered by hg-66u.5 per the earlier review. Not re-inspected here.

## Cross-task seam analysis

- LazyMap::map() (src/daemon/compose.rs) takes `_guard = self.endpoint.guard()` and drops it on return. seat_for, resolve_seat and inspect then call the ThreadsSeatMap without the threads-calls lock. At base, seat_for held the guard for the whole call. The previous final review flagged this and it is unchanged at head.
- ThreadsSeatMap::inspect (src/threads/mapping.rs) returns Err on the first seat whose SeatInspect fails. api_error maps every non-transport code (for example NotFound) to Rejected. observe_seats then turns the whole step into ThreadsRead::Unavailable. ThreadsReconnect (src/threads/reconnect.rs) clears invitations but keeps threads_seat, and resolvable_pane never re-resolves a seated clone. So one stale seat id disables SeatObservation for every clone on every step, and the only trace is one stderr line plus seat_view_error. FakeThreadsSeatView returns Unresolved for unknown seats, so the fakes never show this.
- Restore launch: plan_resume's restore arm requires `eligible`, which includes p.resume.is_some(). start_needed skips every clone with a Restore intent. stuck_restores (src/daemon/clone_status.rs) lists only launched = true. A restored clone with no ended session on record therefore never launches, and status never names it.
- MarkLaunched is admitted through the writer before dispatch, but admission is asynchronous. A transient StartAgent failure retried before the mark commits can dispatch a second start. The agent-present guard limits the damage (ledger minor, hg-66u.30).
- The Retired arm retires through CascadeRule::Pane (ObservedPaneClose) even when Herdr still shows the pane, for example after an operator's final retire in threads. The retirement mechanism is then mislabelled and the pane stays open, against spec §14.10.
- Undo and resurrection reactivate a clone with its old threads_seat without check_free. Before D2 this is safe, because a final-retired seat is never re-resolved to another clone.

## Ledger triage notes

The ledger has 42 minor (deferred) lines, no parked lines and no BLOCKED-AUTH lines. Recurring clusters: the host lacks a C linker (process; the toolbox works around it) and missing-evidence across 8 tasks (process; pass claims are unverifiable from disk, which feeds the full-suite gap). Run-failure gaps recorded at tree 140e8f86 (hg-66u.29 dispatch) are open and unchanged at head: instruction_writers and the integration_sweep wiring test. Both are known failing tests, so they are triaged as must fix, not deferred.

## Verdict
not ready (bounded review; B3, B7-unretire, B9, B10 and the B4 fresh path undelivered behind open D1/D2 gates; LazyMap guard dropped; inspect batch failure disables SeatObservation; two test binaries red)

## Must fix before landing
- Whole-range review incomplete: this pass is bounded (selection provenance unavailable; gaps run.md:72 inv-4aa37d70 incomplete claimed collection and incomplete split bundle). Review the unreviewed paths before landing.
- B3 not delivered: clone re-matching is still the graph matcher's (src/observe/matcher.rs, planner LiveIndex). Deletion hg-66u.14/hg-66u.15 waits on gate hg-66u.1 (D1, open). No human scope change row covers it. Hand it back open.
- B4 partly undelivered: the fresh-agent restart path (hg-66u.21) waits on D1. A clone with no resumable session is not relaunched after a headless cold restart. Hand it back open.
- B7 partly undelivered: undo succeeds the seat (new seat id, one-time re-acceptance) instead of un-retiring it. Un-retire hg-66u.31/hg-66u.32 waits on gate hg-66u.2 (D2, open). Hand it back open and do not fake it.
- B9 not delivered: every start site passes agent_env(&clone, false) (src/reconcile/herdr_exec.rs start, lost-response branch and replace_session), and there is no reservation port, field or want. hg-66u.43/hg-66u.44/hg-66u.33 wait on D1. Hand it back open and emit no unreserved key.
- B10 not delivered: there are no unretire or finalize wants and no membership-preserving restore. They wait on D2 (hg-66u.31/hg-66u.32). Hand it back open.
- src/daemon/compose.rs LazyMap::map() drops the threads-calls guard before the delegated seat_for, resolve_seat and inspect calls, so a ThreadsReconnect can commit mid-call. resolve_seat feeds the write-once threads_seat. Hold the guard across each delegated call (unchanged since the previous final review).
- src/threads/mapping.rs ThreadsSeatMap::inspect fails the whole batch on one seat's Rejected answer (for example NotFound for a seat id kept across ThreadsReconnect). SeatObservation then speaks for no clone on any step. Make a per-seat refusal a per-seat outcome that does not speak for that clone, so the other clones keep threads authority.
- Restored clone with no resumable session stalls silently: plan_resume needs p.resume, start_needed skips any Restore, and stuck_restores lists only launched = true (src/reconcile/planner.rs, src/daemon/clone_status.rs). At minimum, name it in status with its remedy.
- tests/instruction_writers.rs instruction_source_writers_are_inventoried fails: src/plan/core_kinds.rs:1927 `cx.tree.delete_file` (clone purge) is not inventoried. The ledger records a fail at tree 140e8f86, and the static check at head confirms the entry is still missing.
- tests/integration_sweep.rs wiring::sweep_action_and_effect_envelopes_round_trip times out waiting for creation effects to settle (threads.resolve_seat stays Pending in the rig). The ledger records a fail at tree 140e8f86 and the test is unchanged at head.

## Untested scope
- Unreviewed in this bounded pass: the run documents under docs/superpowers/runs (including the spec as a diff), CURRENT-DESIGN.md, DESIGN-NOTES.md, GRAPH_STATE.md, docs/reviews, skills/*, Cargo.lock and the third_party/herdr-threads pin, src/herdr/*, src/transcripts/*, src/bootstrap/*, src/cli/session.rs, src/config.rs, src/model/launch.rs, src/model/change.rs, src/ports/*, src/threads/mod.rs, fake.rs, service_tests.rs, src/plan/core_kinds.rs outside the resurrection hunks, and every test file other than the inspected hunks.
- Selection provenance gaps: run.md:72 inv-4aa37d70 incomplete claimed collection; incomplete split bundle (missing cluster report or slices).
- No full-suite measurement of head 992f07ab exists; the caller runs it after this invocation.
- Composed daemon tests run with an empty FakeThreadsSeatView (Services::new). No composed test exercises SeatObservation, the restart resume trigger or the restore launch end to end.
- B5: no test asserts post-handoff availability or binding of pre-existing clones. live_handoff_under_a_running_loop_reconnects_and_creates_nothing asserts SplitPane <= 1, and its comment still says tabs and workspaces are unknown until hg-66u.28. hg-66u.17 (private-rig smoke) is open.
- The rewritten tests/e2e_private_herdr.rs adoption tests are compile-checked only, not run (ledger hg-66u.29).
- ThreadsSeatMap::inspect against a real v3.0.0 service with an unknown or stale seat id is untested. The fake never errors per seat.
- MarkLaunched asynchronous admission racing a transient StartAgent retry is untested.
- hg-66u.23 records parity scenario 'tab moved between workspaces' as n/a. The bead asked the final review to confirm that; this pass did not check it.
- The Retired arm when Herdr still shows the clone's pane (threads retired the seat with the pane open) is untested.

## Deferred OK
- Ordinary invitations are not released on seat succession (ledger hg-66u.8). Threads ends a final-retired seat's memberships itself before D2.
- tab_for_seat falls through to fallbacks when an older-incarnation bound id is owned by another seat (ledger hg-66u.11).
- Stale undo-adoption comments in planner.rs, herdr_exec.rs, matcher.rs and kinds_extra.rs; UndoDeps.herdr unused (ledger hg-66u.29).
- clone_wants lost its doc comment; resolve_seat reports Done before admission is known (ledger hg-66u.7).
- GRAPH_STATE.md does not document CloneRecord.seat_fact; rustfmt-only reflow in assignment.rs (ledger hg-66u.28).
- Retired-arm retirement labelled ObservedPaneClose when the pane may still exist. Fix it together with spec §14.10's retire-closes-pane rule.
- Undo and resurrection skip check_free when they reactivate the old threads_seat. Safe before D2; revisit with hg-66u.32.
- Recurring missing-evidence and host-linker clusters are process issues. They do not change product code but leave pass claims unverifiable.
- The unseated restart arm (threads_seat = None) is not implemented. It waits on D1 (hg-66u.33).
