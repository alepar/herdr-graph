# Final review: cluster 14 (src-plan), epic hg-66u

Source: base 0a84c67554aa3fc8e8caf20db4163683ab30af61, head 6576a117e595097ead48ca7178935cf039916735 (both verified with `git rev-parse --verify`). Caller checkout observed at HEAD 6576a11, clean, before and during the review; nothing was built or run. The review is static, from `git show` at head.

Cluster files: src/plan/core_kinds.rs, src/plan/tests.rs (slices 000073, 000074).

## What the cluster changes

- New `ClonePurge` plan kind (`clone purge <clone>`, spec §14.1-3). It resolves the clone by id, or by name among retired clones and then among live ones. It refuses a clone that is not retired ("retire it first") or that is already a tombstone ("already purged"). It plans one `clone.purge` effect with a warning that the undo window ends. The mutate step re-checks the same facts, then either writes the `purge_tombstone` record (`Finalize{seat}` for a seated clone) or deletes `clone.toml` for a seatless clone. `action: None`, so the purge itself cannot be undone, as intended.
- `do_retire_seat` and `CloneRetire` call `clear_restore_on_retire` (spec §13, §14.6).
- `do_resurrect_seat` skips tombstones (spec §13: tombstones are inert).
- `new_clone_record` and the test fixture gain the three new CloneRecord fields. The plan test fixture registers the threads mutations so `admit_threads_seat` works in plan tests.
- Six new tests cover: a seated purge producing a Finalize tombstone, a seatless purge removing the record, refusal for an active clone (by id and by name), refusal for a tombstone, seat resurrect skipping a purged clone while bringing back its sibling, and `clone_status` listing only retired-unpurged clones.

## Requirement baseline trace (cluster entry points)

- B7 / B10 / B1, from the `seat resurrect` entry point (`do_resurrect_seat`, core_kinds.rs ~l.503-547). Spec §8 and §14.5 require the resurrection commit to write `Restore{seat, epoch}` for a seated clone. The succession or unretire want and the resume driver then put the clone back on the same (or succeeded) seat. At head, `do_resurrect_seat` sets the clone Active, clears `retired`, `occupant` and `runtime`, and writes no intent. `write_restore_intent` (src/threads/assignment.rs:56) has no production caller anywhere in src/. For a resurrected seated clone this has three effects. `resolvable_pane` (src/threads/effects.rs:370) returns None because `threads_seat.is_some()`, so the reopened pane's seat is never bound. No succession want exists, because hg-66u.29 is open. `membership_seat` (effects.rs:102) keeps the clone's memberships on the old final-retired seat. So resurrection does not restore the clone onto a working seat, and participation follows a dead seat, which breaks B1's 1:1 mapping for that clone. The pre-D2 interim branch (§8 "Before D2") is not gated on hg-66u.2. Its owner, hg-66u.29, is open. The undo entry point (src/undo) has the same gap, but it is outside this cluster and belongs to the seam reviewer. The unretire branch (hg-66u.32) is legitimately gated on D2 (hg-66u.2 open) and goes in the hand-back as open.
- §14.1 purge as an operator command: delivered through the production plan path (`create_plan` with words, the same path the CLI uses). The `clone status` listing is wired through `clone_status`.
- §14.3 ReleaseKey tombstone: both call sites pass `reservation: None` ("hg-66u.13 passes the recorded key reservation"). CloneRecord has no reservation field at head, and the D1 gate (hg-66u.1) is open, so no reservation exists to release yet. That is consistent with "never fake an unreleased feature". The owning work (hg-66u.43) must change both call sites together.

## Code observations

- Pre-D2 tombstones have no consumer. §8 says a clone whose seat is already final (every seat before D2) "is purged at once, with no intent". §14.2's always-tombstone rule is stated for "after D2". At head, purge writes `Finalize{seat}` for every seated clone, but the finalize want is D2-gated (hg-66u.31 open). So tombstones written now stay until D2 lands. The finalize want accepts "a read showing the seat final" as acknowledgement, so they will drain then. They are inert meanwhile (excluded from membership, resurrection, status and undo preview), so the practical cost is stale records and the name collision below.
- Name resolution sees tombstones. `facts` resolves names with `grammar::resolve_clone(.., Scope::Retired)`, and that filter does not exclude tombstones. Example: clone `second` is purged into a tombstone, then a new `second` is added and retired. `clone purge second` then fails as ambiguous, and the operator must use the id. Purge is the only `Scope::Retired` clone caller at head.
- Plan and mutate both call `facts` and `purge_tombstone`, so a lifecycle change between them is re-checked at mutate. `relied_on` carries the clone rev, so the writer's revision check also covers drift.

## Must fix before landing
- B7/B10 (and B1 for restored clones) not delivered from the `seat resurrect` entry point: `do_resurrect_seat` (src/plan/core_kinds.rs ~l.503-547) writes no `Restore` intent, and `write_restore_intent` (src/threads/assignment.rs:56) has no production caller. A resurrected seated clone therefore keeps its final-retired `threads_seat`, gets no resolve want (`resolvable_pane`, src/threads/effects.rs:370, skips seated clones), no succession want (hg-66u.29 open, not gated) and keeps memberships on the dead seat (`membership_seat`, effects.rs:102). No human scope change covers this. The pre-D2 interim succession is required now; only the D2 unretire branch (hg-66u.32) may wait on gate hg-66u.2.

## Untested scope
- `clear_restore_on_retire` in `do_retire_seat` and `CloneRetire` (src/plan/core_kinds.rs:424, 1811) has no plan-level test: no test retires, by `clone retire` or `seat retire`, a clone that holds a Restore intent and checks that the intent is cleared. Only the observe-path retire is tested (src/threads/tests.rs:2168).
- `clone purge <name>` resolving a retired clone by name is not tested; every success-path test passes the id.
- No test covers name ambiguity between a retired clone and a same-named purge tombstone.
- The ReleaseKey branch of purge is unreachable at head (reservation hard-coded to None), so it has no test through the plan kind.
- No test covers a concurrent lifecycle change between plan and apply (for example, the clone resurrected after the purge plan was made).

## Deferred OK
- Pre-D2 purge writes `Finalize{seat}` tombstones that nothing consumes until the D2 finalize want (hg-66u.31) lands. §8 says pre-D2 final seats are purged at once with no intent. The tombstones are inert and will drain once that want treats a final read as acknowledgement, so hg-66u.31 can absorb this.
- `ClonePurge::facts` name resolution matches tombstones (`Scope::Retired` does not filter `is_tombstone`), so a reused clone name can make `clone purge <name>` ambiguous. The workaround is to pass the clone id; filtering tombstones in the retired-name lookup is the small fix.
- Both `purge_tombstone(&clone, None)` call sites (src/plan/core_kinds.rs ~l.1892, ~l.1923) must switch to the recorded reservation together when D1 reservation work (hg-66u.43, gated on hg-66u.1) lands.
- The D2 unretire branch of resurrection and undo (hg-66u.32) waits on gate hg-66u.2 and goes in the hand-back as open.

## Verdict
not ready (seat resurrect writes no Restore intent; B7/B10 restore onto the same seat undelivered, hg-66u.29 open)

## Must fix before landing
- B7/B10 (B1 for restored clones): `do_resurrect_seat` (src/plan/core_kinds.rs ~l.503-547) writes no Restore intent and `write_restore_intent` has no production caller, so a resurrected seated clone keeps its dead final-retired seat with no resolve or succession want and memberships stay on the dead seat; the pre-D2 interim (hg-66u.29, ungated) must land.

## Untested scope
- `clear_restore_on_retire` in `do_retire_seat` and `CloneRetire` has no plan-level test (retiring a clone that holds a Restore intent).
- `clone purge <name>` success path by name is untested.
- Name ambiguity between a retired clone and a same-named purge tombstone is untested.
- The ReleaseKey purge branch is unreachable at head (reservation None), so it is untested through the plan kind.
- A lifecycle change between purge plan and apply is untested.

## Deferred OK
- Pre-D2 purge writes Finalize tombstones with no consumer until the D2 finalize want (hg-66u.31); they are inert and drain once it lands.
- Purge name lookup (`Scope::Retired`) matches tombstones, so a reused name can be ambiguous; passing the id works around it.
- Both `purge_tombstone(&clone, None)` call sites must take the recorded reservation together when hg-66u.43 (D1, gated on hg-66u.1) lands.
- The D2 unretire branch of resurrection and undo (hg-66u.32) waits on gate hg-66u.2 and goes in the hand-back as open.
