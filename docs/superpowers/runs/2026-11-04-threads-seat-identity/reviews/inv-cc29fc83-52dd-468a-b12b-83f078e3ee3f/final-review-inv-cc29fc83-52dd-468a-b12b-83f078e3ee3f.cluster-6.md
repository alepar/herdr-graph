# Final review, cluster 6 (src-cli), epic hg-66u

Invocation: inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f. Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head e4984a492e2d3ce8344b9be83d56a3471049fd60 (both verified with `git rev-parse --verify <sha>^{commit}`). Caller checkout observed at HEAD e4984a4 with a clean status before and during the review; no worktree was needed.

## Scope

The cluster is one file, `src/cli/session.rs`, slice `slices/000032.diff`: a single added argument, `env.continuity_key.as_deref()`, passed to `report_args` from the `session-report --from-hook claude` subcommand. Introduced by commit 8b9be66 (hg-66u.38, "name a rejected launch export in status at check-in").

## Analysis

- Production path: the Claude SessionStart hook runs `herdr-graph session-report --from-hook claude`; `report()` builds `Env::from_process()`, whose new `continuity_key` field reads `HERDR_THREADS_CONTINUITY_KEY` through `var()`, which maps an empty value to `None` (src/config.rs). The value is passed positionally to `report_args(hook, graph_clone, pane, continuity_key, cwd)`; the argument order at the call site matches the new signature in src/transcripts/capture.rs:85-91, so there is no positional swap with `pane`.
- `report_args` serializes it as `"continuity_key"` in the IPC args; `ReportArgs` deserializes it with `#[serde(default)]`, so spooled reports written by an older binary (no field) still parse.
- The spool path reuses the same `args` value (`args.clone()` into the live call, `args` into `SpooledReport`), so a report spooled while the daemon is down keeps the key and the daemon-side export check (`check_export`) sees the same evidence on replay.
- The early return when neither `HERDR_GRAPH_CLONE` nor `HERDR_PANE_ID` is set is unchanged. A key-only environment does nothing, which is correct: the key is only ever exported alongside `HERDR_GRAPH_CLONE` into a Herdr pane (spec §5), and the graph's check-in report needs a pane or clone to resolve.
- The hook still never fails: errors go to stderr and the exit status stays 0, as the module doc requires (spec §8.1 of the earlier run, unchanged).
- Spec fit: §5 / §13 "Shell export failure is visible" requires the check-in to tell the daemon what the agent saw, so status can name a launch export the shell rejected. Forwarding the env value from the hook is the only CLI-side piece of that; it is delivered. The CLI does not, and per spec need not, present the key to threads itself; threads reads it from its own hook.
- Requirement baseline: the relevant row is B9 (continuity key as cooperative evidence). This cluster only carries the key into the graph's own report; key emission at launch (gated on D1 per §5/§10, `agent_env(.., key_reserved)`), the daemon-side `check_export` and the status surface live in other clusters (model/launch, reconcile, transcripts). Whether B9 is delivered end to end is for the seam reviewer to trace; nothing in this cluster blocks it.
- Seam note for the transcripts/seam reviewer, not judged here: `check_export` flags a missing key for any check-in from the pane recorded in launch_env. A later session in that pane that legitimately lacks the key (for example a Herdr auto-resume after cold restore, which spec §5 says does not carry pane shell env, if Herdr keeps the pane id) would be named as a shell export rejection. Pre-D1 the key is never in the recorded vars, so this cannot fire today.

## Untested scope

- No test drives a `Some` continuity key through `report_args`: every `report_args` call in src/transcripts/tests.rs passes `None`, and the export-alert tests (e.g. `a_later_complete_report_clears_the_export_alert`) hand-build the `continuity_key` JSON, so the CLI-to-IPC field name and the spool round-trip of the key are unasserted; a rename on either side would compile and silently raise false export alerts once D1 lands.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- `report_args` serialization of a present continuity key (and its spool round-trip) has no test; existing tests pass `None` or hand-write the IPC JSON (src/transcripts/tests.rs:1232, 1275, 1547, 1591-1597).

## Deferred OK
- Possible false export-rejection alert when a keyless later session reports from the recorded launch pane (check_export in src/transcripts/capture.rs); outside this cluster, inert until D1, for the seam/transcripts reviewer to judge.
