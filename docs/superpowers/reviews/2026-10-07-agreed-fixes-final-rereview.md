**F1: Preserve specialization by member identity across rename and undo — ADDRESSED.**

### Finding verdict

- `src/templates/kinds.rs:188` snapshots current instruction bytes by stable member ID before deleting or writing any member paths. `src/templates/kinds.rs:206` then gives explicit text/clear priority and otherwise carries that ID's current bytes. Renaming with omission now preserves the specialization; inverse rename carries current text, including a later edit or clear, instead of exposing stale historical text.
- `src/templates/kinds.rs:1104` passes the resolved after-document to this shared mutation path, so name-matched members whose input omitted IDs still receive stable-ID retention. Create uses the same writer with no previous record (`src/templates/kinds.rs:545`), preserving explicit create instructions without inheriting orphan destination contents.
- Snapshot-before-write handles swaps. Removing former instruction paths and clearing destinations with no retained/explicit bytes prevents new IDs or instruction-less members from inheriting another member's specialization (`src/templates/kinds.rs:190`, `:193`, `:215`). The change only targets member instruction files; unrelated sibling content and root omission semantics remain untouched.
- Collision validation rejects ambiguous new create/edit documents with an actionable member/path diagnostic (`src/templates/document.rs:85`). It does not alter stored-record reading or application hydration. Its effect on inverse documents is explicitly documented in the release handoff and matches the controller's recorded ruling.

### New breakage in the fix diff

None. **0 Critical, 0 Important, 0 Minor.**

### Out-of-scope observations

None. No additional ledger entries or deferred findings.

### Checks

- Reviewed fix range `b1a35d6ec1d72949dfee01a0bf33073b652c4b09..02f24ce7fbee441087b5428ac526d297d30db52a`, implementation report, changed production code, new regressions and handoff changes. Review scope was F1 and breakage introduced by this fix, not a second whole-branch review.
- The bootstrap regressions cover canonical and participating-application member references, exact committed/working-tree bytes, stable member/seat/clone mappings, omission and immediate undo (`src/bootstrap/tests.rs:1816`), preservation of later instructions/clears/root/responsibility (`:1868`), and rename with explicit clear followed by undo (`:1905`).
- The template regressions cover swap/undo (`src/templates/tests.rs:2158`), retained/new identities moving into a departed member's path without inheriting its text (`:2209`), and create/edit collision rejection before a write (`:2259`). These use actual isolated Git-backed plan/apply/undo flows.
- Inspected `/private/tmp/hg-final-fix-red.log`: all six regressions compiled and failed at the expected behavioral assertions before the production correction. Inspected `hg-final-fix-green.log`: **6 passed, 0 failed**. Inspected `hg-final-fix-lib.log`: **623 passed, 0 failed, 0 ignored**. Inspected `hg-final-fix-integrations.log`: **33 passed, 0 failed, 0 ignored** across daemon_composed, daemon_ipc and integration_sweep (**14 + 12 + 7**).
- No tests or full suites were rerun. The earlier feature-matrix and real-threads evidence remains distinct from the correction's covering tests. Formatting and the byte-preserved historical-review whitespace exception are controller/implementation evidence, not independently rerun checks in this scoped review.
- Read-only source/index/HEAD review; no subagents, live services or external instances. Only this authorized review artifact was written.

### Verdict

**Fix round: All findings addressed, no new Critical/Important breakage.** F1 is closed. Combined with the prior whole-branch review, **ready to merge: Yes** from code review; controller-owned integration/push and release handoff gates remain.
