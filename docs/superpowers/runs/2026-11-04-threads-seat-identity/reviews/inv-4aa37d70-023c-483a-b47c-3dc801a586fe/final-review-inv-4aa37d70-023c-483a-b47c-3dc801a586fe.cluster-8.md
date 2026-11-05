# Final review: cluster 8 (src-cli), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify`). The caller checkout was at the head commit with a clean status at the start and the end of the review. The review source binding was recorded in progress.md first.

Cluster inputs: `src/cli/session.rs`, slice `slices/000057.diff`. One line was added, by commit 8b9be66 ("feat: name a rejected launch export in status at check-in").

## What changed

`session-report` (the Claude SessionStart hook command) now passes `env.continuity_key.as_deref()`, read from `HERDR_THREADS_CONTINUITY_KEY` by `Env::from_process`, to `transcripts::capture::report_args`. That function puts it in the `session.report` IPC payload as `"continuity_key"`. The live daemon call and the durable spool fallback both use the same `args` value, so a spooled report keeps the key too.

## Trace against the spec

- Spec §5 and §13 ("Shell export failure is visible"): the graph exports `HERDR_GRAPH_CLONE` and, once the key reservation is recorded, `HERDR_THREADS_CONTINUITY_KEY` into the pane shell before `agent.start` (`model::launch::agent_env`). The agent inherits them, and so does its SessionStart hook child, which runs `herdr-graph session-report --from-hook claude` (`bootstrap/setup_claude.rs:172`).
- The daemon side (`capture.rs` `check_export`) compares the vars recorded in the clone's `launch_env` meta for that pane with what the report carries. It names a missing `HERDR_THREADS_CONTINUITY_KEY` only when `a.continuity_key` is `None`. Without this line, every keyed launch would be wrongly flagged as an export rejection. The line therefore makes the §13 behaviour correct for the key. The field name (`continuity_key`) is the same in `report_args` (json! literal) and in `ReportArgs` (serde field with `#[serde(default)]`), so older spool files without the field still deserialize.
- `var()` treats an empty value as unset. An empty export would then show up as a missing key, which is the right outcome.
- The early return when neither `HERDR_GRAPH_CLONE` nor `HERDR_PANE_ID` is set comes before the key is read. A report with only the key set does nothing. That matches the spec: the key is not a clone locator on the graph side, and threads consumes it at its own check-in.
- Baseline row B9 (continuity key as cooperative evidence) is delivered on the threads side, by the agent's own threads check-in presenting the key, and is gated on D1. This cluster only forwards the key for graph-side export-failure visibility. Tracing B9 from launch to threads acceptance is the seam reviewer's job. No baseline row fails because of this cluster.
- The hook error contract still holds: `report()` errors are printed and the exit status stays 0.

## Seam notes (outside this cluster)

- `check_export` also runs during spool replay (`resolve_report` is shared). This is the transcripts/capture cluster's area, not this one. The interaction is noted under Deferred OK.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- The CLI wiring itself (`HERDR_THREADS_CONTINUITY_KEY` env -> `report_args` -> `session.report` payload) has no test. Every `report_args` call in `src/transcripts/tests.rs` (lines 1275, 1547) passes `None` for the key, and the export-alert tests (1156-1235) hand-build the JSON payload. A rename on either side of the `continuity_key` field would only be caught by review. A unit test of `report_args(.., Some("cl_x"), ..)` asserting `args["continuity_key"] == "cl_x"` would close the gap.

## Deferred OK
- A spool file written before this change has no `continuity_key`. If it is replayed after the same pane's `launch_env` meta was recorded with the key var, it raises a stale export-rejected alert, and the next complete report clears it. This only happens to cross-upgrade spools, and the logic belongs to the transcripts/capture cluster.
