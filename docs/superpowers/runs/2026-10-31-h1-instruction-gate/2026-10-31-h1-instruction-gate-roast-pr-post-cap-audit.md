super-roast verdict: clean (0 nits) [low coverage] [review readiness incomplete]
mode: PR        iteration: post-cap audit
profile (assumed): herdr-graph v0.2.0 (`publish = false`) is a local, single-operator Herdr plugin: a Rust CLI plus a long-running daemon over a unix socket, with all state in a git repository on the operator's machine. There is no network exposure, no multi-tenant data and no money; rollback is the previous binary plus git history. Blast radius is low. The branch under review is itself a security control (the instruction gate), so findings that break its stated purpose are weighed on that purpose, not on the project's small footprint.
inputs: super-auto/h1-instruction-gate@a879957 vs @dc2ac23 (post-loop changes)
delta vs prior: 0 new confirmed (0 Blocking) · 0 carried (0 Blocking) · 0 resolved · 0 regressed (0 Blocking) · 2 punch-listed (open)
coverage: scouts 14/14 (correctness, security, premortem, simplicity-design, hot-path-perf, concurrency-async, regression, testing, test-causality-lifecycle, test-isolation-environment, test-suite-cost, hygiene-docs, api-contract, observability) · raw 0 → deduped 0 → panel 0 · spot 0 · promoted 0 · judge completion n/a (no panels) · remainder-capped: 0 · review readiness: coverage complete, provenance unavailable — basis: "direct review of the post-loop range dc2ac23b54a09a2667a7d92a03eccec11734fa84..a879957c3f4ca659efac6c3055c3de30d4a615e0 (hg-45e.49 fix); generated-input provenance unavailable because of the run's collection gaps"
independence: same-family (Claude) — seat-differentiated panel · rung: Workflow
lane-yield (found/confirmed/unique/refuted): correctness 0/0/0/0 · security 0/0/0/0 · premortem 0/0/0/0 · simplicity-design 0/0/0/0 · hot-path-perf 0/0/0/0 · concurrency-async 0/0/0/0 · regression 0/0/0/0 · testing 0/0/0/0 · test-causality-lifecycle 0/0/0/0 · test-isolation-environment 0/0/0/0 · test-suite-cost 0/0/0/0 · hygiene-docs 0/0/0/0 · api-contract 0/0/0/0 · observability 0/0/0/0

Readiness note: all 14 scouts returned and raised zero candidates, so no dedupe or judge stage ran. The engine reports coverage complete but provenance unavailable (collection gaps), so `[review readiness incomplete]` and `[low coverage]` are set; stage `coverage.lowCoverage` is false and the qualifiers reflect the provenance limit, not a dead seat. This is an empty late round: returning scouts and zero candidates do not establish source completeness, so the qualification is preserved. `[converged]` is not set: the engine reports convergenceEligible false and the verdict carries a readiness qualifier. Nothing Blocking is confirmed, so convergence is blocked only by readiness, not by findings.

Range reviewed: dc2ac23..a879957 touches src/undo/preview.rs (+16/-?), src/templates/tests.rs (+101) and run documentation; the code change is commit 16d87db ("fix: legacy template-edit undo falls back to exact recorded text", hg-45e.49).

Prior-report tracking (iteration 3 confirmed findings and the caller's punch list):
- [Should-fix] src/undo/preview.rs:760; src/undo/preview.rs:752 (`template_inverse` refuses every pre-upgrade template-edit undo lacking `before_raw`) — punch-listed (open) per the caller's list; no current packet re-surfaces it. Discrepancy for the caller: commit 16d87db in the reviewed range is, by its message and its touched files (src/undo/preview.rs, src/templates/tests.rs), the fix for exactly this finding, and the regression lane raised nothing against it. The panel did not verify that fix (zero candidates, no seats ran), so this report does not mark it resolved; the caller should reconcile its punch list against hg-45e.49 rather than read the "open" count as "nobody touched it".
- [Nit] src/templates/kinds.rs:1405 (`edit_cap_warnings` hand-projects seat changes and diverges from `mutate` on Retire/Keep withdrawals) — punch-listed (open); the reviewed range does not touch src/templates/kinds.rs and no current packet re-surfaces it.
- Prior rejected src/model/change.rs:31 (downgrade to v0.2.0 after `plan instructions` fails strict serde) — not re-surfaced; the rejection stands.
- Prior unverified nits src/bootstrap/tests.rs:1463 (exact-restore branch after a hook rewrite untested; misleading "hook: removed" line) and src/bootstrap/mod.rs:171 (unbounded summaries list in `seat.resolve`) — not re-surfaced; never panel-verified, so nothing is marked resolved.

## Confirmed findings
- none

## Not verified (beyond panel cap)
- none

## Not verified (dedupe failed or judge lost)
- none

## Beyond remainder cap (count only)
- none

## Rejected (with reason)
- none this round. The prior rejection of src/model/change.rs:31 (downgrade compatibility not required by the design doc or the repo; CHANGELOG.md:72 already declares older-binary discovery unsupported; earlier PR roast rejected the same finding 3/3) stands and was not re-litigated.

## Unverified nits (spot-checked)
- none

## Escalations (need human)
- none
