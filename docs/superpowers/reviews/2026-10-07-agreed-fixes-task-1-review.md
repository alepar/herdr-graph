# Task 1 scoped spec and code-quality review

Reviewed: `bf0e46f..32bb1e9`, using the supplied 4,000-line diff, task brief and implementation report. The supplied diff was read once in consecutive chunks. Unchanged code was inspected only to verify named propagation, staleness and undo risks.

**Spec verdict: issues found; substantially implemented, but not fully compliant.**

**Task code-quality verdict: needs fixes.**

Critical: none. Important: two. Minor: none.

## Important findings

### 1. Changing a retained seat's reusable reference can replace its session without a replacement in the reviewed plan

Primary changed-code evidence: `src/templates/kinds.rs:870` and `src/templates/kinds.rs:877`. These correctly resolve the old and new referenced seat definitions, but the surrounding replacement traversal is restricted to memberships of active applications of the edited team template (`src/templates/kinds.rs:730-734`, `src/templates/kinds.rs:856-857`).

Concrete trigger:

1. Create reusable seat definitions A and B with different models.
2. Instantiate owner team T with member M referencing A and give its seat an occupied active clone.
3. Explicitly reuse that seat in another team's application, then retire T's original application. The seat remains alive with its canonical T/M reference, as the new retained-seat regression itself establishes.
4. Edit T/M's `seat_template` from A to B.

There are now no active T applications in `compute_edit`, so the reviewed plan contains no `session.replace` for the retained seat. Nevertheless, `resolve_in` still reads T/M and its new definition (`src/model/effective.rs:103-118`), and the runtime desired-state reader calls that resolver (`src/reconcile/desired.rs:148`). The reconciler compares the changed launch shape and schedules `ReplaceSession` independently (`src/reconcile/planner.rs:633-652`). The result is an actual session replacement omitted from the approval preview. Undo of that reference edit has the same traversal gap.

This violates the explicit requirement for reviewable immediate runtime changes across live references and explicit reuse. The new seat-definition edit helper already handles this retained-seat case by enumerating canonical seat references (`src/templates/seat_definitions.rs:29-62`), but team-member reference edits do not share that coverage.

Fix direction: calculate runtime replacements for team edits from all surviving seats whose canonical template reference names the edited team, excluding seats actually being retired by this edit; attribute replacements to any current consuming applications separately. Cover reference switches after original-application retirement, after every application retires, and inverse edits in focused regressions.

Verification: confirmed by tracing the planner and runtime consumer; no live session was exercised.

### 2. Undo falsely conflicts on adding a member without inline instructions, including historical actions

Primary changed-code evidence: `src/undo/preview.rs:639`. Undo now loads the current document through `doc_with_agents`, which always represents a missing member instruction file as `Some("")` (`src/templates/kinds.rs:170-175`). The stored post-edit document still leaves `agents_md` as `None` when the edit omitted it (`src/templates/kinds.rs:1245-1255`).

Concrete trigger: edit an existing team template to add a member with a name, startup and optional reusable `seat_template`, omitting `agents_md`; immediately undo that template edit, with no intervening writes.

The original edit records an added `members.<id>` change (`src/templates/document.rs:241-244`). During undo, comparing the stored `after` document against current state yields a spurious `members.<id>.agents_md` drift because `None != Some("")` (`src/templates/document.rs:230-236`). The parent-path relationship check treats that drift as a conflict with the added member and returns `Inverse::Conflict` (`src/undo/preview.rs:640-644`, `src/undo/preview.rs:655-677`; ancestor matching at `src/undo/preview.rs:521-526`). A clean, immediate undo therefore becomes repair-required. Old compensation documents with an added member and omitted instructions follow the same failing path.

This is a regression introduced by loading instruction files in undo, and directly violates preservation of undo and existing compensation records. Reusable members naturally omit inline instructions, so it affects a normal use of this feature.

Fix direction: normalize the expected instruction state when comparing persisted post-edit documents, preserving the distinction between omission-as-no-write and explicit clearing. Apply equivalent normalization to old compensation records. Add a regression for immediate undo of a newly added member with no `agents_md`, plus a legacy compensation fixture; retain the existing later-clear conflict regression.

Verification: confirmed from the exact persisted-document, diff and conflict branches; no test-suite rerun was needed to establish the deterministic mismatch.

## Strengths and satisfied requirements

- The central resolver implements the requested six-level precedence for harness, model, args and summaries, including explicit empty args (`src/model/effective.rs:39-89`). The focused precedence regression covers all six levels.
- Typed reusable references use the existing template namespace, legacy kind defaults to team, and resolution rejects nonexistent or wrong-kind seat definitions. Seat definitions cannot contain team structure (`src/model/effective.rs:122-145`, `src/templates/document.rs:69-73`).
- Shared definitions do not share instantiated seat identities. Tests cover two independently instantiated teams, explicit reuse, overrides, exclusions and copied member correspondence.
- The new seat-definition propagation helper deduplicates runtime replacement effects by canonical seat identity and includes retained seats after application retirement. The implementation appropriately distinguishes runtime ownership from application membership.
- `/seat` now exposes reusable and member instruction files, application mappings, instance records/context and scoped rules as references (`src/bootstrap/mod.rs:155-235`). No profession registry, interpolation or generated semantic brief was added.
- Root instruction copying, exact before/after edit previews, explicit clear behavior and definition revisions in relevant reviewed effects are useful additions. The direct content-write guard is consistent with the controller's accepted policy and rejects only root or actual member instruction paths.
- Operational duty aliases preserve old serialized `role` inputs, and summarizer routing/source-summary eligibility continue to use the effective duty. Native transcript/session structures were not changed by this task.

## Validation and limits

- The implementation report records 608 passing library tests and 21 passing affected integration tests. These runs were supplied evidence; this review did not rerun the suites or claim independent execution.
- The added tests cover the principal new behavior, but do not exercise the two edge cases above. The existing retained-seat regression edits the shared definition, not the owner team member's reference. The instruction undo regression edits a reusable root file, not a structural member addition with omitted instructions.
- Legacy deserialization support is visible in the diff, but complete compatibility against a real historical instance or every old compensation shape cannot be established from this task diff. Finding 2 identifies one concrete historical-compensation regression.
- Actual native-harness replacement/resume and preservation of live external transcript history were not exercised. No external service or live instance was touched.
- Task 2 path changes and Task 3 consolidated documentation were deliberately outside this review.
- This was one independent, inline reviewer, with no subagents or panel. No source files, Git index, HEAD or branches were changed; only this permitted review artifact was written.
