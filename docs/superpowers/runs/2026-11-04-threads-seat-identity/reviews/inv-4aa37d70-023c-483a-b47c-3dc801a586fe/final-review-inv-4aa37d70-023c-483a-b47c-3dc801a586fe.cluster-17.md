# Final review: cluster 17 (src-store)

Invocation: inv-4aa37d70-023c-483a-b47c-3dc801a586fe
Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify <sha>^{commit}`; caller checkout HEAD observed at 6576a117, status clean, not modified).
Epic: hg-66u (Threads seat as the clone identity, graph side). Spec: docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md.

## Scope of the cluster

The cluster is one file, `src/store/tests.rs`, one slice (`slices/000081.diff`). The change adds three lines to the `clone_rec` test fixture helper so it initializes the three new `CloneRecord` fields: `threads_seat: None`, `threads_seat_history: vec![]`, `seat_intent: None`. No production code under `src/store/` changed in this range.

## Analysis

- The fixture change is required for compilation: `CloneRecord` (src/model/clone.rs at head) gained `threads_seat: Option<String>`, `threads_seat_history: Vec<String>` and `seat_intent: Option<SeatIntent>`, and struct literals must name every field. The defaults chosen (no seat, empty history, no intent) are the neutral, pre-assignment state, which matches how the store tests use the fixture (paths, archive, locate, rename); none of those tests depend on seat identity.
- Persistence of the new fields through the store is generic TOML serde of `CloneRecord`. All three fields carry `#[serde(default, skip_serializing_if = ...)]`, so clone records written before this branch (no such keys) still deserialize. TOML round-trip of populated fields, including every `SeatIntent` variant with `threads_seat` and `threads_seat_history` set, is covered in `src/model/tests.rs::clone_record_roundtrip` (outside this cluster, checked only to rule out an untested-persistence risk). The all-`None` round-trip there emits TOML with the keys omitted, which exercises the legacy shape implicitly. toml is 1.1.6, so the scalar `threads_seat` after the `sessions` array-of-tables does not hit the old value-after-table serializer error; the model round-trip confirms it.
- Requirement baseline (B1 to B11, no human scope changes): this cluster delivers none of the rows directly and does not need to; it is a test fixture. The rows' production paths (seat assignment in src/threads/assignment.rs, plan/reconcile wiring, undo/restore, handoff) leave this cluster and are the seam reviewer's and the owning clusters' to trace. Nothing in this cluster weakens or contradicts any row.
- No store-level test commits a clone record with a populated `threads_seat` through `GitStore` and reads it back at a later commit. Given the store is field-agnostic serde over TOML bytes and the model round-trip covers the encoding, this is a minor gap rather than a risk.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No `src/store/tests.rs` case writes a `CloneRecord` with `threads_seat`, `threads_seat_history` and `seat_intent` populated through `GitStore` commit and reads it back (encoding is covered by `src/model/tests.rs::clone_record_roundtrip`, so the gap is store integration only).

## Deferred OK
- none
