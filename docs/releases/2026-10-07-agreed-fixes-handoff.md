# Agreed fixes: integration and release-owner handoff — 2026-10-07

This document records the implementation tree and validation for the authorized scope in [AGREED-FIXES-HANDOFF.md](../../AGREED-FIXES-HANDOFF.md). Implementation is on `agreed-fixes-2026-10-07`; Task 3 scoped review is approved. Broad whole-branch review identified F1 (member instruction continuity across rename); its implementation correction is recorded below and scoped re-review remains pending. The controller still owns final review acceptance, merge/push verification and exact pushed-SHA handoff to mod. This document does not claim those gates have finished or authorize a live restart.

## Delivered scope

- Reusable `kind = "seat"` and `kind = "team"` definitions share `TemplateId`; omitted kind remains team. Typed member `seat_template` references, responsibility, independent instantiation and explicit actual-seat reuse replace unused profession-role machinery. Runtime precedence is seat instance > team member > seat template > team template > graph defaults > applicable built-ins, including arguments and summary eligibility.
- `/seat` returns live reusable/team/member/application references, seat context and scoped rules without interpolation or a generated brief. Explicitly reused seats retain canonical runtime configuration. Reviewed changes include exact values, definition revisions and immediate occupied-clone replacement effects, including retained seats after the owning application retires. Undo preserves omission semantics, restores instructions/configuration and rejects conflicting later edits.
- Stable member IDs now carry current specialization bytes across member renames and inverse renames. Omitted `agents_md` preserves those bytes; explicit text/clears win. Undoing a rename preserves unrelated later specialization/root/responsibility edits. All source bytes are captured before path writes, so name swaps cannot cross-contaminate instructions; a new member never inherits a previous path owner's text. Superseded member instruction files are removed by the reviewed edit.
- Operational markers use `SystemDuty` / `system_duty`. Existing serialized `role` fields and CLI `--role` remain aliases; unused `role_ref` is ignored. Summarizer discovery/eligibility remains intact. Template root/actual member instruction writes must use reviewed template edit; omission retains instructions and empty `agents_md` clears them.
- New transcript indexes use `transcripts/<team-name>-<team-id>/<seat-name>-<seat-id>/<transcript-id>.toml`, safe slugs and full stable IDs. Optional `capture_attribution` stores immutable teamspace identity and team/seat names at registration. Index folders remain fixed through rename, observed move, retirement, undo and late completion.
- New records use `mutations/graph-changes/<yyyy-mm>/<op-id>.toml`, `mutations/undoable-actions/<yyyy-mm>/<action-id>.toml` and `mutations/transcript-processing/<request-id>.toml`. Requests remain transcript work; the local SQLite journal retains live/rejected/failed/cancelled operation authority.
- Updated current design/state/factory/README/skill prose and dependency lockfile; preserved original approved artifacts and October 5 roast/JSON evidence unchanged in `7a87863b7e0f84e086955b56433f8217340f003b`. Chronological DESIGN-NOTES decisions remain intact, with an October 7 implementation appendix.

Task 1 commits: `32bb1e97d2e50124ccef33acee8bb450de58de3e` and `2ad05272b832396ae8cb7a8a8d114fd23b66d97b`. Task 2 commits: `71a85baff7d504d7d40ce806e6557edc45c2a791` and `0cf38e8ee6a025f5b5a1801867d294cd4cd0a5d2`. Task 3 integration base is that final Task 2 SHA. The controller will identify the final reviewed/pushed graph SHA in its handoff; embedding a document's own commit SHA here is intentionally avoided.

## Threads version and official release evidence

The read-only dependency checkout is `/Users/alepar/AleCode/herdr-threads`, reached through the repository's absolute `third_party/herdr-threads` symlink. Local clean HEAD and dereferenced `v0.2.9^{commit}` both identify **`223b61a88625d7f442d22d9b8728dc4b2282b15f`**; Cargo package version is **0.2.9** and `protocol::wire::PROTOCOL_VERSION` is **6**. The annotated tag object is `b62531f601b0fa845c3728bb6d833b38e1d794f5`, distinct from its commit. Local clean/tag/SHA verification was repeated after the real-daemon validation and remained unchanged.

Official primary evidence: `gh api repos/alepar/herdr-threads/releases/tags/v0.2.9` returned [release v0.2.9](https://github.com/alepar/herdr-threads/releases/tag/v0.2.9), published `2026-10-07T07:44:30Z`, target `main`, four Linux/macOS aarch64/x86_64 archives and `SHA256SUMS`. The controller also verified `gh api repos/alepar/herdr-threads/git/ref/tags/v0.2.9` → annotated tag object `b62531f…`, and `gh api repos/alepar/herdr-threads/git/tags/b62531f601b0fa845c3728bb6d833b38e1d794f5` → commit `223b61a…`: official remote tag and local checkout match exactly. No archive download, hash verification, installation or upstream modification is claimed.

The release removes automatically injected Codex `--no-daemon`; optional upstream `HERDR_THREADS_CODEX_OPTS`/`HERDR_THREADS_CLAUDE_OPTS` select foreground arguments and preserve frozen handoff arguments. Launch declares registered contracts without using diagnostic flags to infer identity. Invitation acceptance and receipt provenance are unchanged; harness adapters are deferred to 0.3.0. Graph uses its own harness launch path/configuration; this task does not validate real native foreground/root-agent execution.

## Public API inspection versus tests

Local `git diff v0.2.6 v0.2.9^{commit}` inspected the actual dependency source/history:

| Contract | Source inspection | Runtime evidence |
|---|---|---|
| Service registration, ACK send/receipts, memberships, required invitations, Notify | `src/protocol/service.rs` and `src/client/service.rs` unchanged; wire version remains 6 | Existing graph adapter/fake regressions plus isolated real-daemon test below |
| Seat lookup | Existing `SeatsQuery` contract remains; graph explicitly uses `include_retired = false` and resolved continuity | Graph mapping tests and synthetic public-hook seat lookup |
| Discovery | Upstream `src/daemon/ownership.rs` and `src/daemon/paths.rs` unchanged; graph uses namespace/descriptor APIs, current state-directory precedence and ambiguity refusal | Isolated default-HOME production discovery test plus graph discovery regressions |
| Fallback | Graph registers v2; only Unsupported can select Notify fallback. Busy/disconnected errors remain errors/retryable; feature-disabled build selects Notify | Default adapter tests for unsupported-only downgrade and error handling; full no-default suite. No real older daemon was run |
| Additive APIs | PickerDirectory command/result/capability and associated result validation are additive; no graph consumer needs a production change | All graph API consumers compile under all-target/all-feature check |
| Hook/foreground launch | Versionless declaration and wrapper/foreground changes inspected in hook/launch source and official release | Public hook receives synthetic SessionStart payload; no native model process or credentials |

No changed graph-consumed service contract required adapter changes or a new behavioral regression. Existing focused contract tests were rerun through the covering suites. Task 1/2 red/green evidence remains in their implementation reports. Task 3 changes the fixture's nested cargo build to `build --locked --offline --manifest-path … --target-dir target/threads-bin`, preventing an external lockfile update and network resolution; no behavioral red/green claim is made for these build flags.

The installed Herdr binary is **0.9.1**. Real threads tests use separate private Herdr roots/sockets and a daemon built from the verified 0.2.9 checkout, with synthetic Claude SessionStart registration and explicit cooperative CLI required acceptance/read/exact-ID ACK. They test idempotent channel creation, topic, Pending→Accepted invitation membership, notifications, Pending→Acknowledged receipts, repeated requirement release, ServiceBusy refusal and default production discovery. ACK proves receipt/dispatch, never successful processing. These are real daemon/service tests, not real native agent launch/bootstrap/summarizer execution, native root-agent cells, live handoff, or live-session tests.

## Validation

All commands completed sequentially with exit code 0 on macOS aarch64. Graph package/plugin metadata remains 0.1.0; installed Herdr is 0.9.1. Test commands requiring temporary Git/Unix/private sockets used sandbox escalation; no live resources were tested. Full logs remain in `/private/tmp/hg-task3-*.log` and are not published as repository artifacts.

| Command | Result | Log suffix |
|---|---|---|
| `cargo check --offline --all-targets --all-features` | pass; all API consumers compile | `check` |
| `cargo test --offline` | 664 passed, 0 failed, 1 ignored; library 617 | `default` |
| `cargo test --offline --no-default-features` | 662 passed, 0 failed, 1 ignored; library 615 | `no-default` |
| `cargo test --offline --features test-support` | 678 passed, 0 failed, 1 ignored; library 618; crash/concurrency targets included | `test-support` |
| `HG_REAL_AGENTS=0 HG_REAL_THREADS=0 cargo test --offline --features private-herdr -- --test-threads=1 --nocapture` | Cargo reports 710 passed, 0 failed, 6 ignored; 5 passing entries explicitly return early for opt-ins (details below) | `private` |
| `HG_REAL_AGENTS=0 HG_REAL_THREADS=1 cargo test --offline --features private-herdr --test threads_real -- --test-threads=1 --nocapture` | 2 passed, 0 failed, 0 ignored; both opt-ins actually run, 22.63s | `real-threads` |
| `cargo fmt --all -- --check` | pass | `fmt` |
| `cargo test --offline --features private-herdr --test test_isolation test_sources_spawn_only_through_isolated_helpers -- --test-threads=1` | 1 passed, 0 failed, 9 filtered; source isolation audit | `isolation-audit` |
| `git diff --check` / staged whitespace check | pass | self-review |

Default nonzero targets: library 617, daemon_composed 14, daemon_ipc 12, integration_sweep 7, packaging 3, test_isolation 8, writer_concurrency 3. No-default has library 615 and the same other counts. Test-support: library 618, daemon_composed 20, daemon_ipc 15, integration_sweep 8, packaging 3, test_isolation 8, writer_concurrency 3, writer_crash 3. Each ignores only the release-build packaging test.

Private nonzero targets: library 617 (156.36s), config_smoke 4, daemon_composed 14, daemon_ipc 12, e2e_private_herdr 30 (167.19s), herdr_client 5, integration_sweep 8, packaging 4, real_agent_smoke 1, test_isolation 10, threads_real 2, writer_concurrency 3. Its 5 opt-in early returns are `config_claude_launch_and_resume`, `config_codex_launch_and_resume`, `real_agent_seat_bootstrap_and_summarizer_flow`, and both threads_real tests. The remaining 705 entries pass; the two threads entries are subsequently exercised in the separate real-threads command. Its 6 ignored entries are five exploratory Herdr spike tests and the release-build packaging test. Real-agent launch/bootstrap/root-agent/summarizer cells remain unexercised. The private suite does exercise shell launch/configuration, 30 Herdr end-to-end tests, native structural APIs and ten isolation checks.

Self-review checked task scope, actual public service/mapping/discovery/fallback consumers, the locked/offline nested build, serialized target usage, doc statements against the reviewed implementation, count/skip provenance, mixed-layout/duty/undo limits, four verbatim rulings, exact release ownership and unchanged historical evidence. Six bundled reports/reviews match scratch evidence byte-for-byte; updated local links resolve. `git ls-files .superpowers` is empty. External threads status is clean after the real tests, and HEAD/dereferenced tag/tag object still match the identities above. Fresh Task 3 scoped review is approved with zero findings; broad final review remains a controller-owned gate, which scoped review and self-review do not replace.

## Final-review F1 correction validation

The [whole-branch review](../superpowers/reviews/2026-10-07-agreed-fixes-final-review.md) at `b1a35d6ec1d72949dfee01a0bf33073b652c4b09` found one Important issue: renaming a stable member with omitted instructions hid specialization, and undoing that rename could expose stale text after a later instruction edit. The original report is preserved byte-for-byte. The [correction implementation report](../superpowers/reviews/2026-10-07-agreed-fixes-final-fix-implementation.md) records the red/green evidence and self-review; controller-owned scoped re-review and final acceptance remain pending.

Six new regressions were first observed failing on the original implementation, then all passed with the correction. They cover canonical and participating-application `/seat` references; exact committed/working-tree bytes including CRLF, Unicode and trailing whitespace; stable member/seat/clone mappings; immediate undo; later instructions/clears and unrelated fields; explicit clear plus undo; simultaneous name swaps; destination reuse by a retained/new identity; and rejection of ambiguous instruction paths before a write. These use real isolated Git-backed plan/apply/undo flows. No native agent, live daemon or external instance was used.

Final-fix checks all exited 0:

| Command | Result |
|---|---|
| `cargo test --offline --lib member_rename -- --test-threads=1` | 6 passed, 0 failed; 617 filtered (green run) |
| `cargo test --offline --lib` | 623 passed, 0 failed, 0 ignored; 617 previous tests plus 6 new; 127.98s |
| `cargo test --offline --test daemon_composed --test daemon_ipc --test integration_sweep` | 33 passed, 0 failed, 0 ignored: 14 + 12 + 7 |
| `cargo fmt --all -- --check`; working/staged whitespace check excluding preserved review | passed |

The full staged whitespace check flagged only the original Markdown hard break (two trailing spaces on the Base line) in the required byte-identical final-review copy. `git diff --cached --check -- . ':(exclude)docs/superpowers/reviews/2026-10-07-agreed-fixes-final-review.md'` passed; `cmp` confirms that preserved artifact is unchanged. Cargo ran sequentially; tests used escalation for isolated temporary Git/sockets. Logs are `/private/tmp/hg-final-fix-{red,green,lib,integrations}.log`. This correction's results are recorded separately from the earlier Task 3 feature matrix; that full feature matrix was not repeated.

## Compatibility and migration limits

The upgraded binary reads legacy `transcripts/<seat-id>/`, `requests/`, `actions/<yyyy-mm>/` and `operations/<yyyy-mm>/` alongside the new layouts. Existing records update at their discovered location; old compensation paths remain valid. Native transcript bytes/history are untouched and there is no eager historical relocation. New transcript registrations require resolvable team/seat records, including archived objects. Legacy capture attribution remains absent/unknown and is never inferred from current names.

Targeted lookup parses matching ID filenames across supported directories and rejects corrupt targeted payloads, filename/body mismatches and duplicate requested-ID matches. Unrelated corrupt history does not block a new record collision check. Full enumeration still reports history-wide corruption/duplicate IDs. Supported depths are legacy one-level/current two-level transcript grouping and one month-level action/operation grouping; arbitrary nesting is unsupported. Directory discovery/absent filename probes still scale with directory count; no new index or large-history latency benchmark is supplied. Old-binary downgrade discovery is unsupported because older binaries cannot find new-layout records.

Template create/edit documents now reject distinct member names that normalize to the same instruction path, with an error naming the members/path and requesting a rename. This includes inverse documents: a historical edit that would restore ambiguous names requires a noncolliding document. Existing stored templates remain readable, and application hydration is not newly gated on this validation. No stored template/history is eagerly rewritten; the fix operates when a reviewed edit applies.

Missing kind defaults to team; old duty fields and historical undo documents remain readable. Old saved plans can need re-planning because reviewed effect details now include definition revisions/exact values. Direct template instruction content writes must switch to template edit. The Rust structs expose SystemDuty/system_duty; serialized compatibility is maintained, and graph is not a published library package. Existing harness replacement/resume limits remain; no native-history rewrite or new harness capability is claimed.

## Controller rulings retained verbatim

Ruling: Work on a feature branch in the coordinated shared checkout — user and mod established explicit shared-checkout implementation ownership and all local artifacts must be preserved — if isolation proves insufficient, recover via branch commits and create a worktree.

Ruling: Reuse TemplateId with explicit team/seat kind rather than new profession identities — this fits existing plan/apply/copy/undo paths and the approved no-role-registry model — if wrong, schema rework is recoverable through Git and legacy readers.

Ruling: Historical transcript folder names remain those at registration; old record paths remain readable and updated in place — preserves attribution and old compensation paths without an eager destructive migration — folders may show earlier organizational names, documented for users.

Ruling: Guard direct content writes to actual reusable/member template instruction locations and route them through template edit — instruction edits must participate in template dependency revisions and undo — existing direct-write callers must switch to template edit, while stored files remain readable and other opaque writes remain available.

Ruling: Reject ambiguous member instruction path collisions in template create/edit documents rather than silently overwrite — stable identity cannot preserve two texts at one existing path — colliding documents must rename a member before reviewed template edits can apply; existing stored templates remain readable.

## Review evidence and remaining release ownership

Task 1: [implementation](../superpowers/reviews/2026-10-07-agreed-fixes-task-1-implementation.md), [review](../superpowers/reviews/2026-10-07-agreed-fixes-task-1-review.md), [round 1 re-review](../superpowers/reviews/2026-10-07-agreed-fixes-task-1-rereview-1.md). Two Important findings were fixed; scoped re-review found no new Important/Critical finding.

Task 2: [implementation](../superpowers/reviews/2026-10-07-agreed-fixes-task-2-implementation.md), [review](../superpowers/reviews/2026-10-07-agreed-fixes-task-2-review.md), [round 1 re-review](../superpowers/reviews/2026-10-07-agreed-fixes-task-2-rereview-1.md). One Important targeted-lookup finding was fixed; scoped re-review approved. Its implementation report was moved from the accidentally tracked scratch path into this committed evidence directory; an ignored scratch copy remains available for review gates.

Task 3: [implementation](../superpowers/reviews/2026-10-07-agreed-fixes-task-3-implementation.md), [fresh scoped review](../superpowers/reviews/2026-10-07-agreed-fixes-task-3-review.md). Spec compliance and task quality are approved with zero Critical, Important or Minor findings against `0cf38e8ee6a025f5b5a1801867d294cd4cd0a5d2..53e77ac3285d6467cbb075cd757ecaf6552e0bbb`. Both artifacts are copied byte-for-byte. The controller resolved the review's evidence boundaries with fresh successful formatting/whitespace checks, a clean external checkout with matching HEAD/tag, unchanged actual service/discovery source paths, and official GitHub tag-object/commit verification. These checks add no native-agent runtime claim or new ruling.

[October 5 roast](../superpowers/reviews/2026-10-05-graph-state-roast-design-1.md) and accompanying JSON evidence are historical and unchanged. Task 3 self-review/validation is recorded above and fresh scoped review is approved. Broad whole-branch review found F1; its correction is implemented and scoped re-review/final acceptance remain pending. Merge/push, remote-SHA confirmation and exact-SHA ownership handoff remain pending.

No existing graph release/tag/changelog/packaging policy was found. Cargo/plugin metadata remains **0.1.0**, `publish = false`, plugin platform macOS and minimum Herdr **0.9.1**. Mod owns the first release version/tag/changelog/build/archive/publication decisions after receiving and verifying the exact pushed main SHA. Source builds require dependency setup because the tracked absolute symlink is not a portable dependency bundle. Do not publish earlier main or mutate the shared checkout concurrently; ownership transfers only through the controller's explicit handoff. No live service, observer, native instance or shared server was changed or restarted by this integration.
