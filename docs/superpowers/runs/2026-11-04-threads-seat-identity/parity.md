# Parity: old clone matcher vs SeatObservation (spec section 6, requirement B3)

Bead hg-66u.23. Every scenario the observe matcher (`src/observe/matcher.rs`, `match_snapshot` clone passes) and the
planner `LiveIndex` clone lookups (`src/reconcile/planner.rs`, `LiveIndex::pane` / `pane_claimed_by`) handle today,
with the SeatObservation test that replays it. Tests live in `src/observe/tests.rs`, section "parity with the old
matcher". The fake `ThreadsSeatView` answers what herdr-threads v3.0.0 documents for the scenario (live handoff keeps
the seat Resolved at the same pane via process continuity; a cold restart with a resumed session reattaches by C1; a bare
pane after a cold restart stays Unresolved until D1; a closed pane retires the seat; two seats claiming one pane are
Resolved plus KeyConflict). Where the scenario allows, the clone's token and terminal evidence is gone, so the matcher
cannot be what makes the test pass.

Verdicts: same = identical outcome; better = threads decides where the matcher could not (or decided arbitrarily);
worse = threads alone would do worse than the matcher (see Findings); n/a = the mechanism does not apply to
SeatObservation, with the reason.

Mutation check: with `seat_observation::observe` stubbed to return immediately, 9 of the 17 `parity_*` tests fail
(the "better" rows: token-gone, renumbering, resumed session, partial restore, live handoff, resumed in another pane,
duplicates, headless unknown fact, close-while-down). The other 8 pass on the matcher alone, as their "same" verdict
says: SeatObservation leaves the matcher's result in force there (pre-D1 precedence).

## Old-matcher scenario table

| scenario | old mechanism | old test | SeatObservation test | verdict | note |
|---|---|---|---|---|---|
| Pane carries the clone's graph token | `match_snapshot` pass 1 (token); `LiveIndex::pane` token scan | `herdr_restart_no_mass_retirement`, `pane_close_retires_clone_only` | `parity_token_pass_clone_found_by_seat_when_the_token_is_gone` | better | threads finds each of two clones without any token (handoff dropped them). Token still binds when threads is silent. |
| Same-incarnation bound pane id / terminal id | pass 2, `b.incarnation == snap.incarnation`; `LiveIndex::pane` bound-id branch | `event_loss_recovered_by_snapshot`, `settle` in every test | `parity_same_incarnation_bound_pane_is_a_steady_state` | same | Resolved at the bound pane changes nothing and commits nothing per step. |
| Terminal id + cwd + session across incarnations | pass 2 cross-incarnation branch | `restart_without_tokens_matches_terminal_id_with_cwd` | `parity_live_handoff_two_clones_stay_present` | same / better | handoff reallocates terminal ids; threads keeps both seats Resolved at the same panes, no cwd or session cross-check needed. |
| Live handoff (new incarnation, new terminal ids, no tokens, same pane ids) | pass 2 cross-incarnation (terminal id + cwd + session); a handoff reallocates terminal ids and drops tokens, so the matcher loses the clone | `herdr_restart_no_mass_retirement` (restart), `rebind_before_the_first_post_restart_pass_survives_it` | `parity_live_handoff_two_clones_stay_present` (also `live_handoff_with_resolved_seats_keeps_clones_present`, `seat_resolved_binds_and_marks_present`) | better | three consecutive steps keep both clones Present, bound to the same panes, no creates, no closure. Without threads `seat_resolved_binds_and_marks_present` shows the matcher loses the clone. |
| Pane renumbering (cold restart, new pane ids, tokens and terminal ids dropped) | pass 1/2 find nothing; clone Unknown until `clone rebind` | `restart_without_tokens_or_terminal_ids_is_unknown_not_recreated` | `parity_pane_renumbering_after_cold_restart` | better | two clones, each Resolved at its renumbered pane: Present at the right pane in the new incarnation with no operator step. |
| Native session id | pass 3 (current native id equals the pane's agent session) | `herdr_resumed_agent_adopted_as_occupancy_start` | `parity_cold_restart_with_resumed_session_reattaches_by_seat` | better | the pane's agent exposes no session id to Herdr; C1 resume check-in reattaches the seat and the clone binds. |
| Agent resumed in another pane | pass 3 moves the clone to the pane showing its session | (pass 3 has no dedicated observe test) | `parity_agent_resumed_in_another_pane_follows_the_seat` | better | seat Resolved at the user-added pane: the clone follows it; the old bare pane stays unclaimed. |
| Duplicate / ambiguous candidates | pass 2 `break` takes the first clone in id order; pass 3 `find` takes the first clone holding a session id | none (arbitrary tie-break, not asserted) | `parity_duplicate_candidates_pane_goes_to_exactly_one_clone`; plus existing `threads_resolved_pane_is_withheld_from_a_matcher_bound_clone` | better | one seat Resolved, the other KeyConflict at the same pane: the pane goes to exactly one clone, the other is Unknown with a key-conflict fact, nothing closed. |
| Partial restore | pass 1/2/3 bind whatever still has evidence | `restart_with_preserved_ids_rebinds_tab_and_workspace_without_votes` (clone passes unchanged: Unknown) | `parity_partial_restore_binds_only_what_threads_resolved` | better | resumed clone Present at its new pane, the bare one Unknown and Active, nothing created or retired. |
| Cold restart, headless (no operator) | no evidence: clone Unknown, not recreated, not retired | `restart_without_tokens_or_terminal_ids_is_unknown_not_recreated`, `restart_with_a_missing_tab_id_is_unknown_not_voted`, `e2e_herdr_restart_rebind_no_mass_retirement_no_duplicates` | `parity_cold_restart_headless_bare_pane_is_unknown_not_recreated` | same | bare pane stays Unresolved until D1: Unknown, Active, `Unresolved` fact, no create. |
| Cold restart, tokens survive | pass 1 token binds the clone | `herdr_restart_no_mass_retirement` (`restart(true, true)`) | `parity_cold_restart_with_surviving_tokens_keeps_the_matcher_result` | same (worse after matcher removal) | pre-D1 precedence keeps the token result when threads says Unresolved. See finding F1. |
| Cold restart, attended `clone rebind` | pass 2 `adopted` branch; `LiveIndex::pane` rebind-adopted branch | `rebind_after_restart_restamps_token`, `rebind_before_the_first_post_restart_pass_survives_it`, `e2e_rebind_restamps_token_after_herdr_restart`, `e2e_restored_panes_resolve_without_env` | `parity_cold_restart_attended_rebind_adopts_the_pane` | same (worse after matcher removal) | Unresolved leaves the adoption in force. See finding F2. |
| Same-incarnation pane moved between tabs (known tab) | token/bound id follow the pane; diff flags `reload_required` | `move_sets_reload_required` | `parity_pane_moved_between_tabs_keeps_the_binding` | same | seat Resolved at the same pane; clone follows it into the other seat's tab, reload flagged, no closure. |
| Pane moved into an unknown tab | bound clone unbound, Unknown | `move_into_unknown_tab_unbinds` | `parity_pane_moved_into_an_unknown_tab_is_unbound` | same | tab ownership is still the matcher/diff's; threads does not rebind a clone into a tab graph does not know. |
| Tab moved between workspaces | tab/workspace links by bound id, not clone passes | `restart_with_a_missing_tab_id_is_unknown_not_voted`, `restart_with_a_missing_workspace_id_and_no_token_is_unknown`, `live_index_matches_bound_tab_and_workspace_across_incarnations` | none | n/a | tabs and workspaces carry no seat; spec section 6 scopes SeatObservation to clone (pane) identity. The fake has no tab-move operation either. Tab and workspace matching is unchanged by this epic. |
| Pane closed, daemon running | diff reports the pane closed, `ObservedPaneClose` cascade | `pane_close_retires_clone_only`, `graph_issued_close_not_double_retired` | `parity_pane_close_retires_the_clone_once` | same | threads Retired plus the closed pane retire the clone once (one cascade action, not two), sibling untouched, not recreated. |
| Pane closed while the daemon was down | rebind pass cannot tell closed from restarted: Unknown, never silent retirement | `close_while_daemon_down_is_unknown_not_recreated` | `parity_close_while_daemon_down_is_decided_by_threads`; `parity_close_while_daemon_down_with_threads_unreachable_is_unknown` | better | threads knows the seat retired: the clone is retired, undoably (undo returns it Active). Behaviour change from "Unknown": intended by the spec; with threads unreachable the old Unknown answer stands. |
| Herdr disconnect / event loss | clones Unknown, bindings kept, rebind restores | `disconnect_marks_unknown_never_retires`, `event_loss_recovered_by_snapshot` | `parity_herdr_disconnect_marks_unknown_never_retires` | same | threads Resolved does not override a missing snapshot; the rebind pass restores presence with no create. |
| Threads unreachable | n/a to the old matcher | none | existing `threads_unreachable_changes_no_binding`, `parity_close_while_daemon_down_with_threads_unreachable_is_unknown` | n/a (new) | the matcher's result stands, no invented state. |
| `LiveIndex::pane` bound-id branch (same incarnation) | planner locates an existing clone pane | reconcile tests that settle with an existing pane | none | n/a | planner effects (create, adopt, close) read desired state and the snapshot only; no planner path reads `threads_seat` or `seat_fact`, and the observer owns the binding SeatObservation writes. |
| `LiveIndex::pane` rebind-adopted branch | planner honours a Present binding with a token-less pane | `rebind_after_restart_restamps_token` | `parity_cold_restart_attended_rebind_adopts_the_pane` (observer side) | n/a (planner) / same (observer) | the planner branch is untouched and keeps working because the observer keeps the adoption. |
| `LiveIndex::pane` journal `live_ref` + `pane_claimed_by` | planner finds a just-created pane before it is stamped; a pane another clone's binding claims is not this clone's | `adopted_pane_of_retired_clone_is_not_closed` | none | n/a | a clone being created has no verified binding, so no `threads_seat` (resolve-seat fires only on verified bindings); creation-time identity stays journal-based. |
| Create nonce (lost-response create adopted) | planner correlates a created tab/workspace by nonce label | `lost_response_create_adopted_by_nonce`, `workspace_nonce_recovery_rejects_reused_ids_foreign_tokens_and_ambiguity`, `late_applied_*_create_is_closed_as_duplicate` | none | n/a | nonce recovery covers workspace and tab creates, which have no seat; a clone pane created in flight has no `threads_seat` until verified. |

## Findings (threads worse than the matcher)

Each case is "worse" only once the old matcher is removed (the matcher stays authoritative before D1, so nothing regresses
today). hg-66u.14 re-runs this table after D1 and must turn each item below into a better or same verdict.

- F1. Restart that keeps pane metadata, bare pane (shell harness clones, or an agent that has not checked in yet): the
  graph token binds the clone today (`parity_cold_restart_with_surviving_tokens_keeps_the_matcher_result`). Threads
  answers Unresolved for a pane with no resumed agent, so with the matcher removed the clone would be Unknown until the
  agent checks in. Expected closer: D1 (continuity key HERDR_THREADS_CONTINUITY_KEY reattaching a bare pane's seat).
- F2. Explicit `clone rebind --pane` after a cold restart (`parity_cold_restart_attended_rebind_adopts_the_pane`): the
  operator's adoption is a matcher branch (pass 2 `adopted`, `LiveIndex::pane` rebind-adopted). Threads has no operator
  "this pane is this seat" assertion, so without the matcher a bare pane could not be re-adopted. Expected closer: D1
  (reserve/bind the key to the seat, `ReserveContinuityKey{seat}`) or an explicit graph-side binding that survives matcher removal.
- F3. Headless cold restart with no evidence at all (`parity_cold_restart_headless_bare_pane_is_unknown_not_recreated`)
  is the same as the matcher (Unknown), but the matcher's pass 3 would recover a pane whose agent exposes the clone's
  session id to Herdr without any threads check-in. Threads recovers it only when the agent hooks check in (C1). Until D1
  the matcher covers it; after matcher removal an agent that never runs the threads hook (no check-in) stays Unknown.
  Expected closer: D1; not closable for harnesses that cannot check in.

No scenario was found where threads gives a wrong binding that the matcher got right; the cases above are all "threads
silent, matcher binds". Behaviour changes that are intended, not findings: a pane closed while the daemon was down now
retires the clone undoably (was Unknown); duplicate candidates are decided by threads' key conflict instead of id order.

## Not testable here

Real herdr-threads behaviour (C1 reattach, process continuity across a handoff, KeyConflict production) is simulated by the
fake view; the live check belongs to hg-66u.14 against a threads release carrying D1. e2e restart tests in
`tests/e2e_private_herdr.rs` are listed as old tests; they need a private Herdr rig and were not run for this task.
