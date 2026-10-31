# Final review, cluster 11 (src-templates), epic hg-45e

Invocation: inv-p3b-20261031T222244. Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161. I verified both with `git rev-parse --verify`. The review source binding (`bind-review-source`) exited 0.

Scope: src/templates/document.rs, src/templates/kinds.rs, src/templates/tests.rs (slices 000071-000073). The spec is docs/superpowers/runs/2026-10-31-h1-instruction-gate/2026-10-31-h1-instruction-gate-design.md, sections 1.3 (stop seeding the seat copy), 2.2a (template plan-time content check) and 4.4 (template edit cap warning).

Evidence: I ran the tests in a review-owned detached worktree at head (/var/home/alepar/AleCode/herdr-graph/.worktrees/review-c11-inv-p3b-20261031T222244, target seeded by reflink). `cargo test --offline --lib templates::` gave 39 passed, 0 failed. `cargo test --offline --features test-support --lib templates::` also gave 39 passed, 0 failed, and that run includes template_edit_rejects_inadmissible_agents_md, template_edit_cap_warning and hydrate_creates_dormant_and_active_per_startup. The caller checkout stayed at bce6589 and clean throughout.

## Findings against the spec

- §1.3, hydration stops seeding the seat copy: met. `create_member_seat` no longer writes `<seat>/AGENTS.md`, and the unused `tpl_dir` parameter and `HydratePlan.loc` were dropped. Both callers, `ApplicationApply` and live propagation in `TemplateEdit::mutate`, go through the same function. No other seat-copy path remains in kinds.rs; `template copy` copying template-to-template is unaffected. The updated hydrate test asserts that no seat AGENTS.md exists and that the member file stays in the template folder.
- §2.2a, plan-time content check on `plan template edit`: met. `instruction_texts()` lists the root `agents_md` and each member's `agents_md` and `responsibility`. Each one goes through the shared `check_content` predicate before any other planning, and the error carries the field path plus the code point and line. Tests cover a tag character (U+E0041), a bare CR and ESC in the three fields. The check covers every text the document sets, not only the changed ones. That is stricter than "when agents_md or responsibility changes": an edit that resubmits pre-existing inadmissible text is refused until the text is fixed. This fails closed, which is acceptable.
- §4.4, cap warning on template edit: met for the main path. When any change path is `agents_md`, `*.agents_md` or `*.responsibility`, `edit_cap_warnings` lays the new record and the edited AGENTS.md files over the committed tree with `PatchedTree`. It then calls the shared `cap_warning` for every non-retired seat whose `template_participation` includes the edited template, which also covers seats that reach a reusable seat template through a team member. The test covers both cases: no warning under the cap, and the warning prefix with "template solo" in the omitted labels over the cap.
- Defect, minor: the overlay in `edit_cap_warnings` re-implements part of `write_agents` and disagrees with it in two cases.
  - A renamed member whose document omits `agents_md`: `write_agents` moves the retained text to the new slug path, but the overlay leaves it at the old path. The render then finds no member file and undercounts, so the warning can be missed. Example: member `lead` has a 9 KB AGENTS.md, and one edit renames it to `chief` and changes `responsibility`.
  - A new member that reuses an old slug with `agents_md` omitted: `write_agents` deletes the stale file, but the overlay keeps it and overcounts.

  Both cases only affect an advisory warning, not committed state.
- Doc drift (already deferred under hg-45e.18): the `/// The edited document as stored…` comment now sits above `edit_cap_warnings` and merges into its rustdoc, so `merged_after` has no doc comment.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- template edit cap warning when only a member `responsibility` or member `agents_md` changes (the test grows only the root agents_md)
- template edit cap warning for a seat that takes part through a reusable seat template referenced by a team member, and for a seat bound through `template_ref`
- cap warning branch `seat AGENTS.md will be cut` is not exercised from template edit
- member rename combined with an omitted agents_md and an instruction change (the overlay/write_agents divergence above)
- live-propagation path (edit adding a member to an applied template) asserting no seat AGENTS.md is seeded; only hydration is asserted
- retired seats being skipped by edit_cap_warnings

## Deferred OK
- edit_cap_warnings overlay diverges from write_agents for member renames with omitted agents_md (missed warning) and slug reuse by a new member (spurious warning); advisory only, fix by deriving the overlay from the same retained-text logic as write_agents
- seats created or resurrected by the edit itself are not sized for the cap warning (stated assumption, hg-45e.18)
- plan template create and template copy run no plan-time content check; inadmissible text there still renders as the §2.2a placeholder, and the spec names only template edit
- plan-time check rejects pre-existing inadmissible text even when unchanged (stricter than spec, fails closed)
- misplaced doc comment above edit_cap_warnings / missing doc on merged_after (hg-45e.18 deferred minor)
- possibly stale #[allow(clippy::too_many_arguments)] on create_member_seat (hg-45e.17 deferred minor)
