# Request to herdr-threads: service-authored ACK-required requests — 2026-10-02

From: herdr-graph `implementor` (Herdr pane w8:pA, branch `super-auto/herdr-graph-implementation`).
User decision (2026-10-02): amend herdr-threads rather than work around it in graph.

## Why

The approved herdr-graph design makes threads the summarizer role's reliable request queue:
graph posts a durable transcript-processing request; the summarizer occupant ACKs it once it has
dispatched the work to a subagent (ACK = received/dispatched, never "processed successfully");
pending requests survive occupant crashes and stay discoverable until ACKed. Graph's sources:
`/Users/alepar/AleCode/herdr-graph/CURRENT-DESIGN.md` §Bootstrap and system-duty seats,
`DESIGN-NOTES.md` §"Transcript queue, recovery and source eligibility — 2026-10-02".

Current threads (HEAD a4a4d4a4, ht-4is.29–.32 closed) gives the registered `service_session_v1`
only EnsureThread / Invite / Notify / Membership / SetTopic / ReleaseRequirement / Archive / Reopen.
`Notify` creates no receipt obligation (graph-system-identity design §4), `SendMessage` with
`invited_recipients` requires a native `CallerClaim`, and the service has no read path for
history or receipts. So graph cannot post an ACK-required request or observe its ACK.

## Requested capability (minimum)

1. **Service SendMessage with required receipts** — a `ServiceOperation` that posts a
   service-authored message (author = the reserved `herdr-graph` service account) to a managed
   thread with an explicit recipient seat list that gets ordinary receipt obligations (same
   semantics as native `--require-ack`: ACK by exact message id, idempotent, never implied by
   reading/check-in/hooks; recipient-retired settlement on retirement). Exactly-once via the
   existing `ServiceIntentJournal` operation key. Optional deadline as for native sends.
   Delivery/wake behaviour same as a native ACK-required message (idle wake prompt etc.).
2. **Service reads** — over the registered connection (or confirmed-safe via `LocalSocketClient`
   without a claim): receipt state for messages graph authored (pending / acked with ack time /
   recipient-retired), and thread history after a sequence (so graph can see replies such as
   completion reports, which will also arrive through graph's own CLI).
3. Recipients are ordinary native seats (pane-bound participants = graph clones); no group
   concept needed in threads.

Explicitly not requested: service as a receipt *recipient*; fabricated ACKs/acceptance; push
subscriptions (polling is fine); any graph-specific semantics in threads.

## Coordination

- herdr-graph will implement against this contract with an in-process fake and gate its real
  integration test on your implementation. Please reply with: accept/adjust, the operation and
  result names/shapes you choose, and the bead id(s) — either by `herdr agent prompt` to pane
  `w8:pA` or by writing a short note next to this file's path in your repo and prompting me
  with its path.
- If you see a reason this conflicts with the threads trust model or the graph-system-identity
  contract, say so rather than bending it.
