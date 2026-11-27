# Resurrected seats on undo, frictionless Herdr restart (hg-lnw)

Bead: hg-lnw (root epic). Branch: `super-auto/resurrect-seat-frictionless-restart`. Base: `main@60e287a`.

## Goal

Undoing the closure of a graph-managed tab or pane, adopted or created, brings back its original herdr-threads seat with its memberships. After a Herdr server restart, every graph-managed seat comes back on its own threads seat with no operator action. Graph pins the herdr-threads release that provides both.

## Requirement baseline

Source: the human user, 2026-11-26. B1 and B2 were said in the herdr-graph coordinator's pane and relayed in herdr-threads message mnSGHEfT9jck as the user's words. B3 and B4 are the user's answers in this run's pane, to the two blocker questions asked after research.

### Human request

> On "closing the tab and then undoing the closure gives the pane a fresh threads seat": "i think we added ht capability to re-adopt closed tabs seat. use that"
> On "a fresh agent start after a Herdr restart is held until an operator rebinds it": "this is not acceptable if so. same policy as herdr-threads here: herdr restarts need to be frictionless"
> To "How should graph get v4?": "Upstream fix (Recommended)" (ask herdr-threads for a small change so PersistentServiceClient can register at a chosen level; graph pins that release instead of 3.5.0)
> To "How should graph make this case frictionless?": "Upstream fix (Recommended)" (ask herdr-threads to let a v4 service rebind a restart-held keyless seat it manages; graph calls it before relaunching; graph always resumes when it has a session)

### Requirements

- B1 · "i think we added ht capability to re-adopt closed tabs seat. use that" · source: coordinator relay, mnSGHEfT9jck
- B2 · "herdr restarts need to be frictionless" · source: coordinator relay, mnSGHEfT9jck
- B3 · "Upstream fix (Recommended)" for the v4 service session · source: this pane, 2026-11-26
- B4 · "Upstream fix (Recommended)" for restart-held fresh starts · source: this pane, 2026-11-26

### Human scope changes

- none

## Problem description

Graph adopts running agent tabs as keyless clones (epic hg-ihi). Closing such a tab and undoing the closure reopens it with the original graph seat but a fresh herdr-threads seat. Threads memberships graph does not own are lost. The coordinator's sandbox trial confirmed this and found that the undo plan prints no warning. herdr-threads 3.5.0 (ht-eu3v) added `ResurrectSeat` to bring a closed seat back. After a Herdr server restart, a keyless seat whose agent cannot be resumed is held by threads until an operator rebinds it, and two other kinds of seat (graph-created shell seats, and seats whose agent was never observed) also do not come back by themselves. The same trial found a third defect: an undo-restored seat whose agent exits while the pane stays open is not relaunched, while other seats are relaunched within a second.

## Main challenges

The pinned threads client cannot use `ResurrectSeat`. `PersistentServiceClient` registers `service_session_v3` only, `ResurrectSeat` requires v4, and threads admits one service session per instance, so graph cannot run a second, v4 session beside it. Threads 3.5.0 also has no service operation that rebinds an unresolved, restart-held seat. `ResurrectSeat` answers `NotFound` for any seat that is not reversibly retired. Both gaps need herdr-threads changes; the user chose that route (B3, B4). Graph's design therefore depends on a threads release that does not exist yet, and its real-adapter work waits behind a gate. A pin bump alone is not safe either. Threads 3.5.0 makes every `Closed` retirement reversible, keyless ones included. Graph today maps reversible to `UnretireSeat`, which threads refuses for keyless seats. A keyless undo would then stall for 7 days before falling back to a fresh seat. The bump and the resurrect path must land together.

## Key decisions made

Graph pins the first herdr-threads release, called T below, that carries two upstream changes: U1, a caller-chosen service-session level on `PersistentServiceClient`; and U2, a v4 service rebind of an unresolved, restart-held seat. The request went to the herdr-threads main tab as message mnSGwik5XhjI. It was accepted (mnSGPKqWkgyl) as ht-lzhi (U1) and ht-0f0r (U2), both shipping in herdr-threads 3.6.0, so T = 3.6.0. U2 starts as an extension of `ResurrectSeat`, with the same authority, and gets its own design review because it touches the C2 restore holds. Its open points are which unresolved states qualify, whether the target must be the seat's own held pane (graph only ever asks for that pane), and what happens to a binding that reconnects later. Undo of a closed keyless seat calls `ResurrectSeat` while the threads retirement is reversible (7 d), and takes today's fresh-seat succession only once it is final. After a Herdr restart graph rebinds every unresolved seat of a managed pane that still exists through U2, before or alongside any relaunch, so cooperative continuity is no longer load-bearing for keyless seats. Keyed seats keep their key paths; the rebind only adds to them. Graph's own `ThreadsPort` methods and the fake model the new operations first, so all graph logic is built and tested before T ships. Only the real adapter, the pin and the real-threads tiers wait for T.

## Decision points, by section

### 1. Upstream dependency and the gate

Recommended: one gate bead, `Gate: herdr-threads 3.6.0 tagged with ht-lzhi and ht-0f0r`, closed by the orchestrator when that tag exists. It blocks the pin bump, the real adapter and the real-threads tests. Every graph-side bead runs against `ThreadsPort` and the fake, ungated. Considered and discarded: waiting with all work until T exists (idle for an unknown time, and the graph logic does not depend on T's exact API); a submodule patch or a graph-owned v4 client (both rejected by the user, B3). If herdr-threads declines U2 or reshapes it, the restart-rebind effect (§4) keeps its port method and gains a different adapter implementation. If no U2 exists at all, the effect's fallback is the `restart_held` status row (§4.5), and the gap is reported as unmet B2.

### 2. Threads contract audit, 3.1.0 → T

Recommended: record the audit in `docs/verification-matrix.md` (a threads-pin note) and CHANGELOG Unreleased. Findings for 3.1.0→3.5.0, from the herdr-threads CHANGELOG and source at v3.5.0 (`6aa914a`):

| Release | Change graph depends on | Graph action |
|---|---|---|
| 3.1.1 | binding cursor starts at the seat's furthest-offered notice; renumbered-socket reads as daemon-not-running (exit 3) | none |
| 3.1.2, 3.2.0, 3.2.1, 3.4.0, 3.4.1 | harness and text changes; `thread list --seat` unchanged (graph's only CLI call, `src/cli/adopt.rs:76`) | none |
| 3.2.2 | person commands refused in an agent environment; operator repairs unaffected | none (graph runs no `human` command) |
| 3.3.0 | node schema 18, one-way | daemon migration note |
| 3.5.0 | node schema 19, one-way; every `Closed` retirement reversible, keyless included (7 d, obligations settle at 24 h); `service_session_v4` with `ResurrectSeat`; keyless `FinalizeRetirement` allowed (store-level, v3 too); new `RepairHistory.decision_kind` strings; test env `HT_TEST_OBLIGATION_EXPIRY_MS`, `HT_TEST_KEYLESS_FINALIZE_MS` (test-support builds) | §3; daemon migration note; purge path keeps working |

Wire `PROTOCOL_VERSION` stays 9, and `service_session_v1`..`v3` stay accepted. `SeatInspection` is unchanged: keyed versus keyless is visible only through `continuity_key`. T's own delta (U1, U2 and anything else) is audited the same way when T exists, as part of the pin bead. Daemon requirement after the bump: herdr-threads daemon ≥ T. An older daemon refuses the v4 registration, and graph's existing registration-failure path reports it (no silent fallback to v3).

### 3. Undo of a closed seat

Recommended: split today's `succession`/`unretirable` decision in `src/threads/effects.rs` by key and retirement state:

| Seat fact on the Restore seat | Action |
|---|---|
| keyed, `Retired{Reversible}` | `UnretireSeat` (unchanged) |
| keyless, `Retired{Reversible}` | new `threads.resurrect_seat` want: `ThreadsPort::resurrect_seat(seat, pane, replace, op)` onto the reopened pane, op key `resurrect:<clone>:<epoch>:<replace seat id or none>`; outcomes below |
| keyless, `Retired{Final}` (after 7 d, or operator-final) | fresh-seat succession (unchanged), graph channels re-invited |

The seat fact gains the keyed/keyless distinction, read from `continuity_key` in `SeatInspect`. `replace` names a bare seat that threads already minted for the reopened pane: if `ResolveSeat` or a race gave that pane a seat, graph passes it, so threads retires it in the same decision. Otherwise it passes `None`.

`resurrect_seat` outcomes. Each one is either done or deferred; a deferred want is re-planned from the seat fact re-read on the next pass, never from the previous outcome:

- success: `SeatTransition::Acknowledge` (seat unchanged), then the invitation reset below.
- `NotFound`: deferred. The next fact read picks the row: `Retired{Final}` takes fresh-seat succession; `Resolved` on the reopened pane (an earlier call went through and its response was lost) is acknowledged as success; still `Retired{Reversible}` retries.
- transient (`ServiceBusy`, `StaleHostObservation`, which threads classes as transient, timeout, threads unreachable, outcome unknown): deferred and retried on the next pass.
- any other definitive refusal (`TargetAlreadyOwned`, `Unauthorized`, `InvalidRequest`, `Unsupported`, `InstanceMismatch`): recorded once as journal meta `resurrect_refused:<clone>` with the epoch and reason, and deferred. A refusal is not final for the epoch: the want is re-planned whenever its inputs change, namely the re-read Restore seat fact, or the reopened pane's current seat (a bare seat now present becomes `replace`, or a target that freed). Each re-plan with a changed `replace` (or target) uses a distinct op key, so threads' idempotency does not replay the cached refusal. The restore launch stays gated on `threads_done`, because launching on a fresh seat would drop the seat B1 asks for. A seat whose meta is set and whose fact still reads `Retired{Reversible}` one grace period (90 s) after the first refusal gets a `clones.resurrect_held` status row and a doctor finding naming the seat, pane, reason and the operator remedy (`herdr-threads seat rebind SEAT --pane PANE --operator`, which also resurrects a reversibly retired seat). The row clears when the fact leaves `Retired{Reversible}`, and the gated restore launch proceeds once the seat is acknowledged, including when an operator resurrected it by hand (the fact reads `Resolved` on the reopened pane: graph acknowledges it as done).

A Restore seat whose fact is not yet `Retired` (threads has not caught up with the close, or is unreachable) gets no want; the existing caught-up guard in `succession` already waits for it.

The restore launch keeps its current gate (`threads_done`) and argv (`--resume` with the last session). The agent then enrolls onto the original seat. Considered and discarded: resurrecting before the pane is reopened (`ResurrectSeat` needs a target); keeping keyless undo on the fresh-seat path until 7 d (contradicts B1).

Windows, derived from the seat's `retired_at` because threads exposes no clock fields:

- before `retired_at + 24h`: everything returns: memberships, history, pending ACK receipts and invitations (deadlines shifted).
- from 24 h to 7 d: memberships, history and requirement episodes return. Ordinary invitations and ACK receipts owed to the seat have settled as `recipient_retired`. Graph's own invitations are re-issued by the reset below.

Invitation reset after a resurrect. Graph cannot tell a settled ordinary invitation from a pending one: threads reports both as no membership or `Invited`, which graph's poll records as `Pending`, and the channel want counts `Pending` on the recorded seat as satisfied (`src/threads/effects.rs` invitation wants). So the resurrect success transition, in the same journal write as the acknowledgement, marks every invitation row of that clone with constraint `Ordinary`, recorded state `Pending` and a link to the resurrected seat as `Retired`. The existing want rule then re-plans an `Invite` for each one still desired (graph's seat-wide participation threads, minus opt-outs). Accepted rows and `Required` rows (graph's seat and teamspace channels, which are requirement episodes and return intact) are left alone. The reset does not read the window or any clock: a service invite on a seat that still holds a pending invitation to that thread reuses it (herdr-threads `service_controls.rs`, existing pending invitation), so before 24 h the re-issue is a no-op, and between 24 h and 7 d it mints a fresh invitation. "Graph's own invitations" means exactly these ordinary seat-wide invitations; invitations other parties sent are theirs to re-send.
- after 7 d (final): fresh seat, graph channels re-invited, external memberships lost.

The undo plan (`herdr-graph undo` preview) names which of the three applies for each seat, so the warning the trial found missing exists. The adoption plan warning (`src/plan/seat_adopt.rs`) changes from "closing loses the seat" to the windowed wording. `clone purge` of a seated keyless clone tombstones `FinalizeRetirement` as today, which 3.5.0 now accepts for keyless seats. A purge of a closed keyless seat inside the window therefore finalizes it at once (documented; this is the operator's explicit choice).

### 4. Herdr restart, every path

Recommended: one new effect, `threads.rebind_held_seat`: `ThreadsPort::rebind_held_seat(seat, pane, op)` over U2, op key `rebind:<clone>:<incarnation changed_at>`. Every rule in this section reads state on each planner pass; none waits for one effect's outcome, and every wait is bounded by the 90 s grace that already starts at `incarnation:changed_at` (`relaunch_grace`, `src/reconcile/mod.rs`).

- Emission. On every pass for as long as a seated clone's fact, read at or after `incarnation:changed_at`, is `Unresolved` and its bound pane exists, including after the 90 s grace, the planner wants the rebind (one op key per incarnation, so a want is re-planned, not duplicated). The grace opens the keyless gate below; it does not stop the rebind. A fact that turns `Unresolved` later in the incarnation is picked up by the next pass. A fact still reading `Resolved` on its pane, or read before `changed_at`, counts as threads not caught up: nothing is emitted, and the keyless gate below stays closed (the same caught-up reasoning as the close guard in `succession`, `src/threads/effects.rs`). The first pass that sees `Unresolved` in an incarnation records meta `seat_unresolved:<clone>` = that `changed_at`, which is how a later `Resolved` read is known to be fresh.
- Gate. A keyless relaunch (fresh `StartAgent` or `RelaunchOccupant`) after an incarnation change proceeds once either holds: the seat fact reads `Resolved` on the clone's own pane and `seat_unresolved:<clone>` names this incarnation (threads saw the restart and the seat is placed, by graph's rebind or anyone else), or the grace since `changed_at` has passed. It then proceeds whatever the rebind did. Keyed relaunches never wait on it. `RelaunchOccupant` keeps its existing grace deferral unchanged, so for it the gate adds nothing.
  Accepted behaviour: when graph never reads `Unresolved` in an incarnation (threads or cooperative continuity placed the seat before graph's first fresh read), `seat_unresolved:<clone>` is never set, and a fresh keyless `StartAgent` waits the full 90 s grace, because a stale `Resolved` read is indistinguishable from a fresh one. The wait is bounded and needs no operator action. Resume relaunches (`RelaunchOccupant`) already wait out the same grace, so it adds no delay for them.
- Status. When the grace ends with the fact still `Unresolved`, §4.5 applies, whatever happened to the rebind.

Every path:

| Case | Today | After |
|---|---|---|
| created (keyed), session captured | resume + key: back | rebind also runs; relaunch does not wait on it |
| created (keyed), no session | fresh + key: back | same as above |
| adopted (keyless), session captured | resume; cooperative continuity reattaches (Claude documented, Codex unverified) | relaunch waits until the seat is placed or the grace expires, then resumes; when the rebind succeeds the seat is placed before the agent checks in, so the harness does not matter |
| adopted (keyless), no session, or resume fails | fresh start held until operator rebind | relaunch waits until the seat is placed or the grace expires, then starts fresh; the agent enrolls on the original seat |
| graph-created shell seat | nothing launches; seat stays unresolved | rebind places the seat; nothing launches |
| adopted shell pane | seatless, located by pane id | unchanged |
| agent never observed, keyed (`launched:` meta, no session) | nothing relaunches (`restart_arm_needs_an_earlier_agent`) | treated as had-agent: rebind, then fresh keyed start |
| agent never observed, keyless (`adopt --harness`, no agent) | ordinary fresh start, held by threads | rebind; the fresh start waits until the seat is placed or the grace expires |
| pane missing after restart | `resume_blocked` row | unchanged |

4.1 Keyless relaunches follow the gate above: seat placed in this incarnation, or the grace expired. Keyed relaunches do not wait, because their key already reattaches them and a U2 failure must not regress them.
4.2 Rebind outcomes, which only record; none opens or closes the gate. Success and an already-placed answer (`NotFound` or the U2 equivalent, because the seat is already resolved on that pane) are done. A transient outcome (error, timeout, threads unreachable, `ServiceBusy`, `StaleHostObservation`) is deferred and retried on every pass for as long as the fact reads `Unresolved` in the current incarnation, including after the 90 s grace; only the keyless relaunch gate opens at the grace, the rebind does not stop. A late rebind that lands after the agent was already started fresh places the seat on the pane, and the agent enrolls onto it at its next lifecycle check-in (threads ResurrectSeat semantics: no binding opens, the next check-in enrolls). `clones.restart_held` (4.5) clears when the fact reads `Resolved`. Any other definitive refusal is recorded once as meta `rebind_refused:<clone>` with the incarnation and reason, and not retried in that incarnation; the status row (4.5) reports it.
4.3 Never-observed: `restart_covered` counts a `launched:` meta (graph launched the agent) as having had an agent, for non-shell harnesses.
4.4 Trial finding 3: an undo-restored seat whose agent exits with the pane open is not relaunched. The suspected cause is the restore epoch's one-launch mark (`planner.rs` restore branch, `assignment.rs` "allowed once per epoch"), which also suppresses the ordinary exited-agent restart. The bead root-causes it, fixes it so the ordinary restart rule applies once the restore launch has happened, and adds a regression test at the unit tier and the tier-3 private-Herdr tier.
4.5 Status: a seated clone whose fact still reads `Unresolved` when the grace since the incarnation change ends, whatever happened to its rebind (refused, still erroring, never attempted because threads was unreachable), gets a `clones.restart_held` status row and a doctor finding, with the operator remedy (`herdr-threads seat rebind SEAT --pane PANE --operator`). The reason names `rebind_refused:<clone>` when it is set for this incarnation, otherwise "rebind did not complete". The row clears when the fact reads `Resolved`. This is a fallback for a degraded threads, not the expected path.

Considered and discarded: keying adopted seats (reverses G4, keyed seats cannot be released), graph running the operator rebind itself (a service taking operator authority), best-effort plus documented gap (contradicts B2).

### 5. The pin bump and docs

Recommended: one bead, gated, moves `third_party/herdr-threads` to T, updates `Cargo.lock`, the `check_version` fixture (`tests/test_isolation.rs`), and every pin text site. Those are README (service-session and version paragraph, submodule line, verification-matrix summary), CURRENT-DESIGN.md (pin, keyless close and undo, the "resurrect out of scope" line, node identity), the CHANGELOG Unreleased entry with the migration note (daemon ≥ T, schema 19 one-way), `docs/verification-matrix.md`, and the onboarding spec's G4 table. The behavioural text (README keyless restart line, G4 rows, CURRENT-DESIGN keyless lines) is written by a docs bead after §3 and §4 land, so the text describes code that exists.

### 6. Tests

- Tiers 1–2: the fake models a reversible keyless close with both windows, `resurrect_seat`, and `rebind_held_seat` (including its refusals). Unit tests cover the §3 table, the windows and the §4 table rows.
- Tier 3 (private Herdr, real threads binary of T): undoing a closed adopted tab keeps the original seat and an external membership; a Herdr cold restart with a keyless clone that has no captured session brings it back on its seat with no operator action; finding 3's regression (an undo-restored seat whose agent exits is relaunched).
- Tier 5 (real threads T): `ResurrectSeat` end-to-end through graph's adapter, in each window. The test daemon runs with `HT_TEST_OBLIGATION_EXPIRY_MS` and `HT_TEST_KEYLESS_FINALIZE_MS` shortened, which needs a test-support threads build; the tier-5 bead decides how `tests/support/threads_bin.rs` provides one. Also covered: the U2 rebind of a held seat.
- hg-c9e items picked up: the reconcile-layer keyless pane-close cascade and its undo (tier 3 above), and `seat adopt --harness` on a pane with no agent across a restart (§4 row "never observed, keyless").

Every rig goes through `TestRoot`/`PrivateHerdr`; runs only in `hg-build`.

### 7. Out of scope

Herdr-native "undo close" (graph has none; graph undo is the only undo). Automatic threads-side resurrect by resumed session id (threads design D3, a threads follow-up). Trial finding 4 (roster "is Unknown" after a daemon restart) unless a bead touches that code.

## Unchanged behavior

- Keyed seats: undo of a keyed `Retired{Reversible}` seat calls `UnretireSeat`; keyed relaunches after a restart never wait on the rebind; existing keyed undo and restart tests pass unchanged.
- Fresh-seat succession for a `Retired{Final}` seat, with graph channels re-invited, is unchanged.
- The restore launch keeps its `threads_done` gate and `--resume` argv; a restore epoch still launches at most once through the restore path.
- `RelaunchOccupant` keeps its existing 90 s grace after an incarnation change; no new grace constant is added.
- The caught-up guard in `succession` (a pre-close read does not stand in for the close) is unchanged.
- `Required` invitations and accepted memberships are never reset or re-issued by a resurrect.
- Adopted shell panes stay seatless; a pane missing after a restart still gets the `resume_blocked` row.
- `clone purge` of a seated keyless clone still tombstones `FinalizeRetirement`.

## Post-Implementation Notes

> *As this design is implemented and iterated on — bug fixes, adjustments, anything that diverged from the assumptions above — append a dated note here, whether or not a formal debugging skill was used.*
