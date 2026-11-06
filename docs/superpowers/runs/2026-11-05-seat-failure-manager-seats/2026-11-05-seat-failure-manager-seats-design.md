# /seat failure path and manager seats — design

Beads: hg-qam (Part A), hg-au7 (Part B), hg-3mc (Part C). Source review:
[`docs/reviews/2026-10-29-seat-review-H3.md`](../../../reviews/2026-10-29-seat-review-H3.md).

## Goal

A graph seat whose `/seat` fails learns, from a distinct exit code and message, whether to carry on
(daemon unavailable) or escalate (graph integrity error naming the node manager); every seat has a
manager seat it can escalate lifecycle issues to, rooted at a permanent, always-open node manager;
and agent-facing read commands no longer fall back to reading the graph without the daemon.

## Requirement baseline

Source: the human user's messages in the Claude Code session that produced the H3 review
(2026-11-01 to 2026-11-05), relayed verbatim.

### Human request

> lets /super-auto what we scoped in this sesison. fully autonomous, both roasts are on

> id rather not have a such fallback. we can rely on hg daemon being up. if its down, we have bigger problems anyway and need to fix.
> its ok for cli when daemon is down to state to the agent to keep retrying in a bit and/or try to fix the graph daemon (possibly asking its coordinator for that)

> if the daemon is down, it remains unavailable.
> all sessions just carry on doing what they can and hope graph recovers
> the main coordinator session may be optionally given instructions to try to recover from that, but that's outside our work, we don't assume that

> we should add hierarchy i think. arbitrary optional pointer who's "your manager". [...] takeaway is each session know who their "manager" is that they can escalatae critical "lifecycle" issues to

> i think we dont have to fix thread, we can just leave a reference to the seat. "manager seat". how they communicate is open (likely 1:1 though).

> anything oncerning environment. any conflicting issues with peer agents. anything concerning self's ability to operate (running out of usage limits, trust prompts, etc). managers are tasked with listening, detecting, unblocking.

> node's manager, dedicated seat. likely (but not necessarily) is a root of the manger tree for all agents on the node

> all children of the retired seat get reparented to the retired seat's parent

> i thought all state mutation is via plans, no?

> lets have node manager to be the root of everything on the node, which can't be retired
> node managers dont have a parent

> - Should who and show display the manager chain?
> yes

> first seat that gets seated on the node becomes the node manager. [...] graph templates just provide default tempaltes which create the first seat with the node manager role.

> cant be moved either. same tab, stays open, gets reopened if it ever is closed (somewhat matches herdr where machine with no tabs is awkward)

> when none is picked, there's cli command to do so
> each graph command prints warning that node manager should get assgined
> and fine, lets have the same command be able to move node manager role to another seat. when that happens, all immediate children of old node manager, and old node manager itself, become children of the new node manager

> > I added your OK to override the "closing a tab retires its seats" rule for the node manager.
> ok

### Requirements

- B1 · "id rather not have a such fallback. we can rely on hg daemon being up" · source: session, 2026-11-04
- B2 · "its ok for cli when daemon is down to state to the agent to keep retrying in a bit" · source: session, 2026-11-04
- B3 · "all sessions just carry on doing what they can and hope graph recovers" · source: session, 2026-11-04
- B4 · approved: a named non-transient integrity error for unresolved references, distinct from the transient case, and the currently silent drops fail with that same error ("lgtm" ×3) · source: session, 2026-11-04
- B5 · "arbitrary optional pointer who's "your manager"" · "we can just leave a reference to the seat" · source: session, 2026-11-04
- B6 · "all children of the retired seat get reparented to the retired seat's parent" · source: session, 2026-11-04
- B7 · "i thought all state mutation is via plans, no?" (manager pointers change only through plans) · source: session, 2026-11-04
- B8 · "node manager to be the root of everything on the node, which can't be retired" · "node managers dont have a parent" · source: session, 2026-11-04
- B9 · "Should who and show display the manager chain? yes" · source: session, 2026-11-04
- B10 · "first seat that gets seated on the node becomes the node manager" · "graph templates just provide default tempaltes which create the first seat with the node manager role" · source: session, 2026-11-04
- B11 · "same tab, stays open, gets reopened if it ever is closed" · source: session, 2026-11-04
- B12 · "when none is picked, there's cli command to do so" · "each graph command prints warning that node manager should get assgined" · source: session, 2026-11-04
- B13 · "the same command be able to move node manager role to another seat. when that happens, all immediate children of old node manager, and old node manager itself, become children of the new node manager" · source: session, 2026-11-04
- B14 · approved: remove daemon-free fallbacks from who/ops/request/undo (hg-3mc, "create") · source: session, 2026-11-04

### Human scope changes

- S1 · 2026-11-04 · "lgtm" (in reply to: 'I wrote "the first seat created"' as the reading of "first seat that gets seated") · source: session · affects: B10

## 1. Problem description

`herdr-graph seat` is the only way a graph seat learns its identity and instructions, and the
SessionStart hook tells every graph pane to run it before replying. It has one handled outcome.
When the daemon is unreachable it prints an anyhow line and exits 1; when the committed graph has a
reference that does not resolve, the daemon answers `internal` and the CLI does the same. The skill
has no failure step, so the agent loops or invents an identity. Several unresolved references are
also silently dropped from the reply, so a seat can run with partial instructions unknowingly.
Separately, the graph has no notion of whom a session escalates lifecycle problems to, and four
agent-facing read commands quietly read the committed graph when the daemon is down, contradicting
the decision that the daemon is authoritative.

## 2. Main challenges

The failure path is mostly plumbing (an IPC code, two exit codes, error classification), but it
must classify precisely: an unresolved reference is integrity, an I/O or parse failure is not.
Manager seats touch the graph's most cross-cutting paths: every seat-retirement route (seven
callers converge on `do_retire_seat`), the observed-closure classifier, seat creation through three
kinds, and `graph.toml`, which today has no mutation kind, no `rev`, and needs a blob precondition.
The node manager's "never retired, always reopened" property must hold against plans, undo and
direct closure alike, and reparenting must be visible in plans while remaining correct for
retirements that happen through observation, where no plan preview exists.

## 3. Key decisions made

Integrity becomes a first-class IPC error code (`IpcErrorCode::Integrity`) raised from one helper,
and the CLI maps daemon-unavailable to exit 5 and integrity to exit 6 through one shared helper used
by every daemon-calling command touched here. Manager pointers are plain optional `SeatId` fields
on seat and teamspace records plus `node_manager_seat` in `graph.toml`, resolved by one pure
function. All pointer changes go through plan kinds (`manager set|clear`, `node-manager set`).
Reparenting lives inside `do_retire_seat`, so every retirement route, observed or planned,
reparents identically; plan previews list the rewrites via the same helper. The node manager is
protected at two choke points: plan admission (any plan whose effects retire it is refused) and the
observed-closure mutation (`CascadeMutation` decides at apply time, from one per-clone survival
fact, to end only its closed clones and keep the seat, so the existing reconcile planner reopens
it; workspace placement never adopts another teamspace's workspace for it). The first seat on a node is assigned automatically by the seat-creating plan
when the node has no live seat and no node manager; existing graphs get a stderr warning on every
command until `node-manager set` is applied.

## 4. Decision points, by section

### 4.1 Integrity as an IPC code (Part A)

Recommended: add `IpcErrorCode::Integrity` and `CommandError::integrity(msg)`, without bumping
`IPC_VERSION`. Compatibility rule for the no-bump decision, both skewed pairs: (a) new CLI + old
daemon — every new reply or record field (`manager`, `node_manager`, `reports`, `threads_seat`,
`manager_chain`, the record pointers) is `#[serde(default)]`, so the new CLI parses an old reply; an
old daemon never sends `integrity`. (b) new daemon + old CLI — the old CLI cannot decode the new
`integrity` code and reports a frame error; that degradation is accepted (the old CLI predates the
failure path anyway). The new CLI classifies a frame error as a transient daemon failure (exit 5,
§4.4). A version bump would break every mixed pair until restart. Considered: keep `Internal`
with a fixed message prefix the CLI matches (rejected: string matching is fragile and invisible to
the type system); bump `IPC_VERSION` (rejected: turns a rare cosmetic degradation into a guaranteed
outage at every upgrade).

### 4.2 What counts as integrity (Part A)

Recommended: one store-level error variant for "a reference does not resolve at the committed
revision" — e.g. `StoreError::Integrity { holder, field, target, reason }` — produced by a single
`unresolved(holder, field, target)` helper that replaces `missing()` in `src/bootstrap/mod.rs` and
the `missing()`/silent-skip sites in `src/bootstrap/instructions.rs`, and by mapping
`referenced_seat_template`'s missing/wrong-kind/team-structure failures to it.
`seat_reply` maps that variant to `CommandError::integrity` and every other `StoreError` to
`internal`. Covered references: seat, clone, teamspace, the seat's `template_ref` template, each
application, each application's template, and every shared seat template. The three silent skips
(`template_ref` not found, missing application, missing application template) become integrity
errors (B4). Plain read failures (I/O, corrupt object, malformed TOML) stay `Internal`. The reply
on integrity carries no partial paths. Considered: per-reference warnings with partial context
(rejected by the human, B4).

### 4.3 Integrity message names the node manager (Part A, depends on Part B data)

Recommended: when `seat.resolve` fails with integrity, the daemon reads `graph.toml`'s
`node_manager_seat` (independent of the broken object) and appends
`report it to the node manager seat <name> (<st_id>)` to the message; when unset, it says
`no node manager is assigned; report it to the user`. When the failing caller is itself the node
manager seat, the suffix uses the "report it to the user" form. The message names the holder, the
field, the missing target and the HEAD revision.

### 4.4 CLI exit codes and messages (Parts A and C)

Recommended: a shared helper in `src/cli` (`cli::daemon_error`) classifies by condition, and its
mapping is complete over every `ClientError` variant and `IpcErrorCode`:

| Condition | Source | Exit | stderr |
|---|---|---|---|
| not inside a Herdr pane | `HERDR_SOCKET_PATH` unset, and the error is a no-reply condition (`Unavailable`, `Frame`, `NoInstance`); a real `Remote` reply from a running daemon keeps its own row | 2 | `herdr-graph: not inside a Herdr pane; this session has no graph seat` |
| no instance configured | `ClientError::NoInstance` | 2 (`EXIT_NO_INSTANCE`) | its existing line |
| no well-formed reply from a working daemon | `ClientError::Unavailable`, `ClientError::Frame` (I/O, timeout, truncated or undecodable frame), `Remote{Unavailable}` ("still starting", "failed to start"), `Remote{VersionMismatch}` | 5 | `herdr-graph: graph daemon unavailable: <reason>. Retry in a bit; if it persists, get the graph daemon fixed (ask your manager or coordinator).` |
| graph integrity | `Remote{Integrity}` | 6 | `herdr-graph: graph integrity error: <message>. This is not transient; retrying will not help.` |
| everything else | `Remote{BadRequest, UnknownCommand, Rejected, Internal}` and any other | 1 | `herdr-graph: <error>` |

Command-specific `Remote` arms (e.g. `StillRunning` in plan/request) are handled by the command
before the helper. For `seat`, exits 5 and 6 append: `Your seat context is not loaded. Tell the user,
carry on with what you can without seat-specific rules, and run /seat again later (exit 5) / do not
retry (exit 6). Do not guess your seat.` Every command that calls the daemon routes its client errors
through this helper. Constants `EXIT_DAEMON_UNAVAILABLE = 5`, `EXIT_INTEGRITY = 6` sit next to the
existing ones in `src/daemon/client.rs`. Tests cover one row per condition, including a hung daemon
(read timeout → Frame) and a daemon that is still starting. Considered: map only in `seat`
(rejected: Part C needs the same behaviour in four more commands); classify by `ClientError`
variant (rejected by design roast r1: hung, malformed and still-starting daemons fell to exit 1).

### 4.5 Skill and hook (Part A)

Recommended: `skills/seat/SKILL.md` step 1 gains a failure branch keyed on the exit code — 5: show
the line, carry on without seat rules, run `/seat` later, do not try to repair the daemon unless
instructed; 6: do not retry, report to the node manager named in the error (or the user), carry
on; any other failure: show the line, do not guess. `HOOK_PROMPT` gains "If /seat fails, follow
the skill's failure step." Step 3 of the skill mentions `manager` and `node_manager` from the reply
as the escalation targets for lifecycle issues (B5). The skill text must be self-contained; the
`setup claude` install path that copies skills is unchanged.

### 4.6 Manager pointer data model (Part B)

Recommended: `SeatRecord.manager: Option<SeatId>`, `TeamspaceRecord.default_manager:
Option<SeatId>`, `GraphRecord.node_manager_seat: Option<SeatId>`, each
`#[serde(default, skip_serializing_if = "Option::is_none")]` (the `summarizer_seat` pattern; no
schema bump — nothing reads schema versions). Effective manager of seat S, by one pure function
`effective_manager(tree, seat) -> Result<Option<SeatId>, StoreError>` in `src/model/`: if S is the
node manager → `None`; else S.manager if set; else S's teamspace `default_manager` if set and ≠ S;
else `node_manager_seat` (which may be `None` on an unassigned graph). `manager_chain(tree, seat)`
walks it to the root with a visited set; a cycle or an unresolvable pointer is an integrity error
(it can only arise from out-of-band edits, since plans reject both). A pointer to a seat that exists but is retired is transparent: resolution continues through that seat's own effective manager, so a resurrected seat or a stale pointer never produces an integrity error. A `node_manager_seat` that is missing or retired is treated as unassigned (fallback `None`, warning per §4.10), not as an integrity error; an explicit `manager` naming a missing seat is integrity. `live_node_manager(tree)` is the one helper that answers "who is the live node manager". Considered: a pointer on
clones (rejected: clones come and go; the seat is the stable identity, B5); a dedicated escalation
thread maintained by graph (rejected by the human, B5).

### 4.7 Plan kinds for pointers (Part B)

Recommended, all through `OrgKind` (B7):
- `manager set <seat|teamspace> <manager-seat>` and `manager clear <seat|teamspace>` — one kind
  (`RequestKind::ManagerEdit`), writing the seat's `manager` or the teamspace's `default_manager`.
  Validation: target seat exists and is not retired; the subject is not the node manager when it is
  a seat (B8: the node manager has no parent); the resulting effective chain from the subject (and,
  for a teamspace default, from every seat in that teamspace) contains no cycle. relied_on: the
  subject's rev, the manager seat's rev, `graph.toml` blob.
- `node-manager set <seat>` — one kind (`RequestKind::NodeManagerSet`), writing `graph.toml` via
  `put_file(layout::graph_toml(), …)` with a `Version::Blob` precondition on `graph.toml`
  (precedent: `src/plan/instructions.rs`). Unset → assign. Set to another seat → move (B13): every
  seat whose explicit `manager` is the old node manager, and every teamspace whose
  `default_manager` is the old node manager, is rewritten to the new one; the old node manager's
  `manager` becomes the new node manager; the new node manager's own `manager` is cleared. Target
  must exist and be live. Setting it to the current node manager is rejected as unchanged. When the recorded node manager is missing or retired, `set` behaves as an assign that also rewrites pointers naming the dangling id and skips the old record.
Both kinds list every rewritten record as a plan effect. Neither writes an undoable action (same as
`SeatOverride`); undo of these edits is out of scope. Considered: setting pointers via
`seat override` (rejected: overrides are runtime configuration, not organisation).

### 4.8 First seat becomes the node manager (Part B)

Recommended: the seat-creating kinds (`SeatCreate`, `ApplicationApply` via `create_member_seat`,
`TemplateEdit` member additions) assign `node_manager_seat` to the first seat they create when, at
plan time, `node_manager_seat` is unset and the graph has no non-retired seat (B10). The assignment
is a listed plan effect and adds the `graph.toml` blob precondition. An application creating several
seats assigns the first in member order. A graph that already has seats and no node manager is
never auto-assigned; it gets the warning (§4.10). Every path that creates or resurrects a seat (`SeatCreate`, application apply/hydrate, `TemplateEdit` additions, `SeatResurrect`, `TeamspaceResurrect`, undo restore) goes through one shared first-seat helper; S1 records the human's approval of "created" as the reading of "seated". Shipped templates gain a seat template
`node-manager` (`templates/node-manager/template.toml` + `AGENTS.md` describing the duties: listen,
detect, unblock; owns graph repair; lifecycle issue classes from B5's discussion) and a team
template `office` with a single active member `main` using it, added to `EXAMPLE_FILES`, so
`init --with-examples` followed by `application apply office` yields the node manager as the first
seat. Considered: init creating the seat directly (rejected: init runs before any daemon or Herdr
layout exists; seat creation belongs to plans).

### 4.9 Node manager permanence and reopening (Part B)

Invariant (B8, B11): **the node manager seat and its teamspace are Active, and only a plan can
change its recorded placement.** Every entry path is checked against it, and the spec promises only
behaviour an existing mechanism delivers.

Recommended, two choke points:
- **Plans.** A shared check `refuse_node_manager_retirement(tree, effects)` runs in every plan kind
  that retires seats — `SeatRetire`, `TeamspaceRetire`, `CloneRetire` (last clone), `TemplateEdit`
  withdrawal, `ApplicationRetire`, and undo (Hydrate/ApplicationRetire compensation) — and rejects
  the plan with `node manager <name> cannot be retired; move the role first with node-manager set`
  (B8). Its last clone likewise cannot be retired by plan.
- **Observation.** Clone survival is one per-clone fact, computed once by the observer and read by
  every consumer: a clone of a closed container (workspace, tab) whose pane is still matched in the
  snapshot is *survived* (the same `pane_survives` test the tab path uses). `src/observe/classify.rs`
  makes no node-manager decision: each cascade lists the closed container's seats and every live
  clone of them, plus the `survived` subset, and its `ends` lists every listed clone (survived
  included). The tab path is unchanged from main: a closed tab with a survived clone is a move (the
  seat is flagged `moved_out`), not a cascade. `CascadeMutation` alone decides, at apply
  time against `live_node_manager` (closing the window between classification and a concurrent
  `node-manager set`):
  - the node manager: only its non-survived clones get `end_occupant(PaneClosed)` and an absent
    runtime; survived clones stay bound where they are; the seat and its teamspace are kept Active
    (teamspace runtime absent), so a direct closure never retires it. When no clone of it remains
    bound after those ends, the mutation clears `moved_out` so the planner reopens it in its tab.
  - every other seat: all listed clones, survived included, are ended, absented and retired through
    `do_retire_seat` (the containment rule as on main; a moved pane does not save a plain seat).
  - the teamspace record written after the seat loop is re-read after reparenting, in both the kept
    and the retired branch.
  The mutation returns the `(clone, reason)` ends it actually applied, and `commit_ops` emits
  `SessionEnded` from that list, never from the classify-time `ends`, so a store end and its event
  cannot disagree. The existing reconcile planner (`plan_effects` creating a missing tab and panes
  for an active seat) reopens the node manager's non-survived clones in its tab (B11).
  **Workspace placement.** `moved_out` is the existing tab-level surfacing flag with its main-branch
  effects; it is not a survival signal and never decides workspace placement. In the planner's
  `LiveIndex::workspace`, a clone pane places its teamspace's workspace only when the live
  workspace holding it is neither token-stamped for a different teamspace nor bound (desired
  runtime) to a different teamspace; otherwise the pane is ignored for placement and reconciliation
  creates a fresh workspace for the teamspace. A survived clone's pane in a foreign workspace is left
  where it is: the planner does not relabel, re-tag or close that workspace or any tab in it.
  Required tests (FakeHerdr): (1) a two-clone node manager with one clone moved into another
  teamspace's tab, then its home workspace closed: the other teamspace's seat stays Active, its
  workspace and tab are untouched, the moved clone stays bound, the node manager's other clone is
  reopened in a fresh home workspace; (2) a plain seat moved into another teamspace's tab, then its
  home workspace closed: the seat is retired and a `SessionEnded` is emitted for its clone; (3) the
  node manager's teamspace `default_manager` names a sibling seat, home workspace closed: the
  stored pointer equals the reparent rewrite in the action record. This deliberately overrides "direct tab/workspace closure follows containment
  retirement" (CURRENT-DESIGN.md) for the node manager only; CURRENT-DESIGN.md gets a dated line.
  Closing a workspace containing the node manager retires the other seats normally; reconciliation recreates the workspace if needed, then the tab and panes. A plan whose effects would change the node manager seat's placement (teamspace, tab or workspace) is refused at the same `make_plan` guard ("same tab", B11), as is any effect that deactivates the node manager seat or its last live clone. An observed move of its pane or tab outside a plan is tolerated and surfaced, not reverted: Herdr exposes no move primitive, so the existing `moved_out` surfacing stays (it gates no survival or placement decision, per the Observation bullet) and the observer logs a warning naming the node manager; reopening is promised only for closure. `node-manager set` and first-seat assignment require an Active target and teamspace (a dormant target is activated as a listed effect). TemplateEdit, ApplicationRetire and undo compensation are covered by the `make_plan` guard. The undo refusal for the node manager's creating action names the `node-manager set` remedy. More generally, `make_plan` refuses any plan whose effects would leave a manager chain cyclic (seat creation into a teamspace with a default, teamspace moves, resurrection, teamspace default changes), using model::manager's would-create-cycle check over every seat whose effective manager changes.
Considered: refusing at `do_retire_seat` only (rejected: fails at apply time instead of plan time,
and the observed path would error instead of reopening).

### 4.10 Unassigned warning (Part B)

Recommended: `cli::main` (via a function in a non-registration module, respecting the
`src/cli/mod.rs` header rule — call it from `main` before dispatch) reads `graph.toml` at HEAD
directly from the instance store and, when `node_manager_seat` is unset and at least one
non-retired seat exists, prints to stderr once:
`herdr-graph: warning: no node manager is assigned; run herdr-graph plan node-manager set <seat>`.
Skipped for `init`, `setup`, `daemon`, `seat --hook-prompt`, and when no instance is configured;
any read failure prints nothing. This is a read of the committed graph for a warning only; it is
not a daemon fallback (B1 concerns answering queries). The same warning fires when `node_manager_seat` names a seat that is missing or retired.

### 4.11 Reparenting on retirement (Part B)

Recommended: inside `do_retire_seat` (all seven routes converge there), before archiving, compute
the retiring seat's effective manager M skipping seats being retired in the same mutation (nearest
surviving ancestor; ultimately the node manager, which cannot be retired), then rewrite every live
seat whose explicit `manager` is the retiring seat, and every teamspace whose `default_manager` is
the retiring seat, to M (B6). M is computed only when at least one such live seat or teamspace
exists: a retiring seat that nothing points at needs no target, so its own (possibly broken) chain
is never walked and preview and apply agree. Seats that reached the retiring seat only through a teamspace default
follow automatically. Every retirement route records each rewrite (`{holder, field, old, new}`) in its action record (plan `Retire`, `ApplicationRetire`, observed cascade, undo-driven). Plan previews: `seat_retire_effects` and the other retire-effect builders list
a `seat.manager` / `teamspace.default_manager` rewrite effect per record via a shared
`reparent_effects` helper, so the apply-time diff matches. An integrity error while computing M during an observed cascade never aborts the cascade: M falls back to `live_node_manager` (or none), and the action record notes the fallback. Observed cascades reparent without a
preview, recorded in their action record: observed closure retirement is the existing sanctioned non-plan mutation (CURRENT-DESIGN containment retirement), and reparenting rides along with it; B7 governs deliberate pointer changes. Undo of a retirement (the `Retire` and `ApplicationRetire` action kinds) restores each pointer the reparent rewrote, but only where that pointer still holds the reparent value; others are left and listed in the undo plan. Undo-driven retirement (e.g. undo of a hydrate) reparents through `do_retire_seat` like every other route.

### 4.12 Surfacing (Part B)

Recommended: the `seat.resolve` reply gains `manager` and `node_manager`, each `null` or
`{seat, name, threads_seat}` where `threads_seat` is the manager's live clone's herdr-threads seat
when the existing binding index resolves it (else `null`). `WhoReply` gains `manager_chain: Vec<{seat, name}>`
rendered as one line `manager: <name> (<st>) → … → <node manager name> (<st>)` (or
`manager: none assigned`); `show <seat>` appends the same line after the record (B9); `show` stays a daemon-free reader and maps a chain integrity failure to the same exit-6 text. The reply also carries `reports` (the seats whose effective manager is this seat), and when it is non-empty the skill tells the session it is a manager: listen for, detect and unblock its reports' lifecycle issues. The text form of `herdr-graph seat` prints `manager:` and `node manager:` lines. When the session's own seat is the node manager, the reply and skill say so and carry the node-manager duties (shared text with `templates/node-manager/AGENTS.md`), so a moved role is not in name only. When `node_manager_seat` does not resolve to a live seat, the integrity suffix uses the "report it to the user" form.

### 4.13 Remove daemon-free fallbacks (Part C)

Recommended: in `who`, `ops` (list, get, check_instruction), `request` and `undo` (candidate list),
replace the `ClientError::Unavailable` → direct-read branch with the shared helper from §4.4 (exit
5); delete the now-unused direct-read functions. `status` keeps its fallback: it is the operator's
diagnostic for a dead daemon (prints "daemon: not running"). `show`, `list`, `path`,
`instructions show` and `seat --hook-prompt` are documented daemon-free readers by design and stay.
Tests that relied on the fallbacks change to assert exit 5.

### 4.14 Testing

Unit tests next to each change (bootstrap, plan, templates, observe, reconcile tests modules) using
the existing fixtures (`kinds_extra::testkit`, per-module fixtures driving plan words); CLI
behaviour (exit codes 2/5/6, warning line) in `tests/` with `TestRoot`. Full suite:
`cargo test` (tiers 1–2); the private-Herdr tier 3 runs where available.

## Key abstractions

**model::manager** — new module, the pure resolver for manager pointers over one tree.
- Owns: the effective-manager rule from §4.6 (node manager → none; then the seat's explicit `manager`; then its teamspace's `default_manager` if that is not the seat itself; then `node_manager_seat`). Also owns `manager_chain` with its visited-set cycle detection, "nearest surviving manager" given a set of seats retiring together (§4.11), and the "would this edit create a cycle" check used to validate `ManagerEdit`. All lookups are built from that one rule.
- Hides: the precedence order and the skip rules for the node manager and self-defaults. Callers never read `manager`, `default_manager` or `node_manager_seat` directly to decide who someone's manager is.
- Must not: write records, build plan effects, render text or know about IPC. Writes belong to plan::managers, rendering to `WhoReply` and `show`, and error classification to `CommandError`.
- Relationships: depends only on `TreeRead` and the model records. Called by plan::managers, `seat_reply` (§4.12), the `who` handler, `show`, and `ManagerEdit` validation. Nothing in `model` depends on its callers.
- Lifecycle: free functions over a borrowed tree. Holds no state and lives per call.
- Errors: returns `StoreError::Integrity` for a cycle or an unresolvable pointer, built through the same `unresolved` helper as Part A. Plain read failures pass through as the other `StoreError` variants.

**plan::managers** — new module, the only writer of manager pointers and the node-manager policy for plans.
- Owns: `reparent_effects` (the plan-time preview: one `seat.manager` or `teamspace.default_manager` effect per rewritten record); the apply-time reparent, called from inside `do_retire_seat`; the node-manager move rewrites (§4.7, B13); the first-seat assignment decision and its effect (§4.8); the `graph.toml` write together with its `Version::Blob` relied_on; `refuse_node_manager_retirement` (§4.9).
- Hides: which records a retirement or a move rewrites, how "first seat on the node" is detected, and how `graph.toml` is written safely, since it has no `rev`.
- Must not: decide who the effective manager is (it delegates to model::manager), parse CLI words (the `OrgKind` impls do that), or touch runtime or observation (classify does that).
- Relationships: depends on model::manager and the writer overlay. Used by `do_retire_seat`, every seat-creating or -resurrecting path (`SeatCreate`, application apply/hydrate via `create_member_seat`, `TemplateEdit` member additions, `SeatResurrect`, `TeamspaceResurrect`, undo restore), the retire-effect builders, `ManagerEdit` and `NodeManagerSet`. The retirement guard runs once in `make_plan` over `body.effects` — the single plan-admission point, which already covers the replan in `plan/apply.rs` and undo (an `OrgKind`), so future retiring kinds need no call site of their own. It refuses a `seat.retire` targeting the node manager, a set of `clone.retire` effects covering all of its live clones, any change to its placement, any deactivation, and (beside it, the cycle guard) any plan leaving a manager chain cyclic.
- Lifecycle: stateless functions. Mutable state exists only in the `MutationCx` overlay passed in, which the writer owns for one mutation.
- Errors: refusals, cycles and "unchanged" become `PlanError::Invalid` with the §4.9 or §4.7 message. Store failures, including integrity, pass through as `PlanError::Store`.

**cli::daemon_error** — new module, the one mapping from `ClientError` to a stderr line and an exit code.
- Owns: the §4.4 condition table, complete over every `ClientError` variant and `IpcErrorCode`: not inside a Herdr pane (no-reply conditions only) → 2; `NoInstance` → 2; no well-formed reply (`Unavailable`, `Frame`, `Remote{Unavailable}`, `Remote{VersionMismatch}`) → 5 with the retry-later text; `Remote{Integrity}` → 6; other `Remote` codes → 1. It also renders a local `StoreError::Integrity` (daemon-free readers such as `show`) to the exit-6 text. The constants `EXIT_DAEMON_UNAVAILABLE` and `EXIT_INTEGRITY` sit beside the existing `EXIT_*` in `src/daemon/client.rs`.
- Hides: which `ClientError` or `IpcErrorCode` maps to which exit code.
- Must not: read the store or fall back to direct reads; those direct readers are deleted (§4.13).
- Relationships: called by every daemon-calling command (`seat`, `who`, `ops`, `request`, `undo`, `plan` and the rest), each replacing its own match arms; command-specific `Remote` arms such as `StillRunning` stay in the command. `seat` adds its extra sentences by composing them with the generic mapping, not through a mode flag inside it.
- Lifecycle: free functions, called once per CLI invocation.

**StoreError** — extended: a new `Integrity { holder, field, target, reason }` variant, built only by `unresolved(...)`, which replaces `missing()` in `bootstrap/mod.rs` and `bootstrap/instructions.rs`, the three silent skips, and `referenced_seat_template`'s `Corrupt` arms.

**CommandError / IpcErrorCode** — extended: `IpcErrorCode::Integrity` and `CommandError::integrity`, with one store-error classifier (`Integrity` → integrity, everything else → internal) used by `seat_reply`, `who` and `show`, so `manager_chain` cycles surface the same way. The node-manager suffix from §4.3 is added in `seat_reply` only.

**OrgKind registry** — extended: two new kinds, `ManagerEdit` and `NodeManagerSet`, registered where the existing kinds are; like `SeatOverride`, neither writes an undoable action.

**observe::classify / CascadeMutation** — extended: classify computes per-clone survival once and lists every live clone of a closed container with its `survived` subset; it holds no node-manager logic (§4.9). `CascadeMutation` is the only place that decides, against `live_node_manager` at apply time: the node manager ends only its non-survived clones and is kept with its teamspace; every other seat ends and retires all its clones. It returns the ends it applied, and `commit_ops` emits `SessionEnded` from exactly those, so a cascade classified before a role move cannot retire the new node manager and store and events cannot disagree.

**reconcile::LiveIndex::workspace** — changed: a clone pane places its teamspace's workspace only when that live workspace is not token-stamped for, or bound to, a different teamspace; `moved_out` is not consulted for placement (§4.9).

Variation: seat-retirement routes (seven today) → reparenting lives in `do_retire_seat` and the guard in `make_plan`, so a new route inherits both without a call; seat-creating or -resurrecting paths (six) → one shared first-seat helper in plan::managers; error outcomes are a closed set → a `match` in cli::daemon_error and the `CommandError` classifier; integrity reference kinds → data (holder and field strings) passed to the one `unresolved` helper.
Walk-through: `/seat` → `seat_reply` hits an application whose template is missing → `unresolved` → `StoreError::Integrity` → classifier → `CommandError::integrity` plus the node-manager suffix → CLI `Remote { Integrity }` → cli::daemon_error prints the line, exit 6. `seat retire A` → `SeatRetire::plan` lists `seat_retire_effects` plus `reparent_effects` (model::manager supplies A's nearest surviving manager) → `make_plan` guard passes → `do_retire_seat` reparents on apply. A future bulk-retiring kind touches one new `OrgKind` and its registration; a future template-level default manager touches only model::manager's precedence rule and the template record.
Per spec §4.4: the seat-specific sentences appear only for `seat` — a deliberate per-caller addition on top of the shared mapping.
Per spec §4.13: `status` keeps its fallback, and `show`, `list`, `path`, `instructions show` and `seat --hook-prompt` stay daemon-free readers — deliberate exceptions to cli::daemon_error.
Per spec §4.10: the unassigned warning reads `graph.toml` directly in a free function in a non-registration CLI module, called from `main` — a warning, not a fallback.
Per spec §4.9: `CascadeMutation` special-cases the node manager, deliberately overriding containment cascades for that one seat; classify does not.
Per spec §4.11: the preview skips seats retiring in the same mutation while `do_retire_seat` retires them sequentially; compare preview and apply on final pointer values, not on the count of writes.

Digest:
- model::manager: owns the effective-manager precedence, chain walk, cycle detection and nearest surviving manager;
    a new pointer source goes here;
    callers never read `manager`, `default_manager` or `node_manager_seat` to decide who someone's manager is.
- plan::managers: owns every write of manager pointers (reparent on retirement, node-manager move, first-seat assignment, the `graph.toml` blob write) and the node-manager retirement guard in `make_plan`;
    new pointer-rewriting rules go here;
    kinds and retirement routes never rewrite pointers or check for node-manager retirement themselves.
- cli::daemon_error: owns the mapping from `ClientError` to stderr text and exit codes 2, 5 and 6;
    a new daemon error outcome is added here;
    commands never match `ClientError::Unavailable` themselves or fall back to direct reads.

## Unchanged behavior

Amend code-1 (node-manager clone survival) must not change:
- The node manager is never retired by plan, undo or observed closure, and its teamspace stays Active (B8).
- A closed node manager is reopened in its own tab by the existing reconcile planner (B11); an observed move of its pane or tab is tolerated, surfaced with a warning naming it, and not reverted.
- Plain (non-node-manager) seats follow the containment rule as on main: closing their workspace or tab retires them and all their clones, with a `SessionEnded` per ended clone.
- The tab path: a closed tab with a surviving clone is a move that flags the seat `moved_out`, not a cascade; `moved_out`'s other planner effects (no tab recreation, no relabel, matcher exclusion) are as on main.
- Reparenting on every retirement route goes through `do_retire_seat` and is recorded in the action record; undo restores pointers that still hold the reparent value.
- Reconciliation of workspaces with no foreign-teamspace panes places and adopts them as before (stamp, nonce label, rebound clone pane).
- Plan admission guards (`make_plan` node-manager retirement, placement and cycle refusals) and exit codes 2/5/6.

## Post-Implementation Notes

> *As this design is implemented and iterated on — bug fixes, adjustments, anything that diverged from the assumptions above — append a dated note here, whether or not a formal debugging skill was used.*
