# Run profile — hg-k4l

```
Profile: context — agents 40 (measured 40) · peak≥80% 0 · compacted 0 agents, 0 compactions · coordinator 601K/1000K (60%), 0 compactions · max coordinator 60%
Profile: context agent — coordinator peak 601K/1000K (60%) · first 157K · compactions 0
Profile: unmeasured — Claude context windows from the model table: claude-opus-5-5 1000K, claude-sonnet-5-5 1000K
Profile: unavailable — no timed task dispatch in the runs given (transcripts missing)
```

## Context

- Profile: context — agents 40 (measured 40) · peak≥80% 0 · compacted 0 agents, 0 compactions · coordinator 601K/1000K (60%), 0 compactions · max coordinator 60%
- Profile: context agent — coordinator peak 601K/1000K (60%) · first 157K · compactions 0

| agent | role | model | tasks | requests | first | peak | window | source | peak% | compactions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| coordinator | coordinator | claude-opus-5-5 | 12 | 171 | 157K | 601K | 1000K | table | 60% | 0 |
| final-review:seam | agent | claude-opus-5-5 | 1 | 31 | 34K | 166K | 1000K | table | 17% | 0 |
| final-review:cluster:14 | agent | claude-opus-5-5 | 1 | 29 | 32K | 164K | 1000K | table | 16% | 0 |
| final-review:cluster:23 | agent | claude-opus-5-5 | 1 | 42 | 32K | 139K | 1000K | table | 14% | 0 |
| final-review:cluster:8 | agent | claude-opus-5-5 | 1 | 36 | 32K | 140K | 1000K | table | 14% | 0 |
| final-review:cluster:13 | agent | claude-opus-5-5 | 1 | 26 | 32K | 132K | 1000K | table | 13% | 0 |
| final-review:cluster:9 | agent | claude-opus-5-5 | 1 | 26 | 32K | 120K | 1000K | table | 12% | 0 |
| final-review:cluster:12 | agent | claude-opus-5-5 | 1 | 19 | 32K | 110K | 1000K | table | 11% | 0 |
| final-review:cluster:1 | agent | claude-opus-5-5 | 1 | 21 | 32K | 100K | 1000K | table | 10% | 0 |
| final-review:cluster:21 | agent | claude-opus-5-5 | 1 | 29 | 32K | 103K | 1000K | table | 10% | 0 |
| final-review:cluster:16 | agent | claude-opus-5-5 | 1 | 27 | 32K | 87K | 1000K | table | 9% | 0 |
| final-review:cluster:18 | agent | claude-opus-5-5 | 1 | 19 | 32K | 90K | 1000K | table | 9% | 0 |
| final-review:cluster:3 | agent | claude-opus-5-5 | 1 | 17 | 32K | 90K | 1000K | table | 9% | 0 |
| final-review:cluster:10 | agent | claude-opus-5-5 | 1 | 23 | 32K | 82K | 1000K | table | 8% | 0 |
| final-review:cluster:4 | agent | claude-opus-5-5 | 1 | 13 | 32K | 81K | 1000K | table | 8% | 0 |
| final-review:cluster:6 | agent | claude-opus-5-5 | 1 | 22 | 32K | 76K | 1000K | table | 8% | 0 |
| final-review:cluster:7 | agent | claude-opus-5-5 | 1 | 19 | 32K | 82K | 1000K | table | 8% | 0 |
| final-review:cluster:19 | agent | claude-opus-5-5 | 1 | 18 | 32K | 75K | 1000K | table | 7% | 0 |
| final-review:cluster:20 | agent | claude-opus-5-5 | 1 | 21 | 32K | 75K | 1000K | table | 7% | 0 |
| assemble:final | agent | claude-sonnet-5-5 | 1 | 12 | 31K | 57K | 1000K | table | 6% | 0 |
| final-review:cluster:11 | agent | claude-opus-5-5 | 1 | 11 | 32K | 55K | 1000K | table | 6% | 0 |
| final-review:cluster:15 | agent | claude-opus-5-5 | 1 | 12 | 32K | 60K | 1000K | table | 6% | 0 |
| final-review:cluster:17 | agent | claude-opus-5-5 | 1 | 15 | 32K | 61K | 1000K | table | 6% | 0 |
| final-review:cluster:22 | agent | claude-opus-5-5 | 1 | 11 | 32K | 58K | 1000K | table | 6% | 0 |
| plan | agent | claude-opus-5-5 | 1 | 7 | 30K | 56K | 1000K | table | 6% | 0 |
| plan-rest | agent | claude-opus-5-5 | 1 | 9 | 31K | 58K | 1000K | table | 6% | 0 |
| edge-audit:1 | agent | claude-opus-5-5 | 1 | 5 | 30K | 50K | 1000K | table | 5% | 0 |
| final-review:cluster:5 | agent | claude-opus-5-5 | 1 | 10 | 32K | 51K | 1000K | table | 5% | 0 |
| read-ledger | agent | claude-sonnet-5-5 | 1 | 3 | 30K | 43K | 1000K | table | 4% | 0 |
| read-ledger:finish | agent | claude-sonnet-5-5 | 1 | 3 | 30K | 44K | 1000K | table | 4% | 0 |
| worktree-sweep | agent | claude-sonnet-5-5 | 1 | 3 | 30K | 39K | 1000K | table | 4% | 0 |
| bd-ready | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| close-epics | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 31K | 1000K | table | 3% | 0 |
| ledger-append:detector | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| ledger-append:edge-audit:1 | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| ledger-append:launch | agent | claude-sonnet-5-5 | 1 | 2 | 30K | 32K | 1000K | table | 3% | 0 |
| ledger-append:metrics | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| ledger-append:recovery:deferred:hg-k4l:sweep:seat-failure-manager-seats-20261105T074235 | agent | claude-sonnet-5-5 | 1 | 2 | 29K | 32K | 1000K | table | 3% | 0 |
| ledger-append:surface:1 | agent | claude-sonnet-5-5 | 1 | 2 | 30K | 32K | 1000K | table | 3% | 0 |
| reconcile-buckets | agent | claude-sonnet-5-5 | 1 | 4 | 29K | 32K | 1000K | table | 3% | 0 |
