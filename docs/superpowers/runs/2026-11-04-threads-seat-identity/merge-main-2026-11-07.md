# Merge of main into super-auto/threads-seat-identity (2026-11-07)

Merge commit: `4595898` ("Merge branch 'main' into super-auto/threads-seat-identity"), parents
`db31c68` (branch) and `0908fea` (main). A follow-up commit holds the post-merge fix described below,
plus this report.

## Conflicted files

- `docs/superpowers/specs/INDEX.md`: both rows kept (threads seat identity, seat failure and manager seats).
- `skills/graph/SKILL.md`: branch's `clone add|retire|purge` (no `rebind`) plus main's `manager set|clear`, `node-manager set` and the node-manager refusal note.
- `src/cli/plan.rs`: main's version. The unused `remote(ClientError)` helper is gone (main moved to `daemon_error`), and the comment no longer names `rebind`.
- `src/model/tests.rs`: both sides' new tests kept (`restore_launched_reads_legacy_bool`, `key_reservation_round_trips_and_defaults_absent`, `manager_fields_*`).
- `src/observe/step.rs`: imports unioned (`cascade_request` and `seat_fact_request` from the branch, `cascade_request_with`, `applied_ends` and `move_request_following` from main). The step runs main's node-manager move warning, then `build_ops`, then the branch's `append_seat_ops`. The branch's seat ops got `verify_ends: false`.
- `src/observe/tests.rs`: both fixture mutations (`TestRestore`, `TestNodeManager`) and all their registrations kept. Both appended test sections (SeatObservation and parity from the branch; reparenting, node manager and per-clone survival from main) kept whole, one after the other.
- `src/plan/tests.rs`: threads mutation registration plus main's pointer and node-manager test mutations. Both appended sections kept whole (clone purge and restore_commit from the branch; node-manager set and cycle guards from main).
- `src/reconcile/planner.rs`: both new `LiveIndex` methods kept (the branch's `bound_pane`, main's `foreign_to`).
- `src/reconcile/tests.rs`: both test mutation registrations kept (`test_seat_state`, `test_node_manager`).
- `src/threads/tests.rs`: all fixture mutations (`SeatFixture`, `SetBinding`, `SetManager`) and their registrations kept.
- `src/undo/compensate.rs`: main's `compute_restore(.., Some(&plan.effects))` signature, without the caller-pane adoption (`adopt.rs` stays deleted).
- `src/undo/preview.rs`: main's pointer restores (`pointer_effects`/`pointer_warnings`, `pointer_restores`) and `r.assignment` kept. Main's displaced-clone adoption block dropped, and the branch's `runtime_effects` doc kept.
- `tests/daemon_composed.rs`: `ORG_KINDS` is the branch list plus `node_manager_set` (23 entries, no `clone_rebind`). Both appended sections kept whole (live handoff from the branch; integration sweep from main).

## Ports from deleted APIs

Main's new code called none of the deleted APIs (old matcher clone passes, LiveIndex rebinding, rebind
proposals, `clone rebind`). The only places it touched them:
- Undo caller-pane adoption (`undo/adopt.rs`, deleted on the branch). Main threaded it through its
  changed `compute_restore` call and the preview's displaced-clone effect. Both were dropped, and main's
  new `compute_restore` argument was kept.
- `cli/plan.rs` `remote()`. Main replaced it, and the branch no longer used it.
Main's `LiveIndex` changes (`foreign_to`, `gates_recreation`, node-manager reopening, the `others` tab
filter) auto-merged onto the branch's `LiveIndex::pane` (seated clone by SeatObservation binding,
seatless clone by token or preserved pane id: S1). They compose without change.

## Post-merge fix (semantic, auto-merged code)

Main's `CascadeMutation` keeps the node manager's clones when their pane closes, so the planner reopens
them (B11). On the branch a seated clone's closed pane also reads `Retired` from threads, and
SeatObservation retires it through the same cascade. For the node manager, that kept the clone with no
Restore intent. Threads keeps reporting the old seat as Retired, and write-once forbids taking the
reopened pane's seat without a Restore, so every step would close the reopened clone again (runtime
absent, sessions ended). Fix (`src/observe/mutations.rs`): the cascade's kept-node-manager paths
(`retire_seat`, `retire_clone`) call `threads::assignment::write_restore_intent`, which is a no-op for a
seatless clone. The reopen then goes through the branch's restore path: the Retired arm holds while the
Restore is pending, then succession before D2 or un-retire after D2, and the resume-driver launch.
New test: `observe::tests::seated_node_manager_retired_seat_is_kept_and_restored`.

## Contradictions

None. Main's B11 needs the node manager to be reopened, never retired. The branch needs a closed clone to
come back on the same seat. The Restore intent above satisfies both.

## Commands and results

All builds and tests ran in the `hg-build` toolbox.
- `git submodule update --init -q`: ok.
- `cargo check --all-targets`: ok. 1 warning: `Fx.herdr` is never read in `src/threads/tests.rs`. Both parents have this field unread, so the merge did not introduce it.
- `cargo check --all-targets --all-features`: ok, with the same warning.
- `nice cargo test --lib`: 1207 passed, 1 failed. The failure is the known `threads::tests::unseated_reserved_clone_gets_its_seat_from_the_reservation_answer`.
- `nice cargo test --features test-support --lib`: 1210 passed, 1 failed (the same known test).
- The private-herdr tier was not run (the caller runs the full sweep). `e2e_summary_lands_in_archived_seat_folder` is in that tier.

## Leak check

`for p in $(pgrep -u "$USER"); do readlink /proc/$p/cwd; done | grep -c '^/tmp/hg-'` returned 0 after every test run.
