## diffstat

 .../2026-10-31-h1-instruction-gate-design.md       |  253 ++
 ...truction-gate-roast-design-1-step-back-keys.txt |    5 +
 ...h1-instruction-gate-roast-design-1-step-back.md |    8 +
 ...026-10-31-h1-instruction-gate-roast-design-1.md |   97 +
 ...026-10-31-h1-instruction-gate-roast-design-2.md |   79 +
 .../coverage-findings-round-1.md                   |   12 +
 .../coverage-findings-round-2.md                   |   17 +
 .../coverage-ledger.md                             |   37 +
 .../coverage-round-1-inputs.md                     |   40 +
 .../coverage-round-1-precheck.md                   |    3 +
 .../coverage-round-1-requirements.md               |   20 +
 .../coverage-round-1-review-1.md                   |   22 +
 .../coverage-round-1-review-2.md                   |   21 +
 .../coverage-round-2-inputs.md                     |   45 +
 .../coverage-round-2-precheck.md                   |    1 +
 .../coverage-round-2-review-1.md                   |   24 +
 .../coverage-round-2-review-2.md                   |   25 +
 .../design-loop-notes.md                           |    4 +
 .../findings-ledger-design.md                      |    9 +
 .../findings-overrides-design.md                   |    0
 .../2026-10-31-h1-instruction-gate/friction.md     |    7 +
 .../input-h1-review.md                             |  184 ++
 .../profile-inv-p3.json                            | 2916 ++++++++++++++++++++
 .../profile-inv-p3.md                              |  304 ++
 .../promotion-review-input.md                      |  130 +
 .../runs/2026-10-31-h1-instruction-gate/run.md     |   45 +
 docs/superpowers/specs/INDEX.md                    |    1 +
 evidence/hg-45e.18/lib-tests.log                   |    5 +
 evidence/hg-45e.6/bootstrap-tests.log              |    7 +
 evidence/hg-45e.7/tests.log                        |   37 +
 evidence/hg-45e.9/features.log                     |   44 +
 evidence/hg-45e.9/full.log                         |   37 +
 evidence/hg-45e.9/tests.log                        |   17 +
 skills/seat/SKILL.md                               |   27 +-
 src/bootstrap/content.rs                           |  276 --
 src/bootstrap/instructions.rs                      |  509 ++++
 src/bootstrap/mod.rs                               |  198 +-
 src/bootstrap/setup_claude.rs                      |  311 ++-
 src/bootstrap/tests.rs                             | 1191 ++++++--
 src/cli/content.rs                                 |  109 +-
 src/cli/instructions.rs                            |  280 ++
 src/cli/mod.rs                                     |    5 +
 src/cli/plan.rs                                    |   85 +-
 src/cli/request.rs                                 |   49 +-
 src/cli/seat.rs                                    |  161 +-
 src/cli/setup.rs                                   |   16 +
 src/cli/undo.rs                                    |    2 +-
 src/config.rs                                      |    6 +
 src/daemon/compose.rs                              |    1 -
 src/daemon/doctor.rs                               |  185 +-
 src/model/action.rs                                |    1 +
 src/model/b64.rs                                   |   41 +
 src/model/change.rs                                |    4 +
 src/model/common.rs                                |    2 +-
 src/model/graph.rs                                 |    9 +-
 src/model/ids.rs                                   |   15 +-
 src/model/mod.rs                                   |    1 +
 src/model/tests.rs                                 |   88 +
 src/plan/commands.rs                               |    5 +
 src/plan/core_kinds.rs                             |    1 +
 src/plan/instructions.rs                           |  313 +++
 src/plan/kinds_extra.rs                            |   20 +-
 src/plan/line_diff.rs                              |  128 +
 src/plan/mod.rs                                    |    2 +
 src/plan/tests.rs                                  |  385 +++
 src/plan/types.rs                                  |   21 +-
 src/store/instruction_source.rs                    |  567 ++++
 src/store/layout.rs                                |   35 +-
 src/store/mod.rs                                   |    1 +
 src/store/tests.rs                                 |  480 ++++
 src/templates/document.rs                          |   18 +
 src/templates/kinds.rs                             |   87 +-
 src/templates/tests.rs                             |   73 +-
 src/threads/effects.rs                             |    2 +-
 src/transcripts/mod.rs                             |   32 +-
 src/transcripts/mutations.rs                       |   95 +-
 src/transcripts/tests.rs                           |  189 ++
 src/undo/candidates.rs                             |   14 +-
 src/undo/compensate.rs                             |   57 +-
 src/undo/mod.rs                                    |    4 +
 src/undo/preview.rs                                |   63 +-
 src/undo/tests.rs                                  |  151 +-
 src/writer/mutation.rs                             |    9 +-
 src/writer/tests.rs                                |   34 +-
 .../system-summarizer/members/summarizer/AGENTS.md |   14 +-
 tests/daemon_composed.rs                           |   14 +-
 tests/integration_sweep.rs                         |   57 +-
 87 files changed, 9980 insertions(+), 919 deletions(-)


## generated-input provenance

## Generated-input provenance
Captured range: c77d9ad166dd2ada5b5a5768b499d12da4c36987..bce6589afead4c031ac9ecfa3a094c9a5c4be161
Selection: complete; selected product/input paths: 87; omitted historical payloads: 0.
diffstat.txt describes selected product/input paths. Reports, findings and collection metadata remain ordinary review inputs.
Open each report/archive at its recorded revision. Omission supplies no finding disposition or verdict.


## report summaries

### task 1
Status: none
Assumption: - third_party/herdr-threads is an empty (uninitialised) submodule dir in the worktree; to build I copied the sources from the main checkout's populated dir into the worktree's third_party/herdr-threads (untracked content of a gitlink, not committed; target/ also seeded per setup).
Assumption: - No amendments in bead or comments (none exist).
### task 2
Status: none
Concerns: none.
### task 3
Status: none
Assumption: - Fresh worktree had an empty third_party/herdr-threads submodule; initialised it with `git submodule update --init` from the local checkout (no tree change).
Assumption: - "One commit" is evidenced by the file being absent and request not Completed at the pre-call head.
Assumption: - No bead amendments or comments existed.
### task 4
Status: none
Assumption: - Fresh worktree lacked the third_party/herdr-threads submodule (also empty in the integration worktree); initialised it in my worktree from the local main checkout (`git -c submodule.herdr-threads.url=<local> submodule update --init`); .gitmodules unchanged.
Assumption: - Undo's agent-marker path is covered via prompt_terminal/decide unit tests, not an undo-specific test (undo() reads stdin directly).
Assumption: - daemon `caller.tty` (src/daemon/client.rs) still reflects the raw stdin tty; the CLI never sends mode "tty" under a marker, but the daemon does not itself enforce it. Out of scope here.
Assumption: - Pre-existing rules file at rules/herdr-graph.rules is treated as owned and overwritten (Updated), then removed on uninstall.
### task 5
Status: none
Assumption: - third_party/herdr-threads submodule was empty in the fresh worktree; copied contents from the main checkout (same commit cdc8423) minus its .git file to build. Untracked by git status; not committed.
Assumption: - Rebase was a no-op (integration tip == base).
### task 6
Status: Status: IMPLEMENTED. base e6ac7a637d825235ed25a0f8aaae6feb0deee70e (stack: hg-45e.1 merged), head d389924aa74b92159f21ed9c1e6b2621e5f853b9, tree 7efb97f6d4ad91c5337dd2e48327d514b8878da7. stacked=true.
Assumptions / Concerns:
### task 7
Status: Status: IMPLEMENTED. Base 7b4b7d492a439c6cc4cb1dde567130e7cef8821d (stack: hg-45e.3), head 75980c2df3fe524f3198c8d82e096f71b81ce745, stacked true. Source tree 941a37277bcb400167c4b3dd4f20605c81a1e3c7.
Assumption: - Setup: the fresh worktree's submodule third_party/herdr-threads was empty (the known pre-flight gap); I ran `git submodule update --init --reference <integration worktree copy>` inside the task worktree so cargo could resolve it (offline).
Assumption: - cap_warning with both omitted parts and a cut part joins both clauses with "; " (brief specifies only the single-part cut wording).
Assumption: - A seat template_ref whose template record is missing is skipped (the old code still listed the record path); unreachable in practice since locate depends on the record.
Assumption: - fit_hook cannot honor the cap if cap is smaller than header+revision line+marker (not reachable with HOOK_RENDER_CAP).
### task 8
Status: none
Assumption: - Fresh worktree had empty third_party/herdr-threads submodule dir (cargo could not load the path dep); I copied the files from the integration worktree (read-only source) into the task worktree's untracked submodule dir, removing the copied .git file. git status is clean.
Assumption: - Dirty-working-tree test writes to the temp instance only.
### task 9
Status: Status IMPLEMENTED, stacked on hg-45e.6 and hg-45e.2. base 434273b10459fae34d750fb206319bdced589b08, head 4fdccf0291350e97384c4224d15cc884c81c9f79.
Assumption: - third_party/herdr-threads submodule was empty in the fresh worktree; populated by copying files from the integration worktree (same commit cdc8423) so cargo resolves. Not a tracked change.
Assumption: - Staleness relies on effects before_sha (same_effects); plan relied_on carries the Blob version but the apply request leaves relied_on empty by existing design.
Assumption: - No bead amendments or comments existed for hg-45e.4.
### task 10
Status: none
### task 11
Status: none
Assumptions: hook_text on Bound-but-render-error falls to today's prompt logic. Existing empty-matcher assertion in setup test also present; a dedicated one added per brief.
### task 12
Status: none
Assumptions: stale-binding resolutions are still bound, so they carry instructions; ambiguity tested via two clones on one pane.
Concerns: none.
### task 13
Status: none
### task 14
Status: none
Assumptions: seats created by the edit itself are not sized (only existing seats in the committed tree); "live" = not Retired. third_party/herdr-threads was empty in the fresh worktree; copied from the integration worktree (submodule, not committed) to build.
### task 15
Status: none
Assumption: - Setup: the worktree needed `git -c protocol.file.allow=always submodule update --init --no-fetch` (third_party/herdr-threads) before cargo would resolve; the setup text lacked this step.
Assumption: - Brief step 4 suggested `put_files` committing directly through fx.store. I used a test-only writer mutation `bookkeeping.test_put` (like TestClone) so the working tree is synced and the dirty-local-edit protection in seat_json test still exercises real writer behavior. Same intent, different mecha
Assumption: - Task text listed stack parent hg-45e.3 twice; hg-45e.8 is already merged on the integration branch.
Assumption: - Legacy test admits through `Journal::admit` directly to get a journaled row, since `WriterCore::admit` now refuses the kind up front (both paths asserted).


## interfaces and seams

src/bootstrap/instructions.rs: pub const HOOK_RENDER_CAP: usize = 9000;
src/bootstrap/instructions.rs: pub struct Part {
src/bootstrap/instructions.rs: pub struct RenderedInstructions {
src/bootstrap/instructions.rs: pub struct Participation {
src/bootstrap/instructions.rs: pub fn template_participation(
src/bootstrap/instructions.rs: pub fn member_agents_path(p: &Participation, member: &TemplateMember) -> RepoPath {
src/bootstrap/instructions.rs: pub fn short_rev(at: &str) -> &str {
src/bootstrap/instructions.rs: pub fn revision_line(name: &str, seat: &SeatId, short_rev: &str) -> String {
src/bootstrap/instructions.rs: pub fn render_instructions(
src/bootstrap/instructions.rs: pub fn hook_header(r: &RenderedInstructions) -> String {
src/bootstrap/instructions.rs: pub fn hook_size(r: &RenderedInstructions) -> usize {
src/bootstrap/instructions.rs: pub struct HookFit {
src/bootstrap/instructions.rs: pub fn fit_hook(r: &RenderedInstructions, cap: usize) -> HookFit {
src/bootstrap/instructions.rs: pub fn cap_warning(
src/bootstrap/instructions.rs: pub struct PatchedTree<'a> {
src/bootstrap/instructions.rs: pub fn new(base: &'a dyn TreeRead) -> Self {
src/bootstrap/instructions.rs: pub fn put(&mut self, p: RepoPath, b: Vec<u8>) {
src/bootstrap/instructions.rs: pub fn delete(&mut self, p: RepoPath) {
src/bootstrap/mod.rs: pub mod instructions;
src/bootstrap/setup_claude.rs: pub enum ApprovalGate {
src/bootstrap/setup_claude.rs: pub fn ask_entries(binary: &Path) -> Vec<String> {
src/bootstrap/setup_claude.rs: pub fn rules_text(binary: &Path) -> String {
src/bootstrap/setup_claude.rs: pub fn gate_installed(target: &Target, config: &Path) -> anyhow::Result<bool> {
src/bootstrap/setup_claude.rs: pub fn owned_hook_installed(target: &Target, config: &Path) -> anyhow::Result<bool> {
src/cli/content.rs: pub const STUB_MESSAGE: &str = "content write was removed: summaries use 'herdr-graph request complete <rq> --summary-from <file> --covered <a-b>'; instructions use 'herdr-graph plan instructions set …' then 'herdr-graph apply'";
src/cli/content.rs: pub fn run(_cmd: Commands) -> anyhow::Result<ExitCode> {
src/cli/instructions.rs: pub enum Commands {
src/cli/instructions.rs: pub enum InstructionsAction {
src/cli/instructions.rs: pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
src/cli/instructions.rs: pub fn show_text(view: &dyn TreeRead, words: &[String]) -> anyhow::Result<String> {
src/cli/mod.rs: pub mod instructions;
src/cli/plan.rs: pub const AGENT_MARKERS: [&str; 4] = [
src/cli/plan.rs: pub fn agent_marker_present(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> bool {
src/cli/plan.rs: pub fn prompt_terminal(stdin_tty: bool, get: impl Fn(&str) -> Option<std::ffi::OsString>) -> bool {
src/cli/plan.rs: pub fn relay_hint(plan: &str, hash: &str) -> String {
src/cli/seat.rs: pub fn hook_output_text(codex: bool, text: &str) -> String {
src/cli/seat.rs: pub fn hook_output(codex: bool) -> String {
src/cli/seat.rs: pub fn instructions_text(
src/cli/seat.rs: pub fn hook_text(
src/daemon/doctor.rs: pub fn harness_gate_checks(claude: Option<&Path>, codex: Option<&Path>) -> Vec<Check> {
src/model/b64.rs: pub fn b64_encode(bytes: &[u8]) -> String {
src/model/b64.rs: pub fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
src/model/graph.rs: pub fn graph_id(&self) -> Result<GraphId, IdError> {
src/model/ids.rs: pub const ALL: [(IdKind, &'static str); 14] = [
src/model/ids.rs: pub fn from_instance_id(instance_id: &str) -> Result<Self, IdError> {
src/model/mod.rs: pub mod b64;
src/plan/instructions.rs: pub struct InstructionsKind;
src/plan/line_diff.rs: pub const CONTEXT: usize = 3;
src/plan/line_diff.rs: pub fn unified(before: &str, after: &str) -> String {
src/plan/mod.rs: pub mod instructions;
src/plan/mod.rs: pub mod line_diff;
src/plan/types.rs: pub fn diff(&self) -> Option<&str> {
src/store/instruction_source.rs: pub enum InstructionScope {
src/store/instruction_source.rs: pub enum InstructionFile {
src/store/instruction_source.rs: pub enum ScopeKind {
src/store/instruction_source.rs: pub enum FolderKind {
src/store/instruction_source.rs: pub enum PathClass {
src/store/instruction_source.rs: pub struct InstructionRef {
src/store/instruction_source.rs: pub struct InstructionTarget {
src/store/instruction_source.rs: pub struct ListedFile {
src/store/instruction_source.rs: pub struct AffectedSeat {
src/store/instruction_source.rs: pub enum InstructionError {
src/store/instruction_source.rs: pub struct Inadmissible {
src/store/instruction_source.rs: pub fn word(self) -> &'static str {
src/store/instruction_source.rs: pub fn parse(scope: ScopeKind, word: &str) -> Result<Self, InstructionError> {
src/store/instruction_source.rs: pub fn rel(&self) -> String {
src/store/instruction_source.rs: pub fn kind(&self) -> ScopeKind {
src/store/instruction_source.rs: pub fn object_id(&self, tree: &dyn TreeRead) -> Result<AnyId, InstructionError> {
src/store/instruction_source.rs: pub fn parse_words(
src/store/instruction_source.rs: pub fn resolve_scope(
src/store/instruction_source.rs: pub fn scope_folder(
src/store/instruction_source.rs: pub fn path(tree: &dyn TreeRead, t: &InstructionTarget) -> Result<RepoPath, InstructionError> {
src/store/instruction_source.rs: pub fn list_files(
src/store/instruction_source.rs: pub fn affected_seats(
src/store/instruction_source.rs: pub fn classify(folder: FolderKind, rel: &str) -> PathClass {
src/store/instruction_source.rs: pub fn is_instruction_source(folder: FolderKind, rel: &str) -> bool {
src/store/instruction_source.rs: pub fn check_name(scope: ScopeKind, rel: &str) -> Result<InstructionFile, Inadmissible> {
src/store/instruction_source.rs: pub fn check_content(bytes: &[u8]) -> Result<&str, Inadmissible> {
src/store/layout.rs: pub const RECORD_FILES: &[&str] = &[
src/store/layout.rs: pub fn graph_id(tr: &dyn TreeRead) -> Result<GraphId, StoreError> {
src/store/mod.rs: pub mod instruction_source;
src/templates/document.rs: pub fn instruction_texts(&self) -> Vec<(String, &str)> {
src/undo/preview.rs: pub fn instructions_inverse(tree: &dyn TreeRead, act: &ActionRecord) -> Result<Inverse, PlanError> {

new public files:
src/bootstrap/instructions.rs
src/cli/instructions.rs
src/model/b64.rs
src/plan/instructions.rs
src/plan/line_diff.rs
src/store/instruction_source.rs

Seam contract beads:
hg-45e.1: Seam contract: GraphId (IdKind::Graph)


## deferred minors

Metrics: completions — review clean 0 · after fix pass 0 · parked 0 · re-entry closes 0 · dispatched early 0 · cancelled 0
Task 3 (hg-45e.8): minor (deferred): [repeat-complete-not-guarded] src/transcripts/mutations.rs write_summary: a request already completed via --output and then re-completed with --summary-from for the same range (no existing file) is written and re-completed rather than rejected; no test covers it
Task 3 (hg-45e.8): minor (deferred): [missing-evidence] report gives test results as prose only, no structured evidence records with source tree per command
Task 5 (hg-45e.17): minor (deferred): [possibly-stale-lint-allow] src/templates/kinds.rs:~392 #[allow(clippy::too_many_arguments)] on create_member_seat after dropping a parameter; may still be needed (8 args), unverified
Task 4 (hg-45e.10): minor (deferred): [missing-edge-case-test] src/cli/undo.rs:72 undo marker path has no undo-specific test; covered only via prompt_terminal/decide unit tests
Task 4 (hg-45e.10): minor (deferred): [overwrites-preexisting-file] src/bootstrap/setup_claude.rs: a pre-existing rules/herdr-graph.rules is overwritten and removed on uninstall (disclosed in report)
Task 4 (hg-45e.10): minor (deferred): [loose-presence-check] src/bootstrap/setup_claude.rs gate_installed RulesFile uses substring match on prefix_rule( and "apply"
Task 4 (hg-45e.10): minor (deferred): [daemon-not-enforcing] src/daemon/client.rs:89 caller.tty still reflects raw stdin tty; daemon does not enforce the agent marker (disclosed, out of scope)
Task 4 (hg-45e.10): minor (deferred): [no-red-run] report captured no separate RED test run (TDD evidence gap, GREEN shown)
Task 6 (hg-45e.3): minor (deferred): [classification-precedence] src/store/instruction_source.rs classify(): RECORD_FILES check runs before the instruction-location check, so rules/seat.toml is Record not Instruction; check rules/ and AGENTS.md first
Task 6 (hg-45e.3): minor (deferred): [impl-placement] src/store/instruction_source.rs: manual Clone/PartialEq/Eq for StoreError belong in src/ports/store.rs; duplicate-impl risk if another task adds them
Task 6 (hg-45e.3): minor (deferred): [dotdir-handling] src/store/instruction_source.rs walk(): dotfiles skipped by basename only, files under dot-directories inside rules/ are still listed
Task 6 (hg-45e.3): minor (deferred): [deferred-wiring] doctor ignoring rules/.gitkeep is not wired in this task (doctor wiring is a later plan task, ~plan line 1311); predicate and list_files are tested on a fresh init instance
Task 6 (hg-45e.3): minor (deferred): [missing-evidence] report has no structured evidence array; test command and pass summary are in prose only
Task 2 (hg-45e.2): minor (deferred): [fallback-quality] src/plan/line_diff.rs:15 oversize inputs (n*m > 4M) degrade to full delete/insert rather than a minimal diff; acceptable guard but undocumented in tests
Task 7 (hg-45e.6): minor (deferred): [missing-edge-case-test] no test that an inadmissible member AGENTS.md or responsibility renders the placeholder (src/bootstrap/instructions.rs member branch)
Task 7 (hg-45e.6): minor (deferred): [spec-gap] cap_warning joins omitted and cut clauses with '; ' when both occur; wording not specified by brief (src/bootstrap/instructions.rs cap_warning)
Task 7 (hg-45e.6): minor (deferred): [thin-evidence] evidence/hg-45e.6/bootstrap-tests.log holds only the summary line, not per-test names
Task 7 (hg-45e.6): minor (deferred): [behavior-drift] bound_paths now skips a seat template_ref whose template record is unreadable; old code still listed the record path (unreachable in practice)
Task 8 (hg-45e.13): minor (deferred): [missing-edge-case-test] src/cli/instructions.rs:33 run() no-instance exit path and exit codes for errors not tested end-to-end; only show_text is covered
Task 15 (hg-45e.9): minor (deferred): [test-mechanism-deviation] src/bootstrap/tests.rs: fixtures use a test-only bookkeeping.test_put mutation instead of the brief's put_files; same intent, accepted
Task 9 (hg-45e.4): minor (deferred): [missing-evidence] report log evidence/t2.log is not committed and no structured evidence records were supplied; test commands are listed in the report only
Task 9 (hg-45e.4): minor (deferred): [missing-edge-case-test] no test for the 'seat AGENTS.md will be cut' cap-warning branch, CRLF content, or --from resolved relative to caller cwd (src/plan/tests.rs)
Task 11 (hg-45e.7): minor (deferred): [committed-evidence-file] evidence/hg-45e.7/tests.log is committed into the repo tree rather than kept in the plan workspace
Task 11 (hg-45e.7): minor (deferred): [weak-source-evidence] Codex SessionStart/PostCompact coverage claim is derived from binary strings of codex 0.163.0, not documentation; recorded as a code comment
Task 12 (hg-45e.14): minor (deferred): [suite-not-run] cargo test green criterion covered only by targeted bootstrap::tests run (51 passed) plus cargo check
Task 12 (hg-45e.14): minor (deferred): [missing-edge-case-test] no seat-reply test that dotfiles like rules/.gitkeep are absent from paths.data (classify covers it in hg-45e.3)
Task 14 (hg-45e.18): minor (deferred): [doc-drift] src/templates/kinds.rs ~1219: the doc comment 'The edited document as stored: ids resolved, agents_md kept only where the edit sets it.' now sits above edit_cap_warnings instead of merged_after
Task 14 (hg-45e.18): minor (deferred): [missing-edge-case-test] src/plan/kinds_extra.rs SeatOverride::plan: cap warning is computed only when instructions_sections is set; clearing sections is not sized (shrinks, so harmless)
Task 14 (hg-45e.18): minor (deferred): [missing-edge-case-test] src/templates/kinds.rs edit_cap_warnings: seats created by the edit itself are not sized (stated assumption in report)
Task 10 (hg-45e.5): minor (deferred): [lossy-utf8-read] src/undo/preview.rs:instructions_inverse reads the current file with from_utf8_lossy; a non-UTF-8 file compares unequal and yields a conflict, which is acceptable but unstated


## amendments

none
