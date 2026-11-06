# H3: `/seat` has no failure path

Tracking: Part A (H3 fix) is Beads **hg-qam**; Part B (manager seats) is Beads **hg-au7**.

Revision 3. Checked against `main` at c77d9ad; line numbers refer to that revision.

History:
- Rev 1 proposed a daemon-free fallback for `seat` and per-reference warnings.
- Rev 2 dropped both at the user's direction. The graph daemon is the single source of truth.
- Rev 3 records the user's decisions: no escalation when the daemon is down, the integrity-error
  semantics, and a manager-seat hierarchy (Part B).

## Abstract

At every start, clear and compaction, herdr-graph tells each graph-managed agent session to run `/seat`
before it replies. `/seat` is the only way a session learns its identity, rules and instructions. Yet the
only outcome the skill tells the agent how to handle is success. It can fail in two ways that call for
different responses.

The first is transient: the graph daemon is unavailable. Nothing the session does will fix that, so the
session should say so, carry on with what it can, and re-run `/seat` later. The second is permanent: the
daemon is up, but the committed graph holds a reference that does not resolve. Every seat that uses that
object then fails, and retrying will never help. The graph has to be repaired by whoever is responsible
for it.

Today both failures reach the agent as a single stderr line and exit 1, and the skill has no failure step.
Elsewhere in the same code, some unresolved references are dropped without any signal, so a seat can run
on incomplete instructions without knowing it.

The fix keeps the daemon authoritative and makes failure explicit. The CLI reports the two cases
distinctly. Every unresolved reference becomes a named, non-transient integrity error that tells the agent
whom to escalate to. The skill gives a bounded procedure with no guessing. A small addition to the graph
model (Part B), an optional manager seat for each session, gives the second case, and lifecycle problems
generally, a defined place to go.

## Verified facts

**Daemon unavailable.**
- `seat` calls `call_daemon("seat.resolve", …, CallMode::Ensure)` at `src/cli/seat.rs:40`.
- Errors are flattened. `cli::main` prints `herdr-graph: <reason>` and exits 1 (`src/cli/mod.rs:84-86`).
- `Ensure` fails in four cases (`src/daemon/client.rs:124-150`), all reported as
  `ClientError::Unavailable`:
  - `HERDR_SOCKET_PATH` is unset;
  - ensure itself fails;
  - the daemon is still unreachable after ensure;
  - the daemon is attached to a different Herdr socket.
- `seat` also collapses `NoInstance` to exit 1. Every other command exits 2 (`EXIT_NO_INSTANCE`).

**Graph integrity.** These cases fail as `internal` at `src/bootstrap/mod.rs:297`:
- a missing seat or clone (`:143-148`);
- a missing teamspace (`:149-150`);
- a shared seat template that is missing, of the wrong kind, or carries team structure. This error is
  raised inside `referenced_seat_template` (`src/model/effective.rs:133-142`), called at `:184` and `:210`.
  The `missing()` calls at `:187` and `:213` cannot be reached in practice.
- an application template whose record cannot be read (`:202`).

These references are silently dropped today:
- the seat's own `template_ref` (`:178-179`);
- a missing application (`:193-195`);
- an application whose template is missing (`:200`).

Normal operations cannot produce any of these states:
- templates have no delete verb (`src/templates/kinds.rs:473,569,908`);
- template kind is immutable (`kinds.rs:879-881`);
- shared-template references are validated at plan time (`src/templates/seat_definitions.rs:4-18`);
- retired records stay in place and still resolve.

An unresolved reference therefore means out-of-band damage or a bug.

**Error transport.** `IpcErrorCode` has no integrity variant (`src/ipc.rs:35-44`). The CLI cannot tell an
integrity failure from any other internal error except by reading free text.

**Hook and skill.**
- `HOOK_PROMPT` (`src/cli/seat.rs:54-56`) says "Before replying, run the seat skill (/seat)".
- Step 1 of `skills/seat/SKILL.md` has no failure branch.
- Graph-launched panes already carry `HERDR_GRAPH_SEAT` and `HERDR_GRAPH_CLONE`
  (`src/model/launch.rs:21-24`).

---

# Part A: H3 fix (decided) — hg-qam

## A1. Daemon unavailable: say so, carry on

When the daemon is unavailable, it stays unavailable as far as a seat is concerned. Sessions carry on
with what they can and do not attempt recovery. No escalation is required. A coordinator may be
separately instructed to try recovery, but graph does not assume it.

`src/cli/seat.rs`: match `ClientError::Unavailable(reason)` and exit **5** with:

> herdr-graph: graph daemon unavailable: <reason>. Your seat context is not loaded. Tell the user, carry
> on with what you can without seat-specific rules, and run `/seat` again later. Do not guess your seat.

Map `NoInstance` to `EXIT_NO_INSTANCE` (2).

## A2. Graph integrity: one named, permanent error

**What it is.** The committed graph is a set of records that refer to each other by id. An integrity
error means one of those references points at something that is not there, or is of the wrong kind. For
example, a seat says `template_ref = tp_X`, but the graph has no `tp_X`. The daemon is healthy and the
data is wrong. Retrying returns the same answer, so someone has to repair the graph.

**Who repairs it.** The node's manager seat (Part B): a dedicated seat, often but not necessarily the
root of the node's manager tree.

**Changes.**
- `bound_paths` (`src/bootstrap/mod.rs:137-243`): every unresolved reference raises one error kind that
  names the object, the reference and the record holding it. That covers the seat, clone, teamspace,
  shared template and application template, plus the three silent skips at `:178`, `:193` and `:200`.
  - Collapse the `missing()` and `referenced_seat_template` failures into this kind.
  - Delete the unreachable calls at `:187` and `:213`.
- `src/ipc.rs`: add `IpcErrorCode::Integrity` and return it from `seat.resolve` for this kind.
  - No IPC version bump. A stale daemon running against a new CLI after an upgrade can only produce a
    worse error message; it does not fail closed for everyone.
- The integrity reply carries the node manager seat when the graph record names one. It reads that from
  `graph.toml`, which does not depend on the broken object.
- `src/cli/seat.rs`: on `Integrity`, exit **6** with:

  > herdr-graph: graph integrity error: seat st_… references shared template tp_…, which does not resolve
  > at <head>. This is not transient; retrying will not help. Report it to your node manager seat
  > (<seat>, if named) and the user. Carry on without seat-specific rules. Do not guess your seat.

- Plain read failures (I/O errors, corrupt git objects, malformed TOML) stay `Internal`. They are a
  different fault, with repo repair rather than a graph edit as the fix.

Making the silent skips fatal enlarges the blast radius only for graphs that are already damaged. Healthy
graphs cannot reach those states.

## A3. Skill and hook

Add to step 1 of `skills/seat/SKILL.md`:

> If `herdr-graph seat --json` fails, show the user its error line verbatim, then:
> - Exit 5, daemon unavailable: carry on with what you can without seat-specific rules. Run `/seat` again
>   later. Do not try to repair the daemon unless your instructions say to.
> - Exit 6, graph integrity error: do not retry. Report it to the node manager seat named in the error
>   (if any) and to the user, then carry on as above.
> - In every case, never guess your seat.

Add one clause to `HOOK_PROMPT`: "If /seat fails, follow the skill's failure step."

## A4. Tests

- Unavailable: the message text and exit 5.
- `NoInstance`: exit 2.
- Each integrity case produces `IpcErrorCode::Integrity`, the named error, exit 6, and a reply with no
  partial data. Cases:
  - missing shared template;
  - wrong-kind shared template;
  - missing teamspace;
  - each previously silent skip.
- Plain read failures stay `Internal`.

## A5. Sequencing

The fix touches `src/cli/seat.rs`, `src/bootstrap/mod.rs`, `src/ipc.rs` and `skills/seat/SKILL.md`.
Another session is editing `seat.rs` and the skills. Land this after that work, or hand it to that
session. A2's "node manager seat" line depends on Part B. Without B, the integrity message omits it, and
A3 says "report to the user".

---

# Part B: manager seats (proposal, separate work item) — hg-au7

## Intent

Every session should know whom to escalate *lifecycle* issues to, and managers are responsible for
listening, detecting and unblocking. Lifecycle issues are:
- anything concerning the environment (graph, Herdr, tooling, filesystem, configuration);
- conflicts with peer agents;
- anything affecting the session's own ability to operate: usage limits, trust or permission prompts,
  stuck harness states, a failed `/seat`.

The hierarchy is flexible. A typical node looks like this:
- each space's sessions report to that space's main tab;
- each space main reports to the node's office main;
- the office main reports to a global coordinator on the originating node.

None of that structure is fixed by the model.

## Model

- **A manager seat is just a pointer.** An optional `manager: SeatId` reference to a seat. A seat is the
  stable identity; clones and panes come and go. How the two sides communicate is left open; a 1:1
  herdr-threads pair is the likely default. The graph does not create or maintain a dedicated thread.
- **Where it can be set.**
  - On a seat.
  - On a teamspace, as the default for that teamspace's seats.
  - Effective manager = seat override, else teamspace default, else the node manager.
- **Node manager (decided).** A graph-level `node_manager_seat: SeatId` in `graph.toml`. There is
  precedent: `summarizer_seat` already lives there (`src/model/graph.rs:12`).
  - It is the root of everything on the node.
  - It has no manager.
  - It owns graph repair and receives integrity errors.
  - Any hierarchy above node level (for example a global coordinator on the originating node) is
    outside the graph.
  - **Assignment.** The first seat created on the node becomes the node manager, and the plan that creates
    it records `node_manager_seat`. The shipped graph templates provide the defaults that create that first
    seat with the node-manager role.
  - **Unassigned graphs.** Graphs created before this rule have no node manager, and nothing is picked
    automatically. While `node_manager_seat` is unset, every graph command prints a warning on stderr
    (so `--json` stdout stays clean) saying a node manager should be assigned, and naming the command
    that assigns one.
  - **Assigning and moving the role.** One command, through a normal plan (proposed:
    `herdr-graph plan node-manager set <seat>`), both assigns the role when it is unset and moves it to
    another seat. On a move:
    - the old node manager's immediate reports become reports of the new node manager. That covers
      explicit `manager` pointers and teamspace defaults naming the old node manager. Seats that reached it
      only through the fallback follow automatically.
    - the old node manager itself becomes a report of the new node manager. It is now an ordinary seat:
      it can be retired and is no longer kept open.
    - the new node manager's own `manager` pointer is cleared. Its existing reports stay with it. Since it
      now has no manager, no cycle can form, even if it was a descendant of the old node manager.
    - the plan shows every rewrite.
  - **Permanence.** The node manager seat cannot be retired while it holds the role. Plans that would
    retire it are refused: retiring its teamspace or application, or undoing its creation. Move the role
    first.
  - **Always open.** The node manager keeps its own tab and stays open. If that tab or pane is closed,
    reconciliation reopens it in the same place instead of retiring the seat. This is an explicit
    exception to "direct tab/workspace closure follows containment retirement" (CURRENT-DESIGN.md). It
    fits Herdr, where a machine with no tabs is an awkward state anyway.
- **How it changes (decided).** All graph state changes go through the plan/confirm/apply flow
  (CURRENT-DESIGN.md), so the manager pointer is set and cleared through plans like every other field.
  There is no separate lightweight command.
- **Validation at plan time**, like other references:
  - the target exists and is not retired;
  - no cycles along manager chains;
  - the node manager has no manager.
- **Retirement (decided).** When a seat is retired, its direct reports are reparented to the retired
  seat's own effective manager:
  - Explicit `manager` pointers on the reports are rewritten.
  - A teamspace default that names the retired seat is rewritten the same way.
  - The plan shows every rewrite.
  - If the parent is retired in the same plan (for example, an application retire), reports go to the
    nearest surviving ancestor. The node manager cannot be retired, so this always ends at a valid
    seat, and "everyone becomes their own root" cannot happen.
- **Surfacing (decided).**
  - `/seat` returns `manager` and `node_manager`, each as a seat id and name, plus that seat's live
    clone's herdr-threads seat when resolvable, so the agent can message it directly.
  - The integrity error carries `node_manager` (A2).
  - `who` and `show` display the manager chain up to the node manager.

## What this deliberately does not do

- No daemon-free path to the manager. When the daemon is down, sessions carry on (A1). They do not need
  to look up their manager through the graph.
- No prescribed transport or thread topology.
- No manager-side tooling. Detecting a stuck report (usage limit, trust prompt) is the manager's job. Many
  of those states leave the report unable to self-report, so managers need to watch pane state, not only
  messages. That belongs in manager instructions, not in graph.

## Open questions

1. **Until hg-au7 lands**, the integrity error omits the node manager, and the A3 step says "report to
   the user".

---

## Remaining open questions for Part A

- **Existing daemon-free fallbacks.** `who`, `status`, `ops`, `request` and `undo` still read the
  committed graph directly when the daemon is down (`src/cli/who.rs:49`, `status.rs:85`, `ops.rs:97`,
  `request.rs:147`, `undo.rs:45`). That is inconsistent with "the daemon is authoritative".
  - `status` is defensible as an operator diagnostic.
  - The others are candidates for removal.
  - This is a separate follow-up, outside H3: Beads hg-3mc.
