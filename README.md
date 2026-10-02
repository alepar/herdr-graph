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
herdr-graph init <path> [--with-examples]
```

The instance is located by, in order: `HERDR_GRAPH_INSTANCE`, the plugin config dir, then `~/.config/herdr-graph/config.toml`.

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

## Threads integration and amendment status

herdr-graph consumes herdr-threads only through its public client API (`third_party/herdr-threads`). The threads amendment epic `ht-5nb` was accepted 2026-10-02 but has not landed. Until it does, delivery uses the Notify fallback: ACK lives in graph, not threads. The cargo feature `threads-service-ack` switches to service ACK once it lands.

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

## Verification matrix

Filled by hg-zmi.19 from [docs/verification-matrix.md](docs/verification-matrix.md).

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
