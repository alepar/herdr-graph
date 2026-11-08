# Survey: agent artifacts, Beads tracking, per-seat sessions and transcripts

Date: 2026-11-08. This is a research report only. It changes no code and files no beads.

## Summary

- **Artifacts and Beads.** Only one idea shipped: a label convention.
  - `herdr-graph seat [--json]` prints the labels `hg-ts:`, `hg-seat:`, `hg-clone:` and `hg-ns:`, plus the query `bd list --label hg-seat:<seat>`.
  - Graph never calls `bd`. Nothing applies or checks the labels; that is left to agent practice.
- **The original seed was never built.** It called for seat history by bead epics, artifact-type, visibility and milestone labels, and evidence on closed beads. All of it got only a mention. The 2026-10-02 decision moved it out of graph and into Beads practice, and nothing ever defined how that practice works.
- **Sessions and transcripts.**
  - Yes, graph tracks these automatically, per clone rather than per seat.
  - Claude: graph gets the native session id and the transcript path, from the SessionStart `session-report` hook or else from Herdr's `agent_session`.
  - Codex: graph gets the id only, from Herdr or from `resume <id>` argv. Its transcript stays unresolved, so Codex seats never get summaries.
  - Graph stores pointers and coverage indexes, never copies of the conversation files.
  - The native id and path appear only in the raw `show <clone>` TOML. `who` and `seat --json` show graph's own `ns_` id.

## Part 1: artifacts, work products, Beads

| # | Idea | Status | Evidence |
|---|---|---|---|
| 1 | Beads carry graph identity labels (`hg-ts/seat/clone/ns`); a successor finds work it inherited by filtering on them (F14) | **Implemented as a convention.** Graph prints the labels and the query. Agents apply them. | `src/bootstrap/mod.rs:355-368`, rendered at `:588-589`. Tests: `src/bootstrap/tests.rs:743-763`, `:802`. Skills: `skills/seat/SKILL.md:58-61`, `skills/graph/SKILL.md:102-113`. Spec: MVP design §9 (`docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-design.md:231-236`). Bead: hg-zmi.13 (closed). Decision: `DESIGN-NOTES.md:375` |
| 2 | Graph never mutates Beads; creating or retiring a seat touches no beads | **Implemented, by building nothing.** No `bd` invocation exists in `src/`. | `DESIGN-NOTES.md:373,376`, `CURRENT-DESIGN.md:10` |
| 3 | One graph repo holds a shared Beads DB, with Beads writes coordinated against graph Git commits | **Dropped.** Dolt is orthogonal to graph's Git. | `archive/RESUME.md:24`, `DESIGN-NOTES.md:27,134`, resolved at `:372` |
| 4 | Several contributors per bead; an exact label format | **Mentioned, left open** | `DESIGN-NOTES.md:31,376` |
| 5 | Seat history indexed by bead epics | **Mentioned only.** Graph has no epic concept. | Seed: `archive/HANDOFF.md:33`. `DESIGN-NOTES.md:228` |
| 6 | Epic labels for executor, artifact type, external or internal visibility, and user-accepted milestones | **Mentioned only.** Later assigned to Beads practice, but no format was ever defined. | `archive/HANDOFF.md:35`, `DESIGN-NOTES.md:228,374`, `CURRENT-DESIGN.md:10` |
| 7 | A closed bead points to evidence; completion stays distinct from user acceptance | **Principle only.** Nothing records evidence or acceptance. | `archive/HANDOFF.md:36`, `DESIGN-NOTES.md:105,228`, `CURRENT-DESIGN.md:9`, `HERDR-GRAPH-VISION.md:88`, `SOFTWARE-FACTORY-VISION.md:97,99` |
| 8 | A history brief at `/seat` built from Beads, wiki and transcript references | **Partly implemented.** `/seat` gives the beads query and the seat and clone paths, but no digest. | `src/bootstrap/mod.rs:326-412`. Idea: `DESIGN-NOTES.md:234`, `SOFTWARE-FACTORY-VISION.md:75` |
| 9 | Bead attribution down to native session and transcript | **Partly implemented.** The `hg-ns:` label exists, and transcript attribution lives only on graph's `TranscriptRecord`. | `src/bootstrap/mod.rs:360-361`, `src/model/transcript.rs:9-38`. Idea: `DESIGN-NOTES.md:16,149` |
| 10 | Completion records with an output reference | **Implemented, but only for transcript summarization**, not for agent work | `RequestResult{output_ref,covered_range,reported_by,at}` at `src/model/request.rs:56-61`. Command: `request complete <rq> --output … / --summary-from … --covered a-b` (`src/cli/request.rs:36-51`). Beads: hg-45e, hg-45e.8 |
| 11 | A factory repo linking Beads, repos, evidence and threads; recipes that end in outcomes carrying evidence; "cost per accepted outcome" | **Mostly vision.** `templates/software-factory/` shipped in a058b10. It covers evidence and acceptance in prose (`templates/software-factory/AGENTS.md:24-31`), but not Beads labels, milestones or evidence pointers. | `SOFTWARE-FACTORY-VISION.md:35,59,71-77,129,145,151`. Templates mention "evidence" in prose only (`templates/researcher/AGENTS.md:3`, `templates/reviewer/AGENTS.md:3`). `CREATING_NEW_GRAPH.md:66,97` |

Other hits are noise. In `src/undo` and `src/threads/reconnect.rs`, "artifact" means preservation JSON. `evidence/hg-66u.*` holds test evidence from the repo's own development. `bd search` finds nothing for epic, milestone, deliverable, outcome or factory, and no open bead tracks any idea above.

## Part 2: native session ids and transcripts

**Data model.** Sessions belong to clones. A seat reaches its sessions only through its clones.

- `CloneRecord.sessions: Vec<NativeSession>` and `occupant` are at `src/model/clone.rs:25,27`.
- `NativeSession{id: ns_, harness, native_session_id, transcript_path?, transcript?, cwd, started, ended, end_reason}` is at `src/model/native_session.rs:9-34`.
- `TranscriptRecord` (`tr_`) lives at `transcripts/<team>/<seat>/<tr>.toml` and holds a path, coverage and gaps (`src/model/transcript.rs:1-31`). Its doc comment says "native conversation files are never copied".

**Triggers.** Two paths, both automatic.

1. **Observation polling.** `occupancy()` (`src/observe/diff.rs:102`) calls `capture_session` (`src/observe/sessions.rs:22`), which tries each harness profile's `session_id_sources` in order:
   - Claude: `GraphHookReport` first, then `HerdrAgentSession`. The transcript is found by id at `~/.claude/projects/<slug(cwd)>/<id>.jsonl`, with a fallback glob `projects/*/<id>.jsonl` (`src/model/harness.rs:83-87,152`; `sessions.rs:82-93`).
   - Codex: `HerdrAgentSession` first, then `ProcessInfoArgv{marker:"resume"}`, with `TranscriptLocator::HookReportedOrUnresolved`. That locator always maps to `None` (`harness.rs:99-103`; `sessions.rs:63`). Nothing reads `~/.codex/sessions`.
   - A changed id produces `OccupancyEnded{SessionChanged}` plus `OccupancyStarted` (`diff.rs:138-148`). That outcome appends a new `NativeSession` (`src/observe/mutations.rs:652-697`).
2. **The Claude SessionStart hook.**
   - `setup claude` installs `seat --hook-prompt; session-report --from-hook claude` (`src/bootstrap/setup_claude.rs:168-173`). `setup codex` installs only `seat --hook-prompt --codex` (`:175-180`).
   - The receiver (`src/cli/session.rs:41-84`) reads `{session_id, transcript_path, source, cwd}`. It does nothing outside a graph clone or pane.
   - It calls daemon `session.report` with a 1 s timeout and spools to `.graph-local/session-spool/` on failure (`src/transcripts/capture.rs:30-105`).

Graph never scrapes pane scrollback for ids.

**Summarizer.**

- When a session ends, graph creates a `tr_` record and an `rq_` processing request, but only if the source seat's effective `summaries` is true (`src/transcripts/mod.rs:235-262`). The default is true for ordinary seats and false for system-duty seats (`src/model/common.rs:199-203`).
- With no path (always the case for Codex), the record is marked `transcript_unresolved` and no request is created (`src/transcripts/mutations.rs:359-370`).
- The watcher (every 30 s) requests only the bytes appended after a resume. A replaced inode starts a new `tr_` (`src/transcripts/watcher.rs`).
- Liveness (every 10 min) redelivers, sends reminders and retries (`src/transcripts/liveness.rs`).
- The summarizer receives a threads message carrying the native path and byte range (`src/transcripts/delivery.rs:74-83`). It reads the `.jsonl` itself and completes into `<seat>/summaries/<tr>-<start>-<end>.md`.

**What the CLI shows.**

| Command | Session information |
|---|---|
| `who` | `teamspace/seat/clone/<ns_>`: graph's id, not the native one (`src/threads/mod.rs:285-301`) |
| `seat --json` | only `labels.ns = "hg-ns:<ns_>"` (`src/bootstrap/mod.rs:360-362`) |
| `show <clone>` / `show <tr_>` | the raw TOML, including `native_session_id`, `transcript_path`, `cwd`, start and end (`src/bootstrap/show.rs:142-170`) |
| `request list` | rq, status, tr, range, destination. No path (`src/transcripts/requests.rs:181-211`) |
| `status` | nothing about sessions |

**Coverage and misses.**

- **Covered:**
  - `/clear` ends the old `ns_` with `SessionChanged` and creates a request.
  - Resume reuses the last session and gets a new `ns_`; the watcher covers the appended bytes.
  - Claude without hooks still falls back to Herdr's `agent_session`.
  - An agent started by hand inside a graph pane is picked up.
- **Missed:**
  - Codex transcripts. The only hint is a "MVP deferral" comment at `harness.rs:45`, and no doc states this.
  - Agents outside graph panes (`diff.rs:107`; `capture.rs:366-370`).
  - Subagent transcripts.
  - The Claude glob fallback takes the first match in sort order when the cwd slug is wrong.

**Tests that prove it:**

- Capture: `src/observe/tests.rs` `capture_claude_agent_session_id_and_transcript_path` :2681, `capture_claude_glob_fallback` :2713, `capture_codex_agent_session` :2748, `capture_codex_argv_resume_id` :2775.
- Transcripts: `src/transcripts/tests.rs` `session_end_creates_request_for_uncovered_range` :628, `codex_without_a_path_leaves_the_transcript_unresolved` :890, `resume_append_creates_request_for_new_bytes_only` :994, `replaced_transcript_file_starts_a_new_record` :1039, `clear_session_change_ends_previous_ns_and_requests` :1063, `spooled_report_is_ingested_and_removed` :1315.
- Integration: `tests/daemon_composed.rs:684` `session_end_flows_to_transcripts`, `tests/daemon_ipc.rs:369`, `tests/real_agent_smoke.rs:160`, `tests/e2e_private_herdr.rs:1683` `e2e_summary_lands_in_archived_seat_folder`.

Code and tests settle all of this, so no private-rig run was needed.

**Doc drift.**

- `CURRENT-DESIGN.md:9` promises attribution down to native session and transcript. That holds only in the raw records.
- `src/transcripts/delivery.rs:4-5` calls the notify fallback "(default)", but `Cargo.toml:17` enables `threads-service-ack` by default.

## Overlap with in-flight designs

- **onboarding-design** (`docs/superpowers/specs/2026-11-07-onboarding-existing-spaces-design.md`)
  - The handover step has new seats label beads `hg-seat:<st>` by hand (`:28`).
  - Old agents' unlabeled beads get no help with relabelling.
  - Resuming an old conversation works only through an `--args "--resume <id>"` override (`:39`).
  - Adoption relies on occupancy capture (`:86`), so an adopted Codex agent gets no summaries.
- **inspection-design** (`docs/superpowers/runs/2026-11-07-graph-state-inspection/2026-11-07-graph-state-inspection-design.md`)
  - It already inventories occupant and `sessions[]` and the hook path (`:24,77-84`), and notes that `Requester.native_session` is never set (`:109`).
  - It says nothing about Beads. Its seat roster would be the natural home for per-seat open beads and for native ids and paths. No work duplicates this survey's scope.
- **factory-template**
  - The template shipped in a058b10 with prose on evidence and acceptance only. It is the place where ideas 6, 7 and 11 become conventions: milestone and artifact labels, evidence pointers on closed beads, and acceptance recorded separately from closure.

## Gaps worth turning into work (not filed)

1. **Codex transcript resolution.** Locate rollouts under `~/.codex/sessions` by session id, so Codex seats get summaries. Until then, say in the README that Codex is id-only.
2. **Expose native session information.** Add the native id and transcript path to `who` and `seat --json`, or to an inspection roster, rather than only to raw `show` TOML. Coordinate with inspection-design.
3. **Make the Beads label convention concrete.** Agents apply it inconsistently, if at all. Options:
   - a `seat`-printed `bd create` snippet carrying the labels;
   - or a check that reports open beads per seat.

   Either way graph stays read-only towards Beads.
4. **Define the work-product convention the seed asked for:**
   - artifact-type, visibility and milestone labels;
   - an evidence pointer on close;
   - user acceptance kept distinct from closure.

   This belongs in skills or templates (factory-template), not graph code, consistent with `DESIGN-NOTES.md:374`.
5. **A seat history brief at `/seat`.** List the seat's recent closed beads and summary files as references, without importing them.
6. **A relabelling aid for onboarding.** Help old agents' beads move to the new `hg-seat:` labels during handover.
7. **Housekeeping.**
   - Fix the stale `delivery.rs:4-5` comment.
   - Either set `Requester.native_session` or remove it.
   - Document that subagent transcripts are not tracked.
