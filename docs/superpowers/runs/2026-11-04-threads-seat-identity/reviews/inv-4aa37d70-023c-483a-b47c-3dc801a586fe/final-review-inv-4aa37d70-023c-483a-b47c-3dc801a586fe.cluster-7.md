# Final review, cluster 7 (src-bootstrap), epic hg-66u

Invocation: inv-4aa37d70-023c-483a-b47c-3dc801a586fe. Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify <sha>^{commit}`). The review source was bound with `dispatch-text.mjs bind-review-source`, which exited 0. The caller checkout was at head 6576a117 with a clean status before and after the review.

## Scope

- The files are src/bootstrap/mod.rs and src/bootstrap/tests.rs, in slices 000055 and 000056.
- The change in mod.rs is to `render_seat`. The pending-invitations header now reads "pending invitations (accept through herdr-threads):" and each row is `<thread> [<constraint>]`. The `accept_command` column is gone.
- The tests.rs changes:
  - `seat_json_includes_paths_pending_ops_invitations_beads_view_rev` now asserts that the JSON entry has `constraint == "required"` and no `accept_command` key. It also asserts the exact rendered line and that the text contains no `herdr-threads accept`.
  - `h1_untested::seat_skill_has_instruction_steps` now asserts that skills/seat/SKILL.md mentions `accept-required` and `thread participants` and does not mention `accept_command`.

## Spec and baseline trace

- Governing text: design section 4, "/seat stops printing acceptance commands (H2 Option D, hg-s6o)". It says `pending_invitations` reports only `thread` and `constraint`, `accept_command` and its formatter are removed, and `render_seat` and the tests are updated.
- Baseline row B11 ("Option D only stops /seat printing accept commands and leaves acceptance to herdr-threads") is the row this cluster serves. Rows B1 to B10 have no surface in this cluster.
- I traced B11 at head from the production entry point:
  - `herdr-graph seat` in src/cli/seat.rs calls daemon `seat.resolve`.
  - That reaches bootstrap `seat.resolve` (src/bootstrap/mod.rs about line 276), which calls `crate::threads::pending_invitations`.
  - That builds `PendingInvitation { thread, constraint }` (src/threads/mod.rs about line 195), and the struct has no accept field.
  - For text output, src/cli/seat.rs then prints `render_seat(&reply)`. For `--json`, it pretty-prints the reply.
  - Neither path can emit an accept command any more. B11 is delivered on both the JSON and the text path.
- The producer side (src/threads/mod.rs) and the SKILL.md step 4 rewrite are outside this cluster. I opened them only to trace B11. SKILL.md step 4 matches the wording in section 4.
- The unbound and ambiguous branch still returns `json!([])` for invitations, and `render_seat` skips empty arrays. That behaviour is unchanged.

## Verification run

- I ran the tests in a review-owned detached worktree at head, /var/home/alepar/AleCode/herdr-graph/.worktrees/review-c7-inv-4aa37d70, with the submodule at 95e9ebac (v3.0.0) and target/ seeded by reflink.
- Command, inside the ht-build toolbox: `cargo test --lib -- bootstrap:: threads::tests::pending_invitations`. Result: 71 passed, 0 failed.
- Passing tests include `seat_json_includes_paths_pending_ops_invitations_beads_view_rev`, `h1_untested::seat_skill_has_instruction_steps` (covered by the bootstrap:: filter) and `threads::tests::pending_invitations_report_thread_and_constraint`.

## Observations

- The change is minimal and matches the spec exactly. The new header tells the agent where acceptance lives without naming a command.
- The rendered-line assertion pins the exact text format. That is intentional here, because the text is the agent-facing contract.
- Outside this cluster, the doc comment on `ThreadsLink` in src/model/clone.rs (lines 81-82) still says the link exists "to print the exact `accept-required` command (spec §7.1)". Since Option D, graph no longer prints that command, so the comment is stale. It has no behavioural effect. The owning cluster's reviewer can judge it.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No CLI-level test runs `herdr-graph seat` (text mode) end to end through the daemon. The rendered output is covered only by calling `render_seat` on a `seat.resolve` reply inside the unit test, and the CLI wrapper is a direct `print!` of that result.

## Deferred OK
- The stale doc comment on `ThreadsLink` (src/model/clone.rs:81-82, outside this cluster) still says the link exists to print the exact `accept-required` command, which Option D removed. It is a docs-only fix for the owning cluster.
