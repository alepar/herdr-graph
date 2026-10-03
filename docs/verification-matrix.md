# Verification matrix

Which contract behaviours of herdr-graph are checked against what. Written by hg-zmi.19; the README's
"Verification matrix" section summarises this file.

| | |
|---|---|
| date | 2026-10-02 |
| herdr | 0.9.1 (API protocol 22), private server per test, never the user's session |
| code under test | `ab1c681e04c0` (hg-zmi.19 base: integration branch + stack parent hg-zmi.16); the tier-3 suite is in the commit that adds this file |
| machine | macOS, aarch64 |

## Status words

- `verified-real`: a test ran the behaviour against a private real Herdr (tier 3), real agents (tier 4) or real
  herdr-threads (tier 5) and passed on the date above.
- `verified-fake`: tiers 1-2 with fakes (`FakeHerdr`, `FakeThreads`, `ManualClock`) or a real git store/journal without
  Herdr; the unit test path is cited.
- `assumed`: nothing exercised it, it was skipped, or the exercise found a product defect (marked `DEFECT Dn`, see
  "Known defects"; the repro test is `#[ignore]`d and named in the row). Never read `assumed` as working.

Commands that produced the tier-3 rows:

```
cargo test --features private-herdr --test e2e_private_herdr -- --test-threads=1 --nocapture
cargo test --features private-herdr --test e2e_private_herdr -- --ignored --test-threads=1   # the DEFECT repros, expected to fail
```

## Tier-3 flows (`tests/e2e_private_herdr.rs`, real daemon + CLI + private Herdr, harness `shell`)

| contract behaviour | spec § | status | evidence |
|---|---|---|---|
| create teamspace + seats: workspace, tab and pane appear; tokens stamped; env `HERDR_GRAPH`, `_INSTANCE`, `_SEAT`, `_CLONE` in the pane shell; dormant seat gets no tab | 4.1, 4.2 | verified-real | `e2e_create_teamspace_seats_tabs_panes_env` |
| rename a tab: seat renamed, directory moved, `name_history` kept, tab not reverted | 4.3 | verified-real | `e2e_rename_tab_renames_seat_moves_path_history` |
| close a pane: that clone retires, seat and tab stay, closure is undoable | 4.3, 6 | verified-real | `e2e_close_pane_retires_clone` |
| close a tab: seat and all its clones retire, folder archived, undoable "tab closure" | 4.3, 6 | verified-real | `e2e_close_tab_retires_seat_and_clones` |
| close a workspace: every seat of the teamspace retires, dormant ones included, teamspace retires, nothing recreated | 4.3 | verified-real | `e2e_close_workspace_retires_all_incl_dormant` |
| undo a tab closure: seat and clones active again, restored in a NEW tab | 6 | verified-real | `e2e_undo_tab_close_restores_in_new_tab` |
| undo run from a pane of the private Herdr adopts that pane as the restored clone (pane-closure and tab-closure variants) | 6 | verified | D1 (fixed by hg-zmi.51): `e2e_undo_from_pane_adopts_caller_pane`, `e2e_undo_tab_close_from_pane_adopts_caller_pane` |
| undo run from a pane bound to another clone adopts the pane; the displaced clone is retired, the pane is never closed | 6 | verified | F2 (hg-zmi.54): `e2e_undo_from_bound_pane_adopts_and_keeps_it`, composed `undo_from_pane_bound_to_other_clone_keeps_that_pane` (`tests/daemon_composed.rs`) |
| template edit adding a member opens exactly one new tab, existing members untouched | 5 | verified-real | `e2e_template_edit_adds_member_new_tab` |
| application retire closes the application's exclusive seats and their tabs, a bystander seat stays | 5 | verified-real | `e2e_application_retire_closes_exclusive_seats` |
| events withheld from the daemon (SIGSTOP burst: 60 renames, final rename, pane close) converge from a fresh snapshot | 4.3.1 | verified-real | `e2e_event_loss_reconnect_converges`; whether Herdr buffers or drops the withheld events was not distinguished, so a truly dropped event is not shown at this tier (tier 2: `src/observe/tests.rs`) |
| SIGKILL of the daemon right after three seat ops commit: restart converges, one tab/pane/token per seat, nothing duplicated | 3.6, 4.4 | verified-real | `e2e_daemon_kill_mid_op_recovers_no_duplicates` (the kill lands while effects are in flight; not a deterministic failpoint, those are tier 2) |
| Herdr private-server restart: bound objects become `unknown`, no mass retirement, no recreation, no duplicate tab/pane; an explicit `clone rebind` re-adopts each pane and the clone is present again | 4.3.2 | verified-real | `e2e_herdr_restart_rebind_no_mass_retirement_no_duplicates` |
| after a rebind the pane's Herdr token is re-stamped | 4.2 | verified | `e2e_rebind_restamps_token_after_herdr_restart` (D2 regression); tier 2: `rebind_after_restart_restamps_token` in `src/observe/tests.rs` |
| close a tab while the daemon is down: seat `unknown` on restart, not retired, tab not recreated | 4.3.2 | verified-real | `e2e_close_while_daemon_down_unknown_not_recreated` |
| retire a seat's last clone: plan shows the induced `seat.retire` and `runtime.close_tab`; afterwards tab gone, seat retired exactly once, no "observed" closure | 3.3, 4.4 | verified-real | `e2e_retire_last_clone_induced_seat_retirement_once` |
| seat deactivate: tab closed, seat dormant, clones and seat not retired, not recorded as a closure | 4.4 | verified-real | `e2e_seat_deactivate_closes_tab_no_retirement` |
| move a seat's only pane to another (non-seat) tab: clone `reload_required`, seat active + `absent` + `moved_out`, no tab recreated | 4.3 | verified-real | `e2e_move_only_pane_moved_out_no_recreate` |
| move a pane into ANOTHER seat's tab leaves that seat's name and tab label alone | 4.3 | verified | `e2e_move_pane_into_other_seat_tab_keeps_that_seats_name` (regression for D3) |
| a rename whose event the daemon never handled (daemon stopped, then killed) is recovered once from the snapshot diff | 4.3.2 | verified-real | `e2e_dropped_rename_recovered` |
| rename a tab while the daemon is down: recorded as an observed rename on restart, not reverted | 4.3.2 | verified-real | `e2e_rename_while_daemon_down_recorded` |
| summary of a closed seat: request routed to the summarizer, ack then complete, file lands in the archived seat folder (`summaries/` only) | 8.2, 3.5 | verified-real | `e2e_summary_lands_in_archived_seat_folder` (the test plays the summarizer; no real agent) |
| restored panes (after a Herdr restart, no `HERDR_GRAPH*` env) resolve via `/seat` and `session-report` through `HERDR_PANE_ID` and the binding | 4.2, 8.1, 9 | verified-real | `e2e_restored_panes_resolve_without_env` |
| `seat override --model`: recorded in the seat, clearing works, pane and tab untouched | 4.5 | verified-real | `e2e_seat_override_model_change_recorded_shell_not_replaced` |
| session replacement after an override (running agent relaunched with the new model) | 4.5 | verified-fake | `src/reconcile/tests.rs` (`ReplaceSession` cases, lines ~500-550); not reachable at tier 3 because a `shell` clone has no occupant and Herdr detects no agent; tier 4 (claude/codex configs) were skipped, see below |
| participation join/leave: seat scope and clone scope (opt-out), duplicates and scope mix-ups refused | 7.1 | verified-real | `e2e_participation_join_leave_scopes` (graph records only; no threads service runs at tier 3) |
| a stale plan applied by a clone is rejected; `ops --unresolved` lists it; `reassign` changes the requester; `cancel` stops reminders and resolves it | 3.7 | verified-real | `e2e_cancel_and_reassign_rejected_op` |
| resurrect a retired seat (`--active`): same seat id, folder out of the archive, tab and pane again, clones active | 2.2, 4.4 | verified-real | `e2e_resurrect_retired_seat` |
| `seat --json` in a bound pane resolves seat, clone, folders; a pane of no seat does not resolve | 9 | verified-real | `e2e_seat_resolution_in_bound_pane` |
| `herdr plugin link` against the private server with its own `HERDR_PLUGIN_STATE_DIR`: actions listed, `[[startup]]` launches exactly one daemon, second `--ensure` keeps the lock pid, the `status` action reports it | 12 | verified-real | `e2e_plugin_link_status_action_single_daemon` (runs `scripts/build.sh`, a release build) |

## Other contract behaviours

| contract behaviour | spec § | status | evidence |
|---|---|---|---|
| plan hash stable, ignores plan id and commit; unrelated commits do not stale a plan, a change to a relied-on object does | 3.3 | verified-fake | `src/plan/tests.rs`: `plan_hash_stable_across_runs_and_key_order`, `unrelated_commit_does_not_stale`, `same_object_change_stales`, `effects_change_without_relied_on_change_stales` |
| stale plan rejected at apply against a real daemon | 3.3 | verified-real | `e2e_cancel_and_reassign_rejected_op` |
| writer crash recovery at every failpoint (real git + journal, subprocess crash) | 3.6 | verified-fake | `tests/writer_crash.rs::crash_at_every_failpoint_recovers`, `tests/daemon_composed.rs` subprocess group (feature `test-support`) |
| crash recovery with the composed daemon against real Herdr | 3.6 | verified-real | `e2e_daemon_kill_mid_op_recovers_no_duplicates` |
| poison ops fail after panic or 3 attempts and the FIFO continues | 3.4 | verified-fake | `src/writer/tests.rs::poison_panic_marks_failed_and_fifo_continues`, `poison_after_three_attempts` |
| git lock handling: stale locks removed on start, contention retried, exhaustion halts the writer | 3.5 | verified-fake | `src/writer/tests.rs::recover_removes_stale_locks`, `lock_contention_retried`, `lock_contention_exhausted_halts_writer` |
| concurrent writers and readers see complete revisions; cancel vs commit linearizable | 3.5 | verified-fake | `tests/writer_concurrency.rs` |
| creation-nonce adoption of a create whose response was lost | 4.2 | verified-fake | `src/reconcile/tests.rs::lost_response_create_adopted_by_nonce`; the real server's nonce label is exercised by every create in tier 3 but a lost response was not induced (`assumed` for the real lost-response path) |
| predictions are single use and discarded when the container remains | 4.4 | verified-fake | `src/observe/tests.rs::prediction_single_use_and_discarded_if_container_remains`, `src/reconcile/tests.rs::predictions_listed_until_consumed` |
| induced closure explained once against real Herdr | 4.4 | verified-real | `e2e_retire_last_clone_induced_seat_retirement_once` |
| transcript coverage merge is order independent; a late older result never shrinks coverage | 8.2 | verified-fake | `src/transcripts/tests.rs`: `coverage_merge_is_order_independent`, `merge_adjacent_and_overlapping_ranges_coalesce`, `late_older_result_never_shrinks_coverage` |
| ACK records `dispatched`, never `completed`; completion records the result | 8.2 | verified-fake | `src/transcripts/tests.rs::ack_records_dispatched_not_completed`, `complete_records_result_and_merges_coverage` |
| ACK then completion through the real CLI and daemon | 8.2 | verified-real | `e2e_summary_lands_in_archived_seat_folder` |
| threads: an acceptance (Required invite) is never fabricated by graph | 7.1 | verified-fake | `src/threads/tests.rs::acceptance_never_fabricated`, `invite_required_for_occupied_clone_on_seat_and_teamspace_channels` |
| threads against a real herdr-threads daemon: channel, invite, membership, notify, release | 7.1 | verified-real | `tests/threads_real.rs::real_threads_channel_invite_membership_notify_release`, run with `HG_REAL_THREADS=1` on the date above: 1 passed (an isolated herdr-threads daemon built from `third_party/herdr-threads`); without the variable the test skips |
| fallback delivery notifies the summarizer channel with the rq id | 7.4 | verified-fake | `src/transcripts/tests.rs::fallback_delivery_notifies_summarizer_channel_with_rq_id` |
| service-ack delivery (graph hands the request to the threads service and waits for its receipt) | 7.4 | verified-real | herdr-threads ht-5nb @ 84de563d; `tests/threads_real.rs::real_threads_channel_invite_membership_notify_release` (`HG_REAL_THREADS=1`: Send → Pending → native `ack` → Acknowledged); adapter `src/threads/service_tests.rs::ack::send_request_and_receipt_state_round_trip`; delivery `src/transcripts/tests.rs::service_ack::service_ack_delivery_records_message_id_and_receipt` (default features) |
| composition root: startup order, every kind registered, reminder loop, session end flows to transcripts | 1, 3.7 | verified-fake | `tests/daemon_composed.rs` (in-process) |
| Herdr client against a private server: create tab/panes with env, snapshot, events, reconnect, process info, rejections | 4.1 | verified-real | `tests/herdr_client.rs` |
| plugin manifest, build script, README structure | 12 | verified-real | `tests/packaging.rs` (`plugin_links_into_private_herdr`, `--disabled` link), `e2e_plugin_link_status_action_single_daemon` |

## Harness configurations (hg-zmi.15, `tests/config_smoke.rs`)

| contract behaviour | spec § | status | evidence |
|---|---|---|---|
| `shell` seat: workspace and tab names, pane token, no `StartAgent`, env readable from the pane | 4.1 | verified-real | `config_shell_seat_tab_pane_env`: `CONFIG shell: VERIFIED` |
| `claude` seat: launch with profile args, session resume after a model change | 4.1, 4.5 | assumed | `config_claude_launch_and_resume`: `CONFIG claude: SKIPPED (HG_REAL_AGENTS=1 not set)`; needs `HG_REAL_AGENTS=1`, `claude` on PATH and an explicit `ANTHROPIC_API_KEY`, none used here |
| `codex` seat: launch with profile args, session resume after a model change | 4.1, 4.5 | assumed | `config_codex_launch_and_resume`: `CONFIG codex: SKIPPED (HG_REAL_AGENTS=1 not set)`; needs `HG_REAL_AGENTS=1`, `codex` on PATH and `OPENAI_API_KEY`, none used here |

## Real agents (hg-zmi.16, `tests/real_agent_smoke.rs`)

| contract behaviour | spec § | status | evidence |
|---|---|---|---|
| a real Claude foreman is prompted `/seat` by the SessionStart hook and resolves; its session end creates an rq; a real summarizer acks and completes with coverage | 8, 9, 11 tier 4 | assumed | `real_agent_seat_bootstrap_and_summarizer_flow`: `REAL-AGENT: SKIPPED (HG_REAL_AGENTS is not 1)`; with the gate set it still skips without an API key or a `claude auth status` login (hg-zmi.16 report); the verified path has never executed |

## Herdr spikes (`docs/herdr-spikes.md`, `tests/herdr_spikes.rs`, run with `--ignored`)

| contract behaviour | spec § | status | evidence |
|---|---|---|---|
| closing a pane/tab/workspace announces only the container's own event; events are hints, never state | 4.3.1 | verified-real | spike 1; relied on by every tier-3 close flow |
| closing a workspace's last tab closes the workspace | 4.4 | verified-real | spike 2 |
| after a server restart tokens are lost, labels survive, terminal ids change, workspace and pane ids happen to be reused | 4.2 | verified-real | spike 3; confirmed again by `e2e_herdr_restart_rebind_no_mass_retirement_no_duplicates` (tokens gone, bound objects `unknown`) |
| a detached daemon started from `[[startup]]` survives the hook's exit | 1 | verified-real | spike 4; confirmed by `e2e_plugin_link_status_action_single_daemon` |
| `agent_session` for an `agent.start`ed Claude without Herdr's Claude integration | 4.1 | assumed | spike 5: needs `HG_REAL_AGENTS=1` and an API key; never run |
| Herdr live handoff (`server.live_handoff`) keeps bindings | 4.2 | assumed | not exercised (spike 3 note) |

## Known defects found by this suite

These are product defects, reported to the coordinator, not fixed here (task 19 changes no source). Each has an
`#[ignore]`d test that reproduces it: `cargo test --features private-herdr --test e2e_private_herdr -- --ignored <name>`.

- **D1 (fixed by hg-zmi.51): undo adoption of the caller's pane lost a race with the reconciler.** `undo` run from a pane commits with
  `undo.adopt_pane pane=<caller>`; the daemon admits the adopted binding only after the op has committed, but the
  commit wakes the reconciler, which sees the restored clone without a live pane and creates one (a `split_pane`
  after a pane closure, a new tab after a tab closure). The new pane takes the token and the binding; the caller's
  pane stays unbound. Seen in both variants. Fix (hg-zmi.51): `undo.apply` reads the caller pane from Herdr's
  snapshot and the undo commit itself writes the adopted clone as `present` with the full binding (the post-commit
  `admit_adopted_binding` watcher is gone); `src/reconcile/planner.rs` stamps the token of a bound live pane
  outside the seat's own tab instead of creating a tab. Code: `src/undo/adopt.rs`, `src/undo/commands.rs`,
  `src/undo/compensate.rs`.
- **D2: no token re-stamp after `clone rebind`.** The reconciler emits `stamp_token` with the clone's creation rev, so
  the effect id (`EffectRecord::identity(op, object, kind, rev)`) equals the original, already `done` effect and nothing
  runs. The binding names the pane but Herdr's pane metadata stays empty, so `LiveIndex::pane` works only through
  the same-incarnation binding, and the token never returns. Spec 4.2 says graph re-stamps tokens on rebind. The same
  collision affects D1's token. Fixed by hg-zmi.50: `clone rebind` attributes the seat to the rebind op, and
  `LiveIndex::pane` (with `tab_for_seat` and `workspace`) honors a `present` rebound pane of an older incarnation.
- **D3: a moved-out seat renames the destination tab.** (fixed by hg-zmi.49) After `pane.move` of seat A's only pane into seat B's tab the
  reconciler's `tab_for_seat(A)` finds that tab through A's token-bearing pane and plans `rename_tab`
  to A's name; the rename is then observed as a user rename of seat B, which becomes "A" (directory
  `A-<suffix>`), so two seats share a name. Moving into a tab that belongs to no seat relabels that tab with A's name
  (observed in a scratch run; `e2e_move_only_pane_moved_out_no_recreate` does not assert the label).

## Notes on what tier 3 could not induce

- A Herdr event that is really dropped (a stopped subscriber is the only way this tier can withhold events; whether Herdr then buffers or drops them was not distinguished). `e2e_event_loss_reconnect_converges`
  and `e2e_dropped_rename_recovered` therefore withhold events from the daemon (SIGSTOP, then SIGKILL for the second)
  and check convergence from snapshots.
- A deterministic crash between two named steps (failpoints need the `test-support` build; those cases are tier 2).
- Session replacement of a running agent (needs a real agent; see the real-agent and harness rows).
- A running herdr-threads service (every tier-3 rig leaves `ensure_thread` effects pending with "herdr-threads is not
  configured"; the rig's `settled()` ignores thread effects on purpose).
