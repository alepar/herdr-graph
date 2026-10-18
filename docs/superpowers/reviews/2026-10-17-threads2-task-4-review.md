### Spec Compliance

- ✅ Spec compliant for Task 4. Reviewed `c1b3af1963bc04734f11686cf8c27d1f27edc89a..c7117df2652d625b5943100e5fe36dbc684706f1`. The immutable official dependency, final qualification, current documentation, preserved historical evidence, and narrowly scoped test repairs meet the task brief. `.gitmodules:1` and `third_party/herdr-threads:1` replace the moving symlink with the exact official `ded34fa971c18cef81ea1b597097a8ff7cd91eb3` gitlink; `README.md:121` explains initialization and source archives.
- ✅ Every requested documentation destination has its corresponding change: `README.md:80`, `CURRENT-DESIGN.md:90`, `GRAPH_STATE.md:60`, `docs/verification-matrix.md:6`, `docs/validation/herdr-release-matrix.md:196`, and `docs/releases/2026-10-17-threads2-herdr-qualification.md:1`. Current claims name exact graph/dependency revisions, protocol, platform, commands, per-version checksums, skips, migration/recovery instructions and rebuild/operator-restart requirements. Historical claims remain dated.
- ✅ Exact-tag all-consumer and sequential feature/private checks are supported by retained commands and raw logs (`docs/validation/2026-10-17-threads2-qualification/command-results.json:3`, `:35`, `:50`, `:66`). Default/no-default runs at `33be69e` are explicitly distinguished from final `d9a5059` runs. The intervening Rust delta is gated to test-support library tests (`src/lib.rs:4`, `src/plan/tests.rs:31`), so retaining the unaffected passes is justified by the actual diff.
- ✅ All five requested releases have exact inventories, zero exit status, no timeout/cancellation and clean cleanup records (`docs/validation/2026-10-17-threads2-qualification/matrix.json:7`, `:1041`, `:2060`, `:3098`, `:4124`). Independently reading the 15 retained output logs confirmed 30 graph / 5 API / 2 actual-threads successful cases per release, with no ignored/filtered cases or panic/warning/error/skip diagnostics. Totals are 185 cases and 190 registered roots, matching `verified-evidence.json:616`.
- ⚠️ Cross-task boundary: the underlying adapter/reconnect and matrix-driver implementations are unchanged in this task; their complete correctness is not re-reviewed here. The copied prior reviews and scoped resolutions provide the preceding gates; whole-branch review and integration remain pending. Native agents, real restore holds, remote continuity, Linux/Intel runtime and live-session handoff remain expressly unqualified (`docs/releases/2026-10-17-threads2-herdr-qualification.md:73`). No new live-service inspection was performed to independently prove absence of all possible external side effects.

### Strengths

- `src/plan/tests.rs:38` confines global crash failpoints to an exact-selected private child, retains the original recovery test body, checks successful exit and exactly one selected passing case/summary, and imposes a 30-second deadline. `src/plan/tests.rs:2660` adds a behavior-level sentinel regression that runs in its own child and verifies both absence of interference and preservation of the parent's armed failpoint. This repairs the observed parallel race without changing production failure semantics or weakening matrix gates.
- `tests/packaging.rs:150` changes only the obsolete dependency-setup assertion. The prior failure is retained, and the replacement follows the actual pinned dependency setup documented in `README.md:123`.
- `docs/validation/2026-10-17-threads2-qualification/README.md:5` makes aggregate, per-case, source, official provenance and raw evidence navigable. All 18 bundle checksums match; all 127 archive members match their manifest hashes/lengths and retained original bytes. The five copied historical review reports are byte-identical to their source reports.
- `docs/releases/2026-10-17-threads2-herdr-qualification.md:25` distinguishes harness passes from the two disabled native configuration paths and the intentionally ignored release-build packaging test. Failed packaging/test-support/setup attempts remain failures in `docs/validation/2026-10-17-threads2-qualification/trial-notes.md:3`; the final matrix is not presented as native-model or publication evidence.

### Issues

#### Critical (Must Fix)

- None.

#### Important (Should Fix)

- None.

#### Minor (Nice to Have)

- None identified within Task 4 scope.

### Checks and Evidence

- Read the brief, binding Global Constraints, implementation report, and supplied diff in bounded portions; large repetitive JSON records were parsed to inspect their inventories, identities, counts, cleanup outcomes and hashes. Some initial tool output was truncated, so affected content was recovered in smaller portions or structured summaries. No git commands, suite reruns, subagents, Herdr/threads actions or live service changes were performed. Only this requested review report was written.
- Named outside-diff risk: the new subprocess wrapper could inherit live routing or leave its child behind. Checked existing `tests/support/isolated.rs:92`, `:99`, `:144`, `:193`, and `:208`: commands clear their environment and use private paths, children own a process group, the wait is bounded, and drop performs owned cleanup. The change reuses that existing contract without adding a new launch path.
- Named provenance risk: retained qualification might describe different executable inputs or silently overwritten local artifacts. Compared the recorded code/test/script/manifest hashes with current source bytes; included `herdr-plugin.toml`, and checked the declared exact gitlink against the diff. All 175 recorded code/build/test/gitlink entries agree. Independently checked original `bin/herdr-graph` size, mtime and SHA-256 against `artifact-before.json:2`; all remain unchanged. The archived `verify-evidence.py:42`–`:56` records private-clone/source comparison and the external checkout's unchanged-clean assertion. No current external checkout traversal or git inspection was needed.
- Official provenance consistency: `final-official-provenance.json:2` records retrieval time, latest-release endpoints and peeled tag; all 20 tested platform pins match the retained official asset metadata. `logs/check-upstream.log:1` lists exactly 0.9.1–0.9.5. This verifies the supplied dated provenance, not a new online release discovery.
- Behavioral evidence inspected: retained `logs/crash-isolation-red.log:12` and `:26` show the sentinel failing when the original fixture interferes with its parent; `logs/crash-isolation-green.log:23` records 16 focused parallel passes. Final `logs/test-support.log:655` and the subsequent integration summaries total 710/0/1 under ordinary scheduling. The original failed parallel run remains separately retained.
- Covering evidence inspected: `logs/default.log:652` and subsequent summaries total 694/0/1; `logs/no-default.log:649` and subsequent summaries total 691/0/1; `logs/config-isolation.log:21` and `:41` total 16 harness passes, with native exclusions independently recorded in `config-smoke.json:2`; `logs/driver-tests.log:27`–`:29` report 24 passing tests; `logs/all-consumers.log:2` records successful compilation. Completed inspected logs contain no unexpected warning/error diagnostics. Expected disabled-native statuses and release-packaging ignore are disclosed exclusions, not matrix passes. Retained final formatting logs are empty, consistent with the reported successful formatting checks; whitespace success is recorded in the implementation handoff.

### Assessment

**Task quality:** Approved — 0 Critical, 0 Important, 0 Minor.

**Reasoning:** The qualification is tied to an immutable official dependency and identifiable executable source, and its durable evidence supports the stated five-version runtime result. The two discovered test defects are repaired narrowly with meaningful regression evidence; documentation preserves historical results and states the remaining operational and native-runtime limits accurately.
