R1 Rust Herdr plugin crate builds: one herdr-graph binary (daemon, CLI, plugin actions) with manifest, build script and herdr-threads dependency
R2 Graph-instance Git repo with human-browsable teamspace/seat/clone layout, stable prefixed IDs, name history, slug collision handling, archive placement
R3 Single serialized writer: durable admission with op IDs, preconditions rechecked at apply, CAS commit with Graph-Op trailers, stale conflicts returned with explanation
R4 Readers (/seat, reconciler, CLI) always see one complete committed revision
R5 Crash recovery: no lost or duplicated op across journal/commit boundaries
R6 Cancellation (admitted only), supersession, reassignment, rejection reminders, delayed-instruction applicability checks
R7 Plan -> confirm -> apply for organizational changes: TTY [y/n] and agent relay with plan hash; stale plan re-confirmation only when relied-on state and effects changed
R8 No-confirmation categories: observed mutations, bookkeeping, approved-plan reconciliation/retries, opaque content writes, summarizer occupant relaunch
R9 Teamspace/seat/clone lifecycle (dormant/active/retired), overrides with precedence, participation intent, resurrection
R10 Herdr client: snapshot, event subscription, create/rename/close/split/start-agent with HERDR_GRAPH env
R11 Reconciliation drives Herdr toward latest committed intent: effect identity, fencing of obsolete effects, backoff retries, unknown-outcome inspection, session replacement with resume
R12 Observation: events + periodic snapshot; manual pane/tab/workspace closure -> containment cascade retirement as one action; occupant exit ends occupancy; renames tracked; moves -> reload required; disconnect -> unknown; graph-issued closures not double-retired
R13 Templates live: edits propagate immediately with plan listing affected instances; applications with member mappings, additions, exclusions, reuse, contributions
R14 Application withdrawal/hydration undo retires only exclusive members (incl. later-created), keeps shared/pre-existing; re-added members keep identity; copy preserves member ids
R15 Undo CLI: recent action list, selection, concrete preview, [y/n], compensating ops, caller-pane adoption, repair_required on conflicts
R16 herdr-threads: managed public seat/teamspace channels, required invites with explicit acceptance (never fabricated), participation join/leave scopes, notifications, cleanup on retirement
R17 Native session/transcript tracking per clone with transcript identity and byte-range boundaries
R18 Durable processing requests: per-seat summaries parameter (off for system roles), ACK/dispatch distinct from success, order-independent coverage with gaps, unresolved missing input, dedup
R19 Summarizer: ordinary seat from shipped template; graph relaunches an absent occupant of an active summarizer; retired summarizer not activated; delivery via threads (fallback now, service ACK when ht-5nb lands)
R20 /seat bootstrap: HERDR_GRAPH=1 session-start hook, caller resolution or create/resurrect/rebind proposal, file pointers, pending ops/invitations
R21 Shipped minimal skills and example templates; Beads label convention for successor discovery
R22 Tiered validation: unit/property, concurrency/crash, private-Herdr integration flows, real-agent smoke, threads tests; verified-vs-assumed separated
R23 Never disturb the user's live Herdr session or memory observer in tests
R24 Daemon startup convergence: journal recovery, fresh Herdr snapshot, re-correlate in-flight effects, reconcile to latest committed intent
R25 Shared harness launch/resume profiles (shell/claude/codex) used by templates, reconciler and smoke tests
R26 Every background loop (writer, reconciler, observer, threads connection, transcript watcher, reminder scheduler) started by the daemon composition root
R27 Opt-in integration test of the real ThreadsPort against an isolated herdr-threads daemon
R28 Undo lists plan-applied retirements/resurrections, not only cascades and template/application actions
