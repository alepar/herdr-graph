# Post-gate audit: threads-seat-identity, f2928b1..3f8e808 (2026-11-07)

Scope: `git diff f2928b1..HEAD -- src tests Cargo.toml` with main's own changes (merge 4595898, second
parent 0908fea) left out. The merge is reviewed only where it touches this branch's code. Covered:
the gated beads hg-66u.43, .44, .40, .31, .32, .33, .15, .68, .21, .24 and .14 (with S1), the hg-66u.18
smoke, a2bf5a7, and the fix beads hg-66u.95 to .98. Read-only. One named lib test was run in `hg-build`
(`pane_closed_during_downtime_retires_after_the_window_undoably_without_resume`, which passes). It uses
only fakes, so it started no rig. One `herdr server` was running with a cwd under `/tmp/hg-*` after the
run (`/tmp/hg-01m6w6fx5geh9vnfm9t2c294s7`, 36 s old). It belongs to the other agent's full suite, not
to this audit.

Verdict: **FINDINGS 0/2/2**

## Findings

### F1 · Important · Re-asking for the key's binding replays the first answer, so an unseated reserved clone never learns its seat

- `src/threads/effects.rs:1924-1927` (`resolve_reserved`, op key `reserve:{clone}:none`)
- `src/threads/tests.rs:4486-4527` (`unseated_reserved_clone_gets_its_seat_from_the_reservation_answer`)
- `src/threads/fake.rs:488` (the fake does not model op-key replay)

Spec §2 ("Unseated clone after D1") and the hg-66u.33 brief ("same op-id discipline as ResolveSeat";
the pane `ResolveSeat` in `src/threads/mapping.rs` mints `graph-resolve-<uuid>` for each call) expect
the resolve want to keep asking `ReserveContinuityKey{key}` until the answer carries `seat`. The
executor sends the same operation key on every retry instead. Threads v3.1.0 stores the first
successful result under `(actor_scope, operation_key)` and returns that stored result for every later
call with the same key and payload. It never re-runs `continuity_keys::reserve`
(`third_party/herdr-threads/src/store/service_controls.rs:88-111`, insert at `:239`). The graph's
client journal (`adapter.rs` `mutate`) replays the same result too.

Failure scenario:
1. A clone has a recorded reservation, no `threads_seat` and no verified pane, for example after a
   restart, or when its agent was started by hand in a pane the graph does not bind.
2. The first `threads.resolve_reserved` call runs before the keyed check-in. Threads answers
   `ContinuityKeyReserved{seat: None}` and records that result.
3. The agent then checks in with the key, and threads binds the key to seat S.
4. Every retry of the want replays `seat: None`. The executor returns
   `Deferred("continuity key not bound to a seat yet")` until the namespace epoch changes, which happens
   only on `ThreadsReconnect`. The clone never gets S through this path.

The S1 pane-id fallback hides this in the common cold-restart case: the matcher re-binds the pane and
the by-pane `ResolveSeat` with a fresh uuid wins. The key path, which §2 says works "with the matcher
deleted", does not work against a real daemon. The `awaiting_restart_answer` branch is unaffected,
because a replayed answer still proves threads is reachable.

The unit test asserts `asked.iter().all(|o| o == &asked[0])`: "the same operation key on every retry".
That is exactly the property that breaks against real threads, so the test asserts the wrong thing. It
passes only because `FakeThreads::reserve_continuity_key` recomputes the answer on every call.

Suggested fix:
- Give each resolve attempt a fresh operation key, either a uuid as `ThreadsSeatMap::seat_for` uses,
  or `reserve:{clone}:none:{attempt}` keyed by the effect's attempt count.
- Invert the test: retries must use distinct keys.
- Make `FakeThreads` replay the stored answer for a repeated `OpKey` (all four D1/D2 methods), so this
  class of bug shows up in the unit tier.

### F2 · Important · Undoing a G5 retirement whose tab and workspace vanished during downtime reopens nothing; the test pins it

- `src/observe/tests.rs:4265-4309` (`pane_closed_during_downtime_retires_after_the_window_undoably_without_resume`, assertion at `:4308`)
- `src/reconcile/planner.rs:735` (`if ws.is_unknown() || !wanted { continue; }`)
- `src/reconcile/planner.rs:817` (`if gates_recreation(desired, tab) || tab.is_unknown() { continue; }`)
- `src/observe/matcher.rs:195-215` (a bound tab id that is absent links nothing, so the tab is Unknown)

Failure scenario (the default layout: a seat in its own tab, one clone):
1. The clone's pane closes while Herdr is down. Its tab and workspace close with it.
2. After restart the first step is a rebind with no baseline, so it classifies no closure. The seat's
   bound tab id and the teamspace's bound workspace id are absent, so both become Unknown (§7).
3. After the G5 window threads reads the seat Retired, and SeatObservation retires the clone, as the
   bead requires.
4. The operator runs `graph undo`. The clone comes back Active with `Restore{seat, threads_done: false}`
   and an absent runtime.
5. The planner skips the Unknown workspace (`:735`) and the Unknown tab (`:817`). No `CreateWorkspace`,
   `CreateTab` or `SplitPane` is planned, so the clone never gets a pane and the restore trigger never
   finds one to launch into. The clone stays Active and paneless, its seat and teamspace stay Unknown,
   and nothing in status names the cause: the Restore is not "launched", and no refusal is recorded.

The unit test reproduces exactly this configuration. `user_close_pane` on the only pane removes the tab,
and with `last_tab_closes_workspace` the workspace too. It asserts `creates(&fx) == 0` after the undo
passes, so it pins the non-reopen as correct. Its stated intent ("no resume attempted") needs only the
`StartAgent` checks. B7 ("graph undo … reopens the tab, resumes the session, unretires the seat") is not
delivered for this case. The real-Herdr smoke (`tests/e2e_private_herdr.rs:2916`
`e2e_pane_closed_during_downtime_retires_undoably`) does not catch it: its victim is the second pane of
a tab that survives, and it checks only that an undo candidate exists. It never applies the undo.

Suggested fix: when a pane, tab or workspace cascade (G5 included) retires the last live clone of a seat
whose tab is Unknown because its id vanished, record that container as gone: clear the stale binding and
set availability Absent, so undo's reopen path can recreate the tab and workspace. Alternatively, let the
undo or restore commit clear an Unknown binding whose id is absent in the current incarnation. Drop
`creates == 0` from the test and add an undo-reopens assertion: `CreateTab` or `SplitPane` follows the
undo, and the restore launch follows the un-retire.

### F3 · Minor · A ReleaseKey tombstone can loop on a replayed binding answer

- `src/threads/effects.rs:1653-1671` (`release_key_tombstone`: `release-ask:{clone}`, then `release:{clone}`)

This is the same replay mechanism as F1. The ask's first stored answer is `seat: None`. If the key is
bound between the ask and the `ReleaseContinuityKey` (a keyed check-in still running from the purged
clone's shell), threads refuses the release with `Conflict: continuity key is bound to a seat`.
Refusals are not stored, so a retry is possible, but it replays `seat: None` from the ask and fails the
release the same way, every time. The tombstone never becomes `Finalize{seat}`, and the key and seat are
never finalized. The window is narrow, because purge needs a retired clone with its pane closed.

Suggested fix: give the ask a fresh key per attempt, as in F1. Alternatively, on a `Conflict` refusal of
the release, re-ask with a fresh key and take the `TombstoneFinalize` branch.

### F4 · Minor · A Finalize tombstone whose seat threads no longer knows never drains

- `src/threads/effects.rs:1593-1629` (`finalize_seat`; `Ok(_) => ExecOutcome::Deferred(msg)` at `:1627`)

When `FinalizeRetirement` is refused, the executor counts the refusal as an acknowledgement only if
`seat_view.inspect` returns `Retired{Final}` for the seat. A seat threads has dropped is a per-seat
refusal (NotFound), which `ThreadsSeatView` omits from the map by contract. The result is `Ok(map
without the seat)`, then `Deferred` and a retry, forever. The tombstone stays, inert but permanent, and
the want is re-planned on every pass. Scenario: an operator purges or prunes the seat threads-side, or
the record is from a graph whose threads node was re-initialised.

Suggested fix: treat a refused finalize whose seat the read omits, after a refusal threads reports as
NotFound, as done. Or treat the NotFound code itself as acknowledgement.

## Checked and found sound

- Seat succession and the restore-ack after hg-66u.98: `succession` acknowledges only on the clone's own
  verified pane. Retired-final goes to succession and Retired-reversible goes to `unretirable`; neither
  overlaps the other.
- Restore, Finalize and ReleaseKey intents: epoch checks happen before every command. `Obsolete` is
  returned on a changed intent. `TombstoneFinalize` is idempotent for the same seat. Purge writes
  `Finalize{seat}` for every seated clone and `ReleaseKey` only for a seatless reserved one (§14.2-3).
- `UnretireSeat` and `FinalizeRetirement`: stable op keys are correct here, since a replayed
  `SeatUnretired` or `RetirementFinalized` is the intended idempotent answer. A refusal records
  `last_refusal` and defers.
- Reserve-key and key-link: they do not overlap (§16.5). Key-link is withheld while a Restore is
  unacknowledged, because the seat is retired and threads would refuse it. After succession it targets
  `Restore.seat`. The restore launch is gated on the recorded binding equalling `Restore.seat` (§16.1).
  Key-link's per-seat op key is safe, because a `Conflict` is an error that is not stored.
- v3.1.0 shapes: `retirement_state` maps `Some(Reversible)` to Reversible and absent to Final
  (`mapping.rs`). A conflict maps to `KeyConflict` only when the seat is not retired.
  `ContinuityKeyPending` maps to NotYet. `Conflict:` maps to a typed `ReserveOutcome::Conflict`.
- ContinuityAlerts: the graph does not consume the pull feed. The alert is the level from
  `SeatInspection.continuity_key.conflict`, committed as `seat_fact`, as spec §6 and §14.9 require, so
  there is no ordinal dedupe to get wrong.
- S1: matcher pass 1 skips seated active clones. Pass 2 uses the preserved pane id for seatless clones
  in any incarnation (refused when another live clone's token is on the pane), and the binding from the
  same incarnation for seated clones. `LiveIndex::pane` mirrors the rule. The terminal-id and
  native-session passes, the rebind proposals and `clone rebind --pane` are gone.
- The node-manager cascade (a2bf5a7) writes a Restore only for a seated clone (`write_restore_intent`
  is a no-op without a seat), as §14.5 requires.
- Test rigs: `RootLease` holds an `owner.lock` flock (O_CLOEXEC, so not inherited by rigs) plus a
  watchdog re-exec in its own process group, with a cwd outside `/tmp/hg-*` and no marker. Its drop
  order runs after the root sweep, and `reap_orphans` is the backstop. No new test spawns a process
  outside `TestRoot` or `PrivateHerdr`. No rig was found that can outlive its test.

## Requirement baseline delivery

| Row | Status | Evidence |
|---|---|---|
| B1 · 1:1 clone ↔ threads seat | Delivered | Write-once `threads_seat` and the one-live-clone invariant (`threads/assignment.rs`). The matcher no longer locates seated clones; SeatObservation does (`observe/matcher.rs` pass 1 and 2). The succession exception is gated on the intent. |
| B2 · Hard dependency on threads | Delivered | Pinned to herdr-threads 3.1.0. Launches wait for a recorded reservation (`planner.rs` `start_needed`, restart and restore arms). An unreachable threads launches nothing (`unreachable_threads_never_retires_from_elapsed_time`, `unseated_reserved_clone_unreachable_threads_writes_no_seat`). |
| B3 · Re-matching as good or better, delegated to threads | Partial | The clone matcher is deleted with the S1 fallback, and seated clones are located only by threads. But the unseated key-resolution path is inert against a real daemon (F1), so a seatless reserved clone depends on its preserved pane id. |
| B4 · Restarts unattended | Partial | The restart arm launches fresh or resumes, keyed, after a fact read past the restart (unit tests in `reconcile/tests.rs`, `observe/tests.rs`). The real-Herdr smoke records the graph-driven keyed fresh launch as unmet (`e2e_private_herdr.rs` Configuration 4 notes). Cold restart with a client attached is a known gap upstream (v3.1.0 notes). A clone closed during downtime cannot be restored by undo when its tab vanished (F2). |
| B5 · `herdr update --handoff` unattended | Delivered | Live-handoff observation tests (`observe/tests.rs` `live_handoff_*`, seated rebinding via SeatObservation). `e2e_herdr_restart_no_mass_retirement_no_duplicates` on a private rig. |
| B6 · No Herdr changes | Delivered | The env reaches the agent through the adapter's `export … ` line plus `agent.start` (`herdr_exec.rs` `agent_env(&clone, key_reservation.is_some())`). There are no Herdr patches. |
| B7 · Undo reopens tab, resumes, unretires | Partial | `e2e_close_then_undo_keeps_the_seat_and_memberships` shows the same seat id, a reopened pane and no new seat. Not delivered when the closed pane's tab and workspace vanished during downtime (F2). |
| B8 · Agent exit keeps seat and memberships | Delivered | Participation is keyed on `threads_seat` (the `clone_wants` satisfied test). A retired clone's want keeps links and invites nothing. |
| B9 · Continuity key as cooperative evidence | Delivered (with F1) | Seatless reserve-key and seat-bearing key-link wants, the keyed env, purge `ReleaseKey`, KeyConflict mapping and the status alert. The key-only resolve path is affected by F1. |
| B10 · Undo fully hands-off, memberships restored | Delivered | `UnretireSeat` drives the reversible branch. The e2e asserts invitations are unchanged and no re-invite happens after undo. |
