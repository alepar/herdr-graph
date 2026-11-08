# Final review: cluster 9 (src-herdr), epic hg-66u

Invocation: inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f
Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head e4984a492e2d3ce8344b9be83d56a3471049fd60 (both verified with `git rev-parse --verify <sha>^{commit}`).
Inputs: src/herdr/client.rs, src/herdr/fake.rs, src/herdr/tests.rs (slices 000037-000039), digest.md, spec `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md` at head (sections 5, 7, 11, 13, 14 and the Requirement baseline).
Caller checkout observed at HEAD e4984a4, status clean, before and during the review; nothing in it was changed except the authorised ledger binding and this file.

## Scope of the cluster

- `export_line` and the `start_agent` change in the Herdr client adapter: a non-empty `StartAgent.env` is validated (shell identifier names; values non-empty and within `[A-Za-z0-9_.-]`) and then typed into the pane as ` export K=V ...` followed by Enter through the existing `send_keys` path (`pane.send_text` + `pane.send_keys`), before `agent.start`.
- `FakeHerdr::live_handoff` (models `herdr update --handoff`: ids, agents, envs and processes survive; generation and server pid move; terminal ids reallocated; pane and workspace tokens dropped; subscribers disconnected).
- `FakeHerdr::renumber_tabs_and_workspaces` (test-only helper for ids that do not survive).
- Tests for the above, plus `env: vec![]` added to existing `StartAgent` literals.

## Conformance to the spec

- Spec section 5 and the "StartAgent / HerdrApi adapter" key abstraction are met as written: the env travels through the port; only the adapter knows how it is delivered; the line has a leading space; unsafe values are refused instead of being quoted; validation happens before anything is typed into the pane (the test checks that only `pane.get` was sent for every refused case); the order is export text, then Enter, then `agent.start`, all on the one pty input.
- B6 (Herdr not modified): met. The adapter uses only existing Herdr RPCs (`pane.get`, `pane.send_text`, `pane.send_keys`, `agent.start`).
- B9 (continuity key): this cluster supplies the delivery mechanism and nothing more. Whether the key is ever emitted depends on `model::launch::agent_env` and the D1 reservation gate. At head every executor start site calls `agent_env(&clone, false)` (src/reconcile/herdr_exec.rs:560, :658), so before D1 only `HERDR_GRAPH_CLONE` is exported. That trace leaves this cluster: it belongs to the seam reviewer and to the gate hand-back (hg-66u.1/.2/.3), so I do not call the row undelivered here.
- B4/B5 (unattended restart and handoff): the fake's `live_handoff` follows spec sections 6-7 (public ids preserved, tokens absent, so matching falls back to preserved-id equality). The recovery behaviour built on it is tested and delivered in other clusters (tests/daemon_composed.rs, reconcile, observe), which are the seam reviewer's to trace.
- Real-Herdr evidence: `tests/herdr_client.rs::private_start_agent_env_reaches_the_agent` (outside this cluster) runs the real adapter path against a private rig and checks that both variables reach the agent's environment. task-7-report.md records it passing (6 passed). I did not run it again here.

## Verification run here

- I created a review-owned detached worktree at head (/var/home/alepar/AleCode/herdr-graph/.worktrees/review-c9-inv-cc29fc83). Its target dir and the third_party/herdr-threads submodule were reflink-copied from the run worktree, and the submodule sits at the head's gitlink cdc8423. In it I ran `toolbox run -c ht-build bash -lc 'cargo test --features test-support --lib herdr::'`: 40 passed, 0 failed. That includes client_start_agent_exports_env_before_agent_start, client_start_agent_refuses_unsafe_env_values, client_start_agent_maps_outcomes and fake_live_handoff_keeps_ids_reallocates_terminals_and_drops_tokens.

## Behaviour analysis

- Failure mapping: a failed export reuses the `start_agent` rejection mapping. Timeout becomes Unknown, `agent_not_ready` becomes BlockedNeedsHuman, any other Rejected becomes NeedsRevision. Unknown after an export timeout is pessimistic, since `agent.start` was never sent, but it is safe. The executor resolves an Unknown start from the snapshot: no agent on the pane means it goes back through the shell-foreground guard and starts again (herdr_exec.rs:545-575).
- Partial delivery: if `pane.send_text` succeeds and the Enter in `pane.send_keys` then fails, the export text stays on the shell's input line. A retry types the export again and presses Enter, which runs a harmless doubled export. A manual command typed later would be appended to that leftover text. This is low-likelihood and recoverable.
- Variables persisting in the pane shell after the start is intended by the spec (a manual relaunch presents the same identity), and the spec's section 14.10 removes the retired-pane-open case.
- The fake's `start_agent` records `StartAgent` including `env`, so callers' tests can assert the env. It does not apply the adapter's value validation, so a fake-backed test cannot catch a caller passing an unsafe value. Values are `cl_` ids by construction, so this is a fidelity note only.
- `live_handoff` keeps `envs` and `processes` because pane ids do not change (unlike `restart`, which has to re-key them), and it increments `restarts`, which moves `server_pid` and `server_started` as the incarnation requires. `renumber_tabs_and_workspaces` re-keys workspace and tab ids only. No other fake map is keyed by those ids, so nothing is left dangling.
- Cosmetic: the `start_agent` doc comment now runs the older `agent.start`/name/outcome paragraph straight into the new "Starts the agent on the pane..." paragraph with no break, so the summary line is no longer the first sentence.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- The leading-space history behaviour and `export K=V` acceptance under zsh (`HIST_IGNORE_SPACE`), bash (`ignorespace`) and fish are claimed in spec section 5 but not tested; the private rig test uses a single shell and checks only that the env arrives, not history.
- No unit test covers the export-step failure arms in `start_agent` (send_text or send_keys Timeout becoming Unknown, Rejected becoming NeedsRevision, or text sent with the Enter lost).
- private_start_agent_env_reaches_the_agent (real Herdr) was not re-run in this review; its pass rests on task-7-report.md.

## Deferred OK
- A failed Enter after a successful `pane.send_text` (src/herdr/client.rs start_agent) leaves the export text on the pane's input line, so the next typed command is appended to it; a retry recovers and the case is low-likelihood.
- FakeHerdr::start_agent (src/herdr/fake.rs) does not apply the adapter's env validation, so fake-backed caller tests cannot catch an unsafe env value; values are cl_ ids by construction.
- The start_agent doc comment (src/herdr/client.rs) merges the old and new paragraphs without a break; cosmetic.
