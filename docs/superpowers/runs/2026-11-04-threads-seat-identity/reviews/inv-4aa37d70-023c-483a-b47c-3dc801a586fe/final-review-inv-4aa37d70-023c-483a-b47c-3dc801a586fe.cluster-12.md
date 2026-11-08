# Final review: cluster 12 (src-model), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify`). The caller checkout was at 6576a117 with a clean status when this review read it. No files changed except this artifact and the ledger binding.

Inputs: slices 000065-000068 (src/model/change.rs, src/model/clone.rs, src/model/launch.rs, src/model/tests.rs), digest.md, and the run spec `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md` at head (§2, §5, §8, §13, §14, Key abstractions, Unchanged behavior, Requirement baseline B1-B11).

## What the cluster delivers

- `CloneRecord` gains `threads_seat: Option<String>`, `threads_seat_history: Vec<String>` and `seat_intent: Option<SeatIntent>`. All three are `#[serde(default)]` with skip-if-empty serialization, so existing records parse unchanged and nothing new is written for clones that have no seat. The schema stays 1, and the model has no `deny_unknown_fields`, so an older binary still reads new records.
- `SeatIntent` is internally tagged (`kind`) and has the three variants that §14.3 requires: `Restore{seat, epoch: OpId, threads_done, launched}` (§8 plus §14.7's `launched`), `Finalize{seat}` and `ReleaseKey{key}`. The two booleans default to false, so a Restore written without them reads as "not acknowledged, not launched", which is the safe direction.
- `CloneRecord::is_tombstone()` is true only for Finalize and ReleaseKey. Restore is never a tombstone. This matches §13 "Tombstones are inert" and §14.2-3.
- `RequestKind::ClonePurge` adds the request kind for §14.1's explicit purge. It has a production producer: `ClonePurge` is registered in src/plan/core_kinds.rs:49 and is reachable through the `clone purge <clone>` verb.
- `model::launch` gains `ENV_CONTINUITY_KEY = "HERDR_THREADS_CONTINUITY_KEY"` (threads' agreed name, §Key decisions) and `agent_env(clone, key_reserved)`. It always returns `HERDR_GRAPH_CLONE` and adds the key, valued with the `cl_` id, only when `key_reserved` is true. This matches §5 and Unchanged behavior: "the key env var is emitted only for a clone whose reservation is recorded; no key is emitted before D1".
- The `ThreadsLink.occupant` doc comment now says the field is attribution only (§3).

## Tracing to production entry points

- `agent_env` is the only env builder at every agent start site at head. Those sites are the executor start (src/reconcile/herdr_exec.rs:560), the lost-response recording path (herdr_exec.rs:549) and the ReplaceSession relaunch (herdr_exec.rs:658, passed through `Relaunch.env` into src/reconcile/session.rs:127). Every caller passes `key_reserved = false`. Before D1 that is correct: no reservation exists, and the epic forbids faking D1. The D1-gated bead (hg-66u.13) has to replace the literal with the recorded reservation. That path leaves this cluster.
- B1 (1:1 mapping) depends on `threads_seat` being write-once and on the one-live-clone-per-seat invariant. The model only holds the field. Enforcement is in ThreadsSeatAssignment (src/threads/assignment.rs), which is another cluster and the seam reviewer's to trace.
- B7 and B10 (undo onto the same seat, membership restore) depend on `SeatIntent::Restore` and its transitions. The data shape is complete here. The unretire/finalize producers are gated on D2, and the restore launch path runs through the reconciler and threads clusters, so the seam reviewer traces these rows.
- B9 (continuity key) is gated on D1. This cluster supplies the constant and the conditional builder, and no production caller can emit the key before D1. That is the requirement, not a gap.
- The model has no field for the key reservation itself. §2 and §5 refer to a "recorded key reservation", and `purge_tombstone` takes it as a parameter, which the purge plan passes as `None`. The reservation is D1-gated, so nothing is missing at this stage. hg-66u.13 must decide where the reservation is stored.

## Observations outside this cluster (for the plan/threads and seam reviewers)

- src/threads/assignment.rs `purge_tombstone` writes `Finalize{seat}` for every seated clone, whether or not D2 has shipped. §8 says a clone whose seat is already final ("every seat before D2") is purged at once, with no intent. §14.2 limits the always-tombstone rule to "after D2". At head nothing in src issues `FinalizeRetirement`, so before D2 a purged seated clone stays as a tombstone with no remover. That is inert per `is_tombstone`, but it accumulates. This is the plan/threads cluster's call. It does not involve the model types.

## Verification performed

- I read the diff slices and the head file contents of src/model/clone.rs and src/model/launch.rs, and grepped head for every use of `agent_env`, `ENV_CONTINUITY_KEY`, `ClonePurge`, `is_tombstone` and `SeatIntent::`.
- I did not run cargo. The full-suite sweep is recorded as deferred to the caller in progress.md (Recovery `hg-66u:sweep:threads-seat-identity-20261105T073527`). The new tests `clone_record_roundtrip` (TOML round-trip of all four intent shapes with seat history) and `agent_env_has_graph_clone_and_the_key_only_when_reserved` are simple, and I read them as correct, but this review did not execute them.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No model test parses a pre-change CloneRecord TOML (no `threads_seat`, `threads_seat_history` or `seat_intent` keys) to pin the `#[serde(default)]` migration path. It is correct by inspection but unpinned.
- No model test parses a `Restore` intent with `threads_done`/`launched` absent to pin their false defaults. Every round-trip test writes both fields.
- This review did not run the cluster's tests. The full-suite sweep is deferred to the caller.

## Deferred OK
- Stale doc comment on `ThreadsLink` (src/model/clone.rs:81-82): it still says the link exists to "print the exact `accept-required` command", but §4 (H2 Option D) removed command printing from /seat. This is wording only.
- The spec writes `threads_seat: Option<SeatId>`, and the code uses `Option<String>`. That is the right choice, because the graph's own `SeatId` type is a different identifier (the graph seat). Only the spec text is imprecise.
- `agent_env` callers hard-code `key_reserved = false` until hg-66u.13 (D1-gated) records reservations. This is required by the epic's no-faking constraint.
- Downgrading to a pre-change binary that rewrites a clone record silently drops `threads_seat` and `seat_intent`, because there is no schema bump and no `deny_unknown_fields`. The next run backfills `threads_seat`, but a pending Restore intent would be lost. This only matters if a rollback is ever supported.
