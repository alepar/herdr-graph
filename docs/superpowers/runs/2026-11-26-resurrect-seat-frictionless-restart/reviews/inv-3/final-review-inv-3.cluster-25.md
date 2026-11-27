# Final review inv-3, cluster 25 (src-model)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified as commits; caller checkout HEAD observed at 11ac415, status `?? evidence/sweep/` only). Inputs: slices/000181.diff (src/model/clone.rs) and slices/000182.diff (src/model/tests.rs), digest.md for context. Commits touching the cluster: 98a6afd (keyed/retired_at on retired seat facts) and d939a73 (hg-lnw.26, legacy retired facts take keyed from the clone's keyless).

The epic spec (`bd show hg-lnw`) has no `## Requirement baseline` section, so the baseline-tracing rule does not apply. The cluster is judged against hg-lnw.26's acceptance and the epic's resurrect-on-undo goal.

## What the cluster changes

- `SeatFactState::Retired` gains `keyed: bool` (serde default `true` via `legacy_keyed`) and `retired_at: Option<Timestamp>` (default `None`, skipped when absent). Existing round-trip tests in src/model/tests.rs were updated to the new shape, and a new test covers a legacy standalone fact (no `keyed`) reading `keyed: true` and a current fact round-tripping through JSON and TOML.
- `CloneRecord` moves to `#[serde(remote = "Self")]` with hand-written `Serialize`/`Deserialize`. Deserialize reads into `serde_json::Value`, and when `seat_fact.state == "retired"` and `keyed` is missing it inserts `keyed = !keyless`. Then it deserializes through the derived inherent function. Serialize just delegates.

## Analysis

- Correctness of the remote-Self pattern: the derived inherent `CloneRecord::serialize`/`deserialize` take precedence over the trait methods inside the impls, so there is no recursion. `serde_json::Value` implements `Deserializer`, so the second pass is sound.
- Format fidelity through `Value`: CloneRecord is read from TOML (store tree) and JSON. Timestamps are chrono values serialized as strings, so no native TOML datetime reaches the `Value` pass. The record's other non-trivial shapes (`#[serde(flatten)]` on `SeatFact.state`, the untagged `launched_compat` on `SeatIntent::Restore`) are self-describing and behave the same over `Value`. Integer ranges are unaffected (TOML integers are i64, and `Value` holds u64/i64).
- Legacy semantics: only a missing `keyed` is filled. An explicit value is kept, which the unit test checks. A clone without `keyless` (keyed) gets `true`, and a keyless clone gets `false`. That matches the hg-lnw.26 criterion that an old-graph reversible retirement fact reads as keyless for a keyless clone. In src/undo/preview.rs:486, `seat_window_note` returns None on `Retired { keyed: true }`, so this defaulting decides whether a legacy keyless clone's undo preview names the window.
- Standalone `SeatFact` default (`true`) applies outside a CloneRecord only. In-tree producers of standalone facts are in-memory: `seat_fact_request` args are built by current code with `keyed` set, and `OperationRecord` persists no request args. So no persisted legacy standalone fact gets the context-free default.
- Doc comments on `Retired.keyed` and on the CloneRecord impl state the two rules consistently.
- I did not run cargo (a sweep is running in the integration worktree). The digest reports task 19/hg-lnw.26 green in hg-build.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- Legacy keyless record to undo ResurrectSeat plan and preview window is covered only at the parse level in this cluster; the end-to-end path is the undo/seam reviewer's.
- Legacy `reversible: false` retired fact without `keyed` has no dedicated test case.

## Deferred OK
- CloneRecord parse errors now go through `D::Error::custom` from the Value pass and lose TOML line/column detail.
- Each CloneRecord parse builds an intermediate serde_json::Value (negligible cost).
