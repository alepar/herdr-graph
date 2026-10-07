---
super-roast verdict: clean (4 nits)
mode: design        iteration: 1 of 3
profile (assumed): Local internal developer tool. This reviews a descriptive state inventory against current code, not a deployment gate or backup manual. Real data persistence matters, but omissions must be material within the actual summary scope.
inputs: GRAPH_STATE.md (untracked, SHA256 in inputs.json) vs code at 1603214845745174f5036ea79375198c7a44a1dd
coverage: completeness, feasibility, filesystem-schema, identity-lifecycle, integration-ownership, transactions-recovery, transcripts-processing, instructions-configuration · 8/8 scouts completed · 7 raw → 6 deduped → 2 panel / 4 spot-checked (0 promoted) · judge completion 100% (10/10) · remainder-capped: 0
independence: same-family (GPT) — seat-differentiated panel
seat-agreement: panels 2 · rr 1.00 · rg 1.00 · fg 1.00 · unanimous 1.00 · ground-loo 1.00 (n=2) · reproduce 0/2/0 · refute 0/2/0 · ground 0/2/0

## Confirmed findings
- none

## Not verified (beyond panel cap)
- none

## Beyond remainder cap (count only)
- none

## Rejected (with reason)
- [FYI] F01 — GRAPH_STATE.md:3,31,47 — Omitted distinction between authoritative committed `refs/heads/main` and the visible checkout.
  verdict: rejected (reproduce REJECT / refute REJECT / ground REJECT).
  evidence: All three seats verified committed-tree reads (`src/store/git.rs:110-128`) and dirty-preserving checkout projection (`src/writer/worktree.rs:110-121`). They found no document claim that checkout bytes are authoritative or direct edits become effective. The rough storage inventory remains accurate; an authority explanation would add scope rather than correct a demonstrated material mismatch.
- [FYI] F05 — GRAPH_STATE.md:43,47 — Omitted complete local operation lifecycle and terminal outcomes.
  verdict: rejected (reproduce REJECT / refute REJECT / ground REJECT).
  evidence: All three seats verified that rejected/failed/cancelled outcomes remain in SQLite, successful mutations produce Git records, and live operation consumers read the journal (`src/writer/mod.rs:279-282,321`; `src/plan/ops.rs:98-109,134-149`). The reproduce seat ran `cargo test --offline --lib writer::tests::same_rev_renames_one_rejected_with_explanation -- --exact`: 1 passed. “Mutation history” does not promise all attempted outcomes, and the nonexclusive local-state list explicitly warns that the journal is durable recovery state. No material contradiction was established.

## Unverified nits (spot-checked)
- [FYI] F02 — GRAPH_STATE.md:47 — Proposed omission of durable session-report ingress and preserved user files.
  spot vote: REJECT (FYI); outcome: rejected candidate, retained here by spot-tier routing; no supported correction.
  evidence: The refute seat verified `.graph-local/session-spool` (`src/transcripts/capture.rs:28-62`) and preserved files under `.graph-local/orphans` (`src/writer/worktree.rs:138-169`). The document’s rough layout and “includes” wording make the inventory nonexclusive, and it already identifies durable Git-ignored recovery state. It gives no deletion, backup, or reconstruction instruction contradicted by these mechanisms.
- [Nit] F03 — GRAPH_STATE.md:42 — “Fixed input range” overstates request-range stability while pending.
  spot vote: CONFIRM (Nit); outcome: supported minor correction from one refute seat; not panel-confirmed.
  evidence: `RequestCreate` merges overlapping Pending requests under the oldest ID and expands its range (`src/transcripts/mutations.rs:375-405`); the seat inspected the end-to-end source test at `src/transcripts/tests.rs:908-934`, which changes `[0,400)` to `[0,600)` under the original ID. Delivery uses the updated range; Delivered requests are excluded from pending merges (`src/transcripts/mutations.rs:477-488`).
  fix-shape hint: Use “explicit input range” or qualify that pending ranges can coalesce.
- [Nit] F04 — GRAPH_STATE.md:47 — The daemon socket can live outside `.graph-local/` for long instance paths.
  spot vote: CONFIRM (Nit); outcome: supported minor correction from one refute seat; not panel-confirmed.
  evidence: `src/config.rs:190-199` returns `/private/tmp/herdr-graph-<uid>/<hash16>.sock` when the direct path is at least 100 bytes. The daemon binds that path (`src/daemon/mod.rs:102`) and clients resolve the same path (`src/daemon/client.rs:127-128`). The local lock contains a reference, not a socket alias; doctor reports the correct location. Normal daemon operation remains functional, so the seat calibrated this narrow location exception as Nit.
  fix-shape hint: Qualify the socket location with the long-path fallback.
- [FYI] F06 — GRAPH_STATE.md:49 — Proposed missing ownership qualification for fallback processing-request acknowledgements.
  spot vote: REJECT (FYI); outcome: rejected candidate, retained here by spot-tier routing; no supported correction.
  evidence: The refute seat verified that fallback manual ACK writes request dispatch bookkeeping (`src/transcripts/mod.rs:596-604`; `src/transcripts/mutations.rs:544-548`), while actual thread receipts come from `threads.receipt_state` (`src/transcripts/liveness.rs:239-260`). GRAPH_STATE.md:24,42 already identifies the request record and dispatch status; fallback request bookkeeping does not contradict the statement about actual thread receipts.

## Escalations (need human)
- none

The user selected all eight lanes listed in coverage, replacing the default premortem/failure-mode/YAGNI roster and requiring document-to-code evidence. Native GPT agents were used because the prescribed Claude model tiers were unavailable in this harness; seat differentiation does not establish model-family independence. There is no prior report, so this first iteration makes no convergence claim.

The verdict’s four-nit count counts every entry in the required spot section, including its two rejected FYI candidates. Only F03 and F04 support minor wording corrections; neither received full-panel confirmation. No severity was demoted by the reporter, and no panel arithmetic was overruled.
---
