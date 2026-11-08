# herdr-threads ht-ma6i provisional protocol shapes (relayed 2026-11-05)

Source: herdr-threads coordinator (seat snSGTpBk7xGd), message mnSGy0OTGAUK on thread dOqG4VQNjQ4, relaying the threads worker (seat snSGeNu15tRd). Provisional: the threads design roast may change them; corrections arrive on the same thread. Threads spec: herdr-threads branch super-auto/graph-clone-identity, docs/superpowers/runs/2026-11-04-graph-clone-identity/2026-11-04-graph-clone-identity-design.md §4.5.

- Env: HERDR_THREADS_CONTINUITY_KEY (1-128 ASCII-graphic bytes). Native capability: seat.continuity_key_v1. Service capability: service_session_v3; an old daemon rejects registration.
- Service operations:
  - ReserveContinuityKey{key, operation} -> ContinuityKeyReserved{key, owner, seat?, reserved_at}. Idempotent for the owner, refused for another service. seat? gives the binding once an agent has checked in.
  - ReleaseContinuityKey{key, operation} -> ContinuityKeyReleased(key). Conflict while a seat holds it.
  - UnretireSeat{seat, operation} -> SeatUnretired{seat, generation}. Owner only; returns the seat unresolved with no pane.
  - FinalizeRetirement{seat, operation} -> RetirementFinalized(seat). Releases the key.
  - ContinuityAlerts{page} query -> Page<ContinuityAlert{ordinal, key, seat?, host, pane, kind, at}>. Pull feed, one row per new conflict (no push channel).
  - ContinuityConflictKind: key_live_elsewhere, key_retired_reversibly, pane_owned_by_other_seat, session_matches_other_seat, seat_holds_other_key.
- SeatInspection adds two optional fields, absent when empty:
  - continuity_key: {key, owner, conflict?: {host, pane, kind, first_seen_at}}
  - retirement_state: "reversible" | "final", present iff the seat reads retired.
  - summary.continuity stays "retired" while reversible; retirement (the job status) is null until final; after un-retire, continuity is "unresolved" with target null.
- Replicated facts seat.retired_reversible / seat.unretired: a 3.0.x node parks them, shows the seat live until the final seat.retired, and replays them after upgrade.
- Behaviour the graph relies on:
  - Reserve before launch.
  - UnretireSeat before relaunching a reversible seat's agent; relaunching first gives a key_retired_reversibly conflict and a held pane until the next check-in after un-retire.
  - No timer on the reversible window: the graph finalizes when its undo window ends.
  - No re-invite after un-retire: memberships and requirement episodes are never released while reversible. Release any stale requirement after reattach.

## Update 2026-11-05 (message mnSGt9sPAujE, threads worker #7 mnSGxCJnHC0D)

- ReserveContinuityKey{key, seat?, operation}. With seat: binds at once, moves nothing, places nothing. Accepted only when the seat is local, resolved, on a Herdr host and unnamed, holds no other key, and the key is unbound or already bound to that seat. A key bound elsewhere or a seat holding another key is Conflict; any other seat state is refused. Idempotent.
- No "service resolved it" guard: the graph must name the right seat (its recorded threads_seat); naming a wrong seat makes that seat reversible and reattachable by the key (accepted limit).
- SeatInspection.continuity_key gains bound_by: "check_in" | "service_reservation".
- Confirmed: no seat needed to reserve for a brand-new clone (bound at first keyed check-in). No threads-side timer finalizes; the reversible window lasts until the service's FinalizeRetirement or an operator retire.

## Correction 2026-11-05 (message mnSGLnbuvcjW, threads worker #8 mnSGAQs25Skk)

- ReserveContinuityKey{seat}: accepts a resolved OR unresolved seat (binding moves nothing); a clone whose seat is already waiting at upgrade can gain a key. Refused for a retired, named or hc seat.
- New native refusal ContinuityKeyPending: a claimless ResolveSeat (plain CLI) on a pane whose key is bound to an unresolved seat; no seat minted, no alert. Hooked agents never hit it (their claim reattaches).
- Conflict alerts on a pane with a resolved owner hold nothing; that seat keeps working.

## Answers 2026-11-05 (message mnSGq1eyidhP, threads worker #10 mnSGRxWdaUrx) to mnSG57YUhprU

1. Every path that makes a retirement final (pane close of a keyless seat, operator retire, --replace, FinalizeRetirement) deletes the key row in the same transaction: the reservation is released, not returned to reserved-unbound. Relink to a successor is a fresh ReserveContinuityKey{key, seat: successor}; it succeeds if the successor holds no other key and is resolved or unresolved. D1 and D2 land on one branch and ship in one release (no between-releases window). A seat retired final before this ships never had a key row. (Design, not yet implemented.)
2. A final-retired seat is never a C1 candidate. A resumed check-in on a pane already resolved to the successor falls through to ordinary check-in on that seat (works today). On an unowned pane a non-resume event mints a new seat; the Resume-event case is untraced. A held pane is refused. With the key re-bound to the successor, a check-in on the successor's pane is a key no-op; on another pane it is key_live_elsewhere. Resolving the successor before resuming is the right order. (Verified in today's code: src/cli/hook.rs:2049-2085.)

## Release v3.1.0 (2026-11-07, messages mnSGZjUc26XY, mnSGpz0269ot, mnSGPRMSMiID)

- herdr-threads v3.1.0 released with D1, D2, G2 and G5 (merged as e3ed33f82).
- SeatInspection.retirement_state is present only as "reversible"; absent means final or not retired ("final" is never emitted: older protocol-9 clients reject unknown values).
- ContinuityAlerts: the last page's cursor stays at the high-water ordinal; the graph dedupes by ordinal (no after_ordinal resume point).
- Upgrade order: the daemon must be 3.1.0 before a graph built on this library connects (the client registers service_session_v3). Node schema 17, one-way.
- Known gap upstream: real-Herdr qualification did not cover a cold restart with a client attached.
