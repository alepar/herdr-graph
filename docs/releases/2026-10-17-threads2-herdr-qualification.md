# Threads 2.0.3 and recent Herdr qualification — 2026-10-17

This is an integration handoff, not a new release. Final runtime qualification uses graph commit `d9a50590c6ca3dccc558664d4ed608eb2153605d` on `threads2-herdr-matrix`, with exact official herdr-threads v2.0.3 commit `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`, wire protocol **9**, on **macOS aarch64**. The final documentation commit adds no executable-source changes to that tested tree. Feature work starts from `6c9db8f5d81cf1771d0afd3ea8e1d37a9c8027fa`; task reviews cover the adapter, recovery, matrix and scoped fixes. Task 4 and whole-branch review/integration remain controller gates.

## Immutable source and official provenance

The external absolute dependency checkout moved from the earlier task's post-tag source to clean `b83d032a64cb9f665f49806b744f7cc730626819` during this work. Its unreleased package version made the graph lockfile incompatible. The original checkout was left untouched. Commit `456dd9cf49e3a2e65e3876f33c81b5136d127866` replaces only the graph-owned symlink with a Git submodule at exact official v2.0.3 and documents setup; Cargo.lock is unchanged. The original absolute target `/Users/alepar/AleCode/herdr-threads` remains in Git history and the preflight evidence.

The packaging README contract still demanded the old symlink command. The first default run correctly remained **FAIL: 693 passed, 1 failed, 1 ignored, exit 101**. Commit `33be69e` changes that one assertion to the submodule initialization command, retaining every other packaging check. Its focused regression passed **1/0, exit 0** before the full rerun. Neither change alters production Rust behavior or the matrix gates.

The first ordinary parallel test-support run also correctly remained **FAIL: 706 passed, 3 failed, 1 ignored, exit 101**. The reconnect crash fixture armed process-global writer failpoints inside the shared library test process, and sibling writer tests hit the injected panic. A serial focused plan group (72 passed) diagnosed the race and was not used as qualification. Commit `d9a5059` confines the unchanged crash/recovery body to an exact-filtered current-test-executable subprocess with scrubbed private state, guarded cleanup and a bounded wait. Its regression arms a sentinel only inside another dedicated child and proves the fixture cannot modify or trigger the parent's registry. Focused red failed before isolation; the parallel eight-thread reconnect group then passed **16/0**. An intermediate helper check incorrectly demanded zero filtered siblings under `--exact`; that failed trial is also retained, and the corrected helper requires exactly the selected successful case and one successful summary. Production failpoint APIs, crash assertions and matrix gates were unchanged.

Final official metadata recheck at `2026-10-18T06:42:19Z` still identifies threads v2.0.3 (published `2026-10-18T01:19:50Z`) and Herdr v0.9.5. Remote annotated tag object `72bcfb5dd9410b2d2d7511297b1b175f22bd628d` peels to exact `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`. The reviewed `--check-upstream` gate returned exit zero with all stable Herdr releases 0.9.1–0.9.5 and all 20 pinned asset digests matching. Sources: [threads release](https://github.com/alepar/herdr-threads/releases/tag/v2.0.3), [official Herdr releases](https://github.com/herdrdev/herdr/releases). Raw metadata, tag refs and retrieval times are in the evidence bundle.

## Sequential validation

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

## Five-version runtime matrix

| Herdr actual version | Graph | API | Threads | Cleanup | Official macOS aarch64 SHA-256 |
|---|---:|---:|---:|---|---|
| `herdr 0.9.1` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `5fc7a7e7adfaca56fa80aa89dcb025693357268dab8285b9ce2d08a2313c89de` |
| `herdr 0.9.2` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `e5dae9c7d93010968897de10c6e9ea2aacb14965f7b9e3b3d061dba82d2a1944` |
| `herdr 0.9.3` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `5173a3e0ae42d5d1ab7ebfa5d5e6329f7c3d23f8e1a3677c7ce3231da2884157` |
| `herdr 0.9.4` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `ea20bfff9cb8c83a996081c926e6ff9370de8e543baa859bca48d19175d895ff` |
| `herdr 0.9.5` | 30/30 | 5/5 | 2/2 | clean, 0 reaped/survivors | `fb77c2232a05a8035421382c894392f4174e4e38a18e48af3dd0290157c3a7e2` |

Every selected executable's actual `--version` matched the row with exit zero, and its SHA-256 matched the official pinned asset. Every version ran **30 graph private E2E + 5 Herdr API + 2 actual threads = 37 cases**: **185 cases total**, **190 owned fixture roots** removed. All 15 suites passed with exact inventories, no ignored/filtered/skipped cases, no Rust panic diagnostics, no timeouts/cancellations, clean teardown, no emergency reaping and no survivors. Logs retain exact per-case observations, runner PID/PGID, ownership/start identities and cleanup results. [The matrix contract](../validation/herdr-release-matrix.md) maps all 37 exact names to behavior.

The driver completed its graph and actual-threads Cargo builds before dispatch; workers reused those binaries and a private plugin package with the original manifest/runtime paths. Runtime Cargo/model commands are guarded. This exercises private plugin startup/actions, not release packaging or a live plugin install. Four-platform pin provenance is not a claim of Linux or Intel macOS runtime qualification.

## Evidence and preserved artifacts

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
