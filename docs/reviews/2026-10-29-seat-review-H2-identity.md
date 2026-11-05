# Proposal — Align graph clone identity with the herdr-threads seat (re-review of H2)

Repo state: `main` @ c77d9ad, vendored herdr-threads 2.6.4. This was a read-only review. Supersedes the
recommendation in `herdr-graph-seat-review-H2.md`; that file's option analysis still holds for the
remaining first-acceptance path.

## Abstract

herdr-graph and herdr-threads already issue identity at the same grain. A graph *clone* is one Herdr
pane, and a threads *seat* is one Herdr pane. Both survive `/clear`, a new session, a resume and a
workspace move, and both retire on pane close. The integration does not use this. Graph keys every
required membership to its *occupant*, the native session record `ns_`. Each `/clear` or relaunch
therefore releases the requirement and re-issues a fresh pending episode on the **same** threads seat.
The new agent must run `accept-required` again, and herdr-graph's own `/seat` may give it a stale
command or a read-only lookup to do so (H2). This contradicts the threads contract, which assigns
membership to the durable participant and explicitly excludes re-invitation per native session. The
proposal: key thread participation to the clone, record the clone's threads seat as its threads
identity, and re-invite only when that seat actually changes. Required memberships then carry across
sessions automatically, as threads intends. H2 shrinks from a per-session failure to a one-time,
first-acceptance step per pane. That step still needs the agent's own explicit acceptance, which no
identity scheme can remove.

## What the code says today (verified)

| Fact | Evidence |
|---|---|
| Graph identity grain: seat = tab, clone = pane, native sessions occupy clones | `CURRENT-DESIGN.md` "Identity and scope"; `src/model/clone.rs:8-31` |
| Threads identity grain: seat = Herdr pane (host + target + terminal proof), random daemon-minted id | `third_party/herdr-threads/migrations/node/0001_node_baseline.sql:414-455`; `src/store/seats.rs:3211-3234` |
| Threads seat survives `/clear`, a new session and a pane move; retires permanently on pane close | `third_party/herdr-threads/TRUST-POLICY.md:218-262, 652-654` |
| Threads: membership belongs to the participant, not the session; "Requiring a new invitation on every native session is outside this extension"; predecessor acceptance is not liveness proof | `third_party/herdr-threads/docs/design/graph-system-identity/design.md:53` |
| Graph: required invitation is "satisfied" only for the `ns_` it was issued for | `src/threads/effects.rs:383-390` |
| Graph: requirement of a non-current occupant is released; Invite waits on that Release | `src/threads/effects.rs:401-439`, `After::Release` at `:255-257` |
| Graph: same pane and same threads seat with a new `ns_` gives release plus a fresh pending episode | test `occupant_replaced_in_one_commit_releases_before_reinviting_the_same_seat`, `src/threads/tests.rs:740-763` |
| Graph: agent exits with pane kept releases required memberships | test `occupancy_end_releases_requirement`, `src/threads/tests.rs:677`; spec §7.1 (`docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-design.md:200`) |
| Origin of occupant keying: roast r1 "key cleanup to the occupancy-ended / occupant-changed transitions" | `…/2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md:11` |
| Graph does not store the clone's threads seat; it re-resolves it with a fresh `ResolveSeat` per invite | `src/threads/mapping.rs:49-76`, `src/threads/effects.rs:1186-1197`; only `Invitation.link.seat` records it |
| Rebind gap: satisfaction ignores the seat, so after a pane change links can keep pointing at the old threads seat | `src/threads/effects.rs:383-390` (seat not compared) |
| Nothing outside `src/threads/` treats `Accepted` as liveness | grep of `InvitationState::Accepted` |
| Threads service cannot pre-allocate, name, move or pre-accept a seat; acceptance is always the seat's own `accept-required` at the current revision | `third_party/herdr-threads/src/protocol/service.rs:142-157`; `src/store/service_controls.rs:348-361, 507`; `src/store/control/invitation.rs:390-452` |
| Requirement revision moves on accept, release and retire only; re-invite of a current requirement is idempotent | `invitation.rs:451`, `service_controls.rs:364-370, 575`, `retirement.rs:375` |

**Correction to the framing:** the identity that can match is **graph clone ↔ threads seat**, not
graph seat ↔ threads seat. A graph seat (tab, possibly several clones, possibly dormant with no pane)
has no threads counterpart. Its threads-side continuity is the seat channel (a thread), plus Beads.
That is consistent with `DESIGN-NOTES.md:23` ("organizational continuity need not preserve the old
mailbox participant ID") and `:112` ("herdr-threads' pane-bound participant would correspond to a graph
clone").

## Proposal

**Principle:** graph does not invent a second continuity rule. A clone's threads identity is whatever
threads resolves for the clone's pane. Graph records it, follows it, and keys participation to it.

1. **Record the threads seat on the clone binding.**
   - Add `threads_seat: Option<String>` to `Binding` (`src/model/common.rs:98-111`).
   - Fill it with a resolve step after stamping (`src/reconcile/herdr_exec.rs:437-501`, or a new
     threads effect). It uses the existing `ThreadsSeatMap::seat_for`.
   - Clear it whenever the binding changes: observed move, `clone rebind`, or incarnation change
     (`src/observe/mutations.rs:548-578`, `src/plan/kinds_extra.rs:804-917`).
   - Want generation stays synchronous over committed state.
2. **Key desired membership to (clone, threads seat), not occupant.** In `clone_wants`
   (`src/threads/effects.rs:350-465`):
   - Desired when the clone is active and `threads_seat` is known (no occupant needed). Invitations
     are then waiting before the first agent's SessionStart, and herdr-threads' own hook shows the
     live `accept-required`.
   - Satisfied when `held.link.seat == binding.threads_seat`. For ordinary invitations, also require
     state Pending or Accepted.
   - Release when the clone is retired or `link.seat != binding.threads_seat`. Pane close needs no
     graph release, because threads retires the seat and its requirement itself.
   - `ThreadsLink.occupant` becomes attribution only: who was occupying when issued. The
     `expect_occupant` guard in `SetInvitation` (`src/threads/mod.rs:128-176`, `effects.rs:1050-1098`)
     becomes `expect_seat`.
3. **Invite executor.** Drop the `Obsolete` return for "no occupant" (`effects.rs:1187`). Invite the
   recorded `threads_seat`, deferring if it is unknown. Do not re-resolve inside the effect.
4. **`/seat` stops printing acceptance commands (H2 option D).** Report only `thread` and
   `constraint`. Skill step 5 then reads: required → use herdr-threads' ready `accept-required`. If it
   is trimmed, follow herdr-threads' required procedure (participants row → `accept-required`, reread
   on stale). Ordinary → decide by topic and goal.
5. **Design/spec updates.**
   - Spec §7.1 line 200 and `CURRENT-DESIGN.md` "Threads and policy": lifecycle cleanup is keyed to
     clone (threads seat) lifetime, not occupancy.
   - Record that acceptance is not liveness.
6. **Tests.**
   - Flip `occupant_replaced_in_one_commit…`: the episode stays Accepted with no new invite.
   - Flip `occupancy_end_releases_requirement`: no release while the pane lives.
   - Add: a new threads seat for the same clone after rebind or Herdr restart releases the old one and
     invites the new one.
   - Add: invitation issued before first occupancy.
7. **Migration.** Existing links already carry `seat`. Once `threads_seat` is resolved and equals it,
   they are satisfied: no churn, and no mass re-acceptance on upgrade.

Cost: moderate, about 1–1.5 days. The work is in `src/threads/effects.rs`, `src/threads/mod.rs`,
`src/model/{common,clone}.rs`, one resolve effect, observe/rebind clearing, `/seat` output and skill,
spec text, and about 6 tests. It does not touch `src/cli/seat.rs`.

## Concerns this does **not** dissolve (flagged)

1. **First acceptance per pane remains manual.** Threads forbids fabricated or service-side
   acceptance (design.md:36-40; no service operation accepts). Every new clone or pane still needs its
   agent to run `accept-required` once. H2's failure mode survives for that first time unless step 4
   lands. With step 4, the only source is threads' own live command and procedure.
2. **Graph-side moves have no threads-side continuity.** Graph can move a clone's identity to another
   pane: `clone rebind`, undo adopting the caller's pane, cross-tab moves, or a token/terminal/session
   match after a Herdr restart. A threads seat cannot follow it. Threads moves a seat only by
   structural Move, by C1 (same native session resuming onto an unresolved seat), or by an
   operator-only rebind of an unresolved seat. In those cases the clone gets a new threads seat, needs
   a new acceptance, and leaves its old mailbox, receipts and cursors behind. The proposal handles this
   correctly (re-invite and release), but it is not continuity.
3. **No graph-seat-level identity in threads.** Pane close retires the threads seat irrevocably. A
   graph seat that outlives its panes (dormant, resurrected, clone replaced) keeps continuity only
   through its seat channel history and Beads. Making a *graph seat* durable in threads would need
   threads features that do not exist: service-reserved seats, seat transfer, un-retire. The threads
   trust policy explicitly declines these on heuristic evidence (TRUST-POLICY.md:204-205).
4. **An empty pane keeps its required memberships.** This reverses spec §7.1 and roast r1: when an
   agent exits but the pane stays, the threads seat (and its memberships) stays a member and keeps
   accruing system notices. This matches threads' model and avoids churn, but it is a deliberate
   design change for you to approve. The alternative is to keep releasing on occupancy end, which
   reintroduces re-acceptance on every relaunch.
5. **Two views of the pane → seat mapping can disagree briefly.** After a Herdr restart, threads may
   hold the old seat unresolved under restore holds while graph has already rebound the clone. Graph
   must defer (`threads_seat` unknown) rather than resolve early. An early resolve could allocate a
   fresh seat and defeat threads' C1 reattachment. Proposal: resolve only when graph's binding is
   verified, and treat `ResolveSeat` refusals (holds) as Deferred, not errors.
6. **Ordinary invitations and the skill gap** (review M-items) are unaffected by identity alignment;
   step 4's skill text covers them.

## Recommendation

Adopt steps 1–7. They are the root-cause fix: graph stops fighting threads' participant model, and the
per-session re-acceptance that made H2 frequent disappears. Step 4 (H2 option D) closes the remaining
first-acceptance path at low cost and can land first, skill text only, after the concurrent `seat.rs`
and skills work. Concern 4 needs your explicit decision before implementation. Concerns 2–3 are
inherent to threads' pane-bound identity and are out of scope for herdr-graph alone.
