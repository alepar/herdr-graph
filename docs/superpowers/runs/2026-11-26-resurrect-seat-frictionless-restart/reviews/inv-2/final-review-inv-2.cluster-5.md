# Final review inv-2, cluster 5 (evidence-hg-lnw-10)

Source: base 60e287aa, head 985d1eb6. Cluster input: evidence/hg-lnw.10/packaging-readme.log (slice 000061), a single new log file.

## Scope read

The cluster is one captured test run for task hg-lnw.10 (docs task): `cargo test --test packaging readme_has_required_sections` built herdr-graph v0.3.0 with herdr-threads v3.6.0 and reports 1 passed, exit=0. It is evidence only; it carries no production code.

## Checks made

- The named test exists in tests/packaging.rs at head and asserts twelve README H2 headings plus the strings `ht-5nb` and `git submodule update --init --recursive`.
- README.md at head contains all twelve required headings (Install, Create an instance, Claude and Codex setup, /seat, Plan, confirm, relay, Undo, Ops, Threads integration and amendment status, Test tiers, Verification matrix, Design documents, License) and both required strings, so the logged result is reproducible in kind.
- The log's build line shows herdr-threads v3.6.0, consistent with the pin the epic requires.

## Requirement baseline

B1 to B4 (undo re-adopts the seat, frictionless restart, upstream v4 session, upstream restart rebind) are production behaviour. This log neither delivers nor contradicts them; tracing them belongs to the code clusters and the seam reviewer. The log only shows the README keeps its required structure after the docs rewrite. It does not prove the docs content criteria of hg-lnw.10 (the test checks headings, not the keyless-undo, restart_held, resurrect_held or invitation-reset text); that is a doc-content matter for the README cluster.

## Verdict

ready
## Must fix before landing

- none

## Untested scope

- The packaging test checks headings only; it does not assert the hg-lnw.10 doc-content criteria (restart_held, resurrect_held, pre-3.6.0 daemon error, invitation reset wording), which are covered by the README and CURRENT-DESIGN cluster review, not by this log.

## Deferred OK

- none
