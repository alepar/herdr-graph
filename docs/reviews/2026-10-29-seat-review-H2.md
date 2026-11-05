# H2 — Required invitations: `/seat` can hand the agent a lookup instead of an answer

Repo state reviewed: `main` @ c77d9ad (read-only; no edits made).

## Abstract

herdr-graph places required thread memberships on a clone's behalf, but only the clone's own agent can
accept one. Acceptance needs three identifiers that only herdr-threads issues: invitation, requirement
and revision. `/seat` reports each pending invitation with a field called `accept_command`. That field
was meant to be an executable answer. When the graph has not yet copied the identifiers into the
clone record, it holds a read-only query (`herdr-threads thread participants <thread>`) instead. The
seat skill tells the agent to run these commands "exactly as printed" and gives no next step after
that. So the agent runs the query and the invitation stays pending until its deadline warning fires.
The identifiers that are present come from the last committed clone record, not live service state.
They can therefore go stale, and they can disagree with the live `accept-required` command that
herdr-threads' own hook puts in the same context. The result is a membership the graph requires but
cannot rely on, and a skill whose instructions contradict the transport's own. The fix is to stop
calling a lookup a command, and to have one authority for acceptance identifiers, preferably the
live service. A precise skill procedure covers whatever gap is left.

## Verified facts

| Claim | Evidence | Verdict |
|---|---|---|
| Missing ids produce a read command in `accept_command` | `src/threads/mod.rs:197-219` (`_ => format!("herdr-threads thread participants {thread}")`); pinned by `src/threads/tests.rs:664-667` | Confirmed |
| Skill says run required commands exactly as printed, no follow-up | `skills/seat/SKILL.md:42-43` | Confirmed |
| Text renderer also frames them as "run the commands" | `src/bootstrap/mod.rs:464-478` | Confirmed (also affects the non-JSON path) |
| `pending_invitations` reads the committed clone record, not live threads | `src/threads/mod.rs:223-240`; `seat_reply` reads `store.head()` via `CommitView` (`src/bootstrap/mod.rs:259-263`) | Confirmed |
| When ids are missing | After `invite`, `membership_detail` is read once and errors are dropped (`.ok().flatten()`, `src/threads/effects.rs:1214-1230`). A voluntary-only answer carries no ids (`detail_of`, `src/threads/adapter.rs`). The poll effect fills them in later, only via an admitted bookkeeping commit (`effects.rs:1370-1405`, `admit_bookkeeping` at `:1034`) | Confirmed. The gap is a window that closes on a successful poll, not a permanent state |
| Plain `accept` on a pending required invitation fails | `third_party/herdr-threads/src/store/control/invitation.rs:97-108` (`StaleRequirementAcceptance`); `join` fails with `MembershipRequired` (`store/control/membership.rs:107-113`) | Confirmed: `accept-required` with current ids is the only way in |
| `accept-required` with an old revision fails | `invitation.rs:~385-400` (`check_current`, tolerance only for a repeat of an already-accepted episode) | Confirmed |
| herdr-threads' hook already emits a live `accept-required` | `third_party/herdr-threads/src/harness/mod.rs:335-420` (`next_actions`, from the live attention digest) plus `REQUIRED_INVITATION_INSTRUCTION` (`:183`) | Confirmed. The ready-command block is budgeted, so it is not guaranteed for every invitation |
| Ordinary invitations are listed but the skill ignores them | `accept_command` returns `herdr-threads accept {thread}` for ordinary; `SKILL.md:42-43` covers required only | Confirmed |

Correction to the original finding: the record is not merely "lagging" in general. The ids are
written once at invite time (if the read succeeds) and refreshed by polling. Staleness needs either a
failed or empty first read, or a service-side revision advance that has not yet been polled back. Both
can happen, and nothing at `/seat` time checks for them.

## Options

### Option A — Skill-only procedure (the review's suggested step 5)

- **Change:** rewrite `skills/seat/SKILL.md` step 5. Prefer herdr-threads' hook `accept-required` for
  that thread. Otherwise run `accept_command`. If that was a `thread participants` lookup, read the
  `required invitation=… requirement=… revision=…` row and run `accept-required`. Reread and retry
  once on `stale_requirement_acceptance`. For `ordinary`, decide by topic and goal and do not accept
  automatically.
- **Pros:** no code change. Does not touch `src/cli/seat.rs`, which another session is editing. Ships
  immediately.
- **Cons:** the agent has to follow a multi-branch procedure. The field is still misnamed, so the
  text output (`bootstrap/mod.rs:464-478`) still says "run the commands". Ids still come from the
  committed record.
- **Cost:** about 15 lines of skill text.
- **Residual risk:** medium. Correctness depends on the agent noticing a command-shape distinction.

### Option B — Make the contract honest: structured action instead of `accept_command`

- **Change:** in `src/threads/mod.rs`, replace `accept_command: String` with
  `action: "accept_required" | "lookup" | "decide"`, plus `command` (only when executable) and `next`
  (for lookup: "read the required row in `thread participants <t>`, then run `accept-required`").
  Update `render_seat` in `src/bootstrap/mod.rs:464-478`, the tests in `src/threads/tests.rs:623-670`
  and `src/bootstrap/tests.rs:679-702`, and skill step 5.
- **Pros:** the agent can no longer mistake a read for an answer. Ordinary invitations explicitly
  become "decide". The text and JSON outputs agree.
- **Cons:** this changes the `/seat` JSON schema (`/seat` is consumed only by the skill). Ids are
  still taken from the committed record, so stale-revision failures remain.
- **Cost:** small, about half a day including tests.
- **Residual risk:** low to medium. Stale-revision failures remain and are recoverable with the
  skill's retry step.

### Option C — Resolve live at `/seat` time

- **Change:** add `threads: Option<Arc<dyn ThreadsPort>>` to `BootstrapDeps` (`src/bootstrap/mod.rs:38`,
  wired in `src/daemon/compose.rs:366-372`). For each pending required invitation in
  `seat.resolve`, call `membership_detail` with a short timeout and build `accept-required` from the
  live ids. Fall back to Option B's `lookup` action when the service is unreachable. Combine with B.
- **Pros:** ids are current when the agent reads them, which removes both the missing-id window and
  the stale-revision window in the normal case. `/seat` and the threads hook agree.
- **Cons:** `seat_reply` is synchronous inside `spawn_blocking` and today reads only the store. This
  adds an async service dependency and a latency and failure mode to session start. It also
  duplicates what the herdr-threads hook already computes. It does not write back, so the record
  still lags until the poll runs.
- **Cost:** medium, about 1 day. It needs fake-port tests and a timeout policy.
- **Residual risk:** low for correctness. It adds a new startup-path dependency.

### Option D — Delegate acceptance to herdr-threads; `/seat` only reports

- **Change:** `pending_invitations` keeps `thread` and `constraint` and drops `accept_command`.
  Skill step 5 becomes: "for each required thread, use herdr-threads' ready `accept-required`
  command. If it is absent (budget trimmed), follow herdr-threads' required-invitation procedure
  (`thread participants` row → `accept-required`, reread on stale). For ordinary threads, decide per
  the herdr-threads skill." Remove the `accept-required` formatting from `threads/mod.rs` and
  update the tests. `ThreadsLink` ids stay for release bookkeeping (`effects.rs:1271-1283`).
- **Pros:** one authority for acceptance ids: the live service and its own hook and skill, which
  already carry the exact procedure (`REQUIRED_INVITATION_INSTRUCTION`). Removes a staleness class
  instead of narrowing it. Smallest long-term surface.
- **Cons:** `/seat` stops being self-contained, so the agent must use the herdr-threads guide. A
  pane without the herdr-threads hook loses the convenience command, but such a pane cannot accept
  anyway without the CLI.
- **Cost:** small, a few hours, mostly deleting code and tests.
- **Residual risk:** low. The remaining risk is the hook budget trimming the ready command, which
  the herdr-threads procedure already covers.

## Recommendation

**Option D, with the step-5 wording from A adapted to it.** herdr-threads already computes the
correct live command and documents the fallback procedure. herdr-graph re-deriving it from a
committed snapshot is the root cause of both the misleading lookup and the stale-revision risk. A
second, staler source of the same identifiers is a liability, not a convenience. If the team wants
`/seat` to stay self-sufficient for acceptance, choose **B + C** instead: an honest action field and
live ids, at roughly double the cost and with a new dependency on the startup path.

Sequencing: apply the skill change (A or D's wording) first. It touches only `skills/seat/SKILL.md`
and can land after the concurrent `src/cli/seat.rs` and skills session finishes. The code change in
`threads/mod.rs` and `bootstrap/mod.rs` is independent of `src/cli/seat.rs`.

Follow-up worth noting, outside H2's scope: the one-shot `membership_detail` read after `invite`
swallows errors (`effects.rs:1214-1220`). That is fine given the poll, but a `Deferred` or debug
note would make the id gap visible in `status`.
