# herdr-graph

A Herdr plugin that keeps a durable, Git-backed organization graph of human and agent collaborators and applies it to Herdr: **teamspace = workspace, seat = tab, clone = pane**.

- The instance is a Git repository of records (teamspaces, seats, clones, templates, requests, transcripts).
- Changes go through plan / confirm / apply: a plan is computed and hashed, and only a confirmed hash is applied. Every applied plan can be undone.
- Seats talk through herdr-threads channels; transcript requests are recorded in the graph.

## Install

```sh
scripts/build.sh                 # cargo build --release, copies to bin/herdr-graph
herdr plugin link <path-to-this-repo>
```

Never run tests against your live Herdr session: all tests use private servers and isolated state. Linking the plugin into your real Herdr is a manual, deliberate step.

## Create an instance

```sh
herdr-graph init <path> [--with-examples] [--no-user-config]
```

The instance is located by, in order: `HERDR_GRAPH_INSTANCE`, the plugin config dir, then `~/.config/herdr-graph/config.toml`. `init` prints where it wrote the user config (or that it left one alone); `--no-user-config` writes nothing under `$HOME`, so probes can create an instance and point at it with `HERDR_GRAPH_INSTANCE`.

## Claude setup

```sh
herdr-graph setup claude [--uninstall]
```

Installs the SessionStart hook and the `/seat` and `graph` skills. Herdr manifests do not declare Claude skills or hooks, so this command does it.

## /seat

`/seat` (in a Claude session started by herdr-graph) shows or sets the session's seat identity. The `graph` skill documents the CLI for agents.

## Plan, confirm, relay

```sh
herdr-graph plan ... --json
herdr-graph apply <pl> --confirm <hash> --confirmed-by user-relay
```

An agent proposes a plan and relays the hash to the user; the user confirms. There is no bypass, and a non-TTY invocation never applies.

## Undo

```sh
herdr-graph undo
```

## Ops

`herdr-graph ops` lists operations; `cancel`, `reassign`, `check-instruction` and `--supersedes` manage them.

## Exit codes

| Code | Meaning |
| ---- | ------- |
| 0 | success |
| 1 | error, rejection or failure |
| 2 | no instance configured (also usage errors of `request complete`) |
| 3 | still running: the change was admitted and finishes in the background (`herdr-graph op <id>`) |

The daemon answers every CLI request within `CALL_TIMEOUT − REPLY_MARGIN` (27 s), so a slow writer yields exit code 3 and the op id instead of a transport timeout.

## Threads integration and amendment status

herdr-graph consumes herdr-threads only through its public client API (`third_party/herdr-threads`). The threads amendment epic `ht-5nb` was accepted 2026-10-02 but has not landed. Until it does, delivery uses the Notify fallback: ACK lives in graph, not threads. The cargo feature `threads-service-ack` switches to service ACK once it lands.

### herdr-threads discovery

The daemon finds the herdr-threads state directory on every call (threads may be installed or started after graph), using the same rules herdr-threads itself uses. First hit wins:

1. `HERDR_GRAPH_THREADS_STATE_DIR` (absolute path).
2. `threads_state_dir = "/abs/path"` in `~/.config/herdr-graph/config.toml` (or the `config.toml` of the plugin config dir; the plugin config dir is read first).
3. The `herdr-threads` sibling of herdr-graph's own plugin state dir (`HERDR_PLUGIN_STATE_DIR` ending in `herdr-graph`, e.g. `<root>/herdr/plugins/herdr-threads`).
4. `$XDG_STATE_HOME/herdr/plugins/herdr-threads`, if it exists.
5. `~/.local/state/herdr/plugins/herdr-threads`, if it exists.

If both 4 and 5 exist and differ, discovery refuses to guess: set `threads_state_dir`. Relative paths are ignored. The daemon socket and instance inside the state directory are derived from the Herdr socket, as herdr-threads does. To skip discovery entirely, set `HERDR_GRAPH_THREADS_SOCKET` and `HERDR_GRAPH_THREADS_INSTANCE` (the daemon socket and the instance UUID).

`herdr-graph doctor` prints a `threads` line: `state dir <path> (<where it was found>)` or `not found (<reason>)`, followed by whether the running daemon is connected to threads (its capability, or the error). A missing herdr-threads install is reported as `[WARN]` and does not fail doctor: graph works without threads, degraded (no channels, notifications or summarizer delivery). The check fails when the state dir is found but the running daemon reports it is not connected, or when discovery is ambiguous.

### Re-pointing `third_party/herdr-threads`

```sh
ln -sfn <path-to-herdr-threads> third_party/herdr-threads
```

## Test tiers

| Tier | Command |
| --- | --- |
| 1-2 unit and integration | `cargo test` |
| crash-injection | `cargo test --features test-support` |
| 3 private Herdr server | `cargo test --features private-herdr` |
| 4 real agents | `HG_REAL_AGENTS=1 cargo test` |
| 5 real threads | `HG_REAL_THREADS=1 cargo test` |

All tiers use private servers and isolated state. The release-build packaging test is ignored by default: `cargo test --test packaging -- --ignored build_script_places_binary`.

Test isolation: every test that spawns a process (Herdr, the daemon, the CLI) goes through `tests/support/isolated.rs` (`TestRoot`) or `PrivateHerdr`. Each gets its own temp root with HOME, the XDG dirs, `CLAUDE_CONFIG_DIR`, the Herdr plugin dirs, the Herdr socket and `HERDR_GRAPH_INSTANCE` all inside it, and `HG_TEST_ROOT=<root>` as a marker. A child that resolves a path outside its root exits 97 and fails the test; an in-process test that resolves the real `~/.config/herdr-graph`, `~/.claude` or the live Herdr socket panics. When a test ends (pass, fail, panic, timeout) its children are signalled and every process still carrying the marker is swept. `tests/test_isolation.rs` pins all of this.

## Verification matrix

What is checked against what, from [docs/verification-matrix.md](docs/verification-matrix.md) (herdr 0.9.1, 2026-10-02). Of 59 rows:

| Status | Rows | Meaning |
| --- | --- | --- |
| `verified-real` | 37 | ran against a private real Herdr (tier 3), or a real herdr-threads daemon (tier 5), and passed |
| `verified-fake` | 13 | covered by tiers 1-2 with fakes or a real git store; the unit test is cited |
| `assumed` | 9 | not exercised, skipped, or exercised and found broken |

Run the tier-3 suite serially: `cargo test --features private-herdr --test e2e_private_herdr -- --test-threads=1` (25 flows, about two minutes; `e2e_plugin_link_status_action_single_daemon` runs a release build).

The `assumed` rows:

- Undo run from a pane adopts that pane as the restored clone: **product defect D1**, the undo commits but the caller's pane is never bound (`#[ignore]`d repro `e2e_undo_from_pane_adopts_caller_pane`).
- Token re-stamp after `clone rebind`: **product defect D2** (`e2e_rebind_restamps_token_after_herdr_restart`).
- Moving a pane into another seat's tab leaves that seat alone: **product defect D3**, the tab is renamed and the other seat with it (`e2e_move_pane_into_other_seat_tab_keeps_that_seats_name`).
- Service-ack delivery: waits for the herdr-threads epic ht-5nb (fallback delivery is verified).
- `claude` and `codex` harness configurations, and the real-agent summarizer flow: skipped without `HG_REAL_AGENTS=1`, the agent binary and an explicit API key.
- `agent_session` of an `agent.start`ed Claude without Herdr's integration (spike 5), and Herdr live handoff: never run.

Reproduce a defect with `cargo test --features private-herdr --test e2e_private_herdr -- --ignored <name> --test-threads=1`.

## Design documents

Design and product exploration for a durable organization of human and agent collaborators inside Herdr.

## Two products

1. [herdr-graph vision](HERDR-GRAPH-VISION.md): the foundation—persistent responsibilities, native Herdr presence, relationships, templates, instructions, and recoverable organizational state.
2. [Software factory vision](SOFTWARE-FACTORY-VISION.md): a concrete product built on that foundation—project teams, temporary feature collaborators, rulebooks, and evidence-based delivery. Its name is still open.

Both are draft product visions, informed by the existing design and external research. They distinguish settled direction from new proposals and do not assert implementation readiness.

## Supporting material

- [Firstmate comparison](research/firstmate-20260928/COMPARISON.md): pinned documentation/code audit, overlap with both visions, and the case for adopting, extending, or building. [HTML](research/firstmate-20260928/COMPARISON.html) · [PDF](research/firstmate-20260928/COMPARISON.pdf)
- [Firstmate operator sentiment](research/firstmate-20260928/SENTIMENT.md): firsthand praise, operational complaints, fixes, and policy disagreements, with a deduplicated sample. [HTML](research/firstmate-20260928/SENTIMENT.html) · [PDF](research/firstmate-20260928/SENTIMENT.pdf)
- [Research synthesis](research/software-factories-20260928/RESEARCH.md): factory designs, operator experience, implications, and 21 cited sources. [HTML](research/software-factories-20260928/RESEARCH.html) · [PDF](research/software-factories-20260928/RESEARCH.pdf)
- [Landscape evidence](research/software-factories-20260928/landscape-evidence.md) and [operator sentiment](research/software-factories-20260928/sentiment-evidence.md).
- [Designer consultation](research/software-factories-20260928/designer-consultation.md) and [review notes](research/software-factories-20260928/validation-review.md).
- [Design notes](DESIGN-NOTES.md): decisions and explicit supersessions from the adjacent designer session.
- [Current design and continuation](CURRENT-DESIGN.md). Historical [resume](archive/RESUME.md), [original seed](archive/HANDOFF.md), and [Herdr identity qualification](archive/HERDR-IDENTITY-CHECK.md).

The deep-research skill is present in the default Codex profile and all three AISW Codex profiles. [Installation record](research/software-factories-20260928/skill-installation.json) includes destinations and matching file hashes. Research sources, evidence, claims, and validation logs are kept alongside the synthesis.

## License

MIT OR Apache-2.0, at your option: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).
