# Final review, cluster 1 (root), epic hg-lnw

Source: base 60e287aadd08af10779df39e625200f28d4b7fa3, head 8f8ec538ca1f1e4d8d897804aae726472f753ef9 (both verified with `git rev-parse --verify <sha>^{commit}`). Caller checkout observed at HEAD 8f8ec53, status clean; not modified by this review. Review source bound with `bind-review-source` (exit 0).

Cluster files: CHANGELOG.md, CURRENT-DESIGN.md, Cargo.lock, Cargo.toml, README.md (slices 000001-000005).

## What was checked

- Pin: `third_party/herdr-threads` gitlink at head is 7c3c09082e8894ebceec9c060b769b07e359a2ad, which is tag v3.6.0 in the submodule; its Cargo.toml reports version 3.6.0, matching the Cargo.lock bump (only the version line changes; builds and the 1536-test lib run in evidence/hg-lnw.12/cargo-test.log pass, so the lock is consistent). README's release commit and URL match.
- Operator remedy text: README/CHANGELOG/CURRENT-DESIGN say `herdr-threads human seat rebind SEAT --pane PANE --operator`. This matches `operator_rebind_remedy` (src/daemon/clone_status.rs:67) and the v3.5.0/v3.6.0 herdr-threads CHANGELOG (`herdr-threads human seat rebind OLD --pane PANE [--replace NEW] --operator`). The spec's `herdr-threads seat rebind` spelling was corrected by hg-lnw.21; the docs follow the code, which is right.
- Named error text: `herdr-threads daemon refused service_session_v4 (...); upgrade the herdr-threads daemon to >= 3.6.0` matches `level_refused` (src/threads/adapter.rs:66-71); the doctor upgrade step and the "fails, or warns when no state dir" rule match src/daemon/doctor.rs:90-125. "Graph retries the registration on its next threads call" matches `ready()` (registration attempted once per call).
- Op keys: `resurrect:<clone>:<epoch>:<replace or none>:<refusal count>` matches src/threads/effects.rs:1812; `rebind:<clone>:<incarnation changed_at>` matches effects.rs:1907. Meta keys `resurrect_refused:`, `rebind_refused:`, `seat_unresolved:` match.
- Status and doctor names `clones.restart_held`/`restart held` and `clones.resurrect_held`/`resurrect held` match src/daemon/doctor.rs:126-169 and clone_status.rs.
- Restart wording (fresh keyless waits until placed or 90 s grace; fresh keyed never waits; `RelaunchOccupant` waits the full grace) agrees with spec §4 and the Unchanged-behaviour list (RelaunchOccupant keeps its existing grace).
- Undo windows (24 h / 7 d, invitation reset of `Ordinary` `Pending` rows, `Required` and accepted rows untouched, purge finalizes) agree with spec §3.

## Requirement baseline trace

- B1 (resurrect the closed tab's seat): production path daemon compose (src/daemon/compose.rs:1015-1027) to adapter `resurrect_seat` (src/threads/adapter.rs:743) driven by the `threads.resurrect_seat` want (src/threads/effects.rs:780, 1767-1870). The want's planning and gating are the src cluster's to judge; the root-cluster docs describe it accurately. Tier-5 real-threads evidence in evidence/hg-lnw.8.
- B2 (frictionless Herdr restart): `threads.rebind_held_seat` want (effects.rs:576, 1881-1911) through compose.rs:1031-1042 to adapter.rs:754; planner gate in src/reconcile/planner.rs. Delivered against a 3.6.0 daemon; see the 3.5.x item under Must fix for the case the docs promise but the code does not guard. Composition with planner/observer is the seam reviewer's to trace.
- B3 (upstream service level): the production adapter builds `PersistentServiceClient::new(...).with_level(GRAPH_SERVICE_LEVEL)` with V4 (adapter.rs:281, 62), using ht-lzhi from the pinned 3.6.0. Delivered.
- B4 (upstream rebind of a restart-held seat): `rebind_held_seat` issues `ResurrectSeat` with no `replace` (ht-0f0r in 3.6.0), mapping `SeatAlreadyResolved` to done (adapter.rs restore_outcome). Delivered at this pin.

## Version-guard claim against a 3.5.x daemon

The root-cluster docs state that a daemon older than 3.6.0 refuses `service_session_v4` by name: CHANGELOG ("A daemon older than 3.6.0 refuses it"), README ("A herdr-threads daemon older than 3.6.0 refuses graph's `service_session_v4` registration"), Cargo.toml comment ("A daemon older than 3.6.0 refuses the v4 registration"), and CURRENT-DESIGN's pin line implies it. That is false for 3.5.x: herdr-threads v3.5.0 (6aa914a) already defines `SERVICE_SESSION_CAPABILITY_V4` (src/protocol/service.rs:30) and serves `ResurrectSeat`. Graph's named error fires only on an `Unsupported` registration answer (adapter.rs `register_error`), so a 3.5.x daemon registers v4 silently. On it, the undo resurrect works, but the U2 held-seat rebind answers `NotFound`, which graph records as a definitive `rebind_refused` and then shows `clones.restart_held` after 90 s: the fresh keyless start after a restart is again held until an operator rebind (the B2 friction), with no upgrade hint. The hg-lnw.7 implementer flagged this; nothing in the root docs or code closed it. The Migration note's "daemon must run 3.6.0" instruction is correct; the "refused by name" guarantee is not.

## Findings

- Docs promise a named refusal for every daemon older than 3.6.0, but a 3.5.x daemon accepts `service_session_v4`; graph connects with no error and B2's restart rebind degrades to `rebind_refused` / `clones.restart_held` (CHANGELOG.md "Older herdr-threads daemons are refused by name", README.md threads-discovery paragraph, Cargo.toml feature comment, CURRENT-DESIGN.md pin line).
- All other root-cluster text (pin commit, remedy spelling, error text, op keys, status and doctor names, window semantics, restart gating) matches the code at head.

## Verdict
not ready (docs claim pre-3.6.0 daemons are refused by name; a 3.5.x daemon is accepted silently and B2's restart rebind degrades without the upgrade hint)

## Must fix before landing
- Correct the "older than 3.6.0 is refused by name" claim in CHANGELOG.md, README.md, Cargo.toml and CURRENT-DESIGN.md to say only daemons older than 3.5.0 refuse v4 and that a 3.5.x daemon connects but cannot rebind restart-held seats (they surface as `clones.restart_held` with `rebind refused: not_found`), or add a real guard (for example map a held-seat rebind `NotFound` that the fact contradicts, or a daemon version check, to the named upgrade error) so the documented guarantee holds; either keeps B2 working on a supported 3.6.0 daemon.

## Untested scope
- No test runs graph against a 3.5.x daemon, so the registration and rebind behaviour on the one older daemon that serves v4 is unverified.
- The tier-5 `operator_rebind_resurrects_and_places` test builds the remedy argv itself rather than parsing `operator_rebind_remedy`, so the remedy string printed in status, doctor and the docs is not checked end to end against real threads (hg-lnw.21 concern).

## Deferred OK
- An empty `/tmp/.git` directory, not created by this epic, breaks `stop-run-processes` temp-root checks; a human should remove it (`rmdir /tmp/.git`) (humanAction from hg-lnw.6, .7, .22, .23).
- Reflink-seeded `target/` dirs in the task worktrees should be removed with the worktrees after merge.
