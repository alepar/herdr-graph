# Final review: cluster 6 (src-root), epic hg-blw

Source: base bfeb8f3104dd3783d67a46570c9f272e935012b5, head 6eb6cec786cde8ca070fbf9aed46bb0609ff63ac (both verified with `git rev-parse --verify <sha>^{commit}`). The caller checkout was at head 6eb6cec with a clean status when the review started. Review source binding recorded with dispatch-text.mjs bind-review-source (exit 0).

## Scope of this cluster

The cluster has one file, src/lib.rs, and one slice, slices/000037.diff. The change is one line: `pub mod inspect;`, added between `pub mod herdr;` and `pub mod ipc;`, which keeps the alphabetical order of the module list.

## Analysis

- The declaration is the only place the crate registers `src/inspect/`. At head there is exactly one `mod inspect` declaration in src/, so nothing is defined twice.
- At head, 56 references in src/ and tests/ go through `crate::inspect` or `herdr_graph::inspect`. The CLI views (roster, sessions, log, instructions) and the integration tests (tests/inspect_*.rs) use it, so the module has to be `pub`. Tests use the crate as an external library, so `pub(crate)` would not work.
- Inside src/inspect/mod.rs, the test-only `fixture` module is gated with `#[cfg(test)]`. Making `inspect` public therefore does not ship test fixtures in the library API.
- The design adds no feature flag for these views. The spec says the read-only views are always available, so leaving the declaration ungated is correct.

## Requirement baseline trace

- Rows B1 to B8 and the scope change S1 are all delivered by code in other clusters: src/inspect/*, src/cli/*, src/daemon/roster_live.rs and src/store/history.rs. This cluster only makes the `inspect` module reachable from the crate root, and every one of those rows depends on that. Production entry point to delivery belongs to the reviewers of those clusters and to the seam reviewer. This cluster neither blocks nor narrows any row.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- none

## Deferred OK
- none
