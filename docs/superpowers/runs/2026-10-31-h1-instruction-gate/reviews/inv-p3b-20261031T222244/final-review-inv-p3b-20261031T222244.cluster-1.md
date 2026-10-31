# Final review — cluster 1 (docs) — epic hg-45e

Invocation: inv-p3b-20261031T222244
Captured source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify <sha>^{commit}`; review source bound in the ledger before this write).
Caller checkout observed at HEAD bce6589, `git status --short` clean, at the start and end of the review. No caller files were edited.

Scope: the 27 files in the `docs` cluster (slices 000001–000027): the run directory `docs/superpowers/runs/2026-10-31-h1-instruction-gate/*` (spec, roast reports, coverage rounds, ledgers, run.md, friction, input, profile, promotion input) and `docs/superpowers/specs/INDEX.md`. The spec is the epic's design doc (`bd show hg-45e`). I opened other files only to check named risks: the user-facing docs that spec §5 and AC7 name, `skills/graph/SKILL.md`, and the child bead states.

## What the cluster contains

- The spec (`2026-10-31-h1-instruction-gate-design.md`) holds together. It reflects both roast rounds: the §2.2a single admissibility predicate (with bare-CR and invisible code points excluded and dotfiles ignored), stop-at-first-overflow truncation from the most specific part, the cap warning on template edit and section override, tty prompts refusing under agent markers, and harness-native project files listed as a residual vector. Key abstractions agree with the body: InstructionSource evaluates admissibility, and the renderer does not truncate. I found no contradiction between sections that would mislead an implementer.
- The run records (roasts, coverage ledger cl-1..cl-32, design-loop notes, friction) trace each design change to a finding and a bead.
- The INDEX.md row is added in the existing table format.

## Findings

### F1. Must fix: spec §5 / AC7 documentation is not in the tree (hg-45e.11 still open)
Spec §5 and AC7 require `CURRENT-DESIGN.md`, `GRAPH_STATE.md`, `CREATING_NEW_GRAPH.md`, `README.md`, `CHANGELOG.md` and `skills/graph/SKILL.md` to reflect the new commands. `git diff --stat base..head` on those six paths is empty, and bead hg-45e.11 ("Docs and graph skill…") is `open`. At head:
- `skills/graph/SKILL.md:42` still tells agents to run `herdr-graph content write --object <id> --rel … --from <file>`. That command now exits 2 (hg-45e.9). This is the agent-facing skill, so every agent that follows it hits the stub.
- `skills/graph/SKILL.md:20` still uses the old "Ask the user for an explicit yes" relay copy, not the §3.3 copy ("show this plan to the user, then run apply; the harness asks the user to approve it"). The §3.5 cooperative-strength note and its residual-vector list are also missing. So is the check that the hash in the approval prompt matches the hash printed above the diff.
- `README.md:52` and `CREATING_NEW_GRAPH.md:70` still describe `content write` as a live command with rejection rules.
- `CHANGELOG.md` has no entry for the removal of `content write`, the new commands (`plan instructions set|clear`, `instructions show`, `request complete --summary-from`, `seat --instructions`), the `paths.clone_files` → `paths.data`/`clone_record` contract change, or the apply ask rule. It also lacks the §1.5 migration note. Existing instances keep a copied summarizer `AGENTS.md` that still calls `content write`, and nothing tells the user to run `plan template edit` and then `plan instructions clear seat <name> AGENTS.md` for seeded seats. Without that note, upgrading breaks the summarizer pipeline on every existing instance, and the user gets only the stub's message as a hint.
- `CURRENT-DESIGN.md` still states that `/seat` "does not synthesize prose" and returns live references. The spec explicitly supersedes this (Key decisions).

### F2. Must fix: epic acceptance beads hg-45e.12 and hg-45e.16 are open
The §6 configuration smoke (hg-45e.12) and the end-to-end integration sweep (hg-45e.16) are open. Coverage ledger entries cl-3/cl-15 (R15 command matrix), cl-7/cl-16 (R16 audit of every writer of instruction-source paths) and cl-25 (set → apply → hook render with a working-tree edit excluded) assign R15/R16 verification to these beads. The docs in this cluster therefore record two requirements as mapped to work that has not landed. Also, `run.md` records the final sweep as "SWEEP DEFERRED (caller-owned)". The spec's goal claim ("every instruction source is editable only through a plan committed by apply") has no audit evidence at this head.

### F3. Must fix: two parked roast-1 escalations still need a human decision
`run.md` `parked:` keeps two "escalation · need human" items open:
- (a) Can a malformed or unsupported Codex `.rules` file abort Codex startup? Partly answered: `src/bootstrap/setup_claude.rs:73` records that the syntax was verified with `codex execpolicy check` on codex-cli 0.163.0. But the escalation is not closed, and the abort-on-bad-syntax behavior is still unrecorded.
- (b) Does Codex cap `additionalContext` at about 2,500 tokens, so that a 9000-byte render could exceed it? Unresolved. The spec's §4.4 cap reasons only about Claude. If Codex truncates or rejects the hook payload silently, Codex seats lose their most specific instructions, which defeats the §4.4 "preserve the most specific layers" rule.
Neither has a recorded disposition in the run docs or the spec.

### F4. Minor: run-state docs are stale relative to head
- `run.md` still shows `codeBuckets.escalated: hg-45e.1, .10, .17, .2, .8` and `review: no work landed`, but all five are closed and merged at head.
- `findings-ledger-design.md` is headed "through r1" and lists all five r1 Should-fix findings as `open`, but `design-loop-notes.md` records them as applied and round 2 as converged.
- `INDEX.md` lists the spec as `draft` although implementation is landing.
These are run records, so the impact is low, but a reader auditing the run gets the wrong state.

### F5. Minor: Post-Implementation Notes in the spec are empty despite recorded divergences
The spec asks for a dated note for anything that diverged. The digest records several divergences that touch spec text:
- `cap_warning` joins the omitted-parts and cut-part clauses with "; " (wording not in §4.4);
- `classify` checks the record-file list first, so `rules/seat.toml` would classify as a record, not an instruction (§2.2a says anything under `rules/` is an instruction location);
- fixtures use a test-only `bookkeeping.test_put` mutation;
- `bound_paths` skips a `template_ref` whose record is missing;
- the `HOOK_RENDER_CAP` comment says no documented Claude limit was found, while roast-1's FYI cites the hooks doc's 10,000-character per-string cap.
None needs to be recorded in the spec, but the §2.2a/classify precedence one contradicts normative spec text.

### F6. Minor: coverage cl-6 rationale conflicts with the final spec
cl-6 rejects R19 because "graph folders are not the agent cwd". The final spec's Out of scope names the fallback cwd under the instance's `.graph-local/` as a place where harness-native files are loaded. The spec's later wording governs. The ledger rationale is stale, but no bead depends on it.

## Verdict
not ready (spec §5/AC7 user docs and graph skill not updated: skill still instructs removed `content write`, no CHANGELOG migration note; acceptance beads hg-45e.11/.12/.16 open; two parked escalations undecided)

## Must fix before landing
- Land hg-45e.11: update skills/graph/SKILL.md (remove the `content write` instructions at line 42, add the §3.3 relay copy, the §3.5 cooperative-strength note with residual vectors, and the hash-match check), README.md, CREATING_NEW_GRAPH.md, GRAPH_STATE.md and CURRENT-DESIGN.md per spec §5
- Add the CHANGELOG.md entry: removal of `content write` (stub exits 2), the new commands, the paths.clone_files to paths.data/clone_record contract change, the apply ask rule, and the §1.5 migration note (update the summarizer template via plan template edit, then plan instructions clear seat <name> AGENTS.md for seeded seats)
- Land hg-45e.12 (§6 configuration smoke over three scopes and two harness gates) and hg-45e.16 (integration sweep with the R15 command matrix and the R16 writer audit) that the coverage ledger relies on
- Record a human disposition for the two parked roast-1 escalations in run.md: Codex behavior on a bad .rules file, and the Codex additionalContext size limit versus HOOK_RENDER_CAP=9000

## Untested scope
- No committed evidence that a Codex SessionStart hook payload near 9000 bytes reaches the model intact (Codex additionalContext limit unverified)
- R16 (no write path to instruction files other than an apply-committed plan) has no audit or test evidence at this head; it is assigned to the open sweep bead hg-45e.16
- The graph skill needle test (src/bootstrap/tests.rs ~1008-1018 at head) still asserts that the graph skill contains "content write", so it enforces the stale text; no needle covers summary-from, plan instructions or the ask rule

## Deferred OK
- Refresh stale run-state records: run.md codeBuckets/review lines, findings-ledger-design.md "open" statuses, and the INDEX.md status `draft`
- Append Post-Implementation Notes to the spec for the recorded divergences (cap_warning "; " join, classify record-file precedence over rules/, test_put fixture, bound_paths skip, HOOK_RENDER_CAP doc-limit comment)
- Update the coverage-ledger cl-6 rationale to match the final spec's .graph-local fallback-cwd residual vector
