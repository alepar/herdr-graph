# Final review, cluster 21 (tests), epic hg-66u

Invocation: inv-4aa37d70-023c-483a-b47c-3dc801a586fe. Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify`; review source bound in the ledger before this write).

Cluster files: tests/daemon_composed.rs, tests/herdr_client.rs, tests/herdr_spikes.rs, tests/instruction_writers.rs, tests/integration_sweep.rs (slices 000095-000099).

## Scope and method

Read the five slices, the root spec (docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md, which carries a `## Requirement baseline` with rows B1-B11 and no human scope change rows), the digest, and the production code each test exercises. Built and ran the cluster's test binaries at the exact head in a review-owned detached worktree (since removed), inside the ht-build toolbox:

- `cargo test --test daemon_composed`: 16 passed.
- `cargo test --features private-herdr --test herdr_client private_`: 6 passed against a private `herdr server` rig (herdr present at ~/.local/bin/herdr), including the new `private_start_agent_env_reaches_the_agent`.
- `cargo test --features private-herdr --test herdr_spikes --no-run`: compiles.
- `cargo test --test instruction_writers`: 1 passed, 1 FAILED (`instruction_source_writers_are_inventoried`).
- `cargo test --test integration_sweep`: 12 passed, 1 FAILED (`wiring::sweep_action_and_effect_envelopes_round_trip`).

Caller checkout observed before and after: HEAD 6576a117, working tree clean; nothing in it was modified.

## Per-file analysis

tests/instruction_writers.rs. The diff adds `src/threads/assignment.rs` to the instruction-source writer inventory; that entry is correct (`ThreadsSeatMutation` `TombstoneDone` only calls `delete_file(&loc.record_path)` on a clone record that `is_tombstone()`). But the test fails at head: the scan also finds `src/plan/core_kinds.rs`, where hg-66u.35's clone purge (`ClonePurgeKind::mutate`, core_kinds.rs:1925) added `cx.tree.delete_file(&loc.record_path)`. The inventory was never updated for it. Assertion output: left contains `src/plan/core_kinds.rs`, right does not. The writer itself is benign (a clone record, committed through a plan kind by apply), so the remedy is an inventory entry with its reason, not a code change.

tests/integration_sweep.rs. Adding `K::ClonePurge` to `sweep_every_request_kind_has_a_mutation` is correct and that test passes. Separately, `sweep_action_and_effect_envelopes_round_trip` (unchanged in this diff, same file) now fails deterministically: it waits until no effect is Pending/Unknown, and the `threads.resolve_seat` effects introduced by hg-66u.7 stay Pending forever in the wiring rig (`last_error: "no seat for pane p2"` / `"no seat for pane p3"`, deferred with backoff, `defer_n` 2-3 after 5 s). The rig's FakePaneSeatMap never maps those panes, so the 20 s "creation effects to settle" wait times out. Either the rig must give the panes a seat, or the settle predicate must exclude resolve-seat effects; as it stands the default-tier suite is red.

tests/daemon_composed.rs. `live_handoff_under_a_running_loop_reconnects_and_creates_nothing` (hg-66u.25) proves the runtime loop resubscribes after `FakeHerdr::live_handoff` (two swaps recorded in `incarnation:last`/`changed_at`), and that nothing is created twice or closed. Instrumented at head: after the swaps plus `clone add sh`, the call log holds exactly one `SplitPane` (target p2, the sh tab, preserved id), two workspace re-stamps, and no create/close of pre-existing objects. Two weaknesses:
- The SplitPane assertion is `<= 1`, with a comment saying the workspace and tab are `unknown` after handoff and get no create effects. That was true when hg-66u.25 was written, but hg-66u.11 (merged after it, bbce2de) made tabs and workspaces match by preserved Herdr id in any incarnation (spec §7), and the head now produces exactly 1. The test therefore accepts a regression in which a post-handoff clone add silently creates no pane; the assertion should be `== 1` and the comment updated.
- The test never asserts what happens to the clones that existed before the handoff. Instrumented at head, both pre-existing clones end `availability=Unknown`, `threads_seat=None`, bindings still stamped with generation 1 (no pane re-stamp). The test name says "reconnects", which is true of the event loop only.

tests/herdr_client.rs. `private_start_agent_env_reaches_the_agent` is a sound real-Herdr check of spec §5: the workspace env carries only PATH (pointing to a stand-in `claude` that dumps `env`), so `HERDR_GRAPH_CLONE` and `HERDR_THREADS_CONTINUITY_KEY` can only arrive via the adapter's shell export before `agent.start`. It passes on the private rig. Unsafe-value refusal and the no-env/no-export case are covered by the fake-socket tests in src/herdr/tests.rs (`client_start_agent_refuses_unsafe_env_values`, `client_start_agent_exports_env_before_agent_start`). The `env: vec![]` additions to the existing private tests are mechanical.

tests/herdr_spikes.rs. One mechanical `env: vec![]` addition; compiles.

## Baseline trace

B5 ("herdr update --handoff ... unattended frictionless resume ... both in threads and graph") is the row this cluster's composed test targets. Traced from production at head: `RuntimeLoop.seat_view` is wired from `compose.rs` but carries `#[allow(dead_code)]` with the note "unused until SeatObservation (hg-66u.28)"; hg-66u.28 is open. No production path consumes `ThreadsSeatView` or rebinds a clone from threads, and the old matcher cannot re-match a pane whose token was dropped and terminal id reallocated. The composed run above shows the consequence: after a live handoff every pre-existing clone is Unknown and stays so until an operator `clone rebind`. B5 is not delivered at this head, and this is not gate-blocked (spec §6 lands the threads reader and binding-from-threads mapping before D1 against v3.0.0, which already has process continuity across live handoff). No human scope change row covers it.

B4 (frictionless restarts) and B7/B10 (undo onto the same seat) depend on the same unmerged SeatObservation and resume-driver work (hg-66u.28, .29, .30, .32) plus the D1/D2 gates; their production paths leave this cluster and are the seam reviewer's to trace. B9 (continuity key) is exercised by the herdr_client test only through the adapter; key emission is correctly withheld until a reservation exists (pre-D1), which is the seam reviewer's to confirm end to end.

## Verdict
not ready (two cluster test binaries are red at head, and baseline row B5 is not delivered: clones stay Unknown after a live handoff)

## Must fix before landing
- tests/instruction_writers.rs `instruction_source_writers_are_inventoried` fails at head: `src/plan/core_kinds.rs` (clone purge `delete_file` of the clone record, hg-66u.35, core_kinds.rs:1925) is a new store writer missing from the inventory; add it with its reason (clone record only, committed through the clone.purge plan kind).
- tests/integration_sweep.rs `wiring::sweep_action_and_effect_envelopes_round_trip` fails at head: hg-66u.7's `threads.resolve_seat` effects stay Pending ("no seat for pane p2/p3") in the wiring rig, so the "creation effects to settle" wait times out after 20 s; give the rig's FakePaneSeatMap seats for the created panes or exclude resolve-seat effects from the settle predicate.
- B5 not delivered: at head no production path consumes ThreadsSeatView (`RuntimeLoop.seat_view` is `#[allow(dead_code)]`, "unused until SeatObservation (hg-66u.28)", bead open), and in the composed live-handoff run both pre-existing clones end availability=Unknown with generation-1 bindings, needing an operator rebind; land SeatObservation (hg-66u.28) and assert post-handoff clone recovery in the handoff test.

## Untested scope
- tests/daemon_composed.rs live-handoff test asserts `SplitPane <= 1` with a stale comment (pre-hg-66u.11) claiming the workspace and tab are unknown after handoff; head produces exactly 1, so the test would pass if a post-handoff clone add stopped creating its pane; tighten to `== 1` and fix the comment.
- tests/daemon_composed.rs live-handoff test never asserts the availability or binding of clones that existed before the handoff, so the B5 recovery behaviour has no composed-daemon test.
- No test in this cluster covers a pane shell that rejects `export` (spec §5: agent starts without the key, falling back to C1 evidence).

## Deferred OK
- herdr_spikes.rs and existing herdr_client.rs `env: vec![]` additions are mechanical and need no further test.
- Shell-history behaviour of the space-prefixed export line is shell configuration dependent and reasonably left untested.
