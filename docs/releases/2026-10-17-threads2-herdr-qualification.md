# Threads 2.0.4 and recent Herdr replacement qualification — 2026-10-18

This is an integration handoff, not a new release. Current replacement qualification uses frozen graph `1005d04657a260dc89a55d3b1d37ce6b6f7cdb7e` with exact official herdr-threads **v2.0.4** commit `b83d032a64cb9f665f49806b744f7cc730626819`, wire protocol **9**, on **macOS aarch64**. The replacement checks private daemon/plugin diagnostics after cleanup and retains their raw bytes before removing successful fixture roots. The independent final review found that the earlier Task 4 gate checked harness output only; that original run and bundle remain unchanged historical evidence. The [final scoped rereview](../superpowers/reviews/2026-10-18-threads2-final-rereview-1.md) closes both findings with zero new issues; [implementation/review completion](../superpowers/reviews/2026-10-18-threads2-completion.md) is recorded. The integration decision remains pending.

## Replacement source and latest-release decision

Driver/fixture commit `249067b` adds the bounded owned-log gate and private plugin stdout/stderr capture. Commit `e621b44` then changes only the threads gitlink and the graph lockfile's threads package version, 2.0.3 → 2.0.4. Commit `1005d04657a260dc89a55d3b1d37ce6b6f7cdb7e` adds Herdr internal tracing/rotation inspection, disables supported private background update checks, and waits for archived worktree materialization in the resurrection fixture. No graph production Rust changes occur in this fix wave. The new source clone is detached at this final amendment before runtime qualification, and the submodule is detached at `b83d032a64cb9f665f49806b744f7cc730626819`.

Official metadata refreshed at `2026-10-18T07:00:36Z` reported newly published v2.0.4 (`2026-10-18T06:56:10Z`); Herdr remained v0.9.5. Annotated threads tag `b869ebe878e1b947c1fb7fad054755dca0950f51` peels to `b83d032a64cb9f665f49806b744f7cc730626819`. The four-commit delta from v2.0.3 fixes child lifecycle execution identities and parent recovery, plus tests/publication/version metadata. Exact diffs under public protocol/service/node and daemon path/discovery modules are empty; protocol remains 9. The controller approved the newer stable target before pinning it. Sources: [official threads release](https://github.com/alepar/herdr-threads/releases/tag/v2.0.4), [official compare](https://github.com/alepar/herdr-threads/compare/v2.0.3...v2.0.4). Raw API responses, tag refs and delta evidence are retained. A final official refresh at `2026-10-18T07:34:08Z` still reports threads v2.0.4 and Herdr v0.9.5.

## Replacement validation and diagnostic gate

| Exact command | Source / dependency | Result | Exit | Seconds |
|---|---|---|---:|---:|
| `cargo test --locked --offline --features test-support --lib threads::` | `e621b440` / `b83d032a` | 56 passed, 0 failed | 0 | 162.34 |
| `cargo test --locked --offline --features test-support --lib reconnect -- --test-threads=8` | `e621b440` / `b83d032a` | 18 passed, 0 failed (8 harness threads) | 0 | 9.73 |
| `cargo check --locked --offline --all-targets --all-features` | `1005d046` / `b83d032a` | all targets/features compiled | 0 | 30.756 |
| `cargo test --locked --offline --features private-herdr --test test_isolation -- --test-threads=1 --nocapture` | `1005d046` / `b83d032a` | 13 passed, 0 failed/ignored/filtered | 0 | 54.586 |
| `python3 -m unittest discover -s scripts/tests -v` | `1005d046` / `b83d032a` | 35 passed, 0 failures/errors/skips | 0 | 17.935 |
| `scripts/herdr-release-smoke --versions 0.9.1 0.9.2 0.9.3 0.9.4 0.9.5 --jobs 2 --cache /private/tmp/hg-final-log-qualification-htb_4uwf/replacement/herdr-cache --report /private/tmp/hg-final-log-qualification-htb_4uwf/replacement/matrix` | `1005d046` / `b83d032a` | 185 passed; 15 cleanup + 15 log gates clean | 0 | 460.315 |

All five Herdr releases **0.9.1–0.9.5 passed 37/37 cases each** (30 graph + 5 API + 2 actual threads): **185 total**. All **15 cleanup gates and 15 diagnostic gates passed**, with **190 owned roots removed**, zero emergency reaping/survivors, and **570 raw diagnostic files (1,686,907 bytes)** retained. Actual binary versions and SHA-256 values matched the official pins. There were no skipped/ignored/filtered matrix cases, Rust panic diagnostics in the inspected streams, timeouts or cancellations.

The consumed adapter and reconnect tests, all consumers, private isolation, private plugin and actual-daemon matrix now cover exact v2.0.4. The broad default/no-default/test-support results listed later ran against v2.0.3 and remain explicitly historical; they were not repeated or relabeled for a driver/fixture and dependency patch with unchanged consumed public API. The fixture changes capture plugin startup/action streams, disable private background network updaters, and wait within the existing bound for an archived path to materialize. Isolation and all five runtime rows cover those changes. No graph production feature was changed.

The gate requires captured stdout/stderr `herdr.log` and internal tracing `config/herdr/herdr-server.log` in each root, inspects known main/client/server tracing files and numeric rotations when present, and checks graph `.graph-local/daemon.log` (including the private plugin daemon), private plugin startup/action stdout and stderr, and threads current/rotated daemon and startup-attempt logs. Threads paths include direct state, the default HOME state and `saved-old-node` preserved by recovery. Root ownership and every path component are checked without following symlinks. Required inventory/read/preservation failures and Rust panic diagnostics fail the suite and retain roots. Graph content, Git objects, native transcripts and plugin package files are excluded from diagnostic scanning. Exact inspected bytes, hashes, sizes, optional absences and exclusions survive successful cleanup under each suite's diagnostics archive.

Routing audit: Herdr and graph daemon stdout/stderr go to their file logs; Herdr also sends tracing to the private config directory. Exact pinned Herdr server bootstrap/logging code installs tracing only. Its client panic hook calls the original hook after terminal restoration, preserving standard Rust stderr panic text. Herdr tracing defaults to 5 MiB with zero retained rotations; the gate also inspects numeric rotations when present. Raw retained files document the diagnostic scope. Elected threads runtime redirects both streams to `daemon.log` with one rotated predecessor; pre-election diagnostics/panics go through stderr to startup-attempt logs. Upstream pre-election stdout is `/dev/null`; the inspected pre-election path reports errors on stderr and has no stdout diagnostic producer. Private plugin commands are captured internally by Herdr, so the fixture now preserves both completed stdout/stderr streams through its public log API into `plugin-diagnostics.log`. Process-inspection utility stderr is not a daemon diagnostic source; cleanup failures still fail qualification.

The regression first failed on the original actual `run_suite`: harness success and clean cleanup still yielded PASS for a child-log-only panic. After the fix it yields FAIL, retains the root and archives the diagnostic. Tests additionally cover successful archival/removal, missing and unreadable required logs, file/ancestor symlink refusal, threads startup/rotation/saved-node diagnostics, required suite destinations, private plugin missing/panic output, and exclusion of graph transcript content. All prior inventory/timeout/isolation/ownership/cleanup tests remain active.

The first amended matrix at `e621b44010298ffe879c51f83601ed95563ac88d` remains **FAIL**: Herdr 0.9.5 resurrection observed a retired Git row before the writer materialized its archived worktree path, and the API fixture left an updater `curl` child requiring emergency reaping. The other four rows passed their narrower gate. The failed graph HEAD/index/view state, 35 failed-suite roots, all raw results, and process identity evidence remain retained. The fixture now waits for that path using the unchanged `WAIT`, preserving all identity/state/resurrection assertions, and disables both upstream-supported private update checks. No production writer behavior or cleanup gate changed. A real updater regression first failed with default config and then passed; driver internal-log regressions first false-passed and then correctly failed. The final freeze follows those focused checks. See the [implementation report](../superpowers/reviews/2026-10-17-threads2-final-fix-implementation.md) for exact red/green outcomes and source heads.

## Replacement evidence and preservation

The retained root is `/private/tmp/hg-final-log-qualification-htb_4uwf`, with the final independent frozen clone under `replacement/graph`, the first failed attempt preserved under `graph`, initialized official submodule, independent build cache, copied official Herdr binaries and raw final/failed logs. [The replacement bundle](../validation/2026-10-18-threads2-log-qualification/README.md) contains every case, cleanup and diagnostic inspection result, raw diagnostic bytes, command outcomes, source hashes, tag/API provenance and preservation checks. The child-log-only failure reproduction retains `/private/tmp/hg-97ad75d3b9894e53941c50cb76e91140` and `failure-regression/` under the qualification root. Red/sandbox/setup attempts remain their actual outcomes.

The original Task 4 bundle and clone at `/private/tmp/hg-task4-qualification-534cd4f_` are retained unchanged. Source `bin/herdr-graph` remains SHA-256 `bbc9a7bf8de817cb3dcccfae2e5300de5cb7daac8700b40622ddc64c5971621c`, 13,624,672 bytes, with unchanged modification time. The external threads checkout remains clean at its preflight `c57c80bda01246d897b3ab63aa1ae3272d95ab94`; this wave only reads it. Frozen source hashes and final preservation assertions document these boundaries.

## Historical Task 4 source and official provenance

The external absolute dependency checkout moved from the earlier task's post-tag source to clean `b83d032a64cb9f665f49806b744f7cc730626819` during this work. Its unreleased package version made the graph lockfile incompatible. The original checkout was left untouched. Commit `456dd9cf49e3a2e65e3876f33c81b5136d127866` replaces only the graph-owned symlink with a Git submodule at exact official v2.0.3 and documents setup; Cargo.lock is unchanged. The original absolute target `/Users/alepar/AleCode/herdr-threads` remains in Git history and the preflight evidence.

The packaging README contract still demanded the old symlink command. The first default run correctly remained **FAIL: 693 passed, 1 failed, 1 ignored, exit 101**. Commit `33be69e` changes that one assertion to the submodule initialization command, retaining every other packaging check. Its focused regression passed **1/0, exit 0** before the full rerun. Neither change alters production Rust behavior or the matrix gates.

The first ordinary parallel test-support run also correctly remained **FAIL: 706 passed, 3 failed, 1 ignored, exit 101**. The reconnect crash fixture armed process-global writer failpoints inside the shared library test process, and sibling writer tests hit the injected panic. A serial focused plan group (72 passed) diagnosed the race and was not used as qualification. Commit `d9a5059` confines the unchanged crash/recovery body to an exact-filtered current-test-executable subprocess with scrubbed private state, guarded cleanup and a bounded wait. Its regression arms a sentinel only inside another dedicated child and proves the fixture cannot modify or trigger the parent's registry. Focused red failed before isolation; the parallel eight-thread reconnect group then passed **16/0**. An intermediate helper check incorrectly demanded zero filtered siblings under `--exact`; that failed trial is also retained, and the corrected helper requires exactly the selected successful case and one successful summary. Production failpoint APIs, crash assertions and matrix gates were unchanged.

Final official metadata recheck at `2026-10-18T06:42:19Z` still identifies threads v2.0.3 (published `2026-10-18T01:19:50Z`) and Herdr v0.9.5. Remote annotated tag object `72bcfb5dd9410b2d2d7511297b1b175f22bd628d` peels to exact `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`. The reviewed `--check-upstream` gate returned exit zero with all stable Herdr releases 0.9.1–0.9.5 and all 20 pinned asset digests matching. Sources: [threads release](https://github.com/alepar/herdr-threads/releases/tag/v2.0.3), [official Herdr releases](https://github.com/herdrdev/herdr/releases). Raw metadata, tag refs and retrieval times are in the evidence bundle.

## Historical Task 4 sequential validation (threads 2.0.3)

| Exact command | Source HEAD | Result | Exit | Seconds |
|---|---|---|---:|---:|
| `cargo check --locked --offline --all-targets --all-features` | `d9a5059` | all consumers compiled | 0 | 1.829 |
| `cargo test --locked --offline --no-fail-fast` | `33be69e` | 694 passed, 0 failed, 1 ignored | 0 | 172.441 |
| `cargo test --locked --offline --no-default-features --no-fail-fast` | `33be69e` | 691 passed, 0 failed, 1 ignored | 0 | 205.015 |
| `cargo test --locked --offline --features test-support --no-fail-fast` | `d9a5059` | 710 passed, 0 failed, 1 ignored | 0 | 209.505 |
| `HG_REAL_AGENTS=0 cargo test --locked --offline --features private-herdr --test config_smoke --test test_isolation -- --test-threads=1 --nocapture` | `d9a5059` | 16 passed, 0 failed, 0 ignored | 0 | 28.147 |
| `python3 -m unittest discover -s scripts/tests -v` | `d9a5059` | 24 passed, 0 failed/errors/skips | 0 | 9.563 |
| `scripts/herdr-release-smoke --versions 0.9.1 0.9.2 0.9.3 0.9.4 0.9.5 --jobs 2 --cache /private/tmp/hg-task4-qualification-534cd4f_/herdr-cache --report /private/tmp/hg-task4-qualification-534cd4f_/matrix` | `d9a5059` | 185 passed; all 15 cleanup gates clean | 0 | 496.352 |

Each Cargo build/test/check command used `--locked --offline`; feature builds were sequential. The default/no-default/test-support suites used normal libtest scheduling. Default and no-default evidence is at `33be69e`; the subsequent `d9a5059` delta is restricted to test-only `test-support` fixture isolation, so the controller retained those unaffected passes. All-consumer, final test-support and matrix evidence use `d9a5059`. The ignored test in each normal feature suite is the explicit release-build packaging test; it was not run because it copies a binary into source `bin/`. Private plugin runtime is separately exercised using a copied package and prebuilt graph binary. The matrix supplies the full 37 private graph/API/threads cases per release; no duplicate standalone 37-case sweep was run on this tree.

The remaining private configuration/isolation target has **16 harness passes**, including **12 isolation tests**, the exercised shell configuration and summary printer, plus **two native early returns** (`config_claude_launch_and_resume`, `config_codex_launch_and_resume`, both `HG_REAL_AGENTS=0`). These two passes do not demonstrate launch/resume. Native `real_agent_smoke` bootstrap/summarizer flow was not enabled. The config result JSON and raw skip diagnostics are retained.

## Historical Task 4 matrix (harness-only panic gate)

| Herdr actual version | Graph | API | Threads | Cleanup | Official macOS aarch64 SHA-256 |
|---|---:|---:|---:|---|---|
| `herdr 0.9.1` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `5fc7a7e7adfaca56fa80aa89dcb025693357268dab8285b9ce2d08a2313c89de` |
| `herdr 0.9.2` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `e5dae9c7d93010968897de10c6e9ea2aacb14965f7b9e3b3d061dba82d2a1944` |
| `herdr 0.9.3` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `5173a3e0ae42d5d1ab7ebfa5d5e6329f7c3d23f8e1a3677c7ce3231da2884157` |
| `herdr 0.9.4` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `ea20bfff9cb8c83a996081c926e6ff9370de8e543baa859bca48d19175d895ff` |
| `herdr 0.9.5` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `fb77c2232a05a8035421382c894392f4174e4e38a18e48af3dd0290157c3a7e2` |

Every selected executable's actual `--version` matched the row with exit zero, and its SHA-256 matched the official pinned asset. Every version ran **30 graph private E2E + 5 Herdr API + 2 actual threads = 37 cases**: **185 cases total**, **190 owned fixture roots** removed. All 15 suites passed with exact inventories, no ignored/filtered/skipped cases, no Rust panic diagnostics in retained harness output, no timeouts/cancellations, clean teardown, no emergency reaping and no survivors. Logs retain exact per-case observations, runner PID/PGID, ownership/start identities and cleanup results. Private daemon logs were not inspected or retained before root removal, so this original evidence cannot establish their absence of panics. The replacement qualification above supplies that stronger gate. [The matrix contract](../validation/herdr-release-matrix.md) maps all 37 exact names to behavior.

The driver completed its graph and actual-threads Cargo builds before dispatch; workers reused those binaries and a private plugin package with the original manifest/runtime paths. Runtime Cargo/model commands are guarded. This exercises private plugin startup/actions, not release packaging or a live plugin install. Four-platform pin provenance is not a claim of Linux or Intel macOS runtime qualification.

## Historical Task 4 evidence and preserved artifacts

The retained qualification root is `/private/tmp/hg-task4-qualification-534cd4f_`. Its private graph clone is detached at `d9a5059`, with its initialized submodule detached at exact `ded34fa9`; both are clean. At qualification start all 427 tracked entries were compared, with only concurrent Markdown edits in the shared checkout. Final equality checks cover all executable source/tests/scripts/manifests/lock/gitlink. The initial `456dd9c` clone comparison was byte-equal for all 427 entries. Final source/docs differences and immutable source hashes are retained.

[The durable evidence bundle](../validation/2026-10-17-threads2-qualification/README.md) includes aggregate and per-case results, actual versions/checksums, cleanup identity evidence, sequential commands/counts/exits, build logs, official provenance, source equivalence, artifact checks and preserved failed/setup attempts. The complete private clones/caches remain at the retained root for local inspection. Trial failures remain failures: no later result relabels the failed default run, the interrupted pre-pin check, or the unrestricted offline metadata attempt that encountered an uncached Android package. Historical Task 3 failed/cancelled trials remain at their original paths and in its implementation report; October 5/7 reports remain unchanged.

Source `bin/herdr-graph` remains the original Oct 7 artifact: **13,624,672 bytes**, SHA-256 `bbc9a7bf8de817cb3dcccfae2e5300de5cb7daac8700b40622ddc64c5971621c`, unchanged size and modification time. The private clone has no source `bin/herdr-graph`; matrix plugin startup uses its owned copy. The external threads checkout remains clean and unchanged from preflight. No live instances/services, upstream trees or read-only release caches were modified.

## Build and migration handoff

Initialize the pinned dependency before source builds:

```sh
git submodule update --init --recursive
cargo build --locked --offline --release
```

Registry dependencies must already be cached for offline commands. A fresh clone can use `git clone --recurse-submodules <graph-repository-url>`. GitHub source archives omit submodule contents: a buildable source bundle must include the pinned tree explicitly. Dependency upgrades require an intentional gitlink/lock review and renewed qualification. Rebuild graph for the new threads wire protocol, then restart the graph daemon through the operator's normal lifecycle when ready. No build artifact was installed and no live restart was performed here.

For a mismatched bound graph before reconnect, restoring the exact currently bound threads node/store resumes that namespace. A populated or historically populated unbound graph requires reviewed reconnect. Preserve the graph repository, `.graph-local` and old threads store; deleting a binding or intent journal is not recovery. Obtain the canonical 32-lowercase-hex target node through threads discovery and the current graph `refs/heads/main`, then review/apply:

```sh
git -C <graph-instance> rev-parse refs/heads/main
herdr-graph plan --json threads reconnect --from <old-node-or-legacy> --to <target-node> --head <exact-main-commit>
herdr-graph apply <plan-id> --confirm <plan-hash> --confirmed-by user-relay
```

Use `legacy` only as an unbound source. The old binding and exact main head are fenced; planning does not start/replace a threads daemon. A same-node restart needs no reconnect. The committed `threads-namespace.json` and `mutations/threads-reconnect/<operation>.json` preserve original references, invitations, external participation/opt-outs, prior undo IDs, request attempt history and graph-journal provenance while assigning a new node epoch. Completed requests/native transcripts remain untouched. Owned channels are recreated; external participation requires explicit new plans.

Publication follows the Git commit, sealing the old journal with exact `threads-intents[-<previous-epoch>].retired.json` transition evidence before updating its binding. New work uses `.graph-local/threads-intents-<epoch>`. Original pending intent bytes remain preserved, and a corrupt/unexpected binding or seal fails closed. Late old-epoch effects/results cannot become new-node work. Review all pre-boundary plans again, including unrelated/equivalent changes. Ordinary undo refuses reconnect and pre-boundary actions; return to a prior node only through another reviewed reconnect with current from/head. It does not restore old external memberships or dormant service intents. [README recovery](../../README.md#threads-node-identity-and-recovery) and [state layout](../../GRAPH_STATE.md#threads-node-boundaries) provide the detailed operational contract.

## Coverage limits and integration gates

The runtime fixtures use shells, a private version stub, synthetic public session hooks/transcripts, cooperative CLI invitation acceptance/read/exact-ID ACK, and manually completed summary requests. They validate actual private Herdr/threads and graph service/recovery contracts; they do not validate native launch/resume/SessionStart bootstrap/root-agent cells, real summarizer/model inference or authentication, remote threads continuity, actual restore-induced holds, live user-session handoff or kernel-level containment. Restore refusal and epoch/race cases also have fake/hybrid evidence and are not relabeled as actual remote/restore scenarios.

The branch remains unmerged/unpublished at this handoff. No version bump, push, merge, source archive, release publication, native credential access, native model launch or live restart occurred. Task 4's independent review and whole-branch review/integration remain required controller work; this report makes no integration claim.

## Controller rulings (verbatim)

Ruling: Continue on a feature branch in the coordinated shared checkout — previous session established shared-checkout ownership/artifact preservation and user now authorizes new implementation — recover through commits/worktree if concurrent ownership appears.

Ruling: Keep graph service channels local and qualify Herdr hosts explicitly — threads2 service channels reject remote seats and local pane IDs are not globally unique — distributed graph orchestration requires a later design.

Ruling: Initial Herdr sweep uses official0.9.1 through0.9.5 with upstream drift detection — matches the requested threads sweep and existing support floor — newly released versions need explicit checksum-list refresh and qualification.

Ruling: Resolve graph-managed pane seats through public host-qualified ResolveSeat rather than ambiguous Seats queries — node2 shares multiple Herdr hosts, guarded resolve proves exact host/target and respects restore holds — graph may allocate a bare threads seat before agent enrollment, but never creates bindings, checks in, accepts or ACKs.

Ruling: Reconnect commits an explicit node/epoch marker and preserved migration snapshot before updating the local binding, with per-epoch service journals — Git is the recoverable transition authority and old intents must remain tied to their original node — reconnect is not ordinary undo; reversing it needs another reviewed reconnect, and old external participation must be re-established explicitly.

Ruling: Require fresh plan review after reconnect even for otherwise equivalent old plans — old effect equivalence cannot prove namespace intent — users must re-plan pending pre-boundary changes, including unrelated ones.

Ruling: Pin herdr-threads as a Git submodule at the verified official release instead of following an absolute external symlink — the external checkout moved to unreleased code during qualification — fresh clones must initialize submodules and source archives must include the pinned dependency contents.

Ruling: Adopt verified official threads v2.0.4 at b83d032a64cb9f665f49806b744f7cc730626819 for replacement qualification — it became latest stable during the run and exact public service/protocol/node API comparison is unchanged — source pin/lock and actual-daemon evidence must move together; v2.0.3 feature results remain explicitly historical, and later releases need renewed qualification.
