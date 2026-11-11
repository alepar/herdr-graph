# Final review: cluster 6 (lib) of super-auto/graph-state-inspection (epic hg-blw)

Captured source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head b9caa7f07c309421854fc87241b76fc40cfda298. Both verified with `git rev-parse --verify <sha>^{commit}`. Caller checkout HEAD observed at b9caa7f, working tree clean apart from untracked review state; no change to the caller checkout was made.

## Scope reviewed

- Cluster inputs: `src/lib.rs`, slice `slices/000076.diff`. The whole change is one line: `pub mod inspect;` inserted alphabetically between `pub mod herdr;` and `pub mod ipc;`.
- Read `src/lib.rs` at head in full and the module declarations of `src/inspect/mod.rs` at head to check the exposure risk below.

## Analysis

- The new line registers the `inspect` module tree in the crate. It is required: `src/cli/instructions.rs`, `src/cli/log.rs` and the other new CLI handlers reference `crate::inspect::*` at head (`pin_head`, `scope`, `stack::build`, `output::emit`, `log::LogQuery`, and so on), so without it the binary would not build.
- Visibility matches the file's convention: every other top-level module in `src/lib.rs` is `pub`, and the crate's integration tests (`tests/inspect_*.rs`) link the library, so `pub` is the consistent choice.
- Exposure risk checked: the test-only fixture `src/inspect/fixture.rs` is declared `#[cfg(test)] pub(crate) mod fixture;` in `src/inspect/mod.rs`, so making `inspect` public does not leak test fixtures into the production library surface. The other submodules (`harness_files`, `inventory`, `loaded`, `log`, `output`, `roster`, `scope`, `sessions`, `stack`) are production code used by the CLI.
- The `#[cfg(all(test, feature = "test-support"))] extern crate self as herdr_graph;` shim is unchanged and does not interact with the new module.

## Requirement baseline trace

- The design doc at head (`docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md`) has a `## Requirement baseline` with rows B1-B8 and human scope change S1 (affecting B8).
- This cluster's line is the module hook that makes every inspection view reachable from the production CLI, so it is necessary for all of B1-B8. None of the rows' behavior lives in `src/lib.rs`: roster and active seats (B1, B8 with S1), loaded files and instruction stack (B2, B7), scoped log (B3, B6), sessions (B4) and staleness display (B5) all leave this cluster through `src/cli/*` and `src/inspect/*`. Tracing those end to end belongs to the inspect, CLI and seam reviewers.
- Within this cluster, nothing blocks, gates or weakens any baseline row: the module is not feature-gated or cfg-gated, so it is compiled into the production binary.

## Findings

- none

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- none

## Deferred OK
- none
