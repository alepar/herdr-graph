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
| keyless, `Retired{Reversible}` | new `threads.resurrect_seat` want: `ThreadsPort::resurrect_seat(seat, pane, replace, op)` onto the reopened pane, op key `resurrect:<clone>:<epoch>`, then `SeatTransition::Acknowledge` (seat unchanged) |
| keyless, `Retired{Final}` (after 7 d, or operator-final) | fresh-seat succession (unchanged), graph channels re-invited |

The seat fact gains the keyed/keyless distinction, read from `continuity_key` in `SeatInspect`. `replace` names a bare seat that threads already minted for the reopened pane: if `ResolveSeat` or a race gave that pane a seat, graph passes it, so threads retires it in the same decision. Otherwise it passes `None`. The restore launch keeps its current gate (`threads_done`) and argv (`--resume` with the last session). The agent then enrolls onto the original seat. Considered and discarded: resurrecting before the pane is reopened (`ResurrectSeat` needs a target); keeping keyless undo on the fresh-seat path until 7 d (contradicts B1).

Windows, derived from the seat's `retired_at` because threads exposes no clock fields:

- before `retired_at + 24h`: everything returns: memberships, history, pending ACK receipts and invitations (deadlines shifted).
- from 24 h to 7 d: memberships, history and requirement episodes return. Ordinary invitations and ACK receipts owed to the seat have settled as `recipient_retired`. Graph's channel wants re-issue graph's own invitations if needed.
- after 7 d (final): fresh seat, graph channels re-invited, external memberships lost.

The undo plan (`herdr-graph undo` preview) names which of the three applies for each seat, so the warning the trial found missing exists. The adoption plan warning (`src/plan/seat_adopt.rs`) changes from "closing loses the seat" to the windowed wording. `clone purge` of a seated keyless clone tombstones `FinalizeRetirement` as today, which 3.5.0 now accepts for keyless seats. A purge of a closed keyless seat inside the window therefore finalizes it at once (documented; this is the operator's explicit choice).

### 4. Herdr restart, every path

Recommended: one new effect, `threads.rebind_held_seat`: `ThreadsPort::rebind_held_seat(seat, pane, op)` over U2, op key `rebind:<clone>:<incarnation changed_at>`. The planner emits it after an incarnation change for every seated clone whose fact, read after the change, is `Unresolved` and whose bound pane exists. A seat already resolved onto its pane needs nothing. Every path:

| Case | Today | After |
|---|---|---|
| created (keyed), session captured | resume + key: back | rebind also runs; relaunch does not wait on it |
| created (keyed), no session | fresh + key: back | same as above |
| adopted (keyless), session captured | resume; cooperative continuity reattaches (Claude documented, Codex unverified) | relaunch waits for the rebind, then resumes; the seat is placed before the agent checks in, so the harness does not matter |
| adopted (keyless), no session, or resume fails | fresh start held until operator rebind | relaunch waits for the rebind, then starts fresh; the agent enrolls on the original seat |
| graph-created shell seat | nothing launches; seat stays unresolved | rebind places the seat; nothing launches |
| adopted shell pane | seatless, located by pane id | unchanged |
| agent never observed, keyed (`launched:` meta, no session) | nothing relaunches (`restart_arm_needs_an_earlier_agent`) | treated as had-agent: rebind, then fresh keyed start |
| agent never observed, keyless (`adopt --harness`, no agent) | ordinary fresh start, held by threads | rebind, then fresh start |
| pane missing after restart | `resume_blocked` row | unchanged |

4.1 Keyless relaunches wait for `rebind` done in the same incarnation. Keyed relaunches do not wait, because their key already reattaches them and a U2 failure must not regress them.
4.2 Rebind refusal: `NotFound` because the seat is already resolved on that pane counts as done; any other definitive refusal records the refusal, and the relaunch proceeds anyway (resume may still reattach). The status row (4.5) reports it.
4.3 Never-observed: `restart_covered` counts a `launched:` meta (graph launched the agent) as having had an agent, for non-shell harnesses.
4.4 Trial finding 3: an undo-restored seat whose agent exits with the pane open is not relaunched. The suspected cause is the restore epoch's one-launch mark (`planner.rs` restore branch, `assignment.rs` "allowed once per epoch"), which also suppresses the ordinary exited-agent restart. The bead root-causes it, fixes it so the ordinary restart rule applies once the restore launch has happened, and adds a regression test at the unit tier and the tier-3 private-Herdr tier.
4.5 Status: a seat still `Unresolved` one grace period (90 s) after its rebind was refused or could not run (old daemon, threads unreachable) gets a `clones.restart_held` status row and a doctor finding, with the operator remedy (`herdr-threads seat rebind SEAT --pane PANE --operator`). This is a fallback for a degraded threads, not the expected path.

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

## Post-Implementation Notes

> *As this design is implemented and iterated on — bug fixes, adjustments, anything that diverged from the assumptions above — append a dated note here, whether or not a formal debugging skill was used.*
