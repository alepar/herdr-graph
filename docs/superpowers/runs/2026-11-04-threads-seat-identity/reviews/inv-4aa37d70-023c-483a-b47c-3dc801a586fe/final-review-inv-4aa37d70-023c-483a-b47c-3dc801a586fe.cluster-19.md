# Final review: cluster 19 (src-transcripts), epic hg-66u

Invocation: inv-4aa37d70-023c-483a-b47c-3dc801a586fe. Captured source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify <sha>^{commit}`; caller checkout observed at 6576a11, clean, branch super-auto/threads-seat-identity, unchanged during the review).

Inputs: slices/000089.diff (src/transcripts/capture.rs), slices/000090.diff (src/transcripts/mod.rs), slices/000091.diff (src/transcripts/tests.rs), digest.md. Spec: docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md at head (Requirement baseline, §5, §13 "Shell export failure is visible", §14.10, §14.12). Task bead: hg-66u.38 (closed; split from hg-66u.34).

## What the cluster does

- `report_args` gains a `continuity_key` argument, and `ReportArgs` deserialises it with `#[serde(default)]`, so older spooled reports without the field still parse.
- `Transcripts::check_export` runs inside `resolve_report` for every report that resolves to an active clone, before the duplicate and unchanged-session early returns. It reads the `launch_env:<clone>` journal meta written by `HerdrExecutor::record_launch_env` (src/reconcile/herdr_exec.rs, outside this cluster). It judges a report only when the recorded pane equals the reporting pane (`a.pane`, else `caller.pane_id`). It writes `export_rejected:<clone>` with the missing names, or deletes that key when nothing is missing.
- `HERDR_GRAPH_CLONE` counts as missing only when neither the report args nor the caller env carries it. `HERDR_THREADS_CONTINUITY_KEY` counts as missing when the report lacks it. Each check applies only if the launch actually exported that name.
- `status_json` gains `export_rejected`, one entry per clone with a stored record, carrying clone, pane, missing and the fixed `EXPORT_REMEDY` text.
- Tests: rejection named in status with the remedy text (the hg-66u.38 acceptance criterion), a report from another pane not judged against this launch, and a later complete report clearing the alert. The `report_args` call-site updates are mechanical.

## Production path trace

- Producer: the Claude SessionStart hook (`src/bootstrap/setup_claude.rs` `hook_command`) runs `herdr-graph session-report --from-hook claude`. `src/cli/session.rs::report` passes `env.continuity_key`, which `src/config.rs` reads from `HERDR_THREADS_CONTINUITY_KEY`, into `report_args`. That goes to IPC `session.report` or to the spool. Both the live path and `ingest_spool` go through `resolve_report`, then `check_export`. The path is reachable in production for Claude agents.
- Record side (seam): `record_launch_env` stores `{pane, vars}` after start and relaunch. At head every call site uses `agent_env(&clone, false)`, so the only name ever recorded is `HERDR_GRAPH_CLONE`.
- Consumer: `herdr-graph status` exposes `status_json` through the transcripts status provider.
- Effective reach before D1: graph-created panes already get `HERDR_GRAPH_CLONE` from pane-creation env (`src/reconcile/desired.rs:191` via `model::launch::launch_env`). So a shell that rejects the export in such a pane is not visible until the key is exported. The key path runs only under the test helper `record_launch_env`, which fakes a recorded key. Keyed launches are hg-66u.44 (open), gated on hg-66u.1 (open). Task 8 already lists this as a known minor in the digest. It matches spec §5: before D1 no clone has a reservation, and a missing key falls back to C1 evidence, which is not an error.

## Requirement baseline trace

- B9 (continuity key): this cluster covers only the check-in side. The CLI forwards the key and the daemon compares it against the recorded launch. Emitting the key at launch, and reserving it, belong to other clusters and the open gate (hg-66u.43, hg-66u.44 behind hg-66u.1). The seam reviewer and the hand-back of gated requirements should trace that path. This cluster takes no position on whether B9 is delivered.
- B4 and B5 (frictionless restart and handoff): this cluster only reports state and blocks nothing. A stale or missing alert does not gate any launch or resume. No baseline behaviour regresses here.
- B8 (relaunch in another harness keeps the seat): the alert neither touches the seat nor depends on it. See the Codex item below for how this interacts with the status signal.
- No human scope change rows exist. No other baseline row passes through this cluster.

## Analysis of correctness and edge behaviour

- The core rule is right. The pane-equality filter stops an old launch record from judging a report from a different pane. Comparing `Value == &str` on `l["pane"]` is valid serde_json. Because the check runs before the duplicate return, a repeated or `/clear` report still refreshes the alert.
- Codex coverage: `codex_hook_command` sends no session report ("Codex session ids come from Herdr, so there is no session report"). So `check_export` never runs for a Codex clone. Spec §13 makes no exception for any harness. Before D1 nothing depends on the export, so nothing is lost yet. After D1 and §14.12, a keyless Codex seat would close final on pane close with no signal in status.
- Ordering: `check_export` ignores the report's `at`, and the launch record has no time of its own. A spooled report ingested after a newer live report can raise or clear the alert out of order. So can a stale report from an earlier agent in the same pane. The next complete check-in corrects it, so the damage is a status line that is wrong for a while.
- Incarnation: the `launch_env` record stores the pane id with no incarnation. If Herdr reuses a pane id string after a cold restore, Herdr's own auto-resume (which does not carry the export, per §5) could be judged against the older launch. It would then be flagged with a remedy that does not fit. This cannot happen before D1, because pane env still supplies `HERDR_GRAPH_CLONE`.
- Stale entries: `export_rejections` walks every key in `g.clones`, retired clones and Finalize tombstones included. Neither `launch_env:` nor `export_rejected:` is removed at retire or purge. A clone retired while an alert stands (§14.10: retire closes its pane) keeps a permanent entry whose remedy says to relaunch it. A listed tombstone also conflicts with §13 "Tombstones are inert … not listed".
- Cost: each resolved report makes one extra journal meta read and one write or delete. That is negligible.

## Evidence

- Static review of the three slices, plus `git show` at head of capture.rs, launch.rs, desired.rs, herdr_exec.rs, config.rs, cli/session.rs and bootstrap/setup_claude.rs to check the named risks.
- I did not rerun tests in this invocation, because a fresh worktree would need a 53G target reflink and a build. The earlier final review of the same cluster at this head (inv-cc29fc83, cluster-17) recorded `cargo test --lib transcripts::` at 70 passed, 0 failed, including the three new tests. I did not reproduce that run here.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test sends a spooled report through `ingest_spool` into `check_export`, and none covers spooled and live reports arriving out of order.
- No test covers `status_json` for a retired or tombstoned clone that still has an `export_rejected` record.
- No test covers the end-to-end seam where `HerdrExecutor::record_launch_env` writes the record and a hook report then reads it; the cluster tests write `launch_env` by hand, so agreement between the executor's pane id and `HERDR_PANE_ID` is unverified here (seam reviewer).
- The continuity-key branch of `check_export` runs only under a faked launch record until hg-66u.44 records keyed launches behind gate hg-66u.1.

## Deferred OK
- Before D1 the detector can name only `HERDR_GRAPH_CLONE`, which pane-creation env already supplies for graph-created panes, so it does nothing until hg-66u.44 exports the key; this is already a known minor on hg-66u.38 and matches §5.
- Codex clones have no graph check-in, so a rejected export in a Codex pane is never named in status, contrary to §13; follow-up before D1 and §14.12 land: detect it from the threads check-in or a shell probe for harnesses without a session-report hook.
- `export_rejections` lists retired clones and Finalize tombstones, and the `launch_env:` and `export_rejected:` meta is never cleaned at retire or purge, so a stale alert with a misleading relaunch remedy can persist; follow-up: list only active clones and delete both keys at retire and purge.
- `check_export` ignores report time and the launch record has no incarnation, so out-of-order spooled reports or reused pane ids after a cold restore can show a wrong alert for a while; follow-up with hg-66u.44: store incarnation and time with `launch_env` and skip reports older than the launch.
