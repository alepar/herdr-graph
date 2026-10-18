# Task 1 implementation report

Task 1 implements threads 2.0.3 compatibility and a preserving namespace fence. The controller explicitly moved operator reconnection/migration to a new sequential Task 2 (formerly Task 1b), before the Herdr matrix. Consequently, a populated legacy graph or a changed node is deliberately blocked until original state is restored or Task 2 supplies reviewed recovery. No reconnect command is claimed to exist.

## Implementation

- Public discovery uses `NodePaths::resolve_read_only`, `read_node_uuid_hint`, and `read_descriptor`; client constructors accept the upstream canonical 32-lowercase-hex `NodeUuid`. `Discovered.instance` keeps its public field name but now contains `NodeUuid`.
- The explicit `HERDR_GRAPH_THREADS_SOCKET`/`HERDR_GRAPH_THREADS_INSTANCE` pair is parsed strictly. Missing half, empty/relative socket, malformed/non-UTF-8 node, or legacy hyphenated UUID fails visibly rather than selecting default discovery.
- `ThreadsSeatMap::new(socket, node, clock, host: HostLocator)` resolves managed local panes through public `ResolveSeat`, carrying an explicit canonical host and fresh operation ID on every call. The 2.0.3 claimless `Seats` query cannot safely identify a host: its SQL filters node + pane across hosts and its result has no locator. The controller approved allocation semantics: a resolution may create a bare seat but cannot enroll an occupant, check in, accept, or ACK. Exact host/pane structural ownership makes fresh retries safe after unknown outcomes; a fixed operation key would replay stale seat IDs after rebind (upstream resolution replay digest also omits host). Restore refusal is returned, with no ambiguous query fallback.
- Service v2 registration remains required. Capability probing additionally checks `messages.send_time_receipts_v1`; only a definitive unsupported answer or absent capability selects Notify fallback. Busy and probe disconnects remain errors. Probing refreshes after daemon restarts instead of retaining an obsolete capability cache. Upstream `PersistentServiceClient::submit(Send)` performs its own mandatory capability round trip; fake peers now implement that public request as well.
- The service intent journal is bound before opening or replaying it. Both production lazy service and pane-map paths rediscover/revalidate on every call before reusing caches.
- Real fixture uses public Claude markers `CLAUDE_PID=<this integration-test process>` and `CLAUDE_CODE_SESSION_ID=graph-threads-test`, matching the synthetic public SessionStart payload. It does not inherit the live caller's identity or invoke a native model. `PrivateHerdr::start_without_credentials` removes both supported API key variables before starting fixture children. Test acceptance/ACK still exercise the actual threads CLI and daemon.
- `Cargo.lock` regenerated offline against the existing read-only external symlink. README records v2.0.3 release commit `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`, actual qualification checkout `b4d5f9628d17f869ab5b139b7f4ae3c933d02841`, protocol 9, new identity/host semantics, and precise recovery boundary. External checkout remains clean.

## Namespace interface for Task 2

`threads::namespace::binding_path(intents)` is the sibling path `intents.with_extension("node.json")`, normally `.graph-local/threads-intents.node.json`. Version 1 content is `{ "version": 1, "node": "<canonical node id>" }`. `ensure(intents, node, graph_root: Option<&Path>)` validates an existing exact binding or creates one only after empty-journal checks and, when a graph root is provided, reading authoritative `refs/heads/main`. Any committed TOML record beyond root `graph.toml` requires explicit assessment. Thus a freshly initialized graph with graph.toml and placeholder files binds automatically, before service mutations create other records. Nonempty unbound journals, corrupt bindings, unavailable graph repositories, and populated unbound graphs are preserved and refused. Production supplies the graph root before *both* service and map operations; direct low-level ServiceThreads constructors protect their own journals but do not inspect an unrelated graph root.

Binding publication uses a private 0600 temporary file, file fsync, no-clobber hard link, and parent-directory fsync. Two competing node identities cannot both win initial publication. This is not an atomic transaction with the graph writer or Git: a concurrently admitted first graph commit is outside the binding lock, and Task 2 must coordinate recovery and stale queued effects through the writer/namespace epoch rather than treating the binding file alone as a migration transaction. Existing active adapter calls also retain their old exact expected node; replacing the store cannot make their old envelopes valid on a new node.

Task 2 must evolve this interface intentionally: fence expected old/new node plus graph head; preserve old refs, invitations, external participation/opt-outs and request attempt history; salt new channel/effect/delivery identities; preserve old service journals without replay across nodes; and publish/recover the migration with writer coordination. A historical external participant ID must remain unresolved, not be silently invited on a new node. Normal undo must not implicitly cross the external node boundary.

## Red/green evidence

1. Reproduced original five compile failures in actual checkout (`/private/tmp/hg-task1-baseline.log`): removed InstancePaths/read_existing_namespace/instance_uuid and the two client NodeUuid type mismatches. Mechanical public-type and fixture updates were made only after reproducing this baseline.
2. Added regressions before behavior fixes: old-node unresolved journal reopen under a fresh node and claimless pane lookup both failed. Existing ACK round-trip also failed because the old fake omitted the mandatory send-time capability probe. `/private/tmp/hg-task1-regressions-red.log`: 7 pass, 3 fail. `/private/tmp/hg-task1-regressions-green.log`: all 10 pass after fencing, qualified resolution, and fixture protocol support.
3. Added send-time capability absence/disconnect regression before changing production capability selection. `/private/tmp/hg-task1-capability-red.log` observed wrong ServiceAck instead of NotifyFallback. It passes in the library verification after explicit probing and error classification.
4. Additional namespace tests cover canonical overrides, real temporary Git history/reference preservation, corrupt bindings, racing nodes, and re-discovery after both a service and a pane-map cache exist. Descriptor fixture initially failed upstream ownership validation (0644 files, then socket mode); corrected to canonical paths and 0600 files/socket without weakening production checks. `/private/tmp/hg-task1-namespace-green.log`: 4 pass.
5. Real fixture first failed because the old harness identity conventions no longer enroll sessions. Subsequent run reached all service operations and caught an invalid restart fixture assertion (same operation key with changed topic); corrected to original exact payload. Final real target passes fully, with no skipped cases.

## Verification

Final command results are appended below after sequential execution. Logs are under `/private/tmp/hg-task1-*`; no Cargo commands were intentionally run concurrently.

- `HG_REAL_THREADS=1 cargo test --offline --features private-herdr --test threads_real -- --test-threads=1 --nocapture`: **2 passed, 0 failed, 0 skipped**, `/private/tmp/hg-task1-real-final.log`. Covers default production discovery, bare-seat idempotence, two actual private Herdr hosts with identical pane IDs and distinct local seats, synthetic native-hook enrollment, ensure/topic, required invite and exact revision acceptance, membership, Notify, exact-ID pending→ACK receipt, release, busy registration, same-node restart retaining ACK, and fresh-store node replacement refusal with old DB retained under a separate private path.
- `cargo test --offline --lib namespace -- --test-threads=1 --nocapture`: **4 passed**, `/private/tmp/hg-task1-namespace-green.log`.

## Self-review and limitations

- Graph historical refs, request attempts and pending service bytes remain untouched by mismatch failures. No old-node receipt can reach the current-node client through the production endpoint.
- Node identity and socket are validated anew even when capability/service/map objects already exist. Socket changes within the same node can rebuild the client. Explicit overrides intentionally trust the supplied exact node and rely on wire identity correlation; they do not silently discover another node.
- Restore-hold propagation is exercised with a fake peer; actual restore-induced hold creation is not claimed as a real scenario in this task. Upstream guarded resolution is used unchanged. Remote-seat exclusion follows that public operation's local-host allocation contract, not a forged local reading of a remote continuity row.
- No real native-model enrollment or summarizer-agent execution is claimed. The public hook, process fingerprint, session claims, CLI acceptance and ACK are exercised with the fixture's synthetic identity. Matrix version provenance and build-once mechanics belong to the subsequent matrix task.
- Low-level ServiceThreads and ThreadsSeatMap are explicitly node-bound. Production lazy wrappers own discovery and graph-reference fencing; callers bypassing those wrappers must own their own graph namespace checks.
- The currently checked-out dependency is an external path symlink, not an immutable Cargo git dependency. README gives the exact qualification source revision; do not silently substitute a different checkout.
- Recovery intentionally remains required follow-on scope. This commit must not be advertised as sufficient to reconnect an existing legacy graph to a fresh threads node.

### Final verification results

- `cargo test --offline --lib --test daemon_composed --test integration_sweep --test test_isolation -- --test-threads=1`: **659 passed total** (630 library + 14 daemon_composed + 7 integration_sweep + 8 test_isolation), 0 failed and 0 ignored; `/private/tmp/hg-task1-default-final.log`. This includes existing receipt semantics (`not_required` never dispatched), transcript request ACK/status behavior, journal replay, and isolation source scans.
- `git diff --check`: passed. `git -C third_party/herdr-threads status --short`: empty, confirming the external checkout was not changed.

The controller explicitly requires Task 2 to close the documented first-binding/writer transaction race before the final full-product compatibility claim. This Task 1 commit is a reviewed boundary, not the final recovery implementation.

- `cargo test --offline --no-default-features --lib threads:: -- --test-threads=1`: **52 passed, 0 failed**, `/private/tmp/hg-task1-no-default.log`.
- `cargo check --offline --all-targets --all-features`: passed, `/private/tmp/hg-task1-check-final.log`.
- The final real-daemon test additionally asserts that the actual seat-inspection JSON contains a null `open_binding` after bare resolution, independently of any provenance label.
