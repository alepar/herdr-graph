# Final review, epic hg-lnw, invocation inv-3, cluster 2 (docs-superpowers)

## Source and inputs

- Base 60e287aadd08af10779df39e625200f28d4b7fa3 and head 11ac415754b5edcedf3790ca4aa050e9fbeabba5, both verified with `git rev-parse --verify <sha>^{commit}`. `bind-review-source` exited 0.
- Caller checkout observed at HEAD 11ac415 with `git status --short` showing only `?? evidence/sweep/` (the background sweep's output). This review did not modify it, created no worktree and ran no cargo.
- Inputs: the 93 files and slices 000006 to 000097 that clusters.json lists for docs-superpowers, digest.md, `bd show hg-lnw --json`, and the root design doc, which carries a `## Requirement baseline` (B1 to B4, no human scope change rows). Source files opened only to check named claims: src/daemon/clone_status.rs, src/reconcile/herdr_exec.rs, src/reconcile/planner.rs, src/threads/assignment.rs, src/threads/effects.rs, src/threads/adapter.rs, src/herdr/client.rs, src/daemon/doctor.rs, src/cli/adopt.rs.
- Historical assembled review payloads under reviews/inv-1/assembled and reviews/inv-2/assembled (176 files for inv-2) are committed but omitted from the selection by generated-input-provenance.md; committing them matches the base tree, which already holds 947 such files from earlier runs.

## What the cluster holds

- Product-facing docs: the run's design doc (with its Post-Implementation Notes), the onboarding spec's G4 table, its status line, prerequisites and a new Post-Implementation note, and one new row in docs/superpowers/specs/INDEX.md.
- Run records: run.md, roast reports for design rounds 1 and 2 and code rounds 1 and 2, step-back and scope-filter keys, coverage rounds and ledger, the code and design ledgers and overrides, amend-design-1.md, friction.md, interim-status.md, two run profiles (md and json), and the inv-1 and inv-2 final review collections. These are process records, not product behaviour.
- inv-2's seam review named this cluster (then docs-runs) as unreviewed because it exceeded the review window; this review covers it.

## Accuracy against the code at head

- Operator remedy: both docs print `herdr-threads human seat rebind SEAT --pane PANE --operator`, matching `operator_rebind_remedy` in src/daemon/clone_status.rs:68 and the doctor rows.
- Keyless start gate: the onboarding G4 restart row and the design note say a fresh keyless start opens early on a fresh `Resolved` read with `seat_unresolved:<clone>` set for the incarnation, else at the grace, and keyed starts never wait. `keyless_start_gate` (src/reconcile/herdr_exec.rs:38) returns None for keyed, unseated and pending-Restore clones and checks exactly those conditions. The resumed-relaunch full-grace wait is recorded as a divergence in the design notes and stated in the onboarding row.
- Resurrect op key: the design note gives `resurrect:<clone>:<epoch>:<replace>:<count>`; src/threads/effects.rs:1812 formats that key. The rebind key `rebind:<clone>:<incarnation changed_at>` matches effects.rs:1907.
- Invitation reset: the note says the reset rides `AcknowledgeResurrected` and retires only `Ordinary`, `Pending` rows linked to the resurrected seat; src/threads/assignment.rs:246-266 does exactly that, leaving Accepted, Rejected and Required rows alone.
- `SeatAlreadyResolved` as a done outcome and rebind `NotFound` as definitive: consistent with `restore_outcome` in src/threads/adapter.rs:112 and the port's `AlreadyResolved` variant.
- Never-observed keyed clones: design 4.3 says `restart_covered` counts a `launched:` meta for non-shell harnesses; src/reconcile/planner.rs:711 does so.
- Daemon floor: the hg-lnw.25 note corrects section 2's "an older daemon refuses v4" claim (true only below 3.5.0) and describes the descriptor-version check; `daemon_floor` exists at src/threads/adapter.rs:227. The onboarding prerequisites line ("3.6.0 or later ... no fallback") agrees.
- Agent name retry (hg-lnw.23): `agent_names` exists at src/herdr/client.rs:54 as the note says.
- The inv-1 seam review's item that the design doc of record was stale is answered by the 2026-11-27 Post-Implementation Notes, which cover each divergence it listed.
- No credentials, tokens or personal email addresses appear in the run records or profiles (pattern scan over the run directory at head).

## Requirement baseline trace

- B1 (undo resurrects the closed tab's seat): documented consistently in the design doc, onboarding G4 "Tab or pane closed" row and its Post-Implementation note. The production path (undo Restore intent to `resurrectable`, `ThreadsExecutor::resurrect_seat`, `LazyThreads`, v4 adapter) lies outside this cluster and is the seam reviewer's to trace; inv-2 traced it as delivered, and its lost-response relaunch item is addressed by hg-lnw.30 at head (commit cd0e869), also outside this cluster.
- B2 (frictionless Herdr restart): the docs describe rebind on every pass while held, a bounded keyless gate and a status row only as a degraded fallback, with no operator step on the normal path. Code trace is the seam reviewer's.
- B3 (upstream v4 level) and B4 (upstream rebind of a held seat): the docs name ht-lzhi and ht-0f0r shipping in herdr-threads 3.6.0 and the pin to 3.6.0. Code trace is the seam reviewer's.
- No docs in this cluster narrow or contradict a baseline row.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- Doc claims about shipped behaviour are checked here by reading code, not by any test; the packaging test checks README headings only, and nothing checks the onboarding spec or design notes against code.

## Deferred OK
- docs/superpowers/specs/INDEX.md row says graph rebinds unresolved seats "before relaunch"; at head keyed relaunches do not wait on the rebind and keyless ones wait only until placement or the grace, so "before or alongside relaunch" would be exact.
- interim-status.md is a dated snapshot (code tip 985239e, "not ready to merge, waits on 3.6.0") that no longer describes the branch; a one-line superseded pointer would prevent misreading.
- findings-ledger.md is headed "through r1" and lists items as open that later rounds resolved (run.md records roastCodeExit converged); the ledger is not refreshed to the final state.
- Design section 2 cites `src/cli/adopt.rs:76` for graph's only threads CLI call; the `Command::new` is at line 75 at head (off by one, harmless).
- Design sections 3 and 4 body text still states the pre-implementation contract (rebind `NotFound` as done, the old op key, the `seat rebind` remedy without `human`); the dated Post-Implementation Notes supersede each, which is the repo's convention, but a reader of the body alone is misled.
