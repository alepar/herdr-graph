# Integration sweep (hg-zmi.20)

Root sweep over the composed daemon: the goal's main flows end to end, plus a search for unwired config values,
parameters and interfaces.

| | |
|---|---|
| date | 2026-10-02 |
| code under test | task branch `task-hg-zmi.20` on `dedd4d3` (integration branch + stack parent hg-zmi.19); the sweep's own commit is the branch head |
| herdr | 0.9.1, one private server per tier-3 test, never the user's session |
| machine | macOS, aarch64 |

## New tests (`tests/integration_sweep.rs`)

| test | tier | covers |
|---|---|---|
| `golden::sweep_golden_path` | `private-herdr` | init `--with-examples` -> daemon -> `template edit` project-team and system-summarizer (harness `shell`) -> teamspace + both applications -> foreman tab/pane + the four `HERDR_GRAPH_*` env values in the pane shell -> `seat --json` bound -> tab rename (seat renamed, folder moved, `name_history`) -> `session-report` -> close the last pane (pane -> tab -> seat retired, exactly one `closure_cascade` act) -> request listed -> `request ack` (dispatched, not completed) -> `content write` into the archived folder -> `request complete` (coverage recorded) -> `plan undo <act>` + apply (seat and clone back, NEW tab, folder out of the archive with the summary in it) |
| `wiring::sweep_summaries_role_defaults_flow_end_to_end` | default | role `summarizer` -> effective `summaries = false` -> its own session end creates no request; an ordinary seat's does (one transcript, owned by the worker) |
| `wiring::sweep_launch_env_reaches_create_calls` | default | workspace create carries 2 values, tab create and split carry the four ids of the clone they create, and the pane really received them |
| `wiring::sweep_harness_profiles_used_for_start_and_resume` | default | `StartAgent` args equal `profile(h).argv(..)` for claude (`--model sonnet`) and codex (`--no-daemon -m gpt-x`); after an ended session and deactivate/activate the claude start resumes (`--resume native-1 --model sonnet`) |
| `wiring::sweep_action_and_effect_envelopes_round_trip` | default | every `actions/*/*.toml` parses as `ActionRecord` and round-trips; every journal effect row round-trips JSON; `undo.list` offers every act |
| `wiring::sweep_capability_switch` | default and `threads-service-ack` | without the feature a service reporting ServiceAck is still used through Notify only; with it the request goes through `send_request` to the mapped summarizer seat and does not also notify |
| `wiring::sweep_config_values_are_read` | default | `graph.toml` `defaults.harness/model` launch a seat that sets neither (codex `--no-daemon -m m-from-graph`), a seat override beats them, and `summarizer_seat` receives the request when no seat has the summarizer role |
| `wiring::sweep_every_request_kind_has_a_mutation` | default | all 25 `RequestKind` variants have a registered mutation (exhaustive match: a new variant breaks the build here) |
| `cli_sweep::sweep_every_cli_command_reaches_a_handler` | `test-support` | the binary with `HG_TEST_FAKE_SERVICES=1`: all spec section 10 commands, with real ids, print no "not implemented" and never panic |

Golden-path delivery outcome (recorded, as the brief asks): with no threads daemon and a `shell` summarizer (no agent
occupant) the request stays `pending` with no delivery attempt and no `undeliverable` reason: the destination
resolves to "relaunch", which defers delivery ("summarizer has no occupant yet"). Delivery itself is covered by
tier 2 (`session_end_flows_to_transcripts`, `sweep_capability_switch`) and tier 5 (`threads_real`).

## Full-suite results

| command | result |
|---|---|
| `cargo build` | ok |
| `cargo test` | lib 430, daemon_composed 6, daemon_ipc 11, integration_sweep 7, packaging 3 (+1 ignored), writer_concurrency 3: 460 passed, 0 failed |
| `cargo test --features test-support` | lib 431, daemon_composed 9, daemon_ipc 11, integration_sweep 8, packaging 3 (+1 ignored), writer_concurrency 3, writer_crash 2: 467 passed, 0 failed |
| `cargo test --features private-herdr -- --test-threads=1` | lib 430, config_smoke 4, daemon_composed 6, daemon_ipc 11, e2e_private_herdr 25 (+4 ignored DEFECT repros), herdr_client 5, integration_sweep 8, packaging 4 (+1 ignored), real_agent_smoke 1 (skips), threads_real 1 (skips), writer_concurrency 3: 498 passed, 0 failed; herdr_spikes 5 ignored (run with `--ignored`) |
| `cargo test --features threads-service-ack` | lib 431, daemon_composed 6, daemon_ipc 11, integration_sweep 7, packaging 3 (+1 ignored), writer_concurrency 3: 461 passed, 0 failed |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `HG_REAL_THREADS=1 cargo test --features private-herdr --test threads_real` | 1 passed (isolated herdr-threads daemon built from `third_party/herdr-threads`) |
| `HG_REAL_AGENTS=1` runs (`config_smoke` claude/codex, `real_agent_smoke`) | skipped: real agents need `claude`/`codex` on PATH and API credentials, which this run does not use; those rows stay `assumed` in `docs/verification-matrix.md` |

Flakiness checks after the fixes below: `daemon_composed` with `test-support` 25 of 25 runs green (it failed 11 of 25
before); `integration_sweep` default 30 of 30, `test-support` 60 of 60, golden path with `private-herdr` 4 of 4.

## Sweep checklist

| item | result |
|---|---|
| `not_implemented(` / `todo!(` / `unimplemented!(` in `src/` | none on a section 10 path. Before this task: `cli::not_implemented` (no caller) and a stale unit test asserting that `session-report` is "not implemented" (it was implemented by hg-zmi.12; the test failed and read stdin). Both removed. 0 occurrences now |
| every `register_*` is called from `compose.rs` | yes: `register_core_kinds`, templates / undo / kinds_extra `register_kinds`, `KindRegistry::register_mutations`, `bookkeeping`, `observe`, `threads`, `transcripts`, `bootstrap`, `plan::ops` `register_mutations`; `plan::commands`, `plan::ops`, `undo`, `threads`, `bootstrap`, `transcripts` `register_commands`; `register_loop` (reminders), `transcripts.register_loops`, `threads::register_with`, `transcripts.register_with`. `Reconciler::register_source/executor` are only called from those `register_with` functions |
| `ReconcilerConfig` / `WriterConfig` fields set from one place | yes: `compose_with` builds `WriterConfig::default()` and `ReconcilerConfig::new(paths.root)`; nothing outside tests overrides a field |
| every `RequestKind` has a mutation or a clear error | all 25 have a mutation (`sweep_every_request_kind_has_a_mutation`); none is "explicitly unsupported" |
| failpoint names | `writer.after_admit`, `writer.after_tree_build`, `writer.in_ref_transaction`, `writer.after_cas_before_journal`, `writer.after_journal_before_ff`, `reconcile.mid_effect` all exist; this task adds `reconcile.mid_effect.<effect kind>` |
| `docs/verification-matrix.md` rows match test names | yes: every test name cited in the matrix exists as a `fn` in `src/` or `tests/` (the remaining backticked identifiers are field and variable names) |
| unwired config values | none found: `graph.toml` `defaults` and `summarizer_seat`, role-derived `summaries`, harness profiles (start and resume), launch env at all three create levels, and the capability switch are each exercised by a test above |

## Gaps fixed inline

| file | fix |
|---|---|
| `src/cli/mod.rs` | removed dead `not_implemented` and the stale `stub_commands_report_not_implemented` test (it failed in `cargo test --lib` and blocked on stdin) |
| `src/reconcile/mod.rs` | added a per-kind failpoint `reconcile.mid_effect.<kind>` next to `reconcile.mid_effect`. The generic point fires after the first effect of any kind, and effect order inside an op is by hash, so the crash-after-CreateTab test died on a different effect in about 45% of runs |
| `tests/daemon_composed.rs` | `crash_mid_effect_executes_at_most_once` arms `reconcile.mid_effect.create_tab` and waits for phase-1 effects to settle before it shuts the first daemon down |
| `src/templates/tests.rs` | `document_roundtrip_and_diff_paths` sorts its three member ids: `diff` orders by id and ULIDs minted in one millisecond are not ordered, so the test failed in about 1 of 3 runs |

## Blockers to file

None: nothing larger than a small fix surfaced. Known product defects D1-D3 from `docs/verification-matrix.md`
("Known defects") are unchanged by this task and remain open.

## Observations

- Closing the last pane of a seat's tab is reported by Herdr as a pane closure; the graph retires the clone, then the
  seat, and records one `closure_cascade` act ("tab closure retired 1 seat, 1 clone"), which undo restores in a new tab.
- A Herdr event that arrives while the observer is between subscriptions is recovered only by the 60 s periodic
  snapshot diff (`wiring::sweep_action_and_effect_envelopes_round_trip` drops the stream to force the resync instead of
  waiting). That is by design (events are hints) and matches the matrix row for event loss.
