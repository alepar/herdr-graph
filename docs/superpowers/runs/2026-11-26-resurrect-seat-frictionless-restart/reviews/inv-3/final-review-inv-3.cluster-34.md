# Final review inv-3, cluster 34 (third-party)

Source: base 60e287a, head 11ac415. Slice 000209 is the only input: the gitlink third_party/herdr-threads moves from 482e0d2 (v3.1.0) to 7c3c090 (v3.6.0, "Freeze the v3.6.0 peer wire").

## Pin analysis

- The epic asks for the threads pin to move from 3.1.0 to at least 3.5.0 (ResurrectSeat, service_session_v4). 7c3c090 is tagged v3.6.0, is a descendant of the old pin, and is reachable from origin/main of the submodule remote, so the bump is a fast-forward of a published commit.
- A newer pin than the bead names is explained by the Cargo.toml comment (with_level ht-lzhi, restart-held seat rebind ht-0f0r). The 3.5.0 requirement is a floor, so 3.6.0 satisfies it.
- Submodule Cargo.toml reads 3.6.0 and the root Cargo.lock diff is a single version line (3.1.0 to 3.6.0); the submodule's only dependency-surface change is a new test target, so no transitive lock churn.
- The submodule worktree at the head is clean and the object is present locally.
- The 3.6.0 daemon floor is enforced in graph by a version check at discovery (compose.rs, doctor.rs). That wiring belongs to other clusters.

## Requirement trace

- The pin itself is not a production behavior. Delivery of ResurrectSeat and the restart rebind through this pin is traced from graph's own entry points in the src clusters; named here as the seam reviewer's.

## Verdict

ready

## Must fix before landing

- none

## Untested scope

- none

## Deferred OK

- none
