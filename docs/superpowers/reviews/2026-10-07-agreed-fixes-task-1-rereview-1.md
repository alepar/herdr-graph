# Task 1 scoped re-review, round 1

Inputs: fix diff `32bb1e9..2ad0527`, original Task 1 brief, prior `task-1-review.md`, appended implementation report and recorded validation logs.

**Scoped verdict: approved. Both prior Important findings are ADDRESSED. No new Critical or Important regression found in the fix diff.**

## Finding 1: retained canonical owner-team reference changes — ADDRESSED

`compute_team_replacements` now enumerates seats by canonical template reference independently of application lifecycle (`src/templates/seat_definitions.rs:127-155`). It excludes retired seats, retired teamspaces and seats actually being retired by this edit (`src/templates/seat_definitions.rs:117-138`), computes old/new effective configurations and emits the occupied-clone replacement effects (`src/templates/seat_definitions.rs:142-164`). A seat is visited once, avoiding duplicate replacements caused by multiple application memberships.

Current application attribution is a separate pass (`src/templates/seat_definitions.rs:167-194`). Borrowers receive `replaced_seats` attribution without inheriting the owner team's structural changes. With no remaining applications, replacement effects still remain in the result. Team edits and their inverse edits both use this computation.

The regression at `src/templates/tests.rs:2034` covers both origin-only retirement and retirement of every application, checks A→B and B→A replacement previews and application counts, applies the inverse and verifies the restored runtime model. The regression at `src/templates/tests.rs:2123` additionally verifies that an occupied seat being withdrawn is retired without a replacement effect.

## Finding 2: immediate and historical member-add undo with omitted instructions — ADDRESSED

`expected_instruction_state` normalizes a comparison copy of the persisted after-document (`src/undo/preview.rs:630-650`). Omission retains known before-text for an existing member and means missing/empty instructions for a newly added member; explicit empty text remains explicit. This removes the false `None` versus `Some("")` drift that previously blocked immediate member-add undo.

The original action documents still determine which fields actually changed (`src/undo/preview.rs:667-669`). Building the inverse from the current record and restoring only those fields avoids turning unrelated loaded instruction files into writes (`src/undo/preview.rs:729-738`). Existing later-edit and explicit-clear conflict detection remains in place.

The current-schema regression at `src/templates/tests.rs:2123` exercises the real edit/undo path for an active reusable member with omitted instructions. The historical compensation fixture at `src/undo/tests.rs:1155` omits kind and instruction fields, uses legacy duty/profession field spellings, obtains the inverse and applies it through the real template-edit path.

## Regression assessment and validation

- Critical findings: none.
- Important findings: none.
- No unrelated observations are raised in this scoped review.
- Inspected recorded logs confirm the three added regressions passed, the existing later-clear conflict regression passed, all **611 library tests** passed, and **14 daemon_composed + 7 integration_sweep tests** passed. Logs inspected: `/private/tmp/hg-task1-review-green.log`, `/private/tmp/hg-task1-review-lib.log`, `/private/tmp/hg-task1-review-integration.log`.
- No test suites were rerun. This is code review plus inspection of supplied execution evidence, not an independent test execution.
- Scope was limited to the two fixes and material regressions in the five-file fix diff. This does not claim exhaustive historical-instance or live native-harness validation.
- No source, index, HEAD, branch or external service mutation occurred. No subagents were spawned. Only this permitted review artifact was written.
