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

- none

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
observation classifier (its closure is never classified as a cascade, so the existing reconcile
planner reopens it). The first seat on a node is assigned automatically by the seat-creating plan
when the node has no live seat and no node manager; existing graphs get a stderr warning on every
command until `node-manager set` is applied.

## 4. Decision points, by section

### 4.1 Integrity as an IPC code (Part A)

Recommended: add `IpcErrorCode::Integrity` and `CommandError::integrity(msg)`, without bumping
`IPC_VERSION`. A stale daemon paired with a new CLI can at worst produce a frame error instead of a
message; a version bump would break every mixed pair until restart. Considered: keep `Internal`
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
`no node manager is assigned; report it to the user`. The message names the holder, the field, the
missing target and the HEAD revision.

### 4.4 CLI exit codes and messages (Parts A and C)

Recommended: a shared helper in `src/cli` (e.g. `cli::daemon_error`) maps
`ClientError::Unavailable(reason)` → stderr
`herdr-graph: graph daemon unavailable: <reason>. Your seat context is not loaded. Tell the user,
carry on with what you can without seat-specific rules, and run /seat again later. Do not guess
your seat.` and exit **5**; `ClientError::Remote { code: Integrity, message }` → stderr
`herdr-graph: graph integrity error: <message>. This is not transient; retrying will not help.
Carry on without seat-specific rules. Do not guess your seat.` and exit **6**;
`ClientError::NoInstance` → its existing line and `EXIT_NO_INSTANCE` (2). Constants
`EXIT_DAEMON_UNAVAILABLE = 5`, `EXIT_INTEGRITY = 6` sit next to the existing ones in
`src/daemon/client.rs`. The seat-specific sentences appear only for `seat`; the other commands use
the generic first clause plus exit 5. Considered: map only in `seat` (rejected: Part C needs the
same behaviour in four more commands).

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
(it can only arise from out-of-band edits, since plans reject both). Considered: a pointer on
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
  must exist and be live. Setting it to the current node manager is rejected as unchanged.
Both kinds list every rewritten record as a plan effect. Neither writes an undoable action (same as
`SeatOverride`); undo of these edits is out of scope. Considered: setting pointers via
`seat override` (rejected: overrides are runtime configuration, not organisation).

### 4.8 First seat becomes the node manager (Part B)

Recommended: the seat-creating kinds (`SeatCreate`, `ApplicationApply` via `create_member_seat`,
`TemplateEdit` member additions) assign `node_manager_seat` to the first seat they create when, at
plan time, `node_manager_seat` is unset and the graph has no non-retired seat (B10). The assignment
is a listed plan effect and adds the `graph.toml` blob precondition. An application creating several
seats assigns the first in member order. A graph that already has seats and no node manager is
never auto-assigned; it gets the warning (§4.10). Shipped templates gain a seat template
`node-manager` (`templates/node-manager/template.toml` + `AGENTS.md` describing the duties: listen,
detect, unblock; owns graph repair; lifecycle issue classes from B5's discussion) and a team
template `office` with a single active member `main` using it, added to `EXAMPLE_FILES`, so
`init --with-examples` followed by `application apply office` yields the node manager as the first
seat. Considered: init creating the seat directly (rejected: init runs before any daemon or Herdr
layout exists; seat creation belongs to plans).

### 4.9 Node manager permanence and reopening (Part B)

Recommended, two choke points:
- **Plans.** A shared check `refuse_node_manager_retirement(tree, effects)` runs in every plan kind
  that retires seats — `SeatRetire`, `TeamspaceRetire`, `CloneRetire` (last clone), `TemplateEdit`
  withdrawal, `ApplicationRetire`, and undo (Hydrate/ApplicationRetire compensation) — and rejects
  the plan with `node manager <name> cannot be retired; move the role first with node-manager set`
  (B8). Its last clone likewise cannot be retired by plan.
- **Observation.** In `src/observe/classify.rs`, the node manager's seat and clones are excluded
  from workspace, tab and pane cascade grouping, so a direct closure never retires it; the existing
  reconcile planner (`plan_effects` creating a missing tab and panes for an active seat) reopens it
  in its tab (B11). This deliberately overrides "direct tab/workspace closure follows containment
  retirement" (CURRENT-DESIGN.md) for the node manager only; CURRENT-DESIGN.md gets a dated line.
  Closing a workspace containing the node manager retires the other seats normally.
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
not a daemon fallback (B1 concerns answering queries).

### 4.11 Reparenting on retirement (Part B)

Recommended: inside `do_retire_seat` (all seven routes converge there), before archiving, compute
the retiring seat's effective manager M skipping seats being retired in the same mutation (nearest
surviving ancestor; ultimately the node manager, which cannot be retired), then rewrite every live
seat whose explicit `manager` is the retiring seat, and every teamspace whose `default_manager` is
the retiring seat, to M (B6). Seats that reached the retiring seat only through a teamspace default
follow automatically. Plan previews: `seat_retire_effects` and the other retire-effect builders list
a `seat.manager` / `teamspace.default_manager` rewrite effect per record via a shared
`reparent_effects` helper, so the apply-time diff matches. Observed cascades reparent without a
preview, recorded in their action record.

### 4.12 Surfacing (Part B)

Recommended: the `seat.resolve` reply gains `manager` and `node_manager`, each `null` or
`{seat, name, threads_seat}` where `threads_seat` is the manager's live clone's herdr-threads seat
when the existing binding index resolves it (else `null`). `WhoReply` gains `manager_chain: Vec<{seat, name}>`
rendered as one line `manager: <name> (<st>) → … → <node manager name> (<st>)` (or
`manager: none assigned`); `show <seat>` appends the same line after the record (B9).

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

## Post-Implementation Notes

> *As this design is implemented and iterated on — bug fixes, adjustments, anything that diverged from the assumptions above — append a dated note here, whether or not a formal debugging skill was used.*
