# Final review, cluster 10 (src-model), epic hg-66u

Invocation: inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f. Captured source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head e4984a492e2d3ce8344b9be83d56a3471049fd60 (both verified with `git rev-parse --verify <sha>^{commit}`). Review source binding (`dispatch-text.mjs bind-review-source`) exited 0. The caller checkout was at head e4984a4 with a clean status before and after the review.

Cluster files: src/model/change.rs, src/model/clone.rs, src/model/launch.rs, src/model/tests.rs (slices 000040 to 000043).

## What the cluster changes

- `RequestKind::ClonePurge` (change.rs). It serializes by snake_case name ("clone_purge"). Nothing persists the discriminant, so adding it mid-enum is safe. It falls into `Category::Organizational` through the wildcard arm, which suits a plan/confirm operator command (spec §14.1). Registered in src/plan/core_kinds.rs:49 and listed in tests/integration_sweep.rs.
- `CloneRecord.threads_seat: Option<String>`, `threads_seat_history: Vec<String>` and `seat_intent: Option<SeatIntent>` (clone.rs). All three use `serde(default)` and skip-if-empty, so clone records written before the change still deserialize and the record bytes of seatless clones do not change. `String` matches the existing `ThreadsLink.seat: String` (clone.rs:86). The spec's `Option<SeatId>` cannot mean the graph's own `SeatId`, which is a graph-seat id, so String is the codebase-consistent reading.
- `SeatIntent` is internally tagged (`kind`, snake_case) with the three variants that spec §14.3 requires: `Restore{seat, epoch, threads_done, launched}` (the `launched` flag from §14.7), `Finalize{seat}` and `ReleaseKey{key}`. `threads_done`/`launched` default to false on read.
- `CloneRecord::is_tombstone()` is true exactly for the Finalize and ReleaseKey intents, which §13 says are the inert purge tombstones. Restore is correctly excluded.
- `ENV_CONTINUITY_KEY = "HERDR_THREADS_CONTINUITY_KEY"` (the neutral name agreed in spec "Key decisions") and `agent_env(clone, key_reserved)`, the single env builder for every agent start (spec "model::launch"). It always emits `HERDR_GRAPH_CLONE`, and emits the key, valued with the `cl_` id, only when `key_reserved`. That matches §5 and the unchanged-behavior rule "no key is emitted before D1".
- Tests: a TOML round trip of a seated retired record for all four intent shapes, plus `agent_env_has_graph_clone_and_the_key_only_when_reserved`.

## Verification performed

- Ran `cargo test --locked --offline --lib model::` in the ht-build toolbox. It used the caller's sources at head and a review-owned reflinked target dir (/var/home/alepar/AleCode/herdr-graph/.review-inv-cc29fc83-cluster-10/target, removed afterwards): 39 passed, 0 failed. These include clone_record_roundtrip and agent_env_has_graph_clone_and_the_key_only_when_reserved.
- TOML ordering risk (a plain value or table after an array of tables): the existing first round-trip case already has non-empty `invitations` followed by `reload_required = true`, and it passes under toml 1.1.6. The trailing `seat_intent` table therefore serializes correctly.
- Start sites: `agent_env` is the only env builder at head. src/reconcile/herdr_exec.rs:549, :560 (StartAgent) and :658 (ReplaceSession relaunch) call it, all with `key_reserved = false`. That is correct pre-D1, because no reservation field exists until hg-66u.43 (open, gated on hg-66u.1).
- Tombstone predicate consumers: clone_status (excludes tombstones from the retired-unpurged list), plan/core_kinds.rs:513 and :1865, threads/assignment.rs:130 and :261, undo/preview.rs:139.

## Seam handoff (paths that leave this cluster)

These baseline rows use this cluster's types, but their producers sit outside src/model. They are for the seam reviewer to trace. This cluster neither delivers nor blocks them.
- B1 (1:1 clone to seat): `threads_seat` is written only by `ThreadsSeatMutation` (src/threads/assignment.rs). At head, `admit_threads_seat` (assignment.rs:37) has no production caller; the resolve-seat want is hg-66u.7, still open. In production, no clone ever gets a `threads_seat` at this head.
- B7/B10 (undo restores the same seat): `write_restore_intent` (assignment.rs:56) has no production caller. Undo and resurrection do not write a Restore intent at head (hg-66u.29 and hg-66u.32 are open), and nothing sets `Restore.launched` (hg-66u.30 is open).
- B9 (continuity key): `agent_env(.., true)` is reachable only from the unit test, because the reservation field and the keyed-launch gating (hg-66u.43, hg-66u.44) wait on gate hg-66u.1 (D1). That is the gated path the global constraints call for, not a fake.
- Spec §14.11: once retired-unpurged clones gain participation wants (hg-66u.8, open), `clone_wants` must exclude `is_tombstone()` records. Purge tombstones keep `lifecycle = Retired` (assignment.rs:91), and src/threads/effects.rs has no tombstone check today. Today it is safe only because wants are active-only (effects.rs:87).

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No model test round-trips a record that has both non-empty `invitations`/`sessions` and a `seat_intent`. TOML ordering is covered only indirectly, by the existing `reload_required` case.
- No test pins the wire spelling of `SeatIntent` (`kind = "restore"` and the other variants) or of `RequestKind::ClonePurge` ("clone_purge"); a rename would pass the round-trip tests while breaking stored records.
- `agent_env(.., true)` is exercised only by its unit test; no production start site can produce `key_reserved = true` before D1 (gated, hg-66u.44).

## Deferred OK
- Threads seat ids, intent seats and `ReleaseKey.key` are bare `String`s, consistent with `ThreadsLink.seat`. A newtype would stop a key being passed where a seat is expected, but nothing at head mixes them up.
- `threads_seat` doc comment says "Written only by ThreadsSeatAssignment", while `purge_tombstone` builds a fresh record copying the field. The helper lives in the assignment module, so the rule holds in spirit; the comment could name the purge copy.
