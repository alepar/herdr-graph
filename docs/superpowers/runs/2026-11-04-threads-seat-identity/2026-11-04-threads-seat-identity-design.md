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

- S1 · 2026-11-07 · "1 lgtm" (to: clones with no threads seat — Shell-harness clones and agents that never check in — are matched by preserved Herdr pane id, the rule tabs and workspaces use; the old matcher is deleted otherwise) · source: session, 2026-11-07 · affects: B3

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
- Never overwritten once set, with one exception, **seat succession**: when the clone carries a
  `Restore{seat, epoch}` intent (§8, written by the undo or resurrection commit) whose `seat` threads
  reports `Retired` *final* (irreversible: every pane-close retirement before D2, or an operator's
  final retire after it), a succession want resolves the seat of the pane the restore reopened (the
  plan path writes that pane as the clone's binding when it creates it, so the binding is verified
  by the create nonce) and the assignment replaces the final-retired seat with it. The assignment
  admits succession only against a `Restore` intent with the same `seat` and `epoch`; without one a
  different seat is rejected as before. The succession is recorded on the clone (previous seat id
  kept for history) and the new seat needs memberships re-invited and accepted once, as today. A
  reversibly retired seat is never succeeded; it is un-retired (§8). At any instant a clone has at
  most one live seat, so B1's 1:1 holds.
- Unseated clone after D1: an active clone with `threads_seat = None` and a recorded key
  reservation (§5) needs no pane binding to learn its seat. The resolve want for it re-issues
  `ReserveContinuityKey{key}` (idempotent for the holder); when the answer carries `seat` (bound at
  the first keyed check-in), that seat is admitted through the assignment. The key is the evidence,
  so this works with the matcher deleted and after a restart; the launch that produces the check-in
  is the resume driver's (§9). Before D1 an unseated clone is bound by the old matcher as today.
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
`expect_seat`. Migration: links whose `seat` equals the backfilled `threads_seat` and whose state
is Pending or Accepted are satisfied — no churn, no mass re-acceptance. The satisfied test checks
state as well as seat for every constraint, Required included: a link on the same seat in state
Released or Retired (left by the old occupant-keyed release) is unsatisfied, so the want of an
active clone re-invites it once on the recorded seat; a retired clone's want invites nothing
(§16.2). A migration test seeds a Released Required row on the backfilled
seat and expects exactly one re-invite. Update spec §7.1 / `CURRENT-DESIGN.md` "Threads and policy" and
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
parameter that the Herdr adapter delivers by exporting it into the pane's shell just before
`agent.start`. Herdr v0.9.5's `agent.start` has no env field (`AgentStartParams`: name, kind,
pane_id, args; `env` exists only on `PaneSplitParams`, pane creation) and cannot take an `env`
argv prefix (args follow the fixed agent executable). It does submit the rendered agent command as
typed input into the pane's existing interactive shell (`src/app/agents.rs` `start_agent`), so the
agent is a child of that shell and inherits its environment. The adapter therefore sends
` export HERDR_GRAPH_CLONE=<cl_> HERDR_THREADS_CONTINUITY_KEY=<cl_>` plus Enter through the existing
text/keys path (`pane.send_text`/`pane.send_keys`, the port's `send_keys`), then calls
`agent.start`; both go into the same pty input in order, so the shell runs the export first. Rules:
the executor's existing guard (pane exists, shell in the foreground, no agent) runs before the
export, as before a start; values are `cl_` ids, and the adapter refuses any value outside
`[A-Za-z0-9_.-]` instead of quoting it; the leading space keeps the line out of history where the
shell ignores space-prefixed lines (fish by default; bash `ignorespace`, zsh `HIST_IGNORE_SPACE`),
otherwise the harmless line is recorded. `export K=V` is accepted by bash, zsh, sh and fish; a pane
shell that rejects it starts the agent without the key, which falls back to C1 evidence, not an
error. The variables stay in the pane shell afterwards, so a manual relaunch in that pane presents
the same key, which is the same identity. The existing `HERDR_GRAPH_CLONE` pane-creation env stays
for panes the graph creates. The
holder of a reservation is the graph's registered threads service; a key can be reserved before
the clone has a seat, so the graph reserves at clone creation, before the first launch. Clones that
predate D1 are backfilled and linked to their current seat through a seat-bearing
`ReserveContinuityKey{key, seat?}` (requested from threads 2026-11-05; fallback: one keyed relaunch
per clone). The graph calls `ReserveContinuityKey` (idempotent for the holder); that call and its port wrapper are gated on D1 (§10). The key variable is emitted only for a
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
§8), **unless** the clone carries a `Restore` intent whose threads step is not yet acknowledged
(§8): then the Retired seat is the expected interim state of the restore, and the arm changes
nothing. Once that step is acknowledged, a Retired seat is a fresh close and retires the clone
(dropping the intent). `KeyConflict` → Unknown plus a key-conflict alert in `status`. The alert is
a level, not an event: it is the conflict field of the clone's last successful read, stored on the
observation state the step commits (one per clone, so no dedup), set by a read that shows a
conflict, cleared by the next successful read that shows none, and held unchanged across a
`ThreadsError`. Nothing else reads it; launches are gated on the current SeatStatus, not on the
alert. On v3.0.0 every retired seat is final (threads retirement is irreversible until D2), and the
adapter reports it so until the D2 bead (hg-66u.31) maps `retirement_state`; by bead order (§16.6)
no key is bound to any seat before that bead lands, so every retirement is still final in fact. Threads unreachable → no binding changes (hold last state; Unknown only
after the existing staleness rule).

Before D1 the old matcher still runs, and precedence is per clone: threads *speaks* for a clone
only when its seat reads `Resolved`, `Retired` or `KeyConflict`. `Unresolved`, an unreachable
threads, and `threads_seat = None` do not speak, and the matcher's result for that clone stands
(binding or Unknown, as today). When threads speaks, its answer replaces the matcher's for that
clone. Pane uniqueness: one pane is bound to at most one clone per step; when a threads `Resolved`
answer and a matcher match claim the same pane for different clones, the threads-bound clone keeps
the pane and the other clone gets no binding this step (Unknown). After D1 the matcher's clone
passes are gone and `Unresolved` → Unknown is the only answer. Then delete: the clone passes of `observe/matcher.rs` (token, terminal, native
session), clone terminal/incarnation tracking used only by them, the clone parts of the planner's
`LiveIndex` (bound-id, rebind-adopted, journal `live_ref` re-matching; the create-nonce match for a
pane the graph is creating stays until its seat is resolved), `/seat` rebind proposals
(`bootstrap/resolve.rs:312-345`) and `clone rebind --pane`. Panes the graph did not create are
adopted only through the first bind of an unseated clone (existing env-based `/seat` resolution),
which resolves its seat once. The deletion is gated on D1 (§10): before D1, a fresh agent or bare
pane after cold restart has no evidence threads accepts, so deleting the matcher first would regress
those cases relative to today's operator rebind; the `ThreadsSeatView` reader and binding-from-
threads mapping land before D1 and run alongside the old matcher under the per-clone precedence
above.
Considered: keeping the matcher as a fallback (rejected: B1/B3 — two sources of truth is the bug).

### 7. Tabs and workspaces across incarnations

Recommended: replace the majority vote (`matcher.rs:200-286`) with preserved-id equality: a seat's
tab and a teamspace's workspace keep their bound Herdr ids across incarnations (Herdr preserves
them across restore and handoff); token first where present; an id that no longer exists in the
new incarnation → Unknown (no vote). Independent of threads releases. Considered: keeping the vote
as fallback (rejected: silent first-wins, the ambiguity the review flagged).

### 8. Undo and resurrection onto the same seat (D2)

Recommended: closing a clone's pane retires its threads seat (threads does it; the graph issues
nothing), and SeatObservation retires the clone (§6). Every graph-initiated threads lifecycle
command and the launch that follows a restore are derived from a persisted **seat intent** on the
clone, never from the bare pair (clone state, seat status):

`CloneRecord.seat_intent: Option<SeatIntent>`, with variants
- `Restore{seat, epoch, threads_done}`: written by the same commit that performs graph undo of the
  clone's retirement or seat resurrection (`seat` = the clone's `threads_seat`, `epoch` = that
  commit's operation id, `threads_done = false`). A later undo or resurrection overwrites it with a
  new epoch; an acknowledgement for an older epoch changes nothing.
- `Finalize{seat}`: written by the purge commit (the end of the clone's undo window) when the seat
  is reversibly retired. The purge keeps the record as a tombstone carrying only identity and the
  intent; the tombstone is removed when threads acknowledges. A clone whose seat is already final
  (every seat before D2) is purged at once, with no intent.

Only the threads executor advances or clears the intent, through ThreadsSeatAssignment, on threads'
acknowledgement. The restore runs:
1. The undo/resurrection commit makes the clone active with `Restore`; the plan path reopens its
   tab in the original workspace and creates the pane, writing it as the clone's binding (create
   nonce). Undo no longer adopts the caller's pane (`undo/adopt.rs`): a different pane has its own
   seat, which would break 1:1. While the threads step is pending, SeatObservation does not
   re-retire the clone (§6), and the ordinary start rule launches nothing for it.
2. Threads step, chosen from the intent and the seat's status:
   - seat Retired *reversible* (D2): `UnretireSeat{seat}`; the seat returns to unresolved with
     memberships and requirement episodes kept (B10). `SeatUnretired` sets `threads_done`.
   - seat Retired *final* (every seat before D2; an operator's final retire after it): seat
     succession (§2) on the reopened pane; the succession commit sets `Restore.seat` to the new
     seat and `threads_done`. Memberships are re-invited and accepted once.
   - seat reads `Resolved` on the clone's own live, bound pane, or `Unresolved` once that pane is
     bound (an acknowledgement lost to a crash): the restore-ack want (§16.4) sets `threads_done`
     without a command. A read on any other pane, or an `Unresolved` one before the pane is bound,
     waits.
3. The resume driver (§9) is the single launcher: it starts the agent in the reopened pane with
   the key (resuming the last session when the harness supports it). The key or C1 check-in
   reattaches the (unretired or succeeded) seat.
4. The intent clears when the seat reads `Resolved` under `Restore.seat` with an agent in the
   bound pane. After reattachment the graph re-checks its wants and releases any requirement it no
   longer wants.

No unretire want exists without a `Restore` intent, so an ordinary pane close is never reversed. A
threads refusal (`Rejected`) keeps the intent pending and visible in `status`; retiring the clone
again clears it. Threads unreachable leaves the intent untouched and issues nothing.

Finalize: the finalize want is derived from a `Finalize{seat}` tombstone, issues
`FinalizeRetirement{seat}` (which also releases the key), and is retried level-triggered until
`RetirementFinalized` or a read showing the seat final; then the tombstone is removed. The
reversible window has no threads timer: only this want or an operator's final retire ends it.
The unretire and finalize commands are gated on D2.
**Before D2 (interim; "before D2" means before the D2 bead hg-66u.31 lands, §16.6):** the same intent drives step 2's final branch for every restore: undo and
resurrection reopen the clone's tab in its original workspace, the restored clone's final-retired
seat is succeeded by the reopened pane's seat, the resume driver resumes the last session, and
memberships are re-invited and accepted once. The D2-gated work adds the reversible branch and the
finalize tombstone; succession remains only for operator-finalized seats.
Considered: true un-retire (rejected by the user in favour of reversible state, threads-side);
new seat on undo (rejected: breaks B1).

### 9. Resume driver after a headless cold restart (G4)

The resume driver is the sole launcher for clones in either trigger below (the planner's ordinary
start rule skips them, and skips any clone with a pending `Restore` intent); undo and resurrection
of a seated clone and the fresh-agent path launch through it (a seatless undo launches through the
ordinary start rule, §16.3). "C1" below is threads' existing same-native-session
continuity claim: a resumed session's check-in reattaches the seat it last held.

Recommended: a reconciler step with two triggers, each requiring the clone's bound (or last bound)
pane id to exist in the current incarnation holding a bare shell (no agent, shell foreground via
`process_info`):
- **restart**: the clone's `threads_seat` reads `Unresolved`, or (after D1) the clone has
  `threads_seat = None` and a recorded key reservation (§2: the keyed check-in this launch causes is
  what lets its seat be resolved). After the existing 90 s incarnation grace. The `Unresolved` fact
  must have been read after the last incarnation change (§16.8). A clone with a pending `Restore`
  is covered only when its restore launch was marked before the last incarnation change (§16.7).
- **restore**: the clone carries a `Restore` intent with `threads_done` (§8). The pane is the one
  the restore reopened; its seat may read `Unresolved` (un-retired) or `Resolved` with a bare shell
  (succeeded), and either way this rule, not the ordinary start rule, launches. No incarnation
  grace: the pane is new. Once the keyed-launch bead (hg-66u.44) lands, the restore trigger also
  requires the clone's reservation to be recorded as bound to `Restore.seat` (§16.1).

In both, start the agent in that pane: resume the clone's last ended session when the harness
supports it, else fresh; always with the key. The pane
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
only a pin bump when released, folded into the D1 gated bead. D1 and D2 ship in one threads
release, so the bead order, not the gate order, decides when reversible retirement can appear:
no graph bead that binds a key to a seat (keyed env, seat-bearing reserve) lands before the D2
bead hg-66u.31 (§16.6). If gates are still closed when the
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

### 13. Clarifications from the amendment's coverage round (coverage-amend-design-1)

- **Clearing a Restore intent on re-retirement.** When a clone with a Restore intent whose threads
  step is acknowledged is retired again (a fresh pane close), the retire commit clears the intent
  through ThreadsSeatAssignment. SeatObservation only decides to retire; it never writes the intent.
- **No early resolve during a restore.** While a Restore intent is pending, no ResolveSeat is issued
  for the clone except by the succession want, so the reopened pane never gets its own seat ahead of
  un-retire (§2's early-resolve rule).
- **Key relink on succession.** After succession, a clone with a recorded key reservation has its
  key bound to the successor seat (`ReserveContinuityKey{key, seat: successor}`, issued by the
  key-link want, §16.1) before the restore launch. Threads deletes a seat's key row whenever its retirement becomes final, so the relink is a fresh
  reservation on the successor; the successor is resolved before the session resumes, and a resumed
  check-in on the successor's pane attaches to it (threads answer mnSGq1eyidhP).
- **Key release at purge.** Threads already deletes the key of a final-retired seat. Purge releases
  the reservation (`ReleaseContinuityKey`) of a clone that never got a seat; a reversibly retired
  seat releases through `FinalizeRetirement` from the tombstone. D1 and D2 ship in one threads
  release, so their gates close together; the graph beads that bind a key to a seat still land
  after the D2 bead hg-66u.31 (§16.6).
- **Launch ownership.** The ordinary start rule launches a clone with no pending `Restore` whose
  current pane has no recorded launch: a clone that has never had an agent (including a new post-D1
  clone with a reservation and no seat), and a pane the graph has just created for a clone without a
  Restore intent (a seatless undo or resurrection, §16.3); like every start it resumes the last
  session when the harness supports it. The resume driver launches only on its restart and restore
  triggers. The unseated restart arm fires only for a clone that had an agent before the
  incarnation change. A restore gets one restore-trigger launch per Restore epoch; a failed attempt
  is surfaced in status, not retried in the same incarnation (§16.7).
- **Shell export failure is visible.** A pane shell that rejects the export is named in status with
  its remedy; (withdrawn by §14.10: a graph retire always closes the pane).
- **Tombstones are inert.** A Finalize tombstone is not planned, listed, undone, resurrected or
  invited; it exists only to drive the finalize want.

### 14. Punch-list resolutions (design roast round 2, converged)

Each item below resolves a sub-Blocking finding of `2026-11-05-threads-seat-identity-roast-design-2.md`
and supersedes any conflicting sentence earlier in this spec.

1. **Purge exists as an explicit operator command.** The graph has no purge today. A new
   `herdr-graph clone purge <clone>` (plan/confirm like other mutations) ends a retired clone's undo
   window; there is no automatic retention timer. Until a clone is purged its reversibly retired
   seat keeps its memberships and key (accepted cost; `status` lists retired-unpurged clones).
2. **Purge always writes a tombstone after D2; before D2 it never does** ("D2" here is the D2 bead
   hg-66u.31 landing, §16.6). After D2, purge of any
   clone that holds a `threads_seat` writes `Finalize{seat}` regardless of the seat's retirement
   kind; the finalize want treats a read showing the seat final as acknowledgement. Purge never needs
   a live threads read. Before D2 every seat is final and no finalize want or `TombstoneDone`
   admitter exists, so a tombstone would never drain: purge deletes the clone record outright, seated
   or not (§8, "purged at once, with no intent"), and `purge_tombstone` returns no `Finalize` intent.
   The `Finalize` tombstone write lands in the same D2-gated bead as the finalize want and the
   `TombstoneDone` admitter (hg-66u.31), and that bead audits every clone reader for tombstone
   filtering (SeatResurrect `revived`/`revives_clone`, the teamspace-resurrect loop, grammar
   `resolve_clone(Scope::Retired)`, and the existing `is_tombstone()` sites). The existing
   `is_tombstone()` filters stay. Undo already treats a missing record as purged.
3. **Purge of a clone that never got a seat** writes a `ReleaseKey{key}` tombstone when a reservation
   is recorded. Its want first asks threads for the key's binding (the `ReserveContinuityKey` answer
   carries `seat`): a bound seat turns the tombstone into `Finalize{seat}`, an unbound key is released
   with `ReleaseContinuityKey`. `SeatIntent` therefore has three variants: `Restore`, `Finalize`,
   `ReleaseKey`.
4. **Key relink is a want, and it gates the restore launch** (as amended by §16.1). Succession
   leaves the clone's reservation record in place; its recorded binding still names the predecessor
   seat (threads deleted that key row at final retirement), so it is not bound to the new
   `threads_seat` and the key-link want (item 12) fires with `seat: threads_seat`, which equals
   `Restore.seat` after succession. Once the keyed-launch bead lands, the restore trigger fires
   only when the clone's reservation is recorded as bound to `Restore.seat`; there is no
   "no reservation" escape. Before that bead no clone has a reservation and the trigger has no key
   condition.
5. **A seatless clone gets no Restore intent.** Undo or resurrection of a clone with
   `threads_seat = None` (retired before the upgrade) writes no intent; the reopened pane follows the
   ordinary resolve want and the ordinary start rule, which launches into the pane the graph just
   created for it and resumes the last session where the harness supports it (§13, §16.3).
6. **Who writes the intent.** Only ThreadsSeatAssignment writes `seat_intent`, through four
   transitions: the undo/resurrection/purge helpers write it; the threads executor advances it on
   acknowledgement (`threads_done`); a **clear-on-reattach want** (Restore with `threads_done`, seat
   Resolved under `Restore.seat`, agent in the bound pane) clears it; the retire commit clears it on
   re-retirement.
7. **One launch per restore epoch is persisted.** `Restore` gains `launched`, the time the launch
   was marked (absent: not launched; §16.7). The Herdr executor sets it through
   ThreadsSeatAssignment in the commit that records the `StartAgent` dispatch, before dispatching
   (at most once per epoch). The restore trigger requires `launched` absent. `status`
   (`stuck_restores`) names every pending Restore intent that cannot make progress, each with a
   reason and a remedy, not only launched ones:
   - launched, no reattach: `launched` set and the clear-on-reattach want has not cleared it;
   - mark failed: the `MarkLaunched` admission returned a writer error on an unchanged intent. The
     executor returns `Failed` with the writer error (logged and marked for attention like any
     failed effect), never `Obsolete`; only a stale or changed epoch is `Obsolete`;
   - not launchable: `threads_done = true`, `launched` absent, and the clone has no resumable
     session. Before D1 the restore launches only by session (§9), so the remedy names a manual
     start in the reopened pane; the fresh-agent launch stays with the D1 bead (hg-66u.21).
   A Restore whose threads step is still unacknowledged is shown with its last threads refusal, if
   any (§8). Retiring the clone again clears the intent and its entry.
8. **The restart arm skips clones with a pending Restore intent**, so a failed restore is never
   relaunched through the restart trigger in the incarnation it was launched in. The one exception
   (§16.7): a Restore whose `launched` predates the last incarnation change is covered by the
   restart arm like any seated clone.
9. **SeatObservation commits the seat fact.** Besides availability and binding, SeatObservation
   commits a per-clone `seat_fact` (Resolved | Unresolved | KeyConflict | Retired{reversible|final},
   with the read time) in observation state. SeatStatus still has one mapping (in SeatObservation);
   the planner and `ThreadsSource::system_wants` read the committed `seat_fact`, never the wire
   result. The restart trigger requires `seat_fact = Unresolved` (so KeyConflict never launches)
   read after the last incarnation change (§16.8), and the unretire, succession and finalize wants
   branch on the committed Retired kind.
10. **A graph retire always closes the clone's pane** (the reconciler closes it; hg-66u.8 verifies and,
    if needed, makes it so). The state "retired clone with its pane open" therefore does not exist, and
    §13's "unset the key in that shell" clause is withdrawn.
11. **Participation want existence.** A clone's desired invitations exist while it is active, or
    retired and not purged, with `threads_seat` known; they end at purge (the finalize want ends the
    seat's memberships threads-side). A retired clone's want only keeps links (no release); it
    invites nothing, so a final-retired seat is never re-invited (§16.2).
12. **Reversibility needs a bound key, and the graph binds it.** After D1/D2 a pane close is
    reversible only for a seat whose key is bound. The graph does not rely on a keyed check-in to
    bind it: a **key-link want** fires for every active clone with a recorded reservation and a
    `threads_seat` whose reservation is not recorded as bound to that seat, and issues the
    seat-bearing `ReserveContinuityKey{key, seat: threads_seat}` (idempotent for the holder); the
    answer's binding is recorded on the reservation. This one want covers pre-D1 backfill (§5),
    the post-succession relink (item 4) and a clone whose shell rejected the export, so a post-D1
    clone does not stay keyless. Only a seat still unlinked at close time (threads unreachable or
    refusing since the seat was resolved) closes final, and its undo goes through succession with
    one re-acceptance. The want is D1-gated (hg-66u.68, after the reservation bead hg-66u.43 and
    the D2 bead hg-66u.31, §16.6) and is the only key relink (§16.1). Fakes in hg-66u.32
    model a keyless pane close as final.

### 16. Fixes from the amend design roast (round 3)

Each item resolves a confirmed finding of `2026-11-05-threads-seat-identity-roast-design-3.md` and
supersedes any conflicting sentence earlier in this spec, §14 included.

1. **One relink mechanism, and the gate cannot open on the succession commit.** The key-link want
   (§14.12) is the only relink; the reserve-key want stays seatless and fires only for a clone with
   no recorded reservation. Succession does not clear the reservation: the record stays, and its
   recorded binding names the predecessor, which no longer equals `threads_seat`, so the key-link
   want issues `ReserveContinuityKey{key, seat: threads_seat}` (= `Restore.seat`), a fresh
   reservation on the successor. From the keyed-launch bead (hg-66u.44) on, the restore trigger
   requires the reservation recorded as bound to `Restore.seat`; a restored clone with no recorded
   reservation waits for the reserve-key want and then the key-link want. Before that bead no clone
   has a reservation and the restore trigger has no key condition (delivery order, no switch).
   For a reversible un-retire the reservation is already bound to `Restore.seat`, so the gate is
   open at once.
2. **A retired clone's want keeps links and invites nothing.** While a clone is retired and not
   purged its desired invitations still exist, so links that satisfy them are not released; but
   the invite executor issues invitations only for an active clone. §3's re-invite of a Released or
   Retired row on the recorded seat applies to an active clone only, so a final-retired seat whose
   memberships threads ended is not re-invited before purge. After undo the clone is active again
   and its wants behave as for any active clone (kept links after un-retire, re-invites on a
   successor).
3. **A seatless undo launches through the ordinary start rule.** The pane the graph creates for an
   undone or resurrected seatless clone clears the clone's recorded launch (the existing
   clear-on-new-pane rule of the Herdr executor), so the ordinary start rule fires for it like for
   a clone that never had an agent, and the start resumes the clone's last session when the harness
   supports it (B7). No resume-driver trigger is involved. §13 "Launch ownership" states this.
4. **Restore-ack want.** A want kind covers §8 step 2's lost-acknowledgement branch: for a clone
   carrying a `Restore` intent without `threads_done` whose committed `seat_fact` for
   `Restore.seat` reads `Resolved` on the clone's own verified (live, bound) pane, or `Unresolved`
   while the clone has a verified pane, the threads executor sets `threads_done`
   through ThreadsSeatAssignment without a threads command. Because the undo/resurrection commit
   drops the clone's `seat_fact`, such a fact can only come from a read after that commit.
5. **Reserve-key and key-link do not overlap.** Reserve-key: an active clone with no recorded
   reservation, seatless `ReserveContinuityKey{key}`, never a `seat` argument. Key-link: an active
   clone with a recorded reservation not recorded as bound to its `threads_seat`, seat-bearing
   `ReserveContinuityKey{key, seat: threads_seat}`; it covers pre-D1 backfill, the post-succession
   relink and an export-rejected shell. §14.4's earlier "reserve-key want then fires with
   `seat: Restore.seat`" is withdrawn (item 1).
6. **Bead order keeps the pre-D2 rules true.** D1 and D2 ship in one threads release, and threads
   retires a seat reversibly only when a key is bound to it. The reservation bead (hg-66u.43, which
   also bumps the pin) records reservations with the seatless `ReserveContinuityKey{key}` only and
   binds no key to a seat. Every bead that binds one (the keyed env and pre-D1 backfill link of
   hg-66u.44, the key-link want of hg-66u.68, and their dependants) lands after the D2 bead
   hg-66u.31 (reversible/final mapping, `Finalize` tombstone, unretire and finalize wants). "Before
   D2" in §6, §8 and §14.2 means before hg-66u.31 lands; until then every retirement is final in
   fact, so the adapter's all-final mapping, outright purge and interim succession stay correct.
7. **A Herdr restart after a restore launch relaunches through the restart arm.** `Restore.launched`
   records the time of the `MarkLaunched` mark. A Restore with `threads_done` whose `launched`
   predates the last incarnation change is covered by the restart arm (seat fact `Unresolved` read
   after the change, bare pane, 90 s grace, once per incarnation) like any seated clone; the restore
   trigger itself still launches at most once per epoch, and within one incarnation a failed
   restore launch is never retried. The clear-on-reattach want then clears the intent as usual.
8. **Seat-fact freshness for launches.** SeatObservation rewrites a clone's `seat_fact` on the first
   successful read after an incarnation change, so its read time is at or after the change. The
   restart trigger acts only on a fact read after the last incarnation change: a fact held across a
   threads outage that spans the restart launches nothing. The Retired-kind wants need no freshness
   rule (each issues a threads command that an unreachable threads cannot accept), and the
   restore-ack want reads only facts taken after the restore commit (item 4).

## Key abstractions

**SeatStatus** — new enum (graph-local value type beside the threads port types), what threads says about one clone's seat.
- Owns: the closed set of seat states the graph acts on: `Resolved` (with target pane), `Unresolved`, `Retired` (reversible or final) and `KeyConflict`.
- Hides: the threads v3 wire shapes of `SeatInspect`/`Seats`. The adapter maps them to these variants and nothing else sees them.
- Must not: carry graph decisions such as availability or retire requests. SeatObservation makes those.
- Relationships: produced by ThreadsSeatView and mapped by exactly one `match` in SeatObservation, which commits the result as the per-clone `seat_fact` (§14.9). The planner and `ThreadsSource::system_wants` read the committed `seat_fact`, never the wire result.
- Errors: none. Every seat state, including a key conflict, is a value. Only transport problems are errors, and those belong to ThreadsSeatView.

**ThreadsSeatView** — new port trait, a read-only view of threads seat state for a set of seats (§6).
- Owns: inspecting the `threads_seat` of every clone that has one, once per observation step.
- Hides: whether the reads are batched (`Seats`) or per seat (`SeatInspect`), the host qualification, and the socket client.
- Must not: write to threads (no resolve, reserve or unretire; PaneSeatMap does those), commit graph state (SeatObservation decides and the step commits), or fabricate a state when threads is unreachable.
- Relationships: a port in `src/ports/`, like the others, with a real implementation (the same struct as `ThreadsSeatMap`, sharing its client, host and clock) and a fake (§11). `RuntimeLoop` holds an `Arc<dyn ThreadsSeatView>`. The dependency runs from observe to the port, never the reverse.
- Lifecycle: built once in daemon compose next to `ThreadsSeatMap`, shared by `Arc`, lives as long as the process; no state apart from the client connection.
- Errors: `ThreadsError`. Callers tell `Disconnected`/`ServiceBusy` (hold the last state; Unknown only under the existing staleness rule) apart from `Rejected`/`Unsupported` (log it and change nothing). A refusal that concerns one seat (an unparseable seat id, a per-seat `Rejected` answer such as NotFound for a seat id kept across `ThreadsReconnect`) is a per-seat outcome, not a whole-read error: the read returns the statuses of the other seats and omits that seat, so threads does not speak for that clone this step (its last state holds, as when threads is unreachable) while every other clone keeps threads authority. The per-seat refusal is logged with the clone and seat ids and kept in the `threads` status component. Only a transport error, `Unsupported`, or a refusal of the whole call fails the read.

**SeatObservation** — new module in `observe/`, turns seat statuses into observation facts for one step.
- Owns: the single `match` on SeatStatus. `Resolved` binds the clone to threads' pane and marks it Present (the existing stamp path re-stamps the token). `Unresolved` marks it Unknown. `Retired` (either kind) retires the clone through the existing undoable retirement, except while the clone carries a `Restore` intent whose threads step is unacknowledged (no change: the expected interim state, §8); the reversible/final distinction matters only to the threads step. `KeyConflict` marks it Unknown and sets the clone's key-conflict alert. It also owns pre-D1 precedence (§6): threads speaks for a clone only with `Resolved`, `Retired` or `KeyConflict`; otherwise the old matcher's result stands; a pane claimed by a threads `Resolved` clone is withheld from any matcher-bound clone that step. It also commits the per-clone `seat_fact` (Resolved | Unresolved | KeyConflict | Retired{reversible|final}) that the planner and wants read (§14.9).
- Hides: how threads' answer is merged into `Matches`/`build_ops`. The binding, retire and availability request builders stay as they are.
- Must not: call threads (it receives statuses already read), touch tabs or workspaces (matcher, preserved-id equality, §7), or write or clear a seat intent.
- Relationships: a function the step calls after `match_snapshot`; takes SeatStatus values and each clone's seat intent and yields entries for `Matches.clone_pane` plus ops.
- Lifecycle: stateless functions. The key-conflict alert is per-clone observation state committed by the step: set by a successful read showing a conflict, cleared by the next successful read showing none, held across `ThreadsError` (§6).

**ThreadsSeatAssignment** — new bookkeeping mutation (`bookkeeping.threads_seat`, registered beside `bookkeeping.binding`), the only writer of `CloneRecord.threads_seat` and the only mutation that advances or clears `CloneRecord.seat_intent`.
- Owns: the write-once rule, its one exception (seat succession of a final-retired seat, admitted only against a `Restore` intent with matching seat and epoch, §2), the invariant that at most one non-retired clone holds a given `threads_seat`, and the intent transitions after threads acknowledges: `threads_done` on `SeatUnretired`, succession or the command-free restore-ack want (§16.4), clearing `Restore` on reattachment, removing a `Finalize` tombstone on `RetirementFinalized` (§8). An acknowledgement for a stale epoch is an idempotent no-op. Its intent transitions (§14.6-7): write (undo/resurrection/purge helpers), acknowledge (`threads_done`), mark `launched` with the mark time (Herdr executor, before dispatch, §16.7), clear on reattach (clear-on-reattach want) and clear on re-retirement (retire commit).
- Hides: the overwrite and uniqueness checks against the committed tree.
- Must not: resolve seats (the threads executor does that through PaneSeatMap), retire anything, or create an intent (undo, resurrection and purge commits write it).
- Relationships: one more entry in the existing `MutationRegistry`, the same shape as `bookkeeping.binding`; the threads executor admits it.
- Errors: `MutationError`. Writing the seat already set is an idempotent success, distinct from a different seat (rejected) and from a seat another live clone holds (rejected, surfaced, never retried).

**CloneRecord** — extended: gains `threads_seat` (write-once, §2) and `seat_intent: Option<SeatIntent>` (`Restore{seat, epoch, threads_done, launched}` with `launched` the optional mark time, `Finalize{seat}` or `ReleaseKey{key}`, §8, §14). After D2, a purged clone that holds a seat (or a recorded reservation) stays as an inert tombstone carrying only its identity and the intent until threads acknowledges (§14.2-3); before D2 purge deletes the record outright and no tombstone exists. Observation state gains the per-clone `seat_fact` that SeatObservation commits (§14.9). `ThreadsLink.occupant` becomes attribution only, and `SetInvitation`'s `expect_occupant` guard becomes `expect_seat` (§3).

**PaneSeatMap** — extended: becomes the port for seat-lifecycle commands. It already has resolve; it gains reserving the continuity key (D1), unretiring a seat (D2) and finalizing a retirement or releasing the key (D2, per the published API). Its resolve reports threads' refusals (restore hold, key-conflict hold) as a non-error "not yet" so the executor returns `Deferred` (§2). Implementations stay `ThreadsSeatMap` and `FakePaneSeatMap`.

**ThreadsSource / ThreadsExecutor (`Payload`)** — extended: new want kinds, still chosen in `system_wants` and dispatched in `execute_scoped`:
- resolve seat: for an active clone with no `threads_seat` and either a verified pane binding (`ResolveSeat`) or, after D1, a recorded key reservation (re-issued `ReserveContinuityKey{key}`, admitting the answer's `seat` when present, §2); it admits ThreadsSeatAssignment.
- reserve key: for an active clone with no recorded reservation, before its first keyed launch; seatless `ReserveContinuityKey{key}`, never a `seat` argument (§16.5).
- key link: for an active clone with a recorded reservation and a `threads_seat` whose reservation is not recorded as bound to that seat; issues `ReserveContinuityKey{key, seat: threads_seat}` and records the binding (§14.12). The only relink: after succession the recorded binding names the predecessor, so this want binds the key to the successor (§16.1). D1-gated, landing after the D2 bead (§16.6).
- unretire seat: only for a clone carrying a `Restore` intent without `threads_done` whose seat reads reversibly retired (§8). No other state produces it.
- succession: only for a clone carrying a `Restore` intent without `threads_done` whose seat reads final-retired; resolves the reopened pane's seat and admits ThreadsSeatAssignment's succession (§2, §8).
- restore ack: only for a clone carrying a `Restore` intent without `threads_done` whose committed `seat_fact` for `Restore.seat` reads `Resolved` on the clone's own verified pane or `Unresolved` with a verified pane (an acknowledgement lost to a crash); sets `threads_done` through ThreadsSeatAssignment with no threads command (§8 step 2, §16.4).
- finalize retirement: for every `Finalize{seat}` tombstone, retried until threads acknowledges (§8).

`clone_wants` keys desired, satisfied and released on `threads_seat` instead of the occupant (§3). The invite executor invites the recorded seat and does not re-resolve it, and invites only for an active clone: a retired, unpurged clone's want keeps its links but issues no invitation (§16.2).

**StartAgent / HerdrApi adapter** — extended: `StartAgent` gains `env`. The Herdr client adapter alone delivers it: a space-prefixed `export K=V …` line plus Enter typed into the pane's shell through `pane.send_text`/`pane.send_keys`, then `agent.start`, whose command the same shell runs and whose environment the agent inherits (§5). It refuses values outside `[A-Za-z0-9_.-]` rather than quoting them. Callers never see the mechanism.

**`model::launch`** — extended: gains the continuity-key env constant and one function that builds the env for every agent start (`HERDR_GRAPH_CLONE` plus `HERDR_THREADS_CONTINUITY_KEY=<cl_ id>`). Every start site calls it: `HerdrExecutor` start/relaunch, `session::start` for ReplaceSession, and the resume rule. No site builds the env by hand.

**Reconcile planner (`plan_effects`, `LiveIndex`)** — extended:
- A resume rule plans the existing `StartAgent` effect for a clone in either §9 trigger: restart (seat fact Unresolved read after the last incarnation change, or after D1 `threads_seat = None` with a recorded reservation; no pending `Restore` unless its `launched` predates that change, §16.7-8) or restore (`Restore` intent with `threads_done` and `launched` absent; from hg-66u.44 on, its reservation recorded as bound to `Restore.seat`, §16.1); in both its bound or last bound pane id exists in this incarnation and that pane has no agent. The executor already applies the 90 s grace (restart trigger only), checks for a shell foreground and chooses resume over fresh. The ordinary start rule skips every clone the resume rule covers and every clone with a pending `Restore` intent; it launches a clone whose current pane has no recorded launch, which includes the new pane of a seatless undo (§16.3). A clone whose seat reads `KeyConflict` matches neither trigger, so the rule needs no alert check.
- The clone side of `LiveIndex` shrinks to a lookup by committed binding plus the create-nonce match. Rebind adoption and `live_ref` re-matching are removed after D1 (§6).

**Matcher** — extended (narrowed): tabs and workspaces match by token, else by preserved-id equality, and become Unknown when the id is absent (§7). The clone passes are deleted once D1 lands (§6).

Variation: threads seat states (G5 limbo is pending) → one closed enum, SeatStatus, matched only in SeatObservation. Threads seat commands (resolve, reserve, unretire, finalize) → fixed operations on PaneSeatMap, with effect kinds chosen in `ThreadsSource::system_wants`. Agent start sites → one env builder in `model::launch`. How env reaches Herdr → hidden in the Herdr client adapter. Release gates are delivery order (gate beads), not runtime flags: before D1 the code simply has no reserve want, with no capability switch.

Walk-through, cold restart headless: the new incarnation forces a rebind step; `RuntimeLoop` snapshots Herdr and asks ThreadsSeatView about each clone's `threads_seat`; threads answers `Unresolved`, SeatObservation marks the clone Unknown; the planner's resume rule sees the last bound pane present with no agent and plans `StartAgent`; the executor waits out the grace, confirms the shell foreground and starts the agent with the `model::launch` env and `--resume` where supported; threads reattaches the seat by C1 or D1 evidence; the next step reads `Resolved{pane}`, SeatObservation writes the binding and marks the clone Present; `clone_wants` finds `link.seat == threads_seat` satisfied, so nothing is re-invited.

Future change, G5 limbo disposition: one new SeatStatus variant, mapped in the adapter, and one new arm in SeatObservation's `match`. No caller changes.

Spec-forced exceptions:
- Per §6: before D1, threads and the old clone matcher run side by side under per-clone precedence (threads speaks only with Resolved, Retired or KeyConflict; a threads-claimed pane is withheld from matcher-bound clones). Deliberate and temporary; the matcher's clone passes are deleted in the D1-gated bead.
- Per §8: undo no longer adopts the caller's pane (`undo/adopt.rs` removed for clones). Undo and resurrection commit the clone active together with a `Restore` intent; unretire or succession, then the resume driver's launch, follow from wants derived from that intent. The undo code makes no threads calls. Purge of a reversibly retired seat's clone leaves a `Finalize` tombstone instead of deleting the record.
- Per §3: `ThreadsLink.occupant` stays on the record, but nothing may branch on it any more.
- Per §5: the key env var is emitted only once the clone's key reservation is recorded (none before D1).

Digest:
- SeatStatus: owns the closed set of threads seat states; a new state is added as one variant plus one arm in SeatObservation; callers never inspect threads wire results.
- ThreadsSeatView: owns read-only seat inspection per observation step; new reads are added on this port and its fake; callers never write to threads through it or invent a state when threads is unreachable.
- SeatObservation: owns turning seat statuses into clone binding, availability, retirement, the committed per-clone seat_fact and the key-conflict alert level, plus pre-D1 precedence over the old matcher; it reads the seat intent but never writes it; new seat-state handling is added in its single match; callers never map seat states to graph changes elsewhere.
- ThreadsSeatAssignment: owns the write-once `threads_seat`, its intent-gated seat-succession exception, its one-live-clone-per-seat invariant and the acknowledgement transitions of `seat_intent` (including the command-free restore ack and the timed launch mark); new rules on the field are added in this mutation; callers never write `threads_seat` directly, clear an intent, or retry a rejected different seat.

## Unchanged behavior

- An ordinary pane close retires the clone through the existing undoable retirement on the next observation step; nothing un-retires it without a graph undo or resurrection.
- `threads_seat` is write-once except for seat succession of a final-retired seat; at most one non-retired clone holds a seat (B1 1:1).
- There is no separate "retired-final" clone state; graph clone retirement stays undoable until purge.
- Participation is keyed to `threads_seat`, survives agent exit with the pane open (B8), and is not released on clone retirement.
- The key env var is emitted only for a clone whose reservation is recorded; no key is emitted before D1.
- `HERDR_GRAPH_CLONE` is still set on every agent start and on panes the graph creates.
- The resume driver never launches into a pane with an agent, applies the 90 s incarnation grace after a restart, and is the only launcher for the clones it covers.
- Threads unreachable changes no binding, writes no seat or intent, and launches nothing that depends on a threads answer not read since the last incarnation change (§16.8).
- Gating stays delivery order: the matcher deletion and fresh-agent path wait on D1, unretire and finalize on D2; no runtime capability switches.
- Undo never adopts the caller's pane.
- Tabs and workspaces match by token, then preserved Herdr id (§7), unaffected by this amendment.
- (amend-code-1) Purge stays an explicit operator command that ends the undo window; undo of an action whose retired clone was purged is still refused, and `seat resurrect` never revives a purged clone.
- (amend-code-1) A restore still gets at most one restore-trigger launch per Restore epoch (`MarkLaunched` before dispatch), and within one Herdr incarnation nothing relaunches it; only a later incarnation change hands it to the restart arm (§16.7). Surfacing a stuck restore in status never retries or relaunches it, and no fresh-agent launch exists before D1.
- (amend-code-1) Seat-keyed participation migration still causes no churn for Pending or Accepted links on the backfilled seat.
- (amend-code-1) A whole-read `ThreadsSeatView` failure (transport error, `Unsupported`, refusal of the whole call) still changes no binding.
- (amend-code-1) No key reservation, key-link want or keyed env exists before D1.

## Post-Implementation Notes

*As this design is implemented and iterated on — bug fixes, adjustments, anything that diverged from the assumptions above — append a dated note here, whether or not a formal debugging skill was used.*

- 2026-11-06 (fix loop round 2, hg-66u.80): reactivating a Dormant seat or teamspace is treated like resurrection — the activation commit writes a Restore intent for each seated clone (pre-D2 succession, post-D2 un-retire); SeatObservation does not inspect clones of a Dormant seat/teamspace, so deactivation never retires them.
- 2026-11-06 (fix loop round 3, hg-66u.81): every Restore writer (resurrection, undo, seat reactivation) commits through one helper that clears the old pane's occupant and runtime and then writes the Restore intent (a seated clone always gets the intent; skipping it would let the Retired arm retire the clone). Decision (orchestrator): an acknowledged Restore with no resumable session for the clone's current harness is launched fresh by the resume driver (pre-D1 without a key, matching main's reactivation behaviour); this supersedes §14.7's "not launchable" rule and, for restores only, the "no fresh-agent launch exists before D1" line under Unchanged behavior. The restart trigger still needs a resumable session before D1 (hg-66u.21). restore_stuck not_launchable applies only when no agent can be launched at all, which pre-D1 is never the case for an agent clone, so the planner no longer writes it.

- 2026-11-07 (hg-66u.14, human decision S1): the matcher deletion keeps one fallback: a clone with no threads seat is located by preserved Herdr pane id (same rule as tabs/workspaces, hg-66u.11); every clone with a threads seat is located only by SeatObservation.
- 2026-11-07 (merge of main, seat failure and manager seats): the node manager is never retired by an observed closure, so when its pane closes the cascade keeps its clone and writes a Restore intent for its threads seat (`write_restore_intent`, as undo does). Without it a Retired seat reading would close the reopened clone again on every step, because write-once forbids taking the reopened pane's seat without a Restore. See `merge-main-2026-11-07.md`.
- 2026-11-07 (hg-66u.98): the node manager's cascade-written Restore can precede threads' retirement of its seat, so a `Resolved` or `Unresolved` read taken then must not acknowledge it. The restore-ack want now requires the clone's own verified pane (`Resolved` on that pane, or `Unresolved` once it is bound); otherwise it waits for the `Retired` read, which takes the succession path once. A further hold (a `Retired` read against an acknowledged, unlaunched Restore whose pane is live re-runs the threads step instead of retiring) was considered and not built: with the tightened ack, an `Unresolved` read marks the clone Unknown, so the acknowledgement cannot fire before the `Retired` read in either event order (pinned by `seated_node_manager_stale_unresolved_outlives_the_reopen_then_retired_succeeds_once`).
