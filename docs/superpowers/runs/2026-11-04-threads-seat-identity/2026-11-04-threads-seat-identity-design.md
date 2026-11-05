# Threads seat as the clone identity (graph side)

Run: `docs/superpowers/runs/2026-11-04-threads-seat-identity/` · governing review:
`docs/reviews/2026-10-29-seat-review-H2-identity-design.md` (with `…-H2.md`, `…-H2-identity.md`) ·
threads-side counterpart: herdr-threads epic ht-ma6i (coordinator seat snSGTpBk7xGd, thread
dOqG4VQNjQ4) · H2 bead: hg-s6o.

## Goal

Every graph clone is bound one-to-one, for life, to a herdr-threads pane seat that is authoritative
for where the clone is and whether it is live; the graph no longer re-matches clones itself,
thread participation follows the clone's seat instead of the agent session, and after a Herdr
live handoff or cold restart clones recover without an operator step (resuming their agents
headless when needed), with graph undo restoring a closed clone onto the same seat.

## Requirement baseline

Source: the human user, in the review session that produced this run (Claude Code session in the
herdr-graph tab, 2026-10-30 → 2026-11-04), relayed verbatim.

### Human request

> /super-auto everything we scoped, fully autonomous, both roasts are on

### Requirements

- B1 · "i just want 1:1 mapping. threads identity can be authoritative and graph can simply piggy back on it if it helps" · source: session, 2026-10-30
- B2 · "yeah graph takes hard dependcy on threads. acceptable" · source: session, 2026-10-31
- B3 · "threads re-matching mechanism should be as-good-or-better, and then we can delegate to threads" · source: session, 2026-10-31
- B4 · "restarts should be frictionless/unattended" · source: session, 2026-10-31
- B5 · "'herdr update --handoff' - that should be supported too, ie unattended frictionless resume when it happens. both in threads and graph" · source: session, 2026-10-31
- B6 · "we dont own herdr project and that is not going to change (at least for now) so we will have to make do without changing herdr" · source: session, 2026-10-31
- B7 · "retirement can happen immediately, but should be undoable (some 'graph undo' cli that reopens the tab, resumes the session, unretires the seat)" · source: session, 2026-11-04
- B8 · "pane existing means operator just wants to relaunch in another harness, meaning the seat is still needed." (agent exit keeps seat and memberships) · source: session, 2026-11-04
- B9 · continuity key approved: "ok" to "Should a pane seat accept a launcher-supplied continuity key … as cooperative evidence?" · source: session, 2026-11-04
- B10 · undo fully hands-off: "this pls" to "Avoiding that means threads restores the memberships on un-retire" · source: session, 2026-11-04
- B11 · H2 Option D: "got it. lgtm" to "Option D only stops /seat printing accept commands and leaves acceptance to herdr-threads" · source: session, 2026-11-04

### Human scope changes

- none

## Problem description

A graph clone (one agent identity, one Herdr pane) and a herdr-threads seat (one Herdr pane) are
the same entity, but each system re-derives the clone↔pane mapping on its own. The graph has two
matchers (`src/observe/matcher.rs` token/terminal/session passes; the reconciler's `LiveIndex` in
`src/reconcile/planner.rs:103-330`); both assume Herdr tokens or terminal ids survive, which they do
not across a live handoff or cold restart (Herdr v0.9.5 preserves public workspace/tab/pane ids,
reallocates `terminal_id`, drops metadata tokens). After a restart the graph marks clones Unknown
and the planner does nothing until a human runs `clone rebind`. Thread participation is keyed to
the occupant (`ns_`), so every `/clear` or relaunch releases and re-invites required memberships on
the unchanged threads seat, and `/seat` prints a possibly stale or read-only "accept command" (H2).

## Main challenges

The authoritative source (threads) is released in pieces: v3.0.0 (shipped) has `SeatInspect`,
process continuity across live handoff, and same-session (C1) reattach after cold restart; the
continuity key (D1, ht-ma6i.2), reversible retirement with `UnretireSeat` and membership restore
(D2, ht-ma6i.3), seat-level shell fingerprints (G2, ht-ma6i.1) and limbo disposition (G5,
ht-ma6i.4) are not yet released. Graph work that needs them must wait on them, not fake them.
The graph must also keep working with the threads daemon unreachable (binding waits, B2), keep
seats/teamspaces (tabs/workspaces) matched without its clone matcher, and start agents with an
environment variable although its Herdr `StartAgent` port carries no env (`src/ports/herdr.rs:106`).

## Key decisions made

Threads seat id is a write-once field of the clone, resolved only from a verified pane binding;
location and liveness of a clone come from `SeatInspect` on that seat; the graph's clone
re-matching (both matchers, `/seat` rebind proposals, `clone rebind --pane`) is deleted once the
continuity key is released; tabs and workspaces match by preserved Herdr ids; participation is
keyed to the seat and survives agent exit; agents are launched with
`HERDR_THREADS_CONTINUITY_KEY=<cl_ id>` (threads' neutral name, agreed with the threads coordinator
2026-11-04) after reserving the key; graph undo/resurrection un-retires the seat and relaunches with
the key; a resume driver relaunches agents after a headless cold restart; `/seat` stops printing
acceptance commands. Gated work sits behind gate beads that close when the threads coordinator
announces the release on thread dOqG4VQNjQ4.

## Decision points, by section

### 1. herdr-threads pin: v3.0.0

Recommended: bump the submodule and path dependency from v2.6.4 straight to v3.0.0 (the threads
coordinator's advice: 2.8.x would leave a wire break later). Adapt the adapter
(`src/threads/adapter.rs`), port (`src/ports/threads.rs`) and `tests/threads_real.rs` to the
v3.0.0 protocol; update `CURRENT-DESIGN.md`'s "threads 2.6.4, wire protocol 9" line. This is the
bottleneck bead: it lands only the bump and compatibility fixes. Considered: 2.8.2 (rejected as
above); staying on 2.6.4 until D1 (rejected: live-handoff continuity ht-hsyg is in ≥2.8.2 and C1
behaviour changed since).

### 2. Clone threads seat (write-once)

Recommended: `CloneRecord` gains `threads_seat: Option<SeatId>` (the clone, not `Binding`, because
it survives rebinding of panes by Herdr moves and undo). A threads effect resolves it once the
clone's pane binding is verified (stamped token or verified bound pane in the current incarnation),
via the existing `ThreadsSeatMap::seat_for` (`ResolveSeat`). Rules:
- Never overwritten once set, with one exception, **seat succession**: when threads reports the
  clone's seat `Retired` *final* (irreversible: every pane-close retirement before D2, or an
  operator's final retire after it) and the graph restores the clone (undo or resurrection), the
  assignment may replace the final-retired seat with the seat resolved for the restored pane. The
  succession is recorded on the clone (previous seat id kept for history) and the new seat needs
  memberships re-invited and accepted once, as today. A reversibly retired seat is never succeeded;
  it is un-retired (§8). At any instant a clone has at most one live seat, so B1's 1:1 holds.
- `ResolveSeat` refusals (restore holds, pane held for a key conflict) are `Deferred`, never errors,
  and never retried with a fresh allocation while the seat is unresolved — an early resolve on a
  restored pane would allocate a new seat and defeat threads' reattach.
- Invariant: at most one non-retired clone per `threads_seat`; a commit that would violate it is
  rejected by the writer's validator.
- Backfill: live clones get the field from the same effect; existing `ThreadsLink.seat` values that
  equal the resolved seat need no re-invite (§3 migration).
Considered: on `Binding` (rejected: bindings are rewritten on every Herdr move); graph-minted seat
names (rejected: B1, threads is authoritative; named seats lack Herdr wake).

### 3. Participation keyed to the seat (H2-identity steps 2–3, D3)

Recommended: in `clone_wants` (`src/threads/effects.rs:351`) a clone's desired invitations exist
when it is active and `threads_seat` is known — no occupant needed, so invitations wait before the
first agent's SessionStart. Satisfied when `link.seat == threads_seat` (ordinary: state Pending or
Accepted). Released only when `link.seat != threads_seat` (seat succession, §2) or the want
itself disappears (opt-out, channel removed). Clone retirement does not release: threads ends a
retired seat's memberships itself when retirement is final, and keeps them while it is reversible
(per the provisional ht-ma6i shapes, `threads-ht-ma6i-protocol-shapes.md`), which is what lets
undo come back with no re-acceptance. Graph clone retirement keeps its existing meaning (undoable through `graph undo`
until purge); there is no separate "retired-final" clone state. Agent exit with the pane
open keeps memberships (B8): `occupancy_end_releases_requirement` and
`occupant_replaced_in_one_commit_releases_before_reinviting_the_same_seat` flip. The invite
executor invites the recorded `threads_seat` (no re-resolve, no `Obsolete` for "no occupant");
`ThreadsLink.occupant` becomes attribution only; `SetInvitation`'s `expect_occupant` guard becomes
`expect_seat`. Migration: links whose `seat` equals the backfilled `threads_seat` are satisfied —
no churn, no mass re-acceptance. Update spec §7.1 / `CURRENT-DESIGN.md` "Threads and policy" and
`DESIGN-NOTES.md:331` to say cleanup follows seat lifetime and acceptance is not liveness.

### 4. /seat stops printing acceptance commands (H2 Option D, hg-s6o)

Recommended: `pending_invitations` reports `thread` and `constraint` only; remove `accept_command`
and its formatter (`src/threads/mod.rs:197-240`), update `render_seat` (`src/bootstrap/mod.rs`
~456-466) and tests. `skills/seat/SKILL.md` step 4 (the step that today says run each
`accept_command`) becomes: required → run herdr-threads' ready `accept-required` command from its
hook; if absent, follow herdr-threads' required-invitation procedure (`thread participants` row →
`accept-required`, reread on stale); ordinary → decide by topic and goal. Independent of all other
work. Note: H3 (hg-qam) edits the same `/seat` files; the bead rebases onto main before merge.
Considered: Options A/B/C of the H2 review (rejected there).

### 5. Launching with the continuity key

Recommended: every agent start the graph performs (`start`, `RelaunchOccupant`, `ReplaceSession`,
the resume driver) runs the agent with `HERDR_THREADS_CONTINUITY_KEY=<cl_ id>` in its environment.
Because `StartAgent` has no env and Herdr cannot change (B6), the graph's Herdr port grows an `env`
parameter that the Herdr adapter realises without Herdr changes (argv prefix `env K=V …` or the
existing pane-creation env, whichever the installed Herdr API supports for agent start; the
implementer verifies on the real Herdr). The existing `HERDR_GRAPH_CLONE` pane env stays. Before
the first keyed launch the graph calls threads' `ReserveContinuityKey{key}` (idempotent for the
holder); that call and its port wrapper are gated on D1 (§10). The key variable is emitted only for a
clone whose reservation is recorded, so no agent ever presents an unreserved key (threads records
an unreserved key as non-service, never un-retirable); until D1 no clone has a reservation.
Considered: relying on pane shell env (rejected: lost on cold restore, and Herdr's own auto-resume
does not carry it — C1 covers that case instead).

### 6. Location and liveness from threads; delete clone re-matching

Recommended: a `ThreadsSeatView` reader wraps `SeatInspect`/`Seats` (new port methods) and is
polled per observation step for clones with a `threads_seat`. Mapping: `Resolved` + target pane →
clone Present, bound to that pane (binding written from threads' target, token re-stamped by the
existing stamp path); `Unresolved` → Unknown; `Retired` (reversible or final) → clone retired
through the existing undoable retirement (the reversible/final distinction matters only to undo,
§8); key-conflict field → Unknown plus a recorded alert in `status`. On v3.0.0 every retired seat
is final (threads retirement is irreversible until D2), and the adapter reports it so. Threads unreachable → no binding changes (hold last state; Unknown only after the existing
staleness rule). Then delete: the clone passes of `observe/matcher.rs` (token, terminal, native
session), clone terminal/incarnation tracking used only by them, the clone parts of the planner's
`LiveIndex` (bound-id, rebind-adopted, journal `live_ref` re-matching; the create-nonce match for a
pane the graph is creating stays until its seat is resolved), `/seat` rebind proposals
(`bootstrap/resolve.rs:312-345`) and `clone rebind --pane`. Panes the graph did not create are
adopted only through the first bind of an unseated clone (existing env-based `/seat` resolution),
which resolves its seat once. The deletion is gated on D1 (§10): before D1, a fresh agent or bare
pane after cold restart has no evidence threads accepts, so deleting the matcher first would regress
those cases relative to today's operator rebind; the `ThreadsSeatView` reader and binding-from-
threads mapping land before D1 and run alongside the old matcher (threads wins when both speak).
Considered: keeping the matcher as a fallback (rejected: B1/B3 — two sources of truth is the bug).

### 7. Tabs and workspaces across incarnations

Recommended: replace the majority vote (`matcher.rs:200-286`) with preserved-id equality: a seat's
tab and a teamspace's workspace keep their bound Herdr ids across incarnations (Herdr preserves
them across restore and handoff); token first where present; an id that no longer exists in the
new incarnation → Unknown (no vote). Independent of threads releases. Considered: keeping the vote
as fallback (rejected: silent first-wins, the ambiguity the review flagged).

### 8. Undo and resurrection onto the same seat (D2)

Recommended: closing a clone's pane retires its threads seat (threads does it; the graph issues
nothing). Graph undo of that retirement, and seat resurrection, reopen the tab/pane through the
existing plan path, call threads `UnretireSeat{seat}` (seat returns to unresolved; memberships and
requirement episodes were kept while reversible, B10), then launch the agent with the key (resuming the
last session when the harness supports resume) so the key check-in reattaches the same seat. Undo
no longer adopts the caller's pane (`undo/adopt.rs`): a different pane has its own seat, which
would break 1:1. When the graph's retirement becomes final (purge), the graph releases the key /
finalizes retirement (operation per D2's published API). After reattachment the graph re-checks its
wants and releases any requirement it no longer wants. All of this is gated on D2.
**Before D2 (interim):** undo and resurrection keep today's behaviour (restore through the plan
path, adopting the caller's pane where they do today); the restored clone's final-retired seat is
succeeded by the restored pane's seat (§2), and memberships are re-invited and accepted once. The
D2-gated work replaces this with un-retire for reversibly retired seats; seat succession remains
only for operator-finalized seats.
Considered: true un-retire (rejected by the user in favour of reversible state, threads-side);
new seat on undo (rejected: breaks B1).

### 9. Resume driver after a headless cold restart (G4)

Recommended: a reconciler step: for a clone whose `threads_seat` is `Unresolved` and whose last
bound pane id exists in the current incarnation holding a bare shell (no agent, shell foreground via
`process_info`), after the existing 90 s incarnation grace, start the agent in that pane: resume the
clone's last ended session when the harness supports it, else fresh; always with the key. The pane
id is a locator only — the seat attaches by C1 (session) or D1 (key) evidence wherever the agent
actually runs, so a wrong locator self-corrects. The resume-by-session path works on v3.0.0 (C1);
the fresh-agent path is gated on D1. Never launches into a pane that has an agent or whose seat is
held for a key conflict. Considered: waiting for a client to attach (rejected: B4).

### 10. Gating on threads releases

Recommended: three gate beads (`sp:gate`), closed by the orchestrator when the threads coordinator
announces the release on thread dOqG4VQNjQ4 (the pin bump is the first gated bead's job):
`Gate: herdr-threads D1 continuity key released (ht-ma6i.2)`, `Gate: herdr-threads D2 reversible
retirement released (ht-ma6i.3)`, `Gate: herdr-threads G5 limbo disposition released (ht-ma6i.4)`.
Each gated bead's description names its gate. A gate release that comes with a new threads version
includes a pin bump in the first gated bead. G2 (ht-ma6i.1) needs no graph work (threads-internal),
only a pin bump when released, folded into the D1 gated bead. If gates are still closed when the
rest of the tree is done, the run reports the gated subtree as remaining, unexecuted.

### 11. Testing

Unit tests per bead (fakes: `FakePaneSeatMap`, a fake `ThreadsSeatView`). Real-Herdr qualification
(`tests/e2e_private_herdr.rs` restart rig, `tests/threads_real.rs`) for: live handoff with idle
agents, bare panes and an exited agent; cold restart with a client attached and headless; pane
closed during downtime; close-then-undo keeping the seat id (after D2). Existing tests that encode
the old matcher (`observe/tests.rs` restart cases) are replaced by threads-driven equivalents. Never
run against the user's live graph instance or live Herdr layout: private rigs only.

### 12. Out of scope

All herdr-threads changes (owned by ht-ma6i); any Herdr change (B6); H3 manager seats (hg-au7).

## Key abstractions

**SeatStatus** — new enum (graph-local value type beside the threads port types), what threads says about one clone's seat.
- Owns: the closed set of seat states the graph acts on: `Resolved` (with target pane), `Unresolved`, `Retired` (reversible or final) and `KeyConflict`.
- Hides: the threads v3 wire shapes of `SeatInspect`/`Seats`. The adapter maps them to these variants and nothing else sees them.
- Must not: carry graph decisions such as availability or retire requests. SeatObservation makes those.
- Relationships: produced by ThreadsSeatView and consumed by exactly one `match` in SeatObservation. The resume rule in the planner reads it only through committed state and journal.
- Errors: none. Every seat state, including a key conflict, is a value. Only transport problems are errors, and those belong to ThreadsSeatView.

**ThreadsSeatView** — new port trait, a read-only view of threads seat state for a set of seats (§6).
- Owns: inspecting the `threads_seat` of every clone that has one, once per observation step.
- Hides: whether the reads are batched (`Seats`) or per seat (`SeatInspect`), the host qualification, and the socket client.
- Must not: write to threads (no resolve, reserve or unretire; PaneSeatMap does those), commit graph state (SeatObservation decides and the step commits), or fabricate a state when threads is unreachable.
- Relationships: a port in `src/ports/`, like the others, with a real implementation (the same struct as `ThreadsSeatMap`, sharing its client, host and clock) and a fake (§11). `RuntimeLoop` holds an `Arc<dyn ThreadsSeatView>`. The dependency runs from observe to the port, never the reverse.
- Lifecycle: built once in daemon compose next to `ThreadsSeatMap`, shared by `Arc`, lives as long as the process; no state apart from the client connection.
- Errors: `ThreadsError`. Callers tell `Disconnected`/`ServiceBusy` (hold the last state; Unknown only under the existing staleness rule) apart from `Rejected`/`Unsupported` (log it and change nothing).

**SeatObservation** — new module in `observe/`, turns seat statuses into observation facts for one step.
- Owns: the single `match` on SeatStatus. `Resolved` binds the clone to threads' pane and marks it Present (the existing stamp path re-stamps the token). `Unresolved` marks it Unknown. Reversible `Retired` retires the clone undoably; final `Retired` retires it finally. `KeyConflict` marks it Unknown and records an alert that `status` reads. It also owns precedence: before D1, threads wins whenever it and the old matcher both claim a clone.
- Hides: how threads' answer is merged into `Matches`/`build_ops`. The binding, retire and availability request builders stay as they are.
- Must not: call threads (it receives statuses already read), or touch tabs or workspaces (matcher, preserved-id equality, §7).
- Relationships: a function the step calls after `match_snapshot`; takes SeatStatus values and yields entries for `Matches.clone_pane` plus ops.
- Lifecycle: stateless functions. The key-conflict alert is a journal entry.

**ThreadsSeatAssignment** — new bookkeeping mutation (`bookkeeping.threads_seat`, registered beside `bookkeeping.binding`), the only writer of `CloneRecord.threads_seat`.
- Owns: the write-once rule, its one exception (seat succession of a final-retired seat on undo or resurrection, §2), and the invariant that at most one non-retired clone holds a given `threads_seat`.
- Hides: the overwrite and uniqueness checks against the committed tree.
- Must not: resolve seats (the threads executor does that through PaneSeatMap) or retire anything.
- Relationships: one more entry in the existing `MutationRegistry`, the same shape as `bookkeeping.binding`; the threads executor admits it.
- Errors: `MutationError`. Writing the seat already set is an idempotent success, distinct from a different seat (rejected) and from a seat another live clone holds (rejected, surfaced, never retried).

**CloneRecord** — extended: gains `threads_seat` (write-once, §2). `ThreadsLink.occupant` becomes attribution only, and `SetInvitation`'s `expect_occupant` guard becomes `expect_seat` (§3).

**PaneSeatMap** — extended: becomes the port for seat-lifecycle commands. It already has resolve; it gains reserving the continuity key (D1), unretiring a seat (D2) and finalizing a retirement or releasing the key (D2, per the published API). Its resolve reports threads' refusals (restore hold, key-conflict hold) as a non-error "not yet" so the executor returns `Deferred` (§2). Implementations stay `ThreadsSeatMap` and `FakePaneSeatMap`.

**ThreadsSource / ThreadsExecutor (`Payload`)** — extended: new want kinds, still chosen in `system_wants` and dispatched in `execute_scoped`:
- resolve seat: for an active clone with a verified pane binding and no `threads_seat`; it admits ThreadsSeatAssignment.
- reserve key: for an active clone with no recorded reservation, before its first keyed launch.
- unretire seat: for an active clone whose seat threads reports as reversibly retired, the state undo or resurrection leaves.
- finalize retirement: on purge.

`clone_wants` keys desired, satisfied and released on `threads_seat` instead of the occupant (§3). The invite executor invites the recorded seat and does not re-resolve it.

**StartAgent / HerdrApi adapter** — extended: `StartAgent` gains `env`. The Herdr client adapter alone decides how to deliver it (an `env K=V` argv prefix or the agent-start env, §5).

**`model::launch`** — extended: gains the continuity-key env constant and one function that builds the env for every agent start (`HERDR_GRAPH_CLONE` plus `HERDR_THREADS_CONTINUITY_KEY=<cl_ id>`). Every start site calls it: `HerdrExecutor` start/relaunch, `session::start` for ReplaceSession, and the resume rule. No site builds the env by hand.

**Reconcile planner (`plan_effects`, `LiveIndex`)** — extended:
- A resume rule plans the existing `StartAgent` effect for a clone in the §9 situation: its seat is Unresolved, its last bound pane id exists in this incarnation, and that pane has no agent. The executor already applies the 90 s grace, checks for a shell foreground and chooses resume over fresh. The rule skips clones that carry a key-conflict alert.
- The clone side of `LiveIndex` shrinks to a lookup by committed binding plus the create-nonce match. Rebind adoption and `live_ref` re-matching are removed after D1 (§6).

**Matcher** — extended (narrowed): tabs and workspaces match by token, else by preserved-id equality, and become Unknown when the id is absent (§7). The clone passes are deleted once D1 lands (§6).

Variation: threads seat states (G5 limbo is pending) → one closed enum, SeatStatus, matched only in SeatObservation. Threads seat commands (resolve, reserve, unretire, finalize) → fixed operations on PaneSeatMap, with effect kinds chosen in `ThreadsSource::system_wants`. Agent start sites → one env builder in `model::launch`. How env reaches Herdr → hidden in the Herdr client adapter. Release gates are delivery order (gate beads), not runtime flags: before D1 the code simply has no reserve want, with no capability switch.

Walk-through, cold restart headless: the new incarnation forces a rebind step; `RuntimeLoop` snapshots Herdr and asks ThreadsSeatView about each clone's `threads_seat`; threads answers `Unresolved`, SeatObservation marks the clone Unknown; the planner's resume rule sees the last bound pane present with no agent and plans `StartAgent`; the executor waits out the grace, confirms the shell foreground and starts the agent with the `model::launch` env and `--resume` where supported; threads reattaches the seat by C1 or D1 evidence; the next step reads `Resolved{pane}`, SeatObservation writes the binding and marks the clone Present; `clone_wants` finds `link.seat == threads_seat` satisfied, so nothing is re-invited.

Future change, G5 limbo disposition: one new SeatStatus variant, mapped in the adapter, and one new arm in SeatObservation's `match`. No caller changes.

Spec-forced exceptions:
- Per §6: before D1, threads and the old clone matcher run side by side and threads wins whenever both speak. Deliberate and temporary; the matcher's clone passes are deleted in the D1-gated bead.
- Per §8: undo no longer adopts the caller's pane (`undo/adopt.rs` removed for clones). Undo commits the clone active; unretire, relaunch and key steps follow from level-triggered wants. The undo code makes no threads calls.
- Per §3: `ThreadsLink.occupant` stays on the record, but nothing may branch on it any more.
- Per §5: the key env var is emitted only once the clone's key reservation is recorded (none before D1).

Digest:
- SeatStatus: owns the closed set of threads seat states; a new state is added as one variant plus one arm in SeatObservation; callers never inspect threads wire results.
- ThreadsSeatView: owns read-only seat inspection per observation step; new reads are added on this port and its fake; callers never write to threads through it or invent a state when threads is unreachable.
- SeatObservation: owns turning seat statuses into clone binding, availability and retirement, plus precedence over the old matcher; new seat-state handling is added in its single match; callers never map seat states to graph changes elsewhere.
- ThreadsSeatAssignment: owns the write-once `threads_seat`, its seat-succession exception and its one-live-clone-per-seat invariant; new rules on the field are added in this mutation; callers never write `threads_seat` directly or retry a rejected different seat.

## Post-Implementation Notes

*As this design is implemented and iterated on — bug fixes, adjustments, anything that diverged from the assumptions above — append a dated note here, whether or not a formal debugging skill was used.*
