# Final seam review: epic hg-66u (threads seat as the clone identity, graph side)

## Source and method

- Base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735, both verified
  with `git rev-parse --verify <sha>^{commit}`. Review source bound in the ledger (bind-review-source exit 0).
- Caller checkout observed clean on `super-auto/threads-seat-identity` at 6576a11 at the start and end of this review.
  No detached worktree was needed: all reads used `git show 6576a117:<path>` and `git diff base..head`.
- Inputs: the 20 cluster review files (clusters 1-3 and 5-21) and the clusters.json partition. Cluster 4
  (runs-top, 25 run documents including the spec itself) has no review file. Spec:
  `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md` at head.
  It has a `## Requirement baseline` section (B1-B11, human scope changes: none), which is the acceptance standard here.
- Bead tree at review time: gates hg-66u.1 (D1), hg-66u.2 (D2) and hg-66u.3 (G5) are open. These ungated beads
  are also open: hg-66u.28 (SeatObservation), hg-66u.29 (Restore intent and succession), hg-66u.30 (resume driver,
  session path), hg-66u.23 (parity check), hg-66u.26 (integration sweep) and hg-66u.17 (pre-D1 rig smoke). Their
  dependency lists contain no gate bead.
- I did not re-run the test suite. The two red test binaries are carried forward from cluster 21's reproduction at this head.

## Baseline trace (whole tree, from production entry points at head)

- B1 (1:1, threads authoritative): partly built. `CloneRecord.threads_seat` is write-once, and it is filled by the
  resolve-seat want (`clone_wants` -> `resolvable_pane` -> `ThreadsExecutor::resolve_seat` -> `admit_threads_seat`).
  But no production code reads seat state. `RuntimeLoop.seat_view` is `#[allow(dead_code)]`
  (src/observe/step.rs:118-120), and `SeatStatus` appears only in compose, ports, fake, mapping and tests. So the
  clone's location and liveness still come only from the graph's own matcher. `graph undo` also still adopts the
  caller's pane (src/undo/compensate.rs `restore_mutate`), which can bind a seated clone to a pane holding a
  different threads seat. Not delivered.
- B2 (hard dependency on threads accepted): this row is a permission, not a behaviour. The v3.0.0 pin landed, and
  the unreachable-threads paths defer rather than error (resolve `NotYet` -> `Deferred`; invite with no seat -> `Deferred`). Delivered.
- B3 (re-matching delegated to threads, as good or better): clone re-matching after a restart or handoff still
  comes only from `observe/matcher.rs`. The threads-driven path (hg-66u.28) is open and ungated. The matcher
  deletion (hg-66u.14/.15) waits on D1. Not delivered.
- B4 (unattended restarts): no reconciler rule relaunches a clone's agent after a headless cold restart. The
  restart trigger of the resume driver (hg-66u.30) is open and ungated; the fresh-agent path (hg-66u.21) waits on D1. Not delivered.
- B5 (unattended resume across `herdr update --handoff`): hg-66u.25 reconnects the daemon. However, in the composed
  live-handoff run, the pre-existing clones end with availability Unknown and generation-1 bindings, and need an
  operator rebind (cluster 21 reproduction). The cause is the same: no SeatObservation. Not delivered.
- B6 (Herdr unchanged): the only submodule moved is third_party/herdr-threads. The key reaches the agent through the
  existing `pane.send_text`/`pane.send_keys` path before `agent.start`. Delivered.
- B8 (agent exit keeps seat and memberships): traced end to end. A verified, stamped, Present binding produces the
  resolve-seat want, and `ThreadsSeatMap::resolve_seat` admits `Assign`. `membership_seat` and the desired/satisfied
  checks then key only on `threads_seat`, never on the occupant. An agent exit leaves the clone Present and active,
  so nothing is released. The migration is churn-free: while `threads_seat` is None, the release loop `continue`s
  and nothing is desired. Delivered. Caveat: `resolve_seat` currently reaches threads without the call guard (cluster 10).
- B7 (undoable retirement: reopen the tab, resume the session, unretire the seat): `write_restore_intent` has no
  production caller, and neither `do_resurrect_seat` nor `graph undo` writes a Restore intent. There is no
  succession want or clear-on-reattach want, and no restore launch trigger. A restored clone keeps its dead
  final-retired seat: `resolvable_pane` skips it because `threads_seat` is set, and its Required links count as
  satisfied because `same_seat` holds. So it never regains live memberships. The pre-D2 interim (hg-66u.29, hg-66u.30)
  is ungated and open. Not delivered.
- B9 (continuity key): every start site passes `agent_env(&clone, false)`, so no production start exports
  `HERDR_THREADS_CONTINUITY_KEY`. The reservation port, field and want (hg-66u.43) and keyed launches (hg-66u.44)
  wait on gate hg-66u.1. This is correctly not faked. Not delivered, and no human scope change row covers it.
- B10 (hands-off undo, memberships restored on un-retire): the unretire and finalize wants (hg-66u.31) and the
  restore via un-retire (hg-66u.32) wait on gate hg-66u.2. Not delivered, and no scope change row covers it.
- B11 (/seat stops printing accept commands): `accept_command` is gone from src and skills; only negative
  assertions remain in src/bootstrap/tests.rs. The skill delegates acceptance to herdr-threads. Delivered, with
  cluster 6's wording item carried below.

## Cross-cluster seams

- Observation and reconcile: the threads cluster produces `threads_seat` and the seat-view port, but neither the
  observe cluster nor the reconcile cluster consumes them. This one gap (hg-66u.28 and hg-66u.30 not on head) is
  behind B1, B3, B4 and B5 together. No single cluster reviewer could see that this one missing bead causes all four rows to fail.
- Plan/undo and threads intent: the plan kinds (cluster 14) and undo (cluster 20) call `clear_restore_on_retire`
  and `purge_tombstone`, but nothing calls `write_restore_intent`. The threads cluster's `SeatTransition`
  acknowledgement arms (cluster 18) are therefore reachable only from tests. The interface agrees, but the producer is missing.
- Purge and the threads wants: before D2, purge writes a `Finalize` tombstone for every seated clone (§14.2 says
  "after D2"; §8 says final seats are purged at once with no intent). No want consumes it until hg-66u.31, and that
  bead is D2-gated. The tombstones are inert everywhere they are checked: `membership_seat`, both live-holder checks,
  `clone_status`, undo preview and resurrection. But they keep `name` and `seat`, so they pile up and keep names occupied until D2 ships.
- Launch env and export detection: `record_launch_env` (cluster 16) keys on the Herdr pane id, and `check_export`
  (cluster 19) compares that id with the hook's `HERDR_PANE_ID`. That is the same convention `/seat` resolution
  already uses, so the interfaces agree. The key branch stays dormant until hg-66u.44.
- Reconnect and the write-once seat: the `threads reconnect --from legacy` mutation (src/threads/reconnect.rs)
  clears invitations, opt-outs and thread ids, but keeps `threads_seat`, and ThreadsSeatAssignment forbids
  rewriting it. Before this epic, invite re-resolved the seat (`seat_for`) on every call. Now it invites the
  recorded seat, and `resolvable_pane` never re-resolves a seated clone. If a legacy-to-node reconnect does not
  preserve seat ids, every clone would invite a seat the new node does not know, with no recovery path. I could not
  confirm from the diff whether seat ids survive, so this is listed under Deferred OK as a check to make, not a confirmed defect.
- Test rig and resolve want: hg-66u.7's resolve-seat effects stay Pending in the integration sweep's wiring rig,
  because its FakePaneSeatMap has no seats for the created panes. This breaks a test outside the cluster that added the want (cluster 21).

## Cluster verdicts carried

- Ready: clusters 1, 2, 3, 5, 6, 7, 8, 9, 11, 12, 15, 17 and 19.
- Not ready: cluster 10 (LazyMap guard), cluster 13 (B1/B3), cluster 14 (B7/B10 resurrection), cluster 16
  (B4/B5/B7/B9 launch path), cluster 18 (B7/B9/B10 wants), cluster 20 (B1/B7/B10 undo) and cluster 21 (two red test binaries, B5).
- Unreviewed: cluster 4 (runs-top), which went over the window.

## Verdict
not ready (baseline rows B1, B3, B4, B5, B7, B9, B10 undelivered; two test binaries red; LazyMap drops the threads call guard; cluster 4 unreviewed)

## Must fix before landing
- Cluster 4 (runs-top, 25 run documents including the spec) is unreviewed: its review went over the window; review it before landing.
- B1 not delivered: no production code consumes ThreadsSeatView or SeatStatus (src/observe/step.rs:118-120 `seat_view` is `#[allow(dead_code)]`); land SeatObservation (hg-66u.28, ungated) with the per-clone pre-D1 precedence (clusters 13, 21, seam).
- B1 broken by undo: `restore_mutate` (src/undo/compensate.rs) still adopts the caller's pane through `adoption`/`displace`, which can bind a seated clone to a pane holding a different threads seat; remove adoption for clones per spec section 8 (hg-66u.29 scope, cluster 20).
- B3 not delivered: clone re-matching after restart or handoff still comes only from observe/matcher.rs; delivered by the same SeatObservation landing (hg-66u.28), not by keeping a matcher fallback (cluster 13).
- B4 not delivered: no reconciler path relaunches a clone's agent after a headless cold restart; land the resume driver session path (hg-66u.30, ungated), and list the fresh-agent path (hg-66u.21, gate hg-66u.1) open in the hand-back (cluster 16).
- B5 not delivered: after a composed live handoff the pre-existing clones stay availability=Unknown with generation-1 bindings and need an operator rebind; land hg-66u.28 and hg-66u.30 and assert post-handoff clone recovery in tests/daemon_composed.rs (clusters 16, 21).
- B7 not delivered: `write_restore_intent` has no production caller, so neither `do_resurrect_seat` (src/plan/core_kinds.rs ~503-547) nor `graph undo` writes a Restore intent; ThreadsSource has no succession want or clear-on-reattach want, and the reconciler has no restore launch trigger, so a restored clone keeps its dead final-retired seat with no live memberships; land hg-66u.29 and the restore arm of hg-66u.30 (clusters 14, 16, 18, 20).
- B9 not delivered: every start site passes `agent_env(&clone, false)` (src/reconcile/herdr_exec.rs start, lost-response branch, replace_session), and there is no ReserveContinuityKey port, reservation field or reserve-key want; hg-66u.43/hg-66u.44 wait on gate hg-66u.1 (D1 unreleased); hand it back open and do not emit an unreserved key (clusters 16, 18).
- B10 not delivered: there are no unretire or finalize wants derived from SeatIntent and no un-retire restore; hg-66u.31/hg-66u.32 wait on gate hg-66u.2 (D2 unreleased); no human scope change row covers it; hand it back open and do not fake it (clusters 14, 18, 20).
- src/daemon/compose.rs:914-978 `LazyMap::map()` drops `_guard` (the `threads-calls.lock` flock plus epoch check) before the delegated call, so `seat_for` (guarded at base), `resolve_seat` (which feeds the write-once `threads_seat`) and `inspect` reach threads unguarded and a ThreadsReconnect can commit mid-call; keep the guard alive across each call (cluster 10).
- tests/instruction_writers.rs `instruction_source_writers_are_inventoried` fails at head: the clone purge `delete_file` in src/plan/core_kinds.rs (~1925, hg-66u.35) is an uninventoried store writer; add it with its reason (cluster 21).
- tests/integration_sweep.rs `wiring::sweep_action_and_effect_envelopes_round_trip` fails at head: hg-66u.7's `threads.resolve_seat` effects stay Pending ("no seat for pane p2/p3") in the wiring rig, so the creation-settle wait times out; give the rig's FakePaneSeatMap seats for the created panes or exclude resolve-seat effects from the settle predicate (cluster 21).

## Untested scope
- No test shows a `LazyMap` call holding `threads-calls.lock` for its whole duration (an exclusive holder blocking `seat_for`/`resolve_seat`/`inspect`) (cluster 10).
- The clear-on-success branch of `LazyMap::inspect`'s `seat_view_error` slot is not exercised; only a simulated Rejected path is (cluster 10).
- No composed-daemon test reaches the `clones` status component through `status`, and its `store.head()` error and `all_clones(...).unwrap_or_default()` branches are untested (clusters 1, 10).
- The v3.0.0 pin was checked in cluster 1 only for lock consistency; the build and `tests/threads_real.rs` against v3.0.0 were not re-run in any final review.
- No test checks skills/seat/SKILL.md against the herdr-threads output it cites; only installed-copy equality is asserted (cluster 6).
- No CLI-level test runs `herdr-graph seat` in text mode through the daemon (cluster 7).
- `report_args` with `Some(continuity_key)` -> `session.report` `continuity_key` field is untested; every call passes None (cluster 8).
- `Env::from_process` reading `HERDR_THREADS_CONTINUITY_KEY` has no direct test (cluster 9).
- `HerdrClient::start_agent` export-step failure arms (send_text/send_keys Timeout, Rejected, text sent but Enter failed) are untested (cluster 11).
- Leading-space history suppression and `export K=V` across bash, zsh and fish are unexercised; the private rig uses one shell (clusters 11, 21).
- `private_start_agent_env_reaches_the_agent` and the `herdr::` unit tests were not re-run in the final review (cluster 11).
- No model test parses a pre-change CloneRecord without `threads_seat`/`threads_seat_history`/`seat_intent`, or a Restore with `threads_done`/`launched` absent (cluster 12).
- Stale or reissued positional tab and workspace ids across a cold restart are unmodelled, because FakeHerdr allocates ids monotonically; tab id preservation across a real Herdr cold restart is unverified, with hg-66u.17 open (clusters 13, 16).
- Observed closure cascade clearing a pending (threads_done = false) Restore intent is untested (cluster 13).
- `clear_restore_on_retire` in `do_retire_seat` and `CloneRetire`, the `clone purge <name>` success path, name ambiguity between a retired clone and a tombstone, the ReleaseKey purge branch and a lifecycle change between purge plan and apply are untested (cluster 14).
- `ThreadsSeatMap::inspect` with a batch where a later seat fails after earlier ones succeed is untested; the fake has no per-seat error injection (clusters 15, 18).
- No test pins the serde shape of `SeatStatus`/`RetirementKind` (cluster 15).
- No reconcile test runs after `FakeHerdr::live_handoff` itself; the cross-incarnation LiveIndex test uses `restart(false, false)` (cluster 16).
- No test covers a Herdr start without session restore, where an older-incarnation bound id could name an unrelated live object (cluster 16).
- The lost-response branch of `start` recording `launch_env`, and `record_launch_env` after `replace_session`, are not asserted on the meta (cluster 16).
- Guard-before-start ordering (process_info before export and agent.start) is not asserted; the fake does not record ProcessInfo (cluster 16).
- No store test commits and reads back a CloneRecord with `threads_seat`, `threads_seat_history` and `seat_intent` populated (cluster 17).
- Required release through the `!wanted` branch (channel thread id changes while the seat stays) is untested (cluster 18).
- Membership behaviour of an active clone of a Dormant seat or teamspace is untested (cluster 18).
- `ThreadsSeatMap::resolve_seat` TargetUnresolved -> NotYet is tested only against the fake (cluster 18).
- Writer rejection of an admitted Assign after `resolve_seat` returned Done is unobserved (cluster 18).
- No test sends a spooled report through `ingest_spool` into `check_export`, covers out-of-order spooled and live reports, or covers `status_json` for a retired or tombstoned clone with an `export_rejected` record (cluster 19).
- The end-to-end seam where `record_launch_env` writes the record and a hook report reads it back is untested; cluster tests write `launch_env` by hand (cluster 19, seam).
- The continuity-key branch of `check_export` runs only under a faked launch record until hg-66u.44 (cluster 19).
- No test undoes a closure cascade where only one of several retired clones was later purged, shows `displace`'s `clear_restore_on_retire` dropping a Restore intent, or pins whether undo candidates list purged-clone actions (cluster 20).
- tests/daemon_composed.rs live-handoff test asserts `SplitPane <= 1` with a stale comment and never asserts post-handoff availability or binding of pre-existing clones (cluster 21).
- No test covers a pane shell that rejects `export` (the agent starts without the key, falling back to C1) (cluster 21).
- No automated check binds the committed review record copies to their run.md finding-source entries (cluster 5).
- `ThreadsReconnect` from legacy to a node with clones that already hold `threads_seat` is untested; no test shows whether participation recovers after the reconnect (seam).

## Deferred OK
- CURRENT-DESIGN.md and DESIGN-NOTES.md describe post-D2 membership lifetime ("end at purge") with no pre-D2 caveat, omit the ReleaseKey tombstone, and carry a stale "updated 2026-10-18" date (cluster 1).
- Before D2, purge writes Finalize tombstones for every seated clone (against spec section 8's "purged at once, with no intent"), and nothing consumes them until hg-66u.31; they are inert but keep `name` and `seat`, so they pile up and occupy names until D2 ships; decide whether pre-D2 purge should delete outright (clusters 1, 14, seam).
- Align the H2-identity review doc with the spec (env var name, re-invite wording, "or policy"), and set the INDEX.md status at finish to one that says the gated subtrees remain (cluster 2).
- Tighten MVP design §7.1 opt-out wording, mark its stale sentences superseded, and give the spec cross-reference a repo path (cluster 3).
- Review-record citation drift for the `agent_env` sites, absolute home paths in committed records, superseded B1/B8/pin statements, and a blank line before one record's verdict (cluster 5).
- skills/seat/SKILL.md step 4 says `thread participants` gives an `accept-required` command, but it prints only the values; reword. The bare `herdr-threads` fallback prefix can miss `--state-dir`/`--host-endpoint` (cluster 6).
- Stale `ThreadsLink` doc comment (src/model/clone.rs:81-82) still mentions printing the accept-required command (clusters 7, 12).
- A spool written before this change has no `continuity_key` and can raise a stale export-rejected alert until the next complete report (cluster 8).
- Reference `model::launch::ENV_CONTINUITY_KEY`/`ENV_CLONE` from `Env::from_process` instead of string literals (cluster 9).
- `seat_view_error` is also filled by endpoint-setup Rejected errors; `clone_status` hides a store read error behind an empty list; `Services::new` defaults to `FakeThreadsSeatView` in non-test code (cluster 10).
- `ThreadsReconnect` keeps `threads_seat` while clearing invitations, and invites no longer re-resolve; check whether a legacy-to-node reconnect preserves seat ids, and if it does not, make reconnect a sanctioned reset of `threads_seat` so the resolve want runs again (clusters 10, 18, seam).
- Enter failing after a successful export `send_text` leaves text on the input line; FakeHerdr::start_agent neither validates env nor records exported vars; rustdoc paragraph merge on `start_agent`; `renumber_tabs_and_workspaces` naming (cluster 11).
- Spec text says `Option<SeatId>` where the code correctly uses `Option<String>`; `key_reserved = false` until hg-66u.13; downgrade silently drops the new fields (cluster 12).
- Decide whether cross-incarnation bound-id equality for tabs and workspaces needs a corroborating signal before trusting a recreated id (cluster 13).
- Purge name lookup matches tombstones (reused names can be ambiguous); both `purge_tombstone(&clone, None)` call sites must take the recorded reservation when hg-66u.43 lands (cluster 14).
- Before SeatObservation consumes `ThreadsSeatView::inspect`, make per-seat rejection (v3.0.0 NotFound mapped to Rejected) a per-seat outcome rather than a whole-batch failure; tighten the inspect doc; state the env refusal contract on the `StartAgent.env` port doc (clusters 15, 18).
- `LiveIndex::workspace` returns early for an older-incarnation id before the live-ref and create-nonce checks; `tab_for_seat` falls through for an older-incarnation id owned by another seat; switch all three `agent_env(&clone, false)` sites together in hg-66u.44 (cluster 16).
- Dormant-seat Required memberships are not released; Ordinary re-invite against a final-retired seat for retired-unpurged clones on v3.0.0 (inferred); no writer-level validator for one live clone per seat outside ThreadsSeatMutation; `resolve_seat` reports Done before admission is known; `clone_wants` lost its doc comment (cluster 18).
- Codex clones have no graph check-in, so a rejected export is never named; `export_rejections` lists retired clones and tombstones, and the `launch_env:`/`export_rejected:` meta is never cleaned; `check_export` ignores report time and incarnation (cluster 19).
- Remove the unreachable "no longer exists" branch in `compute_restore` (src/undo/preview.rs ~216), and optionally mark purged-clone actions not undoable (cluster 20).
- The `env: vec![]` additions in herdr_spikes.rs and herdr_client.rs are mechanical, and shell-history behaviour depends on shell configuration (cluster 21).
- The D1/D2/G5-gated subtree (hg-66u.14, .15, .18, .21, .24, .31, .32, .33, .40, .43, .44) stays unexecuted until its gate releases; list each blocked row open in the hand-back per the epic's global constraint (spec section 10).
