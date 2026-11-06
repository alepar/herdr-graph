# Run profile — hg-66u

```
Profile: hg-66u · 1 invocation · 2026-11-06T19:28Z → 2026-11-06T20:01Z · wall 33m (task graph 20m, after the last landing 12m) · agents 41 (timed 41) · beads dispatched 1, landed 1, planned 54
Profile: graph scope — observed invocation/residual graph; historical topology is unknown
Profile: shape — 54 beads · 29 edges · depth 7 · width 7.7 · beads per level 44/1/1/1/3/3/1 · widest fan-out hg-66u.43 (8 direct, 10 transitive dependents)
Profile: bound — graph lower bound 20m (unlimited slots and merge lanes) · model of the run 20m (cap 6, one merge lane) vs actual 20m (99%) → bead-bound: hg-66u.17 alone set the pace — split it or make it faster
Profile: critical path — 20m = implement 12m · review 38s · merge 49s · planning 3m · ready 9s · startup 4m · dispatch 10s
Profile: diagnosis — planning 3m (12% of first dispatch to last landing)
Profile: critical chain — bd-ready ready 9s → plan planning 3m → hg-66u.17 implement+review+merge 14m
Profile: bottleneck — hg-66u.17 — 14m on the critical path (69%) · implement 12m · review 38s · merge 49s · dispatch 7s · implement on the path 12m: git 3s · read 6s · shell 8m · model 4m (longest: "I=/var/home/alepar/AleCode/herdr-graph/.worktrees/threads-s…" 54s, "python3 - <<'EOF'" 52s) · dependents 0
Profile: waits — ready→start, summed per bead (beads wait at the same time, so not wall time): 0s over 0 of 1 landed beads
Profile: early unblock — 0 landed beads had in-run blockers · stacked on an implementation 0 · bounced on a stack conflict then waited for the merge 0 (0s) · waited for a merge without one 0
Profile: tests — implementers and fixers, summed: 0s running tests and 0s polling background runs, 0% of their 12m · 0 Bash call(s) ran into the 10-minute tool limit
Profile: merge lane — busy 49s of 49s (100%) · seam reviews and fixes 0s (0% of busy) · 1 merge dispatches, first try not merged for 0 of 1 · queue wait 2s total, peak 1 waiting · stack-parent wait 0s
Profile: runtime — peak 3 agents at once of 16 runtime slots · full 0% of the run · slot cap 6
Profile: concurrency scope — all-agent concurrency peak 3 · task-slot concurrency peak 1, cap 6 (ordinary chains retain a slot through merge)
Profile: what-if estimate — faster hg-66u.17 → 14m (−6m, 31%) — its implement time halved; first dispatch to last landing, planning retained
Profile: context — agents 42 (measured 42) · peak≥80% 1 · compacted 0 agents, 0 compactions · coordinator 823K/1000K (82%), 0 compactions · max coordinator 82%
Profile: context agent — coordinator peak 823K/1000K (82%) · first 96K · compactions 0
Profile: unmeasured — Claude context windows from the model table: claude-opus-5-5 1000K, claude-sonnet-5-5 1000K
```

## Per bead

Offsets are from the run's first dispatch. `wait` starts at evidenced creation/mapping/dependency availability, bounded by invocation start; unknown readiness means unknown wait. Creation, mapping and dependency timestamps stay distinct in JSON. `stack` waits for parents to merge; `queue` waits for the single merge lane; `lane` is its lane turn (merge plus seam work).

| bead | deps | dependents | attempts | wait (cause) | implement | of it: tools | review | fix | stack | queue | lane (seam) | landed | critical |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| hg-66u.17 | 0 | 0 | IMPLEMENTED | - | 12m | git 3s · read 6s · shell 8m | 38s | - | - | 2s | 49s | +20m | 14m |

## Critical path, step by step

| at | step | kind | via | duration |
|---|---|---|---|---|
| +0s | run start | startup |  | 4m |
| +4m | bd-ready | ready |  | 9s |
| +4m | previous → plan | dispatch |  | 2s |
| +4m | plan | planning | previous | 3m |
| +6m | planned → impl:hg-66u.17 | dispatch |  | 2s |
| +6m | impl:hg-66u.17 | implement | planned | 12m |
| +19m | chain → review:hg-66u.17 | dispatch |  | 3s |
| +19m | review:hg-66u.17 | review | chain | 38s |
| +19m | chain → merge:hg-66u.17 | dispatch |  | 2s |
| +19m | merge:hg-66u.17 | merge | chain | 49s |

## Longest tool calls in the bottleneck's implement (hg-66u.17)

| tool | category | what | duration |
|---|---|---|---|
| Bash | shell | I=/var/home/alepar/AleCode/herdr-graph/.worktrees/threads-s… | 54s |
| Bash | shell | python3 - <<'EOF' | 52s |
| Bash | shell | W=$PWD | 51s |
| Bash | shell | python3 - <<'EOF' | 45s |
| Bash | shell | python3 - <<'EOF' | 45s |
| Bash | shell | W=$PWD | 45s |
| Bash | shell | W=$PWD | 44s |
| Bash | shell | W=$PWD | 44s |
| Bash | shell | W=$PWD | 15s |
| Bash | shell | sed -n 10,30p Cargo.toml | 14s |

## Schedule model

Landed beads replayed with their measured implement, review+fix and lane times; creation, mapping and invocation bounds retained, with dispatch as the conservative release when readiness is unknown. In-model dependencies release at their modeled implementation/merge times; known releases outside the model stay fixed. Window: first dispatch to last landing; planning retained. Model of the run: 20m against 20m measured. Estimates, not measurements.

| scenario | makespan | saving |
|---|---|---|
| as run (cap 6, one merge lane) | 20m | - |
| unlimited slots | 20m | 0s |
| unlimited merge lanes | 20m | 0s |
| graph lower bound (both) | 20m | 0s |
| faster hg-66u.17 | 14m | 6m |

## Timeline

each bead's final attempt; critical-path steps are marked crit.

```mermaid
gantt
  dateFormat x
  axisFormat %H:%M
  section hg-66u.17
  impl :crit, 1793993710581, 1793994443941
  review :crit, 1793994447101, 1793994484614
  merge :crit, 1793994486846, 1793994535707
  close-check :1793994537507, 1793994543028
```

## Context

- Profile: context — agents 42 (measured 42) · peak≥80% 1 · compacted 0 agents, 0 compactions · coordinator 823K/1000K (82%), 0 compactions · max coordinator 82%
- Profile: context agent — coordinator peak 823K/1000K (82%) · first 96K · compactions 0

| agent | role | model | tasks | requests | first | peak | window | source | peak% | compactions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| coordinator | coordinator | claude-opus-5-5 | 34 | 315 | 96K | 823K | 1000K | table | 82% | 0 |
| final-review | agent | claude-opus-5-5 | 1 | 37 | 34K | 206K | 1000K | table | 21% | 0 |
| impl:hg-66u.17 | agent | claude-sonnet-5-5 | 1 | 41 | 35K | 138K | 1000K | table | 14% | 0 |
| plan | agent | claude-opus-5-5 | 1 | 10 | 30K | 78K | 1000K | table | 8% | 0 |
| plan-rest | agent | claude-opus-5-5 | 1 | 11 | 30K | 77K | 1000K | table | 8% | 0 |
| read-ledger:finish | agent | claude-sonnet-5-5 | 1 | 5 | 30K | 72K | 1000K | table | 7% | 0 |
| read-ledger:finish:retry | agent | claude-sonnet-5-5 | 1 | 5 | 30K | 71K | 1000K | table | 7% | 0 |
| read-ledger | agent | claude-sonnet-5-5 | 1 | 5 | 30K | 62K | 1000K | table | 6% | 0 |
| review:hg-66u.17 | agent | claude-sonnet-5-5 | 1 | 6 | 32K | 63K | 1000K | table | 6% | 0 |
| merge:hg-66u.17 | agent | claude-sonnet-5-5 | 1 | 9 | 33K | 45K | 1000K | table | 5% | 0 |
| assemble:final | agent | claude-sonnet-5-5 | 1 | 4 | 31K | 39K | 1000K | table | 4% | 0 |
| worktree-sweep | agent | claude-sonnet-5-5 | 1 | 3 | 30K | 41K | 1000K | table | 4% | 0 |
| bd-ready | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 31K | 1000K | table | 3% | 0 |
| bd-ready | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 31K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 34K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 32K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 32K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 32K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 32K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 34K | 1000K | table | 3% | 0 |
| bd-ready-topup | agent | claude-sonnet-5-5 | 1 | 3 | 29K | 33K | 1000K | table | 3% | 0 |
| close-check:hg-66u.17 | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 31K | 1000K | table | 3% | 0 |
| close-epics | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 31K | 1000K | table | 3% | 0 |
| close-epics | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 31K | 1000K | table | 3% | 0 |
| ledger-append:detector | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 31K | 1000K | table | 3% | 0 |
| ledger-append:launch | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| ledger-append:metrics | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| ledger-append:recovery:deferred:hg-66u:sweep:threads-seat-identity-20261105T073527 | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| ledger-append:surface:1 | agent | claude-sonnet-5-5 | 1 | 3 | 30K | 34K | 1000K | table | 3% | 0 |
| ledger-recurring:1 | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| ledger:hg-66u.17 | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |

## Unmeasured

- Claude context windows from the model table: claude-opus-5-5 1000K, claude-sonnet-5-5 1000K
