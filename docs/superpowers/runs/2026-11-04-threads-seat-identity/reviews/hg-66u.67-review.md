# hg-66u.67 review: cluster 4 (runs-top) and inv-4aa37d70 gap paths

## Part A (hg-66u.71): design documents

Head reviewed: a1bb8ab949950ee9a4c818297cad4852674d3e2f. Review set: the eleven human-authored design documents of cluster 4 (`runs-top`), all under `docs/superpowers/runs/2026-11-04-threads-seat-identity/`; the other fourteen `runs-top` paths (coverage files, ledger designs, friction-adjacent run records, profiles, run.md) belong to Part B. `clusters.json` lists no human-authored design document that neither part names.

1. `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md`
2. `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-05-threads-seat-identity-roast-design-1-roast-design-1-step-back.md`
3. `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-05-threads-seat-identity-roast-design-1-roast-design-1.md`
4. `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-05-threads-seat-identity-roast-design-1-step-back-keys.txt`
5. `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-05-threads-seat-identity-roast-design-2-format-repair.md`
6. `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-05-threads-seat-identity-roast-design-2.md`
7. `docs/superpowers/runs/2026-11-04-threads-seat-identity/amend-design-1.md`
8. `docs/superpowers/runs/2026-11-04-threads-seat-identity/settled-tree-roast-1.md`
9. `docs/superpowers/runs/2026-11-04-threads-seat-identity/settled-tree-roast-2.md`
10. `docs/superpowers/runs/2026-11-04-threads-seat-identity/threads-ht-ma6i-protocol-shapes.md`
11. `docs/superpowers/runs/2026-11-04-threads-seat-identity/friction.md`

### Outcomes

Findings (each also appended to `findings-ledger.md`):

- [Should-fix] docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md:232 — §7 rests tab/workspace matching on "Herdr preserves them across restore and handoff" and the Problem description says "Herdr v0.9.5 preserves public workspace/tab/pane ids", with no source and no recorded disposition of the design-roast-1 escalations. Evidence: roast-design-1 Escalations list this as UNVERIFIED external and record a material dissent that Herdr `reserve_workspace_ids` lets a deleted highest-numbered workspace id recur; `archive/HERDR-IDENTITY-CHECK.md:19` says the same ("deleted higher workspace addresses can consequently recur"); `docs/herdr-spikes.md` Spike 3 ran on 0.9.1 and did not report tab ids. Equality on a recurred id binds a seat to the wrong workspace silently, the failure the removed vote was meant to avoid. No spec text qualifies the claim, and no bead measures it: hg-66u.11 tests with fakes only and hg-66u.17 asserts clone outcomes, not tab/workspace id stability.
- [Should-fix] docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md:459 — §14.12's key-link want fires "for every active clone with a recorded reservation and a `threads_seat` whose reservation is not recorded as bound to that seat". A clone with a pending `Restore` intent (active, `threads_done` false) whose `threads_seat` is the final-retired predecessor satisfies that predicate before succession, and the threads protocol refuses a seat-bearing `ReserveContinuityKey` for a retired seat (`docs/superpowers/runs/2026-11-04-threads-seat-identity/threads-ht-ma6i-protocol-shapes.md:33`, "Refused for a retired, named or hc seat"). The spec does not exclude retired or pending-Restore clones from the want, and bead hg-66u.68 (open) states "During a pending Restore it targets Restore.seat", which is the same retired seat. Result: a refused command each step (or an alert), not the "no retry storm" the bead promises for conflicts only. Fix shape: the want requires the committed `seat_fact` to read Resolved or Unresolved and no Restore awaiting `threads_done`.
- [Nit] docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md:392 — §14.2 says before D2 "purge deletes the clone record outright, seated or not", while §14.3 writes a `ReleaseKey{key}` tombstone for a never-seated clone with a recorded reservation, and reservations exist from hg-66u.43 (D1) which lands before hg-66u.31 (D2). Read literally, the seatless-with-reservation clone leaks its key before D2. Bead hg-66u.57 resolves it ("the ReleaseKey arm is unchanged") and `SeatIntent::ReleaseKey` exists in `src/model/clone.rs:83`, so only the spec wording is stale.
- [Nit] docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md:184 — §5 still says pre-D1 clones are backfilled "through a seat-bearing `ReserveContinuityKey{key, seat?}` … fallback: one keyed relaunch per clone" and is not marked superseded; §16.5/§16.6 and §14.12 move the backfill link to the key-link want (hg-66u.68), landing after hg-66u.31, and the "one keyed relaunch" fallback is owned by no bead.
- [Nit] docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md:378 — §13 "Shell export failure is visible" ends with "(withdrawn by §14.10: a graph retire always closes the pane)", but §14.10 withdraws the "unset the key in that shell" clause, not the status requirement; the parenthetical sits on the wrong sentence. Also the spec numbers §13, §14 and then §16 (no §15, line 472), which the header of §16 does not explain.
- [Nit] docs/superpowers/runs/2026-11-04-threads-seat-identity/threads-ht-ma6i-protocol-shapes.md:34 — `ContinuityKeyPending` (a claimless `ResolveSeat` on a pane whose key is bound to an unresolved seat) is not named in the spec: §2 lists only "restore holds, pane held for a key conflict" as the `Deferred` refusals, though it is the refusal the post-D1 resolve want will meet on a restored pane. Only hg-66u.33's text names it. No landed task codes against the provisional shapes (no `ReserveContinuityKey`, `UnretireSeat` or `FinalizeRetirement` identifier exists in `src`), so nothing breaks today.

Per document:

1. `docs/superpowers/runs/2026-11-04-threads-seat-identity/2026-11-04-threads-seat-identity-design.md` — reviewed in full. §13, §14 and §16 are consistent with each other (§14.4 is explicitly amended by §16.1; §5's backfill wording is the only unmarked leftover, see the Nit above). Named paths exist at head (`src/observe/matcher.rs`, `src/reconcile/planner.rs`, `src/threads/effects.rs`, `src/bootstrap/resolve.rs`, `skills/seat/SKILL.md`, `DESIGN-NOTES.md`, the three H2 review files, `tests/e2e_private_herdr.rs`); `src/app/agents.rs` is Herdr's own source, not this repo's; `undo/adopt.rs` was deleted by hg-66u.29 as §8 requires. Named functions exist (`purge_tombstone`, `plan_resume`, `start_needed`, `stuck_restores`, `clone_wants`, `SeatIntent`); the two old test names in §3 no longer exist because their expectations were flipped. Code divergence carried by an open bead: `Restore.launched` is a bool at `src/model/clone.rs:77-78` where §14.7/§16.7 specify a mark time (hg-66u.70). Requirements with no owning bead: none found beyond the findings above. The `## Goal` is untouched. Findings: the first five above.
2. `…roast-design-1-roast-design-1-step-back.md` — reviewed, no findings. Its six-finding cluster C1 (persisted `seat_intent`) is the design adopted in §8/§13/§14.6; C2 is resolved by §6 per-clone precedence and §2 unseated-clone paragraph.
3. `…roast-design-1-roast-design-1.md` — reviewed. All ten confirmed findings are resolved in the spec: Blocking re-retire race (§6 intent gate, §8), §9 interim trigger (§9 restore trigger), missing succession want (§2, Key abstractions), finalize-on-purge (§8 Finalize tombstone, §14.2), env delivery (§5 shell export), pre-D1 precedence (§6), unretire race (§8, "no unretire want without a Restore intent"), SeatObservation wording (§6), unseated clone (§2, §9), key-conflict alert level (§6). Its UNVERIFIED-external escalations carry no recorded disposition; the §7 id-preservation one is the first Should-fix above. The headless-restore, B8 "seat stays Resolved after exit" and 90 s grace escalations are covered by hg-66u.12 (closed) and the hg-66u.17 smoke, which reports unmet configurations as open acceptance items.
4. `…roast-design-1-step-back-keys.txt` — reviewed, no findings. Its ten keys match the ten confirmed findings of document 3 in order and wording, and the step-back's dissolves (6) plus remains (4) account for all ten.
5. `…roast-design-2-format-repair.md` — reviewed, no findings. It is byte-identical to document 6 except the removal of the trailing `---` line (the parser error recorded in `friction.md`); both agree on the headline, the twelve confirmed findings and the empty-escalation outcome.
6. `…roast-design-2.md` — reviewed. All twelve confirmed findings (ten Should-fix, two Nits) are resolved: purge command §14.1, always-tombstone after D2 §14.2, key relink §14.4/§14.12/§16.1, seatless undo §14.5/§16.3, clear-on-reattach §14.6, persisted `launched` §14.7, restart arm skips pending Restore §14.8, committed `seat_fact` §14.9, graph retire closes the pane §14.10, `ReleaseKey` tombstone §14.3, want existence §14.11, reversibility needs a bound key §14.12. Nothing unresolved without an owner except the wording Nit on §14.2/§14.3 above.
7. `amend-design-1.md` — reviewed, no findings. Its owners map the four non-cluster findings to spec sections that now state them; its seven work items were superseded by hg-66u.27-.34 and the later split beads; its "unchanged" list is carried forward in the spec's Unchanged behavior (the "launches nothing that depends on a threads answer" line is tightened by §16.8).
8. `settled-tree-roast-1.md` — reviewed, no findings. It is the pre-amendment task tree that design roast 1 consumed (hg-66u.9, .10, .12, .16, .19, .20, .22 since superseded); stale acceptance text such as the `env K=V` argv prefix in hg-66u.9 is historical, not a live requirement.
9. `settled-tree-roast-2.md` — reviewed, no findings. It is the post-amendment tree that design roast 2 consumed; its hg-66u.13 backfill acceptance (seat-bearing reserve before D2) and hg-66u.34's "key unset in that shell" clause were later corrected by §14.10 and §16.6 and moved to hg-66u.43/.44/.68.
10. `docs/superpowers/runs/2026-11-04-threads-seat-identity/threads-ht-ma6i-protocol-shapes.md` — reviewed. Present and consistent with the spec: `ReserveContinuityKey{key, seat?}` (update 2026-11-05), `ReleaseContinuityKey`, `UnretireSeat`, `FinalizeRetirement`, `ContinuityKeyPending` (correction), `continuity_key.conflict` (SeatInspection), answers on key deletion at final retirement and relink to a successor (used by §13/§16.1). Provisional shapes coded against by landed tasks: none. Findings: the key-link predicate Should-fix and the `ContinuityKeyPending` Nit above.
11. `friction.md` — reviewed, no findings. No item names a defect in this repo without a bead: the format-repair rejection and the trailing-`---` parser error are super-roast tooling, the missing C toolchain and the ready-drained re-entries are workflow/environment, and the cluster-4 window overrun is this review (hg-66u.71/.72).

Findings needing a spec edit stay open for the coordinator; this review changed no code and no spec text.

## Part B (hg-66u.72): run records and inv-4aa37d70 gap paths

Head SHA reviewed: a1bb8ab949950ee9a4c818297cad4852674d3e2f (`super-auto/threads-seat-identity`). Paths below are relative to
`docs/superpowers/runs/2026-11-04-threads-seat-identity/` unless they start with `src/`, `tests/` or `docs/`. Machine records
(`run.md`, the profile JSON, `review-inputs.json`, the provenance file) were reviewed structurally with grep, `jq`, `sha256sum`
and sampling, never read whole. Bead statuses are from `bd list --parent hg-66u --all` at review time.

### Gap list (what the inv-4aa37d70 review never covered)

Sources: `reviews/inv-4aa37d70-.../assembled/final-review-inv-4aa37d70-...-final/` (`clusters.json`, `generated-input-provenance.md`,
`review-inputs.json`), `profile-inv-4aa37d70-...md`, and the `review-collection` record in `run.md`. The bead's pointer `run.md:72` has
drifted: line 72 is the cluster-8 finding-source; the gap record is now `run.md:75` (`complete:false`, gaps
`bundle unavailable: ...cluster-4.md`). The later bounded reviews cite the same gap at `run.md:72` (head 992f07a) and `run.md:73`
(head 573ed77), so line-number pointers to it go stale on every append.

Checks that bound the gap list:

- `clusters.json` has 21 clusters. Their file lists partition the 100 `changed` paths exactly (100 files, 100 unique, `diff` against
  `changed` empty), and `diffstat.txt` lists the same 100. 100 slice files exist.
- Per-cluster reports exist for 20 of 21 clusters (1-3, 5-21). Cluster 4 has none. No other cluster is missing a report, so product code
  and tests (clusters 1-3 and 5-21 minus 4) were reviewed by inv-4aa37d70 and are not gap paths.
- `generated-input-provenance.md` lists 73 omitted historical payloads and 26 out-of-range informational entries.

| # | Gap | Paths | Disposition |
|---|-----|-------|-------------|
| G1 | Cluster 4 (runs-top) has no report | 25 run documents | 11 covered by Part A (spec, design-1 and design-2 roasts, design-1 step-back and keys, design-2 format-repair, amend-design-1, friction, settled-tree-roast-1 and -2, threads-ht-ma6i-protocol-shapes); 14 reviewed below |
| G2 | 73 omitted historical payloads | `reviews/inv-cc29fc83-.../assembled/...-final/`: clusters.json, diffstat.txt, digest.md, generated-input-provenance.md, review-inputs.json, 68 slices | Reviewed structurally, no findings (see G2 outcome) |
| G3 | 26 out-of-range entries (13 distinct, each at base and at head) | `docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md` lines 86, 91, 95, 99, 103 (invocations inv-p3b, p3c, p5r1, p5r2, p6fr) | Reviewed structurally; finding F8 |
| G4 | Cluster-4 bundle never written | `final-review-inv-4aa37d70-...cluster-4.md` | Closed by G1 (Part A plus Part B) |

### Part B cluster-4 path outcomes (14)

1. `coverage-round-1-requirements.md` - reviewed, no findings. R1-R21 present; R15-R21 are the "R-new" rows run.md:28 says were appended;
   `coverage-round-2-requirements.md` is byte-identical (diff empty), which is expected since round 2 re-used the list.
2. `coverage-round-1-review-1.md` - reviewed, no findings. Condensed copy; every owner it cites exists. Its 18 findings are the union
   entries of round 1.
3. `coverage-round-1-review-2.md` - reviewed, no findings. Its 12 findings plus the NEEDS-SPEC note for hg-66u.12 are answered by spec §9
   and cov-5/cov-15/cov-42.
4. `coverage-findings-round-1.md` - reviewed, no findings. 20 union entries map to cov-1..cov-22 (20 findings plus the two flag-sweep
   rows); each has an applied or rejected row. The one rejected row (cov-10) carries evidence.
5. `coverage-round-2-requirements.md` - reviewed, no findings (identical to round 1, see 1).
6. `coverage-round-2-review-1.md` - finding F6.
7. `coverage-round-2-review-2.md` - finding F6.
8. `coverage-findings-round-2.md` - reviewed, no findings on content: 24 entries map one-to-one to cov-23..cov-46, and every round-1
   finding is resolved in round 2 or carried (cov-10's shortfall is reopened as cov-27). Count disagreement with run.md is F5.
9. `coverage-ledger.md` - reviewed, no findings. 83 contiguous rows (cov-1..cov-83: 22 r1, 24 r2, 37 amend rounds). All 40 bead ids cited
   by the coverage files exist. Rejections cov-10, 56, 57, 72, 82, 83 each state evidence, and the claims were re-checked: hg-66u.9, .10,
   .12, .16, .19, .20, .22 are closed (cov-56); hg-66u.37, .5, .11 are closed (cov-83); hg-66u.67 is closed (split into .71/.72, cov-79).
   cov-82 ("D1 and D2 ship in one release, no window") was later contradicted by design roast r3 and settled by spec §16.6 ordering;
   hg-66u.44 is blocked by hg-66u.31 and hg-66u.68 by .44, so the order holds in the tracker. Parked rows cov-66, 68, 69 are graph-pass
   proposals about merge serialization, not requirement gaps, and need no owner.
10. `findings-ledger-design.md` - finding F2. Rows: 34 lines, 6 dissolved@r1, 4 resolved@r2, 12 resolved@r3, 8 open (r3).
11. `findings-overrides-design.md` - finding F7.
12. `profile-inv-cc29fc83.json` - reviewed structurally, no findings. `jq type` is `object`; 29 beads, 11 dispatched, 7 landed (matches
    "dispatched 11, landed 7, planned 29"), shape 29 beads/90 edges/depth 10.
13. `profile-inv-cc29fc83.md` - reviewed structurally, no findings. The code block is line-for-line identical to the JSON `summary`
    array (the md adds only a Context section). The 7 landed beads (.5 .11 .27 .25 .37 .38 .35) are all closed in the tracker.
14. `run.md` - findings F1, F3, F4, F5 (119 lines; record kinds: 44 finding-source, 13 review-collection, 3 graph-pass, 1 top-split, 1
    root-acceptance; 43 `final-review` and 1 `roast` artifacts). The `parked:` list (run.md:82-95) was checked item by item:
    - PR-roast escalation, id reissue (run.md:83) and design-roast material dissent (run.md:93): F1.
    - PR-roast escalation, real-threads qualification absent (run.md:84): owned by hg-66u.17/.18 behind gate .36 (open), as the record says.
    - Design-roast UNVERIFIED externals: 90 s grace vs auto-resume latency, B8 seat stays Resolved after agent exit, ids preserved across
      restore/handoff on v0.9.5, headless restore of bare-shell panes (run.md:91, 92, 94, 95): each is exercised by an acceptance line of the
      rig smoke hg-66u.17 (handoff with an exited agent and a bare pane, cold restart with a client attached, headless cold restart),
      which records an unmet configuration as an open hand-back item. Owned.
    - Run-failure degraded verdicts (run.md:85, 86): F4.
    - Cluster 4 unreviewed (run.md:87): closed by this review and Part A.
    - Graph-pass proposals (run.md:88-90): moot or non-requirements (hg-66u.13 is closed).
    Spot-checks of claims about code at head (five required, eight done):
    1. `tests/instruction_writers.rs:126-127` inventories `src/plan/core_kinds.rs` with its reason (the ledger's "disputed: stale" claim): true.
    2. `ClonePurge` at `src/plan/core_kinds.rs:1843` and its `delete_file` at :1927 (profile/ledger claim hg-66u.35 added it): true.
    3. `purge_tombstone` keeps a `Finalize{seat}` for a seated clone (`src/threads/assignment.rs:80-82`), as the open bead hg-66u.57 says: true.
    4. `HERDR_THREADS_CONTINUITY_KEY` is defined at `src/model/launch.rs:9` and read in `src/config.rs:58` (hg-66u.37): true.
    5. `plan_resume` exists at `src/reconcile/planner.rs:875` and gates the restart trigger on `seat_fact.read_at >= changed` (:917), which is
       spec §16.8 (hg-66u.30): true.
    6. `AckRestore` effect exists (`src/threads/effects.rs:269`, :496, :1864), which is spec §16.4's restore-ack (hg-66u.29): true.
    7. `LiveIndex::bound_pane` (`src/reconcile/planner.rs:173`) resolves a bound pane id by id alone; `src/observe/matcher.rs:210` and :260 link
       a bound tab/workspace id by equality in any incarnation (the PR-roast escalation): true, no token or label check.
    8. `is_tombstone` at `src/model/clone.rs:88` and used at `src/plan/core_kinds.rs:514, :1867` (resurrect skips tombstones): true.

### Gap-path outcomes

- G1 cluster 4: 14 Part B outcomes above; 11 covered by Part A.
- G2 `reviews/inv-cc29fc83-.../assembled/...-final/` (73 files) - reviewed structurally, no findings. These are the archived inputs of the
  earlier invocation, whose own 19 cluster reports and seam report are committed (cluster 5 of inv-4aa37d70 already checked those 20 files
  byte-for-byte). All 73 sha256 values in `generated-input-provenance.md` match the files on disk (73/73), every entry points at run.md
  record line 48 at revision 6576a11, the 68 slice files on disk equal the 68 slices its 19-cluster `clusters.json` references, and 6576a11 and 0a84c67 both resolve. Omission "supplies no finding disposition", so no verdict is lost.
- G3 `docs/superpowers/runs/2026-10-31-h1-instruction-gate/run.md:86, 91, 95, 99, 103` (the 26 entries) - reviewed structurally; belongs to
  an earlier run (h1-instruction-gate), predates the base and is unchanged across the range (`git diff --stat 0a84c67 6576a11` over that
  directory is empty). The archives are present: inv-p3b (110 files), and 6 files each for p3c, p5r1, p5r2, p6fr (failed bounded assembly).
  No defect for this epic. The entries do keep whole-range readiness false: F8.
- G4 `...cluster-4.md` (bundle unavailable): not recoverable; replaced by the cluster-4 review (Part A and this section).
- Not gap paths, noted for completeness: `reviews/inv-c9eda770-.../` (head 992f07a) and `reviews/inv-f287b835-.../` (head 573ed77) are
  later bounded reviews outside inv-4aa37d70's range. Their profile JSONs parse, their `review-collection` records show
  `failed-bounded-assembly-v1` with `complete:true`, and their retained gaps are the same inv-4aa37d70 gap closed here.
- `profile-inv-4aa37d70-...md/.json`: reviewed structurally, no findings. JSON parses; summary "dispatched 5, landed 4, planned 31"; the four
  landed beads (.4 .7 .8 .6) are closed, and the unlanded .28 is closed in the tracker later (the profile's "did not land" is a point in time).

### Findings

- [Should-fix] run.md:83 - Herdr id-reissue risk has no owner and no spec disposition. Two parked escalations (run.md:93 design-roast
  material dissent; run.md:83 PR-roast UNVERIFIED with orchestrator evidence: Herdr v0.9.5 `reserve_workspace_ids` sets the counter to max
  surviving + 1, so the highest deleted workspace id can be reissued after restart; asks a human to confirm as Should-fix) are still parked.
  Spec §7 still asserts "Herdr preserves them", `src/observe/matcher.rs:210` and :260 link a bound tab/workspace id by bare equality in any
  incarnation, `LiveIndex::bound_pane` (`src/reconcile/planner.rs:173`) by id alone; `bd search` for reissue/nonce finds no bead, and
  hg-66u.11 closed without a corroboration guard. A reissued id would silently bind a clone to the wrong tab or pane. Needs the human
  decision the escalation asks for, then a bead (token or label corroboration) or a recorded acceptance in the spec.
- [Nit] findings-ledger-design.md:27 - rows 27-34 (eight r3 findings, six Should-fix and two Nit) still say `open`, but spec §16.1-§16.8
  (commit d981afb) resolve each and run.md:117 records "designRoast r3 fixed". Mapping: line 27 and 31 -> §16.1 and §16.5 (key-link is the
  only relink; hg-66u.68, .44, .43); 28 -> §16.2 (hg-66u.59, closed); 29 -> §16.3 (hg-66u.69); 30 -> §16.4 (AckRestore, hg-66u.29 closed);
  32 -> §16.6 (hg-66u.44 and .68 blocked by .31); 33 -> §16.7 (hg-66u.70); 34 -> §16.8 (`planner.rs:917`, hg-66u.30 closed). No row carries a
  `resolved@` marker or a re-check, and the roast ran 3 of 4 iterations, so nothing re-verified the amendments. Update the rows or record
  the dispositions.
- [Nit] run.md:100 - the design-roast exit record says "converged at round 2 ... no re-roast", but `roastDesignRound: 3` (run.md:13),
  `roast-design-3.md` exists with 8 confirmed findings, and run.md:117 records the r3 fixes. The exit line omits round 3 and its eight fixes.
- [Nit] run.md:85 - the two run-failure degraded verdicts (`instruction_source_writers_are_inventoried`, `sweep_action_and_effect`) are
  still parked as "run-regression open", and findings-ledger.md:82 ("Confirm by a measured run ...") is open, though hg-66u.65 closed after a
  measured pass (instruction_writers 2 passed; integration_sweep 22 passed, base = head bf8e2a3, tree 50a8cba). That evidence exists only in
  the git-ignored workspace (`task-40-report.md`); the bead closed with reason "Closed" and no comment, so no committed record resolves
  the verdict.
- [Nit] run.md:29 - the coverage-round-2 divergence record says "findings: 19 -> 20 novel: 12/20", but `coverage-findings-round-1.md`
  has 20 entries and `coverage-findings-round-2.md` has 24 (cov-23..cov-46 is 24 rows). The same record says "widening: yes", and the loop
  proceeded with no third round; the later amend rounds (cov-47+) are the only follow-up.
- [Nit] coverage-round-2-review-1.md:1 - both round-2 review files are identical condensed copies holding only a requirement-to-owner
  map, no findings and no reviewer attribution, so the 24 round-2 findings cannot be traced to a reviewer. The maps also name closed,
  superseded beads as sole owners (R1 -> .19, R4 -> .12, R7 -> .10, R9 -> .9, R10 and R15 -> .16, R17 -> .20, R18 -> .13, R21 -> .22), and
  no committed file maps R1-R21 to the current owners (.27-.34, .40, .43, .44, .68); that history lives piecemeal in cov-47+.
- [Nit] findings-overrides-design.md:1 - six overrides carry only `finding . dissolved@r1` with no reason. The reason exists only in
  run.md `stepBackDesign-round-1` ("dissolves 6"), which names a count, not the findings; the ledger header says `ignored-overrides: 0`.
- [Should-fix] reviews/inv-4aa37d70-023c-483a-b47c-3dc801a586fe/assembled/final-review-inv-4aa37d70-023c-483a-b47c-3dc801a586fe-final/review-inputs.json:1
  - whole-range readiness cannot clear. `wholeRangeReady` is false in inv-4aa37d70 (status complete, gaps empty), in inv-cc29fc83 (gaps empty,
  omitted 0) and in the two later bounded reviews. The one field common to all is the 26 out-of-range entries from the earlier
  h1-instruction-gate run, which provenance calls "neither selected nor waived"; no waiver record exists, and run.md:109 keeps "not ready
  (unreviewed: whole-epic coverage and omission provenance remain incomplete)". This is an inference from the field values, not from the
  tool's source. Result: every final review reports not ready however complete it is. Record an explicit waiver for the pre-base entries
  (this section is their review), and stop citing the gap by line number (`run.md:72` is now :75).

### Items for the coordinator

No code or spec change is made here. F1 needs a human decision and a bead; F8 needs a waiver record; F2-F7 are record hygiene.
