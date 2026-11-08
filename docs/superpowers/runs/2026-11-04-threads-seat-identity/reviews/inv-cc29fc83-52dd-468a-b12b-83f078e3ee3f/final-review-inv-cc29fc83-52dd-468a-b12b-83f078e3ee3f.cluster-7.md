# Final review: cluster 7 (src-config), epic hg-66u

Invocation: inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f
Captured source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head e4984a492e2d3ce8344b9be83d56a3471049fd60 (both verified with `git rev-parse --verify <sha>^{commit}`). The caller checkout HEAD was observed at e4984a492e2d3ce8344b9be83d56a3471049fd60 with a clean status at review time.

Inputs: slices/000033.diff (src/config.rs, +3 lines), digest.md, spec docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md at head.

## What the cluster changes

`config::Env` gains one field, `continuity_key: Option<String>`, filled in `Env::from_process` from `HERDR_THREADS_CONTINUITY_KEY` through the existing `var()` helper. That helper treats an unset or empty value as `None`, the same as every other field.

## Analysis against the spec

- Spec section 5 and the `model::launch` key abstraction name `HERDR_THREADS_CONTINUITY_KEY` as the threads-neutral variable carrying the `cl_` id. The literal in config.rs matches `model::launch::ENV_CONTINUITY_KEY` at head. The literal is written out by hand, not taken from the constant, but every other field in `from_process` does the same (`HERDR_GRAPH_CLONE` is a literal there too, even though `ENV_CLONE` exists). This follows the file's local convention and is not a defect.
- Consumer at head: `src/cli/session.rs::report` passes `env.continuity_key.as_deref()` into `transcripts::capture::report_args`. That puts `continuity_key` in the `session.report` IPC payload. The daemon side (`ReportArgs.continuity_key`, capture.rs around line 435) uses it to flag a launch export that the shell did not deliver (the "rejected launch export" status). This is the only production reader of the new field. It is reachable from the real Claude hook entry point (`herdr-graph session report --from-hook claude`).
- Empty-value semantics: an exported empty key reads as `None`. On the daemon side that counts as "missing" and records an export-rejected note, not a false match. This is the conservative direction and matches how `HERDR_GRAPH_CLONE` is handled.
- Compile safety: `Env` derives `Default`. Every struct literal of `Env` at head (src/config.rs tests, src/daemon/doctor.rs:1009, tests/daemon_ipc.rs:509) uses `..Default::default()`, so the new field breaks no constructor.
- Requirement baseline: the root spec has the section (B1 to B11, no human scope changes). The only row this cluster touches is B9 (continuity key as cooperative evidence). The cluster's part is limited to reading the variable back on the hook side, and it does that correctly. Whether the key reaches the agent at every start site (HerdrExecutor start/relaunch, ReplaceSession, the resume driver) and whether threads accepts it is decided outside this cluster: the launch, herdr adapter and threads-seat paths. Tracing that path is the seam reviewer's job. This review does not claim B9 is delivered or undelivered end to end.

## Findings

- none

## Must fix before landing

- none

## Untested scope

- `Env::from_process` reading `HERDR_THREADS_CONTINUITY_KEY` into `continuity_key`, and its handoff in `cli/session.rs::report`, have no test: no test at head runs the hook binary with the variable set, and transcripts tests call `report_args` with the key passed directly. A wrong variable name here would leave the export-rejected status permanently set without failing any test.

## Deferred OK

- none

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- `Env::from_process` reading `HERDR_THREADS_CONTINUITY_KEY` and `cli/session.rs::report` forwarding it to `session.report` have no test; transcripts tests call `report_args` with the key supplied directly.

## Deferred OK
- none
