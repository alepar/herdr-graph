# Final review: cluster 13 (src-ports), epic hg-66u

Invocation: inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f
Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head e4984a492e2d3ce8344b9be83d56a3471049fd60 (both verified with `git rev-parse --verify <sha>^{commit}`).
Caller checkout observed at HEAD e4984a49, working tree clean, before and during review.
Cluster inputs: src/ports/herdr.rs, slice slices/000049.diff.

## Change under review

The slice adds one field to the Herdr port request type:

    pub struct StartAgent { pane, kind, args, /// doc comment
        pub env: Vec<(String, String)> }

This is the port extension the spec calls for in section 5 and in the Key abstractions entry "StartAgent / HerdrApi adapter": `StartAgent` gains `env`, delivered only by the Herdr client adapter as a space-prefixed shell export before `agent.start`, with callers never seeing the mechanism. The field type mirrors the existing `SplitPane.env: Vec<(String, String)>` in the same file, so it matches the codebase.

## Checks performed

- Serialization compatibility: `StartAgent` derives only `Debug, Clone, PartialEq, Eq` (no serde), so no persisted record of this type can fail to deserialize. The one serialized mirror, `Logged::StartAgent` in src/daemon/compose.rs (persistent fake log), carries `#[serde(default)]` on `env`, so old logs replay. Not a risk.
- Doc-comment claim: src/herdr/client.rs `start_agent` (head, lines 568-607) sends `export_line(&req.env)` plus Enter via `send_keys` before `M_AGENT_START` when env is non-empty, and returns NeedsRevision on a refused value. The port's documented contract holds at head. The fake records the request including env, so tests can assert it.
- Construction sites compile against the new field at head: src/reconcile/herdr_exec.rs (start and relaunch, both via `model::launch::agent_env`), src/reconcile/session.rs (ReplaceSession, from `Relaunch.env`, built at herdr_exec.rs:658 via `agent_env`), src/daemon/compose.rs replay, plus tests in src/herdr/tests.rs, tests/herdr_client.rs, tests/herdr_spikes.rs. No production site builds env by hand.
- Requirement baseline: the spec has a `## Requirement baseline` (B1-B11, no human scope changes). The only row this cluster touches is B9 (continuity key) together with B6 (Herdr unchanged). The port field is the necessary carrier and does not modify Herdr, consistent with B6. Emission of `HERDR_THREADS_CONTINUITY_KEY` itself is gated on D1 by the spec (every production site passes `key_reserved = false` at head); whether B9 is delivered end to end belongs to the gate accounting and the seam reviewer, since the path leaves this cluster (model::launch, reconcile executor, Herdr client adapter).

## Assessment

The cluster is a minimal, correctly placed port change that matches the spec's abstraction boundary and introduces no defect of its own. Behaviour driven by the field lives in other clusters.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- none

## Deferred OK
- B9 continuity-key delivery through this port field is end-to-end only after the D1 gate (all production sites pass key_reserved=false at head); tracing it from entry point to agent environment is the seam reviewer's and gate accounting's, not this cluster's.
