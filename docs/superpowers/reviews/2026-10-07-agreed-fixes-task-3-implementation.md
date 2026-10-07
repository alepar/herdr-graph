# Task 3 — threads 0.2.9 integration, final docs and validation

Base: `0cf38e8ee6a025f5b5a1801867d294cd4cd0a5d2`, branch `agreed-fixes-2026-10-07`. Status: DONE for implementation/validation/self-review. Commit `53e77ac` (full SHA below). Fresh scoped/broad reviews remain controller gates.

## Changes

- Cargo.lock records herdr-threads 0.2.9 plus its new shlex dependency; no release metadata/version policy imposed on graph.
- Real-daemon fixture builds the external dependency using `build --locked --offline --manifest-path … --target-dir target/threads-bin`, protecting its Cargo.lock from mutation and avoiding network resolution.
- Current GRAPH_STATE, CREATING_NEW_GRAPH, CURRENT-DESIGN, README, graph/seat skill prose describes implemented template/duty/instruction behavior and dual layouts. DESIGN-NOTES gains an October 7 appendix; chronological history and original roast/JSON evidence remain unchanged. Original approved documents are recoverable at 7a87863.
- Bundled six byte-identical Task 1/2 implementation/review/re-review artifacts under docs/superpowers/reviews; removed accidentally tracked task-2-report from scratch and recreated an ignored gate copy.
- Release handoff at `docs/releases/2026-10-07-agreed-fixes-handoff.md` records scope, exact release/source API evidence, migration limits, four verbatim controller rulings, prior review gates and remaining controller/mod ownership.

## Version and API evidence

Before validation: external `git status --short` clean; HEAD and dereferenced `v0.2.9^{commit}` both 223b61a88625d7f442d22d9b8728dc4b2282b15f; annotated tag object b62531f601b0fa845c3728bb6d833b38e1d794f5; Cargo.toml 0.2.9; protocol::wire::PROTOCOL_VERSION 6. Absolute dependency symlink resolves to `/Users/alepar/AleCode/herdr-threads`.

Executed `gh api repos/alepar/herdr-threads/releases/tags/v0.2.9 --jq '{tag_name,target_commitish,published_at,html_url,assets:[.assets[].name],body}'`: exit 0, official release https://github.com/alepar/herdr-threads/releases/tag/v0.2.9 published 2026-10-07T07:44:30Z targeting main; four platform/arch archives + SHA256SUMS. Controller independently verified official remote tag-object/commit APIs match those exact local identities; evidence included in handoff. Archives were not downloaded/hash-checked/installed.

Inspected local `git diff v0.2.6 v0.2.9^{commit}`: unchanged protocol/service.rs and client/service.rs for registration/ACK/membership/invite/notify. The preflight diff also named client/service/types.rs, but that path does not exist in either tree; it is not treated as an inspected API file. Commands/results/capabilities add PickerDirectory; wire result validation adds its case while wire version remains 6. Upstream daemon ownership/paths and client/local.rs are unchanged. Graph adapter/mapping retains active-only Seats query, resolved-continuity filtering, exact requirement IDs/revisions, durable pending intent replay, Unsupported-only fallback and separate NotRequired status. Busy/disconnect errors do not downgrade. No changed graph-consumed API contract required a new adapter or behavioral regression; existing covering tests apply. No Task 3 red/green claim for build flags. Task 1/2 reports retain their actual red/green evidence.

Inspected upstream hook/managed-launch changes and changelog/release notes: registered contracts are declared without diagnostic executable probes; 0.2.9 removes automatically injected --no-daemon and offers explicit HERDR_THREADS_CODEX_OPTS/HERDR_THREADS_CLAUDE_OPTS. Invitation/receipt provenance unchanged. Graph's native harness path remains separate; no native root-agent/foreground runtime claims. Installed Herdr binary is 0.9.1.

## Sequential verification

- `cargo check --offline --all-targets --all-features`: exit 0, 16.71s; `/private/tmp/hg-task3-check.log`.
- `cargo test --offline`: exit 0; **664 passed, 0 failed, 1 intentionally ignored**. Library 617 (138.56s); daemon_composed 14; daemon_ipc 12; integration_sweep 7; packaging 3/1 ignored; test_isolation 8; writer_concurrency 3. Other targets/doc tests zero. `/private/tmp/hg-task3-default.log`.
- `cargo test --offline --no-default-features`: exit 0; **662 passed, 0 failed, 1 intentionally ignored**. Library 615 (125.20s), other nonzero suite counts match default. `/private/tmp/hg-task3-no-default.log`.
- `cargo test --offline --features test-support`: exit 0; **678 passed, 0 failed, 1 intentionally ignored**. Library 618 (121.08s), daemon_composed 20, daemon_ipc 15, integration_sweep 8, packaging 3/1 ignored, test_isolation 8, writer_concurrency 3, writer_crash 3. `/private/tmp/hg-task3-test-support.log`.
- `HG_REAL_AGENTS=0 HG_REAL_THREADS=0 cargo test --offline --features private-herdr -- --test-threads=1 --nocapture`: exit 0; Cargo reports **710 passed, 0 failed, 6 ignored**. Of 710 passing entries, **5 explicitly return early**: config Claude/Codex (2), real-agent flow (1), real-threads opt-ins (2). **705 other passing entries**, with private E2E 30 (167.19s), Herdr client 5, isolation 10 and shell/config assertions included. Six ignored: five exploratory Herdr spikes and one release-build packaging test. Library 617 (156.36s); config_smoke 4, daemon_composed 14, daemon_ipc 12, integration_sweep 8, packaging 4/1 ignored, real_agent_smoke 1 early return, test_isolation 10, threads_real 2 early returns, writer_concurrency 3. `/private/tmp/hg-task3-private.log`.
- `HG_REAL_AGENTS=0 HG_REAL_THREADS=1 cargo test --offline --features private-herdr --test threads_real -- --test-threads=1 --nocapture`: exit 0; **2 passed, 0 failed, 0 ignored**, 22.63s; no early returns. Builds actual herdr-threads 0.2.9 via locked/offline nested cargo (first build 17.04s, cached second 0.04s). `/private/tmp/hg-task3-real-threads.log`.
- `cargo fmt --all -- --check`: exit 0; `/private/tmp/hg-task3-fmt.log`.
- `cargo test --offline --features private-herdr --test test_isolation test_sources_spawn_only_through_isolated_helpers -- --test-threads=1`: exit 0; **1 passed, 0 failed, 9 filtered**; `/private/tmp/hg-task3-isolation-audit.log`.

All Cargo builds/tests are sequential. Test suites use sandbox escalation for temporary Git/sockets, without live service/observer/native-instance changes. No subagents/reviewers dispatched by Task 3. Final real-daemon evidence proves synthetic public-hook registration/manual cooperative CLI acceptance/read/ACK and production discovery, not actual native agents or root-agent cells. Native launch/resume/bootstrap/summarizer execution remain unexercised. Private suite ignores five exploratory Herdr spikes and one release-build packaging test; real agents were explicitly disabled. No compiler warnings in the completed build logs; expected runtime test diagnostics are not compiler failures.

After real tests, repeated external `git status --short` (clean) and `git rev-parse HEAD v0.2.9^{commit} v0.2.9`: exact unchanged release commit/tag identities. No external lockfile/source/native-data mutation occurred. Whitespace/historical/evidence checks and task commit recorded below.

## Migration limits

No eager relocation/native transcript rewrite. Readers support legacy and current layouts; old updates preserve located compensation paths. Registration names/attribution immutable; legacy absent capture names stay unknown. Targeted lookup rejects corrupt/mismatched/duplicate requested-ID candidates without parsing unrelated history; full enumeration errors on history corruption/duplicates. Supported directory depths only; directory discovery scales with count and no large-history benchmark is claimed. Older binary discovery of new layout is unsupported; no downgrade migration. Legacy kind defaults team and duty aliases deserialize, while role_ref is ignored; existing saved plans can require re-planning. Template instruction writes must use template edit. Existing native resume/replacement capabilities remain limited.

Graph Cargo/plugin 0.1.0 metadata remains mod-owned; no existing release/version/packaging policy. Absolute symlink requires dependency setup for source builds. Fresh Task 3 and broad whole-branch review, merge/push/pushed-tree verification, exact-SHA ownership transfer and release publication remain controller/mod work.

## Final self-review and artifact checks

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

Self-review checked task scope, actual public service/mapping/discovery/fallback consumers, the locked/offline nested build, serialized target usage, doc statements against the reviewed implementation, count/skip provenance, mixed-layout/duty/undo limits, four verbatim rulings, exact release ownership and unchanged historical evidence. Six bundled reports/reviews match scratch evidence byte-for-byte; updated local links resolve. `git ls-files .superpowers` is empty. External threads status is clean after the real tests, and HEAD/dereferenced tag/tag object still match the identities above. Fresh Task 3 scoped review and broad final review remain controller-owned gates; self-review does not replace them.

## Task commit and final checkout state

Commit: `53e77ac` — `Verify threads 0.2.9 integration and document agreed fixes`. Full SHA from `git rev-parse HEAD`: `53e77ac3285d6467cbb075cd757ecaf6552e0bbb`. Task-owned scope is 17 files (486 insertions, 31 deletions); no production adapter change.

`git diff --check`, `git diff --cached --check`, and post-commit `git diff HEAD --check` passed. The graph checkout is clean after commit; `git ls-files .superpowers` is empty and both scratch reports are ignored. Historical `git diff --exit-code 7a87863 -- AGREED-FIXES-HANDOFF.md docs/superpowers/reviews/2026-10-05-graph-state-roast-design-1.md docs/superpowers/reviews/2026-10-05-graph-state-roast-design-1-evidence` passed. Cargo.toml/plugin metadata/absolute symlink are unchanged from base. Final local-link, notes-prefix, four-ruling, six-evidence-copy and parsed test-count checks passed. External threads status remains clean and tagged release SHA unchanged after commit.

No unresolved implementation concern found in self-review. Fresh Task 3 scoped review/broad branch review, reviewed-tree merge/push verification and exact-SHA handoff to mod remain outside this task. Mod retains release/version/packaging ownership.
