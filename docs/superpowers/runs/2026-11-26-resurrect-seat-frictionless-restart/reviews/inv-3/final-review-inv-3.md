# Final seam review inv-3, epic hg-lnw

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5, both verified with `git rev-parse --verify <sha>^{commit}`. `bind-review-source` exited 0. Caller checkout observed at HEAD 11ac415 with only `?? evidence/sweep/` untracked (the background sweep's output); this review did not touch it, created no worktree and ran no cargo. Inputs: the 34 cluster reviews, clusters.json, the base..head diff, the design doc at head with its `## Requirement baseline` (B1 to B4, no human scope changes), and the in-progress sweep log `evidence/sweep/sweep-11ac415754b5edcedf3790ca4aa050e9fbeabba5.log` read as observed evidence.

## Seam analysis

- Journal meta keys shared across clusters go through one set of helpers: `rebind_refused_key`/`rebind_refused_in`, `seat_unresolved_key`/`seat_unresolved_in` and `incarnation_changed_at` live in src/reconcile/planner.rs and are imported by src/threads/effects.rs (writer of `rebind_refused`, reader in `rebindable`) and src/daemon/clone_status.rs (reader for `restart_held`). `resurrect_refused_key` lives in src/threads/effects.rs and is written and cleared only there; `resurrect_held` in clone_status keys on the Restore intent and that meta. No key spelling disagrees between clusters.
- `SeatStatus::Retired { keyed, retired_at }` (src-ports) is filled by `mapping::seat_status` (src-threads), copied by `fact_state` (src-observe) into `SeatFactState::Retired` (src-model, with the legacy `keyed` default resolved from the clone's `keyless`), and consumed by `resurrectable`/`unretirable` (src-threads) and `seat_window_note` (src-undo). The chain agrees field for field.
- `ThreadsPort::resurrect_seat`/`rebind_held_seat` (src-ports) are overridden by the real adapter (src-threads) and by `LazyThreads` in the production composition (src-daemon), so no production path falls through to the `Unsupported` default. The graph-local `Disconnected("graph-local: ...")` contract from hg-lnw.24 (src-daemon) maps through `SeatRestoreOutcome::from_error` (src-ports) to a retryable refusal, which the executor (src-threads) treats as Transient, and the tier-5 test (tests cluster) was updated to it at 80596ca.
- The keyless start gate (src-reconcile) opens on a fresh `Resolved` read of the clone's own pane; that read comes from the rebind effect (src-threads) placing the seat, with the 90 s grace as fallback and `restart_held` (src-daemon) as the degraded row. The pieces compose: the rebind is level-triggered while held, the gate never blocks keyed clones, and the row appears only after the grace with a definitive refusal or no completion.
- The restore launch (src-reconcile) waits on `threads_done`, which only `AcknowledgeResurrected`/succession acknowledgements (src-threads assignment.rs) set; hg-lnw.30 keeps a keyless restore launch from recording `launched:`, so `restart_covered` and `start_needed` still relaunch an undo-restored agent that exits.
- One composition break exists only once every cluster is merged: the src-threads cluster added `std::fs::write` calls to src/threads/service_tests.rs (a4a5da7, 7101406, 9e1dd41) while tests/instruction_writers.rs was not updated, so `std_fs_writers_are_inventoried` fails. The sweep log at head confirms it (instruction_writers: 1 passed, 1 failed, exit 101). The cluster-32 reviewer found it; it is carried forward below.

## Baseline trace

- B1 (undo of a closed keyless tab brings back its seat): production path is `herdr-graph undo` plan/apply restoring the clone with the cascade-written Restore intent (src/threads/assignment.rs:77), observe re-reading the seat as `Retired{reversible: true, keyed: false}`, `resurrectable` emitting `threads.resurrect_seat`, `ThreadsExecutor::resurrect_seat` calling `LazyThreads` then the v4 adapter's `ResurrectSeat`, and `AcknowledgeResurrected` setting `threads_done` and opening the gated restore launch. At head the sweep shows tests/adopt_private_herdr.rs 8 passed (real Herdr, real threads 3.6.0, real graph CLI undo). Delivered. The undo preview wording gap below is a spec section 3 item, not a loss of the resurrect behaviour.
- B2 (frictionless Herdr restart): reconcile notes the incarnation change, observe reads `Unresolved`, `rebind_wants` emits `threads.rebind_held_seat` each pass, the adapter sends `ResurrectSeat` with no `replace`, keyed clones resume without waiting, keyless fresh starts wait only for placement or the grace, and `agent_names` retries a taken Herdr agent name (hg-lnw.23). No operator step on the normal path; the `restart_held` row is the degraded fallback only. Delivered from production entry points; tier 3 e2e at head was still running in the sweep when read, and earlier task evidence (hg-lnw.9, .13, .23) shows it green.
- B3 (v4 through the upstream level change, pin that release): submodule at v3.6.0 (7c3c090), Cargo.lock 3.6.0, `with_level(ServiceSessionLevel::V4)` in the production adapter, `daemon_floor` refusing 3.5.x at discovery. Delivered.
- B4 (upstream rebind of a restart-held seat, called before relaunch): `rebind_held_seat` is the 3.6.0 `ResurrectSeat` without `replace`, `SeatAlreadyResolved` maps to done, and the keyless gate orders the fresh start after placement. Delivered; tier 5 `rebind_places_a_restart_held_seat_on_its_pane` covers it against the real daemon in task evidence.
- No baseline row is undelivered.

## Verdict
not ready (default cargo test fails at head on the instruction_writers inventory; undo preview omits the keyless window note on the ordinary tab-close ordering)

## Must fix before landing
- tests/instruction_writers.rs `std_fs_writers_are_inventoried` fails at head because src/threads/service_tests.rs (std::fs::write at lines 1375 and 1383, added by this epic) is not inventoried; the sweep log at 11ac415 records the failure (exit 101). Add the file with its reason, or route the writes through a listed helper (cluster 32, cause in src-threads).
- src/undo/preview.rs `seat_window_note` returns None for a retired keyless clone whose committed `seat_fact` is a pre-close read (`Resolved`/`Unresolved`/`KeyConflict`) via `Some(_) => return None`; facts freeze at retirement and an observed tab close retires the clone before threads reports the seat retired, so design section 3's "undo preview names which window applies for each seat" fails on the ordinary keyless tab-close undo. Treat a non-`Retired` fact on a retired keyless clone like an unread fact (conditional note from `c.retired.at`) and add a unit test with a `Resolved` fact (cluster 31).

## Untested scope
- The head sweep had confirmed only the lib tier (1550 passed), the default integration binaries and tier 3 adopt_private_herdr (8 passed) when read; tier 3 e2e_private_herdr and tier 5 threads_real at 11ac415 were still running, so their result at head is not judged here.
- A keyless `--resume` relaunch after a restart whose agent exits at once is not restarted fresh (design section 4 row 4 "or resume fails"); no test covers it and the behaviour is pre-existing for keyed clones (cluster 29).
- `ClearOnReattach` landing before the lost-response re-run of a keyless restore `StartAgent` would record `launched:` again; untested (cluster 29).
- `ThreadsExecutor::resurrect_seat` against the real `ThreadsSeatMap` minting a bare `replace` seat is exercised only by tier 3 (cluster 30).
- A resurrect that keeps answering `NotFound` while graph's fact reads reversible retries without bound and never yields a `resurrect_held` row; no test pins it (cluster 30).
- `doctor()` assembling the `restart held` and `resurrect held` findings in a full report has no doctor-level test (cluster 23).
- Observe-level pass-through of `keyed: false` and `retired_at: Some(..)` into the committed fact is covered only indirectly by threads step-loop tests (cluster 26).
- Real-threads coverage of rebind `NotFound`, `ServiceBusy` and `InvalidRequest` answers is missing, and keyless undo after 7 d is tested end to end only with the fake (cluster 32).
- Teardown of a rig whose graph daemon was SIGSTOPped by the stub when a test panics relies on `E2e::drop` SIGCONT then SIGKILL; not exercised by a failing run (cluster 33).

## Deferred OK
- Suppress the `restart_held` row or its rebind remedy for a clone that also has a `resume_blocked` row, so doctor does not print two contradictory remedies for a missing pane (cluster 23).
- `daemon_floor` accepts unparseable versions and `3.6.0-<pre>` builds as meeting the 3.6.0 floor; docs could say "a parseable version below 3.6.0" (clusters 1, 30).
- Make `SeatRestoreOutcome::Refused` fields private or drop `Deserialize`, and route the `level_refused` error to `Unsupported` in `from_error`, so the recorded reason stays right for future callers (cluster 28).
- `is_level_refused` keys on the error message prefix; a reword would silently change the resurrect mapping (cluster 30).
- Exempt keyless clones from `record_launch` on restart relaunches too, and make the hg-lnw.30 restore-launch detection independent of the intent at the re-run (cluster 29, pre-existing outside spec 4.4).
- Doc drift: verification-matrix pin paragraph says the fake answers ResurrectSeat Unsupported, no matrix row maps B1/B2 to their tiers, INDEX.md says "before relaunch", design body text predates its post-implementation notes, interim-status.md and findings-ledger.md are stale snapshots (clusters 2, 3).
- Evidence directories keep red intermediate runs (hg-lnw.4, .8, .14, .15, .23, .28) without a superseded label, and logs carry no source revision (clusters 4, 8, 9, 12, 15, 18, 21).
- `daemon::lock::tests` (`is_held_reports_holder`, `second_acquire_fails_while_held`) and `private_process_info_is_shell` flake at base too; track as separate follow-ups (clusters 18, 20, 22).
- Dead-code warning on `Fx.herdr` in src/threads/tests.rs (clusters 5, 6, 10, 13, 14, 15, 19, 20).
- The FakeHerdr does not model a name taken by an agent it did not start and keeps stale name entries after exit (cluster 24).
- CloneRecord parse errors lose TOML line/column detail through the `serde_json::Value` pass (cluster 25).
- The SeatAdopt preview warning omits that graph re-invites its own channels after 7 days (cluster 27).
- `restore` ignores the result of `forget_completed`; bounded, self-healing (cluster 30).
- A keyed final-retired seat gets no membership-loss note in the undo preview; pre-existing (cluster 31).
