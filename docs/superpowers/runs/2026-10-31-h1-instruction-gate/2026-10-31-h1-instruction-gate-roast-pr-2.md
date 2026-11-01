super-roast verdict: Nit (1 confirmed) [low coverage] [review readiness incomplete]
mode: PR        iteration: 2 of 3
profile (assumed): herdr-graph v0.2.0 is a local, single-operator Herdr plugin: a Rust CLI plus a long-running daemon over a unix socket, with all state in a git repository on the operator's machine. There is no network exposure, no multi-tenant data and no money; rollback is the previous binary plus git history. Blast radius is low. The change under review is itself a security control (the instruction gate), so findings that break its stated purpose are weighed on that purpose, not on the project's small footprint.
inputs: super-auto/h1-instruction-gate@3809881 vs main@c77d9ad
delta vs prior: 1 new confirmed (0 Blocking) · 0 carried (0 Blocking) · 5 resolved · 0 regressed (0 Blocking) · 2 punch-listed (open)
coverage: scouts 16/16 (correctness, security, premortem, simplicity-design, hot-path-perf, concurrency-async, regression, data-migrations, deploy-safety, api-contract, observability, testing, test-causality-lifecycle, test-isolation-environment, test-suite-cost, hygiene-docs) · raw 3 → deduped 3 → panel 2 · spot 1 · promoted 0 · judge completion 100% · remainder-capped: 0 · review readiness: coverage complete, provenance unavailable — basis: "direct review of the full captured range c77d9ad166dd2ada5b5a5768b499d12da4c36987..3809881e8b7042ba8e178bcda226b4db4d07970d; generated-input provenance unavailable because of the inv-p3b/inv-p3c/inv-p5r1 collection gaps"
independence: same-family (Claude) — seat-differentiated panel · rung: Workflow
seat-agreement: panels 2 · rr 1.00 · rg 0.50 · fg 0.50 · unanimous 0.50 · ground-loo 0.50 (n=2) · reproduce 1/1/0 · refute 1/1/0 · ground 2/0/0
lane-yield (found/confirmed/unique/refuted): correctness 1/1/1/0 · security 0/0/0/0 · premortem 0/0/0/0 · simplicity-design 0/0/0/0 · hot-path-perf 0/0/0/0 · concurrency-async 0/0/0/0 · regression 1/0/0/1 · data-migrations 0/0/0/0 · deploy-safety 0/0/0/0 · api-contract 0/0/0/0 · observability 0/0/0/0 · testing 1/0/0/0 · test-causality-lifecycle 0/0/0/0 · test-isolation-environment 0/0/0/0 · test-suite-cost 0/0/0/0 · hygiene-docs 0/0/0/0

Readiness note: the engine reports coverage complete but provenance unavailable (inv-p3b/inv-p3c/inv-p5r1 collection gaps). The `[review readiness incomplete]` and `[low coverage]` qualifiers reflect that limit; stage coverage itself (scouts, dedupe, judges) was complete and `coverage.lowCoverage` is false. `[converged]` is not set: the engine reports convergenceEligible false and the verdict carries a readiness qualifier.

Prior-report tracking (iteration 1 confirmed findings):
- [Should-fix] src/store/instruction_source.rs:533 (variation selectors) — resolved: not re-surfaced by this round's scouts.
- [Should-fix] src/daemon/doctor.rs:256 (doctor instruction_files scope) — resolved: not re-surfaced.
- [Nit] src/plan/instructions.rs:276 (non-UTF-8 before fails at apply) — resolved: not re-surfaced.
- [Nit] src/plan/line_diff.rs:65 (line-ending-only set gives empty diff) — resolved: not re-surfaced.
- [Nit] src/bootstrap/setup_claude.rs:405 (no-manifest install marks restorable:true) — punch-listed (open) per the caller's list. Note for the caller: commit e56e2eb ("setup never overwrites or deletes pre-existing harness artifacts", via the step-back cluster setup-preexisting-ownership, bead hg-45e.38) changed this exact code, and both setup_claude.rs packets this round treat the :405 Nit as fixed. The punch-list entry looks stale; the count above follows the caller's list, not my reading.
- [Nit] src/plan/instructions.rs:205 (raw before text in approval diff) — resolved: not re-surfaced.
- [Nit] src/templates/kinds.rs:1220 (edit_cap_warnings diverges from write_agents) — punch-listed (open); not re-surfaced.

## Confirmed findings
- [Nit] src/bootstrap/setup_claude.rs:426 — The new `hook_preexisting` flag is set once, on the first install with no manifest, and later runs copy it forward unchanged. If a later `setup` run installs or rewrites the owned SessionStart hook group itself, `uninstall` still skips that group and reports it as "left in place (present before setup)". That breaks the module's new guarantee that uninstall removes exactly what setup added. (new this round; introduced by fix commit e56e2eb for the prior [Nit] setup_claude.rs:405) [lanes: correctness]
  verdict: confirmed (reproduce ✓ / refute ✗-survived / ground ✓)
  evidence: `hook_preexisting` is set only in the two no-manifest branches (setup_claude.rs:429 and :453, `hook == HookChange::Unchanged`). Later runs start from `m = old_manifest.clone()` (~:424) and nothing re-evaluates the flag when `hook` comes back Installed or Updated. uninstall_for gates removal on it: `hook_removed = !hook_preexisting` (:527) and `had_owned = !hook_preexisting && ...` (:533); `hook_kept = hook_preexisting` (:618) drives the src/cli/setup.rs:81 message. Sequence: pre-existing owned group + no manifest → setup (Unchanged, flag=true) → group removed by hand or binary path changes → setup rewrites it (Installed/Updated, sha differs so restorable=false, flag still true) → uninstall takes the selective path and keeps the group setup wrote, printing "left in place (present before setup)". Module doc lines 9-11 say "Uninstall removes exactly what setup added". No test in src/bootstrap/tests.rs covers the second-run case. All three seats Nit: trigger needs a pre-existing hook plus a hand edit or path change plus a re-run; effect is a leftover SessionStart hook and a misleading message, nothing lost.
  fix-shape hint: on every run set `m.hook_preexisting = false` whenever `hook` is Installed or Updated, and add a second-run test (pre-existing group, remove it, re-run setup, uninstall strips it).

## Not verified (beyond panel cap)
- none

## Not verified (dedupe failed or judge lost)
- none

## Beyond remainder cap (count only)
- none

## Rejected (with reason)
- src/bootstrap/setup_claude.rs:453 [fix-regression] — The round-1 fix (e56e2eb) marks any owned hook group found without a manifest as `hook_preexisting`, so `uninstall` after a lost manifest no longer strips the owned SessionStart hook that base c77d9ad removed. Rejected 2/3 (ground CONFIRM Nit). The code facts are agreed by all seats. Majority: the behaviour is the deliberate rule from the run's step-back roast (2026-10-31-h1-instruction-gate-roast-pr-1-step-back.md, cluster setup-preexisting-ownership: "uninstall removes only what setup added and never byte-restores or deletes pre-existing artifacts"); with no manifest a byte-identical owned group cannot be told apart from the user's own copy, and keeping it is the conservative side of that ambiguity; setup prints "left in place (present before setup)" so the outcome is not silent; the trigger needs a lost manifest while the hook is kept, and the base behaviour it replaced deleted a user's pre-existing hook, which was the original Nit. Minority concedes "the behavior follows from the fix's deliberate 'never remove pre-existing artifacts' rule" and cites nothing the majority does not address. Residual docs nit noted by all seats, optional: CHANGELOG line 31 ("removes exactly that") and the module doc ("Uninstall removes exactly what setup added") now overstate; soften to "removes what setup added and leaves pre-existing artifacts in place".

## Unverified nits (spot-checked)
One refute-seat pass each; not panel-strength verification.
- src/undo/tests.rs:1127 — `undo_restores_raw_bytes` asserts `diff.contains("+")`, which is vacuous because every `line_diff::unified` hunk header is `@@ -a +b @@`, so the `+` side the approver sees for restored inadmissible bytes is unchecked; and `vetted()` (src/plan/line_diff.rs) renders that `+` side as "current content is not admissible instruction text" although it describes the content the undo will write. Spot: CONFIRM Nit — a mutation rendering the after side as "" would still pass; `undo_instruction_clear_over_esc_restores_bytes` checks only `!shown.contains('\x1b')`; the safety property (no raw bytes in the render, final bytes correct) still holds, so no bypass or data loss. Fix: assert the `+` line carries the placeholder text, and give vetted() a side-aware wording ("restored content" on the after side).

## Escalations (need human)
- none
