# Final review: cluster 11 (src-herdr), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify`). The caller checkout was at 6576a117 with a clean status throughout; nothing in it was switched or edited. Source binding for invocation inv-4aa37d70-023c-483a-b47c-3dc801a586fe succeeded before this file was written.

Inputs: slices 000062 (src/herdr/client.rs), 000063 (src/herdr/fake.rs), 000064 (src/herdr/tests.rs), plus digest.md and spec docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md (sections 5, 6, 7, 11, 13, 14 and the Requirement baseline).

## What the cluster changes

- `HerdrClient::start_agent` gains env delivery (spec section 5). With a non-empty `req.env`, after the `pane.get` label lookup it builds ` export K=V ...` with `export_line`, sends it as `KeyInput::Text` plus `Enter` through the existing `send_keys` path (`pane.send_text` then `pane.send_keys`), and only then calls `agent.start`. If validation fails, nothing goes to the pane and the result is `StartOutcome::NeedsRevision`. A transport error on the export step goes through the same `reject` mapping as `agent.start`: Timeout gives Unknown, `agent_not_ready` gives BlockedNeedsHuman, any other Rejected gives NeedsRevision.
- `export_line` accepts a name only if it matches `[A-Za-z_][A-Za-z0-9_]*`, and a value only if it is non-empty and inside `[A-Za-z0-9_.-]`. Anything else is refused, never quoted, as the spec requires. The leading space is there.
- `FakeHerdr::live_handoff` models `herdr update --handoff`. The generation and server pid change. Workspace, tab and pane ids survive, and so do agents, envs and processes. Terminal ids are reallocated, pane and workspace metadata tokens are dropped, and subscribers disconnect. This matches the spec's description of Herdr v0.9.5 handoff behaviour (Problem description, section 7).
- `FakeHerdr::renumber_tabs_and_workspaces` is a test helper that gives every tab and workspace a fresh id. It is used by observe and reconcile tests for the "id does not exist in the new incarnation" arm of section 7.
- Tests: the order and text of the export (`pane.get`, `pane.send_text`, `pane.send_keys`, `agent.start`; exact line ` export HERDR_GRAPH_CLONE=cl_x HERDR_THREADS_CONTINUITY_KEY=cl_x`); no export when env is empty; refusal of six unsafe name/value cases with only `pane.get` sent; and a live-handoff fake test.

## Requirement baseline trace for this cluster

- B6 (Herdr not modified): delivered in this cluster. The env reaches the agent only through existing Herdr RPCs (`pane.send_text`, `pane.send_keys`, `agent.start`). No Herdr change and no new wire method.
- B9 (continuity key): this cluster's part is delivered. The adapter puts whatever `StartAgent.env` carries into the pane shell before `agent.start`, and tests/herdr_client.rs `private_start_agent_env_reaches_the_agent` (outside this cluster; recorded as passing on the real private rig in task-7-report.md) shows both variables reaching the agent process. The producer side is the seam reviewer's to trace: who sets `key_reserved`, the D1 gating of `ReserveContinuityKey`, and the reconcile executor's pre-export guard (pane exists, shell in the foreground, no agent). At head `agent_env` (src/model/launch.rs) gives `HERDR_GRAPH_CLONE` always and the key only when `key_reserved` is set.
- B5 (live handoff unattended): this cluster supplies only the fake model of a handoff (`live_handoff`). Recovery behaviour is in observe/reconcile/daemon (for example tests/daemon_composed.rs `live_handoff_under_a_running_loop_reconnects_and_creates_nothing`) and belongs to other clusters and the seam reviewer.
- Spec section 13 "Shell export failure is visible" cannot be detected in the adapter: the export is typed text, and a shell that rejects it still lets `agent.start` succeed. Showing it in status is a status/seam concern, not this cluster's.

## Correctness notes

- Clone ids are `cl_` plus 26 Crockford base32 characters (src/model/ids.rs), so every value a production path passes is inside the allowed set. The refusal arm is defence in depth, not a reachable failure.
- Ordering: `send_keys` awaits each RPC response before `agent.start` is sent, so the export line and Enter are in the pane's input before Herdr types the agent command. The real-rig test confirms the agent inherits the variables.
- Validation runs after `pane.get` but before any pane input, so a bad env never types anything. The unit test asserts this.
- `live_handoff` bumps `restarts`, so `server_pid` and `server_started` change and `incarnation` differs. The test asserts the incarnation change, id preservation, terminal reallocation, token drop, agent survival and subscriber disconnect.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No unit test covers the export-step failure arms in `HerdrClient::start_agent` (src/herdr/client.rs): `pane.send_text` or `pane.send_keys` Timeout mapping to Unknown, Rejected mapping to NeedsRevision, and the case where the text is sent but the Enter fails.
- The leading-space history suppression and `export K=V` acceptance across bash, zsh and fish (spec section 5) are not exercised; the private rig test uses one shell and checks only that the variables arrive.
- `private_start_agent_env_reaches_the_agent` (real Herdr) and the `herdr::` unit tests were not re-run in this review; their pass rests on task-7-report.md.

## Deferred OK
- If the Enter fails after a successful `pane.send_text` in `HerdrClient::start_agent`, the export text is left on the pane's input line and the next typed command is appended to it; a later start re-sends a full line and recovers, so this is low-likelihood and self-healing.
- `FakeHerdr::start_agent` (src/herdr/fake.rs) neither applies the adapter's env validation nor records the exported variables into the pane's env, so fake-backed caller tests cannot catch an unsafe value or model a later manual relaunch that inherits the key; production values are `cl_` ids by construction.
- The `start_agent` doc comment (src/herdr/client.rs) adds the new paragraph with no blank `///` line, so rustdoc merges it into the old one and its "Starts the agent on the pane." summary sits mid-paragraph; cosmetic.
- `FakeHerdr::renumber_tabs_and_workspaces` is documented "Test-only" but is a plain `pub` method on a non-`cfg(test)` module, like the other fake controls; a naming convention, not a defect.
