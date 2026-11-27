# Final review inv-2, cluster 25 (src-herdr)

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 985d1eb6faaa2e92e4e01f430f0f2c07618bf6ad (both verified with `git rev-parse --verify <sha>^{commit}`). Caller checkout observed at HEAD 985d1eb, status clean. Review source binding via `dispatch-text.mjs bind-review-source` succeeded.

Inputs: slices/000140.diff (src/herdr/client.rs), slices/000141.diff (src/herdr/fake.rs), slices/000142.diff (src/herdr/tests.rs), digest.md for context. Spec: `bd show hg-lnw --json` and docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md (Requirement baseline B1-B4, no human scope changes).

## What the cluster changes

The cluster is the hg-lnw.23 fix (commit c95536b). `agent.start` needs a name that is unique among live Herdr agents. Before the fix, a second unlabelled pane of the same kind got `agent_name_taken`, which mapped to NeedsRevision and was never retried. So that pane never got its graph-started agent, and that broke the frictionless restart for adopted unlabelled panes (B2).

- `client::agent_names(label, kind, pane)` returns `[first, retry]`. `first` is the sanitized label, else the sanitized kind, else `agent`. `retry` is `first` cut to `HERDR_AGENT_NAME_MAX - 1 - len(pane id)` plus `-<pane id>`, sanitized again (`w1:p2` becomes `w1-p2`).
- `HerdrClient::start_agent` submits `first`. Only when that fails with `agent_name_taken` (checked with `wire::is_code`, which tests the `code:` message prefix) does it submit `retry`, once. Every other outcome mapping is unchanged: Timeout is Unknown, `agent_not_ready` is BlockedNeedsHuman, other rejections are NeedsRevision. If both names are taken, the result is NeedsRevision carrying the `agent_name_taken` reason.
- `FakeHerdr` now enforces live-name uniqueness. It calls `client::agent_names`, records the chosen name per pane, carries names across the pane-id remap on restart, and returns NeedsRevision with an `agent_name_taken:` reason when both names are live.
- Tests: a scripted-socket test of the one retry (`client_start_agent_retries_taken_name_with_pane_suffix`) and a pure test of the name rule (`agent_names_follow_herdr_name_rule`).

## Requirement baseline trace

- B2 (frictionless Herdr restart): this cluster's part is starting agents on several unlabelled panes of one kind. The production entry point is `reconcile::herdr_exec` (src/reconcile/herdr_exec.rs:673). It calls `HerdrApi::start_agent`, which is `HerdrClient::start_agent` in production. The only `agent.start` sender at head is client.rs. The fake path is covered by `reconcile::tests::two_unlabelled_claude_panes_both_start`. The task-16 review records the real-Herdr tier-3 run with scenario 6's label workaround removed (tests/e2e_private_herdr.rs). That run is outside this cluster and is the seam reviewer's to confirm. Within the cluster, this part is delivered.
- B1, B3, B4: not touched by this cluster.

## Risk analysis

- Self-collision retry (roast-pr-1, client.rs:621). An earlier start whose response was lost could make a re-issued start collide with its own pane's name and retry under the pane-qualified name. The production caller blocks this in the common case: herdr_exec.rs:608 checks `lp.pane.agent.is_some()` before calling `start_agent`, and an Unknown effect with an agent present resolves to Done. The race remains only if the snapshot does not yet show the agent that started. This is narrow, and Herdr's behaviour for a second `agent.start` on an occupied pane decides the impact.
- Long pane ids. When the pane id is 31 bytes or more, `keep` is 0. The retry name then falls back to `first`, so the retry repeats the same name and fails the same way. That is harmless, and real pane ids (`wN:pM`) are far shorter.
- The byte slice `first[..n]` is safe because `sanitize_agent_name` output is ASCII (`[a-z0-9_-]`).
- Fake fidelity. The fake repeats the client's retry rule instead of exercising the client. Agents that were not started through `start_agent` hold no name, so a collision with an externally named live agent is not modelled. The scripted-socket test covers the real client path.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- Real Herdr collision of `first` with an externally started agent that holds the same name (another pane's label equal to this kind or label); only the scripted socket and the fake exercise the retry.
- The long-pane-id path where `retry` equals `first`; there is no test, and real ids are short.
- Herdr's behaviour when the pane-qualified retry lands on a pane whose earlier start succeeded but whose response was lost (self-collision race).

## Deferred OK
- Self-collision retry window (roast-pr-1, src/herdr/client.rs:621): narrowed by the pane-agent precheck at src/reconcile/herdr_exec.rs:608; a follow-up could re-check `agent(pane)` before the retry.
- FakeHerdr repeats the client's one-retry rule (src/herdr/fake.rs start_agent); divergence risk if the client policy changes.
- `agent_names` retry degenerates to `first` for pane ids of 31 bytes or more; harmless but redundant.
