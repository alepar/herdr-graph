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

## Reusable templates and state paths

Team templates reference reusable seat definitions through member `seat_template` IDs and `responsibility`. `kind = "team"` is the legacy default; `kind = "seat"` supplies reusable instructions/runtime defaults. Independent applications create independent seats; explicit mappings reuse an existing seat. Runtime precedence is seat override → team member → seat template → team template → graph defaults → built-ins. `/seat` exposes live instruction references, application specialization, seat context and scoped rules.

Operational duties use `system_duty`; old `role` fields and `--role` still read, and unused `role_ref` is ignored. Edit reusable/member instructions with reviewed `plan template edit … --from …`, whose `agents_md` fields retain omitted text and clear on empty strings. Direct content writes to those instruction locations are rejected. Live changes preview immediate runtime effects and support undo.

New transcript indexes use `transcripts/<team-name>-<team-id>/<seat-name>-<seat-id>/<transcript-id>.toml`. Registration names and capture attribution remain historical through rename/move/retirement/undo. New bookkeeping records use `mutations/graph-changes/`, `mutations/undoable-actions/` and `mutations/transcript-processing/`. The upgraded binary reads legacy paths and updates old records in place; there is no eager migration or native transcript rewrite. Older binaries cannot discover new-layout records. See [GRAPH_STATE.md](GRAPH_STATE.md) for compatibility and [the release handoff](docs/releases/2026-10-07-agreed-fixes-handoff.md) for validation and release ownership.

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

herdr-graph consumes herdr-threads only through its public client API (`third_party/herdr-threads`). Summarizer requests use service-ACK delivery (herdr-threads epic `ht-5nb`, on herdr-threads main since 84de563d): graph registers `service_session_v2`, posts an ACK-required request on the summarizer seat channel, and reads its receipts; an ACK means the summarizer received (dispatched) the request, never that it was processed. If a compatible daemon refuses v2 registration as unsupported, graph falls back to Notify plus `herdr-graph request ack`. The cargo feature `threads-service-ack` is on by default (`--no-default-features` forces the fallback).

The verified dependency is herdr-threads **0.2.9**, release commit `223b61a88625d7f442d22d9b8728dc4b2282b15f` (`v0.2.9`), using wire protocol **6**. [Official release](https://github.com/alepar/herdr-threads/releases/tag/v0.2.9). Rebuild graph when upgrading the threads checkout, and restart the graph daemon to load that binary; a binary built against an older wire protocol cannot communicate with the upgraded threads daemon. Pane lookup excludes retired threads seats. A `not_required` receipt represents a waived ACK obligation and never marks a summarizer request as dispatched. Graph-managed channels are exempt from threads' automatic quiet-channel archival.

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

The repository currently tracks an absolute symlink to an external checkout. Source builds require that checkout or an equivalent local checkout at the verified release commit; point the symlink at it before building. Release packaging/dependency setup belongs to the release owner.

```sh
ln -sfn <path-to-herdr-threads-v0.2.9-checkout> third_party/herdr-threads
```

## Test tiers

| Tier | Command |
| --- | --- |
| 1-2 unit and integration | `cargo test` |
| crash-injection | `cargo test --features test-support` |
| 3 private Herdr server | `cargo test --features private-herdr -- --test-threads=1` |
| 4 real agents | `HG_REAL_AGENTS=1 cargo test --features private-herdr --test config_smoke --test real_agent_smoke -- --test-threads=1 --nocapture` |
| 5 real threads | `HG_REAL_THREADS=1 cargo test --features private-herdr --test threads_real -- --test-threads=1 --nocapture` |

All tiers use private servers and isolated state. The release-build packaging test is ignored by default: `cargo test --test packaging -- --ignored build_script_places_binary`.

Test isolation: every test that spawns a process (Herdr, the daemon, the CLI) goes through `tests/support/isolated.rs` (`TestRoot`) or `PrivateHerdr`. Each gets its own temp root with HOME, the XDG dirs, `CLAUDE_CONFIG_DIR`, the Herdr plugin dirs, the Herdr socket and `HERDR_GRAPH_INSTANCE` all inside it, and `HG_TEST_ROOT=<root>` as a marker. A child that resolves a path outside its root exits 97 and fails the test; an in-process test that resolves the real `~/.config/herdr-graph`, `~/.claude` or the live Herdr socket panics. When a test ends (pass, fail, panic, timeout) its children are signalled and every process still carrying the marker is swept. `tests/test_isolation.rs` pins all of this.

## Verification matrix

[docs/verification-matrix.md](docs/verification-matrix.md) records the October 7 integration and dated historical evidence (private Herdr 0.9.1, macOS aarch64). The [release handoff](docs/releases/2026-10-07-agreed-fixes-handoff.md) contains the current sequential validation counts and compatibility limits.

`verified-real` means exercised against private Herdr or an isolated herdr-threads daemon. `verified-fake` means covered with fakes or a real Git store and journal. `assumed` marks behavior that remains unexercised or skipped.

Run tier 3 serially: `cargo test --features private-herdr -- --test-threads=1`. The seat-rename and undo-adoption regressions wait for the derived view and pane token respectively. Historical defects D1–D3 have been fixed; their regressions run in the normal suite.

Real-agent launch/resume, SessionStart bootstrap, and the agent summarizer flow still require the opt-in tier and credentials. Service-ACK delivery is exercised against an isolated 0.2.9 threads daemon with synthetic public-hook registration and cooperative CLI acceptance/ACK; real summarizer-agent delivery is separate. Herdr live handoff remains untested.

Run feature tiers sequentially: another Cargo feature build can replace `target/debug/herdr-graph` while subprocess tests are using it.

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
