# Final review inv-2, cluster 9 (evidence-hg-lnw-14)

## Scope and method

Cluster holds four evidence logs under evidence/hg-lnw.14/ (final.log, focused-1.log, focused-2.log, lib.log), added by commit 891902a (resurrect outcome mapping, refusal meta, clones.resurrect_held status and doctor finding). No source code is in this cluster. The epic hg-lnw has no `## Requirement baseline` section, so the baseline paragraph does not apply. Base and head commits verified.

## What the logs show

- focused-1.log: 48 focused tests, built while third_party/herdr-threads still compiled as v3.1.0; four resurrect tests FAILED (operator_resurrect_after_refusal..., resurrect_not_found_then_final_takes_succession, resurrect_not_found_then_resolved_acknowledges, resurrect_refused_meta_clears_on_ack_restore). This is the RED step of test-first work, and the later logs show it turned green.
- focused-2.log: same 48 tests, 48 passed, 0 failed, 0 ignored.
- lib.log and final.log: full lib run, 1497 passed, 0 failed, 0 ignored. One dead-code warning for `Fx.herdr` in src/threads/tests.rs, harmless.
- Logs are from the task-14 worktree at the task's own head, not the integration head 985d1eb. They are history, not a re-run at the reviewed head. Later tasks (hg-lnw.28, hg-lnw.29) carry their own tier 5 logs.

## Judgement

Evidence is internally consistent: red then green on the same 48 tests, then full suite green. Nothing here contradicts the spec. Whether the production code behind these tests delivers the epic is for the code clusters and the seam reviewer.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- Logs predate the integration head 985d1eb: they prove task hg-lnw.14 at its own commit, not the merged tip; the later tier 5 logs from hg-lnw.28 and hg-lnw.29 cover the tip.

## Deferred OK

- The unused `Fx.herdr` field warning in src/threads/tests.rs (visible in final.log) can be cleaned up later.
