## diffstat

 .gitignore                                         |    3 +
 CURRENT-DESIGN.md                                  |    1 +
 ...2026-11-05-seat-failure-manager-seats-design.md |  364 +++++
 ...manager-seats-roast-design-1-step-back-keys.txt |   10 +
 ...ilure-manager-seats-roast-design-1-step-back.md |    9 +
 ...05-seat-failure-manager-seats-roast-design-1.md |  126 ++
 ...05-seat-failure-manager-seats-roast-design-2.md |   60 +
 .../coverage-findings-round-1.md                   |   19 +
 .../coverage-findings-round-2.md                   |   16 +
 .../coverage-ledger.md                             |   44 +
 .../coverage-round-1-requirements.md               |   20 +
 .../coverage-round-1-review-1.md                   |   34 +
 .../coverage-round-1-review-2.md                   |   34 +
 .../findings-ledger-design.md                      |   14 +
 .../findings-overrides-design.md                   |    0
 .../friction.md                                    |    7 +
 .../profile-inv1.json                              | 1393 ++++++++++++++++++++
 .../profile-inv1.md                                |  162 +++
 .../2026-11-05-seat-failure-manager-seats/run.md   |   48 +
 docs/superpowers/specs/INDEX.md                    |    1 +
 skills/seat/SKILL.md                               |   11 +
 src/bootstrap/instructions.rs                      |   57 +-
 src/bootstrap/mod.rs                               |  126 +-
 src/bootstrap/show.rs                              |   11 +-
 src/bootstrap/tests.rs                             |  564 +++++++-
 src/cli/daemon_error.rs                            |  311 +++++
 src/cli/instructions.rs                            |    2 +
 src/cli/mod.rs                                     |    3 +
 src/cli/node_manager_warning.rs                    |  201 +++
 src/cli/ops.rs                                     |  102 +-
 src/cli/plan.rs                                    |   29 +-
 src/cli/request.rs                                 |   35 +-
 src/cli/seat.rs                                    |   90 +-
 src/cli/show.rs                                    |   47 +-
 src/cli/status.rs                                  |    6 +-
 src/cli/undo.rs                                    |   61 +-
 src/cli/who.rs                                     |   30 +-
 src/daemon/client.rs                               |   46 +-
 src/daemon/doctor.rs                               |    1 +
 src/daemon/registry.rs                             |   53 +
 src/ipc.rs                                         |   25 +
 src/model/change.rs                                |    2 +
 src/model/effective.rs                             |   19 +-
 src/model/graph.rs                                 |    3 +
 src/model/manager.rs                               |  492 +++++++
 src/model/mod.rs                                   |    1 +
 src/model/seat.rs                                  |    3 +
 src/model/teamspace.rs                             |    5 +-
 src/model/tests.rs                                 |   41 +
 src/observe/classify.rs                            |   52 +-
 src/observe/mutations.rs                           |   34 +-
 src/observe/step.rs                                |    3 +
 src/observe/tests.rs                               |  381 ++++++
 src/plan/core_kinds.rs                             |   52 +-
 src/plan/kind.rs                                   |    1 +
 src/plan/managers/cycle_guard.rs                   |  139 ++
 src/plan/managers/edit.rs                          |  245 ++++
 src/plan/managers/first_seat.rs                    |  147 +++
 src/plan/managers/mod.rs                           |  286 ++++
 src/plan/managers/node_manager.rs                  |  193 +++
 src/plan/managers/reparent.rs                      |  260 ++++
 src/plan/managers/retire.rs                        |   67 +
 src/plan/managers/tests.rs                         |  207 +++
 src/plan/mod.rs                                    |    1 +
 src/plan/tests.rs                                  | 1248 ++++++++++++++++++
 src/ports/store.rs                                 |   84 ++
 src/reconcile/desired.rs                           |    3 +
 src/reconcile/tests.rs                             |   79 ++
 src/store/init.rs                                  |    1 +
 src/store/instruction_source.rs                    |   11 +
 src/store/tests.rs                                 |    2 +
 src/templates/kinds.rs                             |    5 +
 src/templates/structure.rs                         |    1 +
 src/templates/tests.rs                             |  168 ++-
 src/threads/mod.rs                                 |   47 +-
 src/threads/tests.rs                               |  143 +-
 src/undo/tests.rs                                  |  128 ++
 src/writer/mod.rs                                  |    3 +-
 src/writer/tests.rs                                |   15 +
 tests/daemon_composed.rs                           |    3 +-
 tests/daemon_error_commands.rs                     |   74 ++
 tests/daemon_free_reads.rs                         |   96 ++
 tests/integration_sweep.rs                         |    6 +-
 tests/node_manager_warning.rs                      |  154 +++
 tests/seat_exit_codes.rs                           |   92 ++
 tests/writer_concurrency.rs                        |    2 +
 86 files changed, 8898 insertions(+), 277 deletions(-)

## generated-input provenance

## Generated-input provenance
Captured range: 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784..efc21b65eb225578d3a623b29f23b3688849ea8b
Selection: complete; selected product/input paths: 86; omitted historical payloads: 0.
diffstat.txt describes selected product/input paths. Reports, findings and collection metadata remain ordinary review inputs.
- Outside captured range: 26 informational entries (neither selected nor waived); the first 20 are listed; the full list is review-inputs.json outOfRange.
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:86 inv-p3b-20261031T222244: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 0dbe4772f4cd541e6b5ecaa1dd55b9a72f62c784:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): efc21b65eb225578d3a623b29f23b3688849ea8b:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:86 inv-p3b-20261031T222244: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): efc21b65eb225578d3a623b29f23b3688849ea8b:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): efc21b65eb225578d3a623b29f23b3688849ea8b:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): efc21b65eb225578d3a623b29f23b3688849ea8b:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): efc21b65eb225578d3a623b29f23b3688849ea8b:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): efc21b65eb225578d3a623b29f23b3688849ea8b:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): efc21b65eb225578d3a623b29f23b3688849ea8b:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete assembled bundle: digest.md
Open each report/archive at its recorded revision. Omission supplies no finding disposition or verdict.

## report summaries

### hg-k4l.1 report (task-1-report.md)
Status: IMPLEMENTED (completionKind implementation). base ce20ed2f4e7d673d83f7267e329a866dba9645aa, head 69346c7765e7042ec76f1405addb92bd944d46f4, tree 0c1cae6be82809823930c4222741604ff0e6e735. Not stacked.
- bd comment on hg-k4l.1 asks to init the submodule in the integration worktree; that worktree is read-only for me, so I did not. Task worktree already had the submodule initialised and target/ seeded.
- Out of scope noted in the brief: From<StoreError> for Fail in src/writer/mod.rs should treat Integrity as Data (hg-k4l.15).
- No amendments used.

### 10 (hg-k4l.16) report (task-10-report.md)
- "(<st>)" in the line is the seat id (st_...), per brief's `boss (st_..)` example; format `name (id)` joined by ` -> `.
- hg-k4l.11 helpers not present; used local test mutations (`bookkeeping.test_seat_manager`, `bookkeeping.test_manager`).
- Build env: host has no cc/libz.so; used a zig cc shim and libz symlink under the task TMPDIR (PATH/LIBRARY_PATH), as other tasks did.

### 11 (hg-k4l.6) report: NodeManagerSet kind (task-11-report.md)
Status: IMPLEMENTED. base 17211679c4a8af3a105186a709753c98e707e65e (stack: hg-k4l.5), head b5a4c0eb8f5b4ad1e2f9e7f69139f2a1e56fa789, tree 7c530c3077e3d17037067b85d5d57af3fc0b244a. stacked true.
- `cargo test --lib --locked --offline plan::` -> test result: ok. 150 passed; 0 failed
- `cargo test --test daemon_composed every_kind_and_command_registered` -> ok
- `cargo test --test integration_sweep sweep_every_request_kind` -> ok
- `cargo fmt --check` ok; `cargo clippy --all-targets`: only pre-existing warnings, none in changed files.
- Host has no `cc`; cargo ran inside `toolbox run -c ht-build` with `LIBRARY_PATH` pointing at a libz.so symlink shim under /tmp/sp-2d1990ff/hg-k4l.6/lib (zlib-devel absent). No source impact.
- hg-k4l.13 (ManagerEdit) will also touch the same exhaustive lists in tests/integration_sweep.rs and tests/daemon_composed.rs: expect a trivial merge conflict.
- Old live node manager gets `seat.manager -> target` only via the seat loop; a seat that is the target is never rewritten except via its own cleared-manager effect.

### hg-k4l.8 (plan task 13) report (task-13-report.md)

### 14 (hg-k4l.10) report (task-14-report.md)
- `applies` uses `Command::Init(_) | Setup(_) | Daemon(_)` and the hook-prompt seat form.
- No existing CLI test asserting empty stderr needed changes.
- Test fixtures duplicate a small git2 commit helper between the unit tests and the integration test (integration tests cannot reach cfg(test) code).

### 15 (hg-k4l.13) report (task-15-report.md)
- Teamspace subject: cycle check covers every non-retired seat of the teamspace.
- Node-manager subject check applies to clear as well as set.
- cycle_guard (hg-k4l.21) is still a stub in this stack; ManagerEdit does its own cycle check.

### 17 (hg-k4l.21) report (task-17-report.md)

### 2 (hg-k4l.2) report (task-2-report.md)
- Design-roast-r2 "running daemon, HERDR_SOCKET_PATH unset, Remote{Integrity}" is covered by unit test integrity_outside_herdr_is_still_exit_6 (brief permits unit test for 6); no CLI test for exit 6.
- Ensure-mode callers matching Unavailable(_) (none; fallbacks use NoEnsure) are unaffected by NotInHerdr.
- No bead amendments or comments existed.

### hg-k4l.23 report (task-23-report.md)
Status: IMPLEMENTED (implementation). Stacked on task-hg-k4l.5. head 2ad4405edec71efd906267b9223e1470ced793ac.

### hg-k4l.26 report (task-26-report.md)
Status: IMPLEMENTED. base 5992c134312d128f0b9ca52a31c917b9d8264198, head eed04a07be823cd7deea270e7e4e7069ec49b40f, tree e00123135cd4789faa2d4499076edff070d6cdd7. Not stacked.
- Node manager self-reference: if the computed target equals the holder seat itself (the holder is the node manager), the rewrite sets its manager to None rather than pointing it at itself.
- Rebase onto moved integration conflicted only in src/plan/tests.rs (upstream added its own `set_node_manager(fx, Option<&SeatId>)` and registered SetNodeManager); resolved by keeping upstream's and re-adding my block using it.
- Deferred minor (retire_seat 8 params in observe/mutations.rs): not touched; my lines there did not rewrite that signature.
- The lenient fallback with no live node manager (target None) is not covered by a test; only the node-manager fallback is.
- Undo of Retire/ApplicationRetire restoring reparented pointers (spec 4.11) and plan-side previews are other tasks (hg-k4l.27 etc.).
- Environment: building tests needs a toolbox with gcc plus a libz.so shim; not a repo issue.

### 27 (hg-k4l.27) report (task-27-report.md)

### 3 (hg-k4l.12) report (task-3-report.md)
- `short_rev` truncates to 12 chars (brief example shows 7); used as-is.
- seat_parts holder/field is (seat, "record", seat) per table.

### 4 (hg-k4l.4) report (task-4-report.md)

### 5 (hg-k4l.3) report (task-5-report.md)

### 6 (hg-k4l.18) report (task-6-report.md)
- The source-scan test cannot require the brief's strict "only daemon_error.rs and status.rs" yet: who/ops/request/undo (hg-k4l.3) still match ClientError::Unavailable / define remote(), and seat.rs mentions Unavailable in tests. They are listed in AWAITING_HG_K4L_3 (and seat.rs allowed); drop that 
- The scan skips daemon_error.rs itself (its own literals would self-match).

### 7 (hg-k4l.5) report (task-7-report.md)
- Tests live in src/plan/managers/tests.rs (declared from mod.rs) rather than inline, following the codebase's tests.rs pattern.
- apply_pointer_effects ignores effects of other kinds.
- Build needs toolbox ht-build + LIBRARY_PATH workaround on this host.

### 8 (hg-k4l.9) report (task-8-report.md)
- Fallout step 6: hg-k4l.7 not in this stack, so no opt_out needed; no existing test broke.
- reconcile test passes without new reconcile code, as designed (existing planner).
- retire_seat got #[allow(clippy::too_many_arguments)] (8 params).
- Host lacks cc/libz-dev; builds ran in the ht-build toolbox with a libz.so symlink.

### 9 (hg-k4l.11) report (task-9-report.md)
Status: IMPLEMENTED. base 10e78ef4d1aee4f3c2fbb7d35b9929945b4a3228 (stacked on hg-k4l.2, .12, .4), head 6d9027193b06eb90c253a3ceae8890a46bb221eb, tree fd845137b3a409961178e2a143eed513ff05f2a8.
- threads_seat: first Active clone of the seat (all_clones order) with any invitation carrying a link.
- Manager = first element of manager_chain (live managers, retired transparent), per brief; seat with no chain gets null even if node manager exists (node manager itself).
- Task acceptance "cycle surfaces as integrity" covered through Pointers::manager_chain; message contains "manager cycle".
- Environment: distrobox ht-build lacks zlib-devel; worked around via LIBRARY_PATH.

## interfaces and seams

src/bootstrap/mod.rs: pub(crate) fn resolve_error(e: StoreError, head: &CommitId, suffix: Option<&str>) -> CommandError {
src/bootstrap/mod.rs: pub struct SeatRef {
src/bootstrap/mod.rs: pub(crate) fn seat_ref(
src/bootstrap/mod.rs: pub(crate) fn node_manager_suffix(tree: &dyn TreeRead, caller_seat: Option<&SeatId>) -> String {
src/cli/daemon_error.rs: pub const NOT_IN_HERDR: &str =
src/cli/daemon_error.rs: pub const RETRY_LATER: &str =
src/cli/daemon_error.rs: pub const NOT_TRANSIENT: &str = "This is not transient; retrying will not help.";
src/cli/daemon_error.rs: pub enum Outcome {
src/cli/daemon_error.rs: pub struct Failure {
src/cli/daemon_error.rs: pub fn unavailable_line(reason: &str) -> String {
src/cli/daemon_error.rs: pub fn integrity_line(message: &str) -> String {
src/cli/daemon_error.rs: pub fn classify(e: &ClientError, in_herdr: bool) -> Failure {
src/cli/daemon_error.rs: pub fn failure(e: &ClientError) -> Failure {
src/cli/daemon_error.rs: pub fn report(e: ClientError) -> ExitCode {
src/cli/daemon_error.rs: pub fn store_failure(e: &StoreError) -> Option<Failure> {
src/cli/mod.rs: pub mod daemon_error;
src/cli/mod.rs: pub mod node_manager_warning;
src/cli/node_manager_warning.rs: pub fn warning_text() -> String {
src/cli/node_manager_warning.rs: pub fn applies(cmd: &Command) -> bool {
src/cli/node_manager_warning.rs: pub fn check(tree: &dyn TreeRead) -> Option<String> {
src/cli/node_manager_warning.rs: pub fn warn(cmd: &Command) {
src/cli/seat.rs: pub const SEAT_UNAVAILABLE: &str = "Your seat context is not loaded. Tell the user, carry on with what you can without seat-specific rules, and run /seat again later. Do not guess your seat.";
src/cli/seat.rs: pub const SEAT_INTEGRITY: &str = "Your seat context is not loaded. Tell the user, carry on with what you can without seat-specific rules, and do not retry. Do not guess your seat.";
src/cli/seat.rs: pub fn seat_failure(e: &ClientError, in_herdr: bool) -> (u8, String) {
src/daemon/client.rs: pub const EXIT_DAEMON_UNAVAILABLE: u8 = 5;
src/daemon/client.rs: pub const EXIT_INTEGRITY: u8 = 6;
src/daemon/registry.rs: pub fn integrity(m: impl Into<String>) -> Self {
src/daemon/registry.rs: pub fn from_store(e: crate::ports::store::StoreError) -> Self {
src/model/manager.rs: pub struct SeatPointer {
src/model/manager.rs: pub struct Pointers {
src/model/manager.rs: pub enum PointerEdit {
src/model/manager.rs: pub fn load(tree: &dyn TreeRead) -> Result<Self, StoreError> {
src/model/manager.rs: pub fn apply(&mut self, e: &PointerEdit) {
src/model/manager.rs: pub fn name(&self, seat: &SeatId) -> Option<&str> {
src/model/manager.rs: pub fn live_node_manager(&self) -> Option<SeatId> {
src/model/manager.rs: pub fn effective_manager(&self, seat: &SeatId) -> Result<Option<SeatId>, StoreError> {
src/model/manager.rs: pub fn manager_chain(&self, seat: &SeatId) -> Result<Vec<SeatId>, StoreError> {
src/model/manager.rs: pub fn nearest_surviving_manager(
src/model/manager.rs: pub fn cycle_from(&self, subjects: &[SeatId]) -> Option<Vec<SeatId>> {
src/model/manager.rs: pub fn live_node_manager(tree: &dyn TreeRead) -> Result<Option<SeatId>, StoreError> {
src/model/manager.rs: pub fn effective_manager(tree: &dyn TreeRead, seat: &SeatId) -> Result<Option<SeatId>, StoreError> {
src/model/manager.rs: pub fn manager_chain(tree: &dyn TreeRead, seat: &SeatId) -> Result<Vec<SeatId>, StoreError> {
src/model/manager.rs: pub fn nearest_surviving_manager(
src/model/manager.rs: pub fn would_create_cycle(
src/model/mod.rs: pub mod manager;
src/plan/core_kinds.rs: pub(crate) struct SeatActivate;
src/plan/managers/cycle_guard.rs: pub fn refuse_cyclic_chains(tree: &dyn TreeRead, effects: &[PlanEffect]) -> Result<(), PlanError> {
src/plan/managers/edit.rs: pub(super) fn register(reg: &mut KindRegistry) {
src/plan/managers/first_seat.rs: pub fn qualifies(tree: &dyn TreeRead) -> Result<bool, StoreError> {
src/plan/managers/first_seat.rs: pub fn assignment(
src/plan/managers/first_seat.rs: pub fn apply_assignment(
src/plan/managers/first_seat.rs: pub fn forces_active(effects: &[PlanEffect], seat: &SeatId) -> bool {
src/plan/managers/mod.rs: pub mod cycle_guard;
src/plan/managers/mod.rs: pub mod edit;
src/plan/managers/mod.rs: pub mod first_seat;
src/plan/managers/mod.rs: pub mod node_manager;
src/plan/managers/mod.rs: pub mod reparent;
src/plan/managers/mod.rs: pub mod retire;
src/plan/managers/mod.rs: pub const NODE_MANAGER_SET_USAGE: &str = "herdr-graph plan node-manager set <seat>";
src/plan/managers/mod.rs: pub const SEAT_MANAGER: &str = "seat.manager";
src/plan/managers/mod.rs: pub const TEAMSPACE_DEFAULT_MANAGER: &str = "teamspace.default_manager";
src/plan/managers/mod.rs: pub const GRAPH_NODE_MANAGER: &str = "graph.node_manager_seat";
src/plan/managers/mod.rs: pub fn register_kinds(reg: &mut KindRegistry) {
src/plan/managers/mod.rs: pub fn admit(tree: &dyn TreeRead, effects: &[PlanEffect]) -> Result<(), PlanError> {
src/plan/managers/mod.rs: pub fn graph_blob(tree: &dyn TreeRead) -> Result<BlobHash, StoreError> {
src/plan/managers/mod.rs: pub fn graph_relied_on(tree: &dyn TreeRead) -> Result<ReliedOn, StoreError> {
src/plan/managers/mod.rs: pub fn write_graph(tree: &mut Overlay<'_>, g: &GraphRecord) -> Result<(), StoreError> {
src/plan/managers/mod.rs: pub fn seat_manager_effect(
src/plan/managers/mod.rs: pub fn default_manager_effect(
src/plan/managers/mod.rs: pub fn node_manager_effect(
src/plan/managers/mod.rs: pub fn set_seat_manager(
src/plan/managers/mod.rs: pub fn set_default_manager(
src/plan/managers/mod.rs: pub fn set_node_manager(
src/plan/managers/mod.rs: pub fn apply_pointer_effects(
src/plan/managers/mod.rs: pub(crate) mod testing {
src/plan/managers/mod.rs: pub const SET_NODE_MANAGER_KEY: &str = "bookkeeping.test_node_manager";
src/plan/managers/mod.rs: pub struct SetNodeManager;
src/plan/managers/mod.rs: pub fn request(seat: Option<&SeatId>) -> ChangeRequest {
src/plan/managers/mod.rs: pub fn opt_out(root: &std::path::Path) {
src/plan/managers/node_manager.rs: pub(super) fn register(reg: &mut KindRegistry) {
src/plan/managers/reparent.rs: pub const REPARENTED_KEY: &str = "reparented";
src/plan/managers/reparent.rs: pub const REPARENT_FALLBACK_KEY: &str = "reparent_fallback";
src/plan/managers/reparent.rs: pub struct Rewrite {
src/plan/managers/reparent.rs: pub fn to_toml(&self) -> toml::Value {
src/plan/managers/reparent.rs: pub fn from_toml(v: &toml::Value) -> Option<Rewrite> {
src/plan/managers/reparent.rs: pub fn reparent(
src/plan/managers/reparent.rs: pub fn reparent_effects(
src/plan/managers/reparent.rs: pub(crate) mod testing {
src/plan/managers/reparent.rs: pub const SET_POINTER_KEY: &str = "bookkeeping.test_pointer";
src/plan/managers/reparent.rs: pub struct SetPointer;
src/plan/managers/reparent.rs: pub fn recorded(act: &crate::model::action::ActionRecord) -> Vec<super::Rewrite> {
src/plan/managers/reparent.rs: pub fn seat_request(seat: &SeatId, manager: Option<&SeatId>) -> ChangeRequest {
src/plan/managers/reparent.rs: pub fn teamspace_request(ts: &TeamspaceId, manager: Option<&SeatId>) -> ChangeRequest {
src/plan/managers/retire.rs: pub fn refuse_node_manager_retirement(
src/plan/mod.rs: pub mod managers;
src/ports/store.rs: pub fn because(self, reason: impl Into<String>) -> Self {
src/ports/store.rs: pub fn unresolved(
src/threads/mod.rs: pub fn manager_line(chain: &[NamedId]) -> String {
src/threads/mod.rs: pub fn named_manager_chain(tree: &dyn TreeRead, seat: &SeatId) -> Result<Vec<NamedId>, StoreError> {
new public files:
A	src/cli/daemon_error.rs
A	src/cli/node_manager_warning.rs
A	src/model/manager.rs
A	src/plan/managers/cycle_guard.rs
A	src/plan/managers/edit.rs
A	src/plan/managers/first_seat.rs
A	src/plan/managers/mod.rs
A	src/plan/managers/node_manager.rs
A	src/plan/managers/reparent.rs
A	src/plan/managers/retire.rs
A	src/plan/managers/tests.rs
A	tests/daemon_error_commands.rs
A	tests/daemon_free_reads.rs
A	tests/node_manager_warning.rs
A	tests/seat_exit_codes.rs
Seam contract beads in epic tree: none

## deferred minors

Metrics: completions — review clean 0 · after fix pass 0 · parked 0 · re-entry closes 0 · dispatched early 0 · cancelled 0
Task 1 (hg-k4l.1): minor (deferred): [extra-helper] src/ports/store.rs: StoreError::because() goes beyond the brief's 'unresolved is the only constructor helper'; it is a tested modifier, not a constructor
Task 1 (hg-k4l.1): minor (deferred): [unused-new-api] src/daemon/registry.rs: CommandError::from_store has no caller yet; consumers belong to later tasks (writer Fail mapping is hg-k4l.15)
Task 3 (hg-k4l.12): minor (deferred): [short-rev-length] short_rev truncates to 12 chars while the brief example shows 7; harmless
Task 3 (hg-k4l.12): minor (deferred): [no-red-run] no separate failing-test run recorded; tests could not compile before resolve_error existed
Task 2 (hg-k4l.2): minor (deferred): [missing-cli-test] tests/seat_exit_codes.rs: no spawned-binary test for exit 6 or a daemon still starting (Remote{Unavailable}); covered only by classify unit tests (brief permits unit test for 6)
Task 2 (hg-k4l.2): minor (deferred): [stale-fallback-sites] src/cli/ops.rs:97, request.rs:169, status.rs:85, undo.rs:45, who.rs:49 still fall back to direct reads on ClientError::Unavailable; out of this task, belongs to later epic tasks
Task 2 (hg-k4l.2): minor (deferred): [error-text-regression] src/daemon/client.rs:29 NotInHerdr renders 'not inside Herdr' without the 'daemon unavailable:' prefix for non-seat Ensure callers that print via anyhow; benign
Task 4 (hg-k4l.4): minor (deferred): [silent-error-swallow] src/model/manager.rs cycle_from: `while let Ok(Some(n))` ends the walk on Err, so dangling pointers are not reported by would_create_cycle; the ManagerEdit validator must also call manager_chain/effective_manager
Task 4 (hg-k4l.4): minor (deferred): [missing-evidence] task-4-report.md has test command and pass summary in prose only, no structured evidence array
Task 6 (hg-k4l.18): minor (deferred): [deferred-call-sites] src/cli/ops.rs:97,102,186; src/cli/request.rs:136,169; src/cli/undo.rs:33 still match ClientError::Unavailable or define remote(); the brief assigns them to hg-k4l.3 and the source-scan guard allow-lists them (AWAITING_HG_K4L_3), so the bead's 'every command' criterion is complete only once hg-k4l.3 merges and that list is dropped
Task 6 (hg-k4l.18): minor (deferred): [missing-evidence] report gives test commands and pass counts but no structured evidence records (sourceTree/kind/outcome)
Task 6 (hg-k4l.18): minor (deferred): [test-scan-fragility] src/cli/daemon_error.rs no_command_matches_unavailable_itself is a substring scan and allows seat.rs and status.rs wholesale
Task 5 (hg-k4l.3): minor (deferred): [missing-evidence] report gives test commands and results in prose only, no structured evidence records with sourceTree
Task 5 (hg-k4l.3): minor (deferred): [missing-edge-case-test] exit-5 paths for mutating commands (cancel, reassign, request ack/complete, undo apply) are not covered by a test
Task 7 (hg-k4l.5): minor (deferred): [missing-edge-case-test] src/plan/managers/mod.rs set_seat_manager and set_default_manager have no direct unit test; only set_node_manager and apply_pointer_effects (node-manager kind) are exercised
Task 7 (hg-k4l.5): minor (deferred): [unused-constant] NODE_MANAGER_SET_USAGE has no consumers yet; later beads must use it as the only spelling
Task 9 (hg-k4l.11): minor (deferred): [suffix-read-scope] src/bootstrap/mod.rs node_manager_suffix uses Pointers::load (all seats/teamspaces), so an unrelated corrupt record degrades to the user form instead of reading graph.toml alone
Task 9 (hg-k4l.11): minor (deferred): [missing-evidence] report has no structured evidence array and no captured TDD red run
Task 8 (hg-k4l.9): minor (deferred): [too-many-arguments] src/observe/mutations.rs:161 retire_seat grew to 8 params and needed allow(clippy::too_many_arguments); consider bundling args in a later pass
Task 10 (hg-k4l.16): minor (deferred): [duplicated-test-helper] src/threads/tests.rs SetManager and src/bootstrap/tests.rs TestManager are near-identical test mutations; hg-k4l.11 helpers could replace both when they land
Task 10 (hg-k4l.16): minor (deferred): [missing-integration-test] no integration test covers daemon-backed who output with the manager line or the show exit-6 path end to end (report concern: tests/* who cases not run)
Task 14 (hg-k4l.10): minor (deferred): [weak-assertion] tests/node_manager_warning.rs: assert!(stdout(&before).contains("s")) is near-vacuous
Task 14 (hg-k4l.10): minor (deferred): [missing-edge-case-test] no integration test for the no-instance path printing nothing
Task 14 (hg-k4l.10): minor (deferred): [test-duplication] git2 commit helper duplicated between src/cli/node_manager_warning.rs unit tests and tests/node_manager_warning.rs
Task 15 (hg-k4l.13): minor (deferred): [missing-edge-case-test] src/plan/managers/edit.rs:57-80: retired subject seat/teamspace rejection branches are untested
Task 13 (hg-k4l.8): minor (deferred): [no-red-run] tests were written after the implementation; no failing-first run captured in the report
Task 26 (hg-k4l.26): minor (deferred): [missing-edge-case-test] src/plan/managers/reparent.rs:~75: lenient cascade fallback with no live node manager (target None) is untested
Task 11 (hg-k4l.6): minor (deferred): [weak-assertion] src/plan/tests.rs node_manager_set_rejects_missing_retired_unchanged: 'node-manager set nobody' only checks that an error is returned, not its message
Task 11 (hg-k4l.6): minor (deferred): [plan-race-gap] src/plan/managers/node_manager.rs: seats/teamspaces created after planning that point at the old node manager are not covered by relied_on and would be left unrewritten
Task 23 (hg-k4l.23): minor (deferred): [missing-red-run] TDD RED output not captured; report admits implementation preceded tests
Task 23 (hg-k4l.23): minor (deferred): [test-placement] non-retired-seat qualifies cases tested in src/plan/tests.rs rather than first_seat.rs unit tests as the brief listed
Task 17 (hg-k4l.21): minor (deferred): [missing-evidence] report gives commands and pass counts but no sourceTree-bound evidence records
Task 17 (hg-k4l.21): minor (deferred): [brief-deviation] seat.resurrect applied as un-retire instead of PointerEdit::Live per brief table; justified in report (keeps explicit manager)
Task 17 (hg-k4l.21): minor (deferred): [missing-red-run] failing-first run not performed separately for the refusal tests
Task 27 (hg-k4l.27): minor (deferred): [preview-self-rewrite] TeamspaceRetire preview (src/plan/core_kinds.rs ~739) can list a default_manager rewrite for the teamspace being retired itself; harmless, apply does the same

## amendments

hg-k4l.1:  Comments on hg-k4l.1: [Alexey Parfenov] at 2026-11-05 08:45 The blocker is in the build environment, not the task. The task's code is complete and committed (71c8f2a, "ports/ipc: StoreError::Integrity, IpcErrorCode::Integrity, CommandError::from_store classifier (hg-k4l.1)"). Both merge-check cargo
hg-k4l.15:  Comments on hg-k4l.15: [Alexey Parfenov] at 2026-11-05 09:07 split into: hg-k4l.26, hg-k4l.27 
hg-k4l.17:  Comments on hg-k4l.17: [Alexey Parfenov] at 2026-11-05 09:56 This is a dependency-ordering problem in the pipeline. Nothing in hg- k4l.17's requirements is unclear, so the task should wait for hg-k4l.44 to merge rather than be re-dispatched now. Root cause: task-hg-k4l.27 was stacked on the pre-rev
hg-k4l.24:  Comments on hg-k4l.24: [Alexey Parfenov] at 2026-11-05 09:40 Clarification for the hg-k4l.24 re-dispatch. Sources used: bd show hg-k4l.24 and bd comments hg-k4l.24 (no comments, no amendments), plan Task 24 (ordinal 24 in .superpowers/sdd/hg-k4l-plan/hg-k4l-plan.md), and blocker hg- idv. Nothing in
hg-k4l.7:  Comments on hg-k4l.7: [Alexey Parfenov] at 2026-11-05 09:07 split into: hg-k4l.23, hg-k4l.24, hg-k4l.25 
