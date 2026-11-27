# Final review inv-3, cluster 24 (src-herdr)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 11ac415754b5edcedf3790ca4aa050e9fbeabba5 (both verified with `git rev-parse --verify`). Caller checkout HEAD observed at 11ac415, status `?? evidence/sweep/` only. Inputs: slices 000178-000180 (src/herdr/client.rs, src/herdr/fake.rs, src/herdr/tests.rs), digest.md, epic hg-lnw and its design doc (docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/...-design.md), bead hg-lnw.23. No cargo was run (a background sweep owns the integration worktree); this is a reading review.

## What the cluster changes

- `agent_names(label, kind, pane)` in client.rs: first name is the sanitized pane label, else the sanitized kind, else `agent`; the retry name is that name cut to `32 - 1 - len(pane id)` bytes plus `-<pane id>`, re-sanitized (so `w1:p2` becomes `w1-p2`). It reuses herdr-threads 3.6.0 `sanitize_agent_name` and `HERDR_AGENT_NAME_MAX` (third_party/herdr-threads/src/ports.rs:2212-2236), mirroring threads' own `launch_agent_names` with a pane id instead of a seat id as the suffix.
- `HerdrClient::start_agent` now submits the first name and, only on `agent_name_taken` (`wire::is_code`, prefix `agent_name_taken:`), one retry under the pane-qualified name. Every other error still maps exactly as before (timeout to `Unknown`, `agent_not_ready` to `BlockedNeedsHuman`, other rejections to `NeedsRevision`), and a second `agent_name_taken` surfaces as `NeedsRevision` with the real Herdr message.
- FakeHerdr models Herdr's live-name uniqueness: it records the name each `start_agent` used, treats a name as live while its pane still has an agent, applies the same two-name sequence via `client::agent_names`, carries names across the restart re-id, and exposes `agent_names()` for tests.
- Tests: a mini-server test pinning the exact submitted names and the single retry, and a unit test for the naming rule (unlabelled, label sanitizing, 40-char label truncation to fit the suffix).

## Requirement baseline trace

- B2 (frictionless Herdr restart): hg-lnw.23 found that a second unlabelled `claude` pane was refused `agent_name_taken` and never started, so a fresh start after a restart failed for it. Every production agent start goes through `HerdrClient::start_agent` (callers: src/reconcile/herdr_exec.rs:681, src/reconcile/session.rs:127, src/daemon/compose.rs:1934 and its recording wrapper at 2095), so the retry is reachable from the real reconcile StartAgent and resume paths. The cluster's part of B2 is delivered. Whether every restart path in design section 4 reaches `start_agent` at the right time is the planner/executor clusters' and the seam reviewer's to trace.
- B1, B3, B4: not touched by this cluster.

## Correctness notes

- Name validity: before this change a raw label such as `Legacy Agent` was sent unsanitized and would be refused by Herdr's name rule; it is now mapped onto `[a-z][a-z0-9_-]{0,31}`. Slicing `first[..keep]` is byte-safe because sanitized names are ASCII.
- Uniqueness of the retry name rests on pane ids being unique among live panes of one server. docs/herdr-spikes.md shows pane ids are reused across a cold restart, but agent names die with the server, so that does not matter; a pane id reused within one server would belong to a closed pane whose agent is gone. Truncation of the base never touches the pane-id suffix for realistic ids (`wN:pM`).
- Deadline: each attempt gets its own `limit`, so the worst case is about twice `AGENT_START_DEADLINE`, but the second attempt happens only after a fast `agent_name_taken` refusal; a timeout on the first attempt is still `Unknown` and not retried, as documented.
- Degenerate fallback: if the re-sanitized retry were `None` the retry repeats the first name. That needs a pane id with no letter or digit and is unreachable for Herdr's id format.

## Review findings

- The fake records names only for agents it started through `start_agent`. An agent placed on a pane by other means (an adopted pre-existing agent, a threads-launched agent) has no recorded name, so the fake never refuses a graph start that collides with it. That is exactly the original hg-lnw.23 scenario (adopted unlabelled `claude` plus a graph-created `claude`); it is covered by the real-Herdr tier-3 test (tests/e2e_private_herdr.rs around line 3451, pane left unlabelled), not by the fake.
- `agent_names` entries in the fake are never removed when an agent exits; they are filtered by `p.agent.is_some()`. A test that later sets an agent on that pane directly would see the stale name as live. Harmless today.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- I did not run the tests: the background sweep owns the integration worktree. Pass/fail of the new client, unit, reconcile and tier-3 tests is not verified by this review.
- The fake does not model a name taken by an agent it did not start (adopted or threads-launched); only the tier-3 private-Herdr test exercises that collision against real Herdr.

## Deferred OK
- The fake's `agent_names` map keeps entries after an agent exits (they are filtered at read time); worth pruning on exit if a future test sets pane agents directly.
