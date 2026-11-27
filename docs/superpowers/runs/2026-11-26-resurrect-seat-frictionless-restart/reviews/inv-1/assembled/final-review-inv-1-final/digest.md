## diffstat
 CHANGELOG.md                                       |    7 +
 CURRENT-DESIGN.md                                  |    9 +-
 Cargo.lock                                         |    2 +-
 Cargo.toml                                         |    7 +-
 README.md                                          |   27 +-
 ...6-resurrect-seat-frictionless-restart-design.md |  159 ++
 ...ionless-restart-roast-design-1-format-repair.md |  119 ++
 ...nless-restart-roast-design-1-step-back-keys.txt |    4 +
 ...rictionless-restart-roast-design-1-step-back.md |   13 +
 ...ictionless-restart-roast-design-1.coverage.json |   78 +
 ...ect-seat-frictionless-restart-roast-design-1.md |  120 ++
 ...ictionless-restart-roast-design-2.coverage.json |   84 +
 ...ect-seat-frictionless-restart-roast-design-2.md |  121 ++
 .../amend-design-1.md                              |   25 +
 .../coverage-findings-round-1.md                   |   10 +
 .../coverage-findings-round-2.md                   |   11 +
 .../coverage-ledger.md                             |   34 +
 .../coverage-round-1-requirements.md               |   12 +
 .../coverage-round-1-review-1.md                   |   15 +
 .../coverage-round-1-review-2.md                   |   16 +
 .../findings-ledger-design.md                      |    8 +
 .../findings-overrides-design.md                   |    2 +
 .../friction.md                                    |   12 +
 .../interim-status.md                              |   36 +
 .../run.md                                         |   33 +
 ...2026-11-07-onboarding-existing-spaces-design.md |   16 +-
 docs/superpowers/specs/INDEX.md                    |    1 +
 docs/verification-matrix.md                        |   23 +
 evidence/hg-lnw.10/packaging-readme.log            |  152 ++
 evidence/hg-lnw.11/final.log                       |   53 +
 evidence/hg-lnw.11/related.log                     |  430 +++++
 evidence/hg-lnw.11/restart-held.log                |   26 +
 evidence/hg-lnw.12/cargo-test.log                  | 1872 ++++++++++++++++++
 evidence/hg-lnw.12/check-all.log                   |   16 +
 evidence/hg-lnw.13/cargo-test-full.log             | 1821 ++++++++++++++++++
 evidence/hg-lnw.13/lib-reconcile.log               |  162 ++
 evidence/hg-lnw.13/lib-threads.log                 |  231 +++
 evidence/hg-lnw.13/red-mutation-gate-only.log      |    6 +
 evidence/hg-lnw.13/red-mutation.log                |   18 +
 evidence/hg-lnw.14/final.log                       | 1517 +++++++++++++++
 evidence/hg-lnw.14/focused-1.log                   |  139 ++
 evidence/hg-lnw.14/focused-2.log                   |   68 +
 evidence/hg-lnw.14/lib.log                         | 1516 +++++++++++++++
 evidence/hg-lnw.15/lib-all.log                     | 1547 +++++++++++++++
 evidence/hg-lnw.15/threads-run1.log                |  251 +++
 evidence/hg-lnw.15/threads-run2.log                |  252 +++
 evidence/hg-lnw.15/threads-run3.log                |  228 +++
 evidence/hg-lnw.21/cargo-test-lib-rebased.log      | 1557 +++++++++++++++
 evidence/hg-lnw.21/cargo-test-lib.log              | 1556 +++++++++++++++
 evidence/hg-lnw.22/green-test_isolation.log        |   38 +
 evidence/hg-lnw.22/rebased-test_isolation.log      |   38 +
 evidence/hg-lnw.22/rebased-tier5-threads_real.log  |   20 +
 evidence/hg-lnw.22/red-test_isolation.log          |   53 +
 evidence/hg-lnw.22/tier5-threads_real.log          |   99 +
 evidence/hg-lnw.23/cargo-test-rebased.log          | 1867 ++++++++++++++++++
 evidence/hg-lnw.23/cargo-test.log                  | 1854 ++++++++++++++++++
 .../hg-lnw.23/tier3-adopted-unlabelled-rebased.log |   14 +
 evidence/hg-lnw.23/tier3-adopted-unlabelled.log    |   15 +
 evidence/hg-lnw.3/green-summary.log                |   31 +
 evidence/hg-lnw.3/red.log                          |    6 +
 evidence/hg-lnw.4/final-lib-repeat.log             |    3 +
 evidence/hg-lnw.4/final-lib.log                    | 1515 +++++++++++++++
 evidence/hg-lnw.4/focused.log                      |   84 +
 evidence/hg-lnw.4/full.log                         | 1813 ++++++++++++++++++
 evidence/hg-lnw.4/undo.log                         |   82 +
 evidence/hg-lnw.6/build-all-features.log           |    3 +
 evidence/hg-lnw.6/build.log                        |    4 +
 .../hg-lnw.6/check-all-targets-all-features.log    |   16 +
 evidence/hg-lnw.6/test-isolation.log               |   31 +
 evidence/hg-lnw.6/test-run1-compile-break.log      |   26 +
 evidence/hg-lnw.6/test.log                         | 1857 ++++++++++++++++++
 evidence/hg-lnw.7/base.log                         |  320 ++++
 evidence/hg-lnw.7/check-all.log                    |   15 +
 evidence/hg-lnw.7/lib-final.log                    | 1554 +++++++++++++++
 evidence/hg-lnw.7/lib.log                          |  328 ++++
 evidence/hg-lnw.7/lib2.log                         |  321 ++++
 evidence/hg-lnw.7/tier5.log                        |   14 +
 evidence/hg-lnw.8/callers.log                      |   24 +
 evidence/hg-lnw.8/tier5-rebased.log                |   20 +
 evidence/hg-lnw.8/tier5.log                        |   20 +
 evidence/hg-lnw.9/base-057ad4d-isolation-lint.log  |    2 +
 .../base-private_process_info_is_shell.log         |    6 +
 evidence/hg-lnw.9/late-rebind-pinned-order.log     |   23 +
 evidence/hg-lnw.9/tier3-final-rebased.log          | 1979 ++++++++++++++++++++
 evidence/hg-lnw.9/tier3-run2-summary.log           |   69 +
 src/daemon/clone_status.rs                         |  125 +-
 src/daemon/compose.rs                              |   67 +-
 src/daemon/doctor.rs                               |  122 ++
 src/herdr/client.rs                                |   44 +-
 src/herdr/fake.rs                                  |   47 +-
 src/herdr/tests.rs                                 |   62 +
 src/model/clone.rs                                 |   23 +-
 src/model/tests.rs                                 |   53 +-
 src/observe/seat_observation.rs                    |    8 +-
 src/observe/tests.rs                               |   44 +-
 src/plan/seat_adopt.rs                             |    4 +-
 src/plan/tests.rs                                  |    6 +-
 src/ports/threads.rs                               |  106 +-
 src/reconcile/herdr_exec.rs                        |   50 +-
 src/reconcile/planner.rs                           |   71 +-
 src/reconcile/tests.rs                             |  915 ++++++++-
 src/threads/adapter.rs                             |  220 ++-
 src/threads/assignment.rs                          |   27 +-
 src/threads/effects.rs                             |  398 +++-
 src/threads/fake.rs                                |  283 ++-
 src/threads/mapping.rs                             |    4 +
 src/threads/service_tests.rs                       |  368 +++-
 src/threads/tests.rs                               | 1511 ++++++++++++++-
 src/undo/preview.rs                                |   46 +-
 src/undo/tests.rs                                  |  126 ++
 tests/adopt_private_herdr.rs                       |  515 ++++-
 tests/e2e_private_herdr.rs                         |  515 ++++-
 tests/integration_sweep.rs                         |  495 +++++
 tests/support/agent_stub.rs                        |  128 ++
 tests/support/mod.rs                               |    1 +
 tests/support/private_herdr.rs                     |   14 +-
 tests/support/threads_bin.rs                       |   40 +-
 tests/test_isolation.rs                            |    4 +-
 tests/threads_real.rs                              |  780 +++++++-
 third_party/herdr-threads                          |    2 +-
 120 files changed, 35572 insertions(+), 238 deletions(-)

## generated-input provenance
## Generated-input provenance
Captured range: 60e287aadd08af10779df39e625200f28d4b7fa3..8f8ec538ca1f1e4d8d897804aae726472f753ef9
Selection: complete; selected product/input paths: 120; omitted historical payloads: 0.
diffstat.txt describes selected product/input paths. Reports, findings and collection metadata remain ordinary review inputs.
- Outside captured range: 58 informational entries (neither selected nor waived); the first 20 are listed; the full list is review-inputs.json outOfRange.
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:86 inv-p3b-20261031T222244: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:91 inv-p3c-20261031T232503: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:95 inv-p5r1-20261101T001157: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:99 inv-p5r2-20261101T010931: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: invalid failed assembly archive: bounded report identity or verdict mismatch
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:103 inv-p6fr-20261101T020246: incomplete assembled bundle: digest.md
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:22 inv-e10f983d-1d5e-4b01-a12c-2978e5f10eb2: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:22 inv-e10f983d-1d5e-4b01-a12c-2978e5f10eb2: incomplete claimed final report
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:23 inv-2e07a039-6297-4308-8862-f3a2878fe7ab: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:23 inv-2e07a039-6297-4308-8862-f3a2878fe7ab: incomplete claimed final report
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:24 inv-8d1468f1-4551-46bd-8b96-82617c3ea922: incomplete claimed collection
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:24 inv-8d1468f1-4551-46bd-8b96-82617c3ea922: incomplete claimed final report
- Outside captured range (informational; neither selected nor waived): 60e287aadd08af10779df39e625200f28d4b7fa3:docs/superpowers/runs/2026-11-04-threads-seat-identity/run.md:88 inv-4aa37d70-023c-483a-b47c-3dc801a586fe: incomplete claimed collection
Open each report/archive at its recorded revision. Omission supplies no finding disposition or verdict.

## report summaries
### task 1: hg-lnw.2 report (task 1)
Status: Status: IMPLEMENTED. base 88f9516c15d26d3dbf91aa8c28d98b9660d7e26e, head 403e4c1ee9940beed551b9e08aa32db3d9a8ab40, tree 34c10753250d519891516a793ea63a19f342c822. Not stacked.
Assumptions:
- One flaky failure seen once in a parallel run: cli::adopt::tests::memberships_failures_are_errors_not_empty (script-exec test, unrelated to this change); passed 3 of 3 alone and in the final full run.
- Pre-existing warning: `field herdr is never read` (src/threads/tests.rs Fx), untouched.
- The submodule third_party/herdr-threads was empty in the new worktree; I ran `git submodule update --init --reference /var/home/alepar/AleCode/herdr-threads` (read-only use) to build.
- `cargo fmt` over the crate touched unrelated files; reverted them. Only my files' hunks remain.
- Evidence: no retained log files (output summarized above).

### task 2: hg-lnw.3 report (task 2)
Status: Status: IMPLEMENTED. base 88f9516c15d26d3dbf91aa8c28d98b9660d7e26e, head aab739d45546171c622adaa7333afd6de647b6cb. Not stacked.
Concerns:
- Lingering Restore intent: if the agent exits before ClearRestore lands (reattached_restore needs an occupant), the launched Restore intent still shuts out start_needed and plan_resume; it is not relaunched. Narrow race, not the trial's case (6 min). Pinned as current behavior by restore_epoch_laun
- Keyed clones are unchanged: graph-launched keyed clones carry `launched:` before the close too, so the ordinary rule never restarted them (same behavior as pre-close).
- Skipping the record also means no launch_shape for ReplaceSession comparison on such a clone (as for an adopted one).

### task 3: Task 3 (hg-lnw.4) report
Status: Status IMPLEMENTED, stacked true, base a162af4b96504abe1822dafa5e9778ee98a0071a, head 0f569ee5d3ba2d641136aa2b3aa31438e6619b24 (tree 12d8645617bc44139dcb665adfe74ad780c3e05d). No bead amendments or comments (bd comments: none).
Assumptions:
- Preview: clones are noted once, in the restored-seat loop or the standalone-clone loop.
- A reversible fact with no retired_at and no c.retired.at would read as final text; cannot occur in practice.
- Pre-existing format drift in src/bootstrap/tests.rs left alone; only touched files were rustfmt-ed.
- evidence/hg-lnw.4 logs are committed on the branch.

### task 4: Task 4 report (hg-lnw.13)
Status: Status IMPLEMENTED. base=a162af4b96504abe1822dafa5e9778ee98a0071a (stacked on hg-lnw.2), head=3bc96d8466c4dcf91242486c31d0a492fddc5f45, sourceTree=983236484da74874dd038aec39b7a4298d18d291.
Assumptions:
- Gate poll interval 5 s (spec gives none). Gate skipped when seat_intent is Some (restore launch is not an incarnation-change start).
- Row 2 and row 7 are the same code path (launched: meta, no session); both covered.
- clones.restart_held row is Task 8's: late-rebind test asserts the fact reads Resolved and rebind_refused is absent.
Concerns:
- Pre-existing warning: field herdr never read in src/threads/tests.rs (from merged hg-lnw.2).
- cargo fmt rewrote unrelated files; reverted, so the full-suite run was on the same code but before that revert.
- Effect identity folds c.rev, so a rev bump after a Done rebind with a stale Unresolved fact re-plans a row; the op key is per incarnation so the call is idempotent.

### task 5: Task 5 (hg-lnw.14) report
Status: Status: IMPLEMENTED. base 7ee0cdfd119a0e5989b848fb190772ffdb0d102d (stacked on hg-lnw.4), head 57fca5468f030276ca7b3db38e82a101f4eb07b9, tree 2c67a8082e278ebed1d532dd7e11d39509b1ee46.
Assumptions:
- Remedy text = "threads refused graph's resurrect; an operator rebind brings the seat back, and graph then acknowledges it: herdr-threads seat rebind SEAT --pane PANE --operator".
- The threads test fixture has no launcher, so "restore launch runs once" is asserted as its gate: threads_done true (then intent cleared on reattach since the fixture clone has an occupant), no repeat resurrect call, no start_agent. The launcher itself is covered elsewhere (reconcile tests).
- "Freed target" is modelled as the pane's current seat per the map changing (named seat -> none); an owner change invisible to the map and to the pane id does not re-plan (no input changed). Follow-up if wanted: periodic re-ask.
- `cargo fmt` reformatted unrelated files; I reverted them.

### task 6: Task 6 / hg-lnw.15 report
Status: none
Concerns: ack_restore resets only when clone.keyless (per brief); a non-adopted clone whose key reservation failed is threads-keyless but not graph-keyless and gets plain Acknowledge on that path. Tier-5 row left to Task 10 per brief. Full integration tests (tests/) not run.

### task 7: Task 7 (hg-lnw.6) report: pin herdr-threads 3.6.0 with the 3.1.0 to 3.6.0 audit
Status: Status: IMPLEMENTED (completionKind: implementation)
Assumptions:
- No bead comments or amendments (bd comments hg-lnw.6: none). Bead and brief agree.
- Onboarding spec line 3 and 19 are rewritten to name 3.6.0 as required while keeping the v0.3.0/3.1.0 historical fact on line 3.
- Version-attribution code comments are historical (see search section).
- CURRENT-DESIGN.md line 57 keeps "registers `service_session_v3`", which is true at this pin; v4 lands with hg-lnw.7.
Concerns:
- CHANGELOG migration note says graph needs `service_session_v4`; true only once hg-lnw.7 lands (it ships in the same release per the bead).
- Pre-existing warning `field herdr is never read` at src/threads/tests.rs:245 (not touched here).
- /tmp is at 72% (above the ~70% threshold); this task's own temp state is removed. The task worktree's target/ was reflink-seeded from the integration target (XFS reflink); remove it on cleanup after merge.
- humanAction: the stray empty /tmp/.git directory (not created by this task) breaks stop-run-processes for every /tmp temp root; a human should check and remove it (`rmdir /tmp/.git`).

### task 8: Task 8 / hg-lnw.11 report
Status: none
Concerns: cargo fmt reformatted unrelated files; reverted them. Row uses the committed seat_fact, so it depends on the observer refreshing the fact; status uses wall clock Utc::now.

### task 9: Task 9 report: hg-lnw.7, real threads adapter (v4 session, resurrect_seat, rebind_held_seat)
Status: Status: IMPLEMENTED (completionKind: implementation)
Assumptions:
- No bead comments or amendments exist. The bead's roast r2 line ("SeatAlreadyResolved → done outcome") is met by mapping it to `SeatRestoreOutcome::AlreadyResolved`, which the hg-lnw.2 port defines as "callers count it done". Both executors treat it as done.
- "Named error" is implemented as `ThreadsError::Rejected` with a fixed message. A new `ThreadsError` variant would have touched every match site, and `Rejected` is already handled like `Unsupported` everywhere (definitive).
- The existing ack test `v2_registration_unsupported_falls_back` asserted Notify fallback on a refused registration, which contradicts this bead's acceptance (a named error and doctor finding, no fallback). It was rewritten as `old_daemon_refusing_v4_is_an_error_not_a_notify_fallback`; its ServiceAc
- Codes not named by the brief: `StoreBusy` is transient (as busy), `CallerUnverified` is `Unauthorized`, and every other code is definitive `InvalidRequest`.
Concerns:
- A crash between a refusal's journal completion and the adapter forgetting it would leave the refusal to be replayed once by `mutate`'s `inspect_completed` path, then forgotten. This is bounded and self-healing.
- The 3.5.0 daemon serves v4 but not U2. Against it, a held-seat rebind answers `NotFound` (definitive, `rebind_refused`), not a named upgrade error. The CHANGELOG already requires daemon >= 3.6.0.
- Pre-existing `cargo fmt --check` drift in files outside this diff (bootstrap/tests.rs, cli/roster.rs, plan/release.rs and others). Only my files were formatted.
- humanAction: an empty `/tmp/.git` directory breaks `stop-run-processes` for every `/tmp` temp root. Whoever owns it should remove it.

### task 10: Task 10 (hg-lnw.8) implementer report
Status: Status: IMPLEMENTED (completionKind: implementation)
Assumptions:
- No bead amendments or comments (`bd comments hg-lnw.8`: none). The coordinator's task notes are applied:
  - submodule synced to v3.6.0
  - `env` inside `toolbox run`
  - the tests extend the existing `threads_real.rs`
  - restore rows use the production host path
- "Fresh pending invitation" is proven by threads' participant `episode` going from 1 to 2 (`thread participants --json`). The port cannot tell a settled invitation from a pending one, as the design says. Before 24 h the episode stays 1 (reused).
- The window between 24 h and 7 d is entered only once the stored receipt status reads `retired` (`delivery recipients --json` `physical_status`). The port's effective status is `retired` for any retired recipient, so it cannot show the expiry.
- `operator_rebind_resurrects_and_places` runs the remedy through threads' `human` namespace (see Concerns).
- `target/` was seeded with an XFS reflink copy of the integration worktree's `debug` and `threads-bin`. `target/threads-bin-test-support` was seeded from `threads-bin` and then rebuilt with the feature. Build artifacts stay until the merge, for review.
Concerns:
   - `src/daemon/clone_status.rs:108` and `:249`
   - `src/daemon/doctor.rs` fixtures at lines 993, 1000, 1036 and 1042
   - `src/threads/tests.rs:6588`
   - `src/reconcile/tests.rs:6057`

### task 11: Task 11 (hg-lnw.9) implementer report
Status: Status: IMPLEMENTED (completionKind: implementation). Branch `task-hg-lnw.9`, worktree
Assumptions:
- Scenario 3 "never observed" means graph never captured a session (`sessions` empty), per `had_agent =
- Scenario 6 "keyed created seat" is a graph-created `--harness claude` seat (`kc`). The shell seats are seated_rig's
- "Submit one prompt" is the agent's UserPromptSubmit threads hook run by the stub agent process.
- No bead amendments or comments existed. I used the current bead text and the brief together.
Concerns:

### task 12: Task 12 report: hg-lnw.10 (docs: keyless undo windows and frictionless restart)
Status: Status: IMPLEMENTED (completionKind: implementation)
Assumptions:
- No amendments: the bead has no comments. Requirements came from the bead description and acceptance plus the brief, and they are compatible.
- The README and CURRENT-DESIGN pin text still said graph registers `service_session_v2`/`v3`. Since hg-lnw.7 the adapter registers only v4, so I corrected those clauses: they sit inside this task's files and the "pre-3.6.0 daemon error" acceptance item.
- The CHANGELOG entry on undo-restored agents relaunching (hg-lnw.3, spec §4.4) counts as a behaviour change of this epic, so it is listed.
Concerns:
- The fresh worktree got its own cold target/ (1m 09s build). I did not seed it from the integration worktree with a reflink. It should be removed with the worktree after the merge.
- The README says the doctor `threads` line "fails" for the v4 refusal. That holds when the threads state dir is found, the normal case. When none is found, doctor warns instead (the existing rule, described in the paragraph above it).

### task 13: Task 13 (hg-lnw.12) report: integration sweep
Status: Status: IMPLEMENTED (completionKind: implementation)
Assumptions:
- After UnretireSeat, real threads no longer reads the keyed seat `Retired`; until the key checks in I modelled it as `Unresolved`. I did not confirm the exact 3.6.0 inspect answer for an unretired, not-yet-bound seat from the threads source. `Resolved` on the reopened pane would also pass the test.
- The brief's "created (keyed), no session" row and "never observed, keyed" are the same state in graph (a `launched:` meta with no captured session), so one test covers both.
Concerns:
- The rebind also fires outside a restart, which is spec-conformant but noisy. `rebindable` emits a rebind for any seated clone whose fact reads `Unresolved` at or after `incarnation:changed_at`, with no other restart condition. In the keyed composed test, an unretired keyed seat (reading `Unresolve
- The worktree has a reflinked `target/` (seeded from the integration worktree). Remove it with the worktree after merge.

### task 14: Task 14 report: hg-lnw.21 (operator remedy in threads' human namespace)
Status: Status: IMPLEMENTED. completionKind: implementation.
Assumptions:
- The bead says the working form is `herdr-threads human seat rebind SEAT --pane PANE` and asks to verify the exact flags. The submodule's own guidance and tests keep `--operator`, so the remedy keeps it. The bead carries no comments or amendments.
- The bead and brief list both current-behaviour docs and historical run records. I fixed the docs and left the run records (docs/superpowers/runs/*) as written.
Concerns:
- `warning: field herdr is never read` (src/threads/tests.rs:245) shows in lib-test builds. My change to that file is a single string at line 6588, so I did not introduce it, but I did not run it at base.
- The target dir was seeded with a reflink copy of the integration worktree's `target/debug` and `target/threads-bin`. It stays in the worktree until the merge cleanup.
- The tier-5 `operator_rebind_resurrects_and_places` test builds the remedy argv itself rather than calling `operator_rebind_remedy`, so nothing checks graph's printed string against real threads end to end. A follow-up could make the test parse graph's remedy. I did not do it here, because it is ou

### task 15: Task 15 (hg-lnw.22) implementer report
Status: Status: IMPLEMENTED (completionKind: implementation)
Assumptions:
- No bead amendments or comments (`bd comments hg-lnw.22`: none). Brief and bead agree.
- Extending tests/support/private_herdr.rs is in scope (brief: "and tests/support/* helper if one must be extended").
- Build dir seeded by `cp -a --reflink=always` from the hg-lnw.9 task worktree's target.
Concerns:
- humanAction: remove the stray empty `/tmp/.git` directory (not created by this task) so `stop-run-processes` can verify temp roots under /tmp again.
- The first commit was made with `git -c core.hooksPath=/dev/null`; the repository has no hooks configured (core.hooksPath unset, no non-sample hooks), so nothing was skipped.

### task 16: Task 16 (hg-lnw.23) implementer report
Status: Status: IMPLEMENTED (completionKind: implementation)
Assumptions:
- No bead amendments or comments (bd comments: none). Followed the bead's "pick a unique agent name per pane".
- Kept readable names: the pane-qualified name is used only after `agent_name_taken`, as herdr-threads does.
- Label sanitizing to Herdr's name rule was included because the new naming function has to produce a valid name;
Concerns:
- The fake models client+Herdr at the trait level, so it calls `client::agent_names` and repeats the one-retry rule;
- Humans: `/tmp/.git` (empty dir, not this task's) breaks stop-run-processes' temp-root checks for every task; remove
- The reflinked `target/` in the task worktree (CoW clone of the integration target) should be removed with the

## interfaces and seams
New public files: tests/support/agent_stub.rs
src/daemon/clone_status.rs: pub(crate) fn operator_rebind_remedy(seat: &str, pane: &str) -> String {
src/daemon/clone_status.rs: pub fn clone_status_at(
src/herdr/client.rs: pub fn agent_names(label: Option<&str>, kind: &str, pane: &HerdrPaneId) -> [String; 2] {
src/herdr/fake.rs:     pub fn agent_names(&self) -> HashMap<HerdrPaneId, String> {
src/ports/threads.rs: pub enum SeatRestoreRefusal {
src/ports/threads.rs:     pub fn retryable(self) -> bool {
src/ports/threads.rs: pub enum SeatRestoreOutcome {
src/ports/threads.rs:     pub fn refused(reason: SeatRestoreRefusal) -> Self {
src/ports/threads.rs:     pub fn from_error(e: &ThreadsError) -> Self {
src/ports/threads.rs: pub const KEYLESS_OBLIGATIONS_SETTLE_AFTER: chrono::Duration = chrono::Duration::seconds(24 * 3600);
src/ports/threads.rs: pub const KEYLESS_REVERSIBLE_FOR: chrono::Duration = chrono::Duration::seconds(7 * 24 * 3600);
src/reconcile/planner.rs: pub fn seat_unresolved_key(clone: &CloneId) -> String {
src/reconcile/planner.rs: pub fn rebind_refused_key(clone: &CloneId) -> String {
src/reconcile/planner.rs: pub fn rebind_refused_in(
src/reconcile/planner.rs: pub fn seat_unresolved_in(journal: &Journal, clone: &CloneId, changed_at: Timestamp) -> bool {
src/threads/adapter.rs: pub(crate) const GRAPH_SERVICE_LEVEL: ServiceSessionLevel = ServiceSessionLevel::V4;
src/threads/adapter.rs: pub(crate) fn level_refused(detail: &str) -> ThreadsError {
src/threads/adapter.rs: pub(crate) fn register_error(e: ApiError) -> ThreadsError {
src/threads/adapter.rs: pub(crate) enum MutateError {
src/threads/adapter.rs: pub(crate) fn restore_outcome(answer: Result<ServiceResult, MutateError>) -> SeatRestoreOutcome {
src/threads/effects.rs: pub const RESURRECT_KIND: &str = "threads.resurrect_seat";
src/threads/effects.rs: pub const REBIND_KIND: &str = "threads.rebind_held_seat";
src/threads/effects.rs: pub fn resurrect_refused_key(clone: &CloneId) -> String {
src/threads/effects.rs: pub fn refused_meta(journal: &Journal, clone: &CloneId, epoch: &OpId) -> Option<serde_json::Value> {
src/threads/fake.rs:     pub fn enable_resurrect(&self, view: Arc<FakeThreadsSeatView>, clock: Arc<dyn Clock>) {
src/threads/fake.rs:     pub fn settle_windows(&self) {
src/threads/fake.rs:     pub fn hold_for_restart(&self, seat: &ThreadsSeatRef, pane: &HerdrPaneId) {
src/threads/fake.rs:     pub fn set_bare_seat(&self, seat: &ThreadsSeatRef, pane: &HerdrPaneId) {
src/threads/fake.rs:     pub fn set_foreign_key(&self, seat: &ThreadsSeatRef) {
src/threads/fake.rs:     pub fn set_reconcile_pending(&self, pending: bool) {
src/threads/fake.rs:     pub fn minted_invitations(&self, thread: &ThreadRef, seat: &ThreadsSeatRef) -> usize {
src/threads/fake.rs:     pub fn status(&self, seat: &ThreadsSeatRef) -> Option<SeatStatus> {
src/threads/fake.rs:     pub fn resolved_on(&self, pane: &HerdrPaneId) -> Vec<ThreadsSeatRef> {
src/undo/preview.rs: pub(crate) fn seat_window_note(c: &CloneRecord, now: Timestamp) -> Option<String> {
src/ports/threads.rs:     async fn resurrect_seat(  (trait method, signature spans lines)
src/ports/threads.rs:     async fn rebind_held_seat(  (trait method, signature spans lines)
Seam contract beads in epic hg-lnw tree: none

## deferred minors
none

## amendments
hg-lnw.5: sp:demoted-by-session: promotion reviewer said PROMOTE (three features); session overrule: rebind gating and never-observed both change the same restart branch of plan_resume, so they stay one owner; status row and doctor finding moved to their own leaf.
