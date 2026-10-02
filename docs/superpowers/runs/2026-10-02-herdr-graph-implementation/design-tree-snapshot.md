### hg-zmi.1 — Seam contract: crate scaffold, core model types and port traits
deps: []

Bottleneck seam bead: land ONLY the compilable skeleton every other bead codes against. Spec §1, §2.1-2.4, §3.1.
Deliver: Cargo crate herdr-graph (edition 2024), committed symlink third_party/herdr-threads -> /Users/alepar/AleCode/herdr-threads with path dependency on it; module tree per spec §1 table (model, store, journal, writer, plan, herdr, observe, reconcile, threads, transcripts, templates, undo, cli) with mod.rs stubs; model: prefixed ULID id newtypes (ts_, st_, cl_, ns_, tpl_, mem_, app_, op_, ef_, act_, pl_, tr_, rq_), record structs for teamspace/seat/clone/template/application/transcript/request/action/operation with serde TOML (fields per §2.4, schema+id+rev), Lifecycle/Runtime/Occupant enums, ChangeRequest/RequestKind/Requester (§3.1); port traits with doc comments and todo!() or fake-friendly signatures: Store (committed reads at a commit oid), Writer (admit(ChangeRequest)->OpId, status), HerdrApi (snapshot, subscribe, create/rename/close workspace|tab|pane, split, start_agent, process_info, send_keys), ThreadsPort (ensure_thread, invite, membership, notify, set_topic, release_requirement, send_request, receipt_state), Clock; cli/mod.rs with clap root and one file per command group under cli/ (each feature bead adds only its own file + one registration line); daemon/ipc frame types (4-byte BE length + JSON envelope, §1). Test-support feature 'test-support' with failpoint macro stub.
owns: model record schemas; port trait signatures (Store, Writer, HerdrApi, ThreadsPort, Clock); CLI module layout; IPC envelope types; action-record envelope schema (act_: id, kind, time, op ids, affected objects with before/after, retired vs already_retired, compensation data) shared by observer/templates/plan producers and undo; effect-record schema (ef_ identity = hash(op, object, kind, object_rev), fencing rev, status pending|done|obsolete|failed|unknown|needs_revision) that the journal effects table stores; launch env contract (HERDR_GRAPH=1, HERDR_GRAPH_INSTANCE, HERDR_GRAPH_SEAT, HERDR_GRAPH_CLONE) and harness launch/resume profiles table (shell: no agent; claude: kind claude, resume '--resume <id>', model '--model'; codex: kind codex, args '--no-daemon', resume 'resume <id>', model '-m'; session-id capture rule per harness); seat config key 'summaries' (bool; ordinary default true) and role marker 'role' (e.g. summarizer, system, cron, dispatcher → summaries default false); native-session (ns_) and transcript (tr_) record schema with byte-range [start,end) coverage/gap fields; ThreadsPort delivery capability query (fallback vs service-ack).
Acceptance: cargo build + cargo test green; serde round-trip test for each record type; no business logic.
Files: Cargo.toml, third_party/herdr-threads, src/lib.rs, src/main.rs, src/model/**, src/*/mod.rs, src/cli/mod.rs, src/ipc.rs, .gitignore
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): profiles table per spec §4.1 now carries cwd rule, launch args (codex --no-daemon), resume/model args, exit sequence (agent.send_keys), readiness/start outcomes (blocked_needs_human, needs_revision, unknown), session-id source and transcript locator (Claude slug = non-[A-Za-z0-9] → '-', CLAUDE_CONFIG_DIR, path-kind agent_session, glob fallback). Binding schema per §4.2 adds graph token `hg=<id>` and incarnation; ns_ records recorded cwd. Effect record carries predicted end state incl. induced container closures (§3.3, §4.4).

### hg-zmi.2 — Store: committed-revision reads, instance layout, slugging, path resolution
deps: ['hg-zmi.1']

Spec §1 (committed reads), §2.1 (slugs/collisions/name history), §2.3 layout, §2.2 archive placement. Implement Store over git2: open instance repo, resolve refs/heads/main to a commit, read any record/opaque file from that commit's tree (never the working tree), list objects by kind, resolve id -> current path, slug rules with -<id6> collision suffix, archive paths for retired seats/teamspaces; herdr-graph init <path> creates an instance repo (graph.toml, dirs, .gitignore for .graph-local/) with an initial commit. Pure tree-building helpers (path for record, tree edit set) used by the writer.
owns: Store impl; layout/path functions; init.
consumes: model record schemas; Store trait (boundary contract: hg-zmi.1).
blocked-by hg-zmi.1: consumes model record schemas and Store trait
Acceptance: unit tests for slugging/collisions/renames moving paths/archive paths; reads at an older commit return that revision even after later commits; init creates a valid instance.
Files: src/store/**, src/cli/init.rs

### hg-zmi.3 — Journal, serialized writer, preconditions and crash recovery
deps: ['hg-zmi.2']

Spec §3.4-3.6, §3.7 cancel. SQLite journal in <instance>/.graph-local/journal.sqlite3 (WAL, synchronous=FULL): ops table with state machine admitted->applying->committed|rejected, cancelled (only from admitted), superseded; effects table (keyed ef_ = hash(op,object,kind,rev)) schema only (reconciler fills it); checkpoint_commit. Writer: single tokio task FIFO; per op recheck relied_on (object rev / opaque blob hash) against committed tree -> apply a Mutation (trait/enum applied in-memory to a tree edit set; feature beads register mutation kinds) -> build blobs/trees with git2 -> commit with trailers Graph-Op/Graph-Action -> CAS refs/heads/main -> journal committed -> fast-forward working tree (skip dirty files with worktree_dirty warning). Recovery scans trailers since checkpoint. Failpoints at each boundary (test-support). Admission-time compatibility check against queued projection (best effort). Rejection carries reason/explanation/current revs. Also commit operations/<yyyy-mm>/<op>.toml summaries.
owns: Writer impl; Mutation registration API; journal schema incl. effects table and op states incl. cancelled and superseded with a supersedes link (consumed by hg-zmi.17); recovery; failpoint crash-injection harness.
consumes: Store layout + tree-building helpers.
blocked-by hg-zmi.2: consumes Store tree-building helpers and committed reads
Acceptance (tier 2 tests): two renames of same seat from same rev -> exactly one commits, other rejected with explanation; disjoint seat edits both commit; reader thread observing many commits never sees a partial multi-file change; crash at every failpoint then restart -> no lost or duplicate op (each op's trailer appears at most once, admitted ops eventually commit or reject); cancel of admitted op never commits; cancel vs commit race is linearizable (one of cancelled|committed).
Files: src/journal/**, src/writer/**, tests/writer_*.rs
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): working tree is a derived view (spec §3.5): FF re-run on daemon start after recovery; dirty files recorded in .graph-local/worktree_dirty (doctor + Notify); untracked/dirty leftovers under moved folders moved to .graph-local/orphans/<op>/; commit oid reported as view_rev.

### hg-zmi.4 — Daemon, IPC server, CLI framework and status/doctor
deps: ['hg-zmi.1', 'hg-zmi.2']

Spec §1 (daemon roles, lock, socket path + /private/tmp fallback), §10. herdr-graph daemon [--ensure]: owner flock in .graph-local/daemon.lock, idempotent ensure (spawn detached if not running), UDS server with 4-byte BE length JSON frames, dispatch to registered command handlers; a component registry so writer/reconciler/observer/transcript tasks register background loops; graceful shutdown. Client side: CLI sends commands to the daemon, read-only commands fall back to direct Store reads when no daemon. status/doctor commands (instance path, daemon pid, journal counts, Herdr socket reachability, threads registration state placeholder). Config: $HERDR_PLUGIN_CONFIG_DIR/config.toml instance key, HERDR_GRAPH_INSTANCE override.
owns: daemon process model; IPC server/client; component registry API; status/doctor.
consumes: IPC envelope types, Writer trait, Store trait (boundary contract: hg-zmi.1).
blocked-by hg-zmi.1: consumes IPC envelope types and port traits
blocked-by hg-zmi.2: consumes Store impl for daemon-less read-only fallback
Acceptance: integration test starts daemon in a temp instance, --ensure twice yields one daemon, CLI round-trips a command, status reports; socket path fallback for long paths tested.
Files: src/daemon/**, src/ipc.rs (impl), src/cli/status.rs, src/cli/daemon.rs, src/config.rs
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): locate-and-ensure chain (spec §1): HERDR_GRAPH_INSTANCE → `$HERDR_BIN_PATH plugin config-dir herdr-graph`/config.toml → ~/.config/herdr-graph/config.toml; mutating/seat/undo/request commands auto-run `daemon --ensure` (10s) and retry once, else fail clearly without admitting; `daemon --ensure` double-forks with setsid and returns when the socket answers; exits 0 when no instance is configured.

### hg-zmi.5 — Herdr socket client and private-Herdr test fixture
deps: ['hg-zmi.1']

Spec §4.1, §11 tier 3. Implement HerdrApi over NDJSON at $HERDR_SOCKET_PATH: session.snapshot (WorkspaceInfo/TabInfo/PaneInfo/AgentInfo incl agent_session, terminal_id), events.subscribe (workspace.*, tab.*, pane.created|closed|moved|exited|agent_detected, pane.agent_status_changed) as an async stream with reconnect, workspace/tab create (with --env equivalents, label, cwd, no focus), rename, close, pane split/close/process_info, agent.start {name,kind,pane_id,args}, agent send_keys. Verify exact method/param names against 'herdr api schema --json' (read-only). Test support: PrivateHerdr fixture (Rust port of herdr-threads tests/native/recovery/private_host.py pattern): temp root under /private/tmp/hg-<rand>, strip HERDR_* env, set HOME/XDG_*/HERDR_CONFIG_PATH/HERDR_SOCKET_PATH, spawn 'herdr server' in its own session, wait for socket, teardown SIGTERM own pid only after verifying argv; NEVER 'herdr server stop', never touch the user's default session. FakeHerdr in-memory implementing HerdrApi with scripted events for unit tests of other beads.
owns: HerdrApi impl; PrivateHerdr fixture; FakeHerdr; test-isolation guard (every test env scrubs HERDR_*, sets private HOME/XDG/CLAUDE_CONFIG_DIR/HERDR_PLUGIN_STATE_DIR, graph instance and threads state under the private root; a guard refuses to run if a socket path or state path resolves to the user's live default session, live threads state, or the user's memory observer; guard has its own unit test).
consumes: HerdrApi trait (boundary contract: hg-zmi.1).
blocked-by hg-zmi.1: consumes HerdrApi trait
Acceptance: tier-3 test (feature private-herdr) creates workspace/tab/panes with env in a private server, observes created/renamed/closed events, snapshot matches; FakeHerdr unit tests.
Files: src/herdr/**, tests/support/private_herdr.rs, tests/herdr_client.rs
Coverage r2: credential pass-through rule for real-agent tiers — only explicit env (ANTHROPIC_API_KEY, OPENAI_API_KEY) or the macOS keychain (not HOME-based) may reach private agents; never copy the user's settings, hooks, skills or memory-observer config; real-agent tests skip with reason when unauthenticated.
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): HerdrApi adds report_metadata tokens, agent.get, agent.send_keys, pane.current, process start-time lookup for incarnation. Spikes in the PRIVATE server, results written to docs/herdr-spikes.md and consumed by hg-zmi.8: (1) event order when closing a multi-pane tab / multi-tab workspace; (2) whether closing a workspace's last tab closes the workspace; (3) whether metadata tokens / labels survive a Herdr server restart and whether terminal_ids are reused; (4) whether a detached daemon started from a [[startup]] hook survives the hook's exit; (5) whether agent_session is reported for agent.start'ed Claude without Herdr's Claude integration. Design is defensive either way; spikes only choose token vs label-nonce.

### hg-zmi.6 — Plan/confirm/apply engine core and seat lifecycle kinds
deps: ['hg-zmi.3', 'hg-zmi.4']

SPLIT (promotion review): land ONLY the unblocking artifact. Spec §2.2, §3.2-3.3. Plan store in .graph-local/plans/, Plan{id,request,committed_rev,relied_on,effects,warnings}, Effect types (desired-state deltas the reconciler realizes: create tab/pane/start agent, close, rename, replace_session), plan_hash over canonical JSON, TTY [y/n] flow and agent relay 'apply <pl> --confirm <hash> --confirmed-by user-relay' (CLI handlers registered through the daemon IPC registry); apply recomputes against current committed rev and rejects stale_plan only when relied-on revs changed AND effects differ. Mutation-kind framework for organizational kinds + the kinds dependents need: teamspace create|retire, seat create (dormant|active)|activate|deactivate|retire|resurrect|rename, clone add|retire. Effective config resolver (member defaults <- template defaults <- seat overrides) incl. replace_session effect when effective harness/model changes on an occupied clone.
owns: Plan/Effect types and hashing; plan store; apply staleness; mutation-kind framework incl. category flag organizational|observed|bookkeeping|content (confirmation required only for organizational); effective config resolver (incl. summaries/role defaults); core seat/teamspace/clone kinds incl. rename (writes name_history) and every retire/resurrect kind (teamspace, seat, clone); act_ action records (hg-zmi.1 envelope) for retire/resurrect kinds so undo lists them; confirm/apply CLI surface incl. TTY prompt and relay-with-hash (--confirm <plan_hash> --confirmed-by user-relay).
consumes: Writer Mutation API, Store reads, daemon IPC command registry.
blocked-by hg-zmi.3: consumes Writer Mutation registration API and journal op states
blocked-by hg-zmi.4: consumes daemon IPC command registry for plan/apply handlers
Acceptance: plan hash stable; unrelated commit between plan and apply does not stale it; same-object change does; retire seat plan lists its clones; resurrect restores ids; effective config precedence.
Files: src/plan/**, src/model/effective.rs, src/cli/plan.rs
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): confirmation exactness (spec §3.3): apply executes only the confirmed effect set; any recomputed-vs-confirmed effect difference ⇒ stale_plan regardless of relied_on (recompute includes set-shaped reads); no bypass flag; non-TTY without --json prints and never applies; plans predict and show induced container closures. Seat deactivate leaves clones active with runtime absent.

### hg-zmi.7 — Reconciler: desired-vs-live effects, fencing, retries, session replacement
deps: ['hg-zmi.5', 'hg-zmi.6']

Spec §4.2 binding verification, §4.4, §4.5. Loop triggered by op commits, observations, 60s tick, backoff timers. Diff committed desired state vs last Herdr snapshot -> effects (create_workspace/create_tab with env HERDR_GRAPH=1, HERDR_GRAPH_INSTANCE, HERDR_GRAPH_SEAT, HERDR_GRAPH_CLONE / split_pane / start_agent with resume args / rename / close); effect records in journal effects table with ef_ identity; fence: before executing re-read committed object rev and mark obsolete if no longer implied; dependency ordering; transient retry backoff 1s..5min jitter; unknown outcomes inspected in snapshot before retry; needs_revision path notifies requester (via ThreadsPort.notify; tests use a local recording stub — FakeThreads is hg-zmi.11's). Record expected observations for correlation (consumed by observer). Bindings written back as bookkeeping mutations. Session replacement procedure (§4.5). Harness 'shell' = no agent start (tests). Summarizer occupant relaunch hook API for hg-zmi.12.
owns: Reconciler; effect identity/fencing; expected-observation correlation records; binding write-back and effect-status bookkeeping mutation kinds (no confirmation); effect executor registration API (other components such as threads register effect families); occupant relaunch hook API (used by transcripts for the summarizer); harness profile application (uses hg-zmi.1 profiles table).
consumes: HerdrApi trait, Writer, Store, plan effect types.
blocked-by hg-zmi.6: consumes plan effect types and effective config resolver
blocked-by hg-zmi.5: consumes FakeHerdr for unit tests
Acceptance: unit tests with FakeHerdr: activate seat -> tab+pane+agent effects in order; retire after queued create -> no tab created (obsolete); transient failure retried; unknown outcome inspected not duplicated; model change on occupied clone -> replacement with resume args.
Files: src/reconcile/**
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): Reconciler is the second half of the shared observer/reconciler loop step (spec §4.3.6, §4.4): never acts on a snapshot whose observations are uncommitted; no create effects for `unknown` objects; effects record predicted end states (incl. induced container closures) that the observer uses for classification (replaces the 30s correlation window); unknown outcomes resolved by token lookup; start_agent readiness precondition and outcomes (§4.1); session replacement per §4.5 (idle gate, exit timeout → needs_revision, no new agent).

### hg-zmi.8 — Observer: events + snapshots to observed mutations, closure cascades, renames, moves
deps: ['hg-zmi.7']

Spec §4.3, §2.2. Subscribe + initial/periodic (60s) snapshot; detect missing-from-complete-snapshot closure only when connection healthy and object was present before; map pane_closed/tab_closed/workspace_closed to cascade retirement actions (one act_ per cascade recording retired and already_retired ids, mechanism observed_*), pane_exited/agent gone -> end occupancy (session ended), renames -> name change with name_history (observed_at, event_at only if supplied), pane_moved -> reload_required + rebinding; correlation with reconciler expected observations to avoid duplicate retirement; disconnected/ambiguous -> availability unknown, never retirement; native session capture (agent_session id, Claude transcript path derivation ~/.claude/projects/<cwd-slug>/<id>.jsonl, process_info argv resume id) recorded as ns_ on clone (bookkeeping). Emits session-ended hook used by transcripts.
owns: observed mutation kinds; cascade action records; native session capture; session-ended hook API.
consumes: HerdrApi stream, Writer, reconciler expected-observation records, harness profiles session-id capture rule (hg-zmi.1).
blocked-by hg-zmi.7: consumes reconciler expected-observation correlation records
Acceptance: FakeHerdr tests: pane close retires clone only; last-pane close also closing tab retires seat once; workspace close retires dormant seats too; graph-issued close not double-retired; disconnect -> unknown; rename tracked; move -> reload_required; event loss recovered by snapshot.
Files: src/observe/**
Acceptance (coverage r2): per-harness session-id capture unit tests (claude agent_session id + transcript path; codex agent_session or argv resume id; shell none).
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): REDESIGNED (spec §4.3): the observer is a level-triggered structural differ. Events/tick/reconnect only trigger complete snapshots; persisted baseline .graph-local/baseline.json with incarnation; diffs only between same-incarnation complete snapshots; first snapshot after start/reconnect/incarnation change is a rebind pass (identity matching only; unmatched → unknown); matching by graph token then terminal_id then native session; diff yields move / rename / occupancy end/start / disappearance; containment grouping (workspace > tab > pane, moved-out panes handled as moves); every element classified against committed intent + journaled effects' predicted end states (explained → bookkeeping only); no wall-clock correlation window. Replaces the old event→mutation mapping and 30s correlation; 'expected-observation correlation records' from hg-zmi.7 are now effect predicted end states. Acceptance adds: Herdr restart → no mass retirement; close while daemon down → unknown not recreated; retire last clone → induced tab close explained; deactivate → no retirement; dropped rename recovered from diff; move emptying a tab.

### hg-zmi.9 — Templates and applications: live propagation, exclusivity withdrawal, re-addition
deps: ['hg-zmi.6']

Spec §5, §2.4 template/application records. Template create/edit/copy (member ids preserved on copy) as plan kinds; application apply (hydrate: create application record, seats dormant|active per member startup, reuse recorded in each participating application, relationship contributions, hydrate action record); live propagation on template edit (plan lists every affected application/seat/session replacement); member removal -> retire seats exclusive to the application (mechanism application_withdrawal), keep shared/pre-existing/independent; re-add same mem id -> resurrect/remap withdrawn seat, respect exclusions; retiring a template-member seat records exclusion; application retire (withdraw contributions, retire exclusive members incl. ones created by later edits, preview kept vs retired); repair_required for ambiguous dependencies.
owns: template/application mutation kinds incl. template copy (member ids preserved); effective desired structure computation (template semantics consumed by shipped templates); action records for hydrate/template edit/application retire (hg-zmi.1 envelope).
consumes: plan engine and effective config resolver.
blocked-by hg-zmi.6: consumes plan engine and mutation kinds framework
Acceptance: unit tests for Auth/Billing example (DESIGN-NOTES §Composition withdrawal): shared engineer kept, later-added Auth reviewer retired; repeated application of same template maps distinct seats; exclusion scoped per application; re-add keeps seat id; copy preserves member ids.
Files: src/templates/**, src/cli/template.rs

### hg-zmi.10 — Undo CLI: action selection, previews, compensating ops, caller-pane adoption
deps: ['hg-zmi.8', 'hg-zmi.9']

Spec §6. herdr-graph undo: list recent undoable act_ records newest first (closure cascades, retirements, hydrations, template edits, application retirements) with already-undone marked; select -> concrete preview -> [y/n]; --list/--json. Closure undo resurrects exactly 'retired' ids (not already_retired), dormant stay dormant, restored runtime gets new tab/panes; caller pane (HERDR_PANE_ID) adopted as one restored clone, existing binding shown and displaced clone retired with mechanism undo, conflict with another active occupant -> repair_required; hydration undo = application retire rule; template edit undo = inverse patch, later conflicting edits/overrides -> repair_required; undo retiring the caller's own pane prints op id before closure. Undo ops are compensating (kind undo, undoes act_).
owns: undo candidates, previews, compensations, pane adoption.
consumes: cascade action records (observer), application retire + template edit records (templates), plan-applied retire/resurrect act_ records (hg-zmi.6), plan engine.
blocked-by hg-zmi.8: consumes cascade action records with retired/already_retired
blocked-by hg-zmi.9: consumes application retire and template edit action records
Acceptance: unit tests: undo tab-close cascade restores seat+clones excluding already-retired; adoption preview with bound caller pane; hydration undo after later reuse keeps reused seat; template edit undo with conflicting later edit -> repair_required.
Files: src/undo/**, src/cli/undo.rs
Acceptance (coverage r2): undo of a plan-applied seat retirement previews and applies resurrection.

### hg-zmi.11 — Threads integration: system channels, required invites, participation, notifications, cleanup
deps: ['hg-zmi.7', 'hg-zmi.5', 'hg-zmi.17']

Spec §7.1-7.3. Real ThreadsPort over herdr_threads::client::service::PersistentServiceClient (register service_session_v1 today; intents dir .graph-local/threads-intents/) + LocalSocketClient claimless reads; FakeThreads. Reconciler effects: EnsureThread for seat and teamspace channels (OperationId derived from object id), SetTopic on rename, Invite Required to occupied clones' threads seats (map clone pane -> threads seat via Seats{target}), Membership polling to record pending|accepted|released|retired, Ordinary invites for seat-wide participation, whole-seat leave notification with delayed instruction {op,seat,rev}, ReleaseRequirement on clone retirement, Notify for renames/path changes/rejections/reminders/reload_required; ServiceBusy/disconnect -> backoff, never takeover. herdr-graph who <pane>.
owns: ThreadsPort real impl + FakeThreads; threads effects (registered via the reconciler effect executor API); who command; pending-invitations query used by /seat; delivery capability detection (service_session_v2 registration probe → service-ack, else Notify fallback); opt-in real herdr-threads integration test (HG_REAL_THREADS=1, isolated threads state dir + private Herdr) covering EnsureThread, Invite Required, acceptance never fabricated, Membership, Notify, ReleaseRequirement cleanup.
consumes: reconciler effect framework, ThreadsPort trait, participation intent records + scope semantics (hg-zmi.17), PrivateHerdr fixture + test-isolation guard (hg-zmi.5) for its opt-in real test.
blocked-by hg-zmi.7: consumes reconciler effect framework and identity
Acceptance: FakeThreads unit tests for all effects incl never fabricating acceptance; opt-in real-daemon test (HG_REAL_THREADS=1) against an isolated threads daemon + private Herdr for EnsureThread/Invite Required/Membership/Notify/ReleaseRequirement.
Files: src/threads/**, src/cli/who.rs, tests/threads_*.rs
blocked-by hg-zmi.17: consumes participation join/leave scope semantics
blocked-by hg-zmi.5: consumes PrivateHerdr fixture and test-isolation guard
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): membership cleanup keyed to occupancy (spec §7.1): occupancy end (from the differ) → ReleaseRequirement for that occupant; occupant started/changed → re-invite the new occupant's threads seat (Required for system channels, Ordinary for seat-wide participation); clone retirement also releases.

### hg-zmi.12 — Transcripts and processing requests: coverage, delivery, summarizer relaunch
deps: ['hg-zmi.11', 'hg-zmi.8']

Spec §8.1-8.2, §7.4. On session-ended hook (and /clear session change, resume append) create rq_ for [covered_end, current_size) when source seat effective summaries=true; dedup/merge by (tr, range); missing transcript -> unresolved missing_input; destination summarizer seat (teamspace role summarizer else graph.toml summarizer_seat); retired summarizer -> stay pending, no activation; active summarizer with no occupant -> relaunch via reconciler hook (authority: op that made it active). Delivery via ThreadsPort.send_request: NotifyFallback (default; Notify Warn with rq id on summarizer seat channel; ACK via 'herdr-graph request ack <rq>' recording dispatched_at only) and ServiceAckDelivery behind cargo feature threads-service-ack using agreed ht-5nb shapes (ServiceOperation::Send/Receipts, service_session_v2). Coverage: byte ranges half-open newline-aligned, order-independent union, gaps preserved, late results never shrink coverage. CLI: request list --pending|--unresolved|--undispatched, ack, complete --output --covered.
owns: transcript/request records logic; transcript watcher loop (tracks per-clone transcript files: identity + current size; registered as a daemon loop); bookkeeping mutation kinds for tr_/rq_ records (coverage, dispatch ack, completion; no confirmation); delivery adapters; request CLI.
consumes: observer session-ended hook + ns_ records, ThreadsPort real/fake, reconciler relaunch hook.
blocked-by hg-zmi.8: consumes session-ended hook and native session records
blocked-by hg-zmi.11: consumes ThreadsPort impl for Notify delivery
Acceptance: proptest coverage merge under arbitrary result order; summaries=false seat creates no request; disabling keeps pending; missing input unresolved; retired summarizer not activated; absent occupant relaunched; ACK != success.
Files: src/transcripts/**, src/cli/request.rs
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): session identity from graph's own SessionStart hook via `herdr-graph session-report` first, then Herdr agent_session, then process argv (spec §8.1); transcript locator from the profile table; deterministic summarizer destination and `undeliverable` flagging; daemon-owned liveness scan every 10 min (redeliver undelivered, re-deliver un-ACKed after 30 min, retry dispatched-not-completed after 6 h) — the role's /loop is convenience only (spec §8.2).

### hg-zmi.13 — Bootstrap and shipped skills/templates: /seat, graph skill, setup hook, Beads labels
deps: ['hg-zmi.9', 'hg-zmi.11', 'hg-zmi.17', 'hg-zmi.6', 'hg-zmi.4']

Spec §9, §8.3. herdr-graph seat [--json]: resolve caller pane (HERDR_PANE_ID + binding/env/agent_session) -> clone/seat/teamspace or unbound/ambiguous with candidate list and a proposed create/resurrect/rebind plan id; output paths of applicable templates, global/team/seat rules, seat AGENTS.md, clone files, pending ops, pending invitations with exact herdr-threads accept-required commands, reload_required, and the Beads query 'bd list --label hg-seat:<st>'. herdr-graph path/show/list. herdr-graph setup claude installs an owned SessionStart hook group that prints 'run /seat' when HERDR_GRAPH=1 (follow herdr-threads owned-group pattern; idempotent, uninstallable). skills/seat/SKILL.md, skills/graph/SKILL.md (plan -> show user -> relay confirm, content write, ops, check-instruction, Beads labels hg-ts:/hg-seat:/hg-clone:/hg-ns:). Shipped example templates under templates/ in the plugin: system-summarizer (AGENTS.md: ACK on dispatch, subagent per request, request complete, /loop 1h leftover scan, summaries=false), project-team (foreman active + researcher deferred), feature-team; 'herdr-graph init --with-examples' copies them into an instance. content write command.
owns: seat resolution command; skills; setup command 'herdr-graph setup claude' that installs both the SessionStart hook group and the skills (seat, graph) into CLAUDE_CONFIG_DIR, idempotent and uninstallable; example templates incl. system-summarizer (role=summarizer, summaries=false); content write CLI.
consumes: plan engine (rebind/create proposals), template semantics (hg-zmi.9) for shipped templates, pending-invitations query (hg-zmi.11), pending-ops query (hg-zmi.17), launch env contract (hg-zmi.1), daemon CLI framework.
blocked-by hg-zmi.6: consumes plan engine for proposed create/resurrect/rebind plans
blocked-by hg-zmi.4: consumes CLI framework and daemon client
Acceptance: unit tests for resolution (bound, unbound, ambiguous, moved pane); setup hook idempotent and reversible in a temp CLAUDE_CONFIG_DIR; init --with-examples yields templates that hydrate via the template engine.
Files: src/cli/seat.rs, src/cli/setup.rs, src/cli/content.rs, src/bootstrap/**, skills/**, templates/**
blocked-by hg-zmi.9: consumes template semantics for shipped templates
blocked-by hg-zmi.11: consumes pending-invitations query
blocked-by hg-zmi.17: consumes pending-ops query
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): `setup claude` hook runs `herdr-graph session-report --from-hook claude` in addition to prompting /seat when HERDR_GRAPH=1; `content write --object <id> --rel <path>` resolves at write time and rejects non-live targets (spec §3.5); /seat reports view_rev and worktree_dirty; `rebind <clone> --pane <pane>` CLI surface for proposed rebind plans (kind owned by hg-zmi.17). Summarizer AGENTS.md writes via content write --object.

### hg-zmi.14 — Packaging: herdr plugin manifest, build script, README with verified-vs-assumed matrix
deps: ['hg-zmi.4']

Spec §12. herdr-plugin.toml (id herdr-graph, min_herdr_version 0.9.1, platforms macos, [[build]] scripts/build.sh -> cargo build --release + copy to bin/, [[startup]] bin/herdr-graph daemon --ensure, [[actions]] status/doctor). README: what it is, install (herdr plugin link — do not run against the live session in tests), init instance, setup claude, /seat, plan/confirm/relay, undo, threads amendment status (ht-5nb, fallback delivery), third_party/herdr-threads symlink re-pointing, test tiers and how to run them, link to docs/verification-matrix.md (written by hg-zmi.19). LICENSE (MIT OR Apache-2.0 like threads).
owns: manifest, build script, README (incl. an empty 'Verification matrix' section placeholder that hg-zmi.19 fills), LICENSE. Herdr manifests do not declare Claude skills/hooks; README documents 'herdr-graph setup claude' (hg-zmi.13) for those.
consumes: daemon --ensure entrypoint and status/doctor actions.
blocked-by hg-zmi.4: consumes daemon --ensure and status/doctor commands
Acceptance: scripts/build.sh builds and places binary; manifest parses (herdr plugin link against a PRIVATE Herdr server in a test, never the live one).
Files: herdr-plugin.toml, scripts/build.sh, README.md, LICENSE-*

### hg-zmi.15 — Configuration smoke: harness shell, claude, codex launch and resume in private Herdr
deps: ['hg-zmi.7', 'hg-zmi.5']

Spec §4.1, §4.5, §11 tiers 3-4: the spec enumerates harness configurations (shell for tests, claude, codex) and resume vs fresh. Exercise each early in a PRIVATE Herdr server: activate a seat with harness shell (tab+pane+env, no agent); with HG_REAL_AGENTS=1 activate claude and codex seats (agent.start detected idle), capture agent_session/ns_, exit occupant and relaunch with resume args (claude --resume <id>; codex resume <id>), verify env HERDR_GRAPH_* present in pane. Report which configurations were verified vs skipped.
consumes: HerdrApi client + PrivateHerdr fixture; reconciler start/relaunch effects.
blocked-by hg-zmi.5: consumes PrivateHerdr fixture and HerdrApi client
blocked-by hg-zmi.7: consumes reconciler start_agent and session replacement effects
Acceptance: shell config passes in default private-herdr tier; claude/codex pass with HG_REAL_AGENTS=1 or are reported skipped with reason.
Files: tests/config_smoke.rs

### hg-zmi.16 — Real-agent smoke: /seat bootstrap and summarizer request ack/complete end to end
deps: ['hg-zmi.5', 'hg-zmi.13', 'hg-zmi.12', 'hg-zmi.18']

Spec §11 tier 4. With HG_REAL_AGENTS=1 in a PRIVATE Herdr server and an isolated threads daemon (or NotifyFallback): create an instance with init --with-examples, apply project-team + system-summarizer, activate foreman (claude) -> SessionStart hook prompts /seat -> herdr-graph seat resolves identity; end foreman session -> rq_ created -> summarizer (claude) relaunched if absent -> request ack -> request complete with coverage; assert ACK recorded separately from completion. Uses the plugin's real skills/templates.
consumes: bootstrap/skills/templates, transcripts/request flow, PrivateHerdr fixture.
blocked-by hg-zmi.13: consumes /seat command, setup hook and shipped templates
blocked-by hg-zmi.12: consumes transcript request creation, delivery and request CLI
blocked-by hg-zmi.5: consumes PrivateHerdr fixture
blocked-by hg-zmi.18: consumes daemon composition root
Acceptance: test passes with HG_REAL_AGENTS=1 (record transcript evidence path in report) or is reported skipped with reason; never uses the user's live session.
Files: tests/real_agent_smoke.rs

### hg-zmi.17 — Plan kinds remainder: ops/cancel/reassign/check-instruction CLI, reminders, override/participation/teamspace kinds
deps: ['hg-zmi.6']

Remainder of the hg-zmi.6 SPLIT. Spec §3.7, §10. Kinds: teamspace rename (teamspace retire/resurrect live in hg-zmi.6), seat override (harness/model/args/summaries/instructions_sections), participation join|leave --scope seat|clone (intent only; threads effects are hg-zmi.11's). CLI: ops [--unresolved], op <id>, cancel <op> (admitted only), reassign <op> --to <seat>, check-instruction <op> <obj> <rev> (current|obsolete), supersedes linking. Rejection reminder schedule: re-notify requester at 1h, 6h, 24h via ThreadsPort.notify (local recording stub in tests), stopped by a replacement (supersedes) or cancel.
owns: remaining organizational kinds incl. clone rebind (bind an existing clone to a different pane after ambiguity); op supersession (supersedes link: committed op marked superseded by a replacement request, admitted op replaced); ops CLI incl. pending-ops query used by /seat; reminder scheduler (registered as a daemon loop).
consumes: plan engine core and mutation-kind framework.
blocked-by hg-zmi.6: consumes mutation-kind framework and plan engine core
Acceptance: unit tests for each kind; cancel/reassign/check-instruction per spec; reminder schedule fires at 1h/6h/24h on a fake clock and stops on replacement or cancel.
Files: src/plan/kinds_extra.rs, src/plan/reminders.rs, src/cli/ops.rs
Acceptance (added, coverage r1): supersession unit test (replacement marks original superseded and stops its reminders/effects); rebind unit test.

### hg-zmi.18 — Daemon wiring: start writer, reconciler, observer, threads connection and transcript loops
deps: ['hg-zmi.11', 'hg-zmi.4', 'hg-zmi.10', 'hg-zmi.13', 'hg-zmi.8', 'hg-zmi.12', 'hg-zmi.17', 'hg-zmi.9', 'hg-zmi.3', 'hg-zmi.7']

Spec §1 (daemon hosts writer, reconciler, observer, transcript watcher, threads service connection). Owner of composing all components inside 'herdr-graph daemon': register the writer as the IPC mutation handler (admission returns op id), start reconciler loop, observer subscription+snapshot loop, threads PersistentServiceClient registration, transcript watcher; wire hooks (op committed -> reconciler trigger; observer session-ended -> transcripts; reconciler relaunch hook -> transcripts); configuration (Herdr socket from HERDR_SOCKET_PATH, instance). Graceful shutdown and restart recovery ordering (journal recovery before loops).
owns: daemon composition root: registration of every mutation kind (core, remainder, templates, undo, observed, bookkeeping) and every CLI command handler, every background loop (writer, reconciler, observer, threads connection, transcript watcher, reminder scheduler); startup convergence order: journal recovery → fresh Herdr snapshot → re-correlate in-flight effects → reconcile to latest committed intent.
consumes: writer, daemon registry, reconciler, observer, threads port impl, transcripts.
blocked-by hg-zmi.3: consumes Writer impl and recovery entrypoint
blocked-by hg-zmi.4: consumes daemon component registry and IPC dispatch
blocked-by hg-zmi.7: consumes reconciler loop
blocked-by hg-zmi.8: consumes observer loop and session-ended hook
blocked-by hg-zmi.11: consumes ThreadsPort real impl
blocked-by hg-zmi.12: consumes transcript watcher and request delivery
Acceptance: daemon starts against a FakeHerdr/FakeThreads config and a real temp instance; CLI plan/apply via IPC commits; killing and restarting the daemon recovers the journal before loops start.
Files: src/daemon/compose.rs, src/main.rs
blocked-by hg-zmi.9: consumes template/application kinds to register
blocked-by hg-zmi.10: consumes undo command and kinds to register
blocked-by hg-zmi.13: consumes seat/setup/content commands to register
blocked-by hg-zmi.17: consumes reminder scheduler loop and remainder kinds
Acceptance (added, coverage r1): restart test proves startup convergence order; reminder loop fires on a fake clock in the composed daemon.
Acceptance (coverage r2): crash the composed daemon mid-effect (failpoint after effect dispatch, before status write) → restart → effect re-correlated from snapshot and executed at most once.

### hg-zmi.19 — Private-Herdr end-to-end flows (tier 3) and verified-vs-assumed matrix
deps: ['hg-zmi.18', 'hg-zmi.14', 'hg-zmi.10', 'hg-zmi.9', 'hg-zmi.13', 'hg-zmi.5', 'hg-zmi.16', 'hg-zmi.15', 'hg-zmi.17']

Spec §11 tier 3, §12 README matrix. In a PRIVATE Herdr server (PrivateHerdr fixture; harness shell seats), drive the real daemon + CLI: create teamspace+seats -> tabs/panes appear with HERDR_GRAPH_* env; rename tab -> seat renamed (path moved, history); close pane / tab / workspace -> correct cascades (dormant included); undo tab close -> restored seat+clones in new tab; template edit adds member -> new tab appears; application retire -> exclusive seats closed; event-buffer loss / reconnect -> snapshot reconcile converges; daemon kill mid-op -> restart recovers without duplicates. Produce docs/verification-matrix.md listing each contract behaviour as verified-real / verified-fake / assumed, consumed by the README.
consumes: daemon wiring, undo, templates, PrivateHerdr fixture.
blocked-by hg-zmi.18: consumes daemon composition root
blocked-by hg-zmi.10: consumes undo command
blocked-by hg-zmi.9: consumes template/application kinds
blocked-by hg-zmi.5: consumes PrivateHerdr fixture
Acceptance: cargo test --features private-herdr passes all listed flows; matrix written.
Files: tests/e2e_private_herdr.rs, docs/verification-matrix.md
Flows (added, coverage r1): seat override (model change → session replacement with shell harness recorded), participation join/leave scopes, cancel/reassign of a rejected op, resurrection of a retired seat, /seat resolution in a bound pane. Matrix incorporates hg-zmi.15 and hg-zmi.16 results (verified or skipped-with-reason).
blocked-by hg-zmi.13: consumes /seat command
blocked-by hg-zmi.15: consumes configuration smoke results for the matrix
blocked-by hg-zmi.16: consumes real-agent smoke results for the matrix
blocked-by hg-zmi.17: consumes override/participation/cancel/reassign kinds
Coverage r2 additions: owns the README 'Verification matrix' section content (written from docs/verification-matrix.md). Flows also: install the built plugin into the private Herdr (herdr plugin link against the PRIVATE server with its own HERDR_PLUGIN_STATE_DIR), invoke status action, confirm startup '--ensure' runs one daemon; manual pane close → cascade → undo run from a pane in the private Herdr adopts that pane as the restored clone.
blocked-by hg-zmi.14: consumes plugin manifest and build script
Roast design r1 amendments (spec is authoritative; see 2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md): add flows per spec §11 tier 3: Herdr private-server restart (rebind pass, no mass retirement, no duplicates), close while daemon down (unknown, no recreation), retire last clone (induced tab close explained), seat deactivate, pane move emptying a tab, dropped rename recovered.

### hg-zmi.20 — Integration sweep: herdr-graph MVP
deps: ['hg-zmi.19', 'hg-zmi.18', 'hg-zmi.9', 'hg-zmi.3', 'hg-zmi.16', 'hg-zmi.5', 'hg-zmi.8', 'hg-zmi.13', 'hg-zmi.12', 'hg-zmi.2', 'hg-zmi.17', 'hg-zmi.15', 'hg-zmi.10', 'hg-zmi.7', 'hg-zmi.14', 'hg-zmi.1', 'hg-zmi.11', 'hg-zmi.6', 'hg-zmi.4']

Root integration sweep (unknown-unknowns net). Verify the goal's main flows end to end against the composed daemon in a PRIVATE Herdr: init instance → apply project-team + system-summarizer → seats activate (tabs/panes/env) → /seat resolves → rename/close cascades → undo → transcript request created and delivered (fallback) → request ack/complete. Add integration tests no per-seam bead covers; sweep for unwired config values, parameters, interfaces (summaries/role defaults, launch env, harness profiles, action/effect envelopes, capability switch); fix small gaps inline, file blockers for big ones. Run the full suite (cargo test, --features private-herdr) and record results.
blocked-by hg-zmi.1: consumes all leaves (integration sweep)
blocked-by hg-zmi.2: consumes all leaves (integration sweep)
blocked-by hg-zmi.3: consumes all leaves (integration sweep)
blocked-by hg-zmi.4: consumes all leaves (integration sweep)
blocked-by hg-zmi.5: consumes all leaves (integration sweep)
blocked-by hg-zmi.6: consumes all leaves (integration sweep)
blocked-by hg-zmi.7: consumes all leaves (integration sweep)
blocked-by hg-zmi.8: consumes all leaves (integration sweep)
blocked-by hg-zmi.9: consumes all leaves (integration sweep)
blocked-by hg-zmi.10: consumes all leaves (integration sweep)
blocked-by hg-zmi.11: consumes all leaves (integration sweep)
blocked-by hg-zmi.12: consumes all leaves (integration sweep)
blocked-by hg-zmi.13: consumes all leaves (integration sweep)
blocked-by hg-zmi.14: consumes all leaves (integration sweep)
blocked-by hg-zmi.15: consumes all leaves (integration sweep)
blocked-by hg-zmi.16: consumes all leaves (integration sweep)
blocked-by hg-zmi.17: consumes all leaves (integration sweep)
blocked-by hg-zmi.18: consumes all leaves (integration sweep)
blocked-by hg-zmi.19: consumes all leaves (integration sweep)

### hg-zmi.21 — Design fix r1 C1: observer redesign as level-triggered snapshot differ
deps: []

Redesign applied (step-back r1). Spec §4.2-§4.4 rewritten; beads hg-zmi.7/.8 amended.
Roast reports: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1.md. Step-back record: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md. Applied inline as a design (spec + bead description) fix; no code.

### hg-zmi.22 — Design fix r1 C2: harness launch profile table (cwd, args, exit, readiness, transcript locator)
deps: []

Spec §4.1 profile table, §4.5, §8.1; beads hg-zmi.1/.7/.12 amended.
Roast reports: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1.md. Step-back record: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md. Applied inline as a design (spec + bead description) fix; no code.

### hg-zmi.23 — Design fix r1 C3: locate-and-ensure chain for CLI/daemon
deps: []

Spec §1; bead hg-zmi.4 amended.
Roast reports: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1.md. Step-back record: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md. Applied inline as a design (spec + bead description) fix; no code.

### hg-zmi.24 — Design fix r1 C4: working tree as derived view; content write by object id
deps: []

Spec §3.5/§3.6/§10; beads hg-zmi.3/.13 amended.
Roast reports: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1.md. Step-back record: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md. Applied inline as a design (spec + bead description) fix; no code.

### hg-zmi.25 — Design fix r1 C5: confirmation exactness (apply only confirmed effects, no bypass)
deps: []

Spec §3.3; bead hg-zmi.6 amended.
Roast reports: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1.md. Step-back record: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md. Applied inline as a design (spec + bead description) fix; no code.

### hg-zmi.26 — Design fix r1 C6: daemon-owned summarizer liveness and deterministic destination
deps: []

Spec §8.2/§8.3; bead hg-zmi.12 amended.
Roast reports: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1.md. Step-back record: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md. Applied inline as a design (spec + bead description) fix; no code.

### hg-zmi.27 — Design fix r1: threads membership cleanup keyed to occupancy
deps: []

Spec §7.1; bead hg-zmi.11 amended.
Roast reports: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1.md. Step-back record: docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-roast-design-1-step-back.md. Applied inline as a design (spec + bead description) fix; no code.

