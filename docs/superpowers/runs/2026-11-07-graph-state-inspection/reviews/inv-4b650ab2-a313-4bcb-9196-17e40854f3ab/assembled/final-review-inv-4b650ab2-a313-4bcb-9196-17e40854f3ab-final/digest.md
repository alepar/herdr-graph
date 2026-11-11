## diffstat

 .../2026-11-07-graph-state-inspection-design.md    |  283 +++-
 ...te-inspection-roast-design-1-step-back-keys.txt |   16 +
 ...ph-state-inspection-roast-design-1-step-back.md |   14 +
 ...-11-08-graph-state-inspection-roast-design-1.md |  147 ++
 ...-11-08-graph-state-inspection-roast-design-2.md |   52 +
 .../amend-design-1.md                              |   48 +
 .../coverage-amend-design-1-dispositions.md        |   27 +
 .../coverage-findings-round-1.md                   |   14 +
 .../coverage-findings-round-2.md                   |   16 +
 .../coverage-ledger.md                             |   36 +
 .../coverage-round-1-requirements.md               |   15 +
 .../coverage-round-1-review-1.md                   |   30 +
 .../coverage-round-1-review-2.md                   |   30 +
 .../coverage-round-2-review-1.md                   |   20 +
 .../coverage-round-2-review-2.md                   |   19 +
 .../findings-ledger-design.md                      |   24 +
 .../findings-overrides-design.md                   |    3 +
 .../2026-11-07-graph-state-inspection/friction.md  |    3 +
 .../runs/2026-11-07-graph-state-inspection/run.md  |   37 +
 src/cli/log.rs                                     |  245 +++
 src/cli/mod.rs                                     |   58 +
 src/cli/ops.rs                                     |  214 ++-
 src/cli/roster.rs                                  |  194 +++
 src/cli/sessions.rs                                |  168 ++
 src/daemon/compose.rs                              |   10 +
 src/daemon/mod.rs                                  |    1 +
 src/daemon/roster_live.rs                          |  224 +++
 src/herdr/fake.rs                                  |    9 +
 src/inspect/fixture.rs                             |  487 ++++++
 src/inspect/inventory.rs                           |  533 +++++++
 src/inspect/log.rs                                 |  918 +++++++++++
 src/inspect/mod.rs                                 |  163 ++
 src/inspect/output.rs                              |  291 ++++
 src/inspect/roster.rs                              | 1618 ++++++++++++++++++++
 src/inspect/scope.rs                               |  732 +++++++++
 src/inspect/sessions.rs                            |  452 ++++++
 src/lib.rs                                         |    1 +
 src/plan/ops.rs                                    |  295 +++-
 src/store/history.rs                               |  576 +++++++
 src/store/instruction_source.rs                    |   75 +-
 src/store/mod.rs                                   |    1 +
 src/writer/recovery.rs                             |   15 +-
 tests/daemon_composed.rs                           |   13 +-
 tests/daemon_free_reads.rs                         |    6 +
 tests/inspect_live.rs                              |  222 +++
 tests/inspect_log.rs                               |   61 +
 tests/inspect_roster.rs                            |   59 +
 tests/inspect_sessions.rs                          |   51 +
 48 files changed, 8400 insertions(+), 126 deletions(-)

## generated-input provenance

## Generated-input provenance
Captured range: bfeb8f3104dd3783d67a46570c9f272e935012b5..6eb6cec786cde8ca070fbf9aed46bb0609ff63ac
Selection: complete; selected product/input paths: 48; omitted historical payloads: 0.
diffstat.txt describes selected product/input paths. Reports, findings and collection metadata remain ordinary review inputs.
- Outside captured range: 46 informational entries (neither selected nor waived); the first 20 are listed; the full list is review-inputs.json outOfRange.
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:86 inv-p3b-20261031T222244: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:22 inv-e10f983d-1d5e-4b01-a12c-2978e5f10eb2: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:22 inv-e10f983d-1d5e-4b01-a12c-2978e5f10eb2: incomplete claimed final report
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:23 inv-2e07a039-6297-4308-8862-f3a2878fe7ab: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:23 inv-2e07a039-6297-4308-8862-f3a2878fe7ab: incomplete claimed final report
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:24 inv-8d1468f1-4551-46bd-8b96-82617c3ea922: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:24 inv-8d1468f1-4551-46bd-8b96-82617c3ea922: incomplete claimed final report
- Outside captured range (informational; neither selected nor waived): bfeb8f3104dd3783d67a46570c9f272e935012b5:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:88 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete claimed collection
Open each report/archive at its recorded revision. Omission supplies no finding disposition or verdict.

## report summaries

### task 1
- Status: IMPLEMENTED. base e82aa4f0a93d0011c1e5027938ffdca99b42a507, head 6e829ab05f16f151438a7c648d54eef6a6fff5a1, sourceTree eb4ab8f3c036a46a082f2d554b2aa9916e9c9d87. Not stacked.
- Brief says Scope parse of `global x` is Usage; parse_words would give NotAllowed, so ScopeArg::parse pre-checks word count (global 1, teamspace/seat 2, else Usage "too many words").
- Fixture: `clone_rec_id` and `set_runtime`/`clone_mut` added beyond the brief so transcript records can reference a clone id and tests can set runtime/corrupt a parent id. `Fx::dir` omitted (unused).
- Inventory returns an empty tree for Scope::Global before any read.
- Dangling parent check covers all enumerated seats and clones, regardless of scope (except Global).
- `cargo fmt` reformatted four unrelated test files; those were reverted, not committed.
- InstructionsView/Inventory types are unused outside tests until consumer beads land; no dead_code warning appears because they are `pub`.
- stop-run-processes check unusable here (see Test hygiene).
### task 2
- Status: Task 2 (hg-blw.1) report
- Assumptions: ThreadsView.state is the SeatFactState word (resolved/unresolved/key_conflict/retired) as a string, matching spec JSON
- Concerns: `cargo fmt` rewrote unrelated files; reverted. Pre-existing clippy warning in src/transcripts/mutations.rs untouched.
### task 3
- Status: Task 3 (hg-blw.3) report
- `native_session_id` is `String` in NativeSession (brief said Option); row uses String.
- `cargo fmt` touched four unrelated test files; reverted before commit.
- Pre-existing warning: `field herdr is never read` in src/threads/tests.rs.
- Text path column appends one ` [retired]` if any of the three objects is retired.
### task 4
- Status: Task 4 (hg-blw.5) report
- LogQuery::run returns (LogView, warnings); the ops pointer is passed to emit as the only notice (OPS_POINTER), so warnings go to stderr in text mode too (acceptance: one stderr warning).
- Instance scope matches every change (after --by and bookkeeping filters).
- Missing op/action record for a present trailer counts as malformed.
- Row summary is the operation's summary when readable, else the commit subject.
- paths_at is evaluated per scanned commit for non-instance scopes (list+read of records each time); may be slow at 5000 commits on big instances.
- cargo fmt reformats unrelated files in this repo; I reverted them.
### task 5
- Status: Task 5 (hg-blw.8) report
- Assumptions: daemon_free_reads fixture has no seat, so only unfiltered ops flags are added there (unknown name exits 1 before the daemon, by design; verified). Stale comment at plan/ops.rs not touched.
- Concerns: cargo fmt touched unrelated files; reverted before commit. Multi-op live-daemon CLI e2e with --seat not added (not required).
### task 6
- Status: Task 6 (hg-blw.2) report
- `cargo test --lib -- inspect:: cli:: bootstrap::` : 237 passed, 1 failed (cli::tests::inspect_stubs_not_implemented, isolation tripwire on real ~/.config: `sessions` run reads the user config).
- `cargo test --lib -- inspect::stack inspect::harness_files cli::instructions cli::tests::every_spec bootstrap::` : 126 passed, 1 failed (the harness_files test fixed afterwards: depended on /tmp/.git; the no-Git sub-scenario was removed; rerun passed in the 237 run).
- `cargo test --test inspect_stack`: 3 passed (incl. watchdog).
- clippy --all-targets: no warning in touched files.
- Leak check: 0.
- cli::tests::inspect_stubs_not_implemented fails with the tripwire (real HOME config). Not touched by this diff (it runs `sessions`, implemented by hg-blw.3 already in base). NOT verified at base: after that run the hg-build container wedged (`podman exec`: "unable to find user root"; crun kill EPERM
- The toolbox failure also prevented a final post-commit rerun; the commit contains exactly the tested sources plus no changes after the 237-pass run other than none.
- When an AGENTS.md is both shadowed (Codex) and Claude-fallback, note is `shadowed` (single note field).
- Claude AGENTS.md fallback keys on CLAUDE.md/CLAUDE.local.md only, per spec.
- build() treats any render error as the per-seat error row (per brief), not just integrity errors.
- Test `codex_bounded_by_git_root` omits the no-Git case: /tmp/.git exists on this host, making it non-hermetic.
### task 7
- Status: Task 7 (hg-blw.6) report
- CloneLive gains `binding_from_earlier_server: bool` (additive): the text "binding from earlier herdr server" must have a typed JSON field (spec §2.1), and the CLI renders from the deserialized Roster.
- `Roster.unmanaged` is `Option<Option<Unmanaged>>` (absent vs null needed; custom deserializer keeps null across daemon->CLI).
- `--unmanaged` collects bindings from a Scope::Instance inventory, independent of the roster scope, so a seat-scoped call does not list other seats' objects as unmanaged.
- Combined degraded text line: "live data unavailable: herdr: A; threads: B".
- Existing test `cli::tests::inspect_stubs_not_implemented` listed `roster --live` (now implemented) and `sessions` (implemented by hg-blw.3 on the integration branch); reduced to `log`. Later tasks implementing log must drop it.
- `cargo fmt` differences remain in src/observe/tests.rs, reconcile/tests.rs, threads/tests.rs (pre-existing, untouched); clippy warnings shown are in files not changed here.
### task 8
- Status: Task 8 (hg-blw.9) report
- Concerns: cargo fmt reformatted unrelated files; reverted. none_after_root uses a synthetic seat absent from history via fill_last_changes directly, since a real listed seat always has its creation commit in scope.
### task 9
- Status: Task 9 (hg-blw.12) report: BLOCKED
- hg-build toolbox is unusable: `toolbox run -c hg-build` and `podman exec hg-build` fail with
- Uncommitted, uncompiled partial work in the task worktree: HookFit.cut_body_bytes (+ test
- Tests: none run. Test hygiene: none (nothing started).

## interfaces and seams

Added or changed public signatures (from diff, one per line, file):
- src/inspect/mod.rs: pub fn pin_head(store: &dyn Store) -> Result<CommitId, StoreError>
- src/inspect/mod.rs: pub fn parse_since(s: &str) -> Result<Timestamp, String>
- src/inspect/mod.rs: pub fn parse_since_at(s: &str, now: Timestamp) -> Result<Timestamp, String>
- src/inspect/mod.rs: pub mod inventory, log, output, roster, scope, sessions; pub(crate) mod fixture
- src/inspect/output.rs: pub const SCHEMA: u32 = 1; pub trait View: Serialize; pub struct Envelope<B> (new(head, body), with_live(live)); pub fn fail(e: &anyhow::Error) -> ExitCode
- src/inspect/inventory.rs: pub struct Inventory, TeamspaceEntry, SeatEntry, CloneEntry, SessionEntry, RuntimeView, TranscriptRef; pub enum TranscriptSource; Inventory::load(store: &dyn Store, at: &CommitId, scope: &Scope) -> Result<Inventory, StoreError>; Inventory::clones(&self); CloneEntry::current(&self)
- src/inspect/log.rs: pub const OPS_POINTER; pub struct LogQuery (matches(&self, store: &GitStore, c: &Change) -> Result<bool, StoreError>; run(&self, walker: &HistoryWalker<'_>) -> Result<(LogView, Vec<String>), StoreError>); pub struct LogView, LogRow, RequesterView, ConfirmationView, ActionView, ObjectView; pub enum ObjectChange
- src/inspect/roster.rs, sessions.rs, scope.rs: new public view/scope types (see slices)
- src/cli/{log,roster,sessions}.rs: pub enum Commands, pub struct LogArgs, pub fn run(cmd: Commands) -> anyhow::Result<ExitCode>, pub fn execute(...)
- src/daemon/roster_live.rs: pub struct RosterLiveDeps; pub fn register(reg: &mut Registry, deps: RosterLiveDeps)
- src/store/history.rs: new HistoryWalker
- src/herdr/fake.rs: pub fn snapshot_calls(&self) -> usize

New public files: src/cli/log.rs, src/cli/roster.rs, src/cli/sessions.rs, src/daemon/roster_live.rs, src/inspect/{fixture,inventory,log,mod,output,roster,scope,sessions}.rs, src/store/history.rs, tests/inspect_{live,log,roster,sessions}.rs

Seam contract beads in epic tree:
- hg-blw.7: Seam contract: inspect module — Scope, View output envelope, Inventory, CLI group stubs

## deferred minors

Task 1 (hg-blw.7): minor (deferred): [tdd-not-red-first] report admits tests and code were written in one pass with no RED run retained (brief asked for failing tests first)
Task 1 (hg-blw.7): minor (deferred): [dangling-parent-scope] src/inspect/inventory.rs:726 dangling-parent integrity check runs over all seats and clones regardless of scope, so a corrupt unrelated record fails a narrowly scoped view (reported as a deliberate assumption)
Task 1 (hg-blw.7): minor (deferred): [cleanup-inspection] stop-run-processes --check could not complete in the sandbox; only the project pgrep check (0) backs the no-leak claim
Task 1 (hg-blw.7): minor (deferred): [missing-evidence] report supplies no structured evidence records; test run is described in prose only
Task 5 (hg-blw.8): minor (deferred): [exit-code-ordering] src/cli/ops.rs:154-180 --seat/--teamspace resolve against the graph before the daemon call, so with the daemon down an unknown name exits 1 and a missing instance exits 2 rather than 5 (documented in tests/daemon_free_reads.rs)
Task 5 (hg-blw.8): minor (deferred): [missing-edge-case-test] no CLI-level test of a filtered ops call with the daemon down or with a retired seat name via --seat; only list_ops-level coverage
Task 5 (hg-blw.8): minor (deferred): [missing-evidence] report's test run was on tree c2eb13a, not the committed HEAD tree, and no structured evidence records were supplied
Task 3 (hg-blw.3): minor (deferred): [brittle-source-scan-test] src/inspect/sessions.rs transcript_fallback_from_inventory greps its own source text for list_transcripts; fragile but harmless
Task 3 (hg-blw.3): minor (deferred): [swallowed-error] src/inspect/sessions.rs word() maps a serialize error to an empty string; unreachable for unit enums
Task 2 (hg-blw.1): minor (deferred): [text-form-gap] src/inspect/roster.rs clone_line/state_word: text cannot show `retired(reversible)` as in the spec sample (no field carries reversibility); disclosed in report
Task 2 (hg-blw.1): minor (deferred): [noisy-error-rows] src/inspect/roster.rs build: render_instructions runs for retired seats under --all and any StoreError becomes an instructions error row, which may be noisy
Task 2 (hg-blw.1): minor (deferred): [swallowed-error] src/inspect/roster.rs build: effective_manager errors are dropped via .ok().flatten(), so an integrity error in the manager chain shows manager null
Task 4 (hg-blw.5): minor (deferred): [perf-risk] src/inspect/log.rs:~70 LogQuery::matches calls Scope::paths_at (layout::locate, folder lookups) for every scanned commit under non-instance scopes; may be slow near the 5000 cap on large instances (implementer noted it)
Task 4 (hg-blw.5): minor (deferred): [test-hygiene-tooling] report: stop-run-processes --check failed (rc 4, git worktree inventory); leak check done manually, count 0
Task 4 (hg-blw.5): minor (deferred): [missing-evidence] report gives no structured evidence records (command/tree/outcome); prose commands and pass counts only
Task 4 (hg-blw.5): minor (deferred): [preexisting-failure] cli::tests::inspect_stubs_not_implemented fails at base f68b77e per report (run at base shown in throwaway worktree); owned by earlier roster/sessions beads, not this task
Task 7 (hg-blw.6): minor (deferred): [formatting-noise] tests/daemon_composed.rs: unrelated rustfmt re-wrapping of imports and tuples mixed into the change
Task 7 (hg-blw.6): minor (deferred): [unverified-cli-degraded-text] degraded live first-line text is checked at overlay/render level and in-process daemon level, not through the CLI binary (needs a Herdr-less daemon)
Task 7 (hg-blw.6): minor (deferred): [additive-field] CloneLive gains binding_from_earlier_server beyond the brief's shape (justified by spec text needing a typed field)
Task 8 (hg-blw.9): minor (deferred): [weak-assertion] src/inspect/roster.rs reserved_slots_serialize: last_change assertion loosened to is_object()||is_null(), asserts little
Task 8 (hg-blw.9): minor (deferred): [synthetic-test-setup] src/inspect/roster.rs none_after_root drives fill_last_changes directly with a ghost seat rather than the public build path

## amendments

- hg-blw.5: sp:demoted-by-session: promotion reviewer said PROMOTE (size test); session split it into three leaves instead (hg-blw.5 log, hg-blw.8 ops filters, hg-blw.9 roster last_change), spec 2.4 fully specifies each.
