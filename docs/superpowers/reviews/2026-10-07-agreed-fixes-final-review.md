# Whole-branch review — agreed fixes, 2026-10-07

Base: `1603214845745174f5036ea79375198c7a44a1dd`  
Head: `b1a35d6ec1d72949dfee01a0bf33073b652c4b09`

**Ready to merge? With fixes.** One Important finding remains; no Critical or Minor findings.

Reviewed the full approved handoff and implementation plan, ledger/rulings, whole-branch source changes in bounded passes, affected callers, added behavioral tests, final release handoff and scoped re-review evidence. Passes covered template/runtime/instruction behavior, storage/transcript/undo compatibility, then dependency/documentation/validation integration. Historical October 5 review candidates were not treated as implementation requirements.

## Strengths

- Reusable definitions use the existing template identity with explicit kinds and typed references. Runtime resolution implements all six precedence levels, including explicit empty arguments and graph-level arguments/summary defaults (`src/model/effective.rs:40`). Reused seats retain canonical configuration; replacement planning visits canonical seats even after their original application retires (`src/templates/seat_definitions.rs:24`, `:103`).
- Definition revision information and exact changed values participate in reviewed effects. Invalid or wrong-kind references fail explicitly. Root/member instruction edits use the reviewed mutation path and legacy operational duties remain readable (`src/templates/kinds.rs:920`, `src/bootstrap/content.rs:110`, `src/model/seat.rs:23`).
- Compatibility preserves existing locations rather than moving historical records. Targeted lookup checks supported legacy/current locations without deserializing unrelated history, while rejecting corrupt targeted payloads and duplicate matches (`src/store/layout.rs:291`). Request delivery, merging and completion update located paths; transcript capture names and identity remain historical (`src/transcripts/mutations.rs:102`, `:179`, `:429`, `:638`). Native transcript contents are not copied or rewritten.
- Mixed-layout tests cover late processing, organization changes and old compensation records. The prior member-add omission undo regression and retained canonical-seat preview regression have concrete fixes and tests. The release handoff describes migration and native-runtime limits accurately, including private-suite early returns and actual real-daemon execution.

## Issues

### Critical (Must Fix)

None. Count: **0**.

### Important (Should Fix)

#### F1. Preserve specialization by member identity when its display name changes

**Primary location:** `src/templates/kinds.rs:193` (instruction writes); changed live-only behavior at `src/templates/kinds.rs:413`. Related consumers: `src/bootstrap/mod.rs:167`, `:190`, `:216`; undo at `src/undo/preview.rs:666`, `:729`.

Member instruction files are addressed by the current name's slug, but template edit does not relocate or otherwise carry their contents when an existing stable member ID is renamed. `write_agents` only handles explicitly supplied `agents_md`; omission leaves the file under the old slug. `/seat` immediately resolves the new member name and therefore no longer exposes that specialization. Newly instantiated reusable members intentionally have no copied seat `AGENTS.md`, so the new live-reference model loses the specialization entirely from their applicable instruction references.

Concrete trigger:

1. Create a seat definition and a team member with stable ID M, `name = "dev"`, a `seat_template` reference, and `agents_md = "team specialization"`; instantiate it.
2. Edit the team document retaining M and its seat-template reference, set `name = "implementor"`, and omit `agents_md` (documented to preserve instructions).
3. The member mapping/seat identity remains stable and the plan reports the name change. The instruction file remains `members/dev/AGENTS.md`; no `members/implementor/AGENTS.md` is written. Both canonical and participating-application `/seat` lookup now probe only the latter, so the text disappears from the live context despite omission and no requested instruction change.

The same path coupling also violates preservation of unrelated later edits during undo: after that rename, explicitly set new specialization text at `members/implementor/AGENTS.md`, then undo only the name-change action. The inverse compares field paths by stable ID, correctly considers `.name` and `.agents_md` unrelated, and restores only the old name with instruction fields omitted. It then exposes the stale original `members/dev/AGENTS.md`, hiding the later instruction edit. An immediate undo with no intervening edit happens to expose the original old-path text again; this finding does **not** claim that immediate rename undo always conflicts.

This violates the handoff's live instructions/member correspondence/undo requirements and the current documented promise that omitted `agents_md` retains instructions. The previous inline path mechanics existed before the branch, but removing the instance instruction copy makes this a direct gap in the new reusable-member behavior. Validation here is a source-level execution trace through document matching, edit mutation, `/seat` lookup and inverse-field handling; no new executable reproduction is claimed.

**Fix direction:** Resolve retained instruction contents by stable member ID before applying member-name/path changes, and carry the current text to the resulting path in both forward and inverse edits. Preserve explicit clear semantics and unrelated later instruction edits; handle destination collisions/name swaps without overwriting another member's text. Add real plan/apply/bootstrap regressions for rename with omitted instructions, immediate undo, and rename → instruction edit → undo rename. Verify stable member/seat mappings, exact instruction bytes and `/seat` references throughout.

### Minor (Nice to Have)

None. Count: **0**. No parked or deferred findings.

## Spec coverage

| Approved scope | Assessment |
|---|---|
| Reusable seat/team definitions; no profession registry; typed references | Implemented; legacy omitted kind and duty aliases retained |
| Exact runtime precedence, independent instantiation and explicit reuse | Implemented and covered by added tests |
| Live instructions, member correspondence, immediate effects and undo | Broadly implemented; **F1 remains** for member-name instruction continuity and related undo |
| Overrides/exclusions and surviving canonical seats | Preserved in inspected resolution, planning and fix regressions |
| Named transcript indexes and historical capture attribution | Implemented; immutable registration naming and unknown legacy attribution are consistent with the recorded ruling |
| Mutation folders, dual reads, location-preserving updates and compensation | Implemented; journal authority and old compensation locations retained |
| Persistent instances/native contents/history | No eager relocation or native-content rewrite introduced; compatibility fixtures exercise legacy records and undo |
| Threads 0.2.9 synchronization | Lockfile/fixture change and recorded API/service validation support the stated compatibility; native-agent execution remains explicitly untested |
| Approved design/history/release documentation | Included; current docs distinguish historical evidence, implementation and pending integration/release ownership |

## Validation evidence and boundaries

- Inspected existing Task 3 logs: default **664 passed / 0 failed / 1 ignored**; no-default **662 / 0 / 1**; test-support **678 / 0 / 1**; private **710 reported / 0 / 6**, including the five disclosed opt-in early returns; isolated real threads **2 / 0 / 0**, actually executed. No covering suites were rerun.
- Independently read the clean external threads checkout and confirmed HEAD and dereferenced `v0.2.9` both equal `223b61a88625d7f442d22d9b8728dc4b2282b15f`; Cargo version is 0.2.9 and protocol is 6. Official remote-tag verification is controller-supplied evidence recorded in the release handoff, not a new network check by this reviewer.
- `git diff --check 1603214..b1a35d6` passed. The October 5 historical report is unchanged from preservation commit `7a87863`. Graph HEAD remained the requested review SHA and the tracked checkout was clean before writing this ignored review artifact.
- This is code/evidence review, not exhaustive historical-instance, native-agent or live-service execution. No production source, index, HEAD, branch, dependency checkout or live service was modified; no subagents were spawned. Only this authorized review artifact was written.

## Recommendations and assessment

Fix F1 and run focused red/green instruction-lifecycle tests plus the appropriate template/bootstrap/undo covering checks, then obtain scoped re-review of that correction. The existing complete feature-tier evidence need not be rerun solely because this review inspected it.

**Ready to merge? With fixes.** The branch otherwise provides coherent compatibility mechanics and unusually clear validation boundaries. The remaining instruction continuity defect can silently remove or revert applicable specialization during an ordinary supported rename/undo sequence, so it should be corrected before the controller proceeds to merge/push and release ownership transfer.
