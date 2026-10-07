# Graph state on disk

Graph state lives in a dedicated Git repository for each graph instance, separate from the application's source repository. It contains human-readable TOML records and Markdown instructions, plus a Git-ignored directory for operational state.

New records use this layout:

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
│   ├── AGENTS.md                  # reusable instructions
│   └── members/<member-name>/AGENTS.md  # team specialization
├── applications/<application-id>.toml
├── transcripts/<team-name>-<team-id>/<seat-name>-<seat-id>/<transcript-id>.toml
├── mutations/
│   ├── transcript-processing/<request-id>.toml
│   ├── undoable-actions/<yyyy-mm>/<action-id>.toml
│   └── graph-changes/<yyyy-mm>/<operation-id>.toml
├── rules/
└── .graph-local/                 # Git-ignored
```

The Git-tracked state includes:

| Part | What it stores |
|---|---|
| Instance | Instance identity, schema version, default harness/model/arguments/summary eligibility, designated summarizer seat |
| Teamspaces | Identity, name history, lifecycle, project-repository reference, Herdr binding and system channel |
| Seats | Identity, system duty, lifecycle, template provenance, application membership, configuration overrides, seat-wide thread participation and runtime binding |
| Clones | Pane binding, current occupant, native-session history, individual thread opt-outs, invitations and reload status |
| Templates/applications | Team and seat definitions (`kind = "team"` or `"seat"`), typed member `seat_template` references and responsibility; each application's member-to-seat mapping, additions, exclusions, reuse and relationship contributions |
| Instructions/rules | Markdown and instruction sections that agents interpret |
| Transcript tracking | Native transcript path and immutable optional capture attribution (teamspace ID and team/seat display names), successfully covered byte ranges, gaps and unresolved input |
| Processing requests | Explicit input range, delivery/dispatch/completion status and result reference; overlapping pending ranges can merge before delivery |
| Actions/operations | Mutation history, affected objects, before/after information, approval metadata and compensation data for undo |

Names organize the folders; stable IDs identify the objects. Renaming teamspace/seat folders can move them without changing identity. Transcript index folders use safe name slugs and full stable IDs captured at registration and stay at that location through rename, observed move, retirement and undo. Retirement preserves records and history.

The local operational state in `.graph-local/` includes the SQLite journal—queued operations, reconciliation effects, retries and notices—along with saved plans, observation baselines, thread intents, daemon locks/socket/logs and checkout status. For long instance paths, the daemon socket instead lives at `/private/tmp/herdr-graph-<uid>/<hash16>.sock`. The journal is durable recovery state even though Git ignores it.

Native conversation contents remain in their harness transcript files; graph records their paths and processing coverage. Actual thread messages and receipts live in herdr-threads, and project tasks live in Beads/Dolt.

The concrete layout is defined in [layout.rs](src/store/layout.rs); local runtime paths are in [config.rs](src/config.rs).

## Existing instances and compatibility

The upgraded binary reads both the new paths above and legacy `transcripts/<seat-id>/<transcript-id>.toml`, `requests/<request-id>.toml`, `actions/<yyyy-mm>/<action-id>.toml` and `operations/<yyyy-mm>/<operation-id>.toml`. Existing records are updated at their discovered paths, preserving old undo compensation paths. New records use the new layout. No eager migration, native transcript copy or native history rewrite occurs. Legacy transcripts without `capture_attribution` keep that absence unknown; current names are not substituted for historical names.

Targeted ID lookup parses only matching ID filenames across supported locations and rejects corrupt payloads, filename/body ID mismatches and duplicate matches. Full enumeration parses all supported records and reports corrupt or duplicate history. Supported depths are one legacy or two current transcript directories and one month directory for action/operation records; arbitrary nesting is not discovered. Directory discovery still scales with directory count; no large-history latency bound is claimed.

Templates with omitted `kind` remain team templates. Legacy `role` fields read as `system_duty`, including old undo documents; unused `role_ref` is ignored and no longer written. Instructions remain readable, but edits to reusable template or actual member `AGENTS.md` must use reviewed `template edit` so revisions, runtime effects and undo remain consistent.

Keep using the upgraded binary for these instances. Older binaries cannot discover new-layout records, and no downgrade migration is supplied. Git graph-change summaries contain committed mutations; the SQLite journal remains authoritative for live, rejected, failed and cancelled operation lifecycle. Preserve both the Git repository and local journal when backing up or moving an instance.
