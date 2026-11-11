# Final review: cluster 11 (writer), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298 (both verified with `git rev-parse --verify <sha>^{commit}`). Caller checkout observed at HEAD b9caa7f with a clean status; no switch or restore was made. Review source binding to the ledger succeeded (exit 0).

## Scope

The cluster holds one file, `src/writer/recovery.rs`, slice `slices/000081.diff` (introduced by 83f752b, "feat(inspect): log view with HistoryWalker and LogQuery"). The change replaces the inline `Graph-Op:` / `Graph-Action:` line parser in `trailers_since` with a call to `crate::store::history::parse_trailers`, and destructures its `Trailers { op, action }` result.

## Analysis

- Spec fit: the design (§ HistoryWalker, "Relationships") requires that `writer::recovery::trailers_since` and the walker share one trailer-parsing function, extracted rather than duplicated, and that callers never parse trailers directly. At head, `parse_trailers` in `src/store/history.rs:23` is the only parser and recovery calls it; the requirement is met.
- Behavior equivalence: the extracted function is the removed loop verbatim (same `strip_prefix` on each line, same `trim().parse().ok()`, same last-occurrence-wins overwrite, same unparseable-value-becomes-None rule). Recovery still emits a row only when `op` is `Some`, carrying `action` alongside. No observable change to recovery output.
- Layering: `writer` already depends on `crate::store` (`writer/mod.rs`, `writer/commit.rs`, `writer/mutation.rs`), so the new call introduces no new dependency direction. `store::history` is `pub mod` in `src/store/mod.rs`. The `OpId` / `ActionId` imports in `recovery.rs` remain used by the signature, so no dead import.
- Test coverage: `parse_trailers_forms` in `store::history` covers both trailers, op only, no trailers and unparseable values. Recovery's consumption of `trailers_since` stays covered by existing writer tests asserting `marked_committed` (`src/writer/tests.rs:868`) and `resuperseded` (`src/writer/tests.rs:1281`).
- Requirement baseline: rows B1 to B8 and S1 are user-facing inspection behaviors; none is delivered by this cluster. The shared parser feeds B3 (`log`) through `HistoryWalker`, which lives in another cluster and is the seam reviewer's to trace. This cluster neither delivers nor breaks a baseline row.
- Minor style note, not a defect: the call uses a fully qualified path `crate::store::history::parse_trailers` inline rather than a `use` at the top, unlike the rest of the writer module. Harmless.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- none

## Deferred OK
- Inline fully qualified `crate::store::history::parse_trailers` call in `src/writer/recovery.rs:51` could be a top-level `use` to match writer module style; cosmetic only.
