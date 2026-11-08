# Final review, cluster 6 (skills), epic hg-66u

Invocation: inv-4aa37d70-023c-483a-b47c-3dc801a586fe. Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify <sha>^{commit}`; review source bound in the ledger before writing). Caller checkout observed at HEAD 6576a11 with a clean `git status --porcelain`; no worktree was needed, all reads used `git show <head>:<path>`.

## Scope

Two files, slices `000053.diff` and `000054.diff`:

- `skills/graph/SKILL.md`: the change list gains `clone purge` (`clone add|retire|purge|rebind`). Commit 18a43c5.
- `skills/seat/SKILL.md`: step 4 no longer says to run each `accept_command`; required invitations defer to herdr-threads' ready `accept-required` command or its required-invitation procedure, ordinary invitations are decided by topic and goal. Commit 95e216b.

Both files are compiled into the binary (`src/bootstrap/setup_claude.rs:25-26`, `include_str!`) and installed by `herdr-graph setup claude`, so the skill text is the production surface an agent reads.

## Analysis against the spec

- Spec §4 (H2 Option D, hg-s6o) prescribes the step-4 text: "required → run herdr-threads' ready `accept-required` command from its hook; if absent, follow herdr-threads' required-invitation procedure (`thread participants` row → `accept-required`, reread on stale); ordinary → decide by topic and goal." The new step 4 matches that structure line for line.
- The JSON the skill tells the agent to read matches the head: `PendingInvitation` is `{thread, constraint}` only (`src/threads/mod.rs:195-198`), `InviteConstraint` serializes snake_case as `required` / `ordinary` (`src/model/clone.rs:99-104`), so the skill's `constraint: required` / `constraint: ordinary` labels are exact. `render_seat` prints "pending invitations (accept through herdr-threads)" with `thread [constraint]` and no command (`src/bootstrap/mod.rs:456-467`). Tests assert `accept_command` is absent (`src/bootstrap/tests.rs:712`, `:3348`). No other copy of the old "Each entry has an exact accept_command" text survives outside historical review docs.
- herdr-threads at the pinned submodule commit 95e9eba does print a ready `accept required: herdr-threads accept-required THREAD --invitation … --requirement … --revision N` line in its hook block (`src/harness/mod.rs`, docs/agent-usage.md "Ready commands"), and emits its own D2 procedure line (`REQUIRED_INVITATION_INSTRUCTION`) when a required invitation is pending. The skill's primary path therefore has a real producer.
- Fallback wording: the skill says to read the row with `herdr-threads thread participants <thread>` and "run the `accept-required` command it gives". At 95e9eba, `thread participants` prints the values only (`SEAT STATE … [self] required invitation=ID requirement=ID revision=N`, `participant_row` in `src/protocol/output_compact.rs:787-820`); it does not print an `accept-required` command line. Commands are printed by `inbox` (`inbox_rows`, `:700`) and the invitation-detail view (`:268`). herdr-threads' own instruction phrases it correctly ("read the current requirement ID, invitation ID and revision in thread participants, then explicitly use accept-required with those exact values"). An agent following the skill literally finds no command to copy; a capable agent will compose it, and the hook's D2 line is present in the same context, so this is a wording defect, not a broken path.
- The fallback command uses the bare `herdr-threads` prefix. herdr-threads adds `--state-dir`/`--host-endpoint` to its ready commands when the pane would not reach the hook's instance by default; the bare fallback could read the wrong instance in that configuration. It applies only when no ready command was shown, which is the same case where the prefix cannot be learned from the hook, so this is a narrow residual.
- `clone purge`: registered as an org kind with words `("clone", "purge")` (`src/plan/core_kinds.rs:49`, `:1838-1930`), so `herdr-graph plan --json clone purge <clone>` is reachable through the same plan/confirm/apply flow the graph skill describes. This satisfies spec §14.1 ("plan/confirm like other mutations"). `clone rebind` stays in the list; it still exists at head (`src/bootstrap/resolve.rs:324`), consistent with the spec deferring matcher/rebind removal behind the D1 gate. Likewise the seat skill's step 2 still describes rebind proposals, which `seat.resolve` still produces (`src/bootstrap/mod.rs:233-240`); that text is accurate for this head.

## Requirement baseline trace

- B11 (H2 Option D: /seat stops printing accept commands, acceptance left to herdr-threads): delivered in this cluster's part. Production entry point `herdr-graph seat --json` / the installed `/seat` skill: the reply carries no accept command and the skill routes acceptance to herdr-threads. The daemon/bootstrap side of B11 is in another cluster; it was checked here only to confirm the skill matches it.
- B7 (undoable retirement, with purge as the end of the undo window per §14.1): this cluster only documents the `clone purge` plan kind. Whether purge's tombstone/finalize path is delivered end to end is for the plan/threads clusters and the seam reviewer.
- Other baseline rows (B1-B6, B8-B10) have no surface in the skill files.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test checks the skill text against the herdr-threads output it cites; `src/bootstrap/tests.rs:1004-1007` only asserts the installed copy equals the embedded file, so drift in herdr-threads' `thread participants` or ready-command format would not be caught.

## Deferred OK
- skills/seat/SKILL.md step 4 says `thread participants` "gives" an `accept-required` command, but at herdr-threads 95e9eba it prints only `invitation=… requirement=… revision=…` values; reword to "use `herdr-threads accept-required <thread> --invitation … --requirement … --revision …` with those exact values" (or point to `herdr-threads inbox`, which does print the command).
- The skill's fallback uses the bare `herdr-threads` prefix, which can miss the instance when herdr-threads would have prefixed `--state-dir`/`--host-endpoint`; narrow residual limited to the no-hook-output case.
