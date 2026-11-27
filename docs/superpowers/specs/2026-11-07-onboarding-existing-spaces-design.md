# Onboarding existing Herdr and herdr-threads spaces into herdr-graph

Status: approved by the user on 2026-11-25 ("rest lgtm, feel free to implement"). Written 2026-11-07. Applies to herdr-graph v0.3.0 and later (threads pinned at 3.6.0 since hg-lnw; v0.3.0 shipped with 3.1.0; Herdr 0.9.5).

## Question

"We have existing spaces / tabs in herdr/herdr-threads and want to transition to graph. How can we do it today? What can we do to simplify? And gradually move over? Extract templates/roles/etc from existing spaces?"

## Summary

- **Today** there is no adoption. Graph only manages workspaces, tabs and panes it created itself. Moving over means recreating each space as a graph teamspace and relaunching every agent in a new tab, with a new threads seat. Existing threads memberships, the native conversation and the coordinator relationships do not carry over. Unmanaged spaces keep working side by side, untouched.
- **The gap is small.** v0.3.0 already locates a clone by its threads seat, and threads already returns a pane's existing seat on `ResolveSeat`. Threads' own continuity keeps that seat across agent restarts in the pane, so an adopted seat needs no graph continuity key (G4). What is missing is a mutation that writes graph records for a live workspace, tab and pane instead of creating new ones.
- **Recommendation:** add three reviewed, undoable plan kinds: `teamspace adopt <workspace>`, `seat adopt <tab>` and a matching `release`. Adoption never launches, relaunches or closes anything. It records what is there, takes over the pane's existing threads seat (so every membership is kept) and adds graph's own channels on top. Plus an `adopt` skill, run for each session the user wants seated: an agent reads that session's transcript and threads history and proposes the seat's role, rules and place in the manager hierarchy as ordinary plans. A second skill extracts a team template from one or two adopted spaces. Sessions the user does not adopt stay unseated and graph ignores them.

## 1. Today's path (v0.3.0 as shipped)

Every step below is checked against the code or the CLI help. File references are the source of truth.

1. **Prerequisites.** herdr-threads daemon 3.6.0 or later (graph pins 3.6.0 and registers its service session with no fallback: CHANGELOG Migration). Build, `herdr plugin link`, `herdr-graph init <path>`.
2. **Harness setup.** `herdr-graph setup claude` / `setup codex` installs a user-level SessionStart hook, the `seat` and `graph` skills and the apply ask rule. Safe for unmanaged panes: the hook prompts for `/seat` only when `HERDR_GRAPH=1` or the pane id resolves to a bound live clone (`src/bootstrap/mod.rs` `wants_seat_prompt`). Unmanaged agents see nothing new.
3. **Create a teamspace.** `plan teamspace create <name> --project-repo <repo> --active`. This always opens a **new** Herdr workspace (`runtime.open_workspace`, `src/plan/core_kinds.rs` TeamspaceCreate). There is no flag to bind an existing workspace.
4. **Create seats.** `plan seat create <name> --teamspace <ts> --active` always opens a **new** tab and launches a **new** agent (`runtime.open_tab` plus `open_clone_effects`, SeatCreate). The agent starts with a continuity key and gets a fresh threads seat. Or `application apply <template> --teamspace <ts> --name <app>` to hydrate a team template.
   - The first seat created on a graph becomes the node manager (`first_seat::assignment`). Create the user's top coordinator first, or move the role later with `plan node-manager set`.
   - The clone's cwd is the teamspace `project_repo`, else `<instance>/.graph-local/cwd/<seat>` (`src/model/launch.rs` `resolve_cwd`). A seat cwd override exists in the model but has no CLI flag, so per-worker worktrees cannot be expressed.
5. **Managers.** `plan manager set <seat|teamspace> <manager-seat>` and `plan node-manager set <seat>` map the `main`/`mod` coordinator convention onto graph manager seats.
6. **Instructions.** Per-tab role text, which today lives only in a handoff message or the agent's context, can be recorded with `plan instructions set seat <seat> AGENTS.md --from <file>` or teamspace `rules/<name>.md`. Repo `AGENTS.md`/`CLAUDE.md` keep loading natively from the cwd; do not copy them into graph (they would render twice).
7. **Threads.** The new seats start with no memberships. Self-join channels (psa-global, under the user's global instructions) are rejoined by the agent itself. 1:1 pair channels are new (threads creates the canonical pair channel on first `send --seat`). Topic threads need fresh invitations from an existing member. `plan participation join <thread> --scope seat` records graph's intent for a thread, but graph can only invite through its own service identity.
8. **Hand over work.** Old agents hand open work to the new seats over threads (`herdr-threads handoff`) and the new seats label beads `hg-seat:<st>`. Then close the old tabs by hand.

### Where it breaks or forces a restart

| Step | Effect on running agents |
|---|---|
| Teamspace and seat creation | Every agent is a new process in a new tab. The old conversation is not carried over. |
| Threads seat | New seat per clone. Old memberships, open requests, unACKed receipts and the pair channels stay with the old seat. Peers keep addressing the old seat. |
| Coordinator wiring | `main`/`mod` coordinators must be recreated before the workers that report to them, or managers set afterwards. |
| `/seat` run by hand in an unmanaged pane | If the pane's cwd falls inside a teamspace `project_repo`, `/seat` returns `unbound` with a proposal `seat create <tab-label> --teamspace <ts>` (`src/bootstrap/resolve.rs` `proposal_words`). Applying it opens a **second** tab with a new agent; it does not adopt the caller's pane. This is a trap for anyone trying to "join" from an existing pane. |
| `rebind <clone> --pane` | Removed in v0.3.0 (CHANGELOG). There is no command that binds a graph clone to an existing pane. |
| Resuming the old conversation | Possible only by hand: `plan seat override <seat> --args "--resume <session-id>"` makes graph launch the agent resuming that native session. The args persist for every later launch until cleared (`--clear args`), so this is a one-off workaround, not a path to recommend. |

Unmanaged workspaces, tabs and panes are invisible to graph: the observer only diffs objects matched to graph records (`src/observe/diff.rs` `DiffElement`), so the two worlds coexist today without interference. Moving a graph pane into an unmanaged tab is seen as a move (`moved_out`) and graph reopens the clone in a fresh home tab.

## 2. Gaps and what to build

### G1. No adoption of a live workspace, tab or pane (blocking)

Graph creates the Herdr object, then writes its binding. Nothing writes a binding for an object graph did not create. The crash-recovery "adopt" in `src/reconcile/herdr_exec.rs` `create_tab` matches only graph's own nonce labels.

**Build:** `plan teamspace adopt <workspace-id|label> [--name n] [--project-repo p]` and `plan seat adopt <tab-id|label> [--teamspace ts] [--name n] [--harness h]`, see section 3.

### G2. Matching the pane's existing threads seat (mostly already there)

- v0.3.0 locates a seated clone only through SeatObservation of `CloneRecord.threads_seat` (CURRENT-DESIGN "Clone location"), and `threads_seat` is write-once (GRAPH_STATE).
- Graph's `ResolveSeat` for a pane returns "the current unique owner for that host and pane" (README, `src/threads/mapping.rs`). For a pane whose agent has already checked in, that is the agent's existing seat, with all its memberships.
- Threads could bind a continuity key to an existing running seat (`ReserveContinuityKey { key, seat: Some(s) }`, threads `docs/programmatic-service-client.md`), but adoption does not use it: see G4.

So an adopted clone should be created with its `threads_seat` already set to the seat `ResolveSeat` returns for the pane at apply time. From then on the ordinary v0.3.0 machinery applies: SeatObservation locates it, and a later agent start in the same pane returns to the same seat through threads' own continuity (G4). **No relaunch is needed at adoption.**

Gaps inside G2:

- **Seat resolution at apply time.** The seat is a fact observed at admission, not a confirmed effect. Use the existing `admit_apply_with` `extra` channel (`src/plan/commands.rs`: "observed inputs the handler gathered at admission ... stay out of the plan hash"), and recheck in the mutation that the pane still has that owner. The plan shows the seat it saw.
- **Shell panes** (no threads seat): adopt as a seatless clone, located by its committed pane id as today.
- **A pane whose seat is already held by another clone** (another graph instance, or a stale record): refuse, as the write-once rule demands.

### G3. Preserving thread memberships (already true once G2 holds)

Memberships belong to the threads seat, and adoption keeps the seat. psa-global, 1:1 pair channels and topic threads stay as they are. Graph adds its own seat and team system channels as required invitations; the agent accepts them through its threads hook, as any graph clone does. Graph does not import existing external memberships into `participation` intent at adoption: that would make graph re-invite on succession, and it cannot invite to threads it does not own. The `adopt` skill lists them for information.

### G4. A way back out (blocking for "gradual")

Retiring a seat closes its tab (containment), and undo of a creation retires. Neither is a way out that leaves the agent running. Needed: `plan seat release <seat>` and `plan teamspace release <ts>`. They mark the records released (a retirement with reason `released`: history and transcripts kept, no Herdr effect), drop the binding, and leave the pane, the agent and the threads seat untouched. Undo of an adoption is a release.

**Adopted seats stay keyless** (2026-11-25, after the user asked whether threads' built-in continuity is enough; approved). It is enough for an adopted seat, so graph binds no continuity key to it and never launches into it with `HERDR_THREADS_CONTINUITY_KEY`. Release is then always clean: graph holds nothing in threads for the seat beyond its own channel invitations.

What threads gives a keyless seat on its own (threads `docs/operations.md`, `docs/agent-usage.md`):

| Event | Keyless seat |
|---|---|
| Agent exits and a new one starts in the same pane, or `/clear` | Same seat: a pane has one seat, and a new session adds a binding to it. |
| `herdr update --handoff` / live handoff | Same seat: process continuity reattaches it. |
| Herdr restart, agent resumed (`claude --resume`, by hand `codex resume`) | Same seat: a resumed session whose native id matches the seat's last binding reattaches by itself. |
| Herdr restart, agent started fresh (not a resume) | Same seat, no operator action (since hg-lnw, threads 3.6.0): graph rebinds the held seat onto its own pane (`ResurrectSeat`), and the keyless relaunch waits until the seat is placed or the 90 s grace ends, then starts; the agent enrolls on the original seat. A seat still held after the grace is reported as `clones.restart_held` with the operator remedy. Graph's resume driver still launches with the captured native session where the harness supports it. |
| Tab or pane closed | The seat is retired reversibly for 7 d (threads 3.5.0 and later). Undo of the closure resurrects the original seat onto the reopened pane (since hg-lnw): within 24 h with everything; from 24 h to 7 d with memberships and history, graph re-issuing its own seat-wide invitations while invitations and receipts from other parties are not restored. After 7 d the close is final and undo takes graph's fresh-seat succession: graph re-invites its own channels, and memberships graph does not own are lost. |

So the key bought one thing a keyless seat lacked: closing a tab and undoing it with the same threads seat. An adopted seat gave that up, which was no regression from the unmanaged tab it was; since hg-lnw it has it back for 7 d (Post-Implementation Notes).

What it costs to build: graph's ordinary start waits for a key reservation and launches with the key (`start_needed`, `src/reconcile/planner.rs`), and a key presented at check-in binds to the pane's seat. Adopted clones therefore need a recorded keyless mode: no `threads.reserve_key` want, no key-link want and no key in the launch environment. Graph already carries keyless clones (shell panes, the export-rejected case), so the paths exist; the mode makes it a recorded choice.

**Resurrecting a closed tab's seat.** The user asked (2026-11-25) for this capability in herdr-threads itself, which would close the last row of the table without a key. The request went to the herdr-threads main tab (thread dkTNBwdptgT, message mnSG2uhCyCFJ) and shipped as `ResurrectSeat` (ht-eu3v, threads 3.5.0). It was a follow-up to these phases; epic hg-lnw implemented it (Post-Implementation Notes).

A later, explicit `plan clone key <clone>` could upgrade an adopted seat to a keyed one for reversible retirement. A keyed seat cannot be released (threads refuses `ReleaseContinuityKey` while a seat holds the key, and a closed keyed seat stays frozen until finalized), only retired. Not in the first phases.

### G5. Behaviour changes the adoption plan must show

Once adopted, a space follows graph rules. The adoption plan prints these as warnings:

- Closing the tab or workspace retires the seat or teamspace (containment), undoable.
- Renaming the tab in Herdr renames the seat (an unexplained rename is recorded as an observed rename, `src/observe/classify.rs`); renaming the seat in graph relabels the tab (`src/reconcile/planner.rs`).
- If the agent exits with the pane kept, graph starts one in it (`start_needed` / `plan_resume` in `src/reconcile/planner.rs`), resuming the last captured native session when there is one. The observer captures the adopted agent's session (`OccupancyStarted`), so a later restart can resume it. Adoption records the observed harness, and any model or args it can read, as a seat override, so live template defaults do not schedule a replacement of the running session. A shell pane is adopted with the `shell` harness and is never started.
- The node manager is reopened when closed.

### G6. Smaller simplifications

- `/seat` proposal in an unmanaged pane: propose `seat adopt <this tab>` instead of `seat create` once G1 exists. Until then, change the proposal text to say that it opens a new tab.
- CLI flag for the seat cwd override (`seat override --cwd`) so worktree workers can be expressed.
- A listing of unmanaged workspaces, tabs and panes, so the operator sees how much is left to migrate. `roster --live --unmanaged` (hg-blw.6) covers it: ids and labels only.

## 3. Gradual migration

### Model

The unit of adoption is a **tab**, inside an adopted **workspace**. A seat is a tab in its teamspace's workspace, so a tab can only be adopted after its workspace is a teamspace. Adopting a workspace adopts no tabs: it writes a teamspace bound to that workspace, and every tab in it stays unmanaged until adopted. Managed and unmanaged tabs live in the same workspace; graph ignores the unmanaged ones, as it already does for objects it has no record of. (Verify in phase 1 with a tier-3 test that the planner never closes, renames or reuses an unmanaged tab in a teamspace workspace.)

### Order for one space

The user decided (2026-11-25): one space, one tab at a time. Some sessions stay unseated and are ignored.

1. The user picks a session to adopt and runs the `adopt` skill for it (section 4).
2. If the session's workspace is not a teamspace yet, the skill first proposes `teamspace adopt <workspace> --project-repo <repo>`.
3. The skill proposes `seat adopt <tab>` with the name, harness and overrides it observed.
4. The skill proposes the seat's instructions (`instructions set seat <seat> AGENTS.md --from <draft>`, teamspace `rules/<name>.md` where a rule is shared) from the transcript.
5. The skill proposes the hierarchy change this seat implies (see "Hierarchy" below).
6. The adopted agent runs `/seat` (prompted by its threads invitation to the seat channel, or at its next SessionStart) and picks up graph instructions.

Each step is one plan, confirmed by the user, undoable. Other tabs and spaces stay unmanaged throughout.

### Hierarchy

The user decided (2026-11-25): the hierarchy is inferred from transcripts during adoption, not from tab labels, and it is reconsidered as seats are added.

- The first adopted seat is the only seat, so it is the top of the hierarchy. On a fresh graph it becomes the node manager, as the first created seat does today (`first_seat::assignment`; `seat adopt` must honour it).
- Each later adoption reads the new session's transcript and threads history (who handed it work, whom it reports to, whom it hands work to) and proposes where it fits: its own `manager set`, and any change to existing pointers its role implies. A newly adopted coordinator may take over the teamspace `default_manager`, become the manager of seats adopted earlier, or take the node manager role (`node-manager set`).
- Every pointer change is its own plan line, shown with the evidence the skill found, and confirmed by the user. Graph already refuses a cyclic chain (`make_plan`).

### Way back out

- One tab: `seat release <seat>` (or `undo` of its adoption).
- One space: `teamspace release <ts>` releases all its seats; the workspace and agents keep running.
- Graph-created seats (not adopted) leave by `seat retire`, which closes the tab, as today. A release of a graph-created seat would leave an agent graph launched running with `HERDR_GRAPH_*` in its environment; allow it only for adopted seats in phase 1.

## 4. Extraction

Extraction is done by an agent running the `adopt` skill, not by a CLI heuristic: roles and rules live in prose (transcripts, handoff messages), which an agent can read and a parser cannot. The skill proposes; the user confirms each plan. Nothing is inferred silently.

Sources the skill reads for the session being adopted: the Herdr pane (label, cwd, agent kind, process args), the native session transcript of the pane's agent, the seat's threads history (handoffs received and sent, pair channels, memberships), and the existing graph (seats, managers, rules, templates) so its proposal fits what is already there. For an already adopted seat, graph's own transcript index and summaries are available too.

| Source | Becomes | Confidence |
|---|---|---|
| Workspace label | teamspace name | high |
| Common git toplevel of the workspace's pane cwds | teamspace `project_repo` | high when unique; otherwise left unset and listed |
| Tab label | seat name | high |
| Pane agent kind (Herdr pane agent info, `herdr pane process-info`) | seat harness (claude / codex / shell) | high |
| Process args (`--model`, `--dangerously-…` and the like) | seat override model / args | medium; shown, not assumed |
| Transcript and threads history: who hands this session work, whom it reports to, whom it directs | `manager set`, teamspace `default_manager`, `node-manager set` proposals (see "Hierarchy") | medium; each shown with its evidence. A `main`/`mod` tab label is supporting evidence only |
| Transcript: the task brief, standing instructions the user gave, corrections the user repeated | seat `AGENTS.md` draft (the role) and `rules/<name>.md` drafts (standing rules), written to files for review | medium; the user edits before `instructions set` |
| A rule that recurs across seats of the space | teamspace `rules/<name>.md` instead of per-seat copies | medium |
| Role that matches a shipped or existing seat template | suggestion to map the seat to it | low; the user picks |
| Repo `AGENTS.md` / `CLAUDE.md` | nothing | these load natively from the cwd; importing them duplicates them |
| User-global `CLAUDE.md` rules (psa-global, herdr rules) | nothing during migration | unmanaged agents still need them; move them to graph `global` rules only once every space is managed |
| Thread memberships | nothing in graph intent (G3) | listed for information |

**Templates.** The user decided (2026-11-25): build template extraction now, as the same kind of work as seat adoption. A `template-capture` skill reviews one adopted space, or two, and drafts a team template from it: members (one per seat, with harness and `responsibility`), reusable seat templates where a role recurs, member `AGENTS.md` generalised from the seats' instructions (project specifics removed), and the manager relationships. With two spaces it keeps what they share and lists what differs. The output is files for review, applied with the existing `template create <name> --from <file.toml>`.

Capturing a template does not change the source space. Attaching the source seats to the new template (`application apply --reuse <mem>=<seat>`) is a separate, optional plan: live template references then drive those seats, and the plan shows any session replacement that implies.

Gap to check during planning: team templates carry `thread_participation` relationships today; whether a template can declare member managers, or the capture must emit `manager set` plans beside the template, is read from `src/templates` before the bead is written.

## Recommendation

Phase 1 (adoption and release), phase 2 (the `adopt` skill), phase 3 (the `template-capture` skill). Phase 1 alone answers "move over gradually without relaunching"; phase 2 answers "simplify" and "extract roles"; phase 3 answers "extract templates". All three are in scope.

### Phase 1: adopt and release

- `plan teamspace adopt`, `plan seat adopt` (one clone per pane in the tab), `plan seat release`, `plan teamspace release`; undo of an adoption is a release.
- Threads seat taken from `ResolveSeat` at admission and rechecked at apply; refuse a seat held by another clone.
- Adopted clones are keyless (G4): no key reservation, no key link, no key in the launch environment.
- Observed harness, model and args recorded as seat overrides; plan warnings from G5.
- `/seat` proposal offers `seat adopt` in an unmanaged pane.
- Tier-3 tests: adopt a live tab with a checked-in agent, keep its memberships, survive a daemon restart, release, and leave unmanaged tabs in the same workspace untouched. Tier-5 test against real threads for the seat match.

### Phase 2: the `adopt` skill

- A skill shipped with graph and installed by `setup claude|codex`, beside `seat` and `graph`. The user runs it for one session at a time, from any agent pane (the session being adopted, or a coordinator).
- It reads the sources in section 4, then builds the plans in the section-3 order: teamspace adoption when needed, seat adoption, instruction drafts, hierarchy changes. It shows each plan verbatim and applies only through the ordinary relay flow; it writes drafts under the instance's `.graph-local/adopt/` for review.
- It needs read access the CLI does not give today: the native transcript path of an unmanaged pane's session (from Herdr's pane agent session, as the observer's `capture_session` reads it) and the pane's threads seat. Add a read-only `herdr-graph adopt inspect <tab|pane>` that prints those facts as JSON, so the skill does not guess paths.
- Relation to `roster --live --unmanaged` (hg-blw.6, graph state inspection design, bfeb8f3). The user decided that listing's shape: "not managed by graph - just enumerates them, no additional info". It lists only the Herdr workspaces, tabs and panes that have no graph binding, with their ids and labels. The skill uses it to show what is still unseated and gathers everything else itself.
- `seat override --cwd`.

### Phase 3: the `template-capture` skill

- Reviews one or two adopted teamspaces (seats, instructions, rules, managers, transcripts where the instructions are thin) and writes a draft team template TOML, seat templates and member `AGENTS.md` files for review.
- Applying is the existing `template create --from`; attaching the source seats is the optional `application apply --reuse`.

## Rejected options

- **Relaunch-only migration (today's path, documented and scripted).** Loses every conversation, threads seat and membership, and pair channel; peers must be re-pointed. It is the opposite of "gradual".
- **Re-introducing `rebind <clone> --pane`.** It bound an existing graph clone to a pane, but v0.3.0 locates a seated clone only through its threads seat; rebinding by pane would reintroduce the second location rule that release deleted. Adoption creates the clone with the right seat instead.
- **Implicit adoption by the observer** (graph claims any tab whose cwd falls in a teamspace's repo). Violates plan/confirm/apply: graph would change containment and relaunch behaviour of agents nobody approved.
- **Moving the agent's pane into a graph-created tab.** The observer only tracks known clones, so an unknown pane moved in stays unknown, and the user loses the tab layout anyway.
- **One composite "adopt workspace with all its tabs" plan.** Too coarse for gradual adoption and for review, and the user decided on one tab at a time.
- **Binding graph's continuity key to adopted seats** (at adoption, or deferred to graph's first launch). It makes release impossible without a herdr-threads change to unbind a key from a live seat, in exchange for same-seat undo of a closed tab, which an unmanaged tab never had.
- **Importing existing memberships into graph participation intent.** Graph cannot invite to threads it does not own, and recorded intent would trigger re-invites on succession. Threads already keeps the memberships with the seat.
- **A CLI `adopt survey` with fixed heuristics** (tab label `main`/`mod` means manager, first handoff message is the role). Replaced by the agent-run `adopt` skill at the user's direction: roles, rules and reporting lines are in transcripts, and a label convention is weak evidence.
- **Attaching the source seats to a captured template automatically.** Live references would start driving running sessions; it stays a separate plan.

## Decisions

1. Unit of adoption (user, 2026-11-25): one space, one tab at a time. Unadopted sessions stay unseated and are ignored.
2. Way back out: adopted seats stay keyless and rely on threads' built-in continuity, so release is always clean and herdr-threads needs no change (user, 2026-11-25). See G4.
3. Hierarchy (user, 2026-11-25): inferred from transcripts during adoption. The first adopted seat is the top; each later adoption may propose changes to the existing hierarchy.
4. Template extraction (user, 2026-11-25): build now, as a skill that reviews one or two existing spaces.
5. Unmanaged listing (user, relayed 2026-11-07): `roster --live --unmanaged` only enumerates ids and labels.

## Post-Implementation Notes

**2026-11-26, hg-lnw: resurrect on undo and frictionless Herdr restart.** Graph pins herdr-threads 3.6.0 and registers `service_session_v4` (no fallback; an older daemon's refusal is a named error and a doctor `threads` finding with the upgrade step). G4's last two rows changed. A closed keyless seat is reversibly retired for 7 d, and undo resurrects it onto the reopened pane with `ResurrectSeat`: within 24 h everything returns; from 24 h to 7 d memberships and history return, graph re-issues its own seat-wide invitations, and invitations or receipts from other parties are not restored; after 7 d undo takes the fresh-seat succession as before. The undo preview and the adoption plan warning name these windows. After a Herdr restart graph rebinds every held seat of a managed pane on its own pane (`ResurrectSeat` with no `replace`), and a keyless relaunch waits until the seat is placed or the 90 s grace ends, so no seat needs an operator. A seat graph cannot bring back is reported as `clones.restart_held` (after a restart) or `clones.resurrect_held` (after an undo), each with the remedy `herdr-threads seat rebind SEAT --pane PANE --operator`. Adopted seats stay keyless; release is unchanged. Design: docs/superpowers/runs/2026-11-26-resurrect-seat-frictionless-restart/2026-11-26-resurrect-seat-frictionless-restart-design.md.
