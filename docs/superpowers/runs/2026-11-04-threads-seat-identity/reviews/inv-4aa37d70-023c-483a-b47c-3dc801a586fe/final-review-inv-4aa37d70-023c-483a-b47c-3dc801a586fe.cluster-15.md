# Final review: cluster 15 (src-ports), epic hg-66u

## Scope and source

- Integration branch super-auto/threads-seat-identity. Captured source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735. I verified both with `git rev-parse --verify <sha>^{commit}`. The review source was bound with dispatch-text `bind-review-source` (exit 0).
- Cluster inputs: `src/ports/herdr.rs` (slice 000075) and `src/ports/threads.rs` (slice 000076), from the assembled clusters.json.
- Spec: `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md` at head. It has a `## Requirement baseline` with rows B1-B11 and no human scope change rows. I used §5 (StartAgent env), §6 (ThreadsSeatView and the seat-state mapping), the Key abstractions entries for SeatStatus and ThreadsSeatView, and §14.9.
- Caller checkout: HEAD 6576a117, working tree clean at the start and again before this file was written. I created no worktree. All reads were `git show` / `git grep` at head, plus a read-only look at the v3.0.0 submodule source (`third_party/herdr-threads/src/store/queries.rs`).

## What the cluster changes

- `StartAgent` gains `env: Vec<(String, String)>`. Its doc comment says the adapter delivers it as a shell export before `agent.start`. That matches §5 and the StartAgent/HerdrApi key abstraction: env lives on the Herdr port, and Herdr itself is not changed (B6). `StartAgent` is not serde-derived and never persisted, so the new field cannot break any stored record.
- New `RetirementKind { Reversible, Final }`. Its doc says nothing produces `Reversible` until D2. That respects the epic constraint against faking an unreleased threads feature.
- New `SeatStatus { Resolved{pane}, Unresolved, Retired{kind}, KeyConflict{pane: Option} }`. This is the closed set from the SeatStatus key abstraction. It carries no graph decision and no wire shapes. Every seat state is a value, with no error variant, as the abstraction requires.
- New `ThreadsSeatView::inspect(&[ThreadsSeatRef]) -> Result<BTreeMap<ThreadsSeatRef, SeatStatus>, ThreadsError>`. It is read-only and fails the whole batch rather than returning a partial map. This matches the ThreadsSeatView abstraction (owns read-only inspection, never writes, never fabricates a state when threads is unreachable).

## Risk tracing beyond the diff

- Env producers at head: `reconcile/herdr_exec.rs` builds env with `agent_env(&clone, false)` and passes it to `StartAgent`. `reconcile/session.rs` (ReplaceSession) passes `to.env`. The compose logging and isolation wrappers and `FakeHerdr` forward `req.env`. The real adapter, `herdr/client.rs::start_agent`, validates names and values against `[A-Za-z0-9_.-]`, sends ` export K=V ...` plus Enter through `send_keys`, then calls `agent.start`. The port contract is therefore met by its only real implementation, and no production start site leaves env empty.
- ThreadsSeatView implementations at head: `ThreadsSeatMap` (`threads/mapping.rs`), `FakeThreadsSeatView` (`threads/fake.rs`) and the compose `LazyMap` wrapper. The daemon compose builds the real view and hands it to `RuntimeLoop`. `observe/step.rs` holds the view but does not call it yet (its doc: "unused until SeatObservation (hg-66u.28)"). hg-66u.28 is still open. Whether threads' answer reaches clone binding and liveness is a question for the seam/observe reviewer, not for this cluster.
- `seat_status` in the adapter maps Resolved-without-target and Remote to `Unresolved`, and Retired to `Final`. It never produces `KeyConflict` (gated on D1, hg-66u.40). This is consistent with the port docs.
- Per-seat semantic errors and the "no partial map" contract. At v3.0.0, `SeatInspect` of a seat id that is absent for the instance returns `ErrorCode::NotFound` (`seat_inspect` in `third_party/herdr-threads/src/store/queries.rs`). `api_error` maps that to `ThreadsError::Rejected`. The real `inspect` returns on the first failing seat, so one stale or foreign seat id fails the read for every clone in the batch. Under §6's rule for `Rejected` ("log it and change nothing"), that means no clone gets seat-driven binding while the bad seat stays in the set. Threads never deletes seat rows, so this needs an unusual cause, such as a threads store reset or an instance change. No consumer reads the view yet, so nothing in production is affected today. It should be settled before SeatObservation (hg-66u.28) consumes the port.
- The trait doc says the error is "the transport error of the first failing read". The adapter also returns `Rejected` for a malformed seat id or an unexpected result, and those are not transport problems. The wording is narrower than the behaviour.

## Baseline trace for this cluster

- B6 (no Herdr change): delivered at the port level. Env rides on the graph's own Herdr port and is delivered through existing pane input, with no new Herdr API.
- B1, B3, B4, B5 (threads authoritative for location and liveness; unattended restart and handoff): the cluster supplies the read port and the closed state set. The production path from `RuntimeLoop` through SeatObservation to bindings leaves this cluster. hg-66u.28 and hg-66u.30 are open, so the seam reviewer has to trace that path. I do not judge these rows undelivered from here.
- B7, B9, B10 (reversible retirement, continuity key, membership restore on undo): the port types reserve `RetirementKind::Reversible` and `SeatStatus::KeyConflict` without producing them, and the keyed env is withheld (`agent_env(.., false)`). This matches the D1/D2 gating rule. Delivery, or reporting these rows as gated, is decided outside this cluster.
- B2, B8, B11: not touched by this cluster.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test drives `ThreadsSeatMap::inspect` (the real adapter) with a batch where a later seat fails (NotFound or transport error) after earlier seats succeeded, so the "no partial map" contract is asserted only by the fake, which fails whole-batch by construction.
- `FakeThreadsSeatView` has no per-seat error injection, so no observe-side test can model one bad seat among good ones once hg-66u.28 consumes the port.
- No test pins the serde shape of `SeatStatus` / `RetirementKind` (internally tagged `state`, snake_case), which matters once §14.9's committed `seat_fact` or status output serializes it.

## Deferred OK
- Before hg-66u.28 consumes ThreadsSeatView, decide how a per-seat semantic rejection (v3.0.0 `NotFound` for a seat id unknown to the instance, now mapped to `Rejected`) is handled: either a per-seat outcome or a documented whole-batch policy. At head, one stale `threads_seat` would stop seat-driven observation for every clone.
- Tighten the `ThreadsSeatView::inspect` doc: it fails on the first error of any kind (transport, malformed seat id, unexpected result, NotFound), not only on a "transport error".
- The `StartAgent.env` doc comment does not state the adapter's refusal contract (names/values outside `[A-Za-z0-9_.-]` yield `NeedsRevision`, empty env sends nothing). It would be worth one line on the port so other implementations match.
