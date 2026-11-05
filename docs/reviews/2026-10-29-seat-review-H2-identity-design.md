# Design — Threads-authoritative 1:1 identity for graph clones, with unattended restart and live handoff

Status: proposal, revised 2026-11-04 with the user's decisions (§4). Restored from the review session
after the /tmp wipe; the 2026-10-31 revision is the base. Read-only review. Sources:
- herdr-graph `main` @ c77d9ad;
- herdr-threads vendored v2.6.4, and main @ b86202243 ("Prepare v2.8.2": ht-hsyg merged);
- Herdr upstream tag v0.9.5, which is the installed binary.

Builds on `docs/reviews/2026-10-29-seat-review-H2.md` and
`docs/reviews/2026-10-29-seat-review-H2-identity.md`. Tracking: Beads hg-s6o (H2).

## Abstract

herdr-graph and herdr-threads both maintain a per-pane identity: the graph clone and the threads
seat. Each re-derives it independently after Herdr restarts. Neither does so unattended in the case
that matters most:
- After a cold restart, Herdr drops pane metadata tokens and regenerates every terminal id. The
  graph then marks every clone Unknown and needs a user-confirmed rebind per clone.
- Threads holds every seat unresolved until the agent's own session resumes and claims it, or until
  an operator rebinds it.
- After a live handoff (`herdr update --handoff`), the graph's matcher fails outright: it assumes
  terminal ids survive, and they do not.
- Threads recovers live handoff unattended only from v2.8.2 (ht-hsyg, merged, unreleased).

We propose to make the threads seat the single authoritative identity:
- Each graph clone binds 1:1, write-once, to a pane threads seat.
- The graph deletes its own clone re-matching and follows threads.
- Threads closes its remaining restart gaps without Herdr changes, using evidence it observes
  itself: surviving processes, resumed native sessions, and a **continuity key** that graph-launched
  agents present at check-in.
- Pane close retires the seat immediately; **graph undo reverses it** (reopen tab, resume session,
  un-retire seat). Reattachment after un-retire is evidence-based, so no service ever places a seat.

## 1. What each side does today (verified)

Herdr facts (v0.9.5):
- Public workspace, tab and pane ids are preserved across both cold restore and live handoff
  (`persist/restore.rs:325-355`).
- `terminal_id` is newly allocated in both cases (`restore.rs:689`).
- Pane metadata tokens are not persisted or handed off. `PaneSnapshot` has cwd, label, agent name,
  agent session, resume plan and launch argv only (`persist/snapshot.rs:100-114`). Tokens are reset
  (`restore.rs:428`).
- Live handoff keeps the agent processes alive (PTY fds passed with SCM_RIGHTS), so `shell_pid` and
  its start time are unchanged. A cold restart starts new shells.
- Cold restart resumes agents (`claude --resume <id>`) only once a client with real geometry
  attaches (`app/agent_resume.rs:99-103`). A headless restart resumes nothing.
- Herdr emits no handoff or restore event and exposes no boot id. API clients see a disconnect.
- Herdr will not be changed (user, 2026-10-31).

Per-case comparison:

| Case | Graph today | Threads v2.6.4 | Threads v2.8.2 (ht-hsyg) |
|---|---|---|---|
| Rename, reorder, move within one Herdr run | Unattended (token) | Unattended (structural Move by terminal) | same |
| Own daemon restart | Unattended (token) | Unattended (C4 carry) | same |
| Live handoff | **Fails.** Token lost, and terminal id assumed stable but is not (`model/common.rs:36`, `observe/matcher.rs:4`). Only the untested native-session pass can match | Needs the agent's next turn or a watch claim. Idle sessions wait | **Unattended** via process continuity (same shell pid and start time). Excludes seats with no agent binding, `managed_launch`-only seats, and setsid/tmux harnesses |
| Cold restart, Herdr resumed the agent (client attached) | Only the untested native-session pass; otherwise Unknown → user rebind | **Unattended** (C1 claim by the resumed session's hook) | same |
| Cold restart, headless (no resume) | Unknown → user rebind | Unresolved, held → operator | same |
| Cold restart, fresh agent or bare shell in the restored pane | Unknown → user rebind | Operator | same |
| Same session resumed in a different pane | Mostly no re-match | Old seat unresolved → C1 moves it; old seat resolved → new seat | same |
| Pane closed, same run | Retire clone (undoable) | Retire seat (irreversible) | same |
| Pane gone during a restart (closed before the first pass) | Unknown forever | Waits for operator (only watched panes auto-retire) | same |
| Ambiguous evidence | **Silent first-wins** | Refuses; operator argv | same |

Conclusion:
- Threads is already **as good as or better than the graph in every case**, and safer in the
  ambiguous ones. The user's condition for delegating is met for parity.
- The bar the user actually set, frictionless and unattended restarts, is met by **neither**. The
  gaps below must be closed in threads and the graph (no Herdr changes) before delegation delivers it.

## 2. Target model

- **clone ≡ threads seat.** `clone.threads_seat` is write-once and set when the clone's pane is
  first resolved.
  - Invariant: at most one live clone per threads seat; a clone has one threads seat for life,
    including across undo and resurrection (§4 D2).
  - `cl_` stays as the graph's key, because clones exist in plans before panes and `cl_` appears in
    folders, Beads labels and undo records. It is also the continuity key (§4 D1).
- **Threads is authoritative** for where a clone is (seat target), whether it is live (resolved,
  unresolved, retired) and how it survives restarts. The graph reads this with `SeatInspect` /
  `Seats`, both public reads, and never re-matches clones itself.
- **The graph is authoritative for the lifecycle decisions it owns**: launching agents (with the
  key), undo and resurrection (un-retire, then relaunch).
- **Graph seats (tabs) and teamspaces stay graph-matched.** Herdr preserves tab and workspace ids
  across restore and handoff, so that matching can become simple id equality instead of the current
  incarnation majority vote.
- **Hard dependency:** binding waits while threads is unavailable (accepted).

## 3. Threads gaps to close for unattended recovery

**G1. Release process continuity (ht-hsyg) and pin it.**
- Ship v2.8.2 and move the graph submodule from v2.6.4.
- Covers live handoff for seats with an agent binding.

**G2. Process continuity for every seat, not just agent bindings.**
- Today candidates need a top-level agent binding with a liveness fingerprint.
- Change: record the pane's shell fingerprint (pid, start, session, tty) on the **seat** at
  resolution and on each reconcile pass. Judge handoff continuity on that fingerprint.
- This covers bare panes, panes whose agent has exited, `managed_launch`-only seats, and graph
  clones before their first agent.
- Same evidence class as ht-hsyg (a surviving process observed by threads), so it stays within
  policy (C5). ht-hsyg itself names this as its follow-up. Keep its evidence rules: kernel session
  and controlling tty; the saved target is only a locator.

**G3. Cold restart without Herdr changes.**
- Herdr will not change. ht-hsyg likewise treats Herdr lineage, pane ids, labels and Herdr's
  agent_session field as diagnostics only. So threads has **no** evidence of its own that a restored
  pane is the old one: the old shells are gone, tokens and terminal ids are reset.
- Cold restart therefore splits:
  - **(a) Clones with a resumable session.** The session itself is the evidence (C1). The only gap
    is that nobody resumes it on a headless restart. Closed by G4.
  - **(b) Clones with no resumable session** (harness without resume, or a fresh agent wanted).
    The graph launches the agent with the clone's continuity key; its check-in reattaches the
    unresolved seat (§4 D1).
  - **(c) Bare panes** (no agent at all). Nothing presents evidence, so the seat **waits**
    unresolved until the graph launches an agent there, which then reattaches via (b). Sends to the
    seat warn that it is unavailable; nothing breaks and nobody intervenes. No placement by
    assertion (rejected: needs a service-placement rule and risks a reused pane id binding the
    wrong pane).

**G4. Resume driver for headless cold restart.**
- On a cold restart the agent does not come back without an attached client.
- Owner: the graph, which already owns launch and resume (`reconcile/session.rs`). When a clone's
  seat is unresolved and its pane is a bare shell, resume the clone's last session there (or launch
  fresh), always with the continuity key in the environment.
- C1 or the key then confirms with evidence. If the pane was wrong, the seat attaches to where the
  agent actually runs, so a locator error corrects itself.

**G5. No limbo after restart.**
- An unresolved seat whose target is absent in a coherent snapshot of the new incarnation currently
  waits for the operator.
- Change: after a bounded window (reuse ht-wpq4's 10-minute observation), treat it as a pane close:
  retire it. For key-bearing seats that retirement is reversible like any other (§4 D2).

**Remaining operator cases, kept on purpose:**
- genuinely ambiguous evidence: two seats claim one pane, or two seats carry one key;
- person seats.

Neither occurs in an ordinary restart.

## 4. Decisions (user, 2026-11-04) and the threads contract they imply

**D1. Continuity key: approved.**
- The graph launches every clone agent with `HERDR_GRAPH_CLONE=<cl_ id>`.
- Threads records the key on the seat when that agent checks in. A later check-in presenting the
  same key on a pane whose seat is unresolved (or on a held pane) reattaches that seat.
- Evidence class: cooperative, like a resumed native session id: the agent presents something about
  itself. Precedent: named continuity in the named-seats run (ht-kqrt).
- Rules: one live seat per key. Two candidate seats for one key → refuse (operator). A key on a
  resolved seat elsewhere → new seat is not created; the check-in is refused as a conflict.
- Policy: new C-row in TRUST-POLICY (launcher-supplied continuity key on pane seats).

**D2. Pane close retires immediately; graph undo reverses it.**
- Close behaves exactly as today: memberships released, seat out of service, no restore hold.
- `graph undo` (and resurrection) reopens the tab, un-retires the seat, and resumes the session
  with the key; the check-in reattaches the same seat id. Messages, receipts and cursors history
  stay attached to that id.
- Threads contract: `UnretireSeat{seat}` by the registered service, allowed only for seats whose
  recorded key was presented by that service's launches. It returns the seat to **unresolved with
  no pane**; it never places it. Reattachment stays evidence-based (D1 or C1).
- Required memberships were released at retire; the graph re-invites after reattachment, so the
  resumed agent accepts once (its hook prints the live `accept-required`). Flagged: if undo must
  be fully hands-off, threads would instead restore the released requirement episodes on
  un-retire, which is a larger change.
- Implementation choice for threads (same external behaviour):
  - **true un-retire:** relax the `seats_retirement_terminal` trigger, add a replicated
    `seat.unretired` fact, receipts stay terminal, memberships re-materialized as new intervals;
  - **reversible retired state:** a seat state that reads as retired to every peer but is not
    terminal until finalized. Roughly half the cost; no schema-fact change.
  The review recommends the second; either satisfies D2.
- Graph cross-pane moves compose from these: close the old pane, un-retire, launch with the key in
  the new pane. No separate move operation; `clone rebind --pane` goes away.

**D3. An agent exiting while its pane stays open keeps the seat and its memberships.**
- The pane existing means the operator intends to relaunch, possibly in another harness, so the
  seat is still needed. Reverses spec §7.1 and roast r1 (release on occupancy end).

**Not needed any more** (from the 2026-10-31 draft): service-managed park / move / resume
operations and graph manager re-placement on cold restart. D1 + D2 replace them, and no service
ever places a seat.

## 5. Graph changes

1. `clone.threads_seat` (write-once), set by a resolve step after the pane exists. Backfill for
   live clones from verified bindings; conflicts become repair plans.
2. **Launch with the key:** every agent launch and resume (`reconcile/session.rs`) sets
   `HERDR_GRAPH_CLONE=<cl_>`.
3. **Delete clone re-matching:** `observe/matcher.rs` token, terminal and session passes for
   clones; clone terminal and incarnation tracking; `/seat` rebind proposals; `clone rebind --pane`.
   - Location and liveness come from `SeatInspect`, polled per observation step.
   - Mapping: resolved → Present at target; unresolved → Unknown; retired → clone retired
     (undoable).
4. **Undo and resurrection** → reopen tab, `UnretireSeat`, relaunch with the key, re-invite.
5. **Participation keyed to the clone** (that is, the threads seat), per the H2-identity proposal:
   invite once per seat, release only on retire, keep across agent exit (D3). `/seat` drops accept
   commands (H2 Option D).
6. `/seat` resolution: own pane → `ResolveSeat` → clone with that `threads_seat`.
7. Add G4 (resume driver) to the reconciler.
8. Tab and workspace matching across incarnations: use the preserved Herdr ids.

## 6. Sequencing

0. **Graph:** H2 Option D (`/seat` stops printing accept commands). Independent; can land first.
1. **Threads:** G1 (release v2.8.2) and G2 (seat-level shell fingerprint). Live handoff becomes
   unattended for every seat.
2. **Threads:** D1 continuity key. **Graph:** §5 item 2 (launch with the key).
3. **Graph:** §5 items 1, 3, 5, 6, 8, behind the bumped pin. This deletes the matcher.
4. **Threads:** D2 un-retire and G5 limbo disposition. **Graph:** §5 item 4.
5. **Graph:** G4 resume driver. Headless cold restart resumes agents.

Real-Herdr qualification after steps 1, 3 and 5:
- cold restart with a client attached and headless;
- live handoff with idle agents, bare panes and an exited agent;
- pane closed during downtime;
- close then undo, and resurrection, keeping the seat id.

The graph's current handoff claims are untested and partly wrong (§1), so these tests are the
acceptance criteria, not a formality.

## 7. Open items / risks

- **No Herdr change.** Every recovery path rests on evidence threads observes (processes, sessions,
  keys). Bare panes wait for an agent rather than being placed.
- **The key is cooperative**, not authentication: any same-uid process could present it, exactly as
  with native session ids. Consistent with threads' current trust model.
- **A clone whose agent never checked in has no recorded key**, so its first agent after a restart
  gets a new seat. Nothing was accepted on it yet, so little is lost.
- **Undo needs one re-acceptance** of required memberships unless threads restores requirement
  episodes on un-retire (D2 flag).
- **Many unresolved seats must not stall reconciliation**; ht-kaw9 (in progress) touches exactly
  that.
- Ambiguity stays operator-resolved by design.
