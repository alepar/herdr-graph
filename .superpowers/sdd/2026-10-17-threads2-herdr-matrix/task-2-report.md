# Task 2 implementation report

Base: `0d44ce66e9bdfea90388505cce07b2311f631d8d`. No live node transition, external source edits, native model, credential use, Herdr seat mutation, main merge or push.

## Interfaces and implementation

- New organizational kind `ThreadsReconnect`, registered through core kinds and therefore the production `mutation_registry`. CLI: `herdr-graph plan --json threads reconnect --from <canonical-node|legacy> --to <canonical-node> --head <40-lowercase-hex-main-commit>` followed by the existing confirmed `apply`. Strict flags, IDs, old binding and exact head are rechecked at writer application. `legacy` is source-only. Same-node restart needs no reconnect.
- Committed authoritative marker `threads-namespace.json` carries version 1, target node, fresh reserved action-ID epoch, exact previous node/epoch, base head and operation ID. A durable JSON artifact at `mutations/threads-reconnect/<op>.json` shares the same Git commit with all record updates. It contains original teamspace/seat/clone/request records, prior action IDs, and a SQLite read-transaction snapshot of all graph journal ops/effects/meta/notices, including exact serialized request/effect payloads.
- All owned channel references are cleared, including archived records, so later explicit resurrection cannot expose a stale channel. Invitations, opt-outs and external participation move to the explicit preserved artifact; no old external channel is reconstructed or invited. Incomplete non-merged requests retain IDs/ranges/attempt history, reset active receipt/dispatch/reminder state, and become pending. Completed requests/native transcript bytes/session histories remain unchanged.
- Threads channel ensure keys, effect identities, service delivery/reminder keys and topic/reload caches include the epoch. Old open effect rows cannot suppress new work; executors reject old epochs even when an object revision appears reusable. Historical pre-boundary plan/rename notification rows are not replayed as new-node intent.
- `namespace::ensure` retains the version-1/node sibling binding format. Once a marker commits, it accepts only its exact target and either the already-published binding or its exact previous binding. Local publication uses private synced temp + atomic rename + directory fsync. Corrupt/unexpected bindings fail closed. New service journals use `.graph-local/threads-intents-<epoch>` and get their own exact node binding; old journal directories are never moved, reset, rewritten or replayed as target-node work.
- The writer holds the shared namespace-publication lock through every apply/CAS. Initial binding takes the same sibling lock and scans all main ancestry for old committed records, including an empty current tree whose ancestors contain old references. Reconnect additionally takes an exclusive external-call lock. Each production service/map call acquires a shared call lock via `spawn_blocking`, verifies its captured task-local epoch against the authoritative committed marker, and holds that guard through the external call. Client/map creation and binding locks also run on blocking workers. No call lock is held while awaiting a writer commit.
- Threads effects, delivery preparations and liveness/notice snapshots capture their epoch from the same committed tree as their references. Bookkeeping stamps that captured epoch; writer application rejects stale channel/invitation/delivery/ACK writes. A graph mutation does not re-label old queued data with a new epoch.
- Ordinary undo explicitly refuses reconnect actions with reverse-reconnect guidance. It also refuses actions predating the latest boundary using the artifact's preserved prior action IDs; a pre-boundary retirement cannot silently reactivate old references through an inverse snapshot.
- Mapping trait documentation now says a resolved `Some` seat does not imply an enrolled agent. Recovery diagnostics distinguish exact original binding/store restoration from unbound legacy graphs that require reviewed reconnect.

## Crash semantics

1. Before Git CAS: no local binding or old intents changed. The old namespace remains authoritative. Writer recovery requeues the admitted transition using the same plan reservation/epoch, and exact head/from guards still apply.
2. After Git CAS, before writer journal completion/checkout: the committed target marker blocks stale snapshots and old-node endpoint calls. Existing writer recovery discovers its operation trailer and completes the journal without applying a second transition.
3. After commit, before binding publication: the first target-node call verifies the marker and exact previous binding, durably seals the previous intent directory, then publishes atomically. The sibling `threads-intents[-<previous-epoch>].retired.json` contains the exact entire transition, including old node/epoch, new node/epoch, operation ID and base head. No graph dispatch can reinterpret old refs in the target namespace.
4. After seal but before binding publication: the committed marker authorizes idempotent retry only if the existing seal exactly matches; direct low-level construction refuses that sealed directory under either node. During binding publication, rename leaves either the exact previous or exact target binding. Other/corrupt bindings or seals are refused. Original intent files are not modified.
5. After publication, before new service journal creation: old intents remain untouched; the epoch directory is created on retry. The new client never opens the old journal.
6. Multi-call routines interrupted by reconnect fail their next call's captured-epoch check. A result admitted after the boundary carries the old epoch and is rejected by the writer.

## Verification evidence

- `/private/tmp/hg-task2-red.log`: missing reconnect CLI plan and historical-empty-tree auto-binding regressions failed as expected. The broad name filter also selected an unrelated Herdr socket test denied by sandbox; later covering tests run with isolated-test escalation.
- `/private/tmp/hg-task2-green1.log`: initial reviewed legacy plan/apply/reapply regression passed.
- `/private/tmp/hg-task2-focused.log`: preservation fixture initially selected the wrong seat name; corrected fixture to its established `foreman` seat.
- `/private/tmp/hg-task2-focused2.log`: 4 passed; new reconciliation test initially used wrong participation CLI grammar. Corrected to existing `participation join <thread> --scope seat --seat foreman`.
- `/private/tmp/hg-task2-focused3.log`: **5 passed**, reviewed reconnect, preservation/reset/history, strict guards, channel recreation/no external invitations/stale channel write, reconnect undo refusal.
- `/private/tmp/hg-task2-undo-red.log`: pre-boundary retirement undo was accepted, proving the historical inverse gap. Added artifact prior-action provenance and boundary refusal.
- Covering commands and final-tree verification results are recorded below.

## Limits and self-review notes

- Reconnect does not migrate upstream threads databases, identities, memberships or native agents. New-node channels are recreated; external participation requires a separately reviewed resolution/join.
- Returning to a previously used node still creates a fresh graph epoch; it does not replay that node's dormant old intent directory.
- Filesystem locks protect cooperating graph code. Direct operator edits to Git/local bindings are not a transactional migration interface and fail closed where detected.
- Initial unbound-history detection scans complete main ancestry; preservation snapshots include the full graph journal. Long-lived graphs can take longer and produce larger migration artifacts. This favors exact recovery evidence over migration throughput.
- No real native-model or real summarizer-agent claim. The private service fixture uses synthetic public-hook enrollment where applicable; Task 3 owns the Herdr version matrix.

### Milestone

- Covering run `/private/tmp/hg-task2-covering2.log`: **637 library tests passed**, 0 failed/ignored (175.93 s); later integration outcome and fixture correction are recorded below.
- Additional prepared regressions cover initial-binding publication serialization, real transcript re-delivery and late old ACK refusal, eager production binding before first mutation, template/application external relationships, and crash points before/after Git CAS.

### Final-tree edge regressions and integration verification

- `/private/tmp/hg-task2-eager-red.log`: production assembly did not publish a fresh empty graph binding before the first mutation/call. Added eager discovery/binding (no service registration), with startup failures deferred to the existing exact call diagnostics.
- `/private/tmp/hg-task2-template-red.log`: old external template relationships remained live. Reconnect now snapshots original templates/applications and clears their thread-participation relationships only, preserving members, mappings, template links and system duties. The review plan lists exact affected template/application record paths and relationship counts.
- `cargo test --offline --features test-support --lib reconnect_ -- --test-threads=1`: **10 passed, 0 failed/ignored**, `/private/tmp/hg-task2-final-focused.log`. This run includes final template/application behavior, initial-binding serialization, request new-generation re-delivery/old ACK rejection, and panic interruption before CAS and after CAS before journal finalization.
- Covering library/daemon run was on the earlier tree: **637 library + 14 daemon_composed passed**, `/private/tmp/hg-task2-covering2.log`. Its integration sweep found a fixture failure before test_isolation was reached. It does not claim coverage of the later eager-binding/template additions; focused checks and final all-consumer compilation cover those additions separately.
- Integration fixture failure reproduced in `/private/tmp/hg-task2-sweep-reproduce.log`: 20.79 s timeout waiting for the foreman to retire. The fixture closed a tab then deliberately disconnected the subscription. FakeHerdr increments incarnation generation on resubscription and RuntimeLoop intentionally performs a conservative rebind, where absence is not retirement evidence. The test passed only when the close diff happened before that reconnect. Fixed the fixture to wait for the same-incarnation persisted baseline containing the pane, then emit its close over the existing subscription. No production observer semantics, assertion, timeout or safety fence changed.
- `cargo test --offline --test integration_sweep --test test_isolation -- --test-threads=1`: **7 + 8 passed**, 0 failed/ignored, `/private/tmp/hg-task2-integration-final.log` (10.23 s and 8.04 s). Full sweep verifies the corrected fixture.

Eager binding remains conservative: if threads is absent until a new graph already has records, explicit legacy reconnect is required. Planning validates the exact canonical target value; actual discovery must match it before dispatch. Planning does not prove daemon reachability or start/replace any daemon.

### Final seal regression and namespace checks

- `/private/tmp/hg-task2-journal-seal-red.log`: the preserved original journal could be reopened directly under the newly published binding. Added an exact-transition retirement seal written and synced before binding publication; low-level constructors now refuse sealed journals, including when no graph marker is supplied. Corrupt/conflicting seals cannot be overwritten or silently adopted.
- `cargo test --offline --features test-support --lib reconnect_ -- --test-threads=1`: **10 passed, 0 failed/ignored**, `/private/tmp/hg-task2-seal-green.log` (4.66 s). Covers the new seal regression, seal-before-binding interrupted retry, idempotence, corrupt seal refusal, byte preservation, and all earlier reconnect boundary regressions.
- `cargo test --offline --lib namespace -- --test-threads=1`: **5 passed, 0 failed/ignored**, `/private/tmp/hg-task2-namespace-final.log` (0.12 s). Includes production eager binding/cached-node replacement, strict override, first-binding race, malformed binding, populated legacy and currently-empty historical references.
- Production wrappers coordinate each call with the writer and verify captured epochs. Low-level direct `ServiceThreads` construction checks identity and rejects retired journals; independently held low-level clients do not themselves participate in the graph writer's call lock. Integrators bypassing production wrappers must supply equivalent coordination. No guarantee is made for direct operator edits or independently retained raw upstream clients.

- `cargo test --offline --no-default-features --lib threads:: -- --test-threads=1`: **54 passed, 0 failed/ignored**, `/private/tmp/hg-task2-threads-final.log` (21.47 s), after the seal implementation.
- `HG_REAL_THREADS=1 cargo test --offline --features private-herdr --test threads_real -- --test-threads=1 --nocapture`: **2 passed, 0 failed/ignored**, `/private/tmp/hg-task2-real-seal-final.log` (9.44 s), after the seal implementation. The production discovery fixture stops its private old daemon, preserves the old node directory, starts a private fresh node, observes the cached-client fence, creates/applies the real reconnect plan through the production mutation registry, refuses an unscoped stale call, and recreates a channel under the committed epoch. It checks new binding/directory, repeat-call identity, and preservation of old database/intents. The other fixture retains synthetic public-hook enrollment and cooperative invitation acceptance/service ACK. Earlier pre-seal real run also passed 2/2 (`/private/tmp/hg-task2-real-final.log`, 9.72 s).

## Self-review checklist

- Exact `--from`, canonical `--to`, exact head and plan reservation are enforced by plan recomputation under the writer publication lock.
- Marker and complete preservation artifact commit together; local identity publication cannot precede that commit or the old-journal seal.
- Cached production endpoints rediscover node identity. External calls hold shared quiescence guard, release it before writer wait, and writer rejects stale captured epochs. Reconnect lock acquisition order is calls then binding; every ordinary writer mutation also holds binding through apply/CAS, closing first-binding admission/publication races.
- Old effect rows retain provenance; fresh epochs use distinct channel/effect/delivery keys. Pending late ACK/channel writes cannot update target-node state. All old external refs, including template/application contributions, are preserved as unresolved artifact data and cleared from active dispatch sources.
- Pre-boundary undo is refused even if candidate selection skips the reconnect itself. Archived seats/clones also lose old active links, so future resurrection cannot restore old channels.
- Preserved completed request bytes, native transcript bytes, session histories and historical Git tree reads are checked. Old intent bytes and exact old operation/effect evidence remain available.
- No tests bypass the production namespace fence. The one integration fixture change removes artificial reconnect-induced ambiguity after reproducing the failure; assertions and production observer semantics remain intact.

## Final validation

- Final focused suite after adding explicit old cached-Graph/old-effect versus current-tree and new-effect versus old-tree regression: `cargo test --offline --features test-support --lib reconnect_ -- --test-threads=1` — **10 passed, 0 failed/ignored**, `/private/tmp/hg-task2-cache-final.log` (4.84 s). Both mismatches return obsolete with no external calls. Test-only snapshot construction initially used nonexistent `Default`; corrected to the existing FakeHerdr snapshot API before this successful run. Production code is unchanged from the 54-thread and 2-real-daemon successful runs.
- `cargo check --offline --all-targets --all-features` — **passed**, `/private/tmp/hg-task2-check-final.log` (7.47 s), final source/test tree. This compiles all consumers but does not claim runtime execution of the private Herdr version matrix.
- `cargo fmt --all --check` and `git diff --check` — final checks recorded at commit preparation.
- Cargo/test commands were sequential. Process/socket and isolated Git fixture commands used approved escalation. External `third_party/herdr-threads` status remains clean; no upstream source or dependency changes.

Task 2 is ready for independent review. The Herdr version matrix and full final combined task validation remain assigned to later tasks; this report deliberately distinguishes earlier covering runs from final production-code focused/private runs.
