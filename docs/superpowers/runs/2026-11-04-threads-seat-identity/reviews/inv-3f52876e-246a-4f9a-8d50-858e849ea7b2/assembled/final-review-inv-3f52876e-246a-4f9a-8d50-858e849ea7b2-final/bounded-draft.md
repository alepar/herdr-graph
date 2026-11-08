# Final review (bounded): hg-66u, threads seat as the clone identity

Range: 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784..1e0b2e35616bbc89a7175658dc329a7d1bc57802 (branch super-auto/threads-seat-identity, 186 commits, 427 changed paths). Invocation inv-3f52876e-246a-4f9a-8d50-858e849ea7b2.

## Review route

The review-input sidecar reports status unavailable, sourceAvailable true, wholeRangeReady false, omitted 0 and two retained provenance gaps, so this is a bounded captured-source review, not a whole-range review. The failed assembly and the original sidecar were left untouched. I read the full changed-path inventory, then the epic (bd show hg-66u), the spec at head (docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md, including the Requirement baseline B1..B11), the child bead states, and the captured diff hunks listed in bounded-coverage.json: 39 product paths and 129 hunks, covering the seat observation, seat assignment, resume driver, keyed launch, undo, purge and production composition paths. Test files, most of src/threads/effects.rs, src/threads/mapping.rs, src/threads/adapter.rs, src/daemon/clone_status.rs and the run documents were not reviewed hunk by hunk; they are listed under Untested scope. No full-suite run of this head exists yet; the caller runs it after this invocation, so the branch is not treated as tested.

## Requirement baseline trace

- B1 (1:1 mapping, threads authoritative): threads_seat is write-once with an intent-gated succession exception and a one-live-clone-per-seat check (src/threads/assignment.rs). SeatObservation (src/observe/seat_observation.rs) overlays threads' Resolved, Retired and KeyConflict answers on the matcher result in RuntimeLoop::step_once. Threads takes precedence for seated clones, but the graph's own clone re-matching is still in place, because hg-66u.14 is open and blocked on a human decision.
- B2 (hard dependency on threads): every agent start now waits for a recorded key reservation (start_needed, plan_resume in src/reconcile/planner.rs). When threads is unreachable, the code changes no binding and launches nothing. This matches the row.
- B3 (re-matching as good or better before delegating): per the hg-66u.14 BLOCKED record, the D1 parity re-run found two classes still worse without the matcher: Shell-harness clones and agents that never check in. Keeping the matcher respects B3's precondition, but the spec's deletion decision is unexecuted and needs the human's call.
- B4 / B5 (frictionless restart and herdr update --handoff): the resume driver (plan_resume restart arm, RelaunchOccupant with the 90 s grace), the preserved-id tab/workspace matching and FakeHerdr::live_handoff exist. Real-rig evidence is missing: the ledger records hg-66u.17's live-handoff configuration as unmet (the skipped test reports ok), its agent-bearing cold-restart parts as unmet, and hg-66u.18 (post-gates smoke) is open.
- B6 (Herdr unchanged): env delivery uses a shell export typed before agent.start (src/herdr/client.rs export_line), with no Herdr change. Delivered.
- B7 / B10 (undoable retirement that unretires the seat and restores memberships hands-off): the graph logic exists (Restore intent, unretirable want, ThreadsExecutor::unretire_seat, ServiceThreads::unretire_seat). The production composition does not forward the call: compose.rs LazyThreads implements no unretire_seat, so the trait default returns Unsupported. Traced from `herdr-graph undo` to the effect outcome Failed, this row is not delivered in production (see Must fix).
- B8 (agent exit keeps seat and memberships): membership_seat in src/threads/effects.rs is keyed to threads_seat, and occupancy and retirement release nothing. Delivered in the code reviewed.
- B9 (continuity key): reservation, key-link want, keyed env HERDR_THREADS_CONTINUITY_KEY=<cl_ id> (src/model/launch.rs agent_env), and reserve_continuity_key forwarded by LazyThreads. Delivered in code; no live 3.1.0 run (ledger hg-66u.43, hg-66u.44).
- B11 (/seat stops printing accept commands): render_seat and skills/seat/SKILL.md now delegate acceptance to herdr-threads. Delivered.

## Cross-task seams

- Production port wiring vs. fake-tested D2 (hg-66u.31, hg-66u.32, hg-66u.35, hg-66u.43): hg-66u.43 added reserve_continuity_key to both ServiceThreads and the LazyThreads wrapper. hg-66u.31 added unretire_seat, finalize_retirement and release_continuity_key to ServiceThreads only (src/threads/adapter.rs:573-612). Every D2 test drives FakeThreads or ServiceThreads directly (commit 1812c76 tests use crate::threads::fake::FakeThreads), so the missing forwarding is invisible to the suite. In production, port_outcome maps Unsupported to ExecOutcome::Failed("operation unsupported by this threads service"), and record_refusal runs only for Rejected. The stuck restore therefore never gets a last_refusal, and per the ledger (hg-66u.58 status-coverage-gap) it is not named in stuck_restores either.
- The restore launch in plan_resume requires threads_done true. With the unretire effect Failed, an undone clone's tab and pane are reopened but no agent is ever launched, and the seat stays retired. Purge tombstones (Finalize / ReleaseKey) never complete, so continuity keys are never released.
- SeatObservation still runs its pre-D1 precedence post-D1 (the module doc and the Unresolved arm defer to the matcher). That is consistent with hg-66u.14 being open, but it means the "single source of clone location" design is not yet true.
- Minor: in src/threads/effects.rs the doc comment "Resolve the clone's pane to its threads seat and record it write-once" sits above clear_restore instead of resolve_seat, and the plan_resume doc still says "only the session path exists before D1".

## Ledger triage

The `Recurring blocker` (merge check on a host without a C toolchain, hg-66u.5/.11/.27) is environmental and was resolved by the hg-build toolbox; it is not a product defect. The `Recurring minor` missing-evidence cluster (x15 tasks) is process-class: it makes the per-task pass claims unverifiable but names no product defect; it is retained as an evidence gap under Untested scope. Some deferred items name skipped tests, known defects or unmet acceptance; I graded those as must-fix, below. The remaining deferred items are listed under Deferred OK. No parked or BLOCKED-AUTH lines were found. One task escalated: hg-66u.14 (BLOCKED, needs a human decision).

## Verdict
not ready (production LazyThreads drops D2 unretire/finalize/release so B7/B10 fail; hg-66u.14 and hg-66u.18 open; bounded review, no full-suite run)

## Must fix before landing
- [Critical, B7, B10] src/daemon/compose.rs `impl ThreadsPort for LazyThreads` (head ~line 839) forwards reserve_continuity_key but not unretire_seat, finalize_retirement or release_continuity_key, so the trait defaults in src/ports/threads.rs:213-231 return Unsupported and ThreadsExecutor turns that into Failed; a production `herdr-graph undo` or `seat resurrect` of a reversibly retired seat never unretires it, the Restore stays threads_done=false and the agent is never relaunched; fix by forwarding the three calls under the endpoint guard like reserve_continuity_key, and add a composed test through production_ports.
- [Important] the same gap leaves every purge tombstone (Finalize{seat} / ReleaseKey) failing forever in production, so seats are never finalized and continuity keys never released; the ledger's hg-66u.31 stuck-tombstone item (no operator drop for a Finalize tombstone threads does not know) compounds it and is a known product defect, not a minor.
- [Important, B1, B3] hg-66u.14 is open and BLOCKED on a human decision: the clone matcher passes and the LiveIndex clone re-matching are not deleted (the spec's key decision and §6), because the D1 parity re-run found Shell-harness clones and agents that never check in worse without the matcher; the human must pick waive B3 for those classes, keep the matcher and hand back the parity items, or amend the spec.
- [Important, B4, B5, B7] no real-rig acceptance exists for the post-gate behaviour: hg-66u.18 (fresh agent reattached by key after cold restart, pane closed during downtime, close plus undo keeps seat id and memberships) is open; the ledger records hg-66u.17 live handoff as unmet with a skipped test that reports ok, the agent-bearing cold-restart parts as unmet, hg-66u.21 fresh-agent reattach as unmet and hg-66u.32 real-rig undo as unmet; run them or hand each back as an open acceptance item, not a pass.
- [Important] ledger hg-66u.17 name-overstates-coverage: tests/e2e_private_herdr.rs e2e_cold_restart_headless_resumes_via_the_driver passes without exercising the resume driver (shell clones, no agents); rename it or make it drive an agent clone so it is not read as B4 evidence.

## Untested scope
- No full-suite measurement of head 1e0b2e35616bbc89a7175658dc329a7d1bc57802 exists; the caller runs the sweep after this invocation.
- Provenance gap: 1e0b2e35616bbc89a7175658dc329a7d1bc57802:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:82 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete claimed collection.
- Provenance gap: 1e0b2e35616bbc89a7175658dc329a7d1bc57802:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:82 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete split bundle: missing cluster report or slices.
- Bounded review: a whole-range review remains outstanding; only the 129 hunks in bounded-coverage.json were inspected.
- Unreviewed product hunks: src/threads/effects.rs (all hunks except the clone_wants ones), src/threads/mapping.rs, src/threads/adapter.rs, src/threads/fake.rs, src/threads/mod.rs, src/daemon/clone_status.rs, src/daemon/compose.rs mod fakes hunks, src/plan/kinds_extra.rs, src/transcripts/capture.rs, src/transcripts/mod.rs, src/undo/preview.rs (all but the first hunk), src/undo/adopt.rs (deleted), scripts/herdr-release-smoke.
- Unreviewed tests: src/observe/tests.rs, src/threads/tests.rs, src/threads/service_tests.rs, src/reconcile/tests.rs, src/plan/tests.rs, src/model/tests.rs, src/herdr/tests.rs, src/bootstrap/tests.rs, src/undo/tests.rs, src/transcripts/tests.rs, src/store/tests.rs, tests/daemon_composed.rs, tests/e2e_private_herdr.rs, tests/integration_sweep.rs, tests/herdr_client.rs, tests/herdr_spikes.rs, tests/instruction_writers.rs.
- Unreviewed docs and evidence: CURRENT-DESIGN.md, DESIGN-NOTES.md, GRAPH_STATE.md, README.md, docs/reviews/2026-10-29-seat-review-H2-identity-design.md, docs/verification-matrix.md, docs/superpowers/specs/INDEX.md, evidence/hg-66u.23, evidence/hg-66u.24, evidence/hg-66u.31, and the docs/superpowers/runs/** run documents.
- Live herdr-threads 3.1.0 checks (tests/threads_real.rs) for ReserveContinuityKey, UnretireSeat, FinalizeRetirement and the G5 disposition were not run (ledger hg-66u.24, hg-66u.31, hg-66u.43, hg-66u.44).
- B5 test sensitivity: ledger hg-66u.66 notes tests/daemon_composed.rs assert_clones_recover can pass before the loop processes the new incarnation and has no mutation check.
- Recurring missing-evidence cluster (x15 tasks): per-task pass counts have no retained logs or source-tree-bound evidence records.
- Shell-harness clones and agents that never check in still depend on the old matcher after a cold restart (hg-66u.14 parity re-run).

## Deferred OK
- Recurring blocker: merge check on a host without a C toolchain (hg-66u.5, hg-66u.11, hg-66u.27); environmental, resolved by the hg-build toolbox.
- hg-66u.8 ordinary-invite-succession: the old final-retired seat keeps its Ordinary membership after succession; low impact.
- hg-66u.8 unverified-assumption: retired-unpurged clones get invite wants only while active (the head code gates invites on clone_active), so the concern is moot at head.
- hg-66u.7 async-admission-unobserved and duplicated held-elsewhere pre-check in resolve_seat; check_free still enforces the invariant.
- hg-66u.7 doc-drift and the misplaced resolve_seat doc comment above clear_restore in src/threads/effects.rs; cosmetic.
- hg-66u.43 string-matched Conflict detection in src/threads/adapter.rs:57; fragile but correct today, follow-up for a typed code.
- hg-66u.40 host-unaware conflict pane mapping and stale Retired-arm comment in src/threads/mapping.rs; follow-up.
- hg-66u.30 async MarkLaunched admit window; later retries go Obsolete, one launch per epoch holds.
- hg-66u.58 stale restore_mark_failed meta after retire; epoch-filtered in status.
- hg-66u.81 dead not_launchable branch in src/daemon/clone_status.rs; harmless.
- hg-66u.15 legacy RequestKind::CloneRebind variant kept for journal deserialization; intentional.
- hg-66u.15 deferred-cleanup of the matcher adopted-binding branch; belongs to hg-66u.14.
- hg-66u.78 restart test models live handoff, not renumbered pane ids; the renumbered case is unrecoverable by spec §9.
- hg-66u.23 'Tab moved between workspaces' recorded n/a in parity.md; tabs carry no seat, acceptable.
- hg-66u.11 older-incarnation bound tab owned by another seat falls through to fallbacks; disclosed edge.
- Test-quality minors (hg-66u.61 timing-based guard test, hg-66u.60 unused binding and weak lost-message assertion, hg-66u.57 vacuous resurrect assertion, hg-66u.59 fixture-simulated undo, hg-66u.64 unobservable write test, hg-66u.69 weak setup, hg-66u.24 Retired kind not asserted, hg-66u.68 fixture-modelled alert, hg-66u.32 simulated C1 reattach); none hides a product defect found here.
- Process minors (tdd-order, missing RED runs, formatting churn, scope-beyond-file-list, test placement, signature drift, stale line references, untracked submodule copies, committed evidence logs); no product impact.
