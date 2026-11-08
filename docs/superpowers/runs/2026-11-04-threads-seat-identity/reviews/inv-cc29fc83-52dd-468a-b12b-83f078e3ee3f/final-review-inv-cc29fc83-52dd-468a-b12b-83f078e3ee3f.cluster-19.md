# Final review, cluster 19 (tests), epic hg-66u

Invocation inv-cc29fc83-52dd-468a-b12b-83f078e3ee3f. Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head e4984a492e2d3ce8344b9be83d56a3471049fd60 (both verified with `git rev-parse --verify`). Caller checkout observed at e4984a4, clean, unchanged during the review.

Inputs: slices 000064-000068 (tests/daemon_composed.rs, tests/herdr_client.rs, tests/herdr_spikes.rs, tests/instruction_writers.rs, tests/integration_sweep.rs), digest.md, spec docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md (has a `## Requirement baseline`, rows B1-B11, no human scope changes).

## Method

- Review-owned detached worktree /var/home/alepar/AleCode/herdr-graph/.worktrees/review-c19-cc29fc83 at e4984a4, submodule initialised from the caller's clone (pin cdc8423, herdr-threads v2.6.4), target reflink-seeded. All cargo runs in the `ht-build` toolbox.
- Ran: `cargo test --test daemon_composed live_handoff` (pass), `cargo test --test integration_sweep --test instruction_writers` (instruction_writers FAILS), `cargo test --features private-herdr --test herdr_client -- private_start_agent_env_reaches_the_agent private_agent_absent_and_rejections` (pass against a real private herdr server; no SKIPPED line, so not skipped).
- A temporary probe (reverted afterwards; worktree clean) added eprintln of each clone's `runtime.availability` and `runtime.bound` before and after the two `live_handoff()` calls in the live-handoff test.

## Per-file analysis

- tests/instruction_writers.rs: the inventory gains `src/threads/assignment.rs` (TombstoneDone `delete_file` of a clone record at assignment.rs:267, correctly described). But hg-66u.35 (commit 18a43c5) added `cx.tree.delete_file(&loc.record_path)` / `put_record` to `ClonePurge::mutate` in src/plan/core_kinds.rs:1925 without adding that file to the inventory. At head `instruction_source_writers_are_inventoried` fails: found set includes `src/plan/core_kinds.rs`, expected does not. The integration head is red. The writer itself only touches the clone record path, so the fix is an inventory entry with its reason, but the test must be green before landing.
- tests/integration_sweep.rs: `K::ClonePurge` added to the wiring kind list and the exhaustive match. Correct and passing; it keeps the sweep's exhaustiveness guard honest for the new kind.
- tests/herdr_spikes.rs and the `private_agent_absent_and_rejections` hunk of tests/herdr_client.rs: mechanical `env: vec![]` additions for the new `StartAgent.env` field. Fine.
- tests/herdr_client.rs `private_start_agent_env_reaches_the_agent`: real private-Herdr proof that the adapter's ` export K=V` line plus `agent.start` delivers both `HERDR_GRAPH_CLONE` and `HERDR_THREADS_CONTINUITY_KEY` into the started agent's environment (workspace env carries only PATH, so the export is the only source). This is solid evidence for the §5 delivery mechanism. Validation refusal of unsafe values is unit-tested outside this cluster (src/herdr/tests.rs `client_start_agent_refuses_unsafe_env_values`). The test is gated behind `--features private-herdr`, so default `cargo test` compiles it to zero tests.
- tests/daemon_composed.rs `live_handoff_under_a_running_loop_reconnects_and_creates_nothing`: proves the composed loop sees each new incarnation (two consecutive handoffs recorded in `incarnation:*` meta), resubscribes, and creates/closes/retires nothing. The section banner claims "same ids, new incarnation, no operator step", but the test never asserts that clones recover. The probe shows both clones `availability=Present` before the handoff and `availability=Unknown` after it, with `bound` still pointing at the generation-1 incarnation (`terminal_id` term2/term3, tokens of the old server), after settle plus 2 s. The only Herdr calls after the handoff are two `ReportWorkspaceMetadata` re-stamps of the workspace token (the hg-66u.11 preserved-id path working for teamspaces). The post-handoff `clone add sh` assertion is `SplitPane <= 1`, which also accepts the new clone never getting a pane (already a recorded minor from hg-66u.25).

## Requirement baseline trace for this cluster

- B5 (live handoff: "unattended frictionless resume when it happens ... both in threads and graph"): traced from the production composed daemon (the test drives the real reconcile/observe loop with a fake Herdr that models `herdr update --handoff` per spec: ids kept, terminal ids reallocated, tokens dropped). At head every clone goes Present -> Unknown and stays Unknown with no operator step that resolves it; a clone added afterwards may get no pane. The delivering work is open and ungated: hg-66u.4 (pin to v3.0.0; head still pins v2.6.4), hg-66u.6 (ThreadsSeatView port), hg-66u.7 (threads_seat assignment), hg-66u.28 (SeatObservation mapping). Not delivered; no human scope change covers it.
- B9 (continuity key): the adapter delivery path is proven end to end on a real private Herdr by this cluster. Whether production launches emit the key (reservation-gated, hg-66u.43/.44, D1 gate hg-66u.1 open) leaves this cluster and is the seam/launch reviewer's to trace.
- B7 (undoable retirement) and the purge window: this cluster only adds ClonePurge to the wiring sweep; the undo/un-retire path (hg-66u.32, D2 gate) is the seam reviewer's to trace.
- B1-B4, B6, B8, B10, B11: no test in this cluster exercises them; tracing belongs to the clusters that own those paths.

## Verdict
not ready (instruction_writers inventory test is red at head, and B5 live handoff leaves every clone Unknown)

## Must fix before landing
- tests/instruction_writers.rs `instruction_source_writers_are_inventoried` fails at head e4984a4: src/plan/core_kinds.rs (ClonePurge::mutate `delete_file`/`put_record` of the clone record, line 1925, added in 18a43c5 by hg-66u.35) is a new raw writer missing from the inventory; add it with its reason (writes only the purged clone's clone.toml, never an instruction path) so the suite is green.
- B5 not delivered: in tests/daemon_composed.rs live_handoff_under_a_running_loop_reconnects_and_creates_nothing, a probe at head shows both clones Present before `live_handoff()` and Unknown afterwards with the binding stuck on the old incarnation, so a live handoff is not resumed unattended; the ungated delivering beads hg-66u.4, hg-66u.6, hg-66u.7 and hg-66u.28 are still open, and the test must then assert the clones return to Present bound to their preserved pane ids.

## Untested scope
- The live-handoff composed test asserts no duplicates and no closures but never asserts clone availability or binding after the handoff, so the B5 outcome it is named for is unchecked.
- The post-handoff `clone add` assertion `SplitPane <= 1` accepts the new clone never receiving a pane.
- `private_start_agent_env_reaches_the_agent` runs only with `--features private-herdr` and a herdr binary; default `cargo test` does not exercise the real export delivery.
- No real-Herdr (private rig) qualification of live handoff or cold restart exists in this cluster (hg-66u.17 open), as spec §11 requires.

## Deferred OK
- `env: vec![]` additions in tests/herdr_spikes.rs and tests/herdr_client.rs are mechanical and correct.
- ClonePurge wiring in tests/integration_sweep.rs is complete and passing.
