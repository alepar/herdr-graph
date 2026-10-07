# Graph state on disk

Graph state lives in a dedicated Git repository for each graph instance, separate from the application's source repository. It contains human-readable TOML records and Markdown instructions, plus a Git-ignored directory for operational state.

The layout is roughly:

```text
<graph-instance>/
├── graph.toml
├── teamspaces/<team-name>/
│   ├── teamspace.toml
│   ├── rules/
│   ├── seats/<seat-name>/
│   │   ├── seat.toml
│   │   ├── AGENTS.md
│   │   └── clones/<clone-name>/clone.toml
│   └── archive/seats/<name-id>/
├── archive/teamspaces/<name-id>/
├── templates/<template-name>/
│   ├── template.toml
│   └── members/<member-name>/AGENTS.md
├── applications/<application-id>.toml
├── transcripts/<seat-id>/<transcript-id>.toml
├── requests/<request-id>.toml
├── actions/<yyyy-mm>/<action-id>.toml
├── operations/<yyyy-mm>/<operation-id>.toml
├── rules/
└── .graph-local/                 # Git-ignored
```

The Git-tracked state includes:

| Part | What it stores |
|---|---|
| Instance | Instance identity, schema version, default harness/model, designated summarizer seat |
| Teamspaces | Identity, name history, lifecycle, project-repository reference, Herdr binding and system channel |
| Seats | Identity, role, lifecycle, template provenance, application membership, configuration overrides, seat-wide thread participation and runtime binding |
| Clones | Pane binding, current occupant, native-session history, individual thread opt-outs, invitations and reload status |
| Templates/applications | Reusable declarations and instructions; each application's member-to-seat mapping, additions, exclusions, reuse and relationship contributions |
| Instructions/rules | Markdown and instruction sections that agents interpret |
| Transcript tracking | Native transcript path and attribution, successfully covered byte ranges, gaps and unresolved input |
| Processing requests | Explicit input range, delivery/dispatch/completion status and result reference; overlapping pending ranges can merge before delivery |
| Actions/operations | Mutation history, affected objects, before/after information, approval metadata and compensation data for undo |

Names organize the folders; stable IDs identify the objects. Renaming can move a folder without changing its identity. Retirement preserves records and history.

The local operational state in `.graph-local/` includes the SQLite journal—queued operations, reconciliation effects, retries and notices—along with saved plans, observation baselines, thread intents, daemon locks/socket/logs and checkout status. For long instance paths, the daemon socket instead lives at `/private/tmp/herdr-graph-<uid>/<hash16>.sock`. The journal is durable recovery state even though Git ignores it.

Native conversation contents remain in their harness transcript files; graph records their paths and processing coverage. Actual thread messages and receipts live in herdr-threads, and project tasks live in Beads/Dolt.

The concrete layout is defined in [layout.rs](src/store/layout.rs); local runtime paths are in [config.rs](src/config.rs).
