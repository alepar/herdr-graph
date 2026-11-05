## diffstat

 CURRENT-DESIGN.md                                  |   1 +
 GRAPH_STATE.md                                     |   4 +-
 .../2026-10-29-seat-review-H2-identity-design.md   |  24 +-
 .../2026-11-04-threads-seat-identity-design.md     | 506 +++++++++++++++++++++
 ...tity-roast-design-1-roast-design-1-step-back.md |  19 +
 ...-seat-identity-roast-design-1-roast-design-1.md | 122 +++++
 ...seat-identity-roast-design-1-step-back-keys.txt |  10 +
 ...s-seat-identity-roast-design-2-format-repair.md |  82 ++++
 ...6-11-05-threads-seat-identity-roast-design-2.md |  83 ++++
 .../amend-design-1.md                              |  52 +++
 .../coverage-findings-round-1.md                   |  20 +
 .../coverage-findings-round-2.md                   |  24 +
 .../coverage-ledger.md                             |  71 +++
 .../coverage-round-1-requirements.md               |  21 +
 .../coverage-round-1-review-1.md                   |  39 ++
 .../coverage-round-1-review-2.md                   |  34 ++
 .../coverage-round-2-requirements.md               |  21 +
 .../coverage-round-2-review-1.md                   |  23 +
 .../coverage-round-2-review-2.md                   |  23 +
 .../findings-ledger-design.md                      |  14 +
 .../findings-overrides-design.md                   |   6 +
 .../2026-11-04-threads-seat-identity/friction.md   |   6 +
 .../runs/2026-11-04-threads-seat-identity/run.md   |  41 ++
 .../settled-tree-roast-1.md                        | 155 +++++++
 .../settled-tree-roast-2.md                        | 176 +++++++
 .../threads-ht-ma6i-protocol-shapes.md             |  40 ++
 docs/superpowers/specs/INDEX.md                    |   1 +
 skills/graph/SKILL.md                              |   2 +-
 skills/seat/SKILL.md                               |   9 +-
 src/bootstrap/mod.rs                               |   7 +-
 src/bootstrap/tests.rs                             |  15 +-
 src/cli/session.rs                                 |   1 +
 src/config.rs                                      |   3 +
 src/daemon/clone_status.rs                         |  25 +
 src/daemon/compose.rs                              |  34 +-
 src/daemon/mod.rs                                  |   1 +
 src/herdr/client.rs                                |  36 ++
 src/herdr/fake.rs                                  |  35 ++
 src/herdr/tests.rs                                 | 149 ++++++
 src/model/change.rs                                |   1 +
 src/model/clone.rs                                 |  40 +-
 src/model/launch.rs                                |  11 +
 src/model/tests.rs                                 |  55 +++
 src/observe/matcher.rs                             |  18 +-
 src/observe/mutations.rs                           |   1 +
 src/observe/tests.rs                               |  78 ++++
 src/plan/core_kinds.rs                             | 104 +++++
 src/plan/tests.rs                                  | 175 +++++++
 src/ports/herdr.rs                                 |   2 +
 src/reconcile/herdr_exec.rs                        |  27 +-
 src/reconcile/planner.rs                           |  39 +-
 src/reconcile/session.rs                           |   2 +
 src/reconcile/tests.rs                             |  76 +++-
 src/store/tests.rs                                 |   3 +
 src/threads/assignment.rs                          | 282 ++++++++++++
 src/threads/mod.rs                                 |  36 +-
 src/threads/tests.rs                               | 432 ++++++++++++++++--
 src/transcripts/capture.rs                         |  53 ++-
 src/transcripts/mod.rs                             |  26 ++
 src/transcripts/tests.rs                           | 113 ++++-
 src/undo/compensate.rs                             |   1 +
 src/undo/preview.rs                                |   9 +
 src/undo/tests.rs                                  |  38 ++
 tests/daemon_composed.rs                           | 121 +++++
 tests/herdr_client.rs                              |  65 +++
 tests/herdr_spikes.rs                              |   1 +
 tests/instruction_writers.rs                       |   2 +
 tests/integration_sweep.rs                         |   2 +
 68 files changed, 3629 insertions(+), 119 deletions(-)

## generated-input provenance

## Generated-input provenance
Captured range: 0a84c67554aa3fc8e8caf20db4163683ab30af61..e4984a492e2d3ce8344b9be83d56a3471049fd60
Selection: complete; selected product/input paths: 68; omitted historical payloads: 0.
diffstat.txt describes selected product/input paths. Reports, findings and collection metadata remain ordinary review inputs.
- Outside captured range: 26 informational entries (neither selected nor waived); the first 20 are listed; the full list is review-inputs.json outOfRange.
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:86 inv-p3b-20261031T222244: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 0a84c67554aa3fc8e8caf20db4163683ab30af61:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): e4984a492e2d3ce8344b9be83d56a3471049fd60:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:86 inv-p3b-20261031T222244: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): e4984a492e2d3ce8344b9be83d56a3471049fd60:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): e4984a492e2d3ce8344b9be83d56a3471049fd60:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): e4984a492e2d3ce8344b9be83d56a3471049fd60:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): e4984a492e2d3ce8344b9be83d56a3471049fd60:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): e4984a492e2d3ce8344b9be83d56a3471049fd60:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): e4984a492e2d3ce8344b9be83d56a3471049fd60:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete assembled bundle: digest.md
Open each report/archive at its recorded revision. Omission supplies no finding disposition or verdict.

## report summaries

- task 1 (task-1-report.md); title: # Task hg-66u.4 report (attempt 1)
  - Brief mentioned `readonly`/`seat_alerts`/`host` fields; none are constructed by literal in graph code (grep + clean all-features check), so no edits. `tmux_hint` also added to ResolveSeat (not in brief) as required to compile. No unmet acceptance items.
- task 2 (task-2-report.md); title: # Task 2 report (hg-66u.27)
  - - Mutations on an unchanged record (idempotent Assign/Acknowledge/etc.) commit without rewriting the record (rev unchanged; tested).
  - - Succession "already applied" check requires intent threads_done and matching epoch; launched flag ignored.
  - - Test-only fixture mutation `bookkeeping.test_seat_fixture` stands in for the undo and purge commits.
  - - Host lacks a C compiler; builds need `toolbox run -c ht-build`.
- task 3 (task-3-report.md); title: # Task 3 (hg-66u.5) report
- task 4 (task-4-report.md); title: # Task 4 (hg-66u.11) report
  - - Host lacks `cc`; cargo run inside the existing `ht-build` toolbox container. Submodule third_party/herdr-threads initialised locally (reference clone) and target/ reflink-seeded (git-ignored).
  - - tab_for_seat: bound in older incarnation, id present but owned by another seat falls through to the old fallbacks; only the absent case returns None.
- task 5 (task-5-report.md); title: # Task 5 (hg-66u.25) report
  - Status: IMPLEMENTED (completionKind implementation: test support + tests; no production code change).
  - - Third_party submodule initialised locally (reference clone) and target/ seeded by cp from integration worktree (gitignored).
  - - CONCERN: after a live handoff, new graph changes touching workspaces/tabs whose tokens were dropped are not executed until rebinding exists (hg-66u.28+). Dependent tasks must not assume the "SplitPane after swap" behavior of the original brief step 2.
- task 7 (task-7-report.md); title: # Task 7 (hg-66u.37) report
  - - Brief asks for a `FakeCall::ProcessInfo` guard assertion; the fake records no such call and changing fake.rs is outside the file list. The guard-before-start ordering is unchanged code and covered by existing non-shell tests; not asserted via call log.
  - - Tests ran in the `ht-build` toolbox because the host lacks a C linker; target dir was reflink-seeded from the integration worktree.
  - - Spec §14.10 unset-key clause not touched; no key is emitted pre-D1.
- task 8 (task-8-report.md); title: # Task 8 (hg-66u.38) report
  - - Host lacks a C linker; ran in ht-build toolbox, target reflink-seeded from task .37. Submodule third_party/herdr-threads initialised in the worktree.
  - - Pre-D1 only HERDR_GRAPH_CLONE is exported, so only it can be reported missing until hg-66u.13 adds the key.
  - - "clone absent" is judged from the report arg and the caller's graph_clone (both env-derived).
- task 9 (task-9-report.md); title: # Task 9 (hg-66u.35) report
- task 10 (task-10-report.md); title: # Task 10 (hg-66u.8) report
- task 11 (task-11-report.md); title: # Task 11 / hg-66u.6 report
  - - Service-side Rejected/Unsupported is simulated in the compose test by an explicit-endpoint error (Rejected from locate) and in service_tests by a stand-in socket answering Unsupported; no real older threads binary was used.
  - - The slot-clearing-on-success branch of LazyMap::inspect is not covered by a test (needs a live service).
  - - Adding the `seat_view_error` field to Services and `production_ports` goes slightly beyond the brief's wording; needed to reach the status component without changing existing signatures.
  - - `cargo fmt` is not clean on the base (pre-existing diffs in unrelated code); new code was not reformatted repo-wide.
  - - third_party/herdr-threads submodule had to be initialized in this worktree (git submodule update --init) to build.
- task 12 (task-12-report.md); title: # Task 12 (hg-66u.7) report
  - - Admission is asynchronous: a later writer rejection of Assign (race with another clone taking the seat after the
  - executor's graph-snapshot held-elsewhere check) is not observed by the executor (`admit_threads_seat` returns
  - `()`, not the op id; assignment.rs is outside this task's files). The effect is Done and the want re-fires only
  - on the next clone revision.
  - - The held-elsewhere pre-check in the executor duplicates `check_free` in assignment.rs, to give NeedsRevision.
  - - The mapping tests cover no real ThreadsSeatMap TargetUnresolved path (needs a live rig; not run).
  - - `cargo fmt --check` reports pre-existing diffs in src/threads/assignment.rs (Task 2), left untouched.
  - - Submodule was initialised in this worktree (`git submodule update --init`) at 95e9eba (v3.0.0).

## interfaces and seams

- src/daemon/clone_status.rs: pub fn clone_status(tree: &dyn TreeRead) -> serde_json::Value {
- src/daemon/mod.rs: pub mod clone_status;
- src/herdr/fake.rs: pub fn live_handoff(&self) {
- src/herdr/fake.rs: pub fn renumber_tabs_and_workspaces(&self) {
- src/model/clone.rs: pub enum SeatIntent {
- src/model/clone.rs: pub fn is_tombstone(&self) -> bool {
- src/model/launch.rs: pub const ENV_CONTINUITY_KEY: &str = "HERDR_THREADS_CONTINUITY_KEY";
- src/model/launch.rs: pub fn agent_env(clone: &CloneId, key_reserved: bool) -> Vec<(String, String)> {
- src/reconcile/planner.rs: pub fn launch_env_key(clone: &CloneId) -> String {
- src/threads/assignment.rs: pub const THREADS_SEAT_KEY: &str = "bookkeeping.threads_seat";
- src/threads/assignment.rs: pub enum SeatTransition {
- src/threads/assignment.rs: pub fn admit_threads_seat(
- src/threads/assignment.rs: pub fn write_restore_intent(rec: &mut CloneRecord, epoch: &OpId) {
- src/threads/assignment.rs: pub fn clear_restore_on_retire(rec: &mut CloneRecord) {
- src/threads/assignment.rs: pub fn purge_tombstone(rec: &CloneRecord, reservation: Option<&str>) -> Option<CloneRecord> {
- src/threads/assignment.rs: pub struct ThreadsSeatMutation;
- src/threads/mod.rs: pub mod assignment;
- src/transcripts/mod.rs: pub(crate) fn export_rejected_key(clone: &CloneId) -> String {

New public files:
A	src/daemon/clone_status.rs
A	src/threads/assignment.rs

Seam contract beads: none (no bead in the hg-66u tree has a title starting `Seam contract:`)

## deferred minors

Task 5 (hg-66u.25): minor (deferred): [loose-assertion] tests/daemon_composed.rs: SplitPane asserted <= 1 rather than exactly 1; brief step 2 relaxed because post-handoff workspaces/tabs are unknown until rebind (hg-66u.28)
Task 5 (hg-66u.25): minor (deferred): [deferred-rebind] after live handoff, new graph changes touching token-dropped workspaces/tabs are not executed until rebind exists (hg-66u.28+); dependent tasks must not assume SplitPane after swap
Task 5 (hg-66u.25): minor (deferred): [missing-evidence] report's evidence records exist only in the structured reply, no retained log location
Task 3 (hg-66u.5): minor (deferred): [missing-evidence] Report's full-suite log (/tmp/sp-39aed274/hg-66u.5/full.log) was not retained and no evidence records were returned; pass claim (lib 782 passed) is unverifiable from disk.
Task 3 (hg-66u.5): minor (deferred): [build-env-deviation] Report ran cargo via distrobox durak-dev instead of the epic's prescribed toolbox ht-build; also initialised third_party/herdr-threads submodule in the worktree.
Task 4 (hg-66u.11): minor (deferred): [scope-beyond-listed-files] src/herdr/fake.rs and src/reconcile/tests.rs changed though the bead lists only matcher.rs, planner.rs, observe/tests.rs; test-support only
Task 4 (hg-66u.11): minor (deferred): [fallthrough-owned-by-other] src/reconcile/planner.rs tab_for_seat: older-incarnation bound id present but owned by another seat falls through to token/pane fallbacks (disclosed in report)
Task 7 (hg-66u.37): minor (deferred): [missing-guard-order-test] src/reconcile/tests.rs: brief's FakeCall::ProcessInfo guard-before-start assertion not made (fake does not record process_info; fake.rs outside file list); guard code unchanged
Task 2 (hg-66u.27): minor (deferred): [missing-evidence] task-2-report.md: test log lived on tmpfs and was not retained; no source-bound evidence records with tree hash were available to verify the 855-pass claim
Task 2 (hg-66u.27): minor (deferred): [tdd-order] task-2-report.md: implementation written before tests, no RED run captured
Task 8 (hg-66u.38): minor (deferred): [pre-d1-partial-detection] Only HERDR_GRAPH_CLONE is detectable until hg-66u.13 exports the continuity key; the status test fakes the key in launch_env
Task 8 (hg-66u.38): minor (deferred): [missing-evidence] report records no structured evidence array (command is stated in prose, 126 passed)
Task 9 (hg-66u.35): minor (deferred): [missing-red-run] no failing-test (RED) run recorded; only a compile failure preceded the tests
Task 9 (hg-66u.35): minor (deferred): [test-placement] clone_status_lists_retired_unpurged_clones is in src/plan/tests.rs rather than inline in src/daemon/clone_status.rs as the brief said (reported, needs plan fixtures)
Task 9 (hg-66u.35): minor (deferred): [signature-drift] clone_status drops the journal parameter the brief specified (src/daemon/clone_status.rs:88); unused, harmless
Task 9 (hg-66u.35): minor (deferred): [untracked-submodule] third_party/herdr-threads was populated by copying into the worktree, untracked and uncommitted
Task 9 (hg-66u.35): minor (deferred): [missing-evidence] report gives commands and pass counts but no structured evidence records with sourceTree

## amendments

- hg-66u.39: split into hg-66u.43, hg-66u.44
- hg-66u.34: split into hg-66u.37, hg-66u.38
- hg-66u.13: split into hg-66u.39, hg-66u.40
- hg-66u.6: no rework needed; single textual conflict in src/daemon/compose.rs (register_status(reg, &journal, &store)) to resolve on rebase
- hg-66u.4: no rework; merge needs `git submodule update --init third_party/herdr-threads` (v3.0.0, 95e9ebac) before the merge check
