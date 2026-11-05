# Final review: cluster 9 (src-config), epic hg-66u

Invocation: inv-4aa37d70-023c-483a-b47c-3dc801a586fe
Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify <sha>^{commit}`).
Caller checkout observed at review time: HEAD 6576a117 (equal to the reviewed head), working tree clean at session start. Not switched or modified.
Inputs: `slices/000058.diff` (src/config.rs, +3 lines), digest.md for context, spec docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md at head.

## What the cluster changes

`config::Env` gains `continuity_key: Option<String>`, populated in `Env::from_process` from `HERDR_THREADS_CONTINUITY_KEY` through the existing `var()` helper. `var()` already filters empty strings, so an exported-but-empty key reads as `None`, same as the other fields.

## Spec alignment

- Spec §5 makes every graph-performed agent start export `HERDR_THREADS_CONTINUITY_KEY=<cl_ id>` into the pane shell; Key abstractions puts the constant in `model::launch` (`ENV_CONTINUITY_KEY`, present at head). This cluster is the reader side: the agent's hook process (`herdr-graph session-report`) reads the key from its inherited environment.
- At head the only production consumer is `src/cli/session.rs::report`, which passes `env.continuity_key.as_deref()` to `transcripts::capture::report_args`, which puts it into the `session.report` IPC args as `continuity_key`. The daemon side (`capture.rs` ~l.435) uses its absence, when the recorded launch exported the variable, to flag an export-rejected pane. That path leaves this cluster and is the seam reviewer's to trace.
- The env-var name in config.rs is a string literal rather than `model::launch::ENV_CONTINUITY_KEY`. This matches the file's existing convention (`HERDR_GRAPH_CLONE` is also a literal there although `ENV_CLONE` exists), so it is consistent, not a defect. A typo in either spelling would compile silently; both spellings match at head.

## Requirement baseline trace

- B9 (continuity key as cooperative evidence): the cluster delivers the reading of the key from the hook process env. Its emission (launch env builder, Herdr adapter export, D1 reservation gating) and its use (session.report handling, threads ReserveContinuityKey) sit in other clusters. The end-to-end path from launch to threads is the seam reviewer's to trace; within this cluster nothing is missing.
- B1 to B8, B10, B11: no part of them sits in this cluster.
- Human scope changes: none.

## Correctness notes

- Struct literals of `Env` elsewhere (config.rs tests, daemon/doctor.rs tests) compile with the added field (test.log at head shows passing runs), so they use `..Default::default()` or set it.
- No validation of the key's charset happens on read. That is fine: the value is only forwarded as JSON to the daemon, and the spec puts the `[A-Za-z0-9_.-]` refusal on the emitting adapter, not the reader.

## Untested scope

- `Env::from_process` reading `HERDR_THREADS_CONTINUITY_KEY` has no direct test (no other field's process-env read is tested either); downstream tests pass `continuity_key` to `report_args` explicitly, so a misspelt variable name in config.rs would go uncaught.

## Must fix before landing

- none

## Deferred OK

- Optionally reference `model::launch::ENV_CONTINUITY_KEY` (and `ENV_CLONE`) from `Env::from_process` instead of literals, so the writer and reader cannot drift; this is a convention change for the whole file, not a defect of this cluster.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- `Env::from_process` reading `HERDR_THREADS_CONTINUITY_KEY` has no direct test; downstream tests inject `continuity_key` explicitly, so a misspelt variable name in src/config.rs would go uncaught.

## Deferred OK
- Reference `model::launch::ENV_CONTINUITY_KEY` (and `ENV_CLONE`) from `Env::from_process` instead of string literals so the writer and reader cannot drift; this is a file-wide convention change, not a defect of this cluster.
